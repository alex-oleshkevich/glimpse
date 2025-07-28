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

    pub fn new_notification(method: String, params: Option<T>) -> Self {
        JSONRPCRequest {
            jsonrpc: "2.0".to_string(),
            method,
            params,
            id: serde_json::Value::Null,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_null()
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

impl JSONRPCError {
    pub fn parse_error() -> Self {
        JSONRPCError {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        JSONRPCError {
            code: -32600,
            message: "Invalid Request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        JSONRPCError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn invalid_params() -> Self {
        JSONRPCError {
            code: -32602,
            message: "Invalid params".to_string(),
            data: None,
        }
    }

    pub fn internal_error() -> Self {
        JSONRPCError {
            code: -32603,
            message: "Internal error".to_string(),
            data: None,
        }
    }
}
