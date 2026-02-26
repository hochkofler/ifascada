use crate::device::Device;
use crate::error::DomainError;
use crate::id::DeviceId;
use async_trait::async_trait;

#[async_trait]
pub trait DeviceRepository: Send + Sync {
    async fn find_by_id(&self, id: &DeviceId) -> Result<Option<Device>, DomainError>;
    async fn find_all(&self) -> Result<Vec<Device>, DomainError>;
    async fn save(&self, device: Device) -> Result<(), DomainError>;
    async fn delete(&self, id: &DeviceId) -> Result<(), DomainError>;
}
