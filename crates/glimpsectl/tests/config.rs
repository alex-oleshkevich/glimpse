use std::process::Command;

#[test]
fn semantic_problems_fail_validate_and_doctor() {
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
        assert_eq!(output.status.code(), Some(1), "{output:?}");
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
    assert_eq!(output.status.code(), Some(1));
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

#[test]
fn a_clamped_night_light_temperature_fails_validate_and_doctor() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, "[night-light]\ntemperature = 9000\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
            .arg("--config")
            .arg(&path)
            .args(args)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/nonexistent/glimpse-test-bus",
            )
            .output()
            .unwrap()
    };
    let output = run(&["config", "validate"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("night-light.temperature: 9000") && stdout.contains("using 6500"),
        "{stdout}"
    );

    let output = run(&["doctor", "--json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let diagnosis: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diagnosis["configuration"]["loads"], true);
    let warnings = diagnosis["configuration"]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0]
            .as_str()
            .unwrap()
            .starts_with("night-light.temperature: 9000"),
        "{warnings:?}"
    );
}

#[test]
fn validate_json_is_parseable_and_errors_stay_on_stderr() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(
        &path,
        "[night-light]\nstart-time = \"20:00\"\nend-time = \"20:00\"\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["config", "validate", "--json", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], false);
    assert_eq!(report["problems"].as_array().unwrap().len(), 1);
    let stderr = String::from_utf8(output.stderr).unwrap();
    let error: serde_json::Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
    assert_eq!(error["exit"], 1);
    std::fs::write(&path, "").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_glimpsectl"))
        .args(["config", "validate", "--json", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["valid"], true);
}
