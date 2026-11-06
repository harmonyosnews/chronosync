use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

use ring::rand::{SecureRandom, SystemRandom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Server,
    Client,
    Auto,
}

impl Mode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "server" => Some(Self::Server),
            "client" => Some(Self::Client),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Server => "server",
            Self::Client => "client",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub mode: Mode,
    pub cluster_id: String,
    pub node_id: String,
    pub secret: Vec<u8>,
    pub listen_ip: IpAddr,
    pub discovery_port: u16,
    pub time_port: u16,
    pub ntp123: bool,
    pub ntp123_port: u16,
    pub server_eligible: bool,
    pub auto_apply: bool,
    pub max_offset_ms: i64,
    pub max_step_ms: i64,
    pub allow_large_step: bool,
    pub sync_interval: Duration,
    pub seed_peers: Vec<String>,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == Mode::Off {
            return Ok(());
        }
        if !valid_cluster_id(&self.cluster_id) {
            return Err("cluster_id 必须是 1–64 位字母、数字、点、下划线或短横线".to_string());
        }
        if !valid_node_id(&self.node_id) {
            return Err("node_id 必须是 8–64 位字母、数字、下划线或短横线".to_string());
        }
        if self.secret.len() < 16 {
            return Err("集群密钥至少需要 16 字节".to_string());
        }
        if self.listen_ip.is_unspecified() && self.discovery_port == self.time_port {
            return Err("发现端口和时间端口不能相同".to_string());
        }
        if self.discovery_port == 0 || self.time_port == 0 {
            return Err("集群端口不能为 0".to_string());
        }
        if self.ntp123 && self.ntp123_port == 0 {
            return Err("NTP 123 端口不能为 0".to_string());
        }
        if self.discovery_port == self.time_port {
            return Err("发现端口和时间端口不能相同".to_string());
        }
        if !(10..=3_600_000).contains(&self.max_offset_ms) {
            return Err("max_offset_ms 必须在 10–3600000 之间".to_string());
        }
        if self.max_step_ms < self.max_offset_ms {
            return Err("max_step_ms 不能小于 max_offset_ms".to_string());
        }
        if self.sync_interval < Duration::from_secs(10)
            || self.sync_interval > Duration::from_secs(3600)
        {
            return Err("同步间隔必须在 10–3600 秒之间".to_string());
        }
        for seed in &self.seed_peers {
            if seed.trim().is_empty() || seed.contains('@') || seed.contains('/') {
                return Err(format!("无效的 seed 地址：{seed}"));
            }
        }
        Ok(())
    }
}

pub fn load_secret(path: &Path) -> Result<Vec<u8>, String> {
    let mut data =
        fs::read(path).map_err(|err| format!("读取集群密钥失败 {}：{err}", path.display()))?;
    while data
        .last()
        .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
    {
        data.pop();
    }
    if data.len() < 16 {
        return Err("集群密钥至少需要 16 字节".to_string());
    }
    if data.len() > 4096 {
        return Err("集群密钥过长（最大 4096 字节）".to_string());
    }
    Ok(data)
}

pub fn resolve_node_id(arg: Option<&str>, state_dir: &Path) -> Result<String, String> {
    if let Some(value) = arg {
        let value = value.trim();
        if !valid_node_id(value) {
            return Err("node_id 必须是 8–64 位字母、数字、下划线或短横线".to_string());
        }
        return Ok(value.to_string());
    }

    fs::create_dir_all(state_dir)
        .map_err(|err| format!("创建集群状态目录失败 {}：{err}", state_dir.display()))?;
    let path = state_dir.join("node-id");
    if let Ok(value) = fs::read_to_string(&path) {
        let value = value.trim();
        if valid_node_id(value) {
            return Ok(value.to_string());
        }
    }

    let rng = SystemRandom::new();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes)
        .map_err(|_| "生成 node_id 失败：系统随机源不可用".to_string())?;
    let value = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    write_private(&path, &value)?;
    Ok(value)
}

fn write_private(path: &Path, value: &str) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| format!("写入 node_id 失败 {}：{err}", path.display()))?;
    file.write_all(value.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|err| format!("写入 node_id 失败 {}：{err}", path.display()))
}

fn valid_cluster_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn valid_node_id(value: &str) -> bool {
    (8..=64).contains(&value.len())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing_is_normalized() {
        assert_eq!(Mode::parse("AUTO"), Some(Mode::Auto));
        assert_eq!(Mode::parse("server"), Some(Mode::Server));
        assert_eq!(Mode::parse("unknown"), None);
        assert_eq!(Mode::Auto.as_str(), "auto");
    }

    #[test]
    fn cluster_validation_rejects_bad_ports_and_short_secret() {
        let mut config = Config {
            mode: Mode::Auto,
            cluster_id: "lab-1".to_string(),
            node_id: "node-00000001".to_string(),
            secret: b"0123456789abcdef".to_vec(),
            listen_ip: std::net::Ipv4Addr::UNSPECIFIED.into(),
            discovery_port: 18686,
            time_port: 18687,
            ntp123: false,
            ntp123_port: 123,
            server_eligible: true,
            auto_apply: false,
            max_offset_ms: 100,
            max_step_ms: 5_000,
            allow_large_step: false,
            sync_interval: Duration::from_secs(60),
            seed_peers: Vec::new(),
        };
        assert!(config.validate().is_ok());
        config.discovery_port = 0;
        assert!(config.validate().is_err());
        config.discovery_port = 18686;
        config.secret = b"short".to_vec();
        assert!(config.validate().is_err());
    }

    #[test]
    fn node_id_rejects_bad_characters() {
        assert!(valid_node_id("node-12345"));
        assert!(!valid_node_id("node 12345"));
        assert!(!valid_node_id("short"));
    }
}
