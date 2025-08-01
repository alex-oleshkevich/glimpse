use serde::{Deserialize, Serialize};

use crate::Request;

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
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn unwrap(&self) -> &T {
        self.params
            .as_ref()
            .expect("JSON-RPC request must have params")
    }

    pub fn notification(method: String, params: Option<T>) -> Self {
        JSONRPCRequest {
            jsonrpc: "2.0".to_string(),
            method,
            params,
            id: serde_json::Value::Null,
        }
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
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn success(id: serde_json::Value, result: T) -> Self {
        JSONRPCResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn success_for(request: &JSONRPCRequest<Request>, result: T) -> String {
        let response = JSONRPCResponse::success(request.id.clone(), result);
        response
            .to_json()
            .expect("Failed to serialize JSON-RPC response")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}
