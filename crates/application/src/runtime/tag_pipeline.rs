use crate::runtime::live_state::TagState;
use chrono::Utc;
use domain::tag::{QualityReason, Tag, TagQuality, TagValue};
use serde_json::Value;

pub trait TagValuePipeline: Send + Sync {
    fn process(&self, _definition: &Tag, raw_value: TagValue) -> TagState;
}

#[derive(Default)]
pub struct PassthroughTagValuePipeline;

impl TagValuePipeline for PassthroughTagValuePipeline {
    fn process(&self, _definition: &Tag, raw_value: TagValue) -> TagState {
        let trigger_value = raw_value.clone();
        TagState {
            value: raw_value,
            trigger_value: Some(trigger_value),
            quality: TagQuality::good(),
            timestamp: Utc::now(),
        }
    }
}

pub fn build_pipeline_for_tag(definition: &Tag) -> std::sync::Arc<dyn TagValuePipeline> {
    if let Some(cfg) = TagProcessingConfig::from_metadata(&definition.metadata) {
        return std::sync::Arc::new(MetadataTagValuePipeline { cfg });
    }
    std::sync::Arc::new(PassthroughTagValuePipeline)
}

#[derive(Debug, Clone)]
struct TagProcessingConfig {
    extract: Option<ExtractConfig>,
    scale: Option<ScaleConfig>,
    range: Option<RangeConfig>,
    format: Option<FormatConfig>,
    trigger_source: TriggerValueSource,
}

#[derive(Debug, Clone, Copy)]
struct ExtractConfig {
    mode: ExtractMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExtractMode {
    CompoundJson,
}

#[derive(Debug, Clone, Copy)]
struct ScaleConfig {
    factor: f64,
    offset: f64,
}

#[derive(Debug, Clone, Copy)]
struct RangeConfig {
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Debug, Clone)]
struct FormatConfig {
    template: String,
    trim: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriggerValueSource {
    Canonical,
    Display,
}

impl TagProcessingConfig {
    fn from_metadata(metadata: &Value) -> Option<Self> {
        let pipeline = metadata.get("pipeline")?;
        let extract = parse_extract_config(pipeline);
        let scale = parse_scale_config(pipeline);
        let range = parse_range_config(pipeline);
        let format = parse_format_config(pipeline);
        let trigger_source = parse_trigger_source(pipeline);
        if extract.is_none() && scale.is_none() && range.is_none() && format.is_none() {
            return None;
        }
        Some(Self {
            extract,
            scale,
            range,
            format,
            trigger_source,
        })
    }
}

struct MetadataTagValuePipeline {
    cfg: TagProcessingConfig,
}

impl TagValuePipeline for MetadataTagValuePipeline {
    fn process(&self, _definition: &Tag, raw_value: TagValue) -> TagState {
        let mut value = raw_value;
        let mut ctx = PipelineContext::default();
        let mut canonical = value.clone();

        if let Some(extract) = self.cfg.extract {
            let input_for_extract = value.clone();
            match apply_extract(extract, input_for_extract) {
                Ok((v, c)) => {
                    value = v;
                    ctx = c;
                    canonical = value.clone();
                }
                Err(_) => {
                    return TagState {
                        value,
                        trigger_value: None,
                        quality: TagQuality::bad(QualityReason::ValidationFailed),
                        timestamp: Utc::now(),
                    };
                }
            }
        }

        if let Some(scale) = self.cfg.scale {
            value = match as_numeric(&value) {
                Some(n) => TagValue::Float((n * scale.factor) + scale.offset),
                None => {
                    return TagState {
                        value,
                        trigger_value: None,
                        quality: TagQuality::bad(QualityReason::ValidationFailed),
                        timestamp: Utc::now(),
                    };
                }
            };
            canonical = value.clone();
        }

        if let Some(range) = self.cfg.range {
            let n = match as_numeric(&value) {
                Some(n) => n,
                None => {
                    return TagState {
                        value,
                        trigger_value: None,
                        quality: TagQuality::bad(QualityReason::ValidationFailed),
                        timestamp: Utc::now(),
                    };
                }
            };
            if range.min.map(|m| n < m).unwrap_or(false) || range.max.map(|m| n > m).unwrap_or(false) {
                return TagState {
                    value,
                    trigger_value: None,
                    quality: TagQuality::bad(QualityReason::OutOfRange),
                    timestamp: Utc::now(),
                };
            }
            canonical = value.clone();
        }

        if let Some(fmt) = &self.cfg.format {
            value = TagValue::String(apply_format(fmt, &value, &ctx));
        }

        let trigger_value = match self.cfg.trigger_source {
            TriggerValueSource::Canonical => canonical,
            TriggerValueSource::Display => value.clone(),
        };

        TagState {
            value,
            trigger_value: Some(trigger_value),
            quality: TagQuality::good(),
            timestamp: Utc::now(),
        }
    }
}

fn parse_extract_config(pipeline: &Value) -> Option<ExtractConfig> {
    let extract = pipeline.get("extract")?;
    let mode = match extract.as_str()?.trim().to_ascii_lowercase().as_str() {
        "scale:compound" | "compound_json" => ExtractMode::CompoundJson,
        _ => return None,
    };
    Some(ExtractConfig { mode })
}

fn parse_scale_config(pipeline: &Value) -> Option<ScaleConfig> {
    let scale = pipeline.get("scale")?;
    let factor = scale
        .get("factor")
        .and_then(Value::as_f64)
        .or_else(|| scale.get("slope").and_then(Value::as_f64))
        .unwrap_or(1.0);
    let offset = scale
        .get("offset")
        .and_then(Value::as_f64)
        .or_else(|| scale.get("intercept").and_then(Value::as_f64))
        .unwrap_or(0.0);
    Some(ScaleConfig { factor, offset })
}

fn parse_range_config(pipeline: &Value) -> Option<RangeConfig> {
    let range = pipeline
        .get("range")
        .or_else(|| pipeline.pointer("/validate/range"))?;
    let min = range.get("min").and_then(Value::as_f64);
    let max = range.get("max").and_then(Value::as_f64);
    Some(RangeConfig { min, max })
}

fn parse_format_config(pipeline: &Value) -> Option<FormatConfig> {
    let template = pipeline.get("format")?.as_str()?.to_string();
    let trim = pipeline.get("trim").and_then(Value::as_bool).unwrap_or(false);
    Some(FormatConfig { template, trim })
}

fn parse_trigger_source(pipeline: &Value) -> TriggerValueSource {
    let src = pipeline
        .pointer("/trigger/value_source")
        .and_then(Value::as_str)
        .or_else(|| pipeline.get("trigger_source").and_then(Value::as_str))
        .map(|s| s.trim().to_ascii_lowercase());
    match src.as_deref() {
        Some("display") => TriggerValueSource::Display,
        _ => TriggerValueSource::Canonical,
    }
}

fn as_numeric(value: &TagValue) -> Option<f64> {
    match value {
        TagValue::Float(v) => Some(*v),
        TagValue::Integer(v) => Some(*v as f64),
        TagValue::String(s) => s.trim().parse::<f64>().ok(),
        TagValue::Boolean(_) => None,
    }
}

#[derive(Debug, Clone, Default)]
struct PipelineContext {
    unit: Option<String>,
    raw: Option<String>,
}

fn apply_extract(
    cfg: ExtractConfig,
    input: TagValue,
) -> Result<(TagValue, PipelineContext), ()> {
    match cfg.mode {
        ExtractMode::CompoundJson => extract_compound_json(input),
    }
}

fn extract_compound_json(input: TagValue) -> Result<(TagValue, PipelineContext), ()> {
    let text = match input {
        TagValue::String(s) => s,
        _ => return Err(()),
    };
    let v: Value = serde_json::from_str(&text).map_err(|_| ())?;
    let obj = v.as_object().ok_or(())?;
    let value = obj.get("value").cloned().ok_or(())?;
    let out_value = match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                TagValue::Integer(i)
            } else {
                TagValue::Float(n.as_f64().ok_or(())?)
            }
        }
        Value::String(s) => TagValue::String(s),
        Value::Bool(b) => TagValue::Boolean(b),
        _ => return Err(()),
    };
    let unit = obj.get("unit").and_then(Value::as_str).map(ToString::to_string);
    let raw = obj.get("raw").and_then(Value::as_str).map(ToString::to_string);
    Ok((out_value, PipelineContext { unit, raw }))
}

fn apply_format(cfg: &FormatConfig, value: &TagValue, ctx: &PipelineContext) -> String {
    let value_text = match value {
        TagValue::Float(v) => v.to_string(),
        TagValue::Integer(v) => v.to_string(),
        TagValue::Boolean(v) => v.to_string(),
        TagValue::String(v) => v.clone(),
    };
    let mut out = cfg.template.clone();
    out = out.replace("{value}", &value_text);
    out = out.replace("{unit}", ctx.unit.as_deref().unwrap_or(""));
    out = out.replace("{raw}", ctx.raw.as_deref().unwrap_or(""));
    if cfg.trim {
        out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::id::{DeviceId, TagId};
    use domain::tag::{QualityStatus, TagUpdateMode};
    use serde_json::json;

    fn base_tag(metadata: Value) -> Tag {
        let mut t = Tag::new(
            TagId::new("tag_1"),
            "Tag1".to_string(),
            DeviceId::new("dev_1"),
            "sim:1".to_string(),
        );
        t.update_mode = TagUpdateMode::OnMessage;
        t.metadata = metadata;
        t
    }

    #[test]
    fn pipeline_default_passthrough() {
        let tag = base_tag(json!({}));
        let p = build_pipeline_for_tag(&tag);
        let out = p.process(&tag, TagValue::Float(10.0));
        assert_eq!(out.value, TagValue::Float(10.0));
        assert_eq!(out.quality.status, QualityStatus::Good);
    }

    #[test]
    fn pipeline_applies_scale() {
        let tag = base_tag(json!({
            "pipeline": {
                "scale": {"factor": 0.1, "offset": 0.0}
            }
        }));
        let p = build_pipeline_for_tag(&tag);
        let out = p.process(&tag, TagValue::Integer(157));
        match out.value {
            TagValue::Float(v) => assert!((v - 15.7).abs() < 1e-9),
            _ => panic!("expected float output"),
        }
        assert_eq!(out.quality.status, QualityStatus::Good);
    }

    #[test]
    fn pipeline_marks_out_of_range() {
        let tag = base_tag(json!({
            "pipeline": {
                "range": {"min": 0.0, "max": 5.0}
            }
        }));
        let p = build_pipeline_for_tag(&tag);
        let out = p.process(&tag, TagValue::Float(9.0));
        assert_eq!(out.quality.status, QualityStatus::Bad);
        assert_eq!(out.quality.reason, Some(QualityReason::OutOfRange));
    }

    #[test]
    fn pipeline_extracts_compound_and_formats_display() {
        let tag = base_tag(json!({
            "pipeline": {
                "extract": "scale:compound",
                "format": "{value} {unit}",
                "trim": true
            }
        }));
        let p = build_pipeline_for_tag(&tag);
        let out = p.process(
            &tag,
            TagValue::String(r#"{"raw":"+ 12.300 g","unit":"g","value":12.3}"#.to_string()),
        );
        assert_eq!(out.value, TagValue::String("12.3 g".to_string()));
        assert_eq!(out.quality.status, QualityStatus::Good);
    }

    #[test]
    fn pipeline_extract_compound_invalid_payload_marks_bad() {
        let tag = base_tag(json!({
            "pipeline": {
                "extract": "scale:compound"
            }
        }));
        let p = build_pipeline_for_tag(&tag);
        let out = p.process(&tag, TagValue::String("not-json".to_string()));
        assert_eq!(out.quality.status, QualityStatus::Bad);
        assert_eq!(out.quality.reason, Some(QualityReason::ValidationFailed));
    }
}
