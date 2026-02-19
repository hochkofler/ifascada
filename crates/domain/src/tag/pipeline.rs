use crate::AutomationConfig;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Types of parsers available
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ParserConfig {
    /// No parsing, pass raw value (if possible)
    None,
    /// Regex extraction (captures first group)
    Regex { pattern: String },
    /// JSON extraction (access field by path)
    Json { path: String },
    /// Custom parser implemented in code
    Custom {
        name: String,
        config: Option<serde_json::Value>,
    },
    /// Map array by index to keys with optional scaling
    IndexMap {
        keys: Vec<String>,
        #[serde(default)]
        scale: Option<f64>,
    },
}

/// Types of validators available
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ValidatorConfig {
    /// Value must be within range (inclusive)
    Range { min: Option<f64>, max: Option<f64> },
    /// String representation must contain substring
    Contains { substring: String },
    /// Custom validator implemented in code
    Custom {
        name: String,
        config: Option<serde_json::Value>,
    },
}

/// Types of scaling/transformations available
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum ScalingConfig {
    /// Linear scaling: y = mx + b
    Linear { slope: f64, intercept: f64 },
    // Future: Formula, Map, etc.
}

/// Pipeline configuration for a Tag
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineConfig {
    #[serde(default)]
    pub parser: Option<ParserConfig>,
    #[serde(default)]
    pub scaling: Option<ScalingConfig>, // NEW
    #[serde(default)]
    pub validators: Vec<ValidatorConfig>,
    #[serde(default)]
    pub automations: Vec<AutomationConfig>,
}

impl PipelineConfig {
    pub fn linear(slope: f64, intercept: f64) -> ScalingConfig {
        ScalingConfig::Linear { slope, intercept }
    }
}

// Traits for implementation (to be used in Application/Infrastructure layer)

pub trait ValueParser: Send + Sync + Debug {
    fn parse(
        &self,
        raw_value: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;
}

pub trait ValueValidator: Send + Sync + Debug {
    fn validate(
        &self,
        value: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
