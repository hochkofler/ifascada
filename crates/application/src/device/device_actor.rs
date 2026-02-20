use std::sync::Arc;
use tracing::{error, info, warn};

use crate::tag::TagPipeline;
use domain::device::Device;
use domain::driver::DeviceDriver;
use domain::event::{DomainEvent, EventPublisher};
use domain::tag::{PipelineFactory, Tag, TagQuality, TagUpdateMode};
use tokio_util::sync::CancellationToken;

/// Actor that manages a single Device and its Driver
pub struct DeviceActor {
    device: Device,
    driver: Box<dyn DeviceDriver>,
    tags: Vec<Tag>,
    event_publisher: Arc<dyn EventPublisher>,
    pipelines: Vec<TagPipeline>,
    cancel_token: CancellationToken,
}

impl DeviceActor {
    pub fn new(
        device: Device,
        driver: Box<dyn DeviceDriver>,
        tags: Vec<Tag>,
        event_publisher: Arc<dyn EventPublisher>,
        pipeline_factory: Arc<dyn PipelineFactory>,
    ) -> Self {
        let pipelines = tags
            .iter()
            .map(|tag| {
                TagPipeline::new(
                    tag.id().clone(),
                    tag.pipeline_config(),
                    pipeline_factory.as_ref(),
                )
            })
            .collect();

        Self {
            device,
            driver,
            tags,
            event_publisher,
            pipelines,
            cancel_token: CancellationToken::new(),
        }
    }

    pub async fn run(self) {
        let DeviceActor {
            device,
            mut driver,
            mut tags,
            event_publisher,
            pipelines,
            cancel_token,
        } = self;

        info!("Starting DeviceActor for {}", device.id);

        // 1. Start Driver
        if let Err(e) = driver.connect().await {
            error!(device_id = %device.id, "Failed initial connection: {}", e);
        }

        // Determine polling interval
        let interval_ms = tags
            .iter()
            .filter_map(|t| match t.update_mode() {
                TagUpdateMode::Polling { interval_ms } => Some(*interval_ms),
                TagUpdateMode::PollingOnChange { interval_ms, .. } => Some(*interval_ms),
                _ => None,
            })
            .min()
            .unwrap_or(1000);

        info!(device_id = %device.id, interval_ms = %interval_ms, "Starting poll loop");
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(interval_ms));

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    info!("Shutdown signal received");
                    break;
                }
                _ = timer.tick() => {
                    if !driver.is_connected() {
                         match driver.connect().await {
                            Ok(_) => info!(device_id = %device.id, "Reconnected"),
                            Err(e) => {
                                warn!(device_id = %device.id, "Failed to reconnect: {}", e);
                                continue;
                            }
                        }
                    }

                    match driver.poll().await {
                        Ok(results) => {
                            for (tag_id, value_res) in results {
                                if let Some(tag) = tags.iter_mut().find(|t| t.id() == &tag_id) {
                                     match value_res {
                                        Ok(val) => {
                                            // Process value inline to avoid borrowing issues
                                            // 1. Unbox single-element arrays
                                            let processed_val = if let Some(arr) = val.as_array() {
                                                if arr.len() == 1 {
                                                    arr[0].clone()
                                                } else {
                                                    val.clone()
                                                }
                                            } else {
                                                val.clone()
                                            };

                                            let pipeline = pipelines.iter().find(|p| p.tag_id() == tag.id());
                                            let mut final_val = processed_val.clone();
                                            let mut should_update = true;

                                            if let Some(pipe) = pipeline {
                                                match pipe.process(processed_val) {
                                                    Ok(Some(v)) => final_val = v,
                                                    Ok(None) => should_update = false,
                                                    Err(e) => {
                                                        warn!(tag_id = %tag.id(), error = %e, "Pipeline processing error");
                                                        should_update = false;
                                                    }
                                                }
                                            }

                                            if should_update {
                                                tag.update_value(final_val.clone(), TagQuality::Good);
                                                let event = DomainEvent::tag_value_updated(tag.id().clone(), final_val, TagQuality::Good);
                                                if let Err(e) = event_publisher.publish(event).await {
                                                    warn!("Failed to publish event: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            warn!(tag_id = %tag_id, "Read failed: {}", e);
                                            tag.update_value(serde_json::Value::Null, TagQuality::Bad);
                                            let event = DomainEvent::tag_value_updated(tag.id().clone(), serde_json::Value::Null, TagQuality::Bad);
                                             if let Err(e) = event_publisher.publish(event).await {
                                                 warn!("Failed to publish bad quality event: {}", e);
                                             }
                                        }
                                     }
                                }
                            }
                        }
                        Err(e) => {
                             error!(device_id = %device.id, "Batch poll failed: {}", e);
                             let _ = driver.disconnect().await;
                        }
                    }
                }
            }
        }
    }
}
