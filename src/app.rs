use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedMutexGuard, Semaphore};
use tokio::task::JoinSet;

use crate::cluster::{self, ClusterView};
use crate::local::{self, LocalEnvironment};
use crate::ntp::{self, TimeReference};
use crate::remote::{self, OptionalStep, RemoteTarget, SshClient, SyncOptions};
use crate::scan::{self, ScanResult};
use crate::settings::Settings;

const BUILD_TARGET: &str = env!("CHRONOSYNC_BUILD_TARGET");
const BUILD_VARIANT: &str = env!("CHRONOSYNC_BUILD_VARIANT");
const BUILD_TIME: &str = env!("CHRONOSYNC_BUILD_TIME");

const MAX_LOGS: usize = 8_000;
const MAX_MACHINES: usize = 64;
const HOST_CONCURRENCY: usize = 8;
const STALE_REFERENCE_LIMIT_MS: u64 = 5 * 60 * 1000;

#[derive(Clone)]
pub struct Config {
    pub ntp_servers: Vec<String>,
    pub allow_system_clock: bool,
    pub log_dir: PathBuf,
    pub auth_header: Option<String>,
    pub settings_path: PathBuf,
}

pub struct App {
    inner: Arc<Inner>,
    cfg: Arc<Config>,
    local: LocalEnvironment,
    cluster: Mutex<Option<Arc<cluster::Cluster>>>,
    settings: Mutex<Settings>,
    instance_id: String,
}

struct Inner {
    logs: Mutex<Vec<LogEntry>>,
    next_log_id: AtomicU64,
    machines: Mutex<HashMap<String, MachineState>>,
    run: Mutex<Option<RunView>>,
    reference: Mutex<Option<TimeReference>>,
    running: AtomicBool,
    scanning: AtomicBool,
    activity: Arc<tokio::sync::Mutex<()>>,
    run_seq: AtomicU64,
    restarting: AtomicBool,
}

#[derive(Clone, Serialize)]
pub struct LogEntry {
    pub id: u64,
    pub ts: String,
    pub level: String,
    pub host: Option<String>,
    pub message: String,
    pub run_id: u64,
}

#[derive(Clone, Serialize)]
pub struct MachineState {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub status: String,
    pub message: String,
    pub run_id: u64,
    pub hostname: Option<String>,
    pub timezone: Option<String>,
    pub remote_local: Option<String>,
    pub before_drift_ms: Option<i64>,
    pub after_drift_ms: Option<i64>,
    pub after_local: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub hwclock: Option<String>,
    pub timezone_action: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct RunView {
    pub id: u64,
    pub kind: String,
    pub status: String,
    pub total: usize,
    pub done: usize,
    pub success: usize,
    pub failed: usize,
    pub dry_run: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
pub struct ReferenceView {
    pub source: String,
    pub rtt_ms: f64,
    pub samples: usize,
    pub verified: bool,
    pub age_ms: u64,
    pub epoch_ms: i64,
    pub beijing: String,
    pub utc: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct StateResponse {
    pub now_epoch_ms: i64,
    pub now_beijing: String,
    pub now_utc: String,
    pub reference: Option<ReferenceView>,
    pub running: bool,
    pub scanning: bool,
    pub sync_ready: bool,
    pub restart_supported: bool,
    pub run: Option<RunView>,
    pub cluster: Option<ClusterView>,
    pub machines: Vec<MachineState>,
    pub logs: Vec<LogEntry>,
    pub log_total: u64,
    pub local: LocalView,
}

#[derive(Clone, Serialize)]
pub struct LocalView {
    pub hostname: String,
    pub privileged: bool,
    pub timezone: String,
}

#[derive(Deserialize)]
pub struct StateQuery {
    #[serde(default)]
    pub since: u64,
}

#[derive(Clone, Deserialize)]
pub struct MachineInput {
    pub id: Option<String>,
    pub name: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub user: String,
    pub password: String,
}

#[derive(Clone, Copy, Deserialize, Default)]
pub struct SyncOptionsInput {
    #[serde(default)]
    pub sync_hwclock: bool,
    #[serde(default)]
    pub sync_timezone: bool,
    #[serde(default)]
    pub dry_run: bool,
}

impl From<SyncOptionsInput> for SyncOptions {
    fn from(value: SyncOptionsInput) -> Self {
        Self {
            sync_hwclock: value.sync_hwclock,
            sync_timezone: value.sync_timezone,
            dry_run: value.dry_run,
        }
    }
}

#[derive(Deserialize)]
pub struct SyncRequest {
    #[serde(default)]
    pub machines: Vec<MachineInput>,
    #[serde(default)]
    pub options: SyncOptionsInput,
}

#[derive(Deserialize, Default)]
pub struct LocalSyncRequest {
    #[serde(default)]
    pub options: SyncOptionsInput,
}

#[derive(Deserialize, Default)]
pub struct ScanRequest {
    #[serde(default)]
    pub cidrs: Vec<String>,
    #[serde(default = "default_true")]
    pub include_non_ssh: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
pub struct StartResponse {
    pub started: bool,
    pub run_id: u64,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: String,
    pub run_id: Option<u64>,
}

type ApiError = (StatusCode, Json<ApiErrorBody>);

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ApiErrorBody {
            error: message.into(),
            run_id: None,
        }),
    )
}

struct RunGuard {
    inner: Arc<Inner>,
    _activity: OwnedMutexGuard<()>,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        self.inner.running.store(false, Ordering::SeqCst);
        let mut run = lock(&self.inner.run);
        if let Some(run) = run.as_mut()
            && run.status == "running"
        {
            run.status = "failed".to_string();
            run.finished_at = Some(ntp::format_beijing(ntp::system_epoch_ms()));
        }
    }
}

struct ScanGuard {
    inner: Arc<Inner>,
    _activity: OwnedMutexGuard<()>,
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.inner.scanning.store(false, Ordering::SeqCst);
    }
}

impl App {
    pub async fn new(cfg: Config, local: LocalEnvironment, settings: Settings) -> Arc<Self> {
        if let Err(err) = std::fs::create_dir_all(&cfg.log_dir) {
            eprintln!("警告：无法创建日志目录 {}：{err}", cfg.log_dir.display());
        }
        let app = Arc::new(Self {
            inner: Arc::new(Inner {
                logs: Mutex::new(Vec::new()),
                next_log_id: AtomicU64::new(1),
                machines: Mutex::new(HashMap::new()),
                run: Mutex::new(None),
                reference: Mutex::new(None),
                running: AtomicBool::new(false),
                scanning: AtomicBool::new(false),
                activity: Arc::new(tokio::sync::Mutex::new(())),
                run_seq: AtomicU64::new(0),
                restarting: AtomicBool::new(false),
            }),
            cfg: Arc::new(cfg),
            local,
            cluster: Mutex::new(None),
            settings: Mutex::new(settings),
            instance_id: instance_id(),
        });

        match ntp::probe(&app.cfg.ntp_servers).await {
            Ok(reference) => {
                app.set_reference(reference.clone());
                app.log("system", None, "ChronoSync 服务已启动，标准时间源同步完成");
                app.log(
                    "success",
                    None,
                    format!(
                        "标准时间源已锁定：{} · RTT {:.0} ms · {} 个源响应 · 北京时间 {}",
                        reference.source,
                        reference.rtt_ms,
                        reference.samples,
                        ntp::format_beijing(reference.now_epoch_ms())
                    ),
                );
            }
            Err(err) => {
                let fallback = TimeReference::from_system_now("本机系统时钟（NTP 未同步）");
                app.set_reference(fallback);
                app.log(
                    "system",
                    None,
                    "ChronoSync 服务已启动，但标准时间源同步失败",
                );
                if app.cfg.allow_system_clock {
                    app.log(
                        "warn",
                        None,
                        format!(
                            "标准时间源初始化失败：{err}。已按启动参数降级为本机系统时钟，校时精度无法验证"
                        ),
                    );
                } else {
                    app.log(
                        "error",
                        None,
                        format!(
                            "标准时间源初始化失败：{err}。为避免写入错误时间，校时功能已锁定；请检查网络后点击“刷新时间源”"
                        ),
                    );
                }
            }
        }
        app
    }

    pub fn router(self: &Arc<Self>) -> Router {
        let auth_app = self.clone();
        let mut router = Router::new()
            .route("/", get(index))
            .route("/favicon.ico", get(favicon))
            .route("/api/state", get(get_state))
            .route("/api/reference/refresh", post(refresh_reference))
            .route("/api/sync", post(sync_remote))
            .route("/api/local-sync", post(sync_local))
            .route("/api/scan", post(scan_lan))
            .route("/api/logs/download", get(download_logs))
            .route("/api/health", get(health))
            .route("/api/cluster/state", get(cluster_state))
            .route("/api/cluster/settings", post(update_cluster_settings))
            .route("/api/system/restart", post(restart_service))
            .layer(DefaultBodyLimit::max(512 * 1024))
            .layer(middleware::from_fn(security_headers))
            .with_state(self.clone());
        if auth_app.cfg.auth_header.is_some() {
            router = router.layer(middleware::from_fn_with_state(auth_app, auth_middleware));
        }
        router
    }

    pub async fn refresh_reference(&self) -> Result<(), String> {
        match ntp::probe(&self.cfg.ntp_servers).await {
            Ok(reference) => {
                self.set_reference(reference);
                Ok(())
            }
            Err(err) => {
                self.log("warn", None, format!("后台 NTP 刷新失败：{err}"));
                Err(err)
            }
        }
    }

    pub fn set_cluster(&self, cluster: Arc<cluster::Cluster>) {
        *lock(&self.cluster) = Some(cluster);
    }

    pub fn cluster_view(&self) -> Option<ClusterView> {
        lock(&self.cluster).as_ref().map(|cluster| cluster.view())
    }

    fn cluster_handle(&self) -> Option<Arc<cluster::Cluster>> {
        lock(&self.cluster).clone()
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings.save(&self.cfg.settings_path)
    }

    pub fn console_summary(&self, address: &str) -> String {
        let reference = self.current_reference();
        let mut lines = vec![
            format!("ChronoSync v{}", env!("CARGO_PKG_VERSION")),
            format!("监听地址: {address}"),
            format!(
                "本机节点: {} · 校时权限: {}",
                self.local.hostname,
                if self.local.privileged {
                    "已获得管理员/root 权限"
                } else {
                    "受限（本机校时需 root/管理员）"
                }
            ),
            format!("构建信息: {BUILD_TARGET}/{BUILD_VARIANT} · {BUILD_TIME}"),
            format!(
                "访问认证: {}",
                if self.cfg.auth_header.is_some() {
                    "已启用 HTTP Basic Auth"
                } else {
                    "未启用"
                }
            ),
            format!("日志目录: {}", self.cfg.log_dir.display()),
        ];
        if let Some(reference) = reference {
            lines.push(format!(
                "标准时间: {} (北京时间) · 来源 {} · RTT {:.0} ms{}",
                ntp::format_beijing(reference.now_epoch_ms()),
                reference.source,
                reference.rtt_ms,
                if reference.verified {
                    ""
                } else {
                    " · 未验证"
                }
            ));
        }
        if !self.cfg.allow_system_clock {
            lines.push("安全策略: NTP 不可达时将拒绝校时（防止写入错误时间）".to_string());
        }
        if let Some(cluster) = self.cluster_view() {
            lines.push(format!(
                "集群: mode={} role={} server={} peers={}",
                cluster.mode,
                cluster.role,
                cluster.server_id.as_deref().unwrap_or("-"),
                cluster.peers
            ));
        }
        lines.join("\n")
    }

    fn log(&self, level: &str, host: Option<&str>, message: impl Into<String>) -> u64 {
        let standard_ms = self
            .current_reference()
            .map(|reference| reference.now_epoch_ms())
            .unwrap_or_else(ntp::system_epoch_ms);
        let run_id = self
            .inner
            .run
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .filter(|run| run.status == "running")
            .map(|run| run.id)
            .unwrap_or(0);
        let id = self.inner.next_log_id.fetch_add(1, Ordering::SeqCst);
        let entry = LogEntry {
            id,
            ts: ntp::format_beijing(standard_ms),
            level: level.to_string(),
            host: host.map(|value| sanitize_text(value, 120)),
            message: sanitize_text(&message.into(), 1_500),
            run_id,
        };
        let mut logs = lock(&self.inner.logs);
        logs.push(entry);
        if logs.len() > MAX_LOGS {
            let excess = logs.len() - MAX_LOGS;
            logs.drain(0..excess);
        }
        id
    }

    fn set_reference(&self, reference: TimeReference) {
        *lock(&self.inner.reference) = Some(reference);
    }

    fn current_reference(&self) -> Option<TimeReference> {
        lock(&self.inner.reference).clone()
    }

    fn now_standard_ms(&self) -> i64 {
        self.current_reference()
            .map(|reference| reference.now_epoch_ms())
            .unwrap_or_else(ntp::system_epoch_ms)
    }

    fn reference_view(&self, reference: &TimeReference) -> ReferenceView {
        let now = reference.now_epoch_ms();
        ReferenceView {
            source: reference.source.clone(),
            rtt_ms: (reference.rtt_ms * 10.0).round() / 10.0,
            samples: reference.samples,
            verified: reference.verified,
            age_ms: reference.age_ms(),
            epoch_ms: now,
            beijing: ntp::format_beijing(now),
            utc: ntp::format_utc(now),
            created_at: ntp::format_beijing(reference.created_at_ms),
        }
    }

    fn sync_ready(&self) -> bool {
        self.cfg.allow_system_clock
            || self
                .current_reference()
                .map(|reference| reference.verified)
                .unwrap_or(false)
    }

    fn try_begin_run(&self) -> Option<RunGuard> {
        let activity = self.inner.activity.clone().try_lock_owned().ok()?;
        if self
            .inner
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return None;
        }
        Some(RunGuard {
            inner: self.inner.clone(),
            _activity: activity,
        })
    }

    fn begin_scan(&self) -> Option<ScanGuard> {
        let activity = self.inner.activity.clone().try_lock_owned().ok()?;
        if self.inner.running.load(Ordering::SeqCst) {
            return None;
        }
        self.inner.scanning.store(true, Ordering::SeqCst);
        Some(ScanGuard {
            inner: self.inner.clone(),
            _activity: activity,
        })
    }

    fn prepare_run(&self, kind: &str, total: usize, dry_run: bool) -> u64 {
        let id = self.inner.run_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let run = RunView {
            id,
            kind: kind.to_string(),
            status: "running".to_string(),
            total,
            done: 0,
            success: 0,
            failed: 0,
            dry_run,
            started_at: ntp::format_beijing(self.now_standard_ms()),
            finished_at: None,
            duration_ms: None,
        };
        *lock(&self.inner.run) = Some(run);
        id
    }

    fn prepare_remote_machines(&self, targets: &[RemoteTarget], run_id: u64, dry_run: bool) {
        let mut machines = lock(&self.inner.machines);
        for target in targets {
            machines.insert(
                target.id.clone(),
                MachineState {
                    id: target.id.clone(),
                    name: target.name.clone(),
                    host: target.host.clone(),
                    port: target.port,
                    status: "pending".to_string(),
                    message: if dry_run {
                        "等待检测".to_string()
                    } else {
                        "等待校时".to_string()
                    },
                    run_id,
                    hostname: None,
                    timezone: None,
                    remote_local: None,
                    before_drift_ms: None,
                    after_drift_ms: None,
                    after_local: None,
                    elapsed_ms: None,
                    hwclock: None,
                    timezone_action: None,
                    warnings: Vec::new(),
                },
            );
        }
    }

    fn update_machine<F>(&self, id: &str, update: F)
    where
        F: FnOnce(&mut MachineState),
    {
        let mut machines = lock(&self.inner.machines);
        if let Some(machine) = machines.get_mut(id) {
            update(machine);
        }
    }

    fn add_run_result(&self, run_id: u64, success: bool) {
        let mut run = lock(&self.inner.run);
        if let Some(run) = run.as_mut()
            && run.id == run_id
            && run.status == "running"
        {
            run.done = run.done.saturating_add(1);
            if success {
                run.success = run.success.saturating_add(1);
            } else {
                run.failed = run.failed.saturating_add(1);
            }
        }
    }

    async fn acquire_reference_for_run(&self) -> Result<TimeReference, String> {
        self.log("info", None, "正在同步标准时间源…");
        match ntp::probe(&self.cfg.ntp_servers).await {
            Ok(reference) => {
                self.set_reference(reference.clone());
                self.log(
                    "success",
                    None,
                    format!(
                        "标准时间源：{} · RTT {:.0} ms · {} 个源响应 · 北京时间 {}",
                        reference.source,
                        reference.rtt_ms,
                        reference.samples,
                        ntp::format_beijing(reference.now_epoch_ms())
                    ),
                );
                Ok(reference)
            }
            Err(err) => {
                if self.cfg.allow_system_clock {
                    let fallback =
                        TimeReference::from_system_now("本机系统时钟（NTP 不可达，授权降级）");
                    self.set_reference(fallback.clone());
                    self.log(
                        "warn",
                        None,
                        format!(
                            "NTP 获取失败：{err}。已按启动参数降级为本机系统时钟，校时精度不可验证"
                        ),
                    );
                    return Ok(fallback);
                }
                if let Some(existing) = self.current_reference().filter(|reference| {
                    reference.verified && reference.age_ms() <= STALE_REFERENCE_LIMIT_MS
                }) {
                    self.log(
                        "warn",
                        None,
                        format!(
                            "NTP 暂时不可达：{err}。继续使用 {} 秒前已验证的标准时间快照",
                            existing.age_ms() / 1000
                        ),
                    );
                    return Ok(existing);
                }
                Err(err)
            }
        }
    }

    async fn run_remote(
        self: Arc<Self>,
        targets: Vec<RemoteTarget>,
        options: SyncOptions,
        run_id: u64,
    ) {
        let started = Instant::now();
        self.log(
            "system",
            None,
            format!(
                "开始远程校时任务 · {} 台设备 · {}",
                targets.len(),
                if options.dry_run {
                    "仅检测，不修改时间"
                } else {
                    "写入系统时间"
                }
            ),
        );

        let reference = match self.acquire_reference_for_run().await {
            Ok(reference) => reference,
            Err(err) => {
                self.log(
                    "error",
                    None,
                    format!("任务终止：无法获取标准时间（{err}）"),
                );
                for target in &targets {
                    self.update_machine(&target.id, |machine| {
                        machine.status = "error".to_string();
                        machine.message = "未执行：标准时间不可用".to_string();
                        machine.elapsed_ms = Some(started.elapsed().as_millis() as u64);
                    });
                    self.add_run_result(run_id, false);
                }
                self.finalize_run(run_id, started).await;
                return;
            }
        };

        let semaphore = Arc::new(Semaphore::new(HOST_CONCURRENCY));
        let mut tasks = JoinSet::new();
        for target in targets {
            let app = self.clone();
            let reference = reference.clone();
            let semaphore = semaphore.clone();
            tasks.spawn(async move {
                let _permit = semaphore.acquire_owned().await;
                app.run_one_target(target, options, run_id, reference).await;
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Err(err) = result {
                self.log("error", None, format!("设备任务异常退出：{err}"));
            }
        }

        self.finalize_run(run_id, started).await;
    }

    async fn run_one_target(
        &self,
        target: RemoteTarget,
        options: SyncOptions,
        run_id: u64,
        reference: TimeReference,
    ) {
        let started = Instant::now();
        let label = target_label(&target);
        self.update_machine(&target.id, |machine| {
            machine.status = "connecting".to_string();
            machine.message = "SSH 连接中…".to_string();
            machine.run_id = run_id;
        });
        self.log(
            "info",
            Some(&label),
            format!("连接 SSH · {}@{}:{}", target.user, target.host, target.port),
        );

        let mut client = match SshClient::connect(&target).await {
            Ok(client) => client,
            Err(err) => {
                self.fail_target(&target, &label, run_id, started, &err, None);
                return;
            }
        };

        let preflight_started = Instant::now();
        let preflight_output = match client.exec(remote::preflight_command()).await {
            Ok(output) => output,
            Err(err) => {
                let message = format!("读取远程时间失败：{err}");
                self.fail_target(&target, &label, run_id, started, &message, None);
                client.close().await;
                return;
            }
        };
        let exec_elapsed_ms = preflight_started.elapsed().as_millis() as i64;
        if let Some(status) = preflight_output.exit_status
            && status != 0
            && !preflight_output.has_marker("EPOCH")
        {
            let message = format!("读取远程时间失败：{}", preflight_output.concise_error());
            self.fail_target(&target, &label, run_id, started, &message, None);
            client.close().await;
            return;
        }
        let preflight = match remote::parse_preflight(&preflight_output) {
            Ok(preflight) => preflight,
            Err(err) => {
                self.fail_target(&target, &label, run_id, started, &err, None);
                client.close().await;
                return;
            }
        };

        let remote_ms = (preflight.epoch * 1000.0).round() as i64;
        let standard_mid_ms = reference.now_epoch_ms() - (exec_elapsed_ms / 2);
        let drift_ms = remote_ms.saturating_sub(standard_mid_ms);
        let drift_text = drift_description(drift_ms);
        self.update_machine(&target.id, |machine| {
            machine.status = "checking".to_string();
            machine.message = format!("已连接 · {}", drift_text);
            machine.hostname = Some(preflight.hostname.clone());
            machine.timezone = Some(preflight.timezone.clone());
            machine.remote_local = Some(preflight.local_time.clone());
            machine.before_drift_ms = Some(drift_ms);
        });
        self.log(
            if drift_ms.abs() > 1_000 {
                "warn"
            } else {
                "info"
            },
            Some(&label),
            format!(
                "{} · 时区 {} · 远程时间 {} · {}",
                preflight.hostname, preflight.timezone, preflight.local_time, drift_text
            ),
        );

        if options.dry_run {
            self.update_machine(&target.id, |machine| {
                machine.status = "checked".to_string();
                machine.message = format!("检测完成 · {drift_text}");
                machine.elapsed_ms = Some(started.elapsed().as_millis() as u64);
            });
            self.add_run_result(run_id, true);
            self.log(
                "success",
                Some(&label),
                format!(
                    "检测完成（未修改时间）· {} · 耗时 {:.2}s",
                    drift_text,
                    started.elapsed().as_secs_f64()
                ),
            );
            client.close().await;
            return;
        }

        self.update_machine(&target.id, |machine| {
            machine.status = "setting".to_string();
            machine.message = "正在写入系统时间…".to_string();
        });
        let target_ms = reference.now_epoch_ms() + 80;
        let target_epoch = ((target_ms as f64) / 1000.0).round() as i64;
        let target_utc = ntp::format_utc(target_epoch.saturating_mul(1000));
        let target_beijing = ntp::format_beijing(target_epoch.saturating_mul(1000));
        self.log(
            "info",
            Some(&label),
            format!("写入系统时间 · 目标 {target_beijing}（北京时间）"),
        );

        let set_command = remote::set_time_command(
            target_epoch,
            &target_utc,
            options.sync_hwclock,
            options.sync_timezone,
        );
        let apply_output = match client.exec(&set_command).await {
            Ok(output) => output,
            Err(err) => {
                let message = format!("写入系统时间失败：{err}");
                self.fail_target_with_drift(
                    &target,
                    &label,
                    run_id,
                    started,
                    &message,
                    Some(drift_ms),
                );
                client.close().await;
                return;
            }
        };
        let apply = match remote::parse_apply(&apply_output) {
            Ok(apply) => apply,
            Err(err) => {
                self.fail_target_with_drift(&target, &label, run_id, started, &err, Some(drift_ms));
                client.close().await;
                return;
            }
        };

        let mut warnings = Vec::new();
        let logger =
            |level: &str, host: Option<&str>, message: String| self.log(level, host, message);
        let hwclock_action = optional_step_label(
            &apply.hardware_clock,
            "硬件时钟",
            options.sync_hwclock,
            &label,
            &logger,
            &mut warnings,
        );
        let timezone_action = optional_step_label(
            &apply.timezone,
            "时区",
            options.sync_timezone,
            &label,
            &logger,
            &mut warnings,
        );

        // Compare the remote clock read after the write against the epoch we
        // actually sent. Comparing against the current NTP time here would
        // misclassify slow SSH links and slow RTC writes as failures.
        let residual_ms = apply
            .now_epoch
            .map(|remote_now| ((remote_now - target_epoch as f64) * 1000.0).round() as i64);
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let after_local = apply.local_time.clone();

        if let Some(residual) = residual_ms {
            if residual.abs() > 2_500 {
                let message = format!(
                    "已执行写入，但写入后校验偏差 {}（目标 epoch {}），可能有其他校时服务在改写系统时间",
                    drift_description(residual),
                    target_epoch
                );
                self.update_machine(&target.id, |machine| {
                    machine.status = "error".to_string();
                    machine.message = message.clone();
                    machine.after_drift_ms = Some(residual);
                    machine.after_local = Some(after_local.clone());
                    machine.elapsed_ms = Some(elapsed_ms);
                    machine.hwclock = Some(hwclock_action.clone());
                    machine.timezone_action = Some(timezone_action.clone());
                    machine.warnings = warnings.clone();
                });
                self.add_run_result(run_id, false);
                self.log("error", Some(&label), message);
                client.close().await;
                return;
            }
        } else {
            let message = "系统时间已写入，但无法读取校验时间".to_string();
            self.update_machine(&target.id, |machine| {
                machine.status = "error".to_string();
                machine.message = message.clone();
                machine.after_local = Some(after_local.clone());
                machine.elapsed_ms = Some(elapsed_ms);
                machine.hwclock = Some(hwclock_action.clone());
                machine.timezone_action = Some(timezone_action.clone());
                machine.warnings = warnings.clone();
            });
            self.add_run_result(run_id, false);
            self.log("error", Some(&label), message);
            client.close().await;
            return;
        }

        let warning_suffix = if warnings.is_empty() {
            String::new()
        } else {
            "（有警告）".to_string()
        };
        self.update_machine(&target.id, |machine| {
            machine.status = "success".to_string();
            machine.message = format!(
                "校准完成 · 原偏差 {}{}",
                drift_description(drift_ms),
                warning_suffix
            );
            machine.after_drift_ms = residual_ms;
            machine.after_local = Some(after_local.clone());
            machine.elapsed_ms = Some(elapsed_ms);
            machine.hwclock = Some(hwclock_action.clone());
            machine.timezone_action = Some(timezone_action.clone());
            machine.warnings = warnings.clone();
        });
        self.add_run_result(run_id, true);
        self.log(
            "success",
            Some(&label),
            format!(
                "校准完成 · 新时间 {} · 原偏差 {} · 耗时 {:.2}s{}",
                after_local,
                drift_description(drift_ms),
                started.elapsed().as_secs_f64(),
                warning_suffix
            ),
        );
        client.close().await;
    }

    async fn run_local(self: Arc<Self>, options: SyncOptions, run_id: u64) {
        let started = Instant::now();
        let label = format!("本机 · {}", self.local.hostname);
        self.log(
            "system",
            None,
            format!(
                "开始本机校时 · {}",
                if options.dry_run {
                    "仅检测，不修改时间"
                } else {
                    "写入系统时间"
                }
            ),
        );
        let reference = match self.acquire_reference_for_run().await {
            Ok(reference) => reference,
            Err(err) => {
                self.log(
                    "error",
                    None,
                    format!("任务终止：无法获取标准时间（{err}）"),
                );
                self.add_run_result(run_id, false);
                self.finalize_run(run_id, started).await;
                return;
            }
        };

        let before_ms = local::current_epoch_ms();
        let drift_ms = before_ms.saturating_sub(reference.now_epoch_ms());
        let timezone = local::read_timezone();
        self.log(
            if drift_ms.abs() > 1_000 {
                "warn"
            } else {
                "info"
            },
            Some(&label),
            format!(
                "当前本机时间 {} · 时区 {} · {}",
                ntp::format_beijing(before_ms),
                timezone,
                drift_description(drift_ms)
            ),
        );

        if options.dry_run {
            self.log(
                "success",
                Some(&label),
                format!(
                    "本机检测完成（未修改时间）· {} · 耗时 {:.2}s",
                    drift_description(drift_ms),
                    started.elapsed().as_secs_f64()
                ),
            );
            self.add_run_result(run_id, true);
            self.finalize_run(run_id, started).await;
            return;
        }

        if !self.local.privileged {
            let message = "本机校时需要 root/管理员权限，请以相应权限启动 ChronoSync";
            self.log("error", Some(&label), message);
            self.add_run_result(run_id, false);
            self.finalize_run(run_id, started).await;
            return;
        }

        let target_ms = reference.now_epoch_ms() + 2;
        self.log(
            "info",
            Some(&label),
            format!(
                "写入本机系统时间 · 目标 {}（北京时间）",
                ntp::format_beijing(target_ms)
            ),
        );
        if let Err(err) = local::set_system_time(target_ms) {
            self.log("error", Some(&label), format!("本机校时失败：{err}"));
            self.add_run_result(run_id, false);
            self.finalize_run(run_id, started).await;
            return;
        }

        let after_ms = local::current_epoch_ms();
        let residual_ms = after_ms.saturating_sub(reference.now_epoch_ms());
        let mut warnings = Vec::new();
        let logger =
            |level: &str, host: Option<&str>, message: String| self.log(level, host, message);
        let hwclock_action = if options.sync_hwclock {
            let step = local::write_hwclock().await;
            optional_step_label_local(&step, "硬件时钟", &label, &logger, &mut warnings)
        } else {
            "跳过".to_string()
        };
        let timezone_action = if options.sync_timezone {
            let step = local::ensure_timezone().await;
            optional_step_label_local(&step, "时区", &label, &logger, &mut warnings)
        } else {
            "跳过".to_string()
        };

        if residual_ms.abs() > 1_500 {
            let message = format!(
                "已写入本机时间，但校验偏差 {}，可能有其他校时服务在改写系统时钟",
                drift_description(residual_ms)
            );
            self.log("error", Some(&label), message);
            self.add_run_result(run_id, false);
            self.finalize_run(run_id, started).await;
            return;
        }

        let warning_suffix = if warnings.is_empty() {
            String::new()
        } else {
            "（有警告）".to_string()
        };
        self.log(
            "success",
            Some(&label),
            format!(
                "本机校准完成 · 新时间 {} · 校验偏差 {} · 硬件时钟 {} · 时区 {} · 耗时 {:.2}s{}",
                ntp::format_beijing(after_ms),
                drift_description(residual_ms),
                hwclock_action,
                timezone_action,
                started.elapsed().as_secs_f64(),
                warning_suffix
            ),
        );
        self.add_run_result(run_id, true);
        self.finalize_run(run_id, started).await;
    }

    fn fail_target(
        &self,
        target: &RemoteTarget,
        label: &str,
        run_id: u64,
        started: Instant,
        message: &str,
        before_drift_ms: Option<i64>,
    ) {
        self.fail_target_with_drift(target, label, run_id, started, message, before_drift_ms);
    }

    fn fail_target_with_drift(
        &self,
        target: &RemoteTarget,
        label: &str,
        run_id: u64,
        started: Instant,
        message: &str,
        before_drift_ms: Option<i64>,
    ) {
        self.update_machine(&target.id, |machine| {
            machine.status = "error".to_string();
            machine.message = message.to_string();
            machine.elapsed_ms = Some(started.elapsed().as_millis() as u64);
            if machine.before_drift_ms.is_none() {
                machine.before_drift_ms = before_drift_ms;
            }
        });
        self.add_run_result(run_id, false);
        self.log("error", Some(label), message.to_string());
    }

    fn finalize_unfinished_machines(&self, run_id: u64) {
        let unfinished: Vec<(String, String)> = {
            let machines = lock(&self.inner.machines);
            machines
                .values()
                .filter(|machine| {
                    machine.run_id == run_id
                        && !matches!(machine.status.as_str(), "success" | "checked" | "error")
                })
                .map(|machine| (machine.id.clone(), machine_label(machine)))
                .collect()
        };
        for (id, label) in unfinished {
            self.update_machine(&id, |machine| {
                machine.status = "error".to_string();
                machine.message = "任务中断，结果未知".to_string();
            });
            self.add_run_result(run_id, false);
            self.log("error", Some(&label), "任务中断，未获得校时结果");
        }
    }

    async fn finalize_run(&self, run_id: u64, started: Instant) {
        self.finalize_unfinished_machines(run_id);
        let duration = started.elapsed();
        let (done, total, success, failed, dry_run) = {
            let mut run = lock(&self.inner.run);
            match run.as_mut() {
                Some(run) if run.id == run_id => {
                    run.done = run.success.saturating_add(run.failed);
                    (run.done, run.total, run.success, run.failed, run.dry_run)
                }
                _ => (0, 0, 0, 0, false),
            }
        };
        let status = if success == 0 && failed > 0 {
            "failed"
        } else {
            "completed"
        };
        let summary_level = if failed > 0 {
            if success == 0 { "error" } else { "warn" }
        } else {
            "success"
        };
        let summary = format!(
            "任务结束 · 总计 {} 台 · 成功 {} · 失败 {} · 耗时 {:.2}s{}",
            total,
            success,
            failed,
            duration.as_secs_f64(),
            if dry_run { " · 仅检测模式" } else { "" }
        );
        self.log(summary_level, None, summary.clone());
        {
            let mut run = lock(&self.inner.run);
            if let Some(run) = run.as_mut()
                && run.id == run_id
            {
                run.done = done;
                run.success = success;
                run.failed = failed;
                run.status = status.to_string();
                run.duration_ms = Some(duration.as_millis() as u64);
                run.finished_at = Some(ntp::format_beijing(self.now_standard_ms()));
            }
        }
        self.persist_run_log(run_id).await;
    }

    async fn persist_run_log(&self, run_id: u64) {
        let logs: Vec<LogEntry> = lock(&self.inner.logs)
            .iter()
            .filter(|entry| entry.run_id == run_id)
            .cloned()
            .collect();
        if logs.is_empty() {
            return;
        }
        let now_ms = self.now_standard_ms();
        let filename = format!(
            "chronosync-{}-run{}.log",
            ntp::beijing_datetime(now_ms).format("%Y%m%d-%H%M%S"),
            run_id
        );
        let path = self.cfg.log_dir.join(&filename);
        let document = render_log_document(&logs, now_ms);
        if let Err(err) = tokio::fs::create_dir_all(&self.cfg.log_dir).await {
            self.log(
                "warn",
                None,
                format!("无法创建日志目录 {}：{err}", self.cfg.log_dir.display()),
            );
            return;
        }
        if let Err(err) = tokio::fs::write(&path, document).await {
            self.log(
                "warn",
                None,
                format!("日志文件保存失败 {}：{err}", path.display()),
            );
        } else {
            self.log(
                "system",
                None,
                format!("运行日志已保存：{}", path.display()),
            );
        }
    }

    fn snapshot(&self, since: u64) -> StateResponse {
        let reference = self.current_reference();
        let now_ms = reference
            .as_ref()
            .map(|value| value.now_epoch_ms())
            .unwrap_or_else(ntp::system_epoch_ms);
        let logs: Vec<LogEntry> = lock(&self.inner.logs)
            .iter()
            .filter(|entry| entry.id > since)
            .cloned()
            .collect();
        let log_total = self
            .inner
            .next_log_id
            .load(Ordering::SeqCst)
            .saturating_sub(1);
        let mut machines: Vec<MachineState> =
            lock(&self.inner.machines).values().cloned().collect();
        machines.sort_by(|a, b| {
            a.host
                .cmp(&b.host)
                .then_with(|| a.port.cmp(&b.port))
                .then_with(|| a.id.cmp(&b.id))
        });
        StateResponse {
            now_epoch_ms: now_ms,
            now_beijing: ntp::format_beijing(now_ms),
            now_utc: ntp::format_utc(now_ms),
            reference: reference.as_ref().map(|value| self.reference_view(value)),
            running: self.inner.running.load(Ordering::SeqCst),
            scanning: self.inner.scanning.load(Ordering::SeqCst),
            sync_ready: self.sync_ready(),
            restart_supported: cfg!(unix),
            run: lock(&self.inner.run).clone(),
            cluster: self.cluster_view(),
            machines,
            logs,
            log_total,
            local: LocalView {
                hostname: self.local.hostname.clone(),
                privileged: self.local.privileged,
                timezone: self.local.timezone.clone(),
            },
        }
    }
}

async fn index() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Html(include_str!("../web/index.html")),
    )
}

async fn favicon() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

async fn cluster_state(State(app): State<Arc<App>>) -> Json<Option<ClusterView>> {
    Json(app.cluster_view())
}

#[derive(Deserialize)]
pub struct ClusterSettingsRequest {
    #[serde(default)]
    pub ntp123: Option<bool>,
    #[serde(default)]
    pub manual_server: Option<bool>,
    #[serde(default)]
    pub auto_apply: Option<bool>,
}

#[derive(Serialize)]
pub struct RestartResponse {
    pub ok: bool,
    pub message: String,
}

async fn update_cluster_settings(
    State(app): State<Arc<App>>,
    Json(request): Json<ClusterSettingsRequest>,
) -> Result<Json<ClusterView>, ApiError> {
    if request.ntp123.is_none() && request.manual_server.is_none() && request.auto_apply.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "至少需要提供 ntp123、manual_server 或 auto_apply",
        ));
    }
    let Some(cluster) = app.cluster_handle() else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "集群模式未启用，无法修改集群设置",
        ));
    };
    if request.manual_server.is_some()
        && (app.inner.running.load(Ordering::SeqCst) || app.inner.scanning.load(Ordering::SeqCst))
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "校时或扫描任务执行中，不能切换 Server 角色",
        ));
    }

    let old_settings = lock(&app.settings).clone();
    let old_view = cluster.view();
    let mut next = old_settings.clone();
    if let Some(enabled) = request.manual_server {
        if let Err(err) = cluster.set_manual_server(enabled) {
            return Err(api_error(StatusCode::CONFLICT, err));
        }
        next.manual_server = Some(enabled);
    }
    if let Some(enabled) = request.auto_apply {
        if let Err(err) = cluster.set_auto_apply(enabled) {
            return Err(api_error(StatusCode::CONFLICT, err));
        }
        next.auto_apply = Some(enabled);
    }
    if let Some(enabled) = request.ntp123 {
        if let Err(err) = cluster.set_ntp123(enabled).await {
            if request.manual_server.is_some() {
                let _ = cluster.set_manual_server(old_view.manual_server);
            }
            if request.auto_apply.is_some() {
                let _ = cluster.set_auto_apply(old_view.auto_apply);
            }
            return Err(api_error(StatusCode::CONFLICT, err));
        }
        next.ntp123 = Some(enabled);
    }
    if let Err(err) = app.save_settings(&next) {
        if request.manual_server.is_some() {
            let _ = cluster.set_manual_server(old_view.manual_server);
        }
        if request.auto_apply.is_some() {
            let _ = cluster.set_auto_apply(old_view.auto_apply);
        }
        if request.ntp123.is_some() {
            let _ = cluster.set_ntp123(old_view.ntp123).await;
        }
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("保存运行设置失败：{err}"),
        ));
    }
    *lock(&app.settings) = next;
    app.log(
        "system",
        None,
        format!(
            "WebUI 集群设置已更新 · manual_server={} · auto_apply={} · ntp123={}",
            cluster.view().manual_server,
            cluster.view().auto_apply,
            cluster.view().ntp123
        ),
    );
    Ok(Json(cluster.view()))
}

async fn restart_service(
    State(app): State<Arc<App>>,
) -> Result<(StatusCode, Json<RestartResponse>), ApiError> {
    if !cfg!(unix) {
        return Err(api_error(
            StatusCode::NOT_IMPLEMENTED,
            "当前平台不支持 WebUI 内重启，请使用 start/stop 脚本或服务管理器",
        ));
    }
    if app.inner.running.load(Ordering::SeqCst) || app.inner.scanning.load(Ordering::SeqCst) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "校时或扫描任务执行中，不能重启程序",
        ));
    }
    if app
        .inner
        .restarting
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(api_error(StatusCode::CONFLICT, "重启已在进行中"));
    }
    app.log("warn", None, "收到 WebUI 重启请求，服务即将重启");
    let app_clone = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(450)).await;
        if let Err(err) = restart_current_process() {
            app_clone.inner.restarting.store(false, Ordering::SeqCst);
            app_clone.log("error", None, format!("重启失败：{err}"));
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(RestartResponse {
            ok: true,
            message: "服务正在重启，页面将自动重新连接".to_string(),
        }),
    ))
}

async fn health(State(app): State<Arc<App>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "instance_id": app.instance_id,
        "version": env!("CARGO_PKG_VERSION"),
        "build": {
            "target": BUILD_TARGET,
            "variant": BUILD_VARIANT,
            "time": BUILD_TIME,
        },
        "running": app.inner.running.load(Ordering::SeqCst),
        "sync_ready": app.sync_ready(),
        "now_beijing": ntp::format_beijing(app.now_standard_ms()),
    }))
}

async fn get_state(
    State(app): State<Arc<App>>,
    Query(query): Query<StateQuery>,
) -> Json<StateResponse> {
    Json(app.snapshot(query.since))
}

async fn refresh_reference(State(app): State<Arc<App>>) -> Result<Json<ReferenceView>, ApiError> {
    if app.inner.running.load(Ordering::SeqCst) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "正在执行校时任务，无法刷新时间源",
        ));
    }
    app.log("info", None, "正在刷新标准时间源…");
    match ntp::probe(&app.cfg.ntp_servers).await {
        Ok(reference) => {
            let view = app.reference_view(&reference);
            app.log(
                "success",
                None,
                format!(
                    "标准时间源已刷新：{} · RTT {:.0} ms · 北京时间 {}",
                    reference.source,
                    reference.rtt_ms,
                    ntp::format_beijing(reference.now_epoch_ms())
                ),
            );
            app.set_reference(reference);
            Ok(Json(view))
        }
        Err(err) => {
            if app.current_reference().is_none() {
                app.set_reference(TimeReference::from_system_now("本机系统时钟（NTP 未同步）"));
            }
            app.log("error", None, format!("刷新标准时间源失败：{err}"));
            Err(api_error(StatusCode::BAD_GATEWAY, err))
        }
    }
}

async fn sync_remote(
    State(app): State<Arc<App>>,
    Json(request): Json<SyncRequest>,
) -> Result<Json<StartResponse>, ApiError> {
    let targets = validate_machines(request.machines)?;
    let options: SyncOptions = request.options.into();
    let Some(guard) = app.try_begin_run() else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "已有校时任务正在执行，请等待完成",
        ));
    };
    let run_id = app.prepare_run("remote", targets.len(), options.dry_run);
    app.prepare_remote_machines(&targets, run_id, options.dry_run);
    let app_clone = app.clone();
    tokio::spawn(async move {
        let _guard = guard;
        app_clone.run_remote(targets, options, run_id).await;
    });
    Ok(Json(StartResponse {
        started: true,
        run_id,
    }))
}

async fn sync_local(
    State(app): State<Arc<App>>,
    Json(request): Json<LocalSyncRequest>,
) -> Result<Json<StartResponse>, ApiError> {
    let options: SyncOptions = request.options.into();
    let Some(guard) = app.try_begin_run() else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "已有校时任务正在执行，请等待完成",
        ));
    };
    let run_id = app.prepare_run("local", 1, options.dry_run);
    let app_clone = app.clone();
    tokio::spawn(async move {
        let _guard = guard;
        app_clone.run_local(options, run_id).await;
    });
    Ok(Json(StartResponse {
        started: true,
        run_id,
    }))
}

async fn scan_lan(
    State(app): State<Arc<App>>,
    Json(request): Json<ScanRequest>,
) -> Result<Json<ScanResult>, ApiError> {
    let Some(_guard) = app.begin_scan() else {
        return Err(api_error(
            StatusCode::CONFLICT,
            "已有校时任务或扫描任务正在执行，请稍后重试",
        ));
    };
    app.log("system", None, "开始扫描局域网…");
    let plan = match scan::plan(&request.cidrs).await {
        Ok(plan) => plan,
        Err(err) => {
            app.log("error", None, format!("局域网扫描无法开始：{err}"));
            return Err(api_error(StatusCode::BAD_REQUEST, err));
        }
    };
    app.log(
        "info",
        None,
        format!(
            "扫描范围：{} · 探测 TCP 22 与局域网邻居表 · {} 个地址",
            plan.ranges.join(", "),
            plan.addresses_len()
        ),
    );
    let include_non_ssh = request.include_non_ssh;
    let result = scan::execute(plan, include_non_ssh).await;
    if result.active_count == 0 {
        app.log(
            "warn",
            None,
            format!(
                "扫描完成：未发现活跃主机（耗时 {:.1}s）",
                result.duration_ms as f64 / 1000.0
            ),
        );
    } else {
        app.log(
            "success",
            None,
            format!(
                "扫描完成：活跃 {} 台 · 开放 SSH {} 台 · 筛选后 {} 台 · 耗时 {:.1}s",
                result.active_count,
                result.ssh_count,
                result.hosts.len(),
                result.duration_ms as f64 / 1000.0
            ),
        );
    }
    Ok(Json(result))
}

async fn download_logs(State(app): State<Arc<App>>) -> Response {
    let logs: Vec<LogEntry> = lock(&app.inner.logs).clone();
    let now_ms = app.now_standard_ms();
    let document = render_log_document(&logs, now_ms);
    let filename = format!(
        "chronosync-logs-{}.log",
        ntp::beijing_datetime(now_ms).format("%Y%m%d-%H%M%S")
    );
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{filename}\""),
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        document,
    )
        .into_response()
}

async fn auth_middleware(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let expected = app.cfg.auth_header.as_deref();
    if let Some(expected) = expected {
        let provided = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());
        if provided != Some(expected) {
            let mut response = StatusCode::UNAUTHORIZED.into_response();
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"ChronoSync\", charset=\"UTF-8\""),
            );
            return response;
        }
    }
    next.run(request).await
}

async fn security_headers(request: Request, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'",
        ),
    );
    if is_api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    response
}

fn validate_machines(inputs: Vec<MachineInput>) -> Result<Vec<RemoteTarget>, ApiError> {
    let mut targets = Vec::new();
    let mut ids = HashSet::new();
    let mut endpoints = HashSet::new();
    for input in inputs {
        let host_raw = input.host.trim();
        let user_raw = input.user.trim();
        let password_empty = input.password.is_empty();
        if host_raw.is_empty() && user_raw.is_empty() && password_empty {
            continue;
        }
        if host_raw.is_empty() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "存在未填写 IP / 主机的设备",
            ));
        }
        let host = normalize_host(host_raw)?;
        if host.is_empty() {
            return Err(api_error(StatusCode::BAD_REQUEST, "设备 IP / 主机不能为空"));
        }
        let port = input.port.unwrap_or(22);
        if port == 0 {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{host} 的 SSH 端口不能为 0"),
            ));
        }
        if user_raw.is_empty() {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{host} 缺少 SSH 用户名"),
            ));
        }
        if user_raw
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{host} 的 SSH 用户名包含空白或控制字符"),
            ));
        }
        if user_raw.chars().count() > 128 {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{host} 的 SSH 用户名过长"),
            ));
        }
        if password_empty {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{host} 缺少 root / SSH 密码"),
            ));
        }
        if input.password.len() > 1_024 {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("{host} 的密码过长"),
            ));
        }
        let endpoint_key = format!("{}:{port}", host.to_ascii_lowercase());
        if !endpoints.insert(endpoint_key) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("设备 {host}:{port} 重复，请删除重复行"),
            ));
        }
        let id = match input.id {
            Some(id) if !id.trim().is_empty() => {
                let id = id.trim().to_string();
                if id.len() > 80
                    || !id
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        format!("{host} 的设备标识无效"),
                    ));
                }
                id
            }
            _ => format!("m-{}", targets.len() + 1),
        };
        if !ids.insert(id.clone()) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "设备标识重复，请刷新页面后重试",
            ));
        }
        let name = sanitize_label(input.name.as_deref().unwrap_or(""), &host);
        targets.push(RemoteTarget {
            id,
            name,
            host,
            port,
            user: user_raw.to_string(),
            password: input.password,
        });
        if targets.len() > MAX_MACHINES {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("单次最多支持 {MAX_MACHINES} 台设备"),
            ));
        }
    }
    if targets.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "请至少填写一台要校时的设备",
        ));
    }
    Ok(targets)
}

fn normalize_host(input: &str) -> Result<String, ApiError> {
    let mut host = input.trim().to_string();
    if host.starts_with('[') && host.ends_with(']') {
        host = host[1..host.len() - 1].to_string();
    }
    if host.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "设备 IP / 主机不能为空"));
    }
    if host.chars().count() > 253 {
        return Err(api_error(StatusCode::BAD_REQUEST, "设备 IP / 主机名过长"));
    }
    if host
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control() || matches!(ch, '/' | '\\' | '@'))
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("设备地址“{host}”无效，请只填写 IP 或主机名，不要带 root@ 或 ssh://"),
        ));
    }
    Ok(host)
}

fn sanitize_label(input: &str, fallback: &str) -> String {
    let label = input.trim().replace(char::is_control, "");
    if label.is_empty() {
        fallback.to_string()
    } else {
        label.chars().take(48).collect()
    }
}

fn target_label(target: &RemoteTarget) -> String {
    if target.name == target.host {
        target.host.clone()
    } else {
        format!("{} ({})", target.name, target.host)
    }
}

fn machine_label(machine: &MachineState) -> String {
    if machine.name == machine.host {
        machine.host.clone()
    } else {
        format!("{} ({})", machine.name, machine.host)
    }
}

fn optional_step_label<F>(
    step: &OptionalStep,
    title: &str,
    enabled: bool,
    label: &str,
    log: &F,
    warnings: &mut Vec<String>,
) -> String
where
    F: Fn(&str, Option<&str>, String) -> u64,
{
    if !enabled {
        return "跳过".to_string();
    }
    match step {
        OptionalStep::Ok => {
            log("success", Some(label), format!("{title}已同步"));
            "成功".to_string()
        }
        OptionalStep::OkDirectIsa => {
            log(
                "success",
                Some(label),
                format!("{title}已通过 direct ISA 同步"),
            );
            "成功（direct ISA）".to_string()
        }
        OptionalStep::Already => {
            log(
                "info",
                Some(label),
                format!("{title}已是 Asia/Shanghai，无需修改"),
            );
            "已就绪".to_string()
        }
        OptionalStep::NoRtc => {
            log(
                "info",
                Some(label),
                format!("{title}：未检测到可用 RTC，跳过（系统时间已校准）"),
            );
            "无 RTC".to_string()
        }
        OptionalStep::Skipped => "跳过".to_string(),
        OptionalStep::Missing => {
            let warning = format!("{title}工具不可用或缺少时区数据，已跳过");
            log("warn", Some(label), warning.clone());
            warnings.push(warning);
            "不可用".to_string()
        }
        OptionalStep::Failed(err) => {
            let warning = format!("{title}设置失败：{err}");
            log("warn", Some(label), warning.clone());
            warnings.push(warning);
            "失败".to_string()
        }
    }
}

fn optional_step_label_local<F>(
    step: &OptionalStep,
    title: &str,
    label: &str,
    log: &F,
    warnings: &mut Vec<String>,
) -> String
where
    F: Fn(&str, Option<&str>, String) -> u64,
{
    match step {
        OptionalStep::Ok => {
            log("success", Some(label), format!("{title}已同步"));
            "成功".to_string()
        }
        OptionalStep::OkDirectIsa => {
            log(
                "success",
                Some(label),
                format!("{title}已通过 direct ISA 同步"),
            );
            "成功（direct ISA）".to_string()
        }
        OptionalStep::Already => {
            log(
                "info",
                Some(label),
                format!("{title}已是 Asia/Shanghai，无需修改"),
            );
            "已就绪".to_string()
        }
        OptionalStep::NoRtc => {
            log(
                "info",
                Some(label),
                format!("{title}：未检测到可用 RTC，跳过（系统时间已校准）"),
            );
            "无 RTC".to_string()
        }
        OptionalStep::Skipped => "跳过".to_string(),
        OptionalStep::Missing => {
            let warning = format!("{title}工具不可用，已跳过");
            log("warn", Some(label), warning.clone());
            warnings.push(warning);
            "不可用".to_string()
        }
        OptionalStep::Failed(err) => {
            let warning = format!("{title}设置失败：{err}");
            log("warn", Some(label), warning.clone());
            warnings.push(warning);
            "失败".to_string()
        }
    }
}

fn drift_description(delta_ms: i64) -> String {
    if delta_ms == 0 {
        return "与标准时间一致".to_string();
    }
    let direction = if delta_ms > 0 { "快" } else { "慢" };
    let abs = delta_ms.abs();
    if abs < 1_000 {
        format!("{direction} {} 毫秒", abs)
    } else if abs < 60_000 {
        format!("{direction} {:.1} 秒", abs as f64 / 1000.0)
    } else if abs < 3_600_000 {
        format!("{direction} {:.1} 分钟", abs as f64 / 60_000.0)
    } else if abs < 86_400_000 {
        format!("{direction} {:.1} 小时", abs as f64 / 3_600_000.0)
    } else {
        format!("{direction} {:.1} 天", abs as f64 / 86_400_000.0)
    }
}

fn render_log_document(logs: &[LogEntry], now_ms: i64) -> String {
    let reference_source = logs
        .iter()
        .rev()
        .find(|entry| entry.message.contains("标准时间源") && entry.message.contains("RTT"))
        .map(|entry| entry.message.clone())
        .unwrap_or_default();
    let mut output = String::new();
    output.push_str("ChronoSync 校时日志\n");
    output.push_str("============================================================\n");
    output.push_str(&format!(
        "导出时间: {} (北京时间) / {} (UTC)\n",
        ntp::format_beijing(now_ms),
        ntp::format_utc(now_ms)
    ));
    if !reference_source.is_empty() {
        output.push_str(&format!("时间源  : {reference_source}\n"));
    }
    output.push_str("============================================================\n\n");
    if logs.is_empty() {
        output.push_str("(暂无日志)\n");
        return output;
    }
    for entry in logs {
        output.push_str(&format!(
            "[{}] {:<7} {}{}\n",
            entry.ts,
            entry.level.to_ascii_uppercase(),
            entry
                .host
                .as_ref()
                .map(|host| format!("[{host}] "))
                .unwrap_or_default(),
            entry.message
        ));
    }
    output
}

fn sanitize_text(input: &str, max_chars: usize) -> String {
    let mut output = String::with_capacity(input.len().min(max_chars));
    let mut escape = false;
    for ch in input.chars() {
        if escape {
            if ch.is_ascii_alphabetic() || ch == 'm' {
                escape = false;
            }
            continue;
        }
        if ch == '\u{1b}' {
            escape = true;
            continue;
        }
        let replaced = if ch.is_control() { ' ' } else { ch };
        output.push(replaced);
        if output.chars().count() >= max_chars {
            output.push('…');
            break;
        }
    }
    output.trim().to_string()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn instance_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

#[cfg(unix)]
fn restart_current_process() -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let exe = restart_executable_path(
        std::env::current_exe().map_err(|err| format!("无法定位当前可执行文件：{err}"))?,
    );
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let err = std::process::Command::new(&exe).args(args).exec();
    Err(format!("执行 {} 失败：{err}", exe.display()))
}

#[cfg(unix)]
fn restart_executable_path(raw: std::path::PathBuf) -> std::path::PathBuf {
    // Linux /proc/self/exe reports " (deleted)" after an atomic binary upgrade.
    // Exec the replacement at the same path so WebUI restart also works then.
    #[cfg(target_os = "linux")]
    {
        let text = raw.to_string_lossy();
        if let Some(path) = text.strip_suffix(" (deleted)") {
            return std::path::PathBuf::from(path);
        }
    }
    raw
}

#[cfg(not(unix))]
fn restart_current_process() -> Result<(), String> {
    Err("当前平台不支持 WebUI 内重启".to_string())
}

impl cluster::TimeSource for App {
    fn now_epoch_ms(&self) -> i64 {
        self.now_standard_ms()
    }

    fn verified(&self) -> bool {
        self.current_reference()
            .map(|reference| reference.verified)
            .unwrap_or(false)
    }

    fn age_ms(&self) -> u64 {
        self.current_reference()
            .map(|reference| reference.age_ms())
            .unwrap_or(u64::MAX)
    }
}

impl cluster::EventSink for App {
    fn log(&self, level: &str, message: String) {
        App::log(self, level, None, message);
    }

    fn apply_time(&self, epoch_ms: i64) -> Result<(), String> {
        local::set_system_time(epoch_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_text_covers_units() {
        assert_eq!(drift_description(0), "与标准时间一致");
        assert_eq!(drift_description(-500), "慢 500 毫秒");
        assert_eq!(drift_description(1_500), "快 1.5 秒");
        assert_eq!(drift_description(-90_000), "慢 1.5 分钟");
        assert_eq!(drift_description(3_600_000), "快 1.0 小时");
    }

    #[test]
    fn validation_rejects_duplicates_and_empty() {
        let inputs = vec![
            MachineInput {
                id: Some("a".to_string()),
                name: None,
                host: "192.0.2.1".to_string(),
                port: Some(22),
                user: "root".to_string(),
                password: "x".to_string(),
            },
            MachineInput {
                id: Some("b".to_string()),
                name: None,
                host: "192.0.2.1".to_string(),
                port: Some(22),
                user: "root".to_string(),
                password: "x".to_string(),
            },
        ];
        assert!(validate_machines(inputs).is_err());
        assert!(validate_machines(Vec::new()).is_err());
    }

    #[test]
    fn validation_accepts_ipv6_brackets() {
        let inputs = vec![MachineInput {
            id: Some("v6".to_string()),
            name: Some("IPv6".to_string()),
            host: "[::1]".to_string(),
            port: Some(2222),
            user: "root".to_string(),
            password: "secret".to_string(),
        }];
        let targets = validate_machines(inputs).unwrap();
        assert_eq!(targets[0].host, "::1");
        assert_eq!(targets[0].port, 2222);
    }

    #[test]
    fn sanitize_strips_ansi_and_controls() {
        assert_eq!(sanitize_text("a\u{1b}[31mred\u{1b}[0m\nb", 100), "ared b");
        assert_eq!(sanitize_text("0123456789", 4), "0123…");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn restart_path_strips_deleted_suffix() {
        let path = restart_executable_path(std::path::PathBuf::from(
            "/opt/chronosync/chronosync (deleted)",
        ));
        assert_eq!(path, std::path::PathBuf::from("/opt/chronosync/chronosync"));
        let normal = std::path::PathBuf::from("/opt/chronosync/chronosync");
        assert_eq!(restart_executable_path(normal.clone()), normal);
    }

    #[test]
    fn renders_log_download() {
        let logs = vec![
            LogEntry {
                id: 1,
                ts: "2026-11-04 18:00:00".to_string(),
                level: "system".to_string(),
                host: None,
                message: "ChronoSync 服务已启动，标准时间源同步完成".to_string(),
                run_id: 1,
            },
            LogEntry {
                id: 2,
                ts: "2026-11-04 18:00:01".to_string(),
                level: "success".to_string(),
                host: None,
                message: "标准时间源：ntp.aliyun.com · RTT 32 ms · 4 个源响应".to_string(),
                run_id: 1,
            },
            LogEntry {
                id: 3,
                ts: "2026-11-04 18:00:02".to_string(),
                level: "info".to_string(),
                host: Some("test (192.0.2.1)".to_string()),
                message: "hello".to_string(),
                run_id: 1,
            },
        ];
        let rendered = render_log_document(&logs, 1_793_786_400_000);
        assert!(rendered.contains("[test (192.0.2.1)] hello"));
        assert!(rendered.contains("导出时间"));
        assert!(rendered.contains("标准时间源：ntp.aliyun.com · RTT 32 ms"));
        assert!(!rendered.contains("时间源  : ChronoSync 服务已启动"));
    }
}
