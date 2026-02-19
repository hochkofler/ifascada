use serde::{Deserialize, Serialize};

/// Tag value quality indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TagQuality {
    /// Value is valid and trustworthy
    Good,
    /// Value is invalid or corrupted
    Bad,
    /// Value quality is uncertain
    Uncertain,
    /// No value received within expected timeframe
    Timeout,
}

impl TagQuality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Uncertain => "uncertain",
            Self::Timeout => "timeout",
        }
    }

    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Good)
    }
}

impl Default for TagQuality {
    fn default() -> Self {
        Self::Uncertain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_as_str() {
        assert_eq!(TagQuality::Good.as_str(), "good");
        assert_eq!(TagQuality::Bad.as_str(), "bad");
        assert_eq!(TagQuality::Uncertain.as_str(), "uncertain");
        assert_eq!(TagQuality::Timeout.as_str(), "timeout");
    }

    #[test]
    fn test_is_usable() {
        assert!(TagQuality::Good.is_usable());
        assert!(!TagQuality::Bad.is_usable());
        assert!(!TagQuality::Uncertain.is_usable());
        assert!(!TagQuality::Timeout.is_usable());
    }

    #[test]
    fn test_default() {
        assert_eq!(TagQuality::default(), TagQuality::Uncertain);
    }
}
