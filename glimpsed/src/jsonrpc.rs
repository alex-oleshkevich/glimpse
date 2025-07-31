use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

static COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCRequest<T = serde_json::Value> {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<T>,
    pub id: serde_json::Value,
}

impl<T> JSONRPCRequest<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    pub fn new(method: String, params: Option<T>) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        JSONRPCRequest {
            id: serde_json::Value::Number(id.into()),
            method,
            params,
            jsonrpc: "2.0".to_string(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCResponse<T = serde_json::Value> {
    pub jsonrpc: String,
    pub result: Option<T>,
    pub error: Option<JSONRPCError>,
    pub id: serde_json::Value,
}

impl<T> JSONRPCResponse<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    pub fn success(id: serde_json::Value, result: T) -> Self {
        JSONRPCResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: serde_json::Value, error: JSONRPCError) -> Self {
        JSONRPCResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}
