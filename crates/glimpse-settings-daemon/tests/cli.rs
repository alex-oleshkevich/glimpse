use std::process::Command;

#[test]
fn help_and_version_work_without_a_session_bus() {
    let binary = env!("CARGO_BIN_EXE_glimpse-settings-daemon");
    for argument in ["--help", "--version"] {
        let output = Command::new(binary)
            .arg(argument)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .output()
            .expect("the binary runs");
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).expect("UTF-8 output");
        match argument {
            "--help" => assert!(text.contains("Usage: glimpse-settings-daemon")),
            "--version" => assert_eq!(
                text.trim(),
                concat!("glimpse-settings-daemon ", env!("CARGO_PKG_VERSION"))
            ),
            _ => unreachable!(),
        }
    }

    let output = Command::new(binary)
        .arg("--unknown")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));
}
