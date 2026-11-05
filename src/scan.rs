use std::collections::{BTreeMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use if_addrs::IfAddr;
use serde::Serialize;
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::timeout;

const SSH_PORT: u16 = 22;
const PROBE_TIMEOUT: Duration = Duration::from_millis(450);
const PROBE_CONCURRENCY: usize = 128;
const REVERSE_DNS_TIMEOUT: Duration = Duration::from_millis(900);
const REVERSE_DNS_CONCURRENCY: usize = 32;
const MAX_ADDRESSES: usize = 1024;
const MAX_RANGES: usize = 4;

#[derive(Debug, Clone)]
pub struct ScanPlan {
    pub ranges: Vec<String>,
    addresses: Vec<Ipv4Addr>,
}

impl ScanPlan {
    pub fn addresses_len(&self) -> usize {
        self.addresses.len()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanHost {
    pub ip: String,
    pub hostname: Option<String>,
    pub ssh_open: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanResult {
    pub ranges: Vec<String>,
    pub scanned: usize,
    pub active_count: usize,
    pub ssh_count: usize,
    pub include_non_ssh: bool,
    pub duration_ms: u64,
    pub hosts: Vec<ScanHost>,
}

/// Build the scan plan without probing anything. Empty `cidrs` means auto
/// detect the control host's local IPv4 networks (limited to one /24 per
/// interface when the interface has a broader prefix).
pub async fn plan(cidrs: &[String]) -> Result<ScanPlan, String> {
    if cidrs.iter().any(|value| !value.trim().is_empty()) {
        plan_custom(cidrs)
    } else {
        plan_auto().await
    }
}

pub async fn execute(plan: ScanPlan, include_non_ssh: bool) -> ScanResult {
    let started = Instant::now();
    let live = probe_addresses(&plan.addresses).await;
    let live = add_neighbors(plan.addresses.iter().copied().collect(), live).await;

    let hostnames = reverse_resolve(live.keys().copied().collect()).await;
    let active_count = live.len();
    let ssh_count = live.values().filter(|value| **value).count();
    let mut hosts = Vec::new();
    for (ip, is_ssh) in &live {
        if !include_non_ssh && !is_ssh {
            continue;
        }
        hosts.push(ScanHost {
            ip: ip.to_string(),
            hostname: hostnames.get(ip).cloned().flatten(),
            ssh_open: *is_ssh,
        });
    }
    hosts.sort_by_key(|host| host.ip.parse::<Ipv4Addr>().ok());

    ScanResult {
        ranges: plan.ranges,
        scanned: plan.addresses.len(),
        active_count,
        ssh_count,
        include_non_ssh,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        hosts,
    }
}

fn plan_custom(cidrs: &[String]) -> Result<ScanPlan, String> {
    if cidrs.iter().map(|value| value.len()).sum::<usize>() > 512 {
        return Err("扫描范围输入过长".to_string());
    }
    let mut networks = Vec::new();
    let mut seen = HashSet::new();
    for raw in cidrs {
        for token in raw.split([',', ' ', '\t', '\n']) {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let (addr, prefix) = parse_cidr(token)?;
            if prefix < 22 {
                return Err(format!(
                    "扫描范围 {token} 过大；每个网段最多 1024 个地址（请使用 /22 或更小范围）"
                ));
            }
            let mask = prefix_mask(prefix);
            let network = Ipv4Addr::from(u32::from(addr) & mask);
            let key = (u32::from(network), prefix);
            if seen.insert(key) {
                networks.push((network, prefix));
            }
        }
    }
    if networks.is_empty() {
        return Err("请填写要扫描的 IPv4 网段，例如 192.0.2.0/24".to_string());
    }
    if networks.len() > MAX_RANGES {
        return Err(format!("单次最多扫描 {MAX_RANGES} 个网段"));
    }
    let mut addresses = Vec::new();
    let mut ranges = Vec::new();
    for (network, prefix) in networks {
        let generated = addresses_for_network(network, prefix);
        if addresses.len() + generated.len() > MAX_ADDRESSES {
            return Err(format!(
                "扫描地址总数超过 {MAX_ADDRESSES} 个，请缩小网段范围"
            ));
        }
        addresses.extend(generated);
        ranges.push(format!("{network}/{prefix}"));
    }
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.len() > MAX_ADDRESSES {
        return Err(format!(
            "扫描地址总数超过 {MAX_ADDRESSES} 个，请缩小网段范围"
        ));
    }
    Ok(ScanPlan { ranges, addresses })
}

async fn plan_auto() -> Result<ScanPlan, String> {
    let interfaces = tokio::task::spawn_blocking(if_addrs::get_if_addrs)
        .await
        .map_err(|err| format!("读取网卡信息任务失败：{err}"))?
        .map_err(|err| format!("读取网卡信息失败：{err}"))?;
    let mut own_ips = HashSet::new();
    let mut networks: Vec<(Ipv4Addr, u8)> = Vec::new();
    let mut seen = HashSet::new();

    for interface in interfaces {
        if should_skip_interface(&interface.name) {
            continue;
        }
        let IfAddr::V4(v4) = interface.addr else {
            continue;
        };
        if v4.ip.is_loopback() || v4.ip.is_link_local() || v4.ip.is_unspecified() {
            continue;
        }
        own_ips.insert(v4.ip);
        if v4.prefixlen > 30 {
            continue;
        }
        let scan_prefix = v4.prefixlen.max(24);
        let network = Ipv4Addr::from(u32::from(v4.ip) & prefix_mask(scan_prefix));
        if seen.insert((u32::from(network), scan_prefix)) {
            networks.push((network, scan_prefix));
        }
        if networks.len() >= MAX_RANGES {
            break;
        }
    }

    if networks.is_empty() {
        return Err("未找到可扫描的局域网 IPv4 网段，请手动填写扫描范围".to_string());
    }

    let mut addresses = Vec::new();
    let mut ranges = Vec::new();
    for (network, prefix) in networks {
        let generated = addresses_for_network(network, prefix);
        if addresses.len() + generated.len() > MAX_ADDRESSES {
            break;
        }
        addresses.extend(generated.into_iter().filter(|ip| !own_ips.contains(ip)));
        ranges.push(format!("{network}/{prefix}"));
    }
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err("自动检测到的网段没有可扫描地址，请手动填写扫描范围".to_string());
    }
    Ok(ScanPlan { ranges, addresses })
}

fn should_skip_interface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("docker")
        || name.starts_with("veth")
        || name.starts_with("virbr")
        || name.starts_with("tun")
        || name.starts_with("tap")
        || name.starts_with("loopback")
}

#[cfg(unix)]
async fn ip_command(args: &[&str]) -> Result<String, String> {
    for executable in ["ip", "/sbin/ip", "/usr/sbin/ip"] {
        match Command::new(executable).args(args).output().await {
            Ok(output) if output.status.success() => {
                return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                return Err(format!("{executable} 执行失败：{stderr}"));
            }
            Err(_) => continue,
        }
    }
    Err("找不到 ip 命令，无法读取邻居表".to_string())
}

async fn probe_addresses(addresses: &[Ipv4Addr]) -> BTreeMap<Ipv4Addr, bool> {
    let semaphore = Arc::new(Semaphore::new(PROBE_CONCURRENCY));
    let mut tasks = JoinSet::new();
    for ip in addresses.iter().copied() {
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let result = timeout(PROBE_TIMEOUT, TcpStream::connect((ip, SSH_PORT))).await;
            match result {
                Ok(Ok(_stream)) => (ip, Some(true)),
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                    (ip, Some(false))
                }
                _ => (ip, None),
            }
        });
    }
    let mut live = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok((ip, Some(is_ssh))) = result {
            live.insert(ip, is_ssh);
        }
    }
    live
}

#[cfg(unix)]
async fn add_neighbors(
    addresses: HashSet<Ipv4Addr>,
    mut live: BTreeMap<Ipv4Addr, bool>,
) -> BTreeMap<Ipv4Addr, bool> {
    if addresses.is_empty() {
        return live;
    }
    let Ok(output) = ip_command(&["-4", "neigh", "show"]).await else {
        return live;
    };
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let Some(first) = tokens.first() else {
            continue;
        };
        let Ok(ip) = first.parse::<Ipv4Addr>() else {
            continue;
        };
        if !addresses.contains(&ip) {
            continue;
        }
        let state = tokens.last().copied().unwrap_or("");
        if matches!(state, "FAILED" | "INCOMPLETE") {
            continue;
        }
        if line.contains("lladdr") {
            live.entry(ip).or_insert(false);
        }
    }
    live
}

#[cfg(not(unix))]
async fn add_neighbors(
    _addresses: HashSet<Ipv4Addr>,
    live: BTreeMap<Ipv4Addr, bool>,
) -> BTreeMap<Ipv4Addr, bool> {
    live
}

async fn reverse_resolve(addresses: Vec<Ipv4Addr>) -> BTreeMap<Ipv4Addr, Option<String>> {
    let semaphore = Arc::new(Semaphore::new(REVERSE_DNS_CONCURRENCY));
    let mut tasks = JoinSet::new();
    for ip in addresses {
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let result = timeout(
                REVERSE_DNS_TIMEOUT,
                tokio::task::spawn_blocking(move || reverse_lookup(ip)),
            )
            .await;
            let name = match result {
                Ok(Ok(value)) => value,
                _ => None,
            };
            (ip, name)
        });
    }
    let mut names = BTreeMap::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok((ip, name)) = result {
            names.insert(ip, name);
        }
    }
    names
}

fn reverse_lookup(ip: Ipv4Addr) -> Option<String> {
    let name = dns_lookup::lookup_addr(&IpAddr::V4(ip)).ok()?;
    let name = name.trim().trim_end_matches('.').to_string();
    if name.is_empty() || name == ip.to_string() {
        None
    } else {
        Some(name.chars().take(80).collect())
    }
}

fn parse_cidr(value: &str) -> Result<(Ipv4Addr, u8), String> {
    let (address, prefix) = match value.split_once('/') {
        Some((address, prefix)) => (address, prefix.parse::<u8>().ok()),
        None => (value, Some(32)),
    };
    let address = address
        .parse::<Ipv4Addr>()
        .map_err(|_| format!("无效的 IPv4 地址：{value}"))?;
    let prefix = prefix.ok_or_else(|| format!("无效的网段前缀：{value}"))?;
    if prefix > 32 {
        return Err(format!("无效的网段前缀：{value}"));
    }
    Ok((address, prefix))
}

fn prefix_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else if prefix >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix)
    }
}

fn addresses_for_network(network: Ipv4Addr, prefix: u8) -> Vec<Ipv4Addr> {
    let base = u32::from(network);
    let count = if prefix >= 32 {
        1u64
    } else {
        1u64 << (32 - prefix)
    };
    let (start, end) = if prefix >= 31 {
        (0, count)
    } else {
        (1, count.saturating_sub(1))
    };
    (start..end)
        .filter_map(|offset| {
            let value = (base as u64).checked_add(offset)?;
            if value > u32::MAX as u64 {
                None
            } else {
                Some(Ipv4Addr::from(value as u32))
            }
        })
        .collect()
}

#[allow(dead_code)]
fn ip_from_neighbor_line(line: &str) -> Option<(Ipv4Addr, bool)> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let ip = tokens.first()?.parse::<Ipv4Addr>().ok()?;
    let state = tokens.last()?;
    let usable = line.contains("lladdr") && !matches!(*state, "FAILED" | "INCOMPLETE");
    Some((ip, usable))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cidr_and_generates_hosts() {
        let (network, prefix) = parse_cidr("192.0.2.0/24").unwrap();
        assert_eq!(prefix, 24);
        let hosts = addresses_for_network(network, prefix);
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts.first(), Some(&Ipv4Addr::new(192, 0, 2, 1)));
        assert_eq!(hosts.last(), Some(&Ipv4Addr::new(192, 0, 2, 254)));
    }

    #[test]
    fn parses_single_host_cidr() {
        let (network, prefix) = parse_cidr("192.0.2.44/32").unwrap();
        assert_eq!(network, Ipv4Addr::new(192, 0, 2, 44));
        assert_eq!(prefix, 32);
        assert_eq!(
            addresses_for_network(network, prefix),
            vec![Ipv4Addr::new(192, 0, 2, 44)]
        );
    }

    #[test]
    fn rejects_too_large_custom_networks() {
        let result = plan_custom(&["192.0.0.0/16".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn parses_neighbor_states() {
        assert_eq!(
            ip_from_neighbor_line("192.0.2.1 dev eth0 lladdr aa:bb:cc:dd:ee:ff REACHABLE"),
            Some((Ipv4Addr::new(192, 0, 2, 1), true))
        );
        assert_eq!(
            ip_from_neighbor_line("192.0.2.2 dev eth0 FAILED"),
            Some((Ipv4Addr::new(192, 0, 2, 2), false))
        );
    }

    #[test]
    fn custom_multi_range_is_deduped() {
        let plan = plan_custom(&["192.0.2.0/24, 192.0.2.0/24".to_string()]).unwrap();
        assert_eq!(plan.ranges, vec!["192.0.2.0/24"]);
        assert_eq!(plan.addresses.len(), 254);
    }
}
