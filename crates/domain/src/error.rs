use thiserror::Error;

/// Domain-level errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum DomainError {
    #[error("Invalid tag ID: {0}")]
    InvalidTagId(String),

    #[error("Invalid tag configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Tag not found: {0}")]
    TagNotFound(String),

    #[error("Invalid tag value: {0}")]
    InvalidValue(String),

    #[error("Tag is disabled")]
    TagDisabled,

    #[error("Invalid driver configuration: {0}")]
    InvalidDriverConfig(String),

    #[error("Driver error: {0}")]
    DriverError(String),
}

pub type Result<T> = std::result::Result<T, DomainError>;
