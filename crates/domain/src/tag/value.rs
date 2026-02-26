use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TagValue {
    Float(f64),
    Integer(i64),
    Boolean(bool),
    String(String),
}

impl From<f64> for TagValue {
    fn from(v: f64) -> Self {
        Self::Float(v)
    }
}

impl From<bool> for TagValue {
    fn from(v: bool) -> Self {
        Self::Boolean(v)
    }
}

impl From<serde_json::Value> for TagValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Integer(i)
                } else {
                    Self::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::Bool(b) => Self::Boolean(b),
            serde_json::Value::String(s) => Self::String(s),
            _ => Self::String(v.to_string()),
        }
    }
}

impl fmt::Display for TagValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Float(v) => write!(f, "{}", v),
            Self::Integer(v) => write!(f, "{}", v),
            Self::Boolean(v) => write!(f, "{}", v),
            Self::String(v) => write!(f, "{}", v),
        }
    }
}
