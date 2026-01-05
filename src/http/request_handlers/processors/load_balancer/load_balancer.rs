use http::Uri;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::{self, Duration};

use crate::core::running_state_manager;
use crate::logging::syslog::{debug, error};

// Commands sent to a load balancer task
pub enum LoadBalancerCommand {
    GetNextServer { respond_to: oneshot::Sender<Option<String>> },
    Shutdown,
}

// Trait implemented by concrete load balancer algorithms
pub trait LoadBalancerImpl: Send + 'static {
    fn get_next_server(&mut self) -> Option<String>;
    fn check_health(&mut self);
    fn check_uri_health(&self, uri: &str, health_register: Arc<AtomicBool>, request_timeout_secs: u64) {
        let uri_parsed: Result<Uri, _> = uri.parse();
        if uri_parsed.is_err() {
            health_register.store(false, Ordering::SeqCst);
            error(format!("Health check failed: Invalid URI for server '{}'", uri));
            return;
        }
        let server_uri = uri_parsed.unwrap();

        tokio::spawn(async move {
            // Get a client from the running state
            let running_state_manager = running_state_manager::get_running_state_manager().await;
            let running_state = running_state_manager.get_running_state();
            let running_state_read_lock = running_state.read().await;
            let client = running_state_read_lock.get_http_client().get_client(false);

            // Make the request and make sure it times out after X seconds
            let start_time = tokio::time::Instant::now();
            let resp = tokio::time::timeout(Duration::from_secs(request_timeout_secs), client.get(server_uri.clone())).await;
            let elapsed = start_time.elapsed().as_secs_f32();
            let is_healthy = resp.ok().and_then(|r| r.ok()).map(|r| r.status().is_success()).unwrap_or(false);
            debug(format!("Health check for server '{}': {} - Request was done in {:.3} seconds", server_uri, if is_healthy { "Healthy" } else { "Unhealthy" }, elapsed));
            health_register.store(is_healthy, Ordering::SeqCst);
        });
    }
    fn get_health_check_interval_secs(&self) -> u64;
}

// Actor task that owns a single load balancer instance
async fn load_balancer_task<T: LoadBalancerImpl>(mut lb: T, mut rx: mpsc::Receiver<LoadBalancerCommand>) {
    let mut interval = time::interval(Duration::from_secs(lb.get_health_check_interval_secs()));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                lb.check_health();
            }
            Some(cmd) = rx.recv() => {
                match cmd {
                    LoadBalancerCommand::GetNextServer { respond_to } => {
                        let _ = respond_to.send(lb.get_next_server());
                    }
                    LoadBalancerCommand::Shutdown => {
                        break;
                    }
                }
            }
            else => break,
        }
    }
}

// Registry that manages load balancer instances
pub struct LoadBalancerRegistry {
    inner: RwLock<HashMap<String, mpsc::Sender<LoadBalancerCommand>>>,
}

impl LoadBalancerRegistry {
    pub fn new() -> Self {
        Self { inner: RwLock::new(HashMap::new()) }
    }

    pub async fn create<T: LoadBalancerImpl>(&self, id: String, lb: T) {
        let (tx, rx) = mpsc::channel(32);
        tokio::spawn(load_balancer_task(lb, rx));
        self.inner.write().await.insert(id, tx);
    }

    pub async fn get_next_server(&self, id: &str) -> Option<String> {
        let tx = self.inner.read().await.get(id)?.clone();
        let (resp_tx, resp_rx) = oneshot::channel();
        let _ = tx.send(LoadBalancerCommand::GetNextServer { respond_to: resp_tx }).await;
        resp_rx.await.ok().flatten()
    }

    pub async fn remove(&self, id: &str) {
        if let Some(tx) = self.inner.write().await.remove(id) {
            let _ = tx.send(LoadBalancerCommand::Shutdown).await;
        }
    }
}
