use anyhow::Result;
use domain::tag::{
    PipelineConfig, PipelineFactory, ScalingConfig, TagId, ValueParser, ValueValidator,
};
use tracing::{debug, error, warn};

/// Service responsible for processing raw tag values through the configured pipeline.
///
/// Steps:
/// 1. Parsing: Convert raw string/value to structured data
/// 2. Validation: Check against range/logic rules
/// 3. Scaling: Apply linear transformation (y = mx + b)
pub struct TagPipeline {
    tag_id: TagId,
    parser: Option<Box<dyn ValueParser>>,
    validators: Vec<Box<dyn ValueValidator>>,
    scaling: Option<ScalingConfig>,
}

impl TagPipeline {
    pub fn new(
        tag_id: TagId,
        config: &PipelineConfig,
        pipeline_factory: &dyn PipelineFactory,
    ) -> Self {
        let parser = if let Some(parser_config) = &config.parser {
            match pipeline_factory.create_parser(parser_config) {
                Ok(p) => Some(p),
                Err(e) => {
                    error!("Failed to create parser for tag {}: {}", tag_id, e);
                    None
                }
            }
        } else {
            None
        };

        let mut validators = Vec::new();
        for validator_config in &config.validators {
            match pipeline_factory.create_validator(validator_config) {
                Ok(v) => validators.push(v),
                Err(e) => error!("Failed to create validator for tag {}: {}", tag_id, e),
            }
        }

        Self {
            tag_id,
            parser,
            validators,
            scaling: config.scaling.clone(),
        }
    }

    pub fn tag_id(&self) -> &TagId {
        &self.tag_id
    }

    /// Process a raw value through the pipeline.
    /// Returns `Ok(Some(value))` if successful and valid.
    /// Returns `Ok(None)` if validation fails or parsing fails (data discarded).
    /// Returns `Err` only on critical system errors (currently none in this flow).
    pub fn process(&self, raw: serde_json::Value) -> Result<Option<serde_json::Value>> {
        // 1. Parsing
        let parsed_value = if let Some(parser) = &self.parser {
            let raw_str = match &raw {
                serde_json::Value::String(s) => s.clone(),
                _ => raw.to_string(),
            };

            match parser.parse(&raw_str) {
                Ok(v) => v,
                Err(e) => {
                    warn!("Parsing failed for tag {}: {}", self.tag_id, e);
                    return Ok(None);
                }
            }
        } else {
            raw.clone()
        };

        // 2. Validation
        for validator in &self.validators {
            if let Err(e) = validator.validate(&parsed_value) {
                warn!(
                    "Validation failed for tag {}: value = {} error = {}",
                    self.tag_id, parsed_value, e
                );
                return Ok(None);
            }
        }

        // 3. Scaling
        let scaled_value = if let Some(ScalingConfig::Linear { slope, intercept }) = &self.scaling {
            // Try to extract number, handling single-element arrays (common in Modbus)
            let val_to_scale = if let Some(num) = parsed_value.as_f64() {
                Some(num)
            } else if let Some(arr) = parsed_value.as_array() {
                if arr.len() == 1 {
                    arr[0].as_f64()
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(num) = val_to_scale {
                let result = num * slope + intercept;
                // Round to 4 decimal places for cleaner output
                let rounded = (result * 10000.0).round() / 10000.0;

                debug!(
                    tag_id = %self.tag_id,
                    input = %num,
                    slope = %slope,
                    intercept = %intercept,
                    result = %rounded,
                    "Linear scaling applied"
                );

                serde_json::json!(rounded)
            } else {
                warn!(
                    "Scaling configured for tag {} but value is not numeric: {}",
                    self.tag_id, parsed_value
                );
                parsed_value
            }
        } else {
            parsed_value
        };

        Ok(Some(scaled_value))
    }
}
