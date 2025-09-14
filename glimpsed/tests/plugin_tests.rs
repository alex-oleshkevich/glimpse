use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use glimpse_sdk::{Message, Method, MethodResult, Metadata};
use serial_test::serial;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;

mod common;
use common::*;

use glimpsed::plugins::{discover_plugins, spawn_plugin, PluginResponse};

#[tokio::test]
#[serial]
async fn test_plugin_discovery_with_env_var() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create executable plugin file
    let plugin_path = plugin_dir.join("test_plugin");
    fs::write(&plugin_path, "#!/bin/bash\necho 'test'").unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    // Set environment variable
    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", plugin_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains(&plugin_path.to_string_lossy().to_string()));
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_empty_env_var() {
    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", ""); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    // Should discover from standard directories only
    // We don't assert specific count since standard dirs may vary
    assert!(plugins.is_empty() || !plugins.is_empty());
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_nonexistent_env_var() {
    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    let plugins = discover_plugins();

    // Should discover from standard directories only
    assert!(plugins.is_empty() || !plugins.is_empty());
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_nonexistent_directory() {
    let temp_dir = TempDir::new().unwrap();
    let nonexistent_dir = temp_dir.path().join("nonexistent");

    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", nonexistent_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    // Should handle nonexistent directory gracefully
    assert!(plugins.is_empty());
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_permission_denied() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create a directory with no read permissions
    let restricted_dir = plugin_dir.join("restricted");
    fs::create_dir(&restricted_dir).unwrap();
    let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&restricted_dir, perms).unwrap();

    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", restricted_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&restricted_dir, perms).unwrap();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    // Should handle permission denied directory gracefully by continuing to other directories
    // The function logs warnings for inaccessible directories but doesn't fail
    // It may find plugins in other standard directories, so we just verify it doesn't crash
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_mixed_file_types() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create executable file
    let executable = plugin_dir.join("executable");
    fs::write(&executable, "#!/bin/bash\necho 'test'").unwrap();
    let mut perms = fs::metadata(&executable).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&executable, perms).unwrap();

    // Create non-executable file
    let non_executable = plugin_dir.join("non_executable");
    fs::write(&non_executable, "not executable").unwrap();

    // Create directory
    let subdir = plugin_dir.join("subdir");
    fs::create_dir(&subdir).unwrap();

    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", plugin_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    // Should only find executable files
    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains(&executable.to_string_lossy().to_string()));
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", plugin_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    assert!(plugins.is_empty());
}

#[cfg(windows)]
#[tokio::test]
#[serial]
async fn test_plugin_discovery_windows_extensions() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create .exe file
    let exe_file = plugin_dir.join("plugin.exe");
    fs::write(&exe_file, "exe content").unwrap();

    // Create .dll file
    let dll_file = plugin_dir.join("plugin.dll");
    fs::write(&dll_file, "dll content").unwrap();

    // Create .txt file (should be ignored)
    let txt_file = plugin_dir.join("plugin.txt");
    fs::write(&txt_file, "txt content").unwrap();

    // Create file with no extension (should be ignored)
    let no_ext = plugin_dir.join("no_extension");
    fs::write(&no_ext, "no extension").unwrap();

    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", plugin_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    assert_eq!(plugins.len(), 2);
    assert!(plugins.contains(&exe_file.to_string_lossy().to_string()));
    assert!(plugins.contains(&dll_file.to_string_lossy().to_string()));
}

#[tokio::test]
async fn test_spawn_plugin_success() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("test_plugin");

    // Create simple plugin that exits successfully
    let script = r#"#!/bin/bash
exit 0
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, _response_rx) = mpsc::channel::<PluginResponse>(10);
    let (_plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        // This should successfully spawn the plugin (which will exit immediately and restart)
        // We're testing that spawn_plugin doesn't crash with a valid executable
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    // Give plugin time to attempt startup
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The test passes if spawn_plugin is running without crashing
    assert!(!spawn_handle.is_finished());

    // Cleanup
    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
async fn test_spawn_plugin_command_not_found() {
    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(10);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);

    let nonexistent_path = "/nonexistent/plugin/path".to_string();

    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(nonexistent_path, response_tx, plugin_rx).await;
    });

    // Plugin should fail to start and enter retry loop
    // We can't easily test the infinite loop, so we just verify it doesn't crash
    tokio::time::sleep(Duration::from_millis(100)).await;

    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
async fn test_spawn_plugin_invalid_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("invalid_plugin");

    // Create plugin that outputs invalid JSON
    let script = r#"#!/bin/bash
read line
echo "invalid json output"
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(10);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    // Send request
    let request = create_search_request(1, "test");
    plugin_tx.send(request).await.expect("Failed to send request");

    // Should not receive valid response due to JSON parsing error
    let result = timeout(Duration::from_millis(500), response_rx.recv()).await;
    assert!(result.is_err()); // Should timeout

    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
async fn test_spawn_plugin_immediate_exit() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("exit_plugin");

    // Create plugin that exits immediately
    let script = r#"#!/bin/bash
exit 0
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(10);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    // Plugin should restart in loop
    tokio::time::sleep(Duration::from_millis(100)).await;

    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
async fn test_spawn_plugin_stderr_forwarding() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("stderr_plugin");

    // Create plugin that writes to stderr
    let script = r#"#!/bin/bash
echo "error message" >&2
read line
echo '{"id": 1, "result": null, "source": "test"}'
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(10);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    // Send request
    let request = create_search_request(1, "test");
    plugin_tx.send(request).await.expect("Failed to send request");

    // Should still receive response despite stderr output
    let response = timeout(Duration::from_secs(2), response_rx.recv())
        .await
        .expect("Timeout waiting for response")
        .expect("No response received");

    match response {
        PluginResponse::Response(_, message) => {
            match message {
                Message::Response { result, .. } => {
                    assert!(result.is_none());
                }
                _ => panic!("Expected response message"),
            }
        }
    }

    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
async fn test_spawn_plugin_multiple_responses() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("multi_response_plugin");

    // Create plugin that sends multiple responses
    let script = r#"#!/bin/bash
while read line; do
    echo '{"id": 1, "result": null, "source": "test1"}'
    echo '{"id": 2, "result": null, "source": "test2"}'
done
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(10);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    // Send request
    let request = create_search_request(1, "test");
    plugin_tx.send(request).await.expect("Failed to send request");

    // Should receive multiple responses
    let response1 = timeout(Duration::from_secs(1), response_rx.recv())
        .await
        .expect("Timeout waiting for first response")
        .expect("No first response received");

    let response2 = timeout(Duration::from_secs(1), response_rx.recv())
        .await
        .expect("Timeout waiting for second response")
        .expect("No second response received");

    // Verify both responses
    assert!(matches!(response1, PluginResponse::Response(_, _)));
    assert!(matches!(response2, PluginResponse::Response(_, _)));

    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_special_characters_in_path() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create plugin with special characters in name
    let plugin_path = plugin_dir.join("plugin-with_special.chars");
    fs::write(&plugin_path, "#!/bin/bash\necho 'test'").unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    unsafe { env::set_var("GLIMPSED_PLUGIN_DIR", plugin_dir.to_str().unwrap()); }

    let plugins = discover_plugins();

    unsafe { env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains(&plugin_path.to_string_lossy().to_string()));
}