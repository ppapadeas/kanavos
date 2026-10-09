//! Kanavos headless command-line interface: create, inspect, script and export projects, and serve the control
//! protocol and MCP over stdio.
//!
//! Exit codes: 0 success, 1 failure (a command did not apply, a file could not be read or written, a view does not
//! exist), 2 usage error, 3 the requested format is not available in this build.

mod docs;

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kanavos_control::{Config, RpcError, Server, codes};
use kanavos_engine::{Command, Session};
use kanavos_i18n::{t, tf};
use kanavos_model::{Category, Document};
use kanavos_units::{AreaDisplay, LengthDisplay, Locale, format_level};
use serde_json::{Value, json};

const FAILED: u8 = 1;
const USAGE: u8 = 2;
const NOT_AVAILABLE: u8 = 3;

/// A failure with its exit code.
#[derive(Debug)]
struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    fn usage(message: String) -> Failure {
        Failure { code: USAGE, message }
    }
    fn failed(message: String) -> Failure {
        Failure { code: FAILED, message }
    }
}

type Result<T> = std::result::Result<T, Failure>;

/// The language from `KANAVOS_LANG`, then the usual locale variables; Greek when nothing is set.
fn env_locale() -> Locale {
    for var in ["KANAVOS_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(v) = std::env::var(var) else { continue };
        let v = v.trim().to_lowercase();
        if v.is_empty() || v == "c" || v == "posix" {
            continue;
        }
        return if v.starts_with("el") { Locale::El } else { Locale::En };
    }
    Locale::El
}

fn parse_locale(s: &str) -> Option<Locale> {
    match s.trim().to_lowercase().as_str() {
        "el" | "gr" | "greek" => Some(Locale::El),
        "en" | "english" => Some(Locale::En),
        _ => None,
    }
}

/// Parsed arguments of a subcommand.
struct Args {
    positional: Vec<String>,
    options: BTreeMap<String, String>,
    flags: Vec<String>,
}

impl Args {
    fn option(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }
    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }
}

/// Splits arguments into positionals, `--name value` / `--name=value` options and `--flag`s.
fn parse_args(locale: Locale, raw: &[String], with_value: &[&str], flags: &[&str]) -> Result<Args> {
    let mut out = Args { positional: Vec::new(), options: BTreeMap::new(), flags: Vec::new() };
    let mut it = raw.iter();
    while let Some(a) = it.next() {
        let Some(name) = a.strip_prefix("--") else {
            out.positional.push(a.clone());
            continue;
        };
        let (name, inline) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v.to_owned())),
            None => (name, None),
        };
        if with_value.contains(&name) {
            let value = match inline {
                Some(v) => v,
                None => it
                    .next()
                    .cloned()
                    .ok_or_else(|| Failure::usage(tf(locale, "cli.err.needs_value", &[("option", name)])))?,
            };
            out.options.insert(name.to_owned(), value);
        } else if flags.contains(&name) && inline.is_none() {
            out.flags.push(name.to_owned());
        } else {
            return Err(Failure::usage(tf(locale, "cli.err.unknown_option", &[("option", a)])));
        }
    }
    Ok(out)
}

fn one_positional<'a>(locale: Locale, a: &'a Args, what: &str) -> Result<&'a str> {
    match a.positional.as_slice() {
        [one] => Ok(one),
        [] => Err(Failure::usage(tf(locale, "cli.err.missing", &[("what", what)]))),
        _ => Err(Failure::usage(t(locale, "cli.err.too_many").into_owned())),
    }
}

fn load(locale: Locale, path: &Path) -> Result<Document> {
    let loaded = kanavos_engine::io::load(path).map_err(|e| {
        let code =
            if matches!(e, kanavos_engine::io::IoError::NotAvailable(_)) { NOT_AVAILABLE } else { FAILED };
        Failure { code, message: e.message(locale) }
    })?;
    for w in &loaded.warnings {
        eprintln!("{}: {w}", t(locale, "cli.warning"));
    }
    Ok(loaded.document)
}

fn save(locale: Locale, path: &Path, doc: &Document) -> Result<()> {
    kanavos_engine::io::save(path, doc).map_err(|e| Failure::failed(e.message(locale)))
}

fn rpc_failure(e: &RpcError) -> Failure {
    let code = match e.code {
        codes::NOT_AVAILABLE => NOT_AVAILABLE,
        codes::INVALID_PARAMS => USAGE,
        _ => FAILED,
    };
    Failure { code, message: e.message.clone() }
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // Global options anywhere on the line: --lang el|en.
    let mut locale = env_locale();
    let mut rest = Vec::with_capacity(raw.len());
    let mut it = raw.into_iter();
    let mut bad_lang = None;
    while let Some(a) = it.next() {
        let value = if a == "--lang" {
            it.next()
        } else if let Some(v) = a.strip_prefix("--lang=") {
            Some(v.to_owned())
        } else {
            rest.push(a);
            continue;
        };
        match value.as_deref().and_then(parse_locale) {
            Some(l) => locale = l,
            None => bad_lang = Some(value.unwrap_or_default()),
        }
    }
    let result = match bad_lang {
        Some(v) => Err(Failure::usage(tf(locale, "cli.err.bad_lang", &[("value", &v)]))),
        None => run(locale, &rest),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            eprintln!("{}: {}", t(locale, "cli.error"), f.message);
            if f.code == USAGE {
                eprintln!("{}", t(locale, "cli.err.see_help"));
            }
            ExitCode::from(f.code)
        }
    }
}

fn run(locale: Locale, args: &[String]) -> Result<()> {
    let (command, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => ("help", &[][..]),
    };
    match command {
        "new" => cmd_new(locale, rest, false),
        "sample" => cmd_new(locale, rest, true),
        "info" => cmd_info(locale, rest),
        "export" => cmd_export(locale, rest),
        "run" => cmd_run(locale, rest),
        "commands" => cmd_commands(locale, rest),
        "serve" => cmd_serve(locale, rest),
        "mcp" => cmd_mcp(locale, rest),
        "demo" => cmd_demo(locale),
        "version" | "--version" | "-V" => {
            println!(
                "kanavos-cli {} (control protocol {}, MCP {})",
                env!("CARGO_PKG_VERSION"),
                kanavos_control::PROTOCOL_VERSION,
                kanavos_mcp::PROTOCOL_VERSIONS.first().copied().unwrap_or_default()
            );
            Ok(())
        }
        "help" | "--help" | "-h" => {
            match rest.first() {
                Some(c) if help_key(c).is_some() => print!("{}", t(locale, help_key(c).unwrap_or_default())),
                Some(c) => {
                    return Err(Failure::usage(tf(locale, "cli.err.unknown_command", &[("command", c)])));
                }
                None => print!("{}", t(locale, "cli.help.main")),
            }
            Ok(())
        }
        other => Err(Failure::usage(tf(locale, "cli.err.unknown_command", &[("command", other)]))),
    }
}

fn help_key(command: &str) -> Option<&'static str> {
    Some(match command {
        "new" => "cli.help.new",
        "sample" => "cli.help.sample",
        "info" => "cli.help.info",
        "export" => "cli.help.export",
        "run" => "cli.help.run",
        "commands" => "cli.help.commands",
        "serve" => "cli.help.serve",
        "mcp" => "cli.help.mcp",
        "demo" => "cli.help.demo",
        _ => return None,
    })
}

/// Prints the subcommand's help and succeeds when `--help` is among its arguments.
fn wants_help(locale: Locale, command: &str, args: &[String]) -> bool {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        if let Some(k) = help_key(command) {
            print!("{}", t(locale, k));
        }
        return true;
    }
    false
}

// ----- new / sample ----------------------------------------------------------------------------------------------

fn cmd_new(locale: Locale, args: &[String], sample: bool) -> Result<()> {
    let name = if sample { "sample" } else { "new" };
    if wants_help(locale, name, args) {
        return Ok(());
    }
    let with_value: &[&str] = if sample { &[] } else { &["template"] };
    let a = parse_args(locale, args, with_value, &["force"])?;
    let file = PathBuf::from(one_positional(locale, &a, &t(locale, "cli.what.file"))?);
    if file.exists() && !a.flag("force") {
        return Err(Failure::failed(tf(locale, "cli.err.exists", &[("path", &file.display().to_string())])));
    }
    let template = if sample { "sample" } else { a.option("template").unwrap_or("starter") };
    let doc = kanavos_control::template(template).map_err(|e| match e {
        kanavos_control::TemplateError::Unknown(n) => {
            Failure::usage(tf(locale, "cli.err.unknown_template", &[("template", &n)]))
        }
        kanavos_control::TemplateError::Failed(e) => Failure::failed(e.message(locale)),
    })?;
    save(locale, &file, &doc)?;
    println!(
        "{}",
        tf(
            locale,
            "cli.new.done",
            &[
                ("path", &file.display().to_string()),
                ("template", template),
                ("elements", &doc.element_count().to_string())
            ]
        )
    );
    Ok(())
}

// ----- info ------------------------------------------------------------------------------------------------------

fn cmd_info(locale: Locale, args: &[String]) -> Result<()> {
    if wants_help(locale, "info", args) {
        return Ok(());
    }
    let a = parse_args(locale, args, &[], &["json"])?;
    let file = PathBuf::from(one_positional(locale, &a, &t(locale, "cli.what.file"))?);
    let doc = load(locale, &file)?;
    if a.flag("json") {
        let server = Server::with_document(Config { locale, ..Config::default() }, doc.clone(), Some(file));
        let mut s = server;
        let info = s.call("session.info", json!({})).map_err(|e| rpc_failure(&e))?;
        let nok = s.call("query.quantities", json!({ "what": "nok" })).map_err(|e| rpc_failure(&e))?;
        let totals = s.call("query.quantities", json!({ "what": "totals" })).map_err(|e| rpc_failure(&e))?;
        let out = json!({
            "document": info.get("documents").and_then(|d| d.get(0)).cloned().unwrap_or(Value::Null),
            "nok": nok,
            "totals": totals,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return Ok(());
    }
    print_info(locale, &doc, Some(&file));
    Ok(())
}

fn print_info(locale: Locale, doc: &Document, file: Option<&Path>) {
    let label = |k: &str| t(locale, k).into_owned();
    println!("{}: {}", label("cli.info.title"), doc.info().name);
    if let Some(f) = file {
        println!("{}: {}", label("cli.info.file"), f.display());
    }
    if !doc.info().designer.is_empty() {
        println!("{}: {}", label("cli.info.designer"), doc.info().designer);
    }
    println!("{}:", label("cli.info.storeys"));
    for s in doc.storeys() {
        println!(
            "  {:<14} {:>7}  {} {}  {} {}",
            s.name,
            format_level(s.elevation, locale),
            label("cli.info.height"),
            LengthDisplay(s.height, locale),
            doc.elements_on(s.id).count(),
            label("cli.info.elements_short"),
        );
    }
    println!("{}: {}", label("cli.info.elements"), doc.element_count());
    let mut counts = Vec::new();
    for c in Category::ALL {
        let n = doc.elements_of(c).count();
        if n > 0 {
            counts.push(format!("{} {n}", t(locale, &format!("cli.category.{}", c.key()))));
        }
    }
    if !counts.is_empty() {
        println!("  {}", counts.join(" · "));
    }
    println!(
        "{}: {} · {}: {}",
        label("cli.info.views"),
        doc.views().count(),
        label("cli.info.sheets"),
        doc.sheets().len()
    );
    let r = kanavos_rules_gr::nok_report(doc);
    if let Some(plot) = r.plot_area {
        println!("{}: {}", label("cli.info.plot"), AreaDisplay(plot, locale));
        println!("  {}: {}", label("cli.info.coverage"), r.meters.coverage.label(locale));
        println!("  {}: {}", label("cli.info.floor_area"), r.meters.floor_area.label(locale));
        println!(
            "  {}: {} / {}",
            label("cli.info.building_height"),
            LengthDisplay(r.height.0, locale),
            LengthDisplay(r.height.1, locale)
        );
        let ok = r.meters.coverage.within_limit()
            && r.meters.floor_area.within_limit()
            && r.height.0 <= r.height.1;
        println!("  {}", label(if ok { "cli.info.within_terms" } else { "cli.info.over_terms" }));
    }
}

// ----- export ----------------------------------------------------------------------------------------------------

fn id_or_name(s: &str) -> Value {
    match s.parse::<u64>() {
        Ok(n) => json!(n),
        Err(_) => json!(s),
    }
}

fn cmd_export(locale: Locale, args: &[String]) -> Result<()> {
    if wants_help(locale, "export", args) {
        return Ok(());
    }
    let a = parse_args(locale, args, &["view", "sheet", "format", "out"], &["sheets", "model"])?;
    let file = PathBuf::from(one_positional(locale, &a, &t(locale, "cli.what.file"))?);
    let out =
        a.option("out").ok_or_else(|| Failure::usage(tf(locale, "cli.err.missing", &[("what", "--out")])))?;
    let format = match a.option("format") {
        Some(f) => f.to_owned(),
        None => Path::new(out)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_owned)
            .ok_or_else(|| Failure::usage(tf(locale, "cli.err.missing", &[("what", "--format")])))?,
    };
    let targets =
        [a.option("view").is_some(), a.option("sheet").is_some(), a.flag("sheets"), a.flag("model")];
    if targets.iter().filter(|x| **x).count() > 1 {
        return Err(Failure::usage(t(locale, "cli.err.one_target").into_owned()));
    }
    let mut params = json!({ "format": format, "path": out });
    if let Some(m) = params.as_object_mut() {
        if let Some(v) = a.option("view") {
            m.insert("view".into(), id_or_name(v));
        }
        if let Some(s) = a.option("sheet") {
            m.insert("sheet".into(), id_or_name(s));
        }
        if a.flag("sheets") {
            m.insert("sheets".into(), json!(true));
        }
    }
    let doc = load(locale, &file)?;
    let mut server = Server::with_document(Config { locale, ..Config::default() }, doc, Some(file));
    let r = server.call("export.run", params).map_err(|e| rpc_failure(&e))?;
    for f in r.get("files").and_then(Value::as_array).into_iter().flatten() {
        println!("{}", tf(locale, "cli.export.wrote", &[("path", f.as_str().unwrap_or_default())]));
    }
    Ok(())
}

// ----- run -------------------------------------------------------------------------------------------------------

fn cmd_run(locale: Locale, args: &[String]) -> Result<()> {
    if wants_help(locale, "run", args) {
        return Ok(());
    }
    let a = parse_args(locale, args, &["out"], &["dry-run"])?;
    let [script, file] = a.positional.as_slice() else {
        return Err(Failure::usage(tf(
            locale,
            "cli.err.missing",
            &[("what", &t(locale, "cli.what.script_and_file"))],
        )));
    };
    let text = if script == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| Failure::failed(e.to_string()))?;
        s
    } else {
        std::fs::read_to_string(script).map_err(|e| {
            Failure::failed(tf(locale, "cli.err.read", &[("path", script), ("reason", &e.to_string())]))
        })?
    };
    let commands = kanavos_engine::log::parse_jsonl_numbered(&text).map_err(|e| {
        Failure::failed(tf(
            locale,
            "cli.run.parse_error",
            &[("line", &e.line.to_string()), ("path", &e.path), ("reason", &e.message)],
        ))
    })?;
    let file = PathBuf::from(file);
    let doc = if file.exists() {
        load(locale, &file)?
    } else {
        eprintln!("{}", tf(locale, "cli.run.new_file", &[("path", &file.display().to_string())]));
        kanavos_model::starter()
    };
    let mut session = Session::new(doc);
    let (mut created, mut changed, mut deleted) = (0usize, 0usize, 0usize);
    for (line, c) in commands.iter() {
        let name = c.name();
        let label = c.describe(session.document(), locale);
        let out = session.run(c.clone()).map_err(|e| {
            Failure::failed(tf(
                locale,
                "cli.run.failed",
                &[("line", &line.to_string()), ("command", name), ("reason", &e.message(locale))],
            ))
        })?;
        created += out.created.len();
        changed += out.changed.len();
        deleted += out.deleted.len();
        println!("{line:>5}  {label}");
    }
    let target = a.option("out").map(PathBuf::from).unwrap_or(file);
    let summary = |key: &str| {
        tf(
            locale,
            key,
            &[
                ("n", &commands.len().to_string()),
                ("created", &created.to_string()),
                ("changed", &changed.to_string()),
                ("deleted", &deleted.to_string()),
                ("path", &target.display().to_string()),
            ],
        )
    };
    if a.flag("dry-run") {
        println!("{}", summary("cli.run.checked"));
        return Ok(());
    }
    save(locale, &target, session.document())?;
    println!("{}", summary("cli.run.done"));
    Ok(())
}

// ----- commands --------------------------------------------------------------------------------------------------

fn cmd_commands(locale: Locale, args: &[String]) -> Result<()> {
    if wants_help(locale, "commands", args) {
        return Ok(());
    }
    let a = parse_args(locale, args, &[], &["markdown", "json"])?;
    if !a.positional.is_empty() {
        return Err(Failure::usage(t(locale, "cli.err.too_many").into_owned()));
    }
    if a.flag("markdown") {
        print!("{}", docs::markdown());
        return Ok(());
    }
    if a.flag("json") {
        let mut s = Server::new(Config { locale, ..Config::default() });
        let list = s.call("command.list", json!({})).map_err(|e| rpc_failure(&e))?;
        println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
        return Ok(());
    }
    let width = kanavos_engine::AnyCommand::NAMES.iter().map(|n| n.len()).max().unwrap_or(0);
    for name in kanavos_engine::AnyCommand::NAMES {
        println!(
            "{name:<width$}  {} — {}",
            kanavos_engine::commands::command_label(name, locale),
            kanavos_engine::commands::command_help(name, locale)
        );
    }
    Ok(())
}

// ----- servers ---------------------------------------------------------------------------------------------------

fn open_for_server(locale: Locale, a: &Args) -> Result<Server> {
    let config = Config { locale, ..Config::default() };
    Ok(match a.positional.as_slice() {
        [] => Server::new(config),
        [file] => {
            let path = PathBuf::from(file);
            let doc = load(locale, &path)?;
            Server::with_document(config, doc, Some(path))
        }
        _ => return Err(Failure::usage(t(locale, "cli.err.too_many").into_owned())),
    })
}

fn cmd_serve(locale: Locale, args: &[String]) -> Result<()> {
    if wants_help(locale, "serve", args) {
        return Ok(());
    }
    let a = parse_args(locale, args, &[], &["stdio"])?;
    if !a.flag("stdio") {
        return Err(Failure::usage(t(locale, "cli.err.serve_stdio").into_owned()));
    }
    let mut server = open_for_server(locale, &a)?;
    kanavos_control::transport::serve_stdio(&mut server).map_err(|e| Failure::failed(e.to_string()))
}

fn cmd_mcp(locale: Locale, args: &[String]) -> Result<()> {
    if wants_help(locale, "mcp", args) {
        return Ok(());
    }
    let a = parse_args(locale, args, &[], &[])?;
    let server = open_for_server(locale, &a)?;
    let mut mcp = kanavos_mcp::McpServer::with_server(server);
    kanavos_mcp::serve_stdio(&mut mcp).map_err(|e| Failure::failed(e.to_string()))
}

// ----- demo ------------------------------------------------------------------------------------------------------

fn cmd_demo(locale: Locale) -> Result<()> {
    let session = kanavos_engine::sample::sample_session().map_err(|e| Failure::failed(e.message(locale)))?;
    println!("{}", tf(locale, "cli.demo.built", &[("n", &session.version().to_string())]));
    for label in session.history_labels(locale).iter().rev().take(5).rev() {
        println!("  … {label}");
    }
    println!();
    print_info(locale, session.document(), None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn options_and_flags_are_parsed() {
        let a = parse_args(
            Locale::En,
            &strings(&["house.kanavos", "--view", "Κάτοψη ισογείου", "--format=svg", "--sheets"]),
            &["view", "format"],
            &["sheets"],
        )
        .unwrap();
        assert_eq!(a.positional, vec!["house.kanavos"]);
        assert_eq!(a.option("view"), Some("Κάτοψη ισογείου"));
        assert_eq!(a.option("format"), Some("svg"));
        assert!(a.flag("sheets"));
        let err = parse_args(Locale::En, &strings(&["--colour", "red"]), &[], &[]).err().unwrap();
        assert_eq!(err.code, USAGE);
        let err = parse_args(Locale::En, &strings(&["--view"]), &["view"], &[]).err().unwrap();
        assert_eq!(err.code, USAGE);
    }

    #[test]
    fn languages_parse() {
        assert_eq!(parse_locale("EL"), Some(Locale::El));
        assert_eq!(parse_locale("en"), Some(Locale::En));
        assert_eq!(parse_locale("fr"), None);
    }

    #[test]
    fn every_subcommand_has_help_in_both_languages() {
        for c in ["new", "sample", "info", "export", "run", "commands", "serve", "mcp", "demo"] {
            let k = help_key(c).unwrap();
            assert!(kanavos_i18n::has(k), "{k}");
        }
        assert!(kanavos_i18n::has("cli.help.main"));
        for c in Category::ALL {
            assert!(kanavos_i18n::has(&format!("cli.category.{}", c.key())));
        }
    }

    #[test]
    fn ids_and_names_are_told_apart() {
        assert_eq!(id_or_name("29"), json!(29));
        assert_eq!(id_or_name("Α-02"), json!("Α-02"));
    }
}
