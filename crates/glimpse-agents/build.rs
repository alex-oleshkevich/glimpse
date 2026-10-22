fn main() -> shadow_rs::SdResult<()> {
    watch_git_head();
    compile_blueprint(
        "blueprints/agent_dialog.blp",
        "resources/agents/agent_dialog.ui",
    );
    glib_build_tools::compile_resources(
        &["resources"],
        "resources/glimpse-agents.gresource.xml",
        "glimpse-agents.gresource",
    );
    shadow_rs::new()
}

fn compile_blueprint(src: &str, out: &str) {
    println!("cargo:rerun-if-changed={src}");
    let output = std::process::Command::new("blueprint-compiler")
        .args(["compile", src])
        .output()
        .expect("blueprint-compiler not found");
    assert!(
        output.status.success(),
        "blueprint-compiler failed for {src}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    if std::fs::read(out).ok().as_deref() != Some(output.stdout.as_slice()) {
        std::fs::write(out, &output.stdout).unwrap();
    }
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
