//! MCP server adapter over the control protocol.
//!
//! Layer L9. See `docs/architecture.md` and `docs/control-protocol.md`.
//!
//! `kanavos-cli mcp` speaks the Model Context Protocol over stdio (newline-delimited JSON-RPC 2.0), written here
//! directly with `serde_json`. It offers one capability, tools, each mapped onto a control-protocol method of an
//! embedded [`kanavos_control::Server`]: documents, commands, history, queries, rendering and export. Tool
//! failures (a command that does not apply, a missing view) are tool results with `isError: true` and a
//! readable message, so a model can correct itself; only unknown tools and malformed requests are JSON-RPC errors.

pub mod tools;

use std::io::{self, BufRead, Write};

use kanavos_control::rpc::{self, RpcError, codes};
use kanavos_control::{Config, Server};
use kanavos_i18n::tf;
use serde_json::{Value, json};

/// MCP protocol versions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];

/// JSON-RPC error for requests before `initialize`.
pub const NOT_INITIALIZED: i64 = -32002;

/// An MCP server.
#[derive(Debug)]
pub struct McpServer {
    control: Server,
    protocol: Option<String>,
    ready: bool,
}

impl McpServer {
    /// A server with no open document.
    pub fn new(config: Config) -> McpServer {
        McpServer::with_server(Server::new(config))
    }

    /// A server over an existing control server (e.g. with a document already open).
    pub fn with_server(control: Server) -> McpServer {
        McpServer { control, protocol: None, ready: false }
    }

    /// The negotiated protocol version, after `initialize`.
    pub fn protocol_version(&self) -> Option<&str> {
        self.protocol.as_deref()
    }

    /// The embedded control server.
    pub fn control(&self) -> &Server {
        &self.control
    }

    /// Handles one message and returns the replies to send (none for notifications).
    pub fn handle(&mut self, message: &str) -> Vec<String> {
        let reply = match serde_json::from_str::<Value>(message) {
            Err(e) => {
                Some(rpc::error_response(Value::Null, &RpcError::new(codes::PARSE_ERROR, e.to_string())))
            }
            Ok(Value::Array(_)) => Some(rpc::error_response(
                Value::Null,
                &RpcError::new(codes::INVALID_REQUEST, "batches are not supported"),
            )),
            Ok(v) => match rpc::parse_request(v) {
                Err((id, e)) => Some(rpc::error_response(id, &e)),
                Ok(req) => {
                    let result = self.call(&req.method, req.params);
                    req.id.map(|id| match result {
                        Ok(r) => rpc::response(id, r),
                        Err(e) => rpc::error_response(id, &e),
                    })
                }
            },
        };
        // Control-protocol events are not forwarded: MCP clients poll with the tools.
        let _ = self.control.take_events();
        reply.into_iter().filter_map(|v| serde_json::to_string(&v).ok()).collect()
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        let locale = self.control.locale();
        match method {
            "initialize" => Ok(self.initialize(&params)),
            "notifications/initialized" => {
                self.ready = true;
                Ok(Value::Null)
            }
            "ping" => Ok(json!({})),
            _ if self.protocol.is_none() && !method.starts_with("notifications/") => {
                Err(RpcError::new(NOT_INITIALIZED, tf(locale, "cli.mcp.not_initialized", &[])))
            }
            "tools/list" => {
                Ok(json!({ "tools": tools::TOOLS.iter().map(tools::definition).collect::<Vec<_>>() }))
            }
            "tools/call" => self.tools_call(params),
            m if m.starts_with("notifications/") => Ok(Value::Null),
            other => Err(RpcError::new(codes::METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }

    fn initialize(&mut self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str);
        let version = match asked {
            Some(v) if PROTOCOL_VERSIONS.contains(&v) => v,
            _ => PROTOCOL_VERSIONS.first().copied().unwrap_or("2025-06-18"),
        };
        self.protocol = Some(version.to_owned());
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "kanavos", "title": "Kanavos", "version": env!("CARGO_PKG_VERSION") },
            "instructions": tf(self.control.locale(), "cli.mcp.instructions", &[]),
        })
    }

    fn tools_call(&mut self, params: Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("name", "must be a tool name"))?;
        let tool = tools::find(name).ok_or_else(|| {
            RpcError::new(
                codes::INVALID_PARAMS,
                tf(self.control.locale(), "cli.mcp.unknown_tool", &[("tool", name)]),
            )
        })?;
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        if !args.is_object() {
            return Ok(error_result(&RpcError::invalid_params("arguments", "must be an object")));
        }
        let result = self.control.call(tool.method, tools::method_params(tool, args));
        Ok(match result {
            Ok(r) => success_result(tool, r),
            Err(e) => error_result(&e),
        })
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn success_result(tool: &tools::Tool, r: Value) -> Value {
    if tool.name == "render_view" {
        let content = match r.get("format").and_then(Value::as_str) {
            Some("png") => json!([{ "type": "image", "mimeType": "image/png",
                                    "data": r.get("png").cloned().unwrap_or(Value::Null) }]),
            Some("svg") => {
                json!([{ "type": "text", "text": r.get("svg").and_then(Value::as_str).unwrap_or_default() }])
            }
            _ => json!([{ "type": "text", "text": pretty(&r) }]),
        };
        return json!({ "content": content, "isError": false });
    }
    let mut out = json!({ "content": [{ "type": "text", "text": pretty(&r) }], "isError": false });
    if r.is_object()
        && let Value::Object(m) = &mut out
    {
        m.insert("structuredContent".into(), r);
    }
    out
}

fn error_result(e: &RpcError) -> Value {
    let mut text = e.message.clone();
    if let Some(d) = &e.data {
        text.push('\n');
        text.push_str(&pretty(d));
    }
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// Serves MCP over a reader and a writer: one JSON message per line each way, until the input ends.
pub fn serve<R: BufRead, W: Write>(server: &mut McpServer, input: R, mut output: W) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        for reply in server.handle(&line) {
            output.write_all(reply.as_bytes())?;
            output.write_all(b"\n")?;
        }
        output.flush()?;
    }
    Ok(())
}

/// Serves MCP on standard input and output.
pub fn serve_stdio(server: &mut McpServer) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(server, stdin.lock(), stdout.lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_units::Locale;

    fn send(s: &mut McpServer, msg: Value) -> Value {
        let out = s.handle(&msg.to_string());
        out.first().map(|t| serde_json::from_str(t).unwrap()).unwrap_or(Value::Null)
    }

    fn ready() -> McpServer {
        let mut s = McpServer::new(Config { locale: Locale::En, ..Config::default() });
        let r = send(
            &mut s,
            json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert!(s.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_empty());
        s
    }

    fn call(s: &mut McpServer, tool: &str, args: Value) -> Value {
        send(s, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}}))
    }

    #[test]
    fn requests_before_initialize_are_refused() {
        let mut s = McpServer::new(Config::default());
        let r = send(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}));
        assert_eq!(r["error"]["code"], NOT_INITIALIZED);
        let r = send(&mut s, json!({"jsonrpc":"2.0","id":2,"method":"ping"}));
        assert_eq!(r["result"], json!({}));
    }

    #[test]
    fn initialize_negotiates_the_version_and_declares_tools() {
        let mut s = McpServer::new(Config::default());
        let r = send(
            &mut s,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
        assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], false);
        assert_eq!(r["result"]["serverInfo"]["name"], "kanavos");
        assert!(r["result"]["instructions"].as_str().unwrap().len() > 20);
    }

    #[test]
    fn tools_are_listed_with_object_schemas() {
        let mut s = ready();
        let r = send(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}));
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), tools::TOOLS.len());
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            assert!(!t["description"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn a_modelling_session_through_tools() {
        let mut s = ready();
        let r = call(&mut s, "new_document", json!({"template":"sample"}));
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(r["result"]["structuredContent"]["title"], "Νέα διώροφη κατοικία");
        let r = call(&mut s, "list_commands", json!({"name":"opening.place"}));
        assert!(r["result"]["structuredContent"]["commands"][0]["schema"].is_object());
        let r = call(&mut s, "query_elements", json!({"category":"window","tag":"Θ3"}));
        assert_eq!(r["result"]["structuredContent"]["count"], 4);
        let ground = call(&mut s, "document_info", json!({}))["result"]["structuredContent"]["documents"][0]
            ["storeys"][0]["id"]
            .clone();
        let r = call(
            &mut s,
            "run_command",
            json!({"name":"opening.place","args":{
            "storey": ground, "point": [2.0, 10.0], "kind": "window", "tag": "Θ2", "width": 0.8 }}),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(r["result"]["structuredContent"]["label"], "New window");
        let r = call(&mut s, "undo", json!({}));
        assert_eq!(r["result"]["structuredContent"]["undone"], json!(["opening.place"]));
        let r = call(&mut s, "quantities", json!({"what":"nok"}));
        assert_eq!(r["result"]["structuredContent"]["coverage"]["withinLimit"], true);
        let r = call(&mut s, "render_view", json!({"view":"Κάτοψη ισογείου","dpi":20}));
        assert_eq!(r["result"]["content"][0]["type"], "image");
        assert_eq!(r["result"]["content"][0]["mimeType"], "image/png");
    }

    #[test]
    fn failures_are_tool_errors_and_unknown_tools_protocol_errors() {
        let mut s = ready();
        let r = call(&mut s, "run_command", json!({"name":"sheet.permit_set"}));
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("document"));
        call(&mut s, "new_document", json!({}));
        let r = call(&mut s, "run_command", json!({"name":"element.delete","args":{"ids":"all"}}));
        assert_eq!(r["result"]["isError"], true);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("args.ids"));
        let r = call(&mut s, "fly", json!({}));
        assert_eq!(r["error"]["code"], codes::INVALID_PARAMS);
    }

    #[test]
    fn stdio_exchange() {
        let mut s = McpServer::new(Config::default());
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n"
        );
        let mut out = Vec::new();
        serve(&mut s, input.as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(s.protocol_version(), Some("2025-11-25"));
    }
}
