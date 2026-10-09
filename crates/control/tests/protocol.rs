//! Scripted exchanges over the in-memory transport.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use kanavos_control::transport::{MemoryTransport, serve};
use kanavos_control::{Config, Server, codes};
use kanavos_units::Locale;
use serde_json::{Value, json};

fn transport() -> MemoryTransport {
    MemoryTransport::new(Server::new(Config { locale: Locale::En, ..Config::default() }))
}

fn sample() -> MemoryTransport {
    let mut t = transport();
    t.call("document.new", json!({ "template": "sample" })).unwrap();
    t
}

fn code(err: &Value) -> i64 {
    err["code"].as_i64().unwrap()
}

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kanavos-control-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn session_info_reports_versions_and_methods() {
    let mut t = transport();
    let info = t.call("session.info", json!({})).unwrap();
    assert_eq!(info["protocol"], "1.0");
    assert_eq!(info["locale"], "en");
    assert!(info["methods"].as_array().unwrap().iter().any(|m| m == "query.elements"));
    assert!(info["documents"].as_array().unwrap().is_empty());
    assert_eq!(info["exports"]["pdf"], true);
}

#[test]
fn methods_need_a_document() {
    let mut t = transport();
    let err = t.call("command.run", json!({ "name": "sheet.permit_set" })).unwrap_err();
    assert_eq!(code(&err), codes::NO_DOCUMENT);
    assert!(err["message"].as_str().unwrap().contains("document.new"));
}

#[test]
fn json_rpc_errors_follow_the_specification() {
    let mut t = transport();
    let r = t.send("{not json");
    assert_eq!(r[0]["error"]["code"], codes::PARSE_ERROR);
    assert_eq!(r[0]["id"], Value::Null);
    let r = t.send(r#"{"jsonrpc":"2.0","id":3,"method":"no.such"}"#);
    assert_eq!(r[0]["error"]["code"], codes::METHOD_NOT_FOUND);
    assert_eq!(r[0]["id"], 3);
    let r = t.send(r#"{"jsonrpc":"2.0","id":"a","method":"session.info","params":[1]}"#);
    assert_eq!(r[0]["error"]["code"], codes::INVALID_PARAMS);
    assert!(t.send("[]")[0]["error"]["code"] == codes::INVALID_REQUEST);
    // A notification gets no reply.
    assert!(t.send(r#"{"jsonrpc":"2.0","method":"session.info"}"#).is_empty());
    // A batch gets one array of replies, without the notifications.
    let r = t.send(
        r#"[{"jsonrpc":"2.0","id":1,"method":"session.info"},{"jsonrpc":"2.0","method":"session.info"},{"jsonrpc":"2.0","id":2,"method":"x"}]"#,
    );
    let replies = r[0].as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1]["error"]["code"], codes::METHOD_NOT_FOUND);
}

#[test]
fn commands_run_with_versions_and_field_paths() {
    let mut t = transport();
    let doc = t.call("document.new", json!({})).unwrap();
    let ground = doc["storeys"][0]["id"].clone();
    assert_eq!(doc["documentVersion"], 0);
    let wall = json!({
        "storey": ground,
        "kind": { "type": "wall", "start": [0, 0], "end": [5, 0], "structure": { "type": "basic", "material": 1, "thickness": 0.25 }, "height": 3 }
    });
    let r = t.call("command.run", json!({ "name": "element.create", "args": wall, "ifVersion": 0 })).unwrap();
    assert_eq!(r["documentVersion"], 1);
    assert_eq!(r["label"], "New wall");
    assert_eq!(r["created"].as_array().unwrap().len(), 1);
    // A stale ifVersion is refused with the current version.
    let err =
        t.call("command.run", json!({ "name": "element.create", "args": wall, "ifVersion": 0 })).unwrap_err();
    assert_eq!(code(&err), codes::VERSION_CONFLICT);
    assert_eq!(err["data"]["documentVersion"], 1);
    // Invalid arguments name the failing path.
    let bad = json!({ "name": "element.create", "args": { "storey": ground, "kind": { "type": "wall", "start": [0, 0], "end": "east" } } });
    let err = t.call("command.run", bad).unwrap_err();
    assert_eq!(code(&err), codes::INVALID_PARAMS);
    assert_eq!(err["data"]["path"], "args.kind.end");
    // Validation errors from the command carry the field.
    let zero = json!({ "name": "element.create", "args": { "storey": ground, "kind": { "type": "wall", "start": [1, 1], "end": [1, 1], "structure": { "type": "basic", "material": 1, "thickness": 0.25 }, "height": 3 } } });
    let err = t.call("command.run", zero).unwrap_err();
    assert_eq!(code(&err), codes::COMMAND_FAILED);
    assert_eq!(err["data"]["path"], "args.end");
    // Unknown commands and unknown parameters.
    let err = t.call("command.run", json!({ "name": "wall.fly" })).unwrap_err();
    assert_eq!(err["data"]["path"], "name");
    let err = t.call("command.run", json!({ "name": "batch", "argz": {} })).unwrap_err();
    assert_eq!(code(&err), codes::INVALID_PARAMS);
    // Dry runs change nothing.
    let r = t.call("command.run", json!({ "name": "element.create", "args": wall, "dryRun": true })).unwrap();
    assert_eq!(r["dryRun"], true);
    assert_eq!(r["documentVersion"], 1);
}

#[test]
fn batches_are_one_step_and_name_the_failing_command() {
    let mut t = transport();
    let doc = t.call("document.new", json!({})).unwrap();
    let ground = doc["storeys"][0]["id"].clone();
    let wall = |x: f64| json!({ "name": "element.create", "args": { "storey": ground, "kind": { "type": "wall", "start": [x, 0], "end": [x, 4], "structure": { "type": "basic", "material": 1, "thickness": 0.2 }, "height": 3 } } });
    let r = t
        .call("command.batch", json!({ "commands": [wall(0.0), wall(3.0)], "label": "Δύο τοίχοι" }))
        .unwrap();
    assert_eq!(r["created"].as_array().unwrap().len(), 2);
    let h = t.call("history.list", json!({})).unwrap();
    assert_eq!(h["steps"].as_array().unwrap().len(), 1);
    assert_eq!(h["steps"][0]["label"], "Δύο τοίχοι");
    let bad = json!({ "name": "element.delete", "args": { "ids": [999999] } });
    let err = t.call("command.batch", json!({ "commands": [wall(6.0), bad] })).unwrap_err();
    assert_eq!(err["data"]["index"], 1);
    let err = t
        .call(
            "command.batch",
            json!({ "commands": [wall(6.0), { "name": "element.delete", "args": { "ids": "x" } }] }),
        )
        .unwrap_err();
    assert_eq!(err["data"]["path"], "commands[1].args.ids");
    assert_eq!(t.call("history.list", json!({})).unwrap()["steps"].as_array().unwrap().len(), 1);
}

#[test]
fn undo_redo_and_events() {
    let mut t = sample();
    let sub = t.call("events.subscribe", json!({ "events": ["document.changed"] })).unwrap();
    assert_eq!(sub["subscribed"], json!(["document.changed"]));
    let walls = t.call("query.elements", json!({ "category": "wall", "limit": 2 })).unwrap();
    let id = walls["elements"][0]["id"].clone();
    let (r, events) = t.request("command.run", json!({ "name": "element.delete", "args": { "ids": [id] } }));
    assert_eq!(r["result"]["label"], "Delete wall");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["method"], "document.changed");
    assert_eq!(events[0]["params"]["cause"], "command");
    let (r, events) = t.request("history.undo", json!({}));
    assert_eq!(r["result"]["undone"], json!(["element.delete"]));
    assert_eq!(events[0]["params"]["cause"], "undo");
    let r = t.call("history.redo", json!({ "steps": 5 })).unwrap();
    assert_eq!(r["redone"].as_array().unwrap().len(), 1);
    t.call("events.unsubscribe", json!({})).unwrap();
    let (_, events) = t.request("history.undo", json!({}));
    assert!(events.is_empty());
    let err = t.call("events.subscribe", json!({ "events": ["document.exploded"] })).unwrap_err();
    assert_eq!(err["data"]["path"], "events[0]");
}

#[test]
fn queries_and_quantities() {
    let mut t = sample();
    let r = t
        .call(
            "query.elements",
            json!({ "category": ["door"], "storey": "Ισόγειο", "fields": ["name", "quantities"] }),
        )
        .unwrap();
    assert_eq!(r["count"], 6);
    assert!(r["elements"][0]["name"].as_str().unwrap().starts_with("door."));
    let nok = t.call("query.quantities", json!({ "what": "nok" })).unwrap();
    assert!((nok["plotArea"].as_f64().unwrap() - 500.0).abs() < 1e-6);
    let table = t.call("query.quantities", json!({ "what": "schedule", "schedule": "windows" })).unwrap();
    assert!(table["rows"].as_array().unwrap().len() >= 3);
    assert!(table["csv"].as_str().unwrap().contains("Θ3"));
    let err = t.call("query.quantities", json!({ "what": "everything" })).unwrap_err();
    assert_eq!(err["data"]["path"], "what");
}

#[test]
fn views_render_and_export() {
    let mut t = sample();
    let list = t.call("view.list", json!({})).unwrap();
    assert!(list["views"].as_array().unwrap().iter().any(|v| v["kind"] == "section"));
    assert!(list["sheets"].as_array().unwrap().len() >= 6);
    let r = t.call("view.render", json!({ "view": "Κάτοψη ισογείου" })).unwrap();
    assert!(!r["drawing"]["items"].as_array().unwrap().is_empty());
    let r = t.call("view.render", json!({ "sheet": "Α-02", "format": "svg" })).unwrap();
    assert!(r["svg"].as_str().unwrap().contains("<svg"));
    let r = t.call("view.render", json!({ "view": "Νότια όψη", "format": "png", "dpi": 30 })).unwrap();
    assert!(r["png"].as_str().unwrap().starts_with("iVBORw0KGgo"));
    let err = t.call("view.render", json!({ "view": "Όψη που δεν υπάρχει" })).unwrap_err();
    assert_eq!(code(&err), codes::NOT_FOUND);
    let out = tmp("sheets");
    let r = t
        .call("export.run", json!({ "format": "pdf", "sheets": true, "path": out.join("set.pdf") }))
        .unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 1);
    let r =
        t.call("export.run", json!({ "format": "svg", "sheets": true, "path": out.join("svg") })).unwrap();
    assert!(r["files"].as_array().unwrap().len() >= 6);
    let r = t.call("export.run", json!({ "format": "ifc", "path": out.join("model.ifc") })).unwrap();
    assert!(std::fs::read_to_string(r["files"][0].as_str().unwrap()).unwrap().contains("IFCPROJECT"));
    let err = t
        .call("export.run", json!({ "format": "csv", "view": "Κάτοψη ισογείου", "path": out.join("x.csv") }))
        .unwrap_err();
    assert_eq!(code(&err), codes::UNSUPPORTED);
    let _ = std::fs::remove_dir_all(out);
}

#[test]
fn documents_save_open_and_guard_unsaved_changes() {
    let mut t = sample();
    let path = tmp("house.kanavos");
    let err = t.call("document.save", json!({})).unwrap_err();
    assert_eq!(err["data"]["path"], "path");
    t.call("document.save", json!({ "path": path })).unwrap();
    let info = t.call("session.info", json!({})).unwrap();
    assert_eq!(info["documents"][0]["dirty"], false);
    t.call("command.run", json!({ "name": "sheet.permit_set" })).unwrap();
    let err = t.call("document.new", json!({})).unwrap_err();
    assert_eq!(code(&err), codes::UNSAVED_CHANGES);
    t.call("document.close", json!({ "force": true })).unwrap();
    let opened = t.call("document.open", json!({ "path": path })).unwrap();
    assert_eq!(opened["title"], "Νέα διώροφη κατοικία");
    assert_eq!(opened["documentVersion"], 0);
    let err = t.call("document.open", json!({ "path": tmp("missing.kanavos") })).unwrap_err();
    assert_eq!(code(&err), codes::FILE_ERROR);
    let _ = std::fs::remove_file(path);
}

#[test]
fn command_list_has_schemas() {
    let mut t = transport();
    let r = t.call("command.list", json!({ "names": ["wall.join"] })).unwrap();
    let c = &r["commands"][0];
    assert_eq!(c["name"], "wall.join");
    assert_eq!(c["label"], "Join walls");
    assert!(c["schema"]["properties"]["first"].is_object());
    assert!(c["example"]["first"].is_number());
    let all = t.call("command.list", json!({ "schemas": false })).unwrap();
    assert!(all["commands"].as_array().unwrap().len() >= 43);
    assert!(all["commands"][0].get("schema").is_none());
}

#[test]
fn stdio_framing_is_one_message_per_line() {
    let mut server = Server::new(Config::default());
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.info\"}\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session.shutdown\"}\n{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"session.info\"}\n";
    let mut out = Vec::new();
    serve(&mut server, input.as_bytes(), &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "stops after shutdown");
    let first: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["id"], 1);
}
