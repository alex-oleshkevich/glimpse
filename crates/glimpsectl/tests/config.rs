use std::process::Command;

#[test]
fn semantic_problems_are_warnings_in_validate_and_doctor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(
        &path,
        "[night-light]\nstart-time = \"20:00\"\nend-time = \"20:00\"\n",
    )
    .unwrap();
    for args in [vec!["config", "validate"], vec!["doctor"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .arg("--config")
            .arg(&path)
            .args(args)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent/glimpse-test-bus",
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.contains("start-time and night-light.end-time must differ"),
            "{stdout}"
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["doctor", "--json", "--config"])
        .arg(&path)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/glimpse-test-bus",
        )
        .output()
        .unwrap();
    assert!(output.status.success());
    let diagnosis: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diagnosis["configuration"]["loads"], true);
    assert_eq!(
        diagnosis["configuration"]["warnings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
