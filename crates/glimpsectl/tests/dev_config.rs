use std::time::Duration;

use anyhow::Result;
use glimpse_dbus::testing::PrivateBus;

#[tokio::test]
async fn dev_panel_ignores_ambient_config_but_preserves_explicit_config() -> Result<()> {
    let root = tempfile::tempdir()?;
    let cli = root.path().join("glimpsectl");
    std::fs::copy(env!("CARGO_BIN_EXE_glimpsectl"), &cli)?;
    let runtime = root.path().join("runtime");
    std::fs::create_dir(&runtime)?;
    let ambient = root.path().join("ambient");
    let explicit = root.path().join("explicit");
    for dir in [&ambient, &explicit] {
        std::fs::create_dir(dir)?;
        std::fs::write(dir.join("config.toml"), "invalid = [")?;
    }
    let explicit = explicit.to_str().unwrap();
    let bus = PrivateBus::start();
    for (args, expected, unexpected) in [
        (vec!["dev", "panel"], "glimpse-panel", "ambient"),
        (
            vec!["--config-dir", explicit, "dev", "panel"],
            "explicit",
            "ambient",
        ),
        (
            vec!["dev", "panel", "--config-dir", explicit],
            "explicit",
            "ambient",
        ),
        (vec!["config", "validate"], "ambient", "glimpse-panel"),
    ] {
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::process::Command::new(&cli)
                .args(args)
                .env("GLIMPSE_CONFIG_DIR", &ambient)
                .env("XDG_CONFIG_HOME", root.path().join("home"))
                .env("DBUS_SESSION_BUS_ADDRESS", bus.address())
                .env("XDG_RUNTIME_DIR", &runtime)
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains(expected), "{stderr}");
        assert!(!stderr.contains(unexpected), "{stderr}");
    }
    Ok(())
}
