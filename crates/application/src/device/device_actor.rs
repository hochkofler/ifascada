use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use domain::device::Device;
use domain::driver::DeviceDriver;
use domain::event::{DomainEvent, EventPublisher};
use domain::tag::{
    PipelineFactory, ScalingConfig, Tag, TagId, TagQuality, TagUpdateMode, ValueParser,
    ValueValidator,
}; // Added missing imports
use tokio_util::sync::CancellationToken;

use dashmap::DashSet;

/// Actor that manages a single Device and its Driver
pub struct DeviceActor {
    device: Device,
    driver: Box<dyn DeviceDriver>,
    tags: Vec<Tag>,
    event_publisher: Arc<dyn EventPublisher>,
    pipelines: Vec<TagPipelineExecutor>,
    cancel_token: CancellationToken,
    connected_registry: Arc<DashSet<TagId>>,
}

struct TagPipelineExecutor {
    tag_id: String,
    parser: Option<Box<dyn ValueParser>>,
    validators: Vec<Box<dyn ValueValidator>>,
    scaling: Option<ScalingConfig>,
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
                let config = tag.pipeline_config();
                let parser = config
                    .parser
                    .as_ref()
                    .and_then(|c| pipeline_factory.create_parser(c).ok());
                let validators = config
                    .validators
                    .iter()
                    .filter_map(|c| pipeline_factory.create_validator(c).ok())
                    .collect();
                let scaling = config.scaling.clone();

                TagPipelineExecutor {
                    tag_id: tag.id().to_string(),
                    parser,
                    validators,
                    scaling,
                }
            })
            .collect();

        Self {
            device,
            driver,
            tags,
            event_publisher,
            pipelines,
            cancel_token: CancellationToken::new(),
            connected_registry: Arc::new(DashSet::new()),
        }
    }

    pub async fn run(mut self) {
        let DeviceActor {
            device,
            mut driver,
            mut tags,
            event_publisher,
            pipelines,
            cancel_token,
            connected_registry: _, // Not used in run loop directly
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

                                            let pipeline = pipelines.iter().find(|p| p.tag_id == tag.id().as_str());
                                            let mut final_val = processed_val;
                                            let mut should_update = true;

                                            if let Some(pipe) = pipeline {
                                                // 2.1 Parsing
                                                if let Some(parser) = &pipe.parser {
                                                    let raw_str = match &final_val {
                                                        serde_json::Value::String(s) => s.clone(),
                                                        _ => final_val.to_string(),
                                                    };
                                                    if let Ok(v) = parser.parse(&raw_str) {
                                                        final_val = v;
                                                    } else {
                                                        warn!(tag_id = %tag.id(), "Parsing failed");
                                                        should_update = false;
                                                    }
                                                }
                                                // 2.2 Validation
                                                if should_update {
                                                    for v in &pipe.validators {
                                                        if let Err(e) = v.validate(&final_val) {
                                                            warn!(tag_id = %tag.id(), error = %e, "Validation failed");
                                                            should_update = false;
                                                            break;
                                                        }
                                                    }
                                                }

                                                // 2.3 Scaling
                                                if should_update {
                                                    if let Some(ScalingConfig::Linear { slope, intercept }) = &pipe.scaling {
                                                        if let Some(num) = final_val.as_f64() {
                                                            let result = num * slope + intercept;
                                                            let rounded = (result * 10000.0).round() / 10000.0;
                                                            final_val = serde_json::json!(rounded);
                                                        }
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
