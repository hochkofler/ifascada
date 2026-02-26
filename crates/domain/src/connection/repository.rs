use crate::connection::Connection;
use crate::error::DomainError;
use crate::id::ConnectionId;
use async_trait::async_trait;

#[async_trait]
pub trait ConnectionRepository: Send + Sync {
    async fn find_by_id(&self, id: &ConnectionId) -> Result<Option<Connection>, DomainError>;
    async fn find_all(&self) -> Result<Vec<Connection>, DomainError>;
    async fn save(&self, connection: Connection) -> Result<(), DomainError>;
    async fn delete(&self, id: &ConnectionId) -> Result<(), DomainError>;
}
