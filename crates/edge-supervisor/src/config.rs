//! Configuration for the supervisor, read from the same `edge.env` the agent already uses.
//!
//! Built from a map rather than from `std::env` directly so it can be tested without
//! mutating process-global state, which is racy across parallel tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// How long central holds an unanswered long-poll before replying empty.
pub const DEFAULT_WAIT_SECS: u64 = 25;

/// How long to wait before relaunching an agent that exited. Matches what `run-edge.ps1`
/// has always done, so the supervisor is not a behaviour change in this respect.
pub const DEFAULT_RESTART_DELAY_SECS: u64 = 5;

/// Reads an `edge.env` into key/value pairs, accepting exactly what `run-edge.ps1`
/// accepted: comments and lines with no `=` are skipped, and only the first `=` splits.
pub fn parse_env_file(content: &str) -> HashMap<String, String> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (k, v) = line.split_once('=')?;
            let k = k.trim();
            if k.is_empty() {
                return None;
            }
            Some((k.to_string(), v.trim().to_string()))
        })
        .collect()
}

/// Everything needed to ask central for orders. Absent when the host has not been given
/// central's address or a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlConfig {
    pub base_url: String,
    pub enroll_token: String,
    pub edge_id: String,
    pub wait: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorConfig {
    pub agent_path: PathBuf,
    pub restart_delay: Duration,
    /// Where the agent's stdout and stderr are appended. `None` discards them.
    pub log_dir: Option<PathBuf>,
    /// `None` means remote control is disabled and the supervisor only babysits the child.
    pub control: Option<ControlConfig>,
}

impl SupervisorConfig {
    pub fn from_vars(vars: &HashMap<String, String>, default_agent_path: PathBuf) -> Self {
        let get = |k: &str| vars.get(k).map(|v| v.trim()).filter(|v| !v.is_empty());

        let wait = get("EDGE_SUPERVISOR_WAIT_SECS")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_WAIT_SECS);

        // All three are required together: an address with no token cannot authenticate,
        // and neither can identify this edge without its code. Any one missing disables
        // remote control -- it never aborts the supervisor.
        let control = match (
            get("EDGE_CONFIG_URL"),
            get("EDGE_ENROLL_TOKEN"),
            get("EDGE_AGENT"),
        ) {
            (Some(base_url), Some(enroll_token), Some(edge_id)) => Some(ControlConfig {
                base_url: base_url.trim_end_matches('/').to_string(),
                enroll_token: enroll_token.to_string(),
                edge_id: edge_id.to_string(),
                wait: Duration::from_secs(wait),
            }),
            _ => None,
        };

        SupervisorConfig {
            agent_path: get("EDGE_SUPERVISOR_AGENT_PATH")
                .map(PathBuf::from)
                .unwrap_or(default_agent_path),
            restart_delay: Duration::from_secs(DEFAULT_RESTART_DELAY_SECS),
            log_dir: get("EDGE_SUPERVISOR_LOG_DIR").map(PathBuf::from),
            control,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn full() -> Vec<(&'static str, &'static str)> {
        vec![
            ("EDGE_CONFIG_URL", "http://192.168.103.154:8088"),
            ("EDGE_ENROLL_TOKEN", "s3cret"),
            ("EDGE_AGENT", "lcc01"),
        ]
    }

    #[test]
    fn control_is_enabled_when_every_variable_is_present() {
        let cfg = SupervisorConfig::from_vars(&vars(&full()), PathBuf::from("edge-agent.exe"));
        assert_eq!(
            cfg.control,
            Some(ControlConfig {
                base_url: "http://192.168.103.154:8088".to_string(),
                enroll_token: "s3cret".to_string(),
                edge_id: "lcc01".to_string(),
                wait: Duration::from_secs(DEFAULT_WAIT_SECS),
            })
        );
    }

    /// Losing the control channel must not cost the agent: a host with no central address
    /// still gets its child launched and relaunched. Trading a 25-minute outage for a
    /// permanent one would be a worse system than the one being fixed.
    #[test]
    fn supervision_survives_a_missing_central_url() {
        let mut pairs = full();
        pairs.retain(|(k, _)| *k != "EDGE_CONFIG_URL");
        let cfg = SupervisorConfig::from_vars(&vars(&pairs), PathBuf::from("edge-agent.exe"));

        assert_eq!(cfg.control, None, "remote control must be disabled");
        assert_eq!(
            cfg.agent_path,
            PathBuf::from("edge-agent.exe"),
            "the child must still be configured"
        );
    }

    #[test]
    fn supervision_survives_a_missing_enroll_token() {
        let mut pairs = full();
        pairs.retain(|(k, _)| *k != "EDGE_ENROLL_TOKEN");
        let cfg = SupervisorConfig::from_vars(&vars(&pairs), PathBuf::from("edge-agent.exe"));
        assert_eq!(cfg.control, None);
    }

    #[test]
    fn supervision_survives_a_missing_edge_id() {
        let mut pairs = full();
        pairs.retain(|(k, _)| *k != "EDGE_AGENT");
        let cfg = SupervisorConfig::from_vars(&vars(&pairs), PathBuf::from("edge-agent.exe"));
        assert_eq!(cfg.control, None);
    }

    #[test]
    fn an_explicit_agent_path_overrides_the_default() {
        let mut pairs = full();
        pairs.push(("EDGE_SUPERVISOR_AGENT_PATH", "C:\\ifascada\\edge-agent.exe"));
        let cfg = SupervisorConfig::from_vars(&vars(&pairs), PathBuf::from("edge-agent.exe"));
        assert_eq!(cfg.agent_path, PathBuf::from("C:\\ifascada\\edge-agent.exe"));
    }

    #[test]
    fn the_wait_is_configurable_and_falls_back_to_the_default() {
        let mut pairs = full();
        pairs.push(("EDGE_SUPERVISOR_WAIT_SECS", "5"));
        let cfg = SupervisorConfig::from_vars(&vars(&pairs), PathBuf::from("edge-agent.exe"));
        assert_eq!(cfg.control.unwrap().wait, Duration::from_secs(5));

        let mut junk = full();
        junk.push(("EDGE_SUPERVISOR_WAIT_SECS", "not-a-number"));
        let cfg = SupervisorConfig::from_vars(&vars(&junk), PathBuf::from("edge-agent.exe"));
        assert_eq!(
            cfg.control.unwrap().wait,
            Duration::from_secs(DEFAULT_WAIT_SECS),
            "an unparseable value must fall back, not disable control"
        );
    }

    /// `run-edge.ps1` wrote the agent's output to edge.out.log / edge.err.log, and
    /// `scripts/edge-diagnose-and-restart.ps1` reads edge.err.log to report recent port
    /// and protocol errors. Dropping those files would quietly break the diagnosis path.
    #[test]
    fn the_log_directory_is_carried_through_when_configured() {
        let mut pairs = full();
        pairs.push(("EDGE_SUPERVISOR_LOG_DIR", "C:/ProgramData/ifascada/edge/logs"));
        let cfg = SupervisorConfig::from_vars(&vars(&pairs), PathBuf::from("edge-agent.exe"));
        assert_eq!(
            cfg.log_dir,
            Some(PathBuf::from("C:/ProgramData/ifascada/edge/logs"))
        );
    }

    #[test]
    fn without_a_log_directory_the_agent_output_is_discarded_rather_than_lost_in_a_console() {
        let cfg = SupervisorConfig::from_vars(&vars(&full()), PathBuf::from("edge-agent.exe"));
        assert_eq!(cfg.log_dir, None);
    }


    /// `run-edge.ps1` loaded `edge.env` into the process before launching the agent, and
    /// the agent reads all of its configuration from there. The supervisor is the launcher
    /// now, so it has to do the same or the agent starts with nothing.
    ///
    /// The accepted shape matches what the PowerShell did: skip comments and any line
    /// without an `=`, split on the first `=` only.
    #[test]
    fn an_env_file_is_parsed_the_way_run_edge_ps1_parsed_it() {
        let content = "# la config del edge
EDGE_AGENT=lcc01

MQTT_HOST=127.0.0.1
  EDGE_SITE = plant-a
MQTT_OUTBOX_ENCRYPTION_SECRET=abc=def==
esta linea no tiene igual
   # comentario indentado
";
        let parsed = parse_env_file(content);

        assert_eq!(parsed.get("EDGE_AGENT").map(String::as_str), Some("lcc01"));
        assert_eq!(parsed.get("MQTT_HOST").map(String::as_str), Some("127.0.0.1"));
        assert_eq!(
            parsed.get("EDGE_SITE").map(String::as_str),
            Some("plant-a"),
            "keys and values are trimmed"
        );
        assert_eq!(
            parsed.get("MQTT_OUTBOX_ENCRYPTION_SECRET").map(String::as_str),
            Some("abc=def=="),
            "only the first = separates; secrets often end in padding"
        );
        assert_eq!(parsed.len(), 4, "comments and junk lines must be skipped");
    }

}
