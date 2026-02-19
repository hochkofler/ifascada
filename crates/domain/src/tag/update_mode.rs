use serde::{Deserialize, Serialize};

/// How tag values should be updated
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TagUpdateMode {
    /// Event-driven: only report when data changes
    OnChange {
        /// Min time between updates (ms)
        #[serde(default = "default_debounce")]
        debounce_ms: u64,
        /// Mark offline if no data (ms)
        #[serde(default = "default_timeout")]
        timeout_ms: u64,
    },

    /// Periodic polling
    Polling {
        /// Read interval (ms)
        interval_ms: u64,
    },

    /// Poll but only report significant changes
    PollingOnChange {
        /// Poll interval (ms)
        interval_ms: u64,
        /// Minimum change to report
        change_threshold: f64,
    },
}

impl TagUpdateMode {
    /// Get timeout for this mode
    pub fn timeout_ms(&self) -> u64 {
        match self {
            Self::OnChange { timeout_ms, .. } => *timeout_ms,
            Self::Polling { interval_ms } => interval_ms * 3,
            Self::PollingOnChange { interval_ms, .. } => interval_ms * 3,
        }
    }

    pub fn is_continuous(&self) -> bool {
        matches!(self, Self::OnChange { .. })
    }

    pub fn is_polling(&self) -> bool {
        matches!(self, Self::Polling { .. } | Self::PollingOnChange { .. })
    }
}

fn default_debounce() -> u64 {
    100
}

fn default_timeout() -> u64 {
    5000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_change_timeout() {
        let mode = TagUpdateMode::OnChange {
            debounce_ms: 100,
            timeout_ms: 30000,
        };
        assert_eq!(mode.timeout_ms(), 30000);
        assert!(mode.is_continuous());
        assert!(!mode.is_polling());
    }

    #[test]
    fn test_polling_timeout() {
        let mode = TagUpdateMode::Polling { interval_ms: 5000 };
        assert_eq!(mode.timeout_ms(), 15000);
        assert!(!mode.is_continuous());
        assert!(mode.is_polling());
    }

    #[test]
    fn test_polling_on_change() {
        let mode = TagUpdateMode::PollingOnChange {
            interval_ms: 10000,
            change_threshold: 0.5,
        };
        assert_eq!(mode.timeout_ms(), 30000);
        assert!(!mode.is_continuous());
        assert!(mode.is_polling());
    }
}
