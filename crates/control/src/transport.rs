//! Transports: newline-delimited JSON over a reader and a writer (stdio), and an in-memory transport for tests
//! and embedding.

use std::io::{self, BufRead, Write};

use serde_json::Value;

use crate::Server;

/// Serves newline-delimited JSON-RPC: one message per line in, one message per line out, until the input ends
/// or `session.shutdown` is called. Blank lines are ignored. Each reply is flushed immediately.
pub fn serve<R: BufRead, W: Write>(server: &mut Server, input: R, mut output: W) -> io::Result<()> {
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
        if server.is_shutdown() {
            break;
        }
    }
    Ok(())
}

/// Serves on the process's standard input and output.
pub fn serve_stdio(server: &mut Server) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(server, stdin.lock(), stdout.lock())
}

/// An in-memory transport: send a message text, get the parsed replies and notifications back.
#[derive(Debug)]
pub struct MemoryTransport {
    /// The server.
    pub server: Server,
    next_id: u64,
}

impl MemoryTransport {
    /// Wraps a server.
    pub fn new(server: Server) -> MemoryTransport {
        MemoryTransport { server, next_id: 1 }
    }

    /// Sends a raw message and returns every message sent back.
    pub fn send(&mut self, message: &str) -> Vec<Value> {
        self.server.handle(message).iter().filter_map(|m| serde_json::from_str(m).ok()).collect()
    }

    /// Sends a request with a fresh id and returns its response (the first message) and any notifications.
    pub fn request(&mut self, method: &str, params: Value) -> (Value, Vec<Value>) {
        let id = self.next_id;
        self.next_id += 1;
        let msg = serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let mut replies = self.send(&msg.to_string()).into_iter();
        let response = replies.next().unwrap_or(Value::Null);
        (response, replies.collect())
    }

    /// Sends a request and returns its `result`, or the `error` object as `Err`.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let (r, _) = self.request(method, params);
        match (r.get("result"), r.get("error")) {
            (Some(res), _) => Ok(res.clone()),
            (_, Some(err)) => Err(err.clone()),
            _ => Err(r),
        }
    }
}
