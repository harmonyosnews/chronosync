use std::sync::Arc;

use tokio::net::UdpSocket;

use super::manager::Cluster;
use super::protocol::{PROTOCOL_VERSION, TimeRequest, TimeResponse};

const REFERENCE_MAX_AGE_MS: u64 = 300_000;

pub async fn run(cluster: Arc<Cluster>, socket: UdpSocket) {
    let mut buffer = [0u8; 4096];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buffer).await else {
            continue;
        };
        let Ok(request) = serde_json::from_slice::<TimeRequest>(&buffer[..len]) else {
            continue;
        };
        if request.v != PROTOCOL_VERSION
            || request.cluster_id != cluster.cfg().cluster_id
            || !request.verify(&cluster.cfg().secret)
            || !cluster.is_server()
        {
            continue;
        }
        let source = cluster.time_source();
        if !source.verified() || source.age_ms() > REFERENCE_MAX_AGE_MS {
            continue;
        }
        let t2 = source.now_epoch_ms();
        let t3 = source.now_epoch_ms();
        let mut response = TimeResponse {
            v: PROTOCOL_VERSION,
            cluster_id: cluster.cfg().cluster_id.clone(),
            server_id: cluster.cfg().node_id.clone(),
            term: cluster.term(),
            t1_ms: request.t1_ms,
            t2_ms: t2,
            t3_ms: t3,
            verified: true,
            ref_age_ms: source.age_ms(),
            seq: cluster.next_seq(),
            nonce: request.nonce,
            mac: String::new(),
        };
        if response.sign(&cluster.cfg().secret).is_err() {
            continue;
        }
        let Ok(bytes) = serde_json::to_vec(&response) else {
            continue;
        };
        if let Err(err) = socket.send_to(&bytes, from).await {
            cluster
                .sink()
                .log("warn", format!("发送时间响应失败：{err}"));
        }
    }
}
