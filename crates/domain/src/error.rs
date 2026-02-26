use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Invalid ID format: {0}")]
    InvalidId(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Driver error: {0}")]
    DriverError(String),

    #[error("Tag error: {0}")]
    TagError(String),

    #[error("Event bus error: {0}")]
    EventError(String),

    #[error("Not found: {0}")]
    NotFound(String),
}
