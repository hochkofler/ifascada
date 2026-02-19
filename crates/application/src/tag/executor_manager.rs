use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::tag::tag_executor::TagExecutor;
// use domain::driver::DriverType; // Unused
use domain::event::EventPublisher;
use domain::tag::{Tag, TagId};
use infrastructure::DriverFactory;

/// Manages the lifecycle of multiple TagExecutors
pub struct ExecutorManager {
    executors: Arc<Mutex<HashMap<TagId, (JoinHandle<()>, tokio_util::sync::CancellationToken)>>>,
    connected_tags: Arc<Mutex<HashSet<TagId>>>,
    event_publisher: Arc<dyn EventPublisher>,
    #[allow(dead_code)]
    agent_id: String,
}

impl ExecutorManager {
    pub fn new(event_publisher: Arc<dyn EventPublisher>, agent_id: String) -> Self {
        Self {
            executors: Arc::new(Mutex::new(HashMap::new())),
            connected_tags: Arc::new(Mutex::new(HashSet::new())),
            event_publisher,
            agent_id,
        }
    }

    /// Starts executors for the provided tags
    pub async fn start_tags(&self, tags: Vec<Tag>) {
        let mut executors = self.executors.lock().await;

        for tag in tags {
            if !tag.is_enabled() {
                info!(tag_id = %tag.id(), "Skipping disabled tag");
                continue;
            }

            if executors.contains_key(tag.id()) {
                warn!(tag_id = %tag.id(), "Tag executor already running");
                continue;
            }

            let tag_id = tag.id().clone();
            let driver_config = tag.driver_config().clone();
            let driver_type = tag.driver_type();

            // Create driver using factory
            let driver = match DriverFactory::create_driver(driver_type, driver_config) {
                Ok(d) => d,
                Err(e) => {
                    error!(tag_id = %tag_id, error = %e, "Failed to create driver");
                    continue;
                }
            };

            let publisher = self.event_publisher.clone();
            let cancel_token = tokio_util::sync::CancellationToken::new();
            let mut executor = TagExecutor::new(
                tag,
                driver,
                publisher,
                cancel_token.clone(),
                self.connected_tags.clone(),
            );

            let tags_id_clone = tag_id.clone();
            let handle = tokio::spawn(async move {
                info!(tag_id = %tags_id_clone, "Starting tag executor");
                if let Err(e) = executor.execute().await {
                    error!(tag_id = %tags_id_clone, error = %e, "Tag executor failed");
                }
                info!(tag_id = %tags_id_clone, "Tag executor stopped");
            });

            executors.insert(tag_id, (handle, cancel_token));
        }
    }

    /// Stops a specific tag executor
    pub async fn stop_tag(&self, tag_id: &TagId) {
        let mut executors = self.executors.lock().await;
        if let Some((handle, token)) = executors.remove(tag_id) {
            token.cancel();
            // Wait a bit for graceful shutdown or just abort if it takes too long
            // For now, abort to be sure, but token.cancel() already ensures disconnect() runs
            handle.abort();
            info!(tag_id = %tag_id, "Stopped tag executor");
        }
    }

    /// Stops all executors
    pub async fn stop_all(&self) {
        let mut executors = self.executors.lock().await;
        for (tag_id, (handle, token)) in executors.drain() {
            token.cancel();
            handle.abort();
            info!(tag_id = %tag_id, "Aborted tag executor");
        }
    }

    /// Returns the number of active executors
    pub async fn active_count(&self) -> usize {
        self.executors.lock().await.len()
    }

    /// Returns the IDs of all active executors (that are connected)
    pub async fn get_active_tag_ids(&self) -> Vec<String> {
        self.connected_tags
            .lock()
            .await
            .iter()
            .map(|id| id.to_string())
            .collect()
    }
}
