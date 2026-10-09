//! JSON-RPC 2.0 control protocol (docs/control-protocol.md).
//!
//! Layer L8. See `docs/architecture.md`.
//!
//! Everything a person can do in Kanavos can be done by a script through the same commands the interface uses.
//! The core, [`Server`], is independent of any transport: it takes one message text and returns the replies and
//! notifications to send back. [`transport::serve`] runs it over newline-delimited JSON (stdio for
//! `kanavos-cli serve --stdio`); [`transport::MemoryTransport`] runs it in memory for tests and embedding.
//!
//! ```
//! use kanavos_control::{Config, Server, transport::MemoryTransport};
//! use serde_json::json;
//!
//! let mut t = MemoryTransport::new(Server::new(Config::default()));
//! t.call("document.new", json!({ "template": "starter" })).unwrap();
//! let storey = t.call("query.elements", json!({})).unwrap();
//! assert_eq!(storey["count"], 0);
//! let r = t.call("command.run", json!({
//!     "name": "storey.create",
//!     "args": { "name": "Δώμα", "elevation": 6.4, "height": 3.0 }
//! })).unwrap();
//! assert_eq!(r["documentVersion"], 1);
//! ```

mod base64;
pub mod query;
pub mod rpc;
mod server;
pub mod transport;

pub use base64::encode as base64_encode;
pub use rpc::{RpcError, codes};
pub use server::{
    Config, EVENTS, METHODS, PROTOCOL_VERSION, Server, TemplateError, method_help, not_available_message,
    template, write_files,
};
