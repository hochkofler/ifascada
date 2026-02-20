use crate::error::{DomainError, Result};
use serde::{Deserialize, Serialize};

/// Value object representing a Tag identifier
///
/// Rules:
/// - Must be non-empty
/// - Must contain only alphanumeric, underscore, and hyphen
/// - Max length 100 characters
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagId(String);

impl TagId {
    /// Create a new TagId with validation
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();

        // Validate non-empty
        if id.is_empty() {
            return Err(DomainError::InvalidTagId(
                "Tag ID cannot be empty".to_string(),
            ));
        }

        // Validate length
        if id.len() > 100 {
            return Err(DomainError::InvalidTagId(format!(
                "Tag ID too long: {} chars (max 100)",
                id.len()
            )));
        }

        // Validate characters
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/')
        {
            return Err(DomainError::InvalidTagId(format!(
                "Tag ID {id} must contain only alphanumeric, underscore, hyphen, and forward slash"
            )));
        }

        Ok(Self(id))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TagId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_tag_id() {
        let id = TagId::new("SCALE_CABINA_1").unwrap();
        assert_eq!(id.as_str(), "SCALE_CABINA_1");
    }

    #[test]
    fn test_tag_id_with_hyphen() {
        let id = TagId::new("TEMP-REACTOR-01").unwrap();
        assert_eq!(id.as_str(), "TEMP-REACTOR-01");
    }

    #[test]
    fn test_tag_id_hierarchical() {
        let id = TagId::new("plant1/area2/unit3/temp").unwrap();
        assert_eq!(id.as_str(), "plant1/area2/unit3/temp");
    }

    #[test]
    fn test_empty_tag_id() {
        let result = TagId::new("");
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            DomainError::InvalidTagId("Tag ID cannot be empty".to_string())
        );
    }

    #[test]
    fn test_tag_id_too_long() {
        let long_id = "A".repeat(101);
        let result = TagId::new(long_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_tag_id_invalid_characters() {
        let result = TagId::new("SCALE@CABINA#1");
        assert!(result.is_err());
    }

    #[test]
    fn test_tag_id_display() {
        let id = TagId::new("TEST_TAG").unwrap();
        assert_eq!(format!("{}", id), "TEST_TAG");
    }
}
