use super::Device;
use crate::DomainError;
use async_trait::async_trait;

#[async_trait]
pub trait DeviceRepository: Send + Sync {
    async fn save(&self, device: &Device) -> Result<(), DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Device>, DomainError>;
    async fn find_all(&self) -> Result<Vec<Device>, DomainError>;
    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Device>, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;
}
