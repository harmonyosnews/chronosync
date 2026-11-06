use std::sync::Arc;

use tokio::net::UdpSocket;

use super::REFERENCE_MAX_AGE_MS;
use super::manager::Cluster;
use crate::ntp;

const STRATUM: u8 = 3;
const UNSYNC_STRATUM: u8 = 16;

pub async fn run(cluster: Arc<Cluster>, socket: UdpSocket) {
    let mut buffer = [0u8; 512];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buffer).await else {
            continue;
        };
        if len < 48 || (buffer[0] & 0x07) != 3 || ((buffer[0] >> 3) & 0x07) < 3 {
            continue;
        }
        if !cluster.is_server() {
            continue;
        }
        let source = cluster.time_source();
        let verified = source.verified() && source.age_ms() <= REFERENCE_MAX_AGE_MS;
        let t2 = source.now_epoch_ms() as f64 / 1000.0;
        let t3 = source.now_epoch_ms() as f64 / 1000.0;
        let ref_secs = source.now_epoch_ms() as f64 / 1000.0
            - (source.age_ms().min(REFERENCE_MAX_AGE_MS) as f64 / 1000.0);
        let mut response = [0u8; 48];
        response[0] = (if verified { 0 } else { 3 } << 6) | (4 << 3) | 4;
        response[1] = if verified { STRATUM } else { UNSYNC_STRATUM };
        response[2] = 6;
        response[3] = 0xEC; // precision -20
        response[12..16].copy_from_slice(b"CSNC");
        response[16..24].copy_from_slice(&ntp::encode_ntp_timestamp(ref_secs));
        response[24..32].copy_from_slice(&buffer[40..48]); // origin = client transmit
        response[32..40].copy_from_slice(&ntp::encode_ntp_timestamp(t2));
        response[40..48].copy_from_slice(&ntp::encode_ntp_timestamp(t3));
        if let Err(err) = socket.send_to(&response, from).await {
            cluster
                .sink()
                .log("warn", format!("发送 NTPv4 响应失败：{err}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsync_stratum_is_safe() {
        assert_eq!(UNSYNC_STRATUM, 16);
        assert_ne!(STRATUM, UNSYNC_STRATUM);
    }
}
