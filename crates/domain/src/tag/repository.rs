use crate::{DomainError, Tag, TagId};
use async_trait::async_trait;

/// Repository interface for Tag persistence
///
/// This trait defines the contract for tag storage and retrieval.
/// Implementations should be provided in the infrastructure layer.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TagRepository: Send + Sync {
    /// Save a new tag or update existing one
    async fn save(&self, tag: &Tag) -> Result<(), DomainError>;

    /// Find tag by ID
    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError>;

    /// Find all tags
    async fn find_all(&self) -> Result<Vec<Tag>, DomainError>;

    /// Find tags assigned to a specific edge agent
    async fn find_by_agent(&self, agent_id: &str) -> Result<Vec<Tag>, DomainError>;

    /// Find enabled tags only
    async fn find_enabled(&self) -> Result<Vec<Tag>, DomainError>;

    /// Delete tag by ID
    async fn delete(&self, id: &TagId) -> Result<(), DomainError>;
}
