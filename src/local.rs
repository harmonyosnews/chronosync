use std::time::{SystemTime, UNIX_EPOCH};

use crate::remote::OptionalStep;

#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

#[derive(Clone, Debug)]
pub struct LocalEnvironment {
    pub hostname: String,
    pub privileged: bool,
    pub timezone: String,
}

pub fn environment() -> LocalEnvironment {
    LocalEnvironment {
        hostname: read_hostname(),
        privileged: is_privileged(),
        timezone: read_timezone(),
    }
}

#[cfg(unix)]
pub fn is_privileged() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

#[cfg(windows)]
pub fn is_privileged() -> bool {
    // SAFETY: IsUserAnAdmin has no preconditions.
    unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
}

pub fn current_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

pub fn set_system_time(epoch_ms: i64) -> Result<(), String> {
    if !is_privileged() {
        return Err(if cfg!(windows) {
            "本机校时需要管理员权限，请以管理员身份运行 ChronoSync".to_string()
        } else {
            "本机校时需要 root 权限，请使用 sudo 启动 ChronoSync".to_string()
        });
    }
    set_system_time_impl(epoch_ms)
}

#[cfg(unix)]
fn set_system_time_impl(epoch_ms: i64) -> Result<(), String> {
    let seconds = epoch_ms.div_euclid(1000);
    let nanoseconds = (epoch_ms.rem_euclid(1000) * 1_000_000) as libc::c_long;
    let timespec = libc::timespec {
        tv_sec: seconds as libc::time_t,
        tv_nsec: nanoseconds,
    };
    // SAFETY: timespec is fully initialized and CLOCK_REALTIME is valid.
    let result = unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &timespec) };
    if result == 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        Err(format!(
            "设置本机系统时间失败：{error}。请确认以 root 运行，且当前环境允许修改时钟（部分容器会禁止此操作）"
        ))
    }
}

#[cfg(windows)]
fn set_system_time_impl(epoch_ms: i64) -> Result<(), String> {
    use chrono::{Datelike, Timelike};
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::SetSystemTime;

    let seconds = epoch_ms.div_euclid(1000);
    let nanos = (epoch_ms.rem_euclid(1000) * 1_000_000) as u32;
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos)
        .ok_or_else(|| "目标时间无效".to_string())?;
    let system_time = SYSTEMTIME {
        wYear: dt.year() as u16,
        wMonth: dt.month() as u16,
        wDayOfWeek: dt.weekday().num_days_from_sunday() as u16,
        wDay: dt.day() as u16,
        wHour: dt.hour() as u16,
        wMinute: dt.minute() as u16,
        wSecond: dt.second() as u16,
        wMilliseconds: (dt.nanosecond() / 1_000_000) as u16,
    };
    // SAFETY: SYSTEMTIME is fully initialized and SetSystemTime only reads it.
    let result = unsafe { SetSystemTime(&system_time) };
    if result != 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        Err(format!(
            "设置 Windows 系统时间失败：{error}。请以管理员身份运行 ChronoSync"
        ))
    }
}

#[cfg(unix)]
pub async fn write_hwclock() -> OptionalStep {
    if !is_privileged() {
        return OptionalStep::Failed("写入硬件时钟需要 root 权限".to_string());
    }
    let output = tokio::task::spawn_blocking(|| Command::new("hwclock").arg("-w").output()).await;
    match output {
        Ok(Ok(result)) if result.status.success() => OptionalStep::Ok,
        Ok(Ok(result)) => {
            let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
            OptionalStep::Failed(if stderr.is_empty() {
                format!("hwclock 返回状态 {}", result.status)
            } else {
                stderr
            })
        }
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => OptionalStep::Missing,
        Ok(Err(err)) => OptionalStep::Failed(format!("无法执行 hwclock: {err}")),
        Err(err) => OptionalStep::Failed(format!("硬件时钟任务异常: {err}")),
    }
}

#[cfg(windows)]
pub async fn write_hwclock() -> OptionalStep {
    // Windows keeps the clock through its own time service; there is no
    // util-linux-style separate RTC write exposed here.
    OptionalStep::Skipped
}

#[cfg(unix)]
pub async fn ensure_timezone() -> OptionalStep {
    if read_timezone() == "Asia/Shanghai" {
        return OptionalStep::Already;
    }
    if !is_privileged() {
        return OptionalStep::Failed("设置本机时区需要 root 权限".to_string());
    }
    let output = tokio::task::spawn_blocking(|| {
        Command::new("timedatectl")
            .args(["set-timezone", "Asia/Shanghai"])
            .output()
    })
    .await;
    match output {
        Ok(Ok(result)) if result.status.success() => OptionalStep::Ok,
        Ok(Ok(result)) => {
            let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
            OptionalStep::Failed(if stderr.is_empty() {
                format!("timedatectl 返回状态 {}", result.status)
            } else {
                stderr
            })
        }
        Ok(Err(err)) if err.kind() == std::io::ErrorKind::NotFound => OptionalStep::Missing,
        Ok(Err(err)) => OptionalStep::Failed(format!("无法执行 timedatectl: {err}")),
        Err(err) => OptionalStep::Failed(format!("时区设置任务异常: {err}")),
    }
}

#[cfg(windows)]
pub async fn ensure_timezone() -> OptionalStep {
    // Local timezone changes are intentionally not applied on Windows.
    OptionalStep::Skipped
}

#[cfg(unix)]
pub fn read_hostname() -> String {
    if let Ok(value) = std::fs::read_to_string("/etc/hostname") {
        let value = value.trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "本机".to_string())
}

#[cfg(windows)]
pub fn read_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Windows".to_string())
}

#[cfg(unix)]
pub fn read_timezone() -> String {
    let path = Path::new("/etc/localtime");
    if let Ok(target) = std::fs::read_link(path) {
        let target = target.to_string_lossy();
        if target.contains("Asia/Shanghai") {
            return "Asia/Shanghai".to_string();
        }
    }
    if let Ok(target) = std::fs::canonicalize(path) {
        let target = target.to_string_lossy();
        if target.contains("Asia/Shanghai") {
            return "Asia/Shanghai".to_string();
        }
        let cleaned = target
            .trim_start_matches("/usr/share/zoneinfo/")
            .trim_start_matches("posix/")
            .to_string();
        if !cleaned.is_empty() && cleaned != "/etc/localtime" {
            return cleaned;
        }
    }
    if let Ok(name) = std::fs::read_to_string("/etc/timezone") {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    "未知".to_string()
}

#[cfg(windows)]
pub fn read_timezone() -> String {
    "Windows 系统时区".to_string()
}
