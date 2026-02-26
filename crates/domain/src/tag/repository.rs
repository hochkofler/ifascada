use crate::error::DomainError;
use crate::id::TagId;
use crate::tag::Tag;
use async_trait::async_trait;

#[async_trait]
pub trait TagRepository: Send + Sync {
    async fn find_by_id(&self, id: &TagId) -> Result<Option<Tag>, DomainError>;
    async fn find_all(&self) -> Result<Vec<Tag>, DomainError>;
    async fn save(&self, tag: Tag) -> Result<(), DomainError>;
    async fn delete(&self, id: &TagId) -> Result<(), DomainError>;
}
