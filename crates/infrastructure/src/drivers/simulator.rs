use async_trait::async_trait;
use domain::driver::{ConnectionState, DriverConnection};
use domain::error::DomainError;
use domain::id::TagId;
use domain::tag::TagValue;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

pub struct SimulatorDriver {
    state: ConnectionState,
    tag_values: HashMap<TagId, TagValue>,
    tick: f64,
}

impl SimulatorDriver {
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            tag_values: HashMap::new(),
            tick: 0.0,
        }
    }

    pub fn with_tags(tag_ids: Vec<TagId>) -> Self {
        let mut tag_values = HashMap::new();
        for tag_id in tag_ids {
            tag_values.insert(tag_id, TagValue::Float(0.0));
        }

        Self {
            state: ConnectionState::Disconnected,
            tag_values,
            tick: 0.0,
        }
    }
}

#[async_trait]
impl DriverConnection for SimulatorDriver {
    async fn connect(&mut self) -> Result<(), DomainError> {
        self.state = ConnectionState::Connecting;
        sleep(Duration::from_millis(100)).await;
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

    fn state(&self) -> ConnectionState {
        self.state
    }

    async fn poll(&mut self) -> Result<Vec<(TagId, Result<TagValue, DomainError>)>, DomainError> {
        if !self.is_connected() {
            return Err(DomainError::DriverError("Not connected".into()));
        }

        self.tick += 1.0;
        let mut out = Vec::with_capacity(self.tag_values.len());

        for (tag_id, current) in &mut self.tag_values {
            let next = match current {
                TagValue::Float(v) => {
                    // Simple deterministic wave-ish progression for stable tests.
                    let value = if self.tick % 20.0 < 10.0 {
                        *v + 1.0
                    } else {
                        *v - 1.0
                    };
                    TagValue::Float(value)
                }
                TagValue::Integer(v) => TagValue::Integer(*v + 1),
                TagValue::Boolean(v) => TagValue::Boolean(!*v),
                TagValue::String(v) => TagValue::String(format!("{v}.{}", self.tick as i64)),
            };

            *current = next.clone();
            out.push((tag_id.clone(), Ok(next)));
        }

        Ok(out)
    }

    async fn write(&mut self, _tag_id: TagId, _value: TagValue) -> Result<(), DomainError> {
        Ok(())
    }
}

pub struct SimulatorFactory;

impl application::runtime::DriverFactory for SimulatorFactory {
    fn create(&self, connection: &domain::connection::Connection) -> Box<dyn DriverConnection> {
        let tag_ids = connection
            .config
            .transport
            .get("tag_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(TagId::new)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if tag_ids.is_empty() {
            Box::new(SimulatorDriver::new())
        } else {
            Box::new(SimulatorDriver::with_tags(tag_ids))
        }
    }
}
