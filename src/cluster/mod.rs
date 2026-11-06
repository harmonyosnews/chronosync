mod config;
mod manager;
mod protocol;
mod time_service;

use std::sync::Arc;

pub use config::{Config, Mode, load_secret, resolve_node_id};
pub use manager::{Cluster, ClusterView};

pub trait TimeSource: Send + Sync {
    fn now_epoch_ms(&self) -> i64;
    fn verified(&self) -> bool;
    fn age_ms(&self) -> u64;
}

pub trait EventSink: Send + Sync {
    fn log(&self, level: &str, message: String);
    fn apply_time(&self, epoch_ms: i64) -> Result<(), String>;
}

pub async fn start(
    cfg: Config,
    time_source: Arc<dyn TimeSource>,
    sink: Arc<dyn EventSink>,
) -> Result<Arc<Cluster>, String> {
    manager::start(cfg, time_source, sink).await
}
