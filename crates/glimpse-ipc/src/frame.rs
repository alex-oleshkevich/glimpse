use serde::{Deserialize, Serialize};
use serde_json::Value;

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
pub struct Frame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub body: Body,
}
