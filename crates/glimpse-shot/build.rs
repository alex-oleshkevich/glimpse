fn main() -> shadow_rs::SdResult<()> {
    watch_git_head();
    shadow_rs::new()
}

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
