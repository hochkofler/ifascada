use async_trait::async_trait;
use domain::DomainError;
use domain::driver::{ConnectionState, DriverConnection, DriverType};
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Debug, Deserialize, Clone)]
pub struct SimulatorConfig {
    pub min_value: f64,
    pub max_value: f64,
    pub interval_ms: u64,
    pub unit: String,
    pub pattern: Option<String>,
}

pub struct SimulatorConnection {
    config: SimulatorConfig,
    start_time: Instant,
    last_read_time: Instant,
}

impl SimulatorConnection {
    pub fn new(config: SimulatorConfig) -> Self {
        let now = Instant::now();
        Self {
            config,
            start_time: now,
            // Initialize last_read to start_time so first read happens after interval
            last_read_time: now,
        }
    }

    fn generate_current_value(&self) -> String {
        let elapsed = self.start_time.elapsed().as_secs_f64();

        let range = self.config.max_value - self.config.min_value;
        let midpoint = self.config.min_value + (range / 2.0);
        let amplitude = range / 2.0;

        // Sine wave: period 10 seconds
        let frequency = 0.1;
        let raw_value =
            midpoint + amplitude * (elapsed * frequency * 2.0 * std::f64::consts::PI).sin();
        let value = (raw_value * 100.0).round() / 100.0;

        // Use pattern if available, otherwise default to Mettler Toledo format
        if let Some(pattern) = &self.config.pattern {
            return pattern.replace("{}", &format!("{:.2}", value));
        }

        // Format: "ST,GS,  12.34kg"
        format!("ST,GS,  {:.2}{}", value, self.config.unit)
    }
}

#[async_trait]
impl DriverConnection for SimulatorConnection {
    async fn connect(&mut self) -> Result<(), DomainError> {
        // Instant connection
        tracing::info!("Simulator connected with config: {:?}", self.config);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), DomainError> {
        tracing::info!("Simulator disconnected");
        Ok(())
    }

    async fn read_value(&mut self) -> Result<Option<Value>, DomainError> {
        let now = Instant::now();
        let next_read_time = self.last_read_time + Duration::from_millis(self.config.interval_ms);

        if next_read_time > now {
            // Wait until the interval has passed
            sleep(next_read_time - now).await;
        }

        self.last_read_time = Instant::now();
        let payload = self.generate_current_value();

        // Return as String Value, similar to how RS232 driver returns raw string data
        Ok(Some(Value::String(payload)))
    }

    async fn write_value(&mut self, value: Value) -> Result<(), DomainError> {
        // Just log the write
        tracing::info!("Simulator received write: {:?}", value);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn connection_state(&self) -> ConnectionState {
        ConnectionState::Connected
    }

    fn driver_type(&self) -> &str {
        DriverType::Simulator.as_str()
    }
}
