use glimpse_sdk::{Action, Message, Method, MethodResult, Match};

#[cfg(test)]
mod method_tests {
    use super::*;

    #[test]
    fn test_search_method_serialization() {
        let method = Method::Search("hello world".to_string());
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, r#"{"method":"search","params":"hello world"}"#);
    }

    #[test]
    fn test_search_method_deserialization() {
        let json = r#"{"method":"search","params":"hello world"}"#;
        let method: Method = serde_json::from_str(json).unwrap();
        assert_eq!(method, Method::Search("hello world".to_string()));
    }

    #[test]
    fn test_search_method_empty_query() {
        let method = Method::Search("".to_string());
        let json = serde_json::to_string(&method).unwrap();
        let deserialized: Method = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, method);
    }

    #[test]
    fn test_search_method_unicode() {
        let method = Method::Search("こんにちは 🚀 ñoño".to_string());
        let json = serde_json::to_string(&method).unwrap();
        let deserialized: Method = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, method);
    }

    #[test]
    fn test_search_method_long_query() {
        let long_query = "a".repeat(10000);
        let method = Method::Search(long_query.clone());
        let json = serde_json::to_string(&method).unwrap();
        let deserialized: Method = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, Method::Search(long_query));
    }

    #[test]
    fn test_cancel_method_serialization() {
        let method = Method::Cancel;
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, r#"{"method":"cancel"}"#);
    }

    #[test]
    fn test_cancel_method_deserialization() {
        let json = r#"{"method":"cancel"}"#;
        let method: Method = serde_json::from_str(json).unwrap();
        assert_eq!(method, Method::Cancel);
    }

    #[test]
    fn test_quit_method_serialization() {
        let method = Method::Quit;
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, r#"{"method":"quit"}"#);
    }

    #[test]
    fn test_quit_method_deserialization() {
        let json = r#"{"method":"quit"}"#;
        let method: Method = serde_json::from_str(json).unwrap();
        assert_eq!(method, Method::Quit);
    }

    #[test]
    fn test_method_round_trip() {
        let methods = vec![
            Method::Search("test".to_string()),
            Method::Cancel,
            Method::Quit,
        ];

        for method in methods {
            let json = serde_json::to_string(&method).unwrap();
            let deserialized: Method = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, method);
        }
    }
}

#[cfg(test)]
mod method_result_tests {
    use super::*;

    #[test]
    fn test_search_results_empty() {
        let result = MethodResult::Matches(vec![]);
        let json = serde_json::to_string(&result).unwrap();
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_search_results_single_item() {
        let item = Match {
            title: "Test Item".to_string(),
            subtitle: Some("Test Subtitle".to_string()),
            icon: None,
            actions: vec![Action::Clipboard {
                text: "test".to_string(),
            }],
            score: 1.0,
        };
        let result = MethodResult::Matches(vec![item]);
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: MethodResult = serde_json::from_str(&json).unwrap();

        match deserialized {
            MethodResult::Matches(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].title, "Test Item");
                assert_eq!(items[0].subtitle, Some("Test Subtitle".to_string()));
                assert_eq!(items[0].score, 1.0);
            }
            MethodResult::Authenticate(_) => panic!("Expected SearchResults, got Authenticate"),
        }
    }

    #[test]
    fn test_search_results_multiple_items() {
        let items = vec![
            Match {
                title: "Item 1".to_string(),
                subtitle: None,
                icon: None,
                actions: vec![],
                score: 0.8,
            },
            Match {
                title: "Item 2".to_string(),
                subtitle: Some("Subtitle 2".to_string()),
                icon: Some("icon.png".to_string()),
                actions: vec![Action::OpenPath {
                    path: "/tmp".to_string(),
                }],
                score: 0.6,
            },
        ];
        let result = MethodResult::Matches(items);
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: MethodResult = serde_json::from_str(&json).unwrap();

        match deserialized {
            MethodResult::Matches(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].title, "Item 1");
                assert_eq!(items[1].subtitle, Some("Subtitle 2".to_string()));
            }
            MethodResult::Authenticate(_) => panic!("Expected SearchResults, got Authenticate"),
        }
    }

    #[test]
    fn test_method_result_round_trip() {
        let items = vec![Match {
            title: "Round Trip Test".to_string(),
            subtitle: Some("Testing serialization".to_string()),
            icon: Some("test.ico".to_string()),
            actions: vec![
                Action::Clipboard {
                    text: "clipboard text".to_string(),
                },
                Action::ShellExec {
                    command: "echo".to_string(),
                    args: vec!["hello".to_string()],
                },
            ],
            score: 0.95,
        }];
        let result = MethodResult::Matches(items);

        let json1 = serde_json::to_string(&result).unwrap();
        let deserialized: MethodResult = serde_json::from_str(&json1).unwrap();
        let json2 = serde_json::to_string(&deserialized).unwrap();

        assert_eq!(json1, json2);
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn test_request_message_basic() {
        let message = Message::Request {
            id: 42,
            method: Method::Search("test query".to_string()),
            target: None,
            context: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::Request {
                id,
                method,
                target,
                context,
            } => {
                assert_eq!(id, 42);
                assert_eq!(method, Method::Search("test query".to_string()));
                assert_eq!(target, None);
                assert_eq!(context, None);
            }
            _ => panic!("Expected Request message"),
        }
    }

    #[test]
    fn test_request_message_with_target_and_context() {
        let message = Message::Request {
            id: 123,
            method: Method::Cancel,
            target: Some("plugin-name".to_string()),
            context: Some("search-context".to_string()),
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::Request {
                id,
                method,
                target,
                context,
            } => {
                assert_eq!(id, 123);
                assert_eq!(method, Method::Cancel);
                assert_eq!(target, Some("plugin-name".to_string()));
                assert_eq!(context, Some("search-context".to_string()));
            }
            _ => panic!("Expected Request message"),
        }
    }

    #[test]
    fn test_response_message_success() {
        let search_item = Match {
            title: "Response Test".to_string(),
            subtitle: None,
            icon: None,
            actions: vec![],
            score: 1.0,
        };

        let message = Message::Response {
            id: 99,
            error: None,
            source: Some("test-plugin".to_string()),
            result: Some(MethodResult::Matches(vec![search_item])),
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::Response {
                id,
                error,
                source,
                result,
            } => {
                assert_eq!(id, 99);
                assert_eq!(error, None);
                assert_eq!(source, Some("test-plugin".to_string()));
                assert!(result.is_some());
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn test_response_message_error() {
        let message = Message::Response {
            id: 404,
            error: Some("Plugin not found".to_string()),
            source: Some("daemon".to_string()),
            result: None,
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::Response {
                id,
                error,
                source,
                result,
            } => {
                assert_eq!(id, 404);
                assert_eq!(error, Some("Plugin not found".to_string()));
                assert_eq!(source, Some("daemon".to_string()));
                assert_eq!(result, None);
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn test_notification_message() {
        let message = Message::Notification {
            method: Method::Quit,
        };

        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::Notification { method } => {
                assert_eq!(method, Method::Quit);
            }
            _ => panic!("Expected Notification message"),
        }
    }

    #[test]
    fn test_message_id_edge_cases() {
        let test_cases = vec![0, 1, usize::MAX, 42];

        for test_id in test_cases {
            let message = Message::Request {
                id: test_id,
                method: Method::Search("test".to_string()),
                target: None,
                context: None,
            };

            let json = serde_json::to_string(&message).unwrap();
            let deserialized: Message = serde_json::from_str(&json).unwrap();

            match deserialized {
                Message::Request { id, .. } => assert_eq!(id, test_id),
                _ => panic!("Expected Request message"),
            }
        }
    }

    #[test]
    fn test_message_request_raw_json() {
        // Test parsing raw JSON strings for Request messages
        let json = r#"{"id":1,"method":"search","params":"hello world"}"#;
        let message: Message = serde_json::from_str(json).unwrap();

        match message {
            Message::Request {
                id,
                method,
                target,
                context,
            } => {
                assert_eq!(id, 1);
                assert_eq!(method, Method::Search("hello world".to_string()));
                assert_eq!(target, None);
                assert_eq!(context, None);
            }
            _ => panic!("Expected Request message"),
        }

        // Test with target and context
        let json_with_extras = r#"{"id":2,"method":"cancel","target":"plugin1","context":"ctx1"}"#;
        let message: Message = serde_json::from_str(json_with_extras).unwrap();

        match message {
            Message::Request {
                id,
                method,
                target,
                context,
            } => {
                assert_eq!(id, 2);
                assert_eq!(method, Method::Cancel);
                assert_eq!(target, Some("plugin1".to_string()));
                assert_eq!(context, Some("ctx1".to_string()));
            }
            _ => panic!("Expected Request message"),
        }
    }

    #[test]
    fn test_message_response_raw_json() {
        // Test successful response
        let success_json =
            r#"{"id":1,"result":[{"title":"Test","score":1.0,"actions":[]}],"source":"echo"}"#;
        let message: Message = serde_json::from_str(success_json).unwrap();

        match message {
            Message::Response {
                id,
                result,
                error,
                source,
            } => {
                assert_eq!(id, 1);
                assert_eq!(error, None);
                assert_eq!(source, Some("echo".to_string()));
                assert!(result.is_some());
                if let Some(MethodResult::Matches(results)) = result {
                    assert_eq!(results.len(), 1);
                    assert_eq!(results[0].title, "Test");
                }
            }
            _ => panic!("Expected Response message"),
        }

        // Test error response
        let error_json = r#"{"id":2,"error":"Something went wrong","source":"plugin"}"#;
        let message: Message = serde_json::from_str(error_json).unwrap();

        match message {
            Message::Response {
                id,
                result,
                error,
                source,
            } => {
                assert_eq!(id, 2);
                assert_eq!(result, None);
                assert_eq!(error, Some("Something went wrong".to_string()));
                assert_eq!(source, Some("plugin".to_string()));
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn test_message_notification_raw_json() {
        // Test all notification types with correct JSON format
        // Notifications have the format: {"method": "quit"} (flattened)
        let quit_json = r#"{"method":"quit"}"#;
        let message: Message = serde_json::from_str(quit_json).unwrap();
        match message {
            Message::Notification { method } => assert_eq!(method, Method::Quit),
            _ => panic!("Expected Notification message"),
        }

        let cancel_json = r#"{"method":"cancel"}"#;
        let message: Message = serde_json::from_str(cancel_json).unwrap();
        match message {
            Message::Notification { method } => assert_eq!(method, Method::Cancel),
            _ => panic!("Expected Notification message"),
        }

        let search_json = r#"{"method":"search","params":"test"}"#;
        let message: Message = serde_json::from_str(search_json).unwrap();
        match message {
            Message::Notification { method } => {
                assert_eq!(method, Method::Search("test".to_string()))
            }
            _ => panic!("Expected Notification message"),
        }
    }

    #[test]
    fn test_message_actual_json_formats() {
        // Document and test the actual JSON formats each message type uses

        // Request: has id and method fields flattened at top level
        let request = Message::Request {
            id: 1,
            method: Method::Search("hello".to_string()),
            target: None,
            context: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"id":1,"method":"search","params":"hello","target":null,"context":null}"#
        );

        // Response: has id, result/error, and source at top level
        let response = Message::Response {
            id: 2,
            result: Some(MethodResult::Matches(vec![])),
            error: None,
            source: Some("plugin".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            r#"{"id":2,"error":null,"source":"plugin","result":[]}"#
        );

        // Notification: has method flattened at top level
        let notification = Message::Notification {
            method: Method::Quit,
        };
        let json = serde_json::to_string(&notification).unwrap();
        assert_eq!(json, r#"{"method":"quit"}"#);
    }

    #[test]
    fn test_message_serialization_format() {
        // Test that serialization produces expected JSON structure
        let request = Message::Request {
            id: 42,
            method: Method::Search("test".to_string()),
            target: Some("plugin".to_string()),
            context: Some("ctx".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["method"], "search");
        assert_eq!(parsed["params"], "test");
        assert_eq!(parsed["target"], "plugin");
        assert_eq!(parsed["context"], "ctx");

        let response = Message::Response {
            id: 1,
            result: Some(MethodResult::Matches(vec![])),
            error: None,
            source: Some("test".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["id"], 1);
        assert!(parsed["result"].is_array());
        assert_eq!(parsed["source"], "test");
        assert!(parsed.get("error").is_none() || parsed["error"].is_null());
    }

    #[test]
    fn test_message_round_trip_all_variants() {
        let test_messages = vec![
            Message::Request {
                id: 1,
                method: Method::Search("hello".to_string()),
                target: None,
                context: None,
            },
            Message::Request {
                id: 2,
                method: Method::Cancel,
                target: Some("plugin".to_string()),
                context: Some("ctx".to_string()),
            },
            Message::Response {
                id: 3,
                result: Some(MethodResult::Matches(vec![])),
                error: None,
                source: Some("source".to_string()),
            },
            Message::Response {
                id: 4,
                result: None,
                error: Some("error".to_string()),
                source: None,
            },
            Message::Notification {
                method: Method::Quit,
            },
            Message::Notification {
                method: Method::Search("notification search".to_string()),
            },
        ];

        for original_message in test_messages {
            let json = serde_json::to_string(&original_message).unwrap();
            let deserialized: Message = serde_json::from_str(&json).unwrap();
            assert_eq!(original_message, deserialized);
        }
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;

    #[test]
    fn test_shell_exec_action() {
        let action = Action::ShellExec {
            command: "ls".to_string(),
            args: vec!["-la".to_string(), "/tmp".to_string()],
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        match deserialized {
            Action::ShellExec { command, args } => {
                assert_eq!(command, "ls");
                assert_eq!(args, vec!["-la".to_string(), "/tmp".to_string()]);
            }
            _ => panic!("Expected ShellExec action"),
        }
    }

    #[test]
    fn test_shell_exec_action_no_args() {
        let action = Action::ShellExec {
            command: "clear".to_string(),
            args: vec![],
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        match deserialized {
            Action::ShellExec { command, args } => {
                assert_eq!(command, "clear");
                assert!(args.is_empty());
            }
            _ => panic!("Expected ShellExec action"),
        }
    }

    #[test]
    fn test_open_path_action() {
        let test_paths = vec![
            "/home/user/documents",
            "./relative/path",
            "file with spaces.txt",
            "/special-chars/path_with-dashes.file",
        ];

        for test_path in test_paths {
            let action = Action::OpenPath {
                path: test_path.to_string(),
            };

            let json = serde_json::to_string(&action).unwrap();
            let deserialized: Action = serde_json::from_str(&json).unwrap();

            match deserialized {
                Action::OpenPath { path } => assert_eq!(path, test_path),
                _ => panic!("Expected OpenPath action"),
            }
        }
    }

    #[test]
    fn test_copy_to_clipboard_action() {
        let test_texts = vec![
            "Simple text",
            "",
            "Multi\nLine\nText",
            "Unicode: 🚀 こんにちは ñoño",
            "Special chars: !@#$%^&*()[]{}|\\:;\"'<>?,.`~",
        ];

        for test_text in test_texts {
            let action = Action::Clipboard {
                text: test_text.to_string(),
            };

            let json = serde_json::to_string(&action).unwrap();
            let deserialized: Action = serde_json::from_str(&json).unwrap();

            match deserialized {
                Action::Clipboard { text } => assert_eq!(text, test_text),
                _ => panic!("Expected Clipboard action"),
            }
        }
    }

    #[test]
    fn test_custom_action_string() {
        let action = Action::Custom {
            action: "custom_action".to_string(),
            params: serde_json::json!("string parameter"),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        match deserialized {
            Action::Custom { action, params } => {
                assert_eq!(action, "custom_action");
                assert_eq!(params, serde_json::json!("string parameter"));
            }
            _ => panic!("Expected Custom action"),
        }
    }

    #[test]
    fn test_custom_action_object() {
        let params_obj = serde_json::json!({
            "setting": "value",
            "number": 42,
            "enabled": true,
            "list": [1, 2, 3]
        });

        let action = Action::Custom {
            action: "configure".to_string(),
            params: params_obj.clone(),
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        match deserialized {
            Action::Custom { action, params } => {
                assert_eq!(action, "configure");
                assert_eq!(params, params_obj);
            }
            _ => panic!("Expected Custom action"),
        }
    }

    #[test]
    fn test_custom_action_null() {
        let action = Action::Custom {
            action: "simple_action".to_string(),
            params: serde_json::Value::Null,
        };

        let json = serde_json::to_string(&action).unwrap();
        let deserialized: Action = serde_json::from_str(&json).unwrap();

        match deserialized {
            Action::Custom { action, params } => {
                assert_eq!(action, "simple_action");
                assert_eq!(params, serde_json::Value::Null);
            }
            _ => panic!("Expected Custom action"),
        }
    }

    #[test]
    fn test_all_action_types_round_trip() {
        let actions = vec![
            Action::ShellExec {
                command: "test".to_string(),
                args: vec!["arg1".to_string(), "arg2".to_string()],
            },
            Action::OpenPath {
                path: "/test/path".to_string(),
            },
            Action::Clipboard {
                text: "test clipboard".to_string(),
            },
            Action::Custom {
                action: "test_custom".to_string(),
                params: serde_json::json!({"key": "value"}),
            },
        ];

        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let deserialized: Action = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            assert_eq!(json, json2);
        }
    }
}

#[cfg(test)]
mod search_item_tests {
    use super::*;

    #[test]
    fn test_search_item_minimal() {
        let item = Match {
            title: "Minimal Item".to_string(),
            subtitle: None,
            icon: None,
            actions: vec![],
            score: 1.0,
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: Match = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.title, "Minimal Item");
        assert_eq!(deserialized.subtitle, None);
        assert_eq!(deserialized.icon, None);
        assert!(deserialized.actions.is_empty());
        assert_eq!(deserialized.score, 1.0);
    }

    #[test]
    fn test_search_item_full() {
        let item = Match {
            title: "Full Item".to_string(),
            subtitle: Some("With subtitle".to_string()),
            icon: Some("icon.png".to_string()),
            actions: vec![
                Action::OpenPath {
                    path: "/test".to_string(),
                },
                Action::Clipboard {
                    text: "copy this".to_string(),
                },
            ],
            score: 0.85,
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: Match = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.title, "Full Item");
        assert_eq!(deserialized.subtitle, Some("With subtitle".to_string()));
        assert_eq!(deserialized.icon, Some("icon.png".to_string()));
        assert_eq!(deserialized.actions.len(), 2);
        assert_eq!(deserialized.score, 0.85);
    }

    #[test]
    fn test_search_item_score_values() {
        let test_scores = vec![0.0, 0.5, 1.0, -1.0, 999.99, f64::MAX, f64::MIN];

        for score in test_scores {
            let item = Match {
                title: format!("Score test {}", score),
                subtitle: None,
                icon: None,
                actions: vec![],
                score,
            };

            let json = serde_json::to_string(&item).unwrap();
            let deserialized: Match = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.score, score);
        }
    }

    #[test]
    fn test_search_item_unicode_content() {
        let item = Match {
            title: "Unicode Test 🚀".to_string(),
            subtitle: Some("こんにちは world ñoño".to_string()),
            icon: Some("🔍.png".to_string()),
            actions: vec![Action::Clipboard {
                text: "Unicode: ñáéíóú 🎉".to_string(),
            }],
            score: 0.95,
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: Match = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.title, "Unicode Test 🚀");
        assert_eq!(
            deserialized.subtitle,
            Some("こんにちは world ñoño".to_string())
        );
        assert_eq!(deserialized.icon, Some("🔍.png".to_string()));
    }

    #[test]
    fn test_search_item_multiple_actions() {
        let actions = vec![
            Action::ShellExec {
                command: "open".to_string(),
                args: vec!["file.txt".to_string()],
            },
            Action::OpenPath {
                path: "/test".to_string(),
            },
            Action::Clipboard {
                text: "clipboard".to_string(),
            },
            Action::Custom {
                action: "custom".to_string(),
                params: serde_json::json!({"data": "value"}),
            },
        ];

        let item = Match {
            title: "Multi Action Item".to_string(),
            subtitle: Some("Has many actions".to_string()),
            icon: None,
            actions,
            score: 0.75,
        };

        let json = serde_json::to_string(&item).unwrap();
        let deserialized: Match = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.actions.len(), 4);

        // Test each action type was preserved
        match &deserialized.actions[0] {
            Action::ShellExec { command, args } => {
                assert_eq!(command, "open");
                assert_eq!(args, &vec!["file.txt".to_string()]);
            }
            _ => panic!("Expected ShellExec action"),
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_full_request_response_cycle() {
        // Create a search request
        let request = Message::Request {
            id: 1,
            method: Method::Search("test query".to_string()),
            target: Some("echo-plugin".to_string()),
            context: Some("user-search".to_string()),
        };

        // Serialize and deserialize request
        let request_json = serde_json::to_string(&request).unwrap();
        let deserialized_request: Message = serde_json::from_str(&request_json).unwrap();

        // Create corresponding response
        let search_items = vec![Match {
            title: "Echo: test query".to_string(),
            subtitle: Some("From echo plugin".to_string()),
            icon: Some("echo.png".to_string()),
            actions: vec![
                Action::Clipboard {
                    text: "test query".to_string(),
                },
                Action::Custom {
                    action: "echo_action".to_string(),
                    params: serde_json::json!({"original": "test query"}),
                },
            ],
            score: 1.0,
        }];

        let response = Message::Response {
            id: 1,
            error: None,
            source: Some("echo-plugin".to_string()),
            result: Some(MethodResult::Matches(search_items)),
        };

        // Serialize and deserialize response
        let response_json = serde_json::to_string(&response).unwrap();
        let deserialized_response: Message = serde_json::from_str(&response_json).unwrap();

        // Verify the cycle worked correctly
        match (deserialized_request, deserialized_response) {
            (
                Message::Request {
                    id: req_id, method, ..
                },
                Message::Response {
                    id: resp_id,
                    result: Some(MethodResult::Matches(items)),
                    ..
                },
            ) => {
                assert_eq!(req_id, resp_id);
                assert_eq!(method, Method::Search("test query".to_string()));
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].title, "Echo: test query");
            }
            _ => panic!("Expected matching Request and Response messages"),
        }
    }

    #[test]
    fn test_error_response_cycle() {
        let request = Message::Request {
            id: 42,
            method: Method::Search("invalid query".to_string()),
            target: Some("non-existent-plugin".to_string()),
            context: None,
        };

        let error_response = Message::Response {
            id: 42,
            error: Some("Plugin 'non-existent-plugin' not found".to_string()),
            source: Some("daemon".to_string()),
            result: None,
        };

        // Test serialization round-trip
        let request_json = serde_json::to_string(&request).unwrap();
        let response_json = serde_json::to_string(&error_response).unwrap();

        let _: Message = serde_json::from_str(&request_json).unwrap();
        let deserialized_response: Message = serde_json::from_str(&response_json).unwrap();

        match deserialized_response {
            Message::Response {
                id,
                error,
                source,
                result,
            } => {
                assert_eq!(id, 42);
                assert_eq!(
                    error,
                    Some("Plugin 'non-existent-plugin' not found".to_string())
                );
                assert_eq!(source, Some("daemon".to_string()));
                assert_eq!(result, None);
            }
            _ => panic!("Expected Response message with error"),
        }
    }

    #[test]
    fn test_notification_broadcast() {
        let notifications = vec![
            Message::Notification {
                method: Method::Cancel,
            },
            Message::Notification {
                method: Method::Quit,
            },
        ];

        for notification in notifications {
            let json = serde_json::to_string(&notification).unwrap();
            let deserialized: Message = serde_json::from_str(&json).unwrap();

            match (notification, deserialized) {
                (
                    Message::Notification {
                        method: orig_method,
                    },
                    Message::Notification {
                        method: deser_method,
                    },
                ) => {
                    assert_eq!(orig_method, deser_method);
                }
                _ => panic!("Notification serialization failed"),
            }
        }
    }

    #[test]
    fn test_large_payload_handling() {
        // Create a response with many search results
        let mut items = Vec::new();
        for i in 0..1000 {
            items.push(Match {
                title: format!("Item {}", i),
                subtitle: Some(format!("Description for item {}", i)),
                icon: Some(format!("icon_{}.png", i)),
                actions: vec![
                    Action::OpenPath {
                        path: format!("/path/to/item/{}", i),
                    },
                    Action::Clipboard {
                        text: format!("Item {} content", i),
                    },
                    Action::Custom {
                        action: "item_action".to_string(),
                        params: serde_json::json!({
                            "id": i,
                            "metadata": {
                                "created": "2024-01-01",
                                "tags": ["test", "large", "payload"]
                            }
                        }),
                    },
                ],
                score: 1.0 - (i as f64 / 1000.0), // Decreasing scores
            });
        }

        let large_response = Message::Response {
            id: 999,
            error: None,
            source: Some("large-plugin".to_string()),
            result: Some(MethodResult::Matches(items)),
        };

        // Test that large payloads can be serialized and deserialized
        let json = serde_json::to_string(&large_response).unwrap();
        assert!(json.len() > 100_000); // Ensure it's actually large

        let deserialized: Message = serde_json::from_str(&json).unwrap();

        match deserialized {
            Message::Response {
                result: Some(MethodResult::Matches(items)),
                ..
            } => {
                assert_eq!(items.len(), 1000);
                assert_eq!(items[0].title, "Item 0");
                assert_eq!(items[999].title, "Item 999");
                assert_eq!(items[0].score, 1.0);
                assert!(items[999].score < 0.01);
            }
            _ => panic!("Expected large SearchResults response"),
        }
    }
}

#[cfg(test)]
mod malformed_json_tests {
    use super::*;

    #[test]
    fn test_missing_required_fields() {
        // Test missing method in Method enum
        let invalid_json = r#"{"params":"test"}"#;
        assert!(serde_json::from_str::<Method>(invalid_json).is_err());

        // Test missing title in SearchItem
        let invalid_item = r#"{"subtitle":"test","score":1.0,"actions":[]}"#;
        assert!(serde_json::from_str::<Match>(invalid_item).is_err());
    }

    #[test]
    fn test_invalid_enum_variants() {
        // Test invalid Method variant
        let invalid_method = r#"{"method":"InvalidMethod","params":"test"}"#;
        assert!(serde_json::from_str::<Method>(invalid_method).is_err());

        // Test invalid Message - Message doesn't have a "type" field, it's untagged
        let invalid_message = r#"{"invalid_field_name":123}"#;
        assert!(serde_json::from_str::<Message>(invalid_message).is_err());
    }

    #[test]
    fn test_type_mismatches() {
        // Test string where number expected
        let invalid_id =
            r#"{"type":"Request","id":"not_a_number","method":"Search","params":"test"}"#;
        assert!(serde_json::from_str::<Message>(invalid_id).is_err());

        // Test number where string expected
        let invalid_title = r#"{"title":123,"score":1.0,"actions":[]}"#;
        assert!(serde_json::from_str::<Match>(invalid_title).is_err());
    }

    #[test]
    fn test_unknown_fields_ignored() {
        // Test that unknown fields are ignored (forward compatibility)
        let json_with_extra = r#"{"method":"search","params":"test","unknown_field":"ignored"}"#;
        let method: Method = serde_json::from_str(json_with_extra).unwrap();
        assert_eq!(method, Method::Search("test".to_string()));

        let item_with_extra = r#"{
            "title":"Test",
            "score":1.0,
            "actions":[],
            "future_field":"should_be_ignored",
            "another_unknown":42
        }"#;
        let item: Match = serde_json::from_str(item_with_extra).unwrap();
        assert_eq!(item.title, "Test");
    }
}
