use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, AuthResult, KeyboardInteractiveAuthResponse};
use russh::keys::PublicKeyOrCertificate;
use russh::{ChannelMsg, Disconnect, MethodKind, MethodSet};
use tokio::time::timeout;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_TIMEOUT: Duration = Duration::from_secs(12);
const EXEC_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_STDOUT: usize = 128 * 1024;
const MAX_STDERR: usize = 32 * 1024;

#[derive(Clone, Debug)]
pub struct RemoteTarget {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SyncOptions {
    pub sync_hwclock: bool,
    pub sync_timezone: bool,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct Preflight {
    pub hostname: String,
    pub epoch: f64,
    pub local_time: String,
    pub timezone: String,
}

#[derive(Clone, Debug)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<u32>,
}

impl ExecOutput {
    pub fn marker(&self, name: &str) -> Option<&str> {
        let prefix = format!("__CS_{name}__");
        self.stdout
            .lines()
            .find_map(|line| line.trim_end_matches('\r').strip_prefix(&prefix))
            .map(str::trim)
    }

    pub fn has_marker(&self, name: &str) -> bool {
        self.marker(name).is_some()
    }

    pub fn concise_error(&self) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return truncate_chars(stderr, 360);
        }
        let stdout = self.stdout.trim();
        if !stdout.is_empty() {
            return truncate_chars(stdout, 360);
        }
        "远程命令返回非零状态，但没有错误输出".to_string()
    }
}

#[derive(Clone, Debug)]
pub enum OptionalStep {
    Ok,
    OkDirectIsa,
    Skipped,
    Already,
    NoRtc,
    Missing,
    Failed(String),
}

impl OptionalStep {
    pub fn from_marker(output: &ExecOutput, name: &str) -> Self {
        match output.marker(name).unwrap_or("skipped") {
            "ok" => Self::Ok,
            "ok-directisa" => Self::OkDirectIsa,
            "already" => Self::Already,
            "missing" => Self::Missing,
            "missing-data" => Self::Missing,
            "no-rtc" => Self::NoRtc,
            "failed" => Self::Failed(output.concise_error()),
            "skipped" => Self::Skipped,
            "missing-binary" => Self::Missing,
            other => Self::Failed(format!("未知结果: {other}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApplyResult {
    pub now_epoch: Option<f64>,
    pub local_time: String,
    pub hardware_clock: OptionalStep,
    pub timezone: OptionalStep,
}

struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // This is a trusted-LAN fleet tool. Host key pinning is not practical
        // across the many appliance images; the UI/README call this out.
        Ok(true)
    }
}

pub struct SshClient {
    session: client::Handle<ClientHandler>,
}

async fn keyboard_interactive_auth(
    session: &mut client::Handle<ClientHandler>,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let mut response = session
        .authenticate_keyboard_interactive_start(user, None::<String>)
        .await
        .map_err(|err| format!("SSH 键盘交互认证失败：{err}"))?;
    for _ in 0..6 {
        match response {
            KeyboardInteractiveAuthResponse::Success => return Ok(()),
            KeyboardInteractiveAuthResponse::Failure {
                remaining_methods,
                partial_success,
            } => {
                let mut message = "SSH 密码认证失败：用户名或密码不正确".to_string();
                if partial_success {
                    message.push_str("（服务器要求额外的认证步骤）");
                }
                let methods = format_methods(&remaining_methods);
                if !methods.is_empty() {
                    message.push_str(&format!("；服务器允许的其他认证方式: {methods}"));
                }
                return Err(message);
            }
            KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let answers = prompts
                    .iter()
                    .map(|prompt| {
                        let text = prompt.prompt.to_ascii_lowercase();
                        if !prompt.echo
                            || text.contains("password")
                            || text.contains("密码")
                            || prompts.len() == 1
                        {
                            password.to_string()
                        } else {
                            String::new()
                        }
                    })
                    .collect();
                response = session
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|err| format!("SSH 键盘交互认证失败：{err}"))?;
            }
        }
    }
    Err("SSH 键盘交互认证请求次数过多".to_string())
}

fn format_methods(methods: &MethodSet) -> String {
    methods
        .iter()
        .map(|method| match method {
            MethodKind::None => "none",
            MethodKind::Password => "password",
            MethodKind::PublicKey => "publickey",
            MethodKind::HostBased => "hostbased",
            MethodKind::KeyboardInteractive => "keyboard-interactive",
            MethodKind::GssapiWithMic => "gssapi-with-mic",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

async fn disconnect(session: &mut client::Handle<ClientHandler>) {
    let _ = timeout(
        Duration::from_secs(2),
        session.disconnect(Disconnect::ByApplication, "", "zh-CN"),
    )
    .await;
}

impl SshClient {
    pub async fn connect(target: &RemoteTarget) -> Result<Self, String> {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            nodelay: true,
            ..Default::default()
        });
        let address = (target.host.as_str(), target.port);
        let connect_fut = client::connect(config, address, ClientHandler);
        let mut session = match timeout(CONNECT_TIMEOUT, connect_fut).await {
            Ok(Ok(session)) => session,
            Ok(Err(err)) => {
                return Err(format!(
                    "SSH 连接失败：{}",
                    humanize_connect_error(&err.to_string())
                ));
            }
            Err(_) => {
                return Err(format!(
                    "SSH 连接超时（{} 秒），请检查 IP、端口与网络",
                    CONNECT_TIMEOUT.as_secs()
                ));
            }
        };

        let auth_fut = session.authenticate_password(target.user.clone(), target.password.clone());
        let auth_result = match timeout(AUTH_TIMEOUT, auth_fut).await {
            Ok(Ok(result)) => result,
            Ok(Err(err)) => {
                disconnect(&mut session).await;
                return Err(format!("SSH 认证过程失败：{err}"));
            }
            Err(_) => {
                disconnect(&mut session).await;
                return Err(format!("SSH 认证超时（{} 秒）", AUTH_TIMEOUT.as_secs()));
            }
        };

        match auth_result {
            AuthResult::Success => Ok(Self { session }),
            AuthResult::Failure {
                remaining_methods,
                partial_success,
            } => {
                // OpenSSH/PAM appliances (e.g. GentooPlayer) frequently expose
                // the password prompt only through keyboard-interactive.
                if remaining_methods.contains(&MethodKind::KeyboardInteractive) {
                    let kbi_fut =
                        keyboard_interactive_auth(&mut session, &target.user, &target.password);
                    match timeout(AUTH_TIMEOUT, kbi_fut).await {
                        Ok(Ok(())) => return Ok(Self { session }),
                        Ok(Err(err)) => {
                            disconnect(&mut session).await;
                            return Err(err);
                        }
                        Err(_) => {
                            disconnect(&mut session).await;
                            return Err(format!(
                                "SSH 键盘交互认证超时（{} 秒）",
                                AUTH_TIMEOUT.as_secs()
                            ));
                        }
                    }
                }
                disconnect(&mut session).await;
                let mut message = "SSH 认证失败：用户名或密码不正确".to_string();
                if partial_success {
                    message.push_str("（服务器要求额外的认证步骤）");
                }
                let methods = format_methods(&remaining_methods);
                if !methods.is_empty() {
                    message.push_str(&format!("；服务器允许的其他认证方式: {methods}"));
                }
                Err(message)
            }
        }
    }

    pub async fn exec(&mut self, command: &str) -> Result<ExecOutput, String> {
        let mut channel = match timeout(EXEC_TIMEOUT, self.session.channel_open_session()).await {
            Ok(Ok(channel)) => channel,
            Ok(Err(err)) => return Err(format!("无法打开 SSH 会话：{err}")),
            Err(_) => return Err("打开 SSH 会话超时".to_string()),
        };

        if let Err(err) = channel.exec(true, command).await {
            return Err(format!("无法执行远程命令：{err}"));
        }

        let read_loop = async {
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit_status = None;
            let mut exit_signal = None;
            let mut failure = false;

            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        if stdout.len() < MAX_STDOUT {
                            let room = MAX_STDOUT - stdout.len();
                            stdout.extend_from_slice(&data[..data.len().min(room)]);
                        }
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        if stderr.len() < MAX_STDERR {
                            let room = MAX_STDERR - stderr.len();
                            stderr.extend_from_slice(&data[..data.len().min(room)]);
                        }
                    }
                    ChannelMsg::ExitStatus { exit_status: code } => {
                        exit_status = Some(code);
                    }
                    ChannelMsg::ExitSignal {
                        signal_name,
                        error_message,
                        ..
                    } => {
                        exit_signal = Some(if error_message.is_empty() {
                            format!("{signal_name:?}")
                        } else {
                            format!("{signal_name:?} ({error_message})")
                        });
                    }
                    ChannelMsg::Failure => failure = true,
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }

            if failure {
                return Err("SSH 服务器拒绝执行命令".to_string());
            }
            if let Some(signal) = exit_signal {
                return Err(format!("远程命令被信号终止：{signal}"));
            }
            Ok(ExecOutput {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_status,
            })
        };

        match timeout(EXEC_TIMEOUT, read_loop).await {
            Ok(result) => result,
            Err(_) => Err(format!("远程命令执行超时（{} 秒）", EXEC_TIMEOUT.as_secs())),
        }
    }

    pub async fn close(self) {
        let _ = timeout(
            Duration::from_secs(2),
            self.session
                .disconnect(Disconnect::ByApplication, "", "zh-CN"),
        )
        .await;
    }
}

pub fn preflight_command() -> &'static str {
    r#"printf '__CS_HOST__%s\n' "$(hostname 2>/dev/null || uname -n 2>/dev/null || echo unknown)"
E=$(date -u +%s.%N 2>/dev/null)
case "$E" in ""|*%N*|*[!0-9.]*) E=$(date -u +%s 2>/dev/null) ;; esac
printf '__CS_EPOCH__%s\n' "$E"
printf '__CS_LOCAL__%s\n' "$(date '+%Y-%m-%d %H:%M:%S %Z' 2>/dev/null)"
printf '__CS_TZ__%s\n' "$(readlink -f /etc/localtime 2>/dev/null)"
printf '__CS_TZNAME__%s\n' "$(head -n 1 /etc/timezone 2>/dev/null)"
printf '__CS_KERNEL__%s\n' "$(uname -sr 2>/dev/null)"
if command -v timedatectl >/dev/null 2>&1; then echo '__CS_TIMEDATECTL__1'; else echo '__CS_TIMEDATECTL__0'; fi
if command -v hwclock >/dev/null 2>&1; then echo '__CS_HWCLOCK__1'; else echo '__CS_HWCLOCK__0'; fi
exit 0"#
}

pub fn parse_preflight(output: &ExecOutput) -> Result<Preflight, String> {
    let hostname = output
        .marker("HOST")
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string();
    let epoch = output
        .marker("EPOCH")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "无法读取远程系统时间（date 命令可能不可用）".to_string())?
        .parse::<f64>()
        .map_err(|_| "远程系统时间格式无效".to_string())?;
    let local_time = output.marker("LOCAL").unwrap_or("未知").to_string();
    let tz_path = output.marker("TZ").unwrap_or("");
    let tz_name = output.marker("TZNAME").unwrap_or("");
    let timezone = normalize_timezone(tz_path, tz_name);
    Ok(Preflight {
        hostname,
        epoch,
        local_time,
        timezone,
    })
}

pub fn set_time_command(
    target_epoch: i64,
    target_utc: &str,
    sync_hwclock: bool,
    sync_timezone: bool,
) -> String {
    let hwclock_flag = if sync_hwclock { 1 } else { 0 };
    let timezone_flag = if sync_timezone { 1 } else { 0 };
    format!(
        r#"TARGET_EPOCH={target_epoch}
TARGET_UTC="{target_utc}"
FIRST_ERR=""
SECOND_ERR=""
if ! FIRST_ERR=$(date -u -s "@$TARGET_EPOCH" 2>&1); then
  if ! SECOND_ERR=$(date -u -s "$TARGET_UTC" 2>&1); then
    printf '__CS_SET_RC__1\n'
    printf '%s\n' "$FIRST_ERR" >&2
    printf '%s\n' "$SECOND_ERR" >&2
    exit 1
  fi
fi
printf '__CS_SET_RC__0\n'

TZ_ACTION=skipped
if [ {timezone_flag} -eq 1 ]; then
  CUR_TZ=$(readlink -f /etc/localtime 2>/dev/null)
  case "$CUR_TZ" in
    *Asia/Shanghai*) TZ_ACTION=already ;;
    *)
      if command -v timedatectl >/dev/null 2>&1; then
        if TZ_ERR=$(timedatectl set-timezone Asia/Shanghai 2>&1); then
          TZ_ACTION=ok
        else
          TZ_ACTION=failed
          printf '%s\n' "$TZ_ERR" >&2
        fi
      elif [ -f /usr/share/zoneinfo/Asia/Shanghai ]; then
        if TZ_ERR=$(ln -sf /usr/share/zoneinfo/Asia/Shanghai /etc/localtime 2>&1); then
          TZ_ACTION=ok
        else
          TZ_ACTION=failed
          printf '%s\n' "$TZ_ERR" >&2
        fi
      else
        TZ_ACTION=missing-data
      fi
      ;;
  esac
fi
printf '__CS_TZ_ACTION__%s\n' "$TZ_ACTION"

HWC_ACTION=skipped
if [ {hwclock_flag} -eq 1 ]; then
  if command -v hwclock >/dev/null 2>&1; then
    RTC_NODE=""
    for RTC_CANDIDATE in /dev/rtc0 /dev/rtc /dev/misc/rtc; do
      if [ -e "$RTC_CANDIDATE" ]; then RTC_NODE="$RTC_CANDIDATE"; break; fi
    done
    if [ -n "$RTC_NODE" ]; then
      if HWC_ERR=$(hwclock -w 2>&1); then
        HWC_ACTION=ok
      else
        HWC_ACTION=failed
        printf '%s\n' "$HWC_ERR" >&2
      fi
    else
      # Buildroot images sometimes ship without /dev/rtc* even though the
      # x86 CMOS clock is readable through util-linux direct ISA access.
      case "$(uname -m 2>/dev/null)" in
        x86_64|i?86)
          if hwclock --directisa --show >/dev/null 2>&1; then
            if HWC_ERR=$(hwclock --directisa -w 2>&1); then
              HWC_ACTION=ok-directisa
            else
              HWC_ACTION=failed
              printf '%s\n' "$HWC_ERR" >&2
            fi
          else
            HWC_ACTION=no-rtc
          fi
          ;;
        *)
          HWC_ACTION=no-rtc
          ;;
      esac
    fi
  else
    HWC_ACTION=missing-binary
  fi
fi
printf '__CS_HWC__%s\n' "$HWC_ACTION"

sleep 0.2 2>/dev/null || sleep 1
printf '__CS_NOW_EPOCH__%s\n' "$(date -u +%s 2>/dev/null)"
printf '__CS_NOW_LOCAL__%s\n' "$(date '+%Y-%m-%d %H:%M:%S %Z' 2>/dev/null)"
exit 0"#
    )
}

pub fn parse_apply(output: &ExecOutput) -> Result<ApplyResult, String> {
    if output.exit_status.unwrap_or(0) != 0 {
        return Err(format!("写入系统时间失败：{}", output.concise_error()));
    }
    if output.marker("SET_RC") != Some("0") {
        return Err(format!("写入系统时间失败：{}", output.concise_error()));
    }
    let now_epoch = output
        .marker("NOW_EPOCH")
        .and_then(|value| value.parse::<f64>().ok());
    let local_time = output.marker("NOW_LOCAL").unwrap_or("未知").to_string();
    Ok(ApplyResult {
        now_epoch,
        local_time,
        hardware_clock: OptionalStep::from_marker(output, "HWC"),
        timezone: OptionalStep::from_marker(output, "TZ_ACTION"),
    })
}

pub fn normalize_timezone(path: &str, name: &str) -> String {
    let name = name.trim();
    if !name.is_empty() {
        if name.contains("Shanghai") {
            return "Asia/Shanghai".to_string();
        }
        return name.to_string();
    }
    if path.contains("Asia/Shanghai") {
        return "Asia/Shanghai".to_string();
    }
    let trimmed = path
        .trim()
        .trim_start_matches("/usr/share/zoneinfo/")
        .trim_start_matches("posix/")
        .to_string();
    if trimmed.is_empty() || trimmed == "/etc/localtime" {
        "未知".to_string()
    } else {
        trimmed
    }
}

fn humanize_connect_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("connection refused") {
        "目标拒绝连接，请确认 SSH 服务已启动且端口正确".to_string()
    } else if lower.contains("no route to host") || lower.contains("network is unreachable") {
        "目标不可达，请检查 IP 地址和路由".to_string()
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "连接超时，请检查网络和防火墙".to_string()
    } else if lower.contains("connection reset") {
        "连接被目标重置".to_string()
    } else {
        truncate_chars(error, 280)
    }
}

fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.replace('\u{0}', "");
    }
    let truncated: String = input.chars().take(max).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out_with(stdout: &str) -> ExecOutput {
        ExecOutput {
            stdout: stdout.to_string(),
            stderr: String::new(),
            exit_status: Some(0),
        }
    }

    #[test]
    fn parses_preflight_markers() {
        let output = out_with(
            "__CS_HOST__rpi\n__CS_EPOCH__1793789235.125\n__CS_LOCAL__Wed Nov  4 18:47:15 CST 2026\n\
             __CS_TZ__/usr/share/zoneinfo/Asia/Shanghai\n__CS_TZNAME__Asia/Shanghai\n\
             __CS_KERNEL__Linux 6.18\n__CS_TIMEDATECTL__1\n__CS_HWCLOCK__1\n",
        );
        let parsed = parse_preflight(&output).unwrap();
        assert_eq!(parsed.hostname, "rpi");
        assert_eq!(parsed.epoch, 1_793_789_235.125);
        assert_eq!(parsed.timezone, "Asia/Shanghai");
    }

    #[test]
    fn parses_set_result() {
        let output = ExecOutput {
            stdout: "__CS_SET_RC__0\n__CS_TZ_ACTION__already\n__CS_HWC__ok\n\
                     __CS_NOW_EPOCH__1793789248\n__CS_NOW_LOCAL__2026-11-04 18:47:28 CST\n"
                .to_string(),
            stderr: String::new(),
            exit_status: Some(0),
        };
        let parsed = parse_apply(&output).unwrap();
        assert_eq!(parsed.now_epoch, Some(1_793_789_248.0));
        assert!(matches!(parsed.hardware_clock, OptionalStep::Ok));
        assert!(matches!(parsed.timezone, OptionalStep::Already));
    }

    #[test]
    fn timezone_normalization() {
        assert_eq!(
            normalize_timezone("/usr/share/zoneinfo/posix/Asia/Shanghai", ""),
            "Asia/Shanghai"
        );
        assert_eq!(normalize_timezone("/tmp/localtime", ""), "/tmp/localtime");
        assert_eq!(normalize_timezone("", "Europe/Berlin"), "Europe/Berlin");
        assert_eq!(normalize_timezone("", ""), "未知");
    }

    #[test]
    fn set_command_has_portable_quotes() {
        let command = set_time_command(1_793_789_248, "2026-11-04 10:47:28", true, true);
        assert!(command.contains("date -u -s \"@$TARGET_EPOCH\""));
        assert!(command.contains("Asia/Shanghai"));
        assert!(command.contains("hwclock -w"));
        assert!(command.contains("hwclock --directisa -w"));
    }

    #[test]
    fn parses_direct_isa_rtc_result() {
        let output = out_with(
            "__CS_SET_RC__0\n__CS_TZ_ACTION__already\n__CS_HWC__ok-directisa\n             __CS_NOW_EPOCH__1793789248\n__CS_NOW_LOCAL__2026-11-04 18:47:28 CST\n",
        );
        let parsed = parse_apply(&output).unwrap();
        assert!(matches!(parsed.hardware_clock, OptionalStep::OkDirectIsa));
    }

    #[test]
    fn parses_missing_rtc_result() {
        let output = out_with(
            "__CS_SET_RC__0\n__CS_TZ_ACTION__skipped\n__CS_HWC__no-rtc\n             __CS_NOW_EPOCH__1793789248\n__CS_NOW_LOCAL__2026-11-04 18:47:28 CST\n",
        );
        let parsed = parse_apply(&output).unwrap();
        assert!(matches!(parsed.hardware_clock, OptionalStep::NoRtc));
    }
}
