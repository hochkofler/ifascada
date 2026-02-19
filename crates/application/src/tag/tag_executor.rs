use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::time::{interval, sleep};

use domain::driver::DriverConnection;
use domain::tag::{ScalingConfig, TagUpdateMode};
use domain::{DomainEvent, Tag, TagId, TagQuality};

use domain::event::EventPublisher;
use domain::tag::{ValueParser, ValueValidator};
use infrastructure::pipeline::PipelineFactory;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Tag executor - executes a single tag's read/write loop
pub struct TagExecutor {
    tag: Tag,
    driver: Box<dyn DriverConnection>,
    event_publisher: Arc<dyn EventPublisher>,
    parser: Option<Box<dyn ValueParser>>,
    validators: Vec<Box<dyn ValueValidator>>,
    scaling: Option<ScalingConfig>, // NEW
    reconnect_attempts: u32,
    cancel_token: CancellationToken,
    connected_registry: Arc<Mutex<HashSet<TagId>>>,
}

impl TagExecutor {
    /// Create a new tag executor
    pub fn new(
        tag: Tag,
        driver: Box<dyn DriverConnection>,
        event_publisher: Arc<dyn EventPublisher>,
        cancel_token: CancellationToken,
        connected_registry: Arc<Mutex<HashSet<TagId>>>,
    ) -> Self {
        let pipeline_config = tag.pipeline_config().clone();
        let scaling = pipeline_config.scaling.clone();

        if scaling.is_some() {
            tracing::info!(tag_id = %tag.id(), "Executor initialized with scaling config");
        }

        let parser = if let Some(config) = &pipeline_config.parser {
            match PipelineFactory::create_parser(config) {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::error!("Failed to create parser: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let mut validators = Vec::new();
        for config in &pipeline_config.validators {
            match PipelineFactory::create_validator(config) {
                Ok(v) => validators.push(v),
                Err(e) => tracing::error!("Failed to create validator: {}", e),
            }
        }

        Self {
            tag,
            driver,
            event_publisher,
            parser,
            validators,
            scaling, // pre-extracted above
            reconnect_attempts: 0,
            cancel_token,
            connected_registry,
        }
    }

    /// Execute the tag's main loop
    /// This function runs until an unrecoverable error occurs or shutdown signal
    pub async fn execute(&mut self) -> Result<()> {
        // Initial connection
        if let Err(e) = self.connect().await {
            tracing::error!(tag_id = %self.tag.id(), error = ?e, "Failed initial connection - entering retry loop");
            self.tag.mark_error(e.to_string());
            // Do NOT return Err. Fall through to the execution loop which handles reconnection/retries.
            // We just ensure state is clean.
            self.disconnect().await;
        }

        // Execute based on update mode
        let result = match self.tag.update_mode() {
            TagUpdateMode::OnChange { debounce_ms, .. } => {
                self.execute_on_change(*debounce_ms).await
            }
            TagUpdateMode::Polling { interval_ms } => self.execute_polling(*interval_ms).await,
            TagUpdateMode::PollingOnChange {
                interval_ms,
                change_threshold,
            } => {
                self.execute_polling_on_change(*interval_ms, *change_threshold)
                    .await
            }
        };

        // Ensure we disconnect on exit
        self.disconnect().await;
        result
    }

    /// Connect to the driver
    async fn connect(&mut self) -> Result<()> {
        tracing::info!(tag_id = %self.tag.id(), "Connecting to device");

        self.driver
            .connect()
            .await
            .context("Failed to connect driver")?;

        self.tag.mark_offline(); // Will be set to online on first successful read
        self.tag.reset_timeout(); // Initialize timer to prevent immediate timeout
        self.reconnect_attempts = 0;

        // Add to connected registry
        {
            let mut registry = self.connected_registry.lock().await;
            registry.insert(self.tag.id().clone());
        }

        // Publish connection event
        let event = DomainEvent::tag_connected(self.tag.id().clone());
        if let Err(e) = self.event_publisher.publish(event).await {
            tracing::warn!(error = %e, "Failed to publish connection event");
        }

        tracing::info!(tag_id = %self.tag.id(), "Connected successfully");
        Ok(())
    }

    /// Disconnect from the driver
    async fn disconnect(&mut self) {
        if let Err(e) = self.driver.disconnect().await {
            tracing::warn!(tag_id = %self.tag.id(), error = %e, "Error during disconnect");
        }
        self.tag.mark_offline();

        // Remove from connected registry
        let mut registry = self.connected_registry.lock().await;
        registry.remove(self.tag.id());
    }

    /// Execute OnChange mode - event-driven reading
    async fn execute_on_change(&mut self, debounce_ms: u64) -> Result<()> {
        let mut debounce_interval = interval(Duration::from_millis(debounce_ms));

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    tracing::info!(tag_id = %self.tag.id(), "Shutdown signal received");
                    return Ok(());
                }
                _ = debounce_interval.tick() => {
                    // Check if timed out (only if not already offline to avoid spam)
                    if self.tag.status() != domain::tag::TagStatus::Offline && self.tag.is_timed_out() {
                        tracing::warn!(tag_id = %self.tag.id(), "Tag timed out - marking offline but keeping connection");
                        self.handle_timeout().await?;
                        continue;
                    }

                    // Try to read value
                    match self.read_and_publish().await {
                        Ok(_) => {
                            self.reconnect_attempts = 0;
                        }
                        Err(e) => {
                            tracing::error!(tag_id = %self.tag.id(), error = %e, "Read error");
                            self.handle_read_error(e).await?;
                        }
                    }
                }
            }
        }
    }

    /// Execute Polling mode - periodic reading
    async fn execute_polling(&mut self, interval_ms: u64) -> Result<()> {
        let mut poll_interval = interval(Duration::from_millis(interval_ms));

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    tracing::info!(tag_id = %self.tag.id(), "Shutdown signal received");
                    return Ok(());
                }
                _ = poll_interval.tick() => {
                    // Check if timed out (only if not already offline to avoid spam)
                    if self.tag.status() != domain::tag::TagStatus::Offline && self.tag.is_timed_out() {
                        tracing::warn!(tag_id = %self.tag.id(), "Tag timed out - marking offline but keeping connection");
                        self.handle_timeout().await?;
                        continue;
                    }

                    match self.read_and_publish().await {
                        Ok(_) => {
                            self.reconnect_attempts = 0;
                        }
                        Err(e) => {
                            tracing::error!(tag_id = %self.tag.id(), error = %e, "Read error");
                            self.handle_read_error(e).await?;
                        }
                    }
                }
            }
        }
    }

    /// Execute PollingOnChange mode - poll but only publish significant changes
    async fn execute_polling_on_change(
        &mut self,
        interval_ms: u64,
        change_threshold: f64,
    ) -> Result<()> {
        let mut poll_interval = interval(Duration::from_millis(interval_ms));
        let mut last_published_value: Option<f64> = None;

        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                     tracing::info!(tag_id = %self.tag.id(), "Shutdown signal received");
                     return Ok(());
                }
                _ = poll_interval.tick() => {
                    // Check if timed out (only if not already offline to avoid spam)
                    if self.tag.status() != domain::tag::TagStatus::Offline && self.tag.is_timed_out() {
                        tracing::warn!(tag_id = %self.tag.id(), "Tag timed out - marking offline but keeping connection");
                        self.handle_timeout().await?;
                        continue;
                    }

                    match self.driver.read_value().await {
                        Ok(Some(raw_value)) => {
                            if let Some(value) = self.process_value(raw_value)? {
                                // Check if value changed significantly
                                let should_publish = if let Some(num) = value.as_f64() {
                                    match last_published_value {
                                        None => true,
                                        Some(last) => (num - last).abs() >= change_threshold,
                                    }
                                } else {
                                    // For non-numeric values, always publish
                                    true
                                };

                                if should_publish {
                                    self.tag.update_value(value.clone(), TagQuality::Good);
                                    let event = DomainEvent::tag_value_updated(
                                        self.tag.id().clone(),
                                        value.clone(),
                                        TagQuality::Good,
                                    );
                                    if let Err(e) = self.event_publisher.publish(event).await {
                                        tracing::warn!(error = %e, "Failed to publish value update");
                                    } else {
                                        // Re-add to registry in case it was timed out
                                        {
                                            let mut registry = self.connected_registry.lock().await;
                                            registry.insert(self.tag.id().clone());
                                        }
                                        tracing::info!(tag_id = %self.tag.id(), value = %value, "📥 Data received & saved");
                                    }

                                    if let Some(num) = value.as_f64() {
                                        last_published_value = Some(num);
                                    }
                                }
                            }

                            self.reconnect_attempts = 0;
                        }
                        Ok(None) => {
                            // No data available (non-blocking read)
                        }
                        Err(e) => {
                            tracing::error!(tag_id = %self.tag.id(), error = %e, "Read error");
                            self.handle_read_error(e.into()).await?;
                        }
                    }
                }
            }
        }
    }

    /// Read value from driver and publish event
    async fn read_and_publish(&mut self) -> Result<()> {
        match self.driver.read_value().await? {
            Some(raw_value) => {
                tracing::info!(tag_id = %self.tag.id(), raw = %raw_value, "Reading from driver");
                if let Some(value) = self.process_value(raw_value)? {
                    tracing::info!(tag_id = %self.tag.id(), processed = %value, "Value processed");
                    self.tag.update_value(value.clone(), TagQuality::Good);

                    let event = DomainEvent::tag_value_updated(
                        self.tag.id().clone(),
                        value.clone(),
                        TagQuality::Good,
                    );

                    if let Err(e) = self.event_publisher.publish(event).await {
                        tracing::warn!(error = %e, "Failed to publish value update");
                    } else {
                        // Re-add to registry in case it was timed out
                        {
                            let mut registry = self.connected_registry.lock().await;
                            registry.insert(self.tag.id().clone());
                        }
                        tracing::info!(tag_id = %self.tag.id(), value = %value, "📥 Data received & saved");
                    }
                }

                Ok(())
            }
            None => {
                // No data available (non-blocking read)
                Ok(())
            }
        }
    }

    /// Handle timeout condition
    async fn handle_timeout(&mut self) -> Result<()> {
        self.tag.mark_offline();

        // Mark as redundant in registry too?
        // If it's timed out, it's not "active" for data flow
        let mut registry = self.connected_registry.lock().await;
        registry.remove(self.tag.id());

        let event = DomainEvent::tag_disconnected(
            self.tag.id().clone(),
            "Timeout - no data received (connection kept open)".to_string(),
        );
        if let Err(e) = self.event_publisher.publish(event).await {
            tracing::warn!(error = %e, "Failed to publish disconnect event");
        }

        // Do NOT reconnect on logical timeout (silence), just mark offline.
        // Reconnection should only happen on driver errors.
        Ok(())
    }

    /// Handle read error
    async fn handle_read_error(&mut self, error: anyhow::Error) -> Result<()> {
        let error_msg = error.to_string();
        self.tag.mark_error(error_msg.clone());

        let event = DomainEvent::tag_executor_error(self.tag.id().clone(), error_msg.clone());
        if let Err(e) = self.event_publisher.publish(event).await {
            tracing::warn!(error = %e, "Failed to publish error event");
        }

        self.disconnect().await;

        // Try to reconnect
        self.reconnect().await
    }

    /// Reconnect with exponential backoff
    async fn reconnect(&mut self) -> Result<()> {
        self.reconnect_attempts += 1;

        // Exponential backoff with a minimum of 10 seconds
        let backoff_secs = 2u64.pow(self.reconnect_attempts.min(8)) / 2;
        let backoff_duration = Duration::from_secs(backoff_secs.max(10).min(300));

        tracing::debug!(
            tag_id = %self.tag.id(),
            attempt = self.reconnect_attempts,
            backoff_secs = ?backoff_duration.as_secs(),
            "Reconnecting after backoff"
        );

        sleep(backoff_duration).await;

        match self.connect().await {
            Ok(_) => {
                tracing::info!(tag_id = %self.tag.id(), "Reconnected successfully");
                Ok(())
            }
            Err(e) => {
                tracing::warn!(tag_id = %self.tag.id(), error = %e, "Reconnection failed");

                // If we've tried too many times, give up
                if self.reconnect_attempts >= 10 {
                    // Log but don't fail, keep trying
                    // Actually, let's just cap the backoff and keep retrying forever.
                    // The backoff calculation above already caps at 300s (5 mins).
                    // So we just return Ok(()) to keep the loop alive.
                    Ok(())
                } else {
                    // Try again later
                    Ok(())
                }
            }
        }
    }

    pub fn tag(&self) -> &Tag {
        &self.tag
    }

    /// Process raw value through pipeline (parse -> validate)
    fn process_value(&self, raw: serde_json::Value) -> Result<Option<serde_json::Value>> {
        // 1. Parsing
        let parsed_value = if let Some(parser) = &self.parser {
            // Convert to string for parsing if it's not already string-like or if parser expects string
            // Parser trait takes &str.
            let raw_str = match &raw {
                serde_json::Value::String(s) => s.clone(),
                _ => raw.to_string(), // Convert non-strings to string rep
            };

            match parser.parse(&raw_str) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Parsing failed for tag {}: {}", self.tag.id(), e);
                    // If parsing fails, do we discard?
                    // Yes, treating as invalid data format.
                    return Ok(None);
                }
            }
        } else {
            raw.clone()
        };

        // 2. Validation
        for validator in &self.validators {
            if let Err(e) = validator.validate(&parsed_value) {
                tracing::warn!(
                    "Validation failed for tag {}: value = {} error = {}",
                    self.tag.id(),
                    parsed_value,
                    e
                );
                return Ok(None);
            }
        }

        // 3. Scaling (y = x * slope + intercept)
        let scaled_value = if let Some(ScalingConfig::Linear { slope, intercept }) = &self.scaling {
            // Try to get numeric value, also checking for single-element arrays (common in Modbus)
            let val_to_scale = if let Some(num) = parsed_value.as_f64() {
                Some(num)
            } else if let Some(arr) = parsed_value.as_array() {
                if arr.len() == 1 {
                    arr[0].as_f64()
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(num) = val_to_scale {
                let result = num * slope + intercept;
                tracing::info!(tag_id = %self.tag.id(), input = %num, slope = %slope, intercept = %intercept, result = %result, "Linear scaling applied");
                serde_json::json!(result)
            } else {
                tracing::warn!(
                    "Scaling configured for tag {} but value is not numeric or single-element array: {}",
                    self.tag.id(),
                    parsed_value
                );
                parsed_value
            }
        } else {
            if self.tag.id().as_str() == "Temp" || self.tag.id().as_str() == "Humedad" {
                tracing::debug!(tag_id = %self.tag.id(), "Scaling NOT applied (no config found in executor)");
            }
            parsed_value
        };

        tracing::info!(tag_id = %self.tag.id(), final_value = %scaled_value, "Pipeline processing complete");
        Ok(Some(scaled_value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::driver::ConnectionState;
    use domain::tag::{TagStatus, TagValueType};
    use domain::{DomainError, TagId};
    use serde_json::json;
    use std::sync::Mutex;

    // Mock driver for testing
    struct MockDriver {
        connected: bool,
        state: ConnectionState,
        read_values: Mutex<Vec<Option<serde_json::Value>>>,
        fail_next_read: bool,
    }

    impl MockDriver {
        fn new(values: Vec<Option<serde_json::Value>>) -> Self {
            Self {
                connected: false,
                state: ConnectionState::Disconnected,
                read_values: Mutex::new(values),
                fail_next_read: false,
            }
        }
    }

    #[async_trait::async_trait]
    impl DriverConnection for MockDriver {
        async fn connect(&mut self) -> Result<(), DomainError> {
            self.connected = true;
            self.state = ConnectionState::Connected;
            Ok(())
        }

        async fn disconnect(&mut self) -> Result<(), DomainError> {
            self.connected = false;
            self.state = ConnectionState::Disconnected;
            Ok(())
        }

        async fn read_value(&mut self) -> Result<Option<serde_json::Value>, DomainError> {
            if self.fail_next_read {
                return Err(DomainError::DriverError(
                    "Simulated read failure".to_string(),
                ));
            }

            let mut values = self.read_values.lock().unwrap();
            if values.is_empty() {
                Ok(None)
            } else {
                Ok(values.remove(0))
            }
        }

        async fn write_value(&mut self, _value: serde_json::Value) -> Result<(), DomainError> {
            Ok(())
        }

        fn is_connected(&self) -> bool {
            self.connected
        }

        fn connection_state(&self) -> ConnectionState {
            self.state
        }

        fn driver_type(&self) -> &str {
            "Mock"
        }
    }

    // Mock event publisher
    struct MockEventPublisher {
        events: Mutex<Vec<DomainEvent>>,
    }

    impl MockEventPublisher {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn get_events(&self) -> Vec<DomainEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl EventPublisher for MockEventPublisher {
        async fn publish(
            &self,
            event: DomainEvent,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    fn create_test_tag() -> Tag {
        use domain::tag::PipelineConfig;
        Tag::new(
            TagId::new("TEST_TAG").unwrap(),
            "device-1".to_string(),
            json!({"port": "COM3"}),
            TagUpdateMode::OnChange {
                debounce_ms: 100,
                timeout_ms: 30000,
            },
            TagValueType::Simple,
            PipelineConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_executor_connects_on_start() {
        let tag = create_test_tag();
        let driver = Box::new(MockDriver::new(vec![]));
        let publisher = Arc::new(MockEventPublisher::new());
        let token = CancellationToken::new();

        let registry: Arc<tokio::sync::Mutex<HashSet<TagId>>> =
            Arc::new(tokio::sync::Mutex::new(HashSet::new()));
        let mut executor = TagExecutor::new(tag, driver, publisher.clone(), token, registry);

        let result = executor.connect().await;
        assert!(result.is_ok());
        assert!(executor.driver.is_connected());

        // Check connection event was published
        let events = publisher.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), "TagConnected");
    }

    #[tokio::test]
    async fn test_read_and_publish_updates_tag() {
        let tag = create_test_tag();
        let test_value = json!({"weight": 25.5});
        let driver = Box::new(MockDriver::new(vec![Some(test_value.clone())]));
        let publisher = Arc::new(MockEventPublisher::new());
        let token = CancellationToken::new();

        let registry: Arc<tokio::sync::Mutex<HashSet<TagId>>> =
            Arc::new(tokio::sync::Mutex::new(HashSet::new()));
        let mut executor = TagExecutor::new(tag, driver, publisher.clone(), token, registry);
        executor.connect().await.unwrap();

        let result = executor.read_and_publish().await;
        assert!(result.is_ok());

        // Check value was updated
        assert_eq!(executor.tag().last_value(), Some(&test_value));
        assert_eq!(executor.tag().quality(), TagQuality::Good);
        assert_eq!(executor.tag().status(), TagStatus::Online);

        // Check value update event was published
        let events = publisher.get_events();
        assert!(events.iter().any(|e| e.event_type() == "TagValueUpdated"));
    }
}
