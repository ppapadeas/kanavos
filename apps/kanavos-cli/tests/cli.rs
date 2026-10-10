//! The command-line interface end to end: files, scripts, exports, servers and exit codes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kanavos-cli")).args(["--lang", "en"]).args(args).output().unwrap()
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kanavos-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn new_info_and_overwrite_protection() {
    let d = dir("new");
    let file = d.join("a.kanavos");
    let f = file.to_str().unwrap();
    let o = cli(&["new", f]);
    assert!(o.status.success(), "{}", text(&o));
    let o = cli(&["new", f]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o).contains("--force"));
    let o = cli(&["new", f, "--force", "--template", "empty"]);
    assert!(o.status.success());
    let o = cli(&["sample", f, "--force"]);
    assert!(o.status.success());
    let o = cli(&["info", f]);
    assert!(text(&o).contains("Within the building terms"), "{}", text(&o));
    let o = cli(&["info", f, "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["document"]["title"], "Νέα διώροφη κατοικία");
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn scripts_apply_or_name_the_failing_line() {
    let d = dir("run");
    let script = d.join("s.jsonl");
    std::fs::write(
        &script,
        "# comment\n{\"name\":\"storey.create\",\"args\":{\"name\":\"Δώμα\",\"elevation\":6.4,\"height\":3}}\n\
         {\"name\":\"sheet.permit_set\"}\n",
    )
    .unwrap();
    let file = d.join("p.kanavos");
    let o = cli(&["run", script.to_str().unwrap(), file.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("2 commands applied"));
    assert!(file.exists());
    std::fs::write(&script, "{\"name\":\"storey.delete\",\"args\":{\"id\":424242}}\n").unwrap();
    let o = cli(&["run", script.to_str().unwrap(), file.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert!(text(&o).contains("Line 1"), "{}", text(&o));
    // Commands from standard input.
    let mut child = Command::new(env!("CARGO_BIN_EXE_kanavos-cli"))
        .args(["--lang", "en", "run", "-", file.to_str().unwrap(), "--dry-run"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{\"name\":\"sheet.permit_set\"}\n").unwrap();
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success());
    assert!(text(&o).contains("nothing saved"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn exports_write_files_and_report_problems() {
    let d = dir("export");
    let file = d.join("h.kanavos");
    let f = file.to_str().unwrap();
    assert!(cli(&["sample", f]).status.success());
    let pdf = d.join("set.pdf");
    let o = cli(&["export", f, "--sheets", "--out", pdf.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    let svg = d.join("plan.svg");
    let o = cli(&["export", f, "--view", "Κάτοψη ισογείου", "--out", svg.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    let o = cli(&["export", f, "--view", "Missing view", "--out", svg.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    let o = cli(&["export", f, "--view", "29", "--format", "docx", "--out", "x.docx"]);
    assert_eq!(o.status.code(), Some(2));
    let o = cli(&["export", f, "--sheets", "--model", "--out", "x.pdf"]);
    assert_eq!(o.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn usage_errors_exit_with_two() {
    assert_eq!(cli(&["frobnicate"]).status.code(), Some(2));
    assert_eq!(cli(&["new"]).status.code(), Some(2));
    assert_eq!(cli(&["serve"]).status.code(), Some(2));
    assert_eq!(cli(&["info", "a", "b"]).status.code(), Some(2));
    let o =
        Command::new(env!("CARGO_BIN_EXE_kanavos-cli")).args(["--lang", "xx", "version"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(cli(&["help", "export"]).status.success());
    assert!(cli(&["export", "--help"]).status.success());
    assert!(cli(&["version"]).status.success());
}

/// `update` without any network: checks turned off by the environment, usage errors, help.
#[test]
fn update_reports_when_updates_are_off_and_checks_its_arguments() {
    let update = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_kanavos-cli"))
            .env("KANAVOS_NO_UPDATE_CHECK", "1")
            .args(["--lang", "en", "update"])
            .args(args)
            .output()
            .unwrap()
    };
    let o = update(&["--check"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    assert!(text(&o).contains("turned off"), "{}", text(&o));
    let o = update(&["--channel", "test"]);
    assert_eq!(o.status.code(), Some(3), "{}", text(&o));
    let o = update(&["--channel", "nightly"]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("nightly"));
    assert_eq!(update(&["extra"]).status.code(), Some(2));
    assert_eq!(update(&["--force"]).status.code(), Some(2));
    let o = update(&["--help"]);
    assert!(o.status.success());
    assert!(text(&o).contains("--channel stable|test"));
    assert!(text(&cli(&["help"])).contains("update"));
}

#[test]
fn servers_speak_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kanavos-cli"))
        .args(["serve", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"document.new\",\"params\":{\"template\":\"sample\"}}\n\
              {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"query.elements\",\"params\":{\"category\":\"stair\"}}\n",
        )
        .unwrap();
    let o = child.wait_with_output().unwrap();
    let lines: Vec<serde_json::Value> =
        String::from_utf8(o.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["result"]["count"], 1);

    let mut child = Command::new(env!("CARGO_BIN_EXE_kanavos-cli"))
        .args(["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\"}}\n\
              {\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n\
              {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        )
        .unwrap();
    let o = child.wait_with_output().unwrap();
    let lines: Vec<serde_json::Value> =
        String::from_utf8(o.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(lines[1]["result"]["tools"].as_array().unwrap().len() >= 10);
}
