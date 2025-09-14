use glimpse_sdk::{Action, Message, Metadata, Method, MethodResult, SearchItem};
use serde_json;

mod common;
use common::*;

#[test]
fn test_message_serialization_roundtrip() {
    let messages = vec![
        create_search_request(1, "test query"),
        create_cancel_request(2),
        create_quit_request(3),
        create_auth_response(1, "test_plugin"),
    ];

    for original_message in messages {
        // Serialize to JSON
        let json = serde_json::to_string(&original_message).expect("Failed to serialize message");

        // Deserialize back
        let deserialized: Message =
            serde_json::from_str(&json).expect("Failed to deserialize message");

        // Should be equal
        assert_eq!(original_message, deserialized);
    }
}

#[test]
fn test_request_message_variants() {
    // Test Search request
    let search_request = Message::Request {
        id: 1,
        method: Method::Search("test query".to_string()),
        target: Some("specific_plugin".to_string()),
        context: Some("search_context".to_string()),
    };

    match search_request {
        Message::Request {
            id,
            method,
            target,
            context,
        } => {
            assert_eq!(id, 1);
            assert_eq!(method, Method::Search("test query".to_string()));
            assert_eq!(target, Some("specific_plugin".to_string()));
            assert_eq!(context, Some("search_context".to_string()));
        }
        _ => panic!("Expected request message"),
    }

    // Test Cancel request
    let cancel_request = Message::Request {
        id: 2,
        method: Method::Cancel,
        target: None,
        context: None,
    };

    match cancel_request {
        Message::Request { id, method, .. } => {
            assert_eq!(id, 2);
            assert_eq!(method, Method::Cancel);
        }
        _ => panic!("Expected request message"),
    }

    // Test Quit request
    let quit_request = Message::Request {
        id: 3,
        method: Method::Quit,
        target: None,
        context: None,
    };

    match quit_request {
        Message::Request { id, method, .. } => {
            assert_eq!(id, 3);
            assert_eq!(method, Method::Quit);
        }
        _ => panic!("Expected request message"),
    }
}

#[test]
fn test_response_message_variants() {
    // Test successful response
    let success_response = Message::Response {
        id: 1,
        error: None,
        source: Some("test_plugin".to_string()),
        result: Some(MethodResult::SearchResults(vec![])),
    };

    match success_response {
        Message::Response {
            id,
            error,
            source,
            result,
        } => {
            assert_eq!(id, 1);
            assert!(error.is_none());
            assert_eq!(source, Some("test_plugin".to_string()));
            assert!(result.is_some());
        }
        _ => panic!("Expected response message"),
    }

    // Test error response
    let error_response = Message::Response {
        id: 2,
        error: Some("Plugin error occurred".to_string()),
        source: Some("failing_plugin".to_string()),
        result: None,
    };

    match error_response {
        Message::Response {
            id,
            error,
            source,
            result,
        } => {
            assert_eq!(id, 2);
            assert_eq!(error, Some("Plugin error occurred".to_string()));
            assert_eq!(source, Some("failing_plugin".to_string()));
            assert!(result.is_none());
        }
        _ => panic!("Expected response message"),
    }
}

#[test]
fn test_notification_message_variants() {
    let methods = vec![
        Method::Search("notification search".to_string()),
        Method::Cancel,
        Method::Quit,
    ];

    for method in methods {
        let notification = Message::Notification {
            method: method.clone(),
        };

        match notification {
            Message::Notification {
                method: received_method,
            } => {
                assert_eq!(received_method, method);
            }
            _ => panic!("Expected notification message"),
        }
    }
}

#[test]
fn test_method_result_variants() {
    // Test Authentication result
    let auth_metadata = Metadata {
        id: "plugin_id".to_string(),
        name: "Test Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A test plugin".to_string(),
        author: "Test Author".to_string(),
    };

    let auth_result = MethodResult::Authenticate(auth_metadata.clone());
    match auth_result {
        MethodResult::Authenticate(metadata) => {
            assert_eq!(metadata.name, "Test Plugin");
            assert_eq!(metadata.version, "1.0.0");
            assert_eq!(metadata.id, "plugin_id");
            assert_eq!(metadata.description, "A test plugin");
            assert_eq!(metadata.author, "Test Author");
        }
        _ => panic!("Expected authentication result"),
    }

    // Test SearchResults result
    let search_items = vec![
        SearchItem {
            title: "Test Item 1".to_string(),
            subtitle: Some("Subtitle 1".to_string()),
            icon: None,
            actions: vec![Action::ShellExec {
                command: "echo".to_string(),
                args: vec!["test1".to_string()],
            }],
            score: 0.9,
        },
        SearchItem {
            title: "Test Item 2".to_string(),
            subtitle: None,
            icon: Some("icon.png".to_string()),
            actions: vec![Action::OpenPath {
                path: "/test/path".to_string(),
            }],
            score: 0.8,
        },
    ];

    let search_result = MethodResult::SearchResults(search_items.clone());
    match search_result {
        MethodResult::SearchResults(items) => {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].title, "Test Item 1");
            assert_eq!(items[0].subtitle, Some("Subtitle 1".to_string()));
            assert_eq!(items[1].title, "Test Item 2");
            assert_eq!(items[1].icon, Some("icon.png".to_string()));
        }
        _ => panic!("Expected search results"),
    }
}

#[test]
fn test_search_item_structure() {
    let search_item = SearchItem {
        title: "Complex Item".to_string(),
        subtitle: Some("With subtitle".to_string()),
        icon: Some("complex_icon.svg".to_string()),
        score: 0.75,
        actions: vec![
            Action::ShellExec {
                command: "ls".to_string(),
                args: vec!["-la".to_string(), "/tmp".to_string()],
            },
            Action::OpenPath {
                path: "/home/user/documents".to_string(),
            },
        ],
    };

    assert_eq!(search_item.title, "Complex Item");
    assert_eq!(search_item.subtitle, Some("With subtitle".to_string()));
    assert_eq!(search_item.icon, Some("complex_icon.svg".to_string()));
    assert_eq!(search_item.actions.len(), 2);

    match &search_item.actions[0] {
        Action::ShellExec { command, args } => {
            assert_eq!(command, "ls");
            assert_eq!(args, &vec!["-la".to_string(), "/tmp".to_string()]);
        }
        _ => panic!("Expected ShellExec action"),
    }

    match &search_item.actions[1] {
        Action::OpenPath { path } => {
            assert_eq!(path, "/home/user/documents");
        }
        _ => panic!("Expected OpenPath action"),
    }
}

#[test]
fn test_action_variants() {
    // Test ShellExec action
    let shell_action = Action::ShellExec {
        command: "python".to_string(),
        args: vec!["-c".to_string(), "print('hello')".to_string()],
    };

    match shell_action {
        Action::ShellExec { command, args } => {
            assert_eq!(command, "python");
            assert_eq!(args, vec!["-c", "print('hello')"]);
        }
        _ => panic!("Expected ShellExec action"),
    }

    // Test OpenPath action
    let open_action = Action::OpenPath {
        path: "/path/to/file.txt".to_string(),
    };

    match open_action {
        Action::OpenPath { path } => {
            assert_eq!(path, "/path/to/file.txt");
        }
        _ => panic!("Expected OpenPath action"),
    }
}

#[test]
fn test_message_id_correlation() {
    let request_id = 42;
    let request = create_search_request(request_id, "test");

    match request {
        Message::Request { id, .. } => {
            assert_eq!(id, request_id);

            // Response should have matching ID
            let response = Message::Response {
                id,
                error: None,
                source: Some("plugin".to_string()),
                result: Some(MethodResult::SearchResults(vec![])),
            };

            match response {
                Message::Response {
                    id: response_id, ..
                } => {
                    assert_eq!(response_id, request_id);
                }
                _ => panic!("Expected response"),
            }
        }
        _ => panic!("Expected request"),
    }
}

#[test]
fn test_empty_and_minimal_structures() {
    // Test empty search results
    let empty_results = MethodResult::SearchResults(vec![]);
    match empty_results {
        MethodResult::SearchResults(items) => {
            assert!(items.is_empty());
        }
        _ => panic!("Expected search results"),
    }

    // Test minimal search item
    let minimal_item = SearchItem {
        title: "Minimal".to_string(),
        subtitle: None,
        icon: None,
        actions: vec![],
        score: 0.0,
    };

    assert_eq!(minimal_item.title, "Minimal");
    assert!(minimal_item.subtitle.is_none());
    assert!(minimal_item.icon.is_none());
    assert!(minimal_item.actions.is_empty());

    // Test minimal metadata
    let minimal_metadata = Metadata {
        id: "id".to_string(),
        name: "Name".to_string(),
        version: "1.0".to_string(),
        description: "Desc".to_string(),
        author: "Author".to_string(),
    };

    assert_eq!(minimal_metadata.name, "Name");
    assert_eq!(minimal_metadata.version, "1.0");
}

#[test]
fn test_json_schema_compliance() {
    // Test that serialized JSON follows expected schema
    let request = create_search_request(1, "schema test");
    let json = serde_json::to_string(&request).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Request should have required fields
    assert!(parsed.get("id").is_some());
    assert!(parsed.get("method").is_some());
    assert_eq!(parsed["method"], "search");
    assert_eq!(parsed["params"], "schema test");

    // Test response schema
    let response = create_auth_response(2, "schema_plugin");
    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(parsed.get("id").is_some());
    assert!(parsed.get("source").is_some());
    assert!(parsed.get("result").is_some());
    assert_eq!(parsed["id"], 2);
    assert_eq!(parsed["source"], "schema_plugin");
}

#[test]
fn test_error_handling_in_protocol() {
    // Test various error scenarios
    let error_cases = vec![
        (None, Some("General error".to_string())),
        (
            Some(MethodResult::SearchResults(vec![])),
            Some("Error with result".to_string()),
        ),
        (None, None), // No error, no result
    ];

    for (result, error) in error_cases {
        let response = Message::Response {
            id: 1,
            error,
            source: Some("test".to_string()),
            result,
        };

        // Should serialize/deserialize correctly regardless of error/result combination
        let json = serde_json::to_string(&response).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }
}

#[test]
fn test_protocol_version_compatibility() {
    // Test that protocol can handle missing optional fields
    let minimal_json = r#"{"id": 1, "method": "search", "params": "test"}"#;
    let message: Result<Message, _> = serde_json::from_str(minimal_json);
    assert!(message.is_ok());

    if let Ok(Message::Request {
        target, context, ..
    }) = message
    {
        assert!(target.is_none());
        assert!(context.is_none());
    }

    // Test response with minimal fields - this will actually parse as an untagged enum variant
    let minimal_response = r#"{"id": 1}"#;
    let response: Result<Message, _> = serde_json::from_str(minimal_response);
    // This might actually succeed due to untagged enum deserialization
    if let Ok(Message::Response {
        id,
        error,
        source,
        result,
    }) = response
    {
        assert_eq!(id, 1);
        assert!(error.is_none());
        assert!(source.is_none());
        assert!(result.is_none());
    } else {
        // Or it might fail, which is also acceptable
        assert!(response.is_err());
    }
}

#[test]
fn test_unicode_in_protocol_messages() {
    let unicode_search = Message::Request {
        id: 1,
        method: Method::Search("🔍 Unicode search: 测试 café naïve résumé".to_string()),
        target: Some("🚀 plugin".to_string()),
        context: Some("🌍 context".to_string()),
    };

    let json = serde_json::to_string(&unicode_search).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(unicode_search, deserialized);

    // Test unicode in search results
    let unicode_item = SearchItem {
        title: "📄 Document: résumé.pdf".to_string(),
        subtitle: Some("💼 Work → Career".to_string()),
        icon: Some("📋".to_string()),
        actions: vec![Action::OpenPath {
            path: "/home/用户/文档/résumé.pdf".to_string(),
        }],
        score: 0.95,
    };

    let json = serde_json::to_string(&unicode_item).unwrap();
    let deserialized: SearchItem = serde_json::from_str(&json).unwrap();
    assert_eq!(unicode_item, deserialized);
}

#[test]
fn test_large_data_structures() {
    // Test protocol with large amounts of data
    let mut large_actions = Vec::new();
    for i in 0..1000 {
        large_actions.push(Action::ShellExec {
            command: format!("command_{}", i),
            args: vec![format!("arg1_{}", i), format!("arg2_{}", i)],
        });
    }

    let large_item = SearchItem {
        title: "Large item".to_string(),
        subtitle: Some("With many actions".to_string()),
        icon: None,
        actions: large_actions,
        score: 0.5,
    };

    let json = serde_json::to_string(&large_item).unwrap();
    let deserialized: SearchItem = serde_json::from_str(&json).unwrap();
    assert_eq!(large_item.actions.len(), deserialized.actions.len());
    assert_eq!(large_item.actions.len(), 1000);
}

#[test]
fn test_special_characters_in_protocol() {
    let special_chars = r#"Special: "quotes" 'apostrophes' \backslashes/ /forward-slashes\ newlines:
and tabs:	and nulls:"#;

    let message = Message::Request {
        id: 1,
        method: Method::Search(special_chars.to_string()),
        target: None,
        context: None,
    };

    let json = serde_json::to_string(&message).unwrap();
    let deserialized: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(message, deserialized);
}
