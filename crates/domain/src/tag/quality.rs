use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityStatus {
    Good,
    Bad,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityReason {
    Timeout,
    CommunicationFailure,
    OutOfRange,
    ValidationFailed,
    NotConnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagQuality {
    pub status: QualityStatus,
    pub reason: Option<QualityReason>,
}

impl TagQuality {
    pub fn good() -> Self {
        Self {
            status: QualityStatus::Good,
            reason: None,
        }
    }

    pub fn bad(reason: QualityReason) -> Self {
        Self {
            status: QualityStatus::Bad,
            reason: Some(reason),
        }
    }
}
