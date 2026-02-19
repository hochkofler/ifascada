//! Domain layer - Pure business logic with no external dependencies
//!
//! This crate contains:
//! - Entities (Tag, Driver)
//! - Value Objects (TagUpdateMode, TagValueType)
//! - Domain Events
//! - Repository interfaces (traits)
//!
//! Principles:
//! - No dependencies on infrastructure
//! - Business rules enforced at domain level
//! - Rich domain models with behavior
//! - Testable in isolation

pub mod automation;
pub mod device; // NEW
pub mod driver;
pub mod error;
pub mod event;
pub mod printer;
pub mod tag;

// Re-export commonly used types
pub use automation::{ActionConfig, AutomationConfig, TriggerConfig};
pub use error::DomainError;
pub use event::DomainEvent;
pub use tag::{Tag, TagId, TagQuality, TagStatus};
