use async_trait::async_trait;
use domain::DomainEvent;
use domain::event::EventPublisher;
use std::sync::Arc;

pub struct CompositeEventPublisher {
    publishers: Vec<Arc<dyn EventPublisher>>,
}

impl CompositeEventPublisher {
    pub fn new(publishers: Vec<Arc<dyn EventPublisher>>) -> Self {
        Self { publishers }
    }
}

#[async_trait]
impl EventPublisher for CompositeEventPublisher {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for publisher in &self.publishers {
            // Clone event for each publisher since publish takes ownership/reference
            // DomainEvent is Clone.
            if let Err(e) = publisher.publish(event.clone()).await {
                // Log error but continue to other publishers
                tracing::error!("Failed to publish event to one of the publishers: {}", e);
            }
        }
        Ok(())
    }
}
