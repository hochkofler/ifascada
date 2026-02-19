use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use domain::device::Device;
use domain::event::EventPublisher;
use domain::tag::Tag;
use infrastructure::DriverFactory;

use crate::device::DeviceActor;

/// Manages the lifecycle of DeviceActors
pub struct DeviceManager {
    // Map device_id -> (JoinHandle, CancelToken?)
    // Map device_id -> (JoinHandle, CancelToken?)
    // For now simplistic: just handles
    actors: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    // Map device_id -> List of Tag IDs running on that device
    active_tags: Arc<Mutex<HashMap<String, Vec<String>>>>,
    event_publisher: Arc<dyn EventPublisher>,
}

impl DeviceManager {
    pub fn new(event_publisher: Arc<dyn EventPublisher>) -> Self {
        Self {
            actors: Arc::new(Mutex::new(HashMap::new())),
            active_tags: Arc::new(Mutex::new(HashMap::new())),
            event_publisher,
        }
    }

    pub async fn start_devices(&self, devices: Vec<Device>, tags: Vec<Tag>) {
        let mut actors = self.actors.lock().await;

        // Group tags by device_id
        // Tags have optional device_id. If None, they are "legacy" or "virtual"?
        // For Phase 3, we assume they link to devices via device_id.
        // Or we might need to support legacy driver instantiation here too?
        // Let's focus on Device-centric tags.

        let mut device_tags: HashMap<String, Vec<Tag>> = HashMap::new();
        for tag in tags {
            let dev_id = tag.device_id();
            if !dev_id.is_empty() {
                device_tags.entry(dev_id.to_string()).or_default().push(tag);
            } else {
                tracing::debug!(tag_id = %tag.id(), "Tag has empty device_id, skipping in DeviceManager");
            }
        }

        for device in devices {
            if !device.enabled {
                info!(device_id = %device.id, "Skipping disabled device");
                continue;
            }

            if actors.contains_key(&device.id) {
                // Already running
                // Todo: check if config changed? For now, we assume full reload = stop all start all?
                // Or idempotent start?
                warn!(device_id = %device.id, "Device actor already running");
                continue;
            }

            let tags_for_device = device_tags.remove(&device.id).unwrap_or_default();

            // Track active tags
            let tag_ids: Vec<String> = tags_for_device.iter().map(|t| t.id().to_string()).collect();

            // Create driver
            let driver_res =
                DriverFactory::create_device_driver(device.clone(), tags_for_device.clone());

            match driver_res {
                Ok(driver) => {
                    let actor = DeviceActor::new(
                        device.clone(),
                        driver,
                        tags_for_device,
                        self.event_publisher.clone(),
                    );

                    let dev_id = device.id.clone();
                    let handle = tokio::spawn(async move {
                        actor.run().await;
                    });

                    actors.insert(dev_id.clone(), handle);
                    self.active_tags.lock().await.insert(dev_id, tag_ids);
                }
                Err(e) => {
                    error!(device_id = %device.id, "Failed to create driver: {}", e);
                }
            }
        }
    }

    pub async fn stop_all(&self) {
        let mut actors = self.actors.lock().await;
        for (id, handle) in actors.drain() {
            info!(device_id = %id, "Stopping device actor");
            handle.abort(); // Simple abort for now
        }
        // Clear active tags
        self.active_tags.lock().await.clear();
    }

    pub async fn get_active_tag_ids(&self) -> Vec<String> {
        let active_map = self.active_tags.lock().await;
        active_map.values().flatten().cloned().collect()
    }
}
