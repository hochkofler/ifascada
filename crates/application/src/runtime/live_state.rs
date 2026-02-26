use chrono::{DateTime, Utc};
use dashmap::DashMap;
use domain::id::TagId;
use domain::tag::{TagQuality, TagValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagState {
    pub value: TagValue,
    pub trigger_value: Option<TagValue>,
    pub quality: TagQuality,
    pub timestamp: DateTime<Utc>,
}

pub struct LiveState {
    tags: DashMap<TagId, TagState>,
}

impl LiveState {
    pub fn new() -> Self {
        Self {
            tags: DashMap::new(),
        }
    }

    pub fn update_tag(&self, tag_id: TagId, value: TagValue, quality: TagQuality) -> TagState {
        let state = TagState {
            value,
            trigger_value: None,
            quality,
            timestamp: Utc::now(),
        };
        self.tags.insert(tag_id, state.clone());
        state
    }

    pub fn get_tag(&self, tag_id: &TagId) -> Option<TagState> {
        self.tags.get(tag_id).map(|r| r.value().clone())
    }
}
