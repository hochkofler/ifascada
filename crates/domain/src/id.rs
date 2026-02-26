use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Id<T>(String, std::marker::PhantomData<T>);

impl<T> Id<T> {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into(), std::marker::PhantomData)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<T> FromStr for Id<T> {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(DomainError::InvalidId("ID cannot be empty".into()));
        }
        Ok(Self::new(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TagType;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionType;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceType;

pub type TagId = Id<TagType>;
pub type ConnectionId = Id<ConnectionType>;
pub type DeviceId = Id<DeviceType>;
