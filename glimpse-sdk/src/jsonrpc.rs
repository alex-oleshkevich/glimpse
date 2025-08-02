use serde::{Deserialize, Serialize};

use crate::{Request, Response};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCRequest {
    pub jsonrpc: String,
    pub id: Option<usize>,
    pub method: String,
    #[serde(flatten)]
    pub request: Request,
}

impl JSONRPCRequest {
    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_string(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn notification(method: String, request: Request) -> Self {
        JSONRPCRequest {
            jsonrpc: "2.0".to_string(),
            method,
            request,
            id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCResponse {
    pub jsonrpc: String,
    pub result: Response,
    pub error: Option<JSONRPCError>,
    pub id: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<usize>,
}

impl JSONRPCResponse {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn success(request_id: usize, response: Response) -> Self {
        JSONRPCResponse {
            jsonrpc: "2.0".to_string(),
            result: response,
            error: None,
            id: request_id,
            plugin_id: None,
        }
    }

    pub fn with_plugin_id(mut self, plugin_id: usize) -> Self {
        self.plugin_id = Some(plugin_id);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Standard JSON-RPC error codes
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const SERVER_ERROR_START: i32 = -32099;
    pub const SERVER_ERROR_END: i32 = -32000;
}
