use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::oneshot;

use super::REFERENCE_MAX_AGE_MS;
use super::manager::Cluster;
use crate::ntp;

const STRATUM: u8 = 3;
const UNSYNC_STRATUM: u8 = 16;

pub async fn run(cluster: Arc<Cluster>, socket: UdpSocket, mut stop: oneshot::Receiver<()>) {
    let mut buffer = [0u8; 512];
    loop {
        let received = tokio::select! {
            _ = &mut stop => break,
            result = socket.recv_from(&mut buffer) => result,
        };
        let Ok((len, from)) = received else {
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
        let response = build_response(&buffer, source.now_epoch_ms(), source.age_ms(), verified);
        if let Err(err) = socket.send_to(&response, from).await {
            cluster
                .sink()
                .log("warn", format!("发送 NTPv4 响应失败：{err}"));
        }
    }
}

fn build_response(request: &[u8], now_epoch_ms: i64, age_ms: u64, verified: bool) -> [u8; 48] {
    let mut response = [0u8; 48];
    response[0] = (if verified { 0 } else { 3 } << 6) | (4 << 3) | 4;
    response[1] = if verified { STRATUM } else { UNSYNC_STRATUM };
    response[2] = 6;
    response[3] = 0xEC; // precision -20
    response[12..16].copy_from_slice(b"CSNC");
    let ref_secs =
        now_epoch_ms as f64 / 1000.0 - (age_ms.min(REFERENCE_MAX_AGE_MS) as f64 / 1000.0);
    response[16..24].copy_from_slice(&ntp::encode_ntp_timestamp(ref_secs));
    response[24..32].copy_from_slice(&request[40..48]); // origin = client transmit
    response[32..40].copy_from_slice(&ntp::encode_ntp_timestamp(now_epoch_ms as f64 / 1000.0));
    response[40..48].copy_from_slice(&ntp::encode_ntp_timestamp(now_epoch_ms as f64 / 1000.0));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(transmit: [u8; 8]) -> [u8; 48] {
        let mut request = [0u8; 48];
        request[0] = (4 << 3) | 3;
        request[40..48].copy_from_slice(&transmit);
        request
    }

    #[test]
    fn response_is_standard_ntp_v4_shape() {
        let transmit = [1, 2, 3, 4, 5, 6, 7, 8];
        let response = build_response(&request(transmit), 1_793_789_400_000, 1_500, true);
        assert_eq!(response.len(), 48);
        assert_eq!(response[0] & 0x07, 4);
        assert_eq!((response[0] >> 3) & 0x07, 4);
        assert_eq!(response[0] >> 6, 0);
        assert_eq!(response[1], STRATUM);
        assert_eq!(&response[12..16], b"CSNC");
        assert_eq!(&response[24..32], &transmit);
        assert!(response[16..24] != [0u8; 8]);
        assert!(response[32..40] != [0u8; 8]);
        assert_eq!(&response[32..40], &response[40..48]);
    }

    #[test]
    fn unsync_response_never_claims_primary_time() {
        let response = build_response(&request([9; 8]), 1_793_789_400_000, 600_000, false);
        assert_eq!(response[0] >> 6, 3);
        assert_eq!(response[1], UNSYNC_STRATUM);
        assert_eq!(UNSYNC_STRATUM, 16);
        assert_ne!(STRATUM, UNSYNC_STRATUM);
    }
}
