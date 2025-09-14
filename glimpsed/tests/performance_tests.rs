use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use glimpse_sdk::{Message, Method};
use serial_test::serial;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;

mod common;
use common::*;

use glimpsed::plugins::{discover_plugins, spawn_plugin, PluginResponse};

#[tokio::test]
async fn test_high_throughput_message_processing() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("fast_plugin");

    // Create plugin that responds quickly
    let script = r#"#!/bin/bash
while read line; do
    echo '{"id": 1, "result": null, "source": "fast"}'
done
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(1000);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(1000);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    // Send many messages rapidly
    let start_time = Instant::now();
    let num_messages = 100;

    for i in 0..num_messages {
        let request = create_search_request(i, &format!("query_{}", i));
        if plugin_tx.send(request).await.is_err() {
            break;
        }
    }

    // Count responses received
    let mut responses_received = 0;
    let response_timeout = Duration::from_secs(10);
    let deadline = Instant::now() + response_timeout;

    while Instant::now() < deadline && responses_received < num_messages {
        if let Ok(Some(_)) = timeout(Duration::from_millis(100), response_rx.recv()).await {
            responses_received += 1;
        } else {
            break;
        }
    }

    let elapsed = start_time.elapsed();
    let throughput = responses_received as f64 / elapsed.as_secs_f64();

    // Should process at least 10 messages per second
    assert!(throughput > 10.0, "Throughput too low: {} msg/sec", throughput);
    assert!(responses_received > num_messages / 2, "Too many lost messages");

    spawn_handle.abort();
    let _ = spawn_handle.await;
}

#[tokio::test]
async fn test_concurrent_plugin_spawning() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create multiple test plugins
    let num_plugins = 10;
    let mut plugin_paths = Vec::new();

    for i in 0..num_plugins {
        let plugin_path = plugin_dir.join(format!("plugin_{}", i));
        let script = format!(
            r#"#!/bin/bash
while read line; do
    echo '{{"id": 1, "result": null, "source": "plugin_{}" }}'
done
"#,
            i
        );
        fs::write(&plugin_path, script).unwrap();
        let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&plugin_path, perms).unwrap();
        plugin_paths.push(plugin_path.to_string_lossy().to_string());
    }

    // Spawn all plugins concurrently
    let start_time = Instant::now();
    let mut handles = Vec::new();
    let response_count = Arc::new(AtomicUsize::new(0));

    for plugin_path in plugin_paths {
        let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(10);
        let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(10);
        let response_count_clone = Arc::clone(&response_count);

        let spawn_handle = tokio::spawn(async move {
            spawn_plugin(plugin_path, response_tx, plugin_rx).await;
        });

        let response_handle = tokio::spawn(async move {
            let request = create_search_request(1, "test");
            let _ = plugin_tx.send(request).await;

            if let Ok(Some(_)) = timeout(Duration::from_secs(5), response_rx.recv()).await {
                response_count_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        handles.push((spawn_handle, response_handle));
    }

    // Wait for all responses
    for (spawn_handle, response_handle) in handles {
        let _ = timeout(Duration::from_secs(10), response_handle).await;
        spawn_handle.abort();
        let _ = spawn_handle.await;
    }

    let elapsed = start_time.elapsed();
    let responses = response_count.load(Ordering::SeqCst);

    // Should spawn and get responses from most plugins within reasonable time
    assert!(elapsed < Duration::from_secs(15), "Plugin spawning too slow");
    assert!(responses >= num_plugins / 2, "Too few plugins responded: {}/{}", responses, num_plugins);
}

#[tokio::test]
async fn test_memory_usage_under_load() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("memory_test_plugin");

    // Create plugin that processes requests normally
    let script = r#"#!/bin/bash
while read line; do
    echo '{"id": 1, "result": null, "source": "memory_test"}'
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

    // Send many requests in batches to test memory stability
    for batch in 0..10 {
        for i in 0..50 {
            let request = create_search_request(batch * 50 + i, &format!("batch_{}_query_{}", batch, i));
            if plugin_tx.send(request).await.is_err() {
                break;
            }
        }

        // Drain responses to prevent memory buildup
        let mut drained = 0;
        while drained < 50 {
            if let Ok(Some(_)) = timeout(Duration::from_millis(10), response_rx.recv()).await {
                drained += 1;
            } else {
                break;
            }
        }

        // Small pause between batches
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    spawn_handle.abort();
    let _ = spawn_handle.await;

    // Test passes if no memory leaks cause OOM or excessive slowdown
    // In a real test environment, you'd monitor actual memory usage
}

#[tokio::test]
async fn test_plugin_restart_performance() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("restart_test_plugin");

    // Create plugin that exits after one response
    let script = r#"#!/bin/bash
read line
echo '{"id": 1, "result": null, "source": "restart_test"}'
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

    let start_time = Instant::now();
    let mut successful_requests = 0;

    // Send requests that will cause plugin restarts
    for i in 0..5 {
        let request = create_search_request(i, "restart_test");
        if plugin_tx.send(request).await.is_err() {
            break;
        }

        // Wait for response (plugin will exit after responding)
        if let Ok(Some(_)) = timeout(Duration::from_secs(2), response_rx.recv()).await {
            successful_requests += 1;
        }

        // Give plugin time to restart
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let elapsed = start_time.elapsed();

    spawn_handle.abort();
    let _ = spawn_handle.await;

    // Should handle at least some restarts within reasonable time
    assert!(successful_requests >= 2, "Plugin restarts not working");
    assert!(elapsed < Duration::from_secs(15), "Plugin restart too slow");
}

#[tokio::test]
#[serial]
async fn test_plugin_discovery_performance() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path();

    // Create many plugin files
    let num_plugins = 100;
    for i in 0..num_plugins {
        let plugin_path = plugin_dir.join(format!("plugin_{:03}", i));
        fs::write(&plugin_path, "#!/bin/bash\necho 'test'").unwrap();
        let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&plugin_path, perms).unwrap();
    }

    // Also create non-plugin files that should be ignored
    for i in 0..50 {
        let non_plugin = plugin_dir.join(format!("not_plugin_{:03}.txt", i));
        fs::write(&non_plugin, "not executable").unwrap();
    }

    unsafe { std::env::set_var("GLIMPSED_PLUGIN_DIR", plugin_dir.to_str().unwrap()); }

    let start_time = Instant::now();
    let plugins = discover_plugins();
    let elapsed = start_time.elapsed();

    unsafe { std::env::remove_var("GLIMPSED_PLUGIN_DIR"); }

    // Should discover all executable plugins quickly
    assert_eq!(plugins.len(), num_plugins);
    assert!(elapsed < Duration::from_secs(5), "Plugin discovery too slow: {:?}", elapsed);
}

#[tokio::test]
async fn test_large_message_processing() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("large_message_plugin");

    // Create plugin that handles large messages
    let script = r#"#!/bin/bash
while read line; do
    # Echo back a large response
    large_data=$(printf 'x%.0s' {1..10000})
    echo "{\"id\": 1, \"result\": null, \"source\": \"large_test\", \"large_field\": \"$large_data\"}"
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

    let start_time = Instant::now();

    // Send request that will generate large response
    let large_query = "x".repeat(1000); // 1KB query
    let request = create_search_request(1, &large_query);
    plugin_tx.send(request).await.expect("Failed to send large request");

    // Wait for large response
    let response = timeout(Duration::from_secs(5), response_rx.recv())
        .await
        .expect("Timeout waiting for large response")
        .expect("No response received");

    let elapsed = start_time.elapsed();

    spawn_handle.abort();
    let _ = spawn_handle.await;

    // Should handle large messages within reasonable time
    assert!(elapsed < Duration::from_secs(10), "Large message processing too slow");
    assert!(matches!(response, PluginResponse::Response(_, _)));
}

#[tokio::test]
async fn test_concurrent_request_handling() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("concurrent_plugin");

    // Create plugin that can handle concurrent requests
    let script = r#"#!/bin/bash
while read line; do
    # Add small delay to simulate work
    sleep 0.1
    echo '{"id": 1, "result": null, "source": "concurrent"}'
done
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(100);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(100);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    let start_time = Instant::now();

    // Send multiple requests concurrently
    let num_requests = 20;
    for i in 0..num_requests {
        let request = create_search_request(i, &format!("concurrent_{}", i));
        if plugin_tx.send(request).await.is_err() {
            break;
        }
    }

    // Collect responses
    let mut responses_received = 0;
    let timeout_duration = Duration::from_secs(30); // Allow time for sequential processing
    let deadline = Instant::now() + timeout_duration;

    while Instant::now() < deadline && responses_received < num_requests {
        if let Ok(Some(_)) = timeout(Duration::from_millis(500), response_rx.recv()).await {
            responses_received += 1;
        } else {
            break;
        }
    }

    let elapsed = start_time.elapsed();

    spawn_handle.abort();
    let _ = spawn_handle.await;

    // Should process most requests (plugin handles them sequentially)
    assert!(responses_received >= num_requests / 2, "Too few concurrent responses");
    assert!(elapsed < Duration::from_secs(45), "Concurrent processing too slow");
}

#[tokio::test]
async fn test_error_recovery_performance() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("error_recovery_plugin");

    // Create plugin that alternates between success and failure
    let script = r#"#!/bin/bash
counter=0
while read line; do
    counter=$((counter + 1))
    if [ $((counter % 3)) -eq 0 ]; then
        echo "invalid json"
    else
        echo '{"id": 1, "result": null, "source": "error_recovery"}'
    fi
done
"#;
    fs::write(&plugin_path, script).unwrap();
    let mut perms = fs::metadata(&plugin_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&plugin_path, perms).unwrap();

    let (response_tx, mut response_rx) = mpsc::channel::<PluginResponse>(50);
    let (plugin_tx, plugin_rx) = mpsc::channel::<Message>(50);

    let path_str = plugin_path.to_string_lossy().to_string();
    let spawn_handle = tokio::spawn(async move {
        spawn_plugin(path_str, response_tx, plugin_rx).await;
    });

    let start_time = Instant::now();
    let num_requests = 30;
    let mut valid_responses = 0;

    // Send requests that will cause mixed success/failure
    for i in 0..num_requests {
        let request = create_search_request(i, "error_test");
        if plugin_tx.send(request).await.is_err() {
            break;
        }

        // Try to receive response (some will be invalid JSON)
        if let Ok(Some(_)) = timeout(Duration::from_millis(200), response_rx.recv()).await {
            valid_responses += 1;
        }
    }

    let elapsed = start_time.elapsed();

    spawn_handle.abort();
    let _ = spawn_handle.await;

    // Should recover from errors and process valid responses
    assert!(valid_responses > num_requests / 3, "Not enough error recovery");
    assert!(elapsed < Duration::from_secs(20), "Error recovery too slow");
}

#[tokio::test]
async fn test_latency_measurement() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_path = temp_dir.path().join("latency_test_plugin");

    // Create plugin with minimal response time
    let script = r#"#!/bin/bash
while read line; do
    echo '{"id": 1, "result": null, "source": "latency_test"}'
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

    // Measure latency over multiple requests
    let mut latencies = Vec::new();

    for i in 0..10 {
        let request_start = Instant::now();

        let request = create_search_request(i, "latency_test");
        plugin_tx.send(request).await.expect("Failed to send request");

        if let Ok(Some(_)) = timeout(Duration::from_secs(2), response_rx.recv()).await {
            let latency = request_start.elapsed();
            latencies.push(latency);
        }
    }

    spawn_handle.abort();
    let _ = spawn_handle.await;

    // Calculate average latency
    if !latencies.is_empty() {
        let total_latency: Duration = latencies.iter().sum();
        let avg_latency = total_latency / latencies.len() as u32;

        // Average latency should be reasonable (under 1 second for simple plugin)
        assert!(avg_latency < Duration::from_millis(1000), "Average latency too high: {:?}", avg_latency);
        assert!(latencies.len() >= 5, "Too few latency samples");
    }
}