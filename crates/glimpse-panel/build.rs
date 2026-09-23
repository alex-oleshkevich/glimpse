fn main() -> shadow_rs::SdResult<()> {
    watch_git_head();
    shadow_rs::new()
}

/// shadow-rs emits no `rerun-if-changed` of its own, so cargo's default rule applies: rerun the
/// build script only when a file inside this crate changes. A commit that touches only another
/// crate then leaves the embedded commit hash and build time stale, even though the binary
/// relinks against the changed dependency.
fn watch_git_head() {
    let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "logs/HEAD"])
        .output()
    else {
        return;
    };
    if output.status.success() {
        println!(
            "cargo:rerun-if-changed={}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
    }
}
