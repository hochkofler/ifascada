//! Configuración de la purga periódica de `operational_events`.

/// Parámetros de la purga, leídos de `CENTRAL_OPS_EVENTS_*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpsEventsCleanupConfig {
    pub enabled: bool,
    pub retention_days: i64,
    pub interval_secs: u64,
}

impl OpsEventsCleanupConfig {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let retention_days = lookup("CENTRAL_OPS_EVENTS_RETENTION_DAYS")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(90)
            .max(1);
        let interval_secs = lookup("CENTRAL_OPS_EVENTS_CLEANUP_INTERVAL_SECS")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(3600)
            .max(60);
        let enabled = lookup("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);
        Self {
            enabled,
            retention_days,
            interval_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config(vars: &[(&str, &str)]) -> OpsEventsCleanupConfig {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        OpsEventsCleanupConfig::from_lookup(|key| vars.get(key).cloned())
    }

    #[test]
    fn cleanup_is_off_unless_explicitly_enabled() {
        // operational_events es la base del audit trail: borrar debe ser una decisión explícita.
        assert!(!config(&[]).enabled);
    }

    #[test]
    fn cleanup_runs_when_explicitly_enabled() {
        assert!(config(&[("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED", "true")]).enabled);
        assert!(config(&[("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED", "TRUE")]).enabled);
        assert!(config(&[("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED", "1")]).enabled);
    }

    #[test]
    fn cleanup_stays_off_for_any_other_value() {
        assert!(!config(&[("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED", "false")]).enabled);
        assert!(!config(&[("CENTRAL_OPS_EVENTS_CLEANUP_ENABLED", "yes")]).enabled);
    }

    #[test]
    fn retention_and_interval_keep_their_defaults_and_floors() {
        let defaults = config(&[]);
        assert_eq!(defaults.retention_days, 90);
        assert_eq!(defaults.interval_secs, 3600);

        let floored = config(&[
            ("CENTRAL_OPS_EVENTS_RETENTION_DAYS", "0"),
            ("CENTRAL_OPS_EVENTS_CLEANUP_INTERVAL_SECS", "5"),
        ]);
        assert_eq!(floored.retention_days, 1);
        assert_eq!(floored.interval_secs, 60);
    }
}
