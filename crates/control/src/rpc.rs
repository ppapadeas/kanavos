//! JSON-RPC 2.0 messages and errors.

use serde_json::{Map, Value, json};

/// Error codes. The negative range below −32000 is reserved by JSON-RPC; Kanavos uses −32000 … −32009.
pub mod codes {
    /// The message is not valid JSON.
    pub const PARSE_ERROR: i64 = -32700;
    /// The message is not a valid JSON-RPC request.
    pub const INVALID_REQUEST: i64 = -32600;
    /// No such method.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The parameters are invalid; `data.path` names the failing one.
    pub const INVALID_PARAMS: i64 = -32602;
    /// An unexpected internal error.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// A command could not be applied; `data` has `kind`, `field` and `message`.
    pub const COMMAND_FAILED: i64 = -32000;
    /// `ifVersion` did not match; `data.documentVersion` is the current version.
    pub const VERSION_CONFLICT: i64 = -32001;
    /// The method needs an open document.
    pub const NO_DOCUMENT: i64 = -32002;
    /// The feature (an exporter or file format) is not part of this build yet.
    pub const NOT_AVAILABLE: i64 = -32003;
    /// A file could not be read or written.
    pub const FILE_ERROR: i64 = -32004;
    /// The document has unsaved changes (closing or replacing it needs `force`).
    pub const UNSAVED_CHANGES: i64 = -32005;
    /// The request is valid but cannot be done (e.g. CSV of a plan).
    pub const UNSUPPORTED: i64 = -32006;
    /// The referenced view, sheet, storey or element does not exist.
    pub const NOT_FOUND: i64 = -32007;
}

/// A JSON-RPC error.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    /// Code, see [`codes`].
    pub code: i64,
    /// Human-readable message in the server's language.
    pub message: String,
    /// Structured details.
    pub data: Option<Value>,
}

impl RpcError {
    /// An error without data.
    pub fn new(code: i64, message: impl Into<String>) -> RpcError {
        RpcError { code, message: message.into(), data: None }
    }

    /// Adds structured details.
    pub fn with_data(mut self, data: Value) -> RpcError {
        self.data = Some(data);
        self
    }

    /// An invalid-parameter error for a path such as `args.kind.start`.
    pub fn invalid_params(path: &str, message: impl Into<String>) -> RpcError {
        let message = message.into();
        RpcError::new(codes::INVALID_PARAMS, format!("{path}: {message}"))
            .with_data(json!({ "path": path, "message": message }))
    }

    /// The error object of a response.
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("code".into(), json!(self.code));
        m.insert("message".into(), json!(self.message));
        if let Some(d) = &self.data {
            m.insert("data".into(), d.clone());
        }
        Value::Object(m)
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// A parsed request or notification.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Request id; `None` for notifications.
    pub id: Option<Value>,
    /// Method name.
    pub method: String,
    /// Parameters (an object; absent parameters are `{}`).
    pub params: Value,
}

/// Reads one request object. Errors carry the id when it could be read, so the error can be answered.
pub fn parse_request(value: Value) -> Result<Request, (Value, RpcError)> {
    let Value::Object(mut m) = value else {
        return Err((Value::Null, RpcError::new(codes::INVALID_REQUEST, "a request must be a JSON object")));
    };
    let id = m.remove("id");
    let reply_id = id.clone().unwrap_or(Value::Null);
    if let Some(i) = &id
        && !(i.is_string() || i.is_number() || i.is_null())
    {
        return Err((
            Value::Null,
            RpcError::new(codes::INVALID_REQUEST, "id must be a string, a number or null"),
        ));
    }
    if m.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err((reply_id, RpcError::new(codes::INVALID_REQUEST, "jsonrpc must be \"2.0\"")));
    }
    let method = match m.remove("method") {
        Some(Value::String(s)) => s,
        _ => return Err((reply_id, RpcError::new(codes::INVALID_REQUEST, "method must be a string"))),
    };
    let params = match m.remove("params") {
        None | Some(Value::Null) => Value::Object(Map::new()),
        Some(Value::Object(p)) => Value::Object(p),
        Some(_) => {
            return Err((
                reply_id,
                RpcError::invalid_params("params", "must be an object (named parameters)"),
            ));
        }
    };
    Ok(Request { id, method, params })
}

/// A success response.
pub fn response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// An error response.
pub fn error_response(id: Value, error: &RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": error.to_value() })
}

/// A notification from the server.
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_checked() {
        let r = parse_request(json!({"jsonrpc":"2.0","id":1,"method":"session.info"})).unwrap();
        assert_eq!(r.method, "session.info");
        assert_eq!(r.params, json!({}));
        let (id, e) = parse_request(json!({"jsonrpc":"1.0","id":7,"method":"x"})).unwrap_err();
        assert_eq!(id, json!(7));
        assert_eq!(e.code, codes::INVALID_REQUEST);
        let (_, e) = parse_request(json!({"jsonrpc":"2.0","id":1,"method":"x","params":[1]})).unwrap_err();
        assert_eq!(e.code, codes::INVALID_PARAMS);
        let n = parse_request(json!({"jsonrpc":"2.0","method":"events.subscribe"})).unwrap();
        assert!(n.id.is_none());
    }
}
