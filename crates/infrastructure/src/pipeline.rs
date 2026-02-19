use anyhow::{Result, anyhow};
use regex::Regex;

use serde_json::Value;

use domain::tag::{ParserConfig, ValidatorConfig, ValueParser, ValueValidator};

// --- Parsers ---

#[derive(Debug)]
pub struct RegexParser {
    regex: Regex,
}

impl RegexParser {
    pub fn new(pattern: &str) -> Result<Self> {
        Ok(Self {
            regex: Regex::new(pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?,
        })
    }
}

impl ValueParser for RegexParser {
    fn parse(&self, raw_value: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(captures) = self.regex.captures(raw_value) {
            if let Some(match_) = captures.get(1) {
                let val_str = match_.as_str();
                // Try to parse as number if possible, else string
                if let Ok(num) = val_str.parse::<f64>() {
                    return Ok(serde_json::json!(num));
                }
                return Ok(serde_json::json!(val_str));
            }
        }
        Err(anyhow!("No match found for regex").into())
    }
}

#[derive(Debug)]
pub struct JsonParser {
    path: String,
}

impl JsonParser {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }
}

impl ValueParser for JsonParser {
    fn parse(&self, raw_value: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let json: Value =
            serde_json::from_str(raw_value).map_err(|e| anyhow!("Invalid JSON: {}", e))?;

        if self.path.is_empty() {
            return Ok(json);
        }

        // Simple path navigation (e.g. "data.value")
        let mut current = &json;
        for part in self.path.split('.') {
            current = current
                .get(part)
                .ok_or_else(|| anyhow!("Path {} not found", part))?;
        }

        Ok(current.clone())
    }
}

// --- Validators ---

#[derive(Debug)]
pub struct RangeValidator {
    min: Option<f64>,
    max: Option<f64>,
}

impl RangeValidator {
    pub fn new(min: Option<f64>, max: Option<f64>) -> Self {
        Self { min, max }
    }
}

impl ValueValidator for RangeValidator {
    fn validate(&self, value: &Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let num = match value {
            Value::Number(n) => n.as_f64(),
            Value::Object(map) => map.get("value").and_then(|v| v.as_f64()),
            _ => None,
        }
        .ok_or_else(|| anyhow!("Value is not a number"))?;

        if let Some(min) = self.min {
            if num < min {
                return Err(anyhow!("Value {} is below minimum {}", num, min).into());
            }
        }
        if let Some(max) = self.max {
            if num > max {
                return Err(anyhow!("Value {} is above maximum {}", num, max).into());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ContainsValidator {
    substring: String,
}

impl ContainsValidator {
    pub fn new(substring: &str) -> Self {
        Self {
            substring: substring.to_string(),
        }
    }
}

impl ValueValidator for ContainsValidator {
    fn validate(&self, value: &Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let s = value
            .as_str()
            .ok_or_else(|| anyhow!("Value is not a string"))?;
        if !s.contains(&self.substring) {
            return Err(anyhow!(
                "Value does not contain required substring '{}'",
                self.substring
            )
            .into());
        }
        Ok(())
    }
}

// --- Custom Parsers ---
// (Moved below for organization)

#[derive(Debug)]
pub struct IndexMapParser {
    keys: Vec<String>,
    scale: Option<f64>,
}

impl IndexMapParser {
    pub fn new(keys: Vec<String>, scale: Option<f64>) -> Self {
        Self { keys, scale }
    }
}

impl ValueParser for IndexMapParser {
    fn parse(&self, raw_value: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        // The raw_value from Driver might be a JSON string of an array
        // Driver returns Option<Value>, but Executor converts to string?
        // Wait, TagExecutor calls parser with `raw_string`.
        // If driver returns `Value::Array`, Executor converts it to string representation: "[1, 2]"
        // So we need to parse it back to JSON to work with it.

        // 1. Parse raw string as JSON
        let json: Value = serde_json::from_str(raw_value)
            .map_err(|e| anyhow!("IndexMapParser input must be valid JSON: {}", e))?;

        // 2. Expect Array
        let arr = json
            .as_array()
            .ok_or_else(|| anyhow!("IndexMapParser input must be a JSON Array"))?;

        // 3. Map keys
        let mut map = serde_json::Map::new();
        for (i, key) in self.keys.iter().enumerate() {
            if let Some(val) = arr.get(i) {
                let mut value_to_insert = val.clone();

                // Apply scaling if configured and value is a number
                if let Some(scale) = self.scale {
                    if let Some(num) = val.as_f64() {
                        let scaled = num * scale;
                        // Use number_from_f64 to avoid NaN/Infinite issues if any
                        if let Some(n) = serde_json::Number::from_f64(scaled) {
                            value_to_insert = Value::Number(n);
                        }
                    }
                }

                map.insert(key.clone(), value_to_insert);
            } else {
                // If array is shorter, maybe null or skip?
                // Let's set null to indicate missing data
                map.insert(key.clone(), Value::Null);
            }
        }

        Ok(Value::Object(map))
    }
}

// --- Factory ---

pub struct PipelineFactory;

impl PipelineFactory {
    pub fn create_parser(config: &ParserConfig) -> Result<Box<dyn ValueParser>> {
        match config {
            ParserConfig::Regex { pattern } => Ok(Box::new(RegexParser::new(pattern)?)),
            ParserConfig::Json { path } => Ok(Box::new(JsonParser::new(path))),
            ParserConfig::IndexMap { keys, scale } => {
                Ok(Box::new(IndexMapParser::new(keys.clone(), *scale)))
            }
            ParserConfig::None => Err(anyhow!("No parser configured").into()),
            ParserConfig::Custom { name, .. } => match name.as_str() {
                "ScaleParser" => Ok(Box::new(ScaleParser::new())),
                _ => Err(anyhow!("Custom parser '{}' not implemented", name).into()),
            },
        }
    }

    pub fn create_validator(config: &ValidatorConfig) -> Result<Box<dyn ValueValidator>> {
        match config {
            ValidatorConfig::Range { min, max } => Ok(Box::new(RangeValidator::new(*min, *max))),
            ValidatorConfig::Contains { substring } => {
                Ok(Box::new(ContainsValidator::new(substring)))
            }
            ValidatorConfig::Custom { name, .. } => {
                Err(anyhow!("Custom validator '{}' not implemented", name).into())
            }
        }
    }
}

// --- Custom Parsers ---

#[derive(Debug)]
pub struct ScaleParser {
    regex: Regex,
}

impl ScaleParser {
    pub fn new() -> Self {
        // Regex to match a floating point number
        // Matches:
        // - Optional sign [-+]
        // - Integer part [0-9]*
        // - Optional decimal part \.?[0-9]+
        // - Optional exponent (?:[eE][-+]?[0-9]+)?
        //
        // NOTE: We don't use ^ and $ because we search within the string
        let regex =
            Regex::new(r"([-+]?[0-9]*\.?[0-9]+(?:[eE][-+]?[0-9]+)?)").expect("Invalid regex");
        Self { regex }
    }
    fn find_number_start(s: &str) -> Option<usize> {
        s.char_indices()
            .find(|(_, c)| c.is_ascii_digit() || *c == '+' || *c == '-' || *c == '.')
            .map(|(i, _)| i)
    }
}

impl ValueParser for ScaleParser {
    fn parse(&self, raw_value: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Basic clean
        let s = raw_value.trim().replace('\u{00A0}', "");
        if s.is_empty() {
            return Err(anyhow!("Empty input").into());
        }

        // 2. Find start of number to skip prefixes (ST,GS, etc)
        let start = match Self::find_number_start(&s) {
            Some(i) => i,
            None => return Err(anyhow!("No numeric value found").into()),
        };

        // 3. Slice and normalize
        let rest = s[start..].trim().replace(',', ".").replace(' ', "");

        // 4. Extract numeric token using Regex
        let captures = self
            .regex
            .captures(&rest)
            .ok_or_else(|| anyhow!("No numeric value found"))?;

        let num_full_match = captures.get(1).unwrap();
        let num_str = num_full_match.as_str();
        let end = num_full_match.end();

        let unit_str = rest[end..].trim();

        // 5. Parse
        let value: f64 = num_str
            .parse()
            .map_err(|_| anyhow!("Invalid number format: '{}'", num_str))?;

        // 7. Unit must exist
        // Rustscada tests expect "kg", "g".
        // if unit_str.is_empty() { return Err(...) }
        // Let's keep it strict as per rustscada.
        // Actually, if unit is empty, we might want to return just the value?
        // User said: "configurar un tag con resultado compuesto, valor + unidad" and "reutilizar la logica".
        // Rustscada returns error if unit is empty.
        if unit_str.is_empty() {
            return Err(anyhow!("No unit found").into());
        }

        Ok(serde_json::json!({
            "value": value,
            "unit": unit_str
        }))
    }
}
