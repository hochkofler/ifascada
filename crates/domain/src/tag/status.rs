use serde::{Deserialize, Serialize};

/// Tag operational status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TagStatus {
    /// Tag is online and receiving updates
    Online,
    /// Tag has not received updates within timeout
    Offline,
    /// Tag encountered an error
    Error,
    /// Tag status unknown (newly created)
    Unknown,
}

impl TagStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Error => "error",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Online)
    }
}

impl Default for TagStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_status_as_str() {
        assert_eq!(TagStatus::Online.as_str(), "online");
        assert_eq!(TagStatus::Offline.as_str(), "offline");
        assert_eq!(TagStatus::Error.as_str(), "error");
        assert_eq!(TagStatus::Unknown.as_str(), "unknown");
    }

    #[test]
    fn test_is_healthy() {
        assert!(TagStatus::Online.is_healthy());
        assert!(!TagStatus::Offline.is_healthy());
        assert!(!TagStatus::Error.is_healthy());
        assert!(!TagStatus::Unknown.is_healthy());
    }

    #[test]
    fn test_default() {
        assert_eq!(TagStatus::default(), TagStatus::Unknown);
    }
}
