use crate::DomainEvent;
use async_trait::async_trait;

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(
        &self,
        event: DomainEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn publish_batch(
        &self,
        events: Vec<DomainEvent>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for event in events {
            self.publish(event).await?;
        }
        Ok(())
    }
}
