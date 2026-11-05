use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, Utc};
use tokio::net::{UdpSocket, lookup_host};
use tokio::task::JoinSet;

pub const BEIJING_OFFSET_SECS: i32 = 8 * 3600;
const NTP_UNIX_DELTA: f64 = 2_208_988_800.0;
const NTP_TIMEOUT: Duration = Duration::from_millis(1_500);

/// A standard-time anchor. `utc_ms_at_anchor` is the trusted UTC time at the
/// monotonic `Instant` when the reference was created, so the UI keeps ticking
/// correctly even if the host's own wall clock is wrong.
#[derive(Clone)]
pub struct TimeReference {
    anchor: Instant,
    utc_ms_at_anchor: i64,
    pub source: String,
    pub rtt_ms: f64,
    pub samples: usize,
    pub verified: bool,
    pub created_at_ms: i64,
}

impl TimeReference {
    pub fn from_ntp(utc_ms: i64, source: String, rtt_ms: f64, samples: usize) -> Self {
        Self {
            anchor: Instant::now(),
            utc_ms_at_anchor: utc_ms,
            source,
            rtt_ms,
            samples,
            verified: true,
            created_at_ms: utc_ms,
        }
    }

    pub fn from_system_now(source: impl Into<String>) -> Self {
        let utc_ms = system_epoch_ms();
        Self {
            anchor: Instant::now(),
            utc_ms_at_anchor: utc_ms,
            source: source.into(),
            rtt_ms: 0.0,
            samples: 0,
            verified: false,
            created_at_ms: utc_ms,
        }
    }

    pub fn now_epoch_ms(&self) -> i64 {
        self.utc_ms_at_anchor
            .saturating_add(self.anchor.elapsed().as_millis().min(i64::MAX as u128) as i64)
    }

    pub fn age_ms(&self) -> u64 {
        self.anchor.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
}

pub fn beijing_datetime(epoch_ms: i64) -> DateTime<FixedOffset> {
    let secs = epoch_ms.div_euclid(1000);
    let nanos = (epoch_ms.rem_euclid(1000) * 1_000_000) as u32;
    let utc = DateTime::<Utc>::from_timestamp(secs, nanos)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch"));
    utc.with_timezone(&FixedOffset::east_opt(BEIJING_OFFSET_SECS).expect("valid offset"))
}

pub fn format_beijing(epoch_ms: i64) -> String {
    beijing_datetime(epoch_ms)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn format_utc(epoch_ms: i64) -> String {
    let secs = epoch_ms.div_euclid(1000);
    let nanos = (epoch_ms.rem_euclid(1000) * 1_000_000) as u32;
    DateTime::<Utc>::from_timestamp(secs, nanos)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch"))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn system_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn system_epoch_secs_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn standard_ms_from_local_and_offset(local_secs: f64, offset_secs: f64) -> i64 {
    ((local_secs + offset_secs) * 1000.0).round() as i64
}

#[derive(Debug, Clone)]
struct Measurement {
    server: String,
    offset_secs: f64,
    rtt_secs: f64,
}

/// Query all configured SNTP servers concurrently and return a trusted
/// reference to Beijing standard time (UTC internally).
pub async fn probe(servers: &[String]) -> Result<TimeReference, String> {
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for server in servers {
        let server = server.trim();
        if server.is_empty() {
            continue;
        }
        let key = server.to_ascii_lowercase();
        if seen.insert(key) {
            unique.push(server.to_string());
        }
    }
    if unique.is_empty() {
        return Err("未配置可用的 NTP 服务器".to_string());
    }

    let mut set = JoinSet::new();
    for server in &unique {
        let server = server.clone();
        set.spawn(async move {
            (
                server.clone(),
                tokio::time::timeout(NTP_TIMEOUT, query_one(&server)).await,
            )
        });
    }

    let mut measurements = Vec::new();
    let mut errors = Vec::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((_server, Ok(Ok(measurement)))) => measurements.push(measurement),
            Ok((server, Ok(Err(err)))) => errors.push(format!("{server}: {err}")),
            Ok((server, Err(_))) => errors.push(format!("{server}: 超时")),
            Err(err) => errors.push(format!("查询任务异常: {err}")),
        }
    }

    if measurements.is_empty() {
        let detail = errors.first().cloned().unwrap_or_default();
        let mut message = format!("无法获取 NTP 标准时间（已尝试 {} 个时间源）", unique.len());
        if !detail.is_empty() {
            message.push_str(&format!("：{detail}"));
        }
        return Err(message);
    }

    // Reject outliers when we have enough agreeing sources, then prefer the
    // sample with the lowest round-trip time.
    let total_samples = measurements.len();
    let mut usable = measurements;
    if usable.len() >= 3 {
        let mut offsets: Vec<f64> = usable.iter().map(|m| m.offset_secs).collect();
        offsets.sort_by(|a, b| a.total_cmp(b));
        let median = offsets[offsets.len() / 2];
        let filtered: Vec<Measurement> = usable
            .iter()
            .filter(|m| (m.offset_secs - median).abs() <= 0.5)
            .cloned()
            .collect();
        if !filtered.is_empty() {
            usable = filtered;
        }
    }
    usable.sort_by(|a, b| a.rtt_secs.total_cmp(&b.rtt_secs));
    let best = &usable[0];

    // Anchor the standard-time reference to a local monotonic instant.
    let anchor = Instant::now();
    let local_now = system_epoch_secs_f64();
    // NTP's standard offset formula yields (server clock - local clock).
    let standard_ms = standard_ms_from_local_and_offset(local_now, best.offset_secs);
    let mut reference = TimeReference::from_ntp(
        standard_ms,
        best.server.clone(),
        best.rtt_secs * 1000.0,
        total_samples,
    );
    // `from_ntp` captures its own anchor a moment later; restore the earlier
    // anchor so the timestamp above and the monotonic base stay aligned.
    reference.anchor = anchor;
    Ok(reference)
}

async fn query_one(server: &str) -> Result<Measurement, String> {
    let addrs: Vec<_> = lookup_host((server, 123))
        .await
        .map_err(|err| format!("DNS 解析失败: {err}"))?
        .collect();
    let addr = addrs
        .iter()
        .find(|addr| addr.is_ipv4())
        .or_else(|| addrs.first())
        .copied()
        .ok_or_else(|| "DNS 解析无结果".to_string())?;

    let bind_addr = if addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind_addr)
        .await
        .map_err(|err| format!("创建 UDP 套接字失败: {err}"))?;

    let t1 = system_epoch_secs_f64();
    let mut request = [0u8; 48];
    // LI = 0, VN = 4, Mode = 3 (client)
    request[0] = 0x23;
    request[40..48].copy_from_slice(&encode_ntp_timestamp(t1));

    socket
        .send_to(&request, addr)
        .await
        .map_err(|err| format!("发送请求失败: {err}"))?;

    let mut buf = [0u8; 512];
    let (len, from) = socket
        .recv_from(&mut buf)
        .await
        .map_err(|err| format!("接收响应失败: {err}"))?;
    let t4 = system_epoch_secs_f64();
    if from.ip() != addr.ip() {
        return Err("响应来源与目标不一致".to_string());
    }
    parse_response(&buf[..len], t1, t4).map(|(offset_secs, rtt_secs)| Measurement {
        server: server.to_string(),
        offset_secs,
        rtt_secs,
    })
}

fn parse_response(packet: &[u8], t1: f64, t4: f64) -> Result<(f64, f64), String> {
    if packet.len() < 48 {
        return Err("NTP 响应长度无效".to_string());
    }
    let leap = packet[0] >> 6;
    let version = (packet[0] >> 3) & 0x07;
    let mode = packet[0] & 0x07;
    let stratum = packet[1];
    if leap == 3 {
        return Err("NTP 服务器时钟未同步".to_string());
    }
    if version < 3 || mode != 4 {
        return Err("不是有效的 NTP 服务器响应".to_string());
    }
    if stratum == 0 || stratum == 0xFF {
        return Err("NTP 响应为 Kiss-o'-Death 或未同步状态".to_string());
    }

    let t2 = decode_ntp_timestamp(&packet[32..40]);
    let t3 = decode_ntp_timestamp(&packet[40..48]);
    if t3 <= 0.0 {
        return Err("NTP 响应缺少发送时间戳".to_string());
    }
    let offset = ((t2 - t1) + (t3 - t4)) / 2.0;
    let rtt = (t4 - t1) - (t3 - t2);
    if !offset.is_finite() || !rtt.is_finite() || rtt < 0.0 {
        return Err("NTP 时间差值无效".to_string());
    }
    Ok((offset, rtt))
}

fn encode_ntp_timestamp(unix_secs: f64) -> [u8; 8] {
    let ntp_secs = unix_secs + NTP_UNIX_DELTA;
    let secs = ntp_secs.floor().max(0.0) as u32;
    let fraction = ((ntp_secs - secs as f64) * 4_294_967_296.0).clamp(0.0, 4_294_967_295.0) as u32;
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&secs.to_be_bytes());
    bytes[4..].copy_from_slice(&fraction.to_be_bytes());
    bytes
}

fn decode_ntp_timestamp(bytes: &[u8]) -> f64 {
    if bytes.len() < 8 {
        return 0.0;
    }
    let secs = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64;
    let fraction = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as f64;
    secs - NTP_UNIX_DELTA + fraction / 4_294_967_296.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beijing_format_is_utc_plus_eight() {
        // 2026-11-04 10:00:00 UTC == 18:00:00 Beijing
        let ms = 1_793_786_400_000;
        assert_eq!(format_beijing(ms), "2026-11-04 18:00:00");
        assert_eq!(format_utc(ms), "2026-11-04 10:00:00");
    }

    #[test]
    fn ntp_timestamp_roundtrip() {
        let now = 1_793_786_400.125;
        let encoded = encode_ntp_timestamp(now);
        let decoded = decode_ntp_timestamp(&encoded);
        assert!((decoded - now).abs() < 0.000_001);
    }

    #[test]
    fn parses_a_valid_server_response() {
        let t1 = 1_793_786_400.0;
        let t4 = 1_793_786_400.1;
        let t2 = t1 + 0.05; // server thinks it is 50 ms ahead of the local clock
        let t3 = t2 + 0.001;
        let mut packet = [0u8; 48];
        packet[0] = 0x24; // LI 0, VN 4, mode 4
        packet[1] = 2;
        packet[32..40].copy_from_slice(&encode_ntp_timestamp(t2));
        packet[40..48].copy_from_slice(&encode_ntp_timestamp(t3));
        let (offset, rtt) = parse_response(&packet, t1, t4).unwrap();
        assert!((offset - 0.000_5).abs() < 1e-6);
        assert!((rtt - 0.099).abs() < 1e-6);
    }

    #[test]
    fn ntp_offset_sign_is_applied_to_local_clock() {
        // Server is 14.3 s ahead of the local clock.
        let local = 1_793_786_400.0;
        let offset = 14.3;
        assert_eq!(
            standard_ms_from_local_and_offset(local, offset),
            1_793_786_414_300
        );
    }

    #[test]
    fn rejects_kiss_of_death() {
        let mut packet = [0u8; 48];
        packet[0] = 0x24;
        packet[1] = 0;
        let err = parse_response(&packet, 1.0, 2.0).unwrap_err();
        assert!(err.contains("未同步"));
    }
}
