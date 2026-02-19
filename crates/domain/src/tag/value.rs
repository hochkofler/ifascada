use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::TagQuality;

/// The "Golden Record" event for a Tag's value change.
/// This struct is the result of the Acquisition Pipeline and is strictly typed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TagValue {
    /// Logical path of the tag (e.g., "site/area/line/equipment/signal")
    pub tag_id: String,
    /// The processed engineering value (or raw if no transform)
    pub value: Value,
    /// Quality of the value (Good, Bad, Uncertain, etc.)
    pub quality: TagQuality,
    /// Timestamp of acquisition (source time if available, else edge arrival time)
    pub timestamp: DateTime<Utc>,
}

impl TagValue {
    /// Creates a new TagValue event.
    pub fn new(
        tag_id: String,
        value: Value,
        quality: TagQuality,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            tag_id,
            value,
            quality,
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tag_value_creation() {
        let now = Utc::now();
        let val = TagValue::new(
            "plant1/tankA/pressure".to_string(),
            json!(15.5),
            TagQuality::Good,
            now,
        );

        assert_eq!(val.tag_id, "plant1/tankA/pressure");
        assert_eq!(val.value, json!(15.5));
        assert_eq!(val.quality, TagQuality::Good);
        assert_eq!(val.timestamp, now);
    }

    #[test]
    fn test_tag_value_serialization() {
        let now = Utc::now();
        let val = TagValue::new("test/tag".to_string(), json!(true), TagQuality::Bad, now);

        let serialized = serde_json::to_string(&val).unwrap();
        let deserialized: TagValue = serde_json::from_str(&serialized).unwrap();

        assert_eq!(val, deserialized);
    }
}
