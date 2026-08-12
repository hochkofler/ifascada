use std::process::Command;

/// Stamps the build with the short git commit SHA (and a dirty-tree marker)
/// so a running `edge-agent.exe` can report exactly which source it was
/// built from. Without this, a deployed binary is a gitignored, opaque blob
/// with no way to confirm whether it matches any given point in the repo.
fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let build_id = if dirty {
        format!("{sha}-dirty")
    } else {
        sha
    };

    println!("cargo:rustc-env=EDGE_AGENT_GIT_SHA={build_id}");
    // Re-run if HEAD moves (checkout/commit), not on every unrelated file touch.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
