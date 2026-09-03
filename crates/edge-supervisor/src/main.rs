//! Entry point. This is what the Windows scheduled task and the systemd unit launch,
//! replacing `run-edge.ps1`.

use anyhow::Result;
use edge_supervisor::child::{AgentChild, ChildSpec};
use edge_supervisor::config::{env_file_from_args, parse_env_file, SupervisorConfig};
use edge_supervisor::control::ControlClient;
use edge_supervisor::supervisor::{HeartbeatWatch, Supervisor, DEFAULT_CHILD_WATCH_INTERVAL};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let vars = collect_vars();
    let cfg = SupervisorConfig::from_vars(&vars, default_agent_path());

    let mut spec = ChildSpec::new(&cfg.agent_path, Vec::<OsString>::new());
    spec.env = vars
        .iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect();
    if let Some(dir) = &cfg.log_dir {
        spec.out_log = Some(dir.join("edge.out.log"));
        spec.err_log = Some(dir.join("edge.err.log"));
    }

    let control = match &cfg.control {
        Some(c) => {
            info!(
                "remote control enabled: edge={} central={} wait={}s",
                c.edge_id,
                c.base_url,
                c.wait.as_secs()
            );
            Some(ControlClient::new(c.clone())?)
        }
        None => {
            // Deliberately not fatal. Losing the control channel must not cost the agent.
            warn!(
                "remote control DISABLED: EDGE_CONFIG_URL, EDGE_ENROLL_TOKEN and EDGE_AGENT \
                 must all be set. The agent will still be launched and relaunched."
            );
            None
        }
    };

    info!(
        "wedge detection: heartbeat at {} (stale after {}s, {}s of grace after each launch)",
        cfg.heartbeat.path.display(),
        cfg.heartbeat.stale_after.as_secs(),
        cfg.heartbeat.grace.as_secs()
    );

    let mut supervisor = Supervisor::new(
        AgentChild::new(spec),
        control,
        cfg.restart_delay,
        DEFAULT_CHILD_WATCH_INTERVAL,
        Some(HeartbeatWatch {
            path: cfg.heartbeat.path.clone(),
            stale_after: cfg.heartbeat.stale_after,
            grace: cfg.heartbeat.grace,
        }),
    );

    info!("launching agent {}", cfg.agent_path.display());
    supervisor.start()?;
    supervisor.run().await
}

/// The process environment, with `edge.env` layered on top when one is configured.
///
/// The file wins over the inherited environment because that is what `run-edge.ps1` did:
/// it wrote every line of `edge.env` into the process before launching the agent.
fn collect_vars() -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = std::env::vars().collect();
    // The command line wins: it is the only channel a Windows scheduled task has.
    let path = match env_file_from_args(std::env::args()) {
        Some(p) => p.to_string_lossy().into_owned(),
        None => match vars.get("EDGE_SUPERVISOR_ENV_FILE") {
            Some(p) => p.clone(),
            None => return vars,
        },
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let from_file = parse_env_file(&content);
            info!("loaded {} variables from {}", from_file.len(), path);
            vars.extend(from_file);
        }
        // Not fatal for the same reason as above: an agent launched with the inherited
        // environment has a chance of working; one never launched has none.
        Err(e) => warn!("could not read {}: {}", path, e),
    }
    vars
}

/// `edge-agent` next to the supervisor, which is how the release package lays them out.
fn default_agent_path() -> PathBuf {
    let name = if cfg!(windows) {
        "edge-agent.exe"
    } else {
        "edge-agent"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}
