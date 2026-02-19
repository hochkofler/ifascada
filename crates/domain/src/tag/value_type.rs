use serde::{Deserialize, Serialize};

/// Type of value a tag holds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TagValueType {
    /// Simple numeric value
    Simple,
    /// Composite value with multiple fields
    Composite,
}

impl TagValueType {
    pub fn is_simple(&self) -> bool {
        matches!(self, Self::Simple)
    }

    pub fn is_composite(&self) -> bool {
        matches!(self, Self::Composite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple() {
        let vt = TagValueType::Simple;
        assert!(vt.is_simple());
        assert!(!vt.is_composite());
    }

    #[test]
    fn test_composite() {
        let vt = TagValueType::Composite;
        assert!(!vt.is_simple());
        assert!(vt.is_composite());
    }
}
