use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

use domain::device::Device;
use domain::driver::DeviceDriver;
use domain::event::{DomainEvent, EventPublisher};
use domain::tag::{ScalingConfig, Tag, TagQuality, TagUpdateMode, ValueParser, ValueValidator};
use infrastructure::PipelineFactory;

/// Actor that manages a single Device and its Driver
pub struct DeviceActor {
    device: Device,
    driver: Box<dyn DeviceDriver>,
    tags: Vec<Tag>,
    event_publisher: Arc<dyn EventPublisher>,
    // Pipeline executors per tag
    pipelines: Vec<TagPipelineExecutor>,
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
    ) -> Self {
        let pipelines = tags
            .iter()
            .map(|tag| {
                let config = tag.pipeline_config();
                let parser = config
                    .parser
                    .as_ref()
                    .and_then(|c| PipelineFactory::create_parser(c).ok());
                let validators = config
                    .validators
                    .iter()
                    .filter_map(|c| PipelineFactory::create_validator(c).ok())
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
        }
    }

    pub async fn run(mut self) {
        info!("Starting DeviceActor for {}", self.device.id);

        // Initial connect
        if let Err(e) = self.driver.connect().await {
            error!(device_id = %self.device.id, "Failed initial connection: {}", e);
            // We continue, retry logic is inside the loop?
            // Or driver handles reconnect internally?
            // Driver `connect` is usually one-off.
        }

        // Determine polling interval
        // We take the minimum interval_ms from all tags assigned to this device
        let interval_ms = self
            .tags
            .iter()
            .filter_map(|t| match t.update_mode() {
                TagUpdateMode::Polling { interval_ms } => Some(*interval_ms),
                TagUpdateMode::PollingOnChange { interval_ms, .. } => Some(*interval_ms),
                _ => None,
            })
            .min()
            .unwrap_or(1000); // Default to 1s if no polling tags

        info!(device_id = %self.device.id, interval_ms = %interval_ms, "Starting poll loop");
        let mut timer = interval(Duration::from_millis(interval_ms));

        loop {
            timer.tick().await;

            if !self.driver.is_connected() {
                match self.driver.connect().await {
                    Ok(_) => info!(device_id = %self.device.id, "Reconnected"),
                    Err(e) => {
                        warn!(device_id = %self.device.id, "Failed to reconnect: {}", e);
                        continue;
                    }
                }
            }

            match self.driver.poll().await {
                Ok(results) => {
                    for (tag_id, value_res) in results {
                        match value_res {
                            Ok(val) => {
                                // Find tag and update
                                if let Some(tag) = self.tags.iter_mut().find(|t| t.id() == &tag_id)
                                {
                                    // 1. Unbox single-element arrays (Modbus default)
                                    let processed_val = if let Some(arr) = val.as_array() {
                                        if arr.len() == 1 {
                                            arr[0].clone()
                                        } else {
                                            val.clone()
                                        }
                                    } else {
                                        val.clone()
                                    };

                                    // 2. Full Pipeline Processing
                                    let pipeline =
                                        self.pipelines.iter().find(|p| p.tag_id == tag_id.as_str());

                                    let mut final_val = processed_val;

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
                                                warn!(tag_id = %tag_id, "Parsing failed");
                                                continue;
                                            }
                                        }

                                        // 2.2 Validation
                                        let mut valid = true;
                                        for v in &pipe.validators {
                                            if let Err(e) = v.validate(&final_val) {
                                                warn!(tag_id = %tag_id, error = %e, "Validation failed");
                                                valid = false;
                                                break;
                                            }
                                        }
                                        if !valid {
                                            continue;
                                        }

                                        // 2.3 Scaling
                                        if let Some(ScalingConfig::Linear { slope, intercept }) =
                                            &pipe.scaling
                                        {
                                            if let Some(num) = final_val.as_f64() {
                                                let result = num * slope + intercept;
                                                let rounded = (result * 10000.0).round() / 10000.0;
                                                final_val = serde_json::json!(rounded);
                                            }
                                        }
                                    }

                                    tag.update_value(final_val.clone(), TagQuality::Good);

                                    let event = DomainEvent::tag_value_updated(
                                        tag_id.clone(),
                                        final_val,
                                        TagQuality::Good,
                                    );
                                    if let Err(e) = self.event_publisher.publish(event).await {
                                        warn!("Failed to publish event: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(tag_id = %tag_id, "Read failed: {}", e);
                                // Mark tag error?
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(device_id = %self.device.id, "Batch poll failed: {}", e);
                    // Disconnect to trigger reconnect next loop?
                    let _ = self.driver.disconnect().await;
                }
            }
        }
    }
}
