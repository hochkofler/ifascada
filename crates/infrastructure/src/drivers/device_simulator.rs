use async_trait::async_trait;
use domain::device::Device;
use domain::driver::{ConnectionState, DeviceDriver};
use domain::error::DomainError;
use domain::tag::{Tag, TagId};
use serde_json::Value;

use super::simulator_connection::SimulatorConfig;

/// Simulator implementation of a Device Driver
/// Generates values for multiple tags
pub struct SimulatorDeviceDriver {
    #[allow(dead_code)]
    device: Device,
    tags: Vec<Tag>,
    state: ConnectionState,
}

impl SimulatorDeviceDriver {
    pub fn new(device: Device, tags: Vec<Tag>) -> Self {
        Self {
            device,
            tags,
            state: ConnectionState::Disconnected,
        }
    }

    fn generate_value_for_tag(&self, tag: &Tag) -> Result<Value, DomainError> {
        // Parse simulator config from tag
        // Optimization: In a real driver, we would parse this once at creation.
        // For simulator, it's fine.
        let config: SimulatorConfig =
            serde_json::from_value(tag.source_config().clone()).map_err(|e| {
                DomainError::InvalidDriverConfig(format!(
                    "Invalid simulator config for tag {}: {}",
                    tag.id(),
                    e
                ))
            })?;

        // Logic copied/adapted from SimulatorConnection
        // We use system time to generate a deterministic pattern based on the config
        let now = std::time::SystemTime::now();
        let since_epoch = now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        // Use tag ID hash or similar to offset the wave so all tags don't look identical?
        // For now, simple implementation

        let range = config.max_value - config.min_value;
        let midpoint = config.min_value + (range / 2.0);
        let amplitude = range / 2.0;

        // Sine wave: period 10 seconds
        let frequency = 0.1;
        // Use since_epoch as elapsed
        let raw_value =
            midpoint + amplitude * (since_epoch * frequency * 2.0 * std::f64::consts::PI).sin();
        let value = (raw_value * 100.0).round() / 100.0;

        if let Some(pattern) = &config.pattern {
            return Ok(Value::String(
                pattern.replace("{}", &format!("{:.2}", value)),
            ));
        }

        Ok(Value::String(format!(
            "ST,GS,  {:.2}{}",
            value, config.unit
        )))
    }
}

#[async_trait]
impl DeviceDriver for SimulatorDeviceDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    fn connection_state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(&mut self) -> Result<Vec<(TagId, Result<Value, DomainError>)>, DomainError> {
        // Simulate processing delay?
        // tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let mut results = Vec::new();

        for tag in &self.tags {
            // We return Result<Value> for each tag so partial failures don't kill the batch
            let val_res = self.generate_value_for_tag(tag);
            results.push((tag.id().clone(), val_res));
        }

        Ok(results)
    }

    async fn write(&mut self, _tag_id: &TagId, _value: Value) -> Result<(), DomainError> {
        // Log write
        Ok(())
    }
}
