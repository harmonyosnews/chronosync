use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use tokio::net::UdpSocket;
use tokio::time::{self, timeout};

use crate::cluster::config::{Config, Mode};
use crate::cluster::protocol::{
    Announce, KIND_CLAIM, KIND_HEARTBEAT, KIND_HELLO, KIND_RESIGN, PROTOCOL_VERSION, TimeRequest,
    TimeResponse,
};
use crate::cluster::{EventSink, REFERENCE_MAX_AGE_MS, TimeSource};
use crate::ntp;

const SERVER_TIMEOUT: Duration = Duration::from_millis(3_500);
const DISCOVERY_WINDOW: Duration = Duration::from_millis(2_500);
const CLAIM_COLLECT: Duration = Duration::from_millis(500);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const HELLO_INTERVAL: Duration = Duration::from_secs(2);
const PEER_STALE: Duration = Duration::from_secs(10);
const TIME_QUALITY_LIMIT_MS: u64 = 300_000;
const SAMPLES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Client,
    Candidate,
    Server,
    Discovering,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Candidate => "candidate",
            Self::Server => "server",
            Self::Discovering => "discovering",
        }
    }
}

#[derive(Clone, Debug)]
struct Peer {
    node_id: String,
    addr: SocketAddr,
    term: u64,
    last_seen: Instant,
}

#[derive(Clone, Debug)]
struct Claim {
    term: u64,
    node_id: String,
}

#[derive(Clone, Serialize)]
pub struct SyncView {
    pub offset_ms: i64,
    pub rtt_ms: u64,
    pub at: String,
    pub applied: bool,
    pub message: String,
}

#[derive(Clone, Serialize)]
pub struct ClusterView {
    pub enabled: bool,
    pub mode: String,
    pub role: String,
    pub term: u64,
    pub node_id: String,
    pub server_id: Option<String>,
    pub server_addr: Option<String>,
    pub peers: usize,
    pub ntp_verified: bool,
    pub auto_apply: bool,
    pub last_sync: Option<SyncView>,
    pub last_error: Option<String>,
}

struct Inner {
    role: Role,
    term: u64,
    server: Option<Peer>,
    peers: HashMap<String, Peer>,
    discovery_start: Instant,
    last_server_seen: Option<Instant>,
    candidate_deadline: Option<Instant>,
    collect_until: Option<Instant>,
    claim_sent: bool,
    claims: HashMap<String, Claim>,
    last_sync: Option<SyncView>,
    last_error: Option<String>,
    last_heartbeat: Option<Instant>,
    last_hello: Option<Instant>,
    pending_resign: bool,
    packet_errors: u64,
}

pub struct Cluster {
    cfg: Arc<Config>,
    inner: Arc<Mutex<Inner>>,
    time_source: Arc<dyn TimeSource>,
    sink: Arc<dyn EventSink>,
    seq: AtomicU64,
    seeds: Vec<SocketAddr>,
}

impl Cluster {
    pub fn view(&self) -> ClusterView {
        let state = lock(&self.inner);
        let server_id = match state.role {
            Role::Server => Some(self.cfg.node_id.clone()),
            _ => state.server.as_ref().map(|peer| peer.node_id.clone()),
        };
        let server_addr = if state.role == Role::Server {
            None
        } else {
            state.server.as_ref().map(|peer| peer.addr.to_string())
        };
        ClusterView {
            enabled: true,
            mode: self.cfg.mode.as_str().to_string(),
            role: state.role.as_str().to_string(),
            term: state.term,
            node_id: self.cfg.node_id.clone(),
            server_id,
            server_addr,
            peers: state.peers.len(),
            ntp_verified: self.time_source.verified()
                && self.time_source.age_ms() <= REFERENCE_MAX_AGE_MS,
            auto_apply: self.cfg.auto_apply,
            last_sync: state.last_sync.clone(),
            last_error: state.last_error.clone(),
        }
    }

    pub(crate) fn cfg(&self) -> &Config {
        &self.cfg
    }

    pub(crate) fn time_source(&self) -> &Arc<dyn TimeSource> {
        &self.time_source
    }

    pub(crate) fn sink(&self) -> &Arc<dyn EventSink> {
        &self.sink
    }

    pub(crate) fn is_server(&self) -> bool {
        lock(&self.inner).role == Role::Server
    }

    pub(crate) fn term(&self) -> u64 {
        lock(&self.inner).term
    }

    pub(crate) fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub(crate) fn set_last_error(&self, error: Option<String>) {
        lock(&self.inner).last_error = error;
    }

    pub(crate) fn set_last_sync(&self, sync: SyncView) {
        lock(&self.inner).last_sync = Some(sync);
    }

    pub(crate) fn server_endpoint(&self) -> Option<(String, SocketAddr)> {
        let state = lock(&self.inner);
        if state.role != Role::Client {
            return None;
        }
        state
            .server
            .as_ref()
            .map(|peer| (peer.node_id.clone(), peer.addr))
    }

    fn record_packet_error(&self) {
        let mut state = lock(&self.inner);
        state.packet_errors = state.packet_errors.saturating_add(1);
    }

    fn on_announce(&self, message: Announce, from: SocketAddr) {
        if message.v != PROTOCOL_VERSION
            || message.cluster_id != self.cfg.cluster_id
            || message.node_id == self.cfg.node_id
        {
            return;
        }
        if !message.verify(&self.cfg.secret) {
            self.record_packet_error();
            return;
        }
        if message.time_port == 0 || from.ip().is_unspecified() {
            return;
        }

        let now = Instant::now();
        let peer = Peer {
            node_id: message.node_id.clone(),
            addr: SocketAddr::new(from.ip(), message.time_port),
            term: message.term,
            last_seen: now,
        };
        let mut state = lock(&self.inner);
        state.peers.insert(message.node_id.clone(), peer.clone());

        match message.kind.as_str() {
            KIND_HEARTBEAT if message.role == "server" => {
                if !message.ntp_verified {
                    return;
                }
                if state.role == Role::Server {
                    let lose = message.term > state.term
                        || (message.term == state.term && message.node_id < self.cfg.node_id);
                    if lose {
                        let old_term = state.term;
                        state.term = message.term;
                        state.role = Role::Client;
                        state.server = Some(peer);
                        state.last_server_seen = Some(now);
                        state.pending_resign = true;
                        drop(state);
                        self.sink.log(
                            "warn",
                            format!(
                                "集群仲裁：server term {} 让位给 {} term {}",
                                old_term, message.node_id, message.term
                            ),
                        );
                    }
                    return;
                }

                let accept = match &state.server {
                    None => true,
                    Some(current) => {
                        message.term > current.term
                            || (message.term == current.term && message.node_id < current.node_id)
                    }
                };
                if accept {
                    let changed = state
                        .server
                        .as_ref()
                        .map(|old| old.node_id != message.node_id)
                        .unwrap_or(true);
                    state.term = state.term.max(message.term);
                    state.server = Some(peer);
                    state.role = Role::Client;
                    state.last_server_seen = Some(now);
                    state.claims.clear();
                    state.claim_sent = false;
                    state.candidate_deadline = None;
                    state.collect_until = None;
                    state.last_error = None;
                    if changed {
                        drop(state);
                        self.sink
                            .log("system", format!("集群发现 server：{}", message.node_id));
                    }
                }
            }
            KIND_CLAIM if message.role == "candidate" => {
                if message.term > state.term {
                    state.term = message.term;
                }
                state.claims.insert(
                    message.node_id.clone(),
                    Claim {
                        term: message.term,
                        node_id: message.node_id,
                    },
                );
            }
            KIND_RESIGN
                if state.server.as_ref().map(|peer| peer.node_id.as_str())
                    == Some(message.node_id.as_str()) =>
            {
                state.server = None;
                state.last_server_seen = None;
            }
            _ => {}
        }
    }

    async fn tick(&self, socket: &UdpSocket) {
        let now = Instant::now();
        let mut send: Option<&'static str> = None;
        let mut events: Vec<(&'static str, String)> = Vec::new();

        {
            let mut state = lock(&self.inner);
            state
                .peers
                .retain(|_, peer| now.duration_since(peer.last_seen) < PEER_STALE);

            if state.pending_resign {
                state.pending_resign = false;
                state.role = Role::Client;
                send = Some(KIND_RESIGN);
            }

            match state.role {
                Role::Server => {
                    if state
                        .last_heartbeat
                        .map(|last| now.duration_since(last) >= HEARTBEAT_INTERVAL)
                        .unwrap_or(true)
                    {
                        state.last_heartbeat = Some(now);
                        send = Some(KIND_HEARTBEAT);
                    }
                }
                Role::Discovering => {
                    if state
                        .last_hello
                        .map(|last| now.duration_since(last) >= HELLO_INTERVAL)
                        .unwrap_or(true)
                    {
                        state.last_hello = Some(now);
                        send = Some(KIND_HELLO);
                    }
                    if now.duration_since(state.discovery_start) >= DISCOVERY_WINDOW {
                        if self.cfg.server_eligible
                            && self.time_source.verified()
                            && self.time_source.age_ms() <= REFERENCE_MAX_AGE_MS
                        {
                            state.role = Role::Candidate;
                            state.claim_sent = false;
                            state.claims.clear();
                            state.candidate_deadline = Some(now + random_delay());
                            events.push(("info", "集群未发现 server，准备参与选举".to_string()));
                        } else {
                            state.last_error =
                                Some("未发现 server，且本节点不参与选举".to_string());
                        }
                    }
                }
                Role::Candidate => {
                    if state
                        .last_hello
                        .map(|last| now.duration_since(last) >= HELLO_INTERVAL)
                        .unwrap_or(true)
                    {
                        state.last_hello = Some(now);
                        send = Some(KIND_HELLO);
                    }
                    if !state.claim_sent {
                        if state
                            .candidate_deadline
                            .map(|deadline| now >= deadline)
                            .unwrap_or(true)
                        {
                            state.term = state.term.saturating_add(1);
                            let election_term = state.term;
                            state.claim_sent = true;
                            state.collect_until = Some(now + CLAIM_COLLECT);
                            state.claims.clear();
                            state.claims.insert(
                                self.cfg.node_id.clone(),
                                Claim {
                                    term: election_term,
                                    node_id: self.cfg.node_id.clone(),
                                },
                            );
                            send = Some(KIND_CLAIM);
                            events.push(("info", format!("发起选举 term {election_term}")));
                        }
                    } else if state
                        .collect_until
                        .map(|deadline| now >= deadline)
                        .unwrap_or(false)
                    {
                        let max_term = state
                            .claims
                            .values()
                            .map(|claim| claim.term)
                            .max()
                            .unwrap_or(state.term);
                        let winner = state
                            .claims
                            .values()
                            .filter(|claim| claim.term == max_term)
                            .min_by(|a, b| a.node_id.cmp(&b.node_id))
                            .cloned();
                        state.claims.clear();
                        state.claim_sent = false;
                        state.collect_until = None;
                        if let Some(winner) = winner {
                            if winner.node_id == self.cfg.node_id {
                                state.term = max_term;
                                state.role = Role::Server;
                                state.server = None;
                                state.last_heartbeat = None;
                                state.last_error = None;
                                events.push((
                                    "success",
                                    format!("当选集群 server · term {max_term}"),
                                ));
                            } else if let Some(peer) = state.peers.get(&winner.node_id).cloned() {
                                state.term = max_term;
                                state.site_server_start(peer, now);
                                events.push((
                                    "system",
                                    format!("选举完成，跟随 server {}", winner.node_id),
                                ));
                            } else {
                                state.role = Role::Discovering;
                                state.discovery_start = now;
                                state.last_error = Some("选举赢家地址未知，重新发现".to_string());
                            }
                        } else {
                            state.role = Role::Discovering;
                            state.discovery_start = now;
                            state.last_error = Some("选举没有有效候选，重新发现".to_string());
                        }
                    }
                }
                Role::Client => {
                    if state
                        .last_hello
                        .map(|last| now.duration_since(last) >= HELLO_INTERVAL)
                        .unwrap_or(true)
                    {
                        state.last_hello = Some(now);
                        send = Some(KIND_HELLO);
                    }
                    let expired = state
                        .last_server_seen
                        .map(|last| now.duration_since(last) >= SERVER_TIMEOUT)
                        .unwrap_or(true);
                    if expired && self.cfg.mode == Mode::Auto {
                        state.role = Role::Discovering;
                        state.discovery_start = now;
                        state.server = None;
                        state.last_error = Some("server 心跳超时，重新发现".to_string());
                        events.push(("warn", "server 心跳超时，开始重新选举".to_string()));
                    } else if expired {
                        state.server = None;
                        state.last_error = Some("未发现可用 server".to_string());
                    }
                }
            }
        }

        if let Some(kind) = send
            && let Err(err) = self.send_announce(socket, kind).await
        {
            self.set_last_error(Some(format!("发送集群消息失败：{err}")));
        }
        for (level, message) in events {
            self.sink.log(level, message);
        }
    }

    async fn send_announce(&self, socket: &UdpSocket, kind: &str) -> Result<(), String> {
        let (role, term) = {
            let state = lock(&self.inner);
            (state.role.as_str().to_string(), state.term)
        };
        let mut message = Announce {
            v: PROTOCOL_VERSION,
            kind: kind.to_string(),
            cluster_id: self.cfg.cluster_id.clone(),
            node_id: self.cfg.node_id.clone(),
            role: if kind == KIND_CLAIM {
                "candidate".to_string()
            } else if role == "server" {
                "server".to_string()
            } else {
                "client".to_string()
            },
            term,
            eligible: self.cfg.server_eligible,
            ntp_verified: self.time_source.verified()
                && self.time_source.age_ms() <= REFERENCE_MAX_AGE_MS,
            server_epoch_ms: self.time_source.now_epoch_ms(),
            sent_at_ms: ntp::system_epoch_ms(),
            seq: self.next_seq(),
            nonce: random_u64(),
            time_port: self.cfg.time_port,
            mac: String::new(),
        };
        message
            .sign(&self.cfg.secret)
            .map_err(|err| format!("签名失败：{err}"))?;
        let bytes =
            serde_json::to_vec(&message).map_err(|err| format!("编码集群消息失败：{err}"))?;
        let mut destinations = vec![SocketAddr::from((
            [255, 255, 255, 255],
            self.cfg.discovery_port,
        ))];
        destinations.extend(self.seeds.iter().copied());
        destinations.sort();
        destinations.dedup();
        let mut sent = false;
        let mut last_error = None;
        for destination in destinations {
            match socket.send_to(&bytes, destination).await {
                Ok(_) => sent = true,
                Err(err) => last_error = Some(err.to_string()),
            }
        }
        if !sent {
            return Err(last_error.unwrap_or_else(|| "没有可用的集群发送地址".to_string()));
        }
        Ok(())
    }
}

impl Inner {
    fn site_server_start(&mut self, peer: Peer, now: Instant) {
        self.server = Some(peer);
        self.role = Role::Client;
        self.last_server_seen = Some(now);
        self.last_error = None;
    }
}

pub async fn start(
    cfg: Config,
    time_source: Arc<dyn TimeSource>,
    sink: Arc<dyn EventSink>,
) -> Result<Arc<Cluster>, String> {
    cfg.validate()?;
    let discovery = UdpSocket::bind(SocketAddr::new(cfg.listen_ip, cfg.discovery_port))
        .await
        .map_err(|err| format!("绑定集群发现端口 {} 失败：{err}", cfg.discovery_port))?;
    discovery
        .set_broadcast(true)
        .map_err(|err| format!("启用 UDP 广播失败：{err}"))?;
    let time_socket = UdpSocket::bind(SocketAddr::new(cfg.listen_ip, cfg.time_port))
        .await
        .map_err(|err| format!("绑定集群时间端口 {} 失败：{err}", cfg.time_port))?;

    let seeds = resolve_seeds(&cfg).await?;
    let now = Instant::now();
    let role = match cfg.mode {
        Mode::Server => Role::Server,
        Mode::Auto => Role::Discovering,
        _ => Role::Client,
    };
    let cluster = Arc::new(Cluster {
        inner: Arc::new(Mutex::new(Inner {
            role,
            term: if cfg.mode == Mode::Server { 1 } else { 0 },
            server: None,
            peers: HashMap::new(),
            discovery_start: now,
            last_server_seen: None,
            candidate_deadline: None,
            collect_until: None,
            claim_sent: false,
            claims: HashMap::new(),
            last_sync: None,
            last_error: None,
            last_heartbeat: None,
            last_hello: None,
            pending_resign: false,
            packet_errors: 0,
        })),
        cfg: Arc::new(cfg),
        time_source,
        sink,
        seq: AtomicU64::new(0),
        seeds,
    });

    cluster.sink.log(
        "system",
        format!(
            "集群模式启动 · mode={} cluster={} node={} discovery={}/udp time={}/udp",
            cluster.cfg.mode.as_str(),
            cluster.cfg.cluster_id,
            cluster.cfg.node_id,
            cluster.cfg.discovery_port,
            cluster.cfg.time_port
        ),
    );

    let discovery = Arc::new(discovery);
    tokio::spawn(receive_loop(cluster.clone(), discovery.clone()));
    tokio::spawn(manager_loop(cluster.clone(), discovery));
    tokio::spawn(super::time_service::run(cluster.clone(), time_socket));
    tokio::spawn(client_loop(cluster.clone()));
    Ok(cluster)
}

async fn resolve_seeds(cfg: &Config) -> Result<Vec<SocketAddr>, String> {
    let mut seeds = Vec::new();
    for seed in &cfg.seed_peers {
        let addrs: Vec<_> = tokio::net::lookup_host(seed)
            .await
            .map_err(|err| format!("解析 seed {seed} 失败：{err}"))?
            .collect();
        let addr = addrs
            .iter()
            .find(|addr| addr.is_ipv4())
            .or_else(|| addrs.first())
            .copied()
            .ok_or_else(|| format!("seed {seed} 没有可用地址"))?;
        seeds.push(addr);
    }
    Ok(seeds)
}

async fn receive_loop(cluster: Arc<Cluster>, socket: Arc<UdpSocket>) {
    let mut buffer = [0u8; 4096];
    loop {
        let Ok((len, from)) = socket.recv_from(&mut buffer).await else {
            continue;
        };
        let Ok(message) = serde_json::from_slice::<Announce>(&buffer[..len]) else {
            cluster.record_packet_error();
            continue;
        };
        cluster.on_announce(message, from);
    }
}

async fn manager_loop(cluster: Arc<Cluster>, socket: Arc<UdpSocket>) {
    let mut tick = time::interval(Duration::from_millis(250));
    loop {
        tick.tick().await;
        cluster.tick(&socket).await;
    }
}

async fn client_loop(cluster: Arc<Cluster>) {
    let mut next_attempt = Instant::now();
    loop {
        time::sleep(Duration::from_millis(500)).await;
        if Instant::now() < next_attempt {
            continue;
        }
        let Some((server_id, server_addr)) = cluster.server_endpoint() else {
            continue;
        };
        next_attempt = Instant::now() + cluster.cfg.sync_interval;
        let result = sync_once(&cluster, &server_id, server_addr).await;
        if let Err(err) = result {
            cluster.set_last_error(Some(err.clone()));
            cluster.sink.log("warn", format!("集群校时失败：{err}"));
        }
    }
}

async fn sync_once(
    cluster: &Cluster,
    server_id: &str,
    server_addr: SocketAddr,
) -> Result<(), String> {
    let socket = UdpSocket::bind(("0.0.0.0", 0))
        .await
        .map_err(|err| format!("创建 client UDP 套接字失败：{err}"))?;
    let mut samples = Vec::new();
    for _ in 0..SAMPLES {
        let t1 = ntp::system_epoch_ms();
        let nonce = random_u64();
        let mut request = TimeRequest {
            v: PROTOCOL_VERSION,
            cluster_id: cluster.cfg.cluster_id.clone(),
            node_id: cluster.cfg.node_id.clone(),
            t1_ms: t1,
            nonce,
            mac: String::new(),
        };
        request.sign(&cluster.cfg.secret)?;
        let bytes =
            serde_json::to_vec(&request).map_err(|err| format!("编码时间请求失败：{err}"))?;
        socket
            .send_to(&bytes, server_addr)
            .await
            .map_err(|err| format!("发送时间请求失败：{err}"))?;
        let mut buffer = [0u8; 4096];
        let Ok(Ok((len, from))) =
            timeout(Duration::from_secs(1), socket.recv_from(&mut buffer)).await
        else {
            continue;
        };
        if from.ip() != server_addr.ip() {
            continue;
        }
        let Ok(response) = serde_json::from_slice::<TimeResponse>(&buffer[..len]) else {
            continue;
        };
        let t4 = ntp::system_epoch_ms();
        if response.v != PROTOCOL_VERSION
            || response.cluster_id != cluster.cfg.cluster_id
            || response.server_id != server_id
            || response.t1_ms != t1
            || response.nonce != nonce
            || !response.verified
            || response.ref_age_ms > TIME_QUALITY_LIMIT_MS
            || !response.verify(&cluster.cfg.secret)
        {
            continue;
        }
        if let Some(sample) = offset_and_rtt(t1, response.t2_ms, response.t3_ms, t4) {
            samples.push(sample);
        }
    }
    if samples.is_empty() {
        return Err("没有收到有效时间样本".to_string());
    }
    let mut offsets: Vec<i64> = samples.iter().map(|(offset, _)| *offset).collect();
    offsets.sort_unstable();
    let offset_ms = offsets[offsets.len() / 2];
    let rtt_ms = samples.iter().map(|(_, rtt)| *rtt).min().unwrap_or(0);
    let mut applied = false;
    let mut message = if cluster.cfg.auto_apply {
        "仅记录，未写入".to_string()
    } else {
        "自动写入已关闭，仅检测".to_string()
    };

    if cluster.cfg.auto_apply && offset_ms.abs() > cluster.cfg.max_offset_ms {
        if offset_ms.abs() > cluster.cfg.max_step_ms && !cluster.cfg.allow_large_step {
            message = format!("偏差 {}ms 超过大步长阈值，拒绝写入", offset_ms.abs());
        } else {
            let target = ntp::system_epoch_ms().saturating_add(offset_ms);
            match cluster.sink.apply_time(target) {
                Ok(()) => {
                    applied = true;
                    message = format!("已写入本机时间，offset={offset_ms}ms");
                }
                Err(err) => {
                    message = format!("写入失败：{err}");
                }
            }
        }
    } else if cluster.cfg.auto_apply {
        message = format!("偏差 {offset_ms}ms 在阈值内，不写入");
    }

    let view = SyncView {
        offset_ms,
        rtt_ms,
        at: ntp::format_beijing(ntp::system_epoch_ms()),
        applied,
        message: message.clone(),
    };
    cluster.set_last_sync(view);
    cluster.set_last_error(None);
    cluster.sink.log(
        if applied { "success" } else { "info" },
        format!("集群校时：{message} · rtt={rtt_ms}ms"),
    );
    Ok(())
}

fn offset_and_rtt(t1: i64, t2: i64, t3: i64, t4: i64) -> Option<(i64, u64)> {
    let offset = ((t2 - t1) as i128 + (t3 - t4) as i128) / 2;
    let rtt = (t4 - t1) as i128 - (t3 - t2) as i128;
    if rtt < 0 || offset.abs() > i64::MAX as i128 || rtt > u64::MAX as i128 {
        return None;
    }
    Some((offset as i64, rtt as u64))
}

fn random_delay() -> Duration {
    Duration::from_millis(random_u64() % 2_000)
}

fn random_u64() -> u64 {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 8];
    if rng.fill(&mut bytes).is_ok() {
        u64::from_be_bytes(bytes)
    } else {
        ntp::system_epoch_ms() as u64
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_delay_is_bounded() {
        for _ in 0..100 {
            assert!(random_delay() < Duration::from_secs(2));
        }
    }

    #[test]
    fn offset_and_rtt_are_consistent_for_symmetric_paths() {
        assert_eq!(offset_and_rtt(1_000, 1_010, 1_011, 1_021), Some((0, 20)));
        assert_eq!(offset_and_rtt(1_000, 1_020, 1_021, 1_021), Some((10, 20)));
        assert_eq!(offset_and_rtt(1_000, 1_000, 1_001, 999), None);
    }

    #[test]
    fn same_term_tie_break_uses_lowest_node_id() {
        let mut claims = HashMap::new();
        for (id, term) in [("node-b", 3), ("node-a", 3), ("node-c", 2)] {
            claims.insert(
                id.to_string(),
                Claim {
                    term,
                    node_id: id.to_string(),
                },
            );
        }
        let max_term = claims.values().map(|claim| claim.term).max().unwrap();
        let winner = claims
            .values()
            .filter(|claim| claim.term == max_term)
            .min_by(|a, b| a.node_id.cmp(&b.node_id))
            .unwrap();
        assert_eq!(winner.node_id, "node-a");
    }
}
