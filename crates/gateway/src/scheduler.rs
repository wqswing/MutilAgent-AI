use dashmap::DashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

/// Dual-lane scheduler for controller execution.
/// - Global lane: limits total concurrent executions.
/// - Session lane: serializes executions per session ID.
pub struct ControllerScheduler {
    global: Arc<Semaphore>,
    session_lanes: DashMap<String, Arc<Mutex<()>>>,
}

impl ControllerScheduler {
    pub fn new(global_limit: usize) -> Self {
        Self {
            global: Arc::new(Semaphore::new(global_limit.max(1))),
            session_lanes: DashMap::new(),
        }
    }

    pub async fn run<F, Fut, T>(&self, session_id: Option<&str>, operation: F) -> T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let _global_permit = self.global.acquire().await.expect("semaphore closed");
        if let Some(sid) = session_id.filter(|s| !s.is_empty()) {
            let lane = self
                .session_lanes
                .entry(sid.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone();
            let _lane_guard = lane.lock().await;
            operation().await
        } else {
            operation().await
        }
    }
}

impl Default for ControllerScheduler {
    fn default() -> Self {
        Self::new(32)
    }
}

