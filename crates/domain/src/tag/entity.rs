use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tag::{ScalingConfig, TagId, TagUpdateMode, TagValueType, ValidatorConfig};

/// Represents a Logical Tag in the system.
/// A Tag defines HOW to interpret data from a Device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub device_id: String,
    pub source_config: Value,
    pub update_mode: TagUpdateMode,
    pub data_type: TagValueType,
    pub scaling: Option<ScalingConfig>,
    pub validators: Vec<ValidatorConfig>,
}

impl Tag {
    pub fn new(
        id: TagId,
        device_id: String,
        source_config: Value,
        update_mode: TagUpdateMode,
        data_type: TagValueType,
        scaling: Option<ScalingConfig>,
        validators: Vec<ValidatorConfig>,
    ) -> Self {
        Self {
            id,
            device_id,
            source_config,
            update_mode,
            data_type,
            scaling,
            validators,
        }
    }

    /// Applies configured scaling to a raw value.
    /// Returns the scaled value, or the original if no scaling is configured.
    pub fn apply_scaling(&self, raw: Value) -> Result<Value, String> {
        let scaling = match &self.scaling {
            Some(s) => s,
            None => return Ok(raw),
        };

        match scaling {
            ScalingConfig::Linear { slope, intercept } => {
                let num = raw
                    .as_f64()
                    .ok_or_else(|| "Value is not a number".to_string())?;
                let scaled = num * slope + intercept;
                Ok(serde_json::json!(scaled))
            }
        }
    }

    /// Validates a value against configured validators.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        for validator in &self.validators {
            match validator {
                ValidatorConfig::Range { min, max } => {
                    let num = value
                        .as_f64()
                        .ok_or_else(|| "Value is not a number".to_string())?;
                    if let Some(min_val) = min {
                        if num < *min_val {
                            return Err(format!("Value {} is below minimum {}", num, min_val));
                        }
                    }
                    if let Some(max_val) = max {
                        if num > *max_val {
                            return Err(format!("Value {} is above maximum {}", num, max_val));
                        }
                    }
                }
                ValidatorConfig::Contains { substring } => {
                    let s = value
                        .as_str()
                        .ok_or_else(|| "Value is not a string".to_string())?;
                    if !s.contains(substring) {
                        return Err(format!("Value does not contain '{}'", substring));
                    }
                }
                _ => {} // Implement others as needed
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::PipelineConfig;
    use serde_json::json;

    #[test]
    fn test_tag_creation_refactored() {
        let tag = Tag::new(
            TagId::new("plant1/tank/level").unwrap(),
            "plc-01".to_string(),
            json!({"address": "40001"}),
            TagUpdateMode::Polling { interval_ms: 1000 },
            TagValueType::Simple,
            None,   // No scaling
            vec![], // No validators
        );

        assert_eq!(tag.id.as_str(), "plant1/tank/level");
        assert_eq!(tag.device_id, "plc-01");
        assert_eq!(tag.source_config, json!({"address": "40001"}));
    }

    #[test]
    fn test_tag_scaling_linear() {
        // TDD: Linear Scaling (raw * 2 + 10)
        let scaling = PipelineConfig::linear(2.0, 10.0);

        let tag = Tag::new(
            TagId::new("temp").unwrap(),
            "dev1".into(),
            "addr".into(),
            TagUpdateMode::OnChange {
                debounce_ms: 0,
                timeout_ms: 0,
            },
            TagValueType::Simple,
            Some(scaling),
            vec![],
        );

        let raw = 10.0;
        let scaled = tag.apply_scaling(json!(raw)).unwrap();
        assert_eq!(scaled.as_f64(), Some(30.0)); // 10*2 + 10 = 30
    }

    #[test]
    fn test_tag_validation_range() {
        // TDD: Range Validation (0 - 100)
        let validator = ValidatorConfig::Range {
            min: Some(0.0),
            max: Some(100.0),
        };

        let tag = Tag::new(
            TagId::new("pressure").unwrap(),
            "dev1".into(),
            "addr".into(),
            TagUpdateMode::Polling { interval_ms: 1000 },
            TagValueType::Simple,
            None,
            vec![validator],
        );

        // Valid
        assert!(tag.validate(&json!(50.0)).is_ok());

        // Invalid (Too high)
        assert!(tag.validate(&json!(150.0)).is_err());
    }
}
