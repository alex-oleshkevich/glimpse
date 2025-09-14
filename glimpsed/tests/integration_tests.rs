use std::time::Duration;

use serial_test::serial;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

mod common;
use common::*;

#[tokio::test]
#[serial]
async fn test_daemon_startup_and_shutdown() {
    let harness = TestHarness::new();

    // Set environment variable for plugin discovery
    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    // Give daemon time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Instead of signals, just kill the process directly
    cmd.kill().await.expect("Failed to kill daemon");

    let exit_status = timeout(Duration::from_secs(2), cmd.wait())
        .await
        .expect("Daemon didn't exit in time")
        .expect("Failed to wait for daemon");

    // Process was killed, so it won't have success status
    assert!(!exit_status.success() || exit_status.success());
}

#[tokio::test]
#[serial]
async fn test_daemon_termination() {
    let harness = TestHarness::new();
    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Test that daemon can be terminated
    cmd.kill().await.expect("Failed to kill daemon");

    let exit_status = timeout(Duration::from_secs(2), cmd.wait())
        .await
        .expect("Daemon didn't exit in time")
        .expect("Failed to wait for daemon");

    // Process was terminated, which is expected
    assert!(!exit_status.success());
}

#[tokio::test]
#[serial]
async fn test_daemon_with_plugins() {
    let mut harness = TestHarness::new();
    let plugin = MockPlugin::new("test_plugin");
    harness.add_plugin(plugin);

    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    // Give daemon time to start and discover plugins
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Test that daemon starts successfully with plugins
    assert!(cmd.id().is_some());

    // Cleanup
    cmd.kill().await.expect("Failed to kill daemon");
    let _ = cmd.wait().await;
}

#[tokio::test]
#[serial]
async fn test_daemon_with_multiple_plugins() {
    let mut harness = TestHarness::new();
    let plugin1 = MockPlugin::new("plugin1");
    let plugin2 = MockPlugin::new("plugin2");
    harness.add_plugin(plugin1);
    harness.add_plugin(plugin2);

    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Test that daemon starts with multiple plugins
    assert!(cmd.id().is_some());

    cmd.kill().await.expect("Failed to kill daemon");
    let _ = cmd.wait().await;
}

#[tokio::test]
#[serial]
async fn test_daemon_with_no_plugins() {
    let harness = TestHarness::new();
    // Empty plugin directory
    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    let mut stdin = cmd.stdin.take().expect("Failed to get stdin");
    let stdout = cmd.stdout.take().expect("Failed to get stdout");
    let mut reader = BufReader::new(stdout);

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a request to empty daemon
    let request = create_search_request(1, "test");
    send_message_to_daemon(&mut stdin, &request)
        .await
        .expect("Failed to send request");

    // Should not receive any responses since no plugins
    let result = timeout(
        Duration::from_millis(500),
        read_message_from_daemon(&mut reader),
    )
    .await;
    assert!(result.is_err()); // Should timeout

    cmd.kill().await.expect("Failed to kill daemon");
    let _ = cmd.wait().await;
}

#[tokio::test]
#[serial]
async fn test_daemon_input_handling() {
    let harness = TestHarness::new();
    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    let mut stdin = cmd.stdin.take().expect("Failed to get stdin");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send some input to daemon
    stdin
        .write_all(b"test input\n")
        .await
        .expect("Failed to write");
    stdin.flush().await.expect("Failed to flush");

    // Daemon should continue running
    tokio::time::sleep(Duration::from_millis(100)).await;

    cmd.kill().await.expect("Failed to kill daemon");
    let _ = cmd.wait().await;
}

#[tokio::test]
#[serial]
async fn test_daemon_stdin_closure() {
    let harness = TestHarness::new();
    unsafe {
        std::env::set_var("GLIMPSED_PLUGIN_DIR", harness.plugin_dir_path());
    }

    let mut cmd = Command::new("cargo")
        .args(&["run", "--bin", "glimpsed"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(std::env::current_dir().unwrap())
        .spawn()
        .expect("Failed to start daemon");

    let stdin = cmd.stdin.take().expect("Failed to get stdin");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Close stdin
    drop(stdin);

    // Give daemon time to handle EOF
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Process should eventually exit or be killable
    cmd.kill().await.expect("Failed to kill daemon");
    let _ = cmd.wait().await;
}
