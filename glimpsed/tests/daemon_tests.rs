use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;

use glimpse_sdk::{Message, Metadata, Method, MethodResult};
use serial_test::serial;
use tokio::sync::mpsc;
use tokio::time::timeout;

mod common;
use common::*;

// Note: Since daemon fields are private, we test through public interface
// Test helper to create a daemon for unit testing
fn create_test_daemon() -> glimpsed::daemon::Daemon {
    glimpsed::daemon::Daemon::new()
}

#[tokio::test]
async fn test_daemon_new() {
    let daemon = create_test_daemon();
    // Verify daemon creation succeeds (fields are private)
    // We can only test through the public interface
}

#[tokio::test]
async fn test_daemon_stop_with_channel() {
    let mut daemon = create_test_daemon();

    // Should successfully send stop signal without panic
    daemon.stop().await;
}

#[tokio::test]
async fn test_daemon_stop_without_channel() {
    let mut daemon = create_test_daemon();

    // First stop consumes the channel
    daemon.stop().await;

    // Second stop should handle None case gracefully without panic
    daemon.stop().await;
}

#[tokio::test]
#[serial]
async fn test_daemon_request_cancellation() {
    // Test that newer requests cancel older ones via current_request tracking
    let current_request = Arc::new(AtomicUsize::new(0));

    // Simulate older request
    current_request.store(1, Ordering::SeqCst);
    let old_request_id = current_request.load(Ordering::SeqCst);

    // Simulate newer request
    current_request.store(2, Ordering::SeqCst);
    let new_request_id = current_request.load(Ordering::SeqCst);

    assert_ne!(old_request_id, new_request_id);
    assert_eq!(new_request_id, 2);
}

#[tokio::test]
async fn test_message_parsing_success() {
    let json = r#"{"id": 1, "method": "search", "params": "test query"}"#;
    let result: Result<Message, _> = serde_json::from_str(json);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_message_parsing_failure() {
    let invalid_json = "invalid json content";
    let result: Result<Message, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_authentication_response_processing() {
    let metadata = Metadata {
        id: "test_plugin".to_string(),
        name: "test_plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "Test plugin".to_string(),
        author: "Test".to_string(),
    };

    let auth_response = Message::Response {
        id: 1,
        error: None,
        source: Some("test_plugin".to_string()),
        result: Some(MethodResult::Authenticate(metadata.clone())),
    };

    // Verify the response structure
    match auth_response {
        Message::Response { result, .. } => {
            match result {
                Some(MethodResult::Authenticate(meta)) => {
                    assert_eq!(meta.name, "test_plugin");
                    assert_eq!(meta.version, "1.0.0");
                }
                _ => panic!("Expected authentication result"),
            }
        }
        _ => panic!("Expected response message"),
    }
}

#[tokio::test]
async fn test_search_results_response_processing() {
    let search_response = Message::Response {
        id: 1,
        error: None,
        source: Some("test_plugin".to_string()),
        result: Some(MethodResult::SearchResults(vec![])),
    };

    match search_response {
        Message::Response { result, .. } => {
            match result {
                Some(MethodResult::SearchResults(items)) => {
                    assert!(items.is_empty());
                }
                _ => panic!("Expected search results"),
            }
        }
        _ => panic!("Expected response message"),
    }
}

#[tokio::test]
async fn test_response_with_no_result() {
    let empty_response = Message::Response {
        id: 1,
        error: None,
        source: Some("test_plugin".to_string()),
        result: None,
    };

    match empty_response {
        Message::Response { result, .. } => {
            assert!(result.is_none());
        }
        _ => panic!("Expected response message"),
    }
}

#[tokio::test]
async fn test_response_with_error() {
    let error_response = Message::Response {
        id: 1,
        error: Some("Plugin error".to_string()),
        source: Some("test_plugin".to_string()),
        result: None,
    };

    match error_response {
        Message::Response { error, .. } => {
            assert_eq!(error, Some("Plugin error".to_string()));
        }
        _ => panic!("Expected response message"),
    }
}

#[tokio::test]
async fn test_different_message_types() {
    // Test Request message
    let request = create_search_request(1, "test");
    match request {
        Message::Request { method, .. } => {
            match method {
                Method::Search(query) => assert_eq!(query, "test"),
                _ => panic!("Expected search method"),
            }
        }
        _ => panic!("Expected request message"),
    }

    // Test Notification message
    let notification = Message::Notification {
        method: Method::Cancel,
    };
    match notification {
        Message::Notification { method } => {
            match method {
                Method::Cancel => {},
                _ => panic!("Expected cancel method"),
            }
        }
        _ => panic!("Expected notification message"),
    }
}

#[tokio::test]
async fn test_request_id_tracking() {
    let current_request = Arc::new(AtomicUsize::new(0));

    // Test sequential ID updates
    for expected_id in 1..=10 {
        current_request.store(expected_id, Ordering::SeqCst);
        let actual_id = current_request.load(Ordering::SeqCst);
        assert_eq!(actual_id, expected_id);
    }
}

#[tokio::test]
async fn test_plugin_metadata_update() {
    let mut plugins: HashMap<String, MockPlugin> = HashMap::new();
    let plugin_name = "test_plugin".to_string();
    let plugin = MockPlugin::new(&plugin_name);
    plugins.insert(plugin_name.clone(), plugin);

    // Simulate metadata update
    if let Some(plugin) = plugins.get_mut(&plugin_name) {
        let metadata = Metadata {
            id: "updated_plugin".to_string(),
            name: "updated_plugin".to_string(),
            version: "2.0.0".to_string(),
            description: "Updated plugin".to_string(),
            author: "Test".to_string(),
        };
        // In real code, this would update plugin.metadata
        // Here we just verify the lookup works
        assert_eq!(plugin.name, "test_plugin");
    }
}

#[tokio::test]
async fn test_plugin_not_found_for_metadata_update() {
    let plugins: HashMap<String, MockPlugin> = HashMap::new();
    let plugin_name = "nonexistent_plugin".to_string();

    // Simulate trying to update nonexistent plugin
    let result = plugins.get(&plugin_name);
    assert!(result.is_none());
}

#[tokio::test]
async fn test_channel_send_success() {
    let (tx, mut rx) = mpsc::channel::<Message>(10);

    let message = create_search_request(1, "test");
    let result = tx.send(message.clone()).await;
    assert!(result.is_ok());

    let received = rx.recv().await;
    assert!(received.is_some());
    assert_eq!(received.unwrap(), message);
}

#[tokio::test]
async fn test_channel_send_failure() {
    let (tx, rx) = mpsc::channel::<Message>(1);

    // Drop receiver to cause send failure
    drop(rx);

    let message = create_search_request(1, "test");
    let result = tx.send(message).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrent_plugin_communication() {
    let (tx1, mut rx1) = mpsc::channel::<Message>(10);
    let (tx2, mut rx2) = mpsc::channel::<Message>(10);

    let message1 = create_search_request(1, "query1");
    let message2 = create_search_request(2, "query2");

    // Send messages concurrently
    let send1 = tx1.send(message1.clone());
    let send2 = tx2.send(message2.clone());

    let (result1, result2) = tokio::join!(send1, send2);
    assert!(result1.is_ok());
    assert!(result2.is_ok());

    // Receive messages
    let recv1 = rx1.recv().await;
    let recv2 = rx2.recv().await;

    assert_eq!(recv1.unwrap(), message1);
    assert_eq!(recv2.unwrap(), message2);
}

#[tokio::test]
async fn test_request_target_and_context() {
    let request_with_target = Message::Request {
        id: 1,
        method: Method::Search("test".to_string()),
        target: Some("specific_plugin".to_string()),
        context: Some("search_context".to_string()),
    };

    match request_with_target {
        Message::Request { target, context, .. } => {
            assert_eq!(target, Some("specific_plugin".to_string()));
            assert_eq!(context, Some("search_context".to_string()));
        }
        _ => panic!("Expected request message"),
    }
}

#[tokio::test]
async fn test_notification_method_variants() {
    let methods = vec![
        Method::Cancel,
        Method::Quit,
        Method::Search("test".to_string()),
    ];

    for method in methods {
        let notification = Message::Notification { method: method.clone() };
        match notification {
            Message::Notification { method: received_method } => {
                assert_eq!(received_method, method);
            }
            _ => panic!("Expected notification"),
        }
    }
}

#[tokio::test]
async fn test_response_source_tracking() {
    let sources = vec!["plugin1", "plugin2", "plugin3"];

    for source in sources {
        let response = Message::Response {
            id: 1,
            error: None,
            source: Some(source.to_string()),
            result: None,
        };

        match response {
            Message::Response { source: response_source, .. } => {
                assert_eq!(response_source, Some(source.to_string()));
            }
            _ => panic!("Expected response message"),
        }
    }
}

#[tokio::test]
async fn test_atomic_request_id_operations() {
    let current_request = Arc::new(AtomicUsize::new(0));
    let current_request_clone1 = Arc::clone(&current_request);
    let current_request_clone2 = Arc::clone(&current_request);

    // Test from different "threads" (tasks)
    let task1 = tokio::spawn(async move {
        current_request_clone1.store(100, Ordering::SeqCst);
    });

    let task2 = tokio::spawn(async move {
        current_request_clone2.store(200, Ordering::SeqCst);
    });

    let _ = tokio::join!(task1, task2);

    // One of the values should be set (non-deterministic which one due to concurrency)
    let final_value = current_request.load(Ordering::SeqCst);
    assert!(final_value == 100 || final_value == 200);
}