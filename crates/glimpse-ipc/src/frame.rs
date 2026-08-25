use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Body {
    Hello {
        protocol: u32,
    },
    HelloAck {
        protocol: u32,
        daemon_version: String,
    },
    Subscribe {
        pattern: String,
    },
    SubscribeAck {
        matched: usize,
    },
    Unsubscribe {
        pattern: String,
    },
    Get {
        topic: String,
    },
    GetResult(Status<Option<Event>>),
    Call {
        command: String,
        args: Value,
    },
    CallResult(Status<Value>),
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status<T> {
    Ok { value: T },
    Error { error: CallError },
}

impl<T> From<Status<T>> for Result<T, CallError> {
    fn from(status: Status<T>) -> Self {
        match status {
            Status::Ok { value } => Ok(value),
            Status::Error { error } => Err(error),
        }
    }
}

impl<T> From<Result<T, CallError>> for Status<T> {
    fn from(result: Result<T, CallError>) -> Self {
        match result {
            Ok(value) => Self::Ok { value },
            Err(error) => Self::Error { error },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub topic: String,
    pub seq: u64,
    pub ts: u64,
    pub stale: bool,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct CallError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl CallError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            // One site owns the rule, so `retryable` cannot drift between the services that produce
            // errors and the clients whose whole decision procedure it is.
            retryable: matches!(
                code,
                ErrorCode::NotReady
                    | ErrorCode::Unavailable
                    | ErrorCode::Timeout
                    | ErrorCode::LimitExceeded
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProtocolVersion,
    UnknownTopic,
    UnknownCommand,
    InvalidArgs,
    NotReady,
    Unavailable,
    Timeout,
    LimitExceeded,
    Internal,
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(frame: &Frame) -> Frame {
        let line = serde_json::to_string(frame).expect("serialize");
        serde_json::from_str(&line).expect("deserialize")
    }

    #[test]
    fn every_body_survives_a_round_trip() {
        let event = Event {
            topic: "audio.volume".into(),
            seq: 7,
            ts: 1_700_000_000_000,
            stale: false,
            data: serde_json::json!({ "percent": 42 }),
        };

        let bodies = [
            Body::Hello { protocol: 1 },
            Body::HelloAck {
                protocol: 1,
                daemon_version: "0.16.0".into(),
            },
            Body::Subscribe {
                pattern: "audio.*".into(),
            },
            Body::SubscribeAck { matched: 3 },
            Body::Unsubscribe {
                pattern: "audio.*".into(),
            },
            Body::Get {
                topic: "audio.volume".into(),
            },
            Body::GetResult(Status::Ok {
                value: Some(event.clone()),
            }),
            Body::GetResult(Status::Ok { value: None }),
            Body::Call {
                command: "audio.set_volume".into(),
                args: serde_json::json!({ "volume": 0.42 }),
            },
            Body::CallResult(Status::Error {
                error: CallError::new(ErrorCode::NotReady, "the sink is not ready"),
            }),
            Body::Event(event),
        ];

        for body in bodies {
            let frame = Frame { id: Some(9), body };
            assert_eq!(round_trip(&frame), frame);
        }
    }

    #[test]
    fn a_frame_without_an_id_omits_the_field() {
        let frame = Frame {
            id: None,
            body: Body::Hello { protocol: 1 },
        };
        let line = serde_json::to_string(&frame).expect("serialize");
        assert_eq!(line, r#"{"type":"hello","data":{"protocol":1}}"#);
    }

    #[test]
    fn an_unknown_field_does_not_kill_the_connection() {
        let line = r#"{"type":"hello","data":{"protocol":1,"nickname":"from the future"}}"#;
        let frame: Frame = serde_json::from_str(line).expect("deserialize");
        assert_eq!(frame.body, Body::Hello { protocol: 1 });
    }

    #[test]
    fn an_unknown_error_code_stays_actionable() {
        let error: CallError =
            serde_json::from_str(r#"{"code":"kettle","message":"","retryable":true}"#)
                .expect("deserialize");
        assert_eq!(error.code, ErrorCode::Unknown);
        assert!(error.retryable);
    }
}
