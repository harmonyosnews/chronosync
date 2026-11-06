use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Settings changed from the WebUI. `None` means "follow the CLI/installer".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub manual_server: Option<bool>,
    #[serde(default)]
    pub ntp123: Option<bool>,
}

impl Settings {
    /// Read settings without making a missing/corrupt file fatal at startup.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match fs::read(path) {
            Ok(data) => match serde_json::from_slice(&data) {
                Ok(settings) => (settings, None),
                Err(err) => (
                    Self::default(),
                    Some(format!(
                        "运行设置文件 {} 无法解析，已使用启动参数：{err}",
                        path.display()
                    )),
                ),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Self::default(), None),
            Err(err) => (
                Self::default(),
                Some(format!(
                    "运行设置文件 {} 无法读取，已使用启动参数：{err}",
                    path.display()
                )),
            ),
        }
    }

    /// Persist atomically enough for a small local settings file. The temporary
    /// file is removed on any failure so a later start never reads half a file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|err| format!("创建运行设置目录失败 {}：{err}", parent.display()))?;
        }
        let data =
            serde_json::to_vec_pretty(self).map_err(|err| format!("编码运行设置失败：{err}"))?;
        let tmp = temporary_path(path);
        let result = write_private(&tmp, &data).and_then(|()| {
            #[cfg(windows)]
            if path.exists() {
                fs::remove_file(path)
                    .map_err(|err| format!("替换运行设置失败 {}：{err}", path.display()))?;
            }
            fs::rename(&tmp, path)
                .map_err(|err| format!("写入运行设置失败 {}：{err}", path.display()))
        });
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "runtime.json".into());
    name.push(format!(".tmp.{}", std::process::id()));
    path.with_file_name(name)
}

fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| format!("创建运行设置临时文件失败 {}：{err}", path.display()))?;
    file.write_all(data)
        .and_then(|()| file.sync_all())
        .map_err(|err| format!("写入运行设置临时文件失败 {}：{err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chronosync-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_preserves_overrides() {
        let dir = temp_dir();
        let path = dir.join("runtime.json");
        let settings = Settings {
            manual_server: Some(true),
            ntp123: Some(false),
        };
        settings.save(&path).unwrap();
        let (loaded, warning) = Settings::load(&path);
        assert_eq!(loaded, settings);
        assert!(warning.is_none());
        assert!(!dir.join("runtime.json.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_file_uses_defaults() {
        let dir = temp_dir();
        let (loaded, warning) = Settings::load(&dir.join("missing.json"));
        assert_eq!(loaded, Settings::default());
        assert!(warning.is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_and_reports() {
        let dir = temp_dir();
        let path = dir.join("runtime.json");
        fs::write(&path, b"{not-json").unwrap();
        let (loaded, warning) = Settings::load(&path);
        assert_eq!(loaded, Settings::default());
        assert!(warning.unwrap().contains("无法解析"));
        fs::remove_dir_all(dir).unwrap();
    }
}
