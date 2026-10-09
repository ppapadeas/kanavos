//! The transport-agnostic server: one open document, its session, subscriptions and every method.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kanavos_engine::commands::{Batch, ParseError};
use kanavos_engine::export::{self, ExportError, ExportFormat, ExportTarget};
use kanavos_engine::io::IoError;
use kanavos_engine::{AnyCommand, Command, CommandError, Outcome, Session};
use kanavos_i18n::{t, tf};
use kanavos_model::{Document, Id, ViewKind};
use kanavos_units::Locale;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use crate::query;
use crate::rpc::{self, RpcError, codes};

/// Version of this protocol, reported by `session.info`. Breaking changes bump the major number.
pub const PROTOCOL_VERSION: &str = "1.0";

/// Every method with a one-line description, in documentation order.
pub const METHODS: &[(&str, &str)] = &[
    ("session.info", "Application and protocol versions, language, open document"),
    ("session.set_locale", "Language of messages, labels and generated text (el or en)"),
    ("session.shutdown", "Ends the session after the reply"),
    ("document.new", "New document from a template: starter (default), empty or sample"),
    ("document.open", "Opens a document file"),
    ("document.save", "Saves the document (to `path`, or where it was opened)"),
    ("document.close", "Closes the document"),
    ("document.get", "The whole document as JSON"),
    ("command.list", "Every command with its label, help and JSON schema"),
    ("command.run", "Runs one command as one undo step"),
    ("command.batch", "Runs several commands as one undo step"),
    ("history.undo", "Undoes steps"),
    ("history.redo", "Redoes steps"),
    ("history.list", "The undo and redo steps with labels"),
    ("history.log", "The applied commands as a JSON-lines log"),
    ("query.elements", "Elements filtered by category, storey, tag, name or property"),
    ("query.quantities", "ΝΟΚ figures, schedules, the area table or totals"),
    ("view.list", "Views and sheets"),
    ("view.render", "A view or sheet as drawing JSON, SVG or PNG"),
    ("export.run", "Exports views, sheets or the model to files"),
    (
        "events.subscribe",
        "Change notifications: document.changed, document.opened, document.saved, document.closed",
    ),
    ("events.unsubscribe", "Stops change notifications"),
];

/// Events a client can subscribe to.
pub const EVENTS: &[&str] = &["document.changed", "document.opened", "document.saved", "document.closed"];

/// Server settings.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Language of messages and generated text.
    pub locale: Locale,
    /// Application version reported by `session.info`.
    pub app_version: String,
}

impl Default for Config {
    fn default() -> Self {
        Config { locale: Locale::El, app_version: env!("CARGO_PKG_VERSION").to_owned() }
    }
}

struct Open {
    session: Session,
    path: Option<PathBuf>,
}

/// A control-protocol server. Feed it messages with [`Server::handle`] (any transport), or call methods
/// directly with [`Server::call`] and collect notifications with [`Server::take_events`].
pub struct Server {
    config: Config,
    open: Option<Open>,
    subscriptions: BTreeSet<String>,
    events: Vec<Value>,
    shutdown: bool,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("open", &self.open.as_ref().map(|o| o.session.revision()))
            .field("subscriptions", &self.subscriptions)
            .finish()
    }
}

/// Reads method parameters, reporting the path of a failing value.
fn params<T: DeserializeOwned>(value: Value) -> Result<T, RpcError> {
    serde_path_to_error::deserialize(value).map_err(|e| {
        let path = e.path().to_string();
        let path = if path == "." { "params".to_owned() } else { path };
        RpcError::invalid_params(&path, e.into_inner().to_string())
    })
}

fn parse_error(prefix: &str, e: ParseError) -> RpcError {
    let path = if prefix.is_empty() {
        e.path
    } else if e.path.is_empty() {
        prefix.to_owned()
    } else {
        format!("{prefix}.{}", e.path)
    };
    RpcError::invalid_params(&path, e.message)
}

fn outcome_json(out: &Outcome) -> Value {
    json!({ "created": out.created, "changed": out.changed, "deleted": out.deleted })
}

/// A view or sheet given by id or by name/number.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
enum Ref {
    Id(Id),
    Name(String),
}

impl Server {
    /// A server with no open document.
    pub fn new(config: Config) -> Server {
        Server { config, open: None, subscriptions: BTreeSet::new(), events: Vec::new(), shutdown: false }
    }

    /// A server with a document already open (as from a file at `path`).
    pub fn with_document(config: Config, doc: Document, path: Option<PathBuf>) -> Server {
        let mut s = Server::new(config);
        s.open = Some(Open { session: Session::new(doc), path });
        s
    }

    /// The open document's session, if any.
    pub fn session(&self) -> Option<&Session> {
        self.open.as_ref().map(|o| &o.session)
    }

    /// The server's language.
    pub fn locale(&self) -> Locale {
        self.config.locale
    }

    /// Whether `session.shutdown` was called.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    /// Notifications produced since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.events)
    }

    /// Handles one message (a request, a notification or a batch) and returns the messages to send back: the
    /// response(s) followed by any notifications, each a compact single-line JSON text.
    pub fn handle(&mut self, message: &str) -> Vec<String> {
        let mut out: Vec<Value> = Vec::new();
        match serde_json::from_str::<Value>(message) {
            Err(e) => {
                let err = RpcError::new(
                    codes::PARSE_ERROR,
                    self.msg("cli.rpc.parse_error", &[("reason", &e.to_string())]),
                );
                out.push(rpc::error_response(Value::Null, &err));
            }
            Ok(Value::Array(items)) => {
                if items.is_empty() {
                    let err = RpcError::new(codes::INVALID_REQUEST, self.msg("cli.rpc.empty_batch", &[]));
                    out.push(rpc::error_response(Value::Null, &err));
                } else {
                    let mut replies = Vec::new();
                    for item in items {
                        if let Some(r) = self.handle_value(item) {
                            replies.push(r);
                        }
                    }
                    if !replies.is_empty() {
                        out.push(Value::Array(replies));
                    }
                }
            }
            Ok(v) => {
                if let Some(r) = self.handle_value(v) {
                    out.push(r);
                }
            }
        }
        out.extend(self.take_events());
        out.iter().filter_map(|v| serde_json::to_string(v).ok()).collect()
    }

    fn handle_value(&mut self, v: Value) -> Option<Value> {
        match rpc::parse_request(v) {
            Err((id, e)) => Some(rpc::error_response(id, &e)),
            Ok(req) => {
                let result = self.call(&req.method, req.params);
                let id = req.id?;
                Some(match result {
                    Ok(r) => rpc::response(id, r),
                    Err(e) => rpc::error_response(id, &e),
                })
            }
        }
    }

    fn msg(&self, key: &str, args: &[(&str, &str)]) -> String {
        tf(self.config.locale, key, args)
    }

    fn command_error(&self, e: &CommandError) -> RpcError {
        let mut data = Map::new();
        data.insert("kind".into(), json!(e.kind()));
        data.insert("message".into(), json!(e.message(self.config.locale)));
        if let Some(f) = e.field() {
            data.insert("field".into(), json!(f));
            data.insert("path".into(), json!(format!("args.{f}")));
        }
        if let CommandError::NotFound(id) | CommandError::InUse(id) = e {
            data.insert("id".into(), json!(id));
        }
        RpcError::new(codes::COMMAND_FAILED, e.message(self.config.locale)).with_data(Value::Object(data))
    }

    fn io_error(&self, e: &IoError) -> RpcError {
        let code = match e {
            IoError::NotAvailable(_) => codes::NOT_AVAILABLE,
            _ => codes::FILE_ERROR,
        };
        RpcError::new(code, e.message(self.config.locale))
    }

    fn export_error(&self, e: &ExportError) -> RpcError {
        let code = match e {
            ExportError::NotAvailable(_) => codes::NOT_AVAILABLE,
            ExportError::Unsupported { .. } => codes::UNSUPPORTED,
            ExportError::NotFound(_) => codes::NOT_FOUND,
            ExportError::Failed(_) => codes::INTERNAL_ERROR,
        };
        RpcError::new(code, e.message(self.config.locale))
    }

    fn open_ref(&self) -> Result<&Open, RpcError> {
        self.open
            .as_ref()
            .ok_or_else(|| RpcError::new(codes::NO_DOCUMENT, self.msg("cli.rpc.no_document", &[])))
    }

    fn open_mut(&mut self) -> Result<&mut Open, RpcError> {
        let msg = self.msg("cli.rpc.no_document", &[]);
        self.open.as_mut().ok_or_else(|| RpcError::new(codes::NO_DOCUMENT, msg))
    }

    fn doc(&self) -> Result<&Document, RpcError> {
        Ok(self.open_ref()?.session.document())
    }

    fn version(&self) -> Option<u64> {
        self.open.as_ref().map(|o| o.session.revision())
    }

    fn check_version(&self, if_version: Option<u64>) -> Result<(), RpcError> {
        let current = self.open_ref()?.session.revision();
        match if_version {
            Some(v) if v != current => Err(RpcError::new(
                codes::VERSION_CONFLICT,
                self.msg(
                    "cli.rpc.version_conflict",
                    &[("expected", &v.to_string()), ("current", &current.to_string())],
                ),
            )
            .with_data(json!({ "documentVersion": current }))),
            _ => Ok(()),
        }
    }

    fn emit(&mut self, event: &str, params: Value) {
        if self.subscriptions.contains(event) {
            self.events.push(rpc::notification(event, params));
        }
    }

    fn doc_info(&self) -> Value {
        let Some(o) = &self.open else { return Value::Null };
        let doc = o.session.document();
        json!({
            "path": o.path.as_ref().map(|p| p.display().to_string()),
            "title": doc.info().name,
            "documentVersion": o.session.revision(),
            "dirty": o.session.is_dirty(),
            "steps": o.session.version(),
            "canUndo": o.session.can_undo(),
            "canRedo": o.session.can_redo(),
            "storeys": doc.storeys().iter().map(|s| json!({
                "id": s.id, "name": s.name, "elevation": s.elevation.as_m(), "height": s.height.as_m(),
            })).collect::<Vec<_>>(),
            "elements": doc.element_count(),
            "views": doc.views().count(),
            "sheets": doc.sheets().len(),
        })
    }

    /// Calls a method directly. Notifications it produces are kept for [`Server::take_events`].
    pub fn call(&mut self, method: &str, p: Value) -> Result<Value, RpcError> {
        match method {
            "session.info" => self.session_info(p),
            "session.set_locale" => self.set_locale(p),
            "session.shutdown" => {
                self.shutdown = true;
                Ok(json!({ "ok": true }))
            }
            "document.new" => self.document_new(p),
            "document.open" => self.document_open(p),
            "document.save" => self.document_save(p),
            "document.close" => self.document_close(p),
            "document.get" => {
                let o = self.open_ref()?;
                Ok(json!({ "documentVersion": o.session.revision(), "document": o.session.document() }))
            }
            "command.list" => self.command_list(p),
            "command.run" => self.command_run(p),
            "command.batch" => self.command_batch(p),
            "history.undo" => self.history_step(p, true),
            "history.redo" => self.history_step(p, false),
            "history.list" => self.history_list(),
            "history.log" => {
                let o = self.open_ref()?;
                Ok(json!({ "documentVersion": o.session.revision(), "log": o.session.log_jsonl() }))
            }
            "query.elements" => {
                let q: query::ElementQuery = params(p)?;
                let mut r = query::elements(self.doc()?, &q)?;
                if let Value::Object(m) = &mut r {
                    m.insert("documentVersion".into(), json!(self.version()));
                }
                Ok(r)
            }
            "query.quantities" => {
                let q: query::QuantityQuery = params(p)?;
                let mut r = query::quantities_query(self.doc()?, &q, self.config.locale)?;
                if let Value::Object(m) = &mut r {
                    m.insert("documentVersion".into(), json!(self.version()));
                }
                Ok(r)
            }
            "view.list" => self.view_list(),
            "view.render" => self.view_render(p),
            "export.run" => self.export_run(p),
            "events.subscribe" => self.subscribe(p, true),
            "events.unsubscribe" => self.subscribe(p, false),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                self.msg("cli.rpc.method_not_found", &[("method", other)]),
            )
            .with_data(json!({ "methods": METHODS.iter().map(|(m, _)| *m).collect::<Vec<_>>() }))),
        }
    }

    // ----- session --------------------------------------------------------------------------------------------

    fn session_info(&self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {}
        let _: P = params(p)?;
        let exports: Map<String, Value> =
            ExportFormat::ALL.iter().map(|f| (f.name().to_owned(), json!(f.available()))).collect();
        Ok(json!({
            "name": "Kanavos",
            "version": self.config.app_version,
            "protocol": PROTOCOL_VERSION,
            "locale": self.config.locale,
            "methods": METHODS.iter().map(|(m, _)| *m).collect::<Vec<_>>(),
            "events": EVENTS,
            "documents": self.open.as_ref().map(|_| vec![self.doc_info()]).unwrap_or_default(),
            "commands": AnyCommand::NAMES.len(),
            "exports": exports,
        }))
    }

    fn set_locale(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            locale: Locale,
        }
        let p: P = params(p)?;
        self.config.locale = p.locale;
        Ok(json!({ "locale": p.locale }))
    }

    // ----- documents ------------------------------------------------------------------------------------------

    fn check_can_replace(&self, force: bool) -> Result<(), RpcError> {
        match &self.open {
            Some(o) if o.session.is_dirty() && !force => {
                Err(RpcError::new(codes::UNSAVED_CHANGES, self.msg("cli.rpc.unsaved", &[])))
            }
            _ => Ok(()),
        }
    }

    fn replace(&mut self, doc: Document, path: Option<PathBuf>) {
        self.open = Some(Open { session: Session::new(doc), path });
        let info = self.doc_info();
        self.emit("document.opened", info);
    }

    fn document_new(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default)]
            template: Option<String>,
            #[serde(default)]
            force: bool,
        }
        let p: P = params(p)?;
        self.check_can_replace(p.force)?;
        let doc = template(p.template.as_deref().unwrap_or("starter")).map_err(|e| match e {
            TemplateError::Unknown(name) => RpcError::invalid_params(
                "template",
                format!("unknown template '{name}'; one of starter, empty, sample"),
            ),
            TemplateError::Failed(e) => self.command_error(&e),
        })?;
        self.replace(doc, None);
        Ok(self.doc_info())
    }

    fn document_open(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            path: PathBuf,
            #[serde(default)]
            force: bool,
        }
        let p: P = params(p)?;
        self.check_can_replace(p.force)?;
        let loaded = kanavos_engine::io::load(&p.path).map_err(|e| self.io_error(&e))?;
        self.replace(loaded.document, Some(p.path));
        let mut info = self.doc_info();
        if let Value::Object(m) = &mut info {
            m.insert("warnings".into(), json!(loaded.warnings));
        }
        Ok(info)
    }

    fn document_save(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default)]
            path: Option<PathBuf>,
        }
        let p: P = params(p)?;
        let missing = self.msg("cli.rpc.no_path", &[]);
        let o = self.open_ref()?;
        let path =
            p.path.or_else(|| o.path.clone()).ok_or_else(|| RpcError::invalid_params("path", missing))?;
        kanavos_engine::io::save(&path, o.session.document()).map_err(|e| self.io_error(&e))?;
        let o = self.open_mut()?;
        o.session.mark_saved();
        o.path = Some(path.clone());
        let version = o.session.revision();
        let result = json!({ "path": path.display().to_string(), "documentVersion": version });
        self.emit("document.saved", result.clone());
        Ok(result)
    }

    fn document_close(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default)]
            force: bool,
        }
        let p: P = params(p)?;
        self.open_ref()?;
        self.check_can_replace(p.force)?;
        self.open = None;
        self.emit("document.closed", json!({}));
        Ok(json!({ "closed": true }))
    }

    // ----- commands -------------------------------------------------------------------------------------------

    fn command_list(&self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default)]
            names: Option<Vec<String>>,
            #[serde(default = "yes")]
            schemas: bool,
            #[serde(default)]
            locale: Option<Locale>,
        }
        fn yes() -> bool {
            true
        }
        let p: P = params(p)?;
        let locale = p.locale.unwrap_or(self.config.locale);
        if let Some(names) = &p.names {
            for (i, n) in names.iter().enumerate() {
                if !kanavos_engine::commands::is_command(n) {
                    return Err(RpcError::invalid_params(
                        &format!("names[{i}]"),
                        format!("unknown command '{n}'"),
                    ));
                }
            }
        }
        let list: Vec<Value> = AnyCommand::NAMES
            .iter()
            .filter(|n| p.names.as_ref().is_none_or(|names| names.iter().any(|x| x == *n)))
            .map(|n| {
                let mut m = Map::new();
                m.insert("name".into(), json!(n));
                m.insert("label".into(), json!(kanavos_engine::commands::command_label(n, locale)));
                m.insert("help".into(), json!(kanavos_engine::commands::command_help(n, locale)));
                if p.schemas {
                    m.insert("schema".into(), AnyCommand::args_schema(n).unwrap_or(Value::Null));
                }
                Value::Object(m)
            })
            .collect();
        Ok(json!({ "commands": list }))
    }

    fn run_on_session(&mut self, cmd: AnyCommand, dry_run: bool) -> Result<Value, RpcError> {
        let locale = self.config.locale;
        let o = self.open_ref()?;
        let label = cmd.describe(o.session.document(), locale);
        let name = cmd.name();
        if dry_run {
            let (_, out) = cmd.apply(o.session.document()).map_err(|e| self.command_error(&e))?;
            let mut r = outcome_json(&out);
            if let Value::Object(m) = &mut r {
                m.insert("documentVersion".into(), json!(o.session.revision()));
                m.insert("label".into(), json!(label));
                m.insert("dryRun".into(), json!(true));
            }
            return Ok(r);
        }
        let result = self.open_mut()?.session.run(cmd);
        let out = result.map_err(|e| self.command_error(&e))?;
        let version = self.version();
        let mut r = outcome_json(&out);
        if let Value::Object(m) = &mut r {
            m.insert("documentVersion".into(), json!(version));
            m.insert("label".into(), json!(label));
        }
        let mut event = r.clone();
        if let Value::Object(m) = &mut event {
            m.insert("cause".into(), json!("command"));
            m.insert("command".into(), json!(name));
        }
        self.emit("document.changed", event);
        Ok(r)
    }

    fn command_run(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct P {
            name: String,
            #[serde(default)]
            args: Value,
            #[serde(default)]
            if_version: Option<u64>,
            #[serde(default)]
            dry_run: bool,
        }
        let p: P = params(p)?;
        let cmd = AnyCommand::from_parts(&p.name, p.args).map_err(|e| parse_error("", e))?;
        self.check_version(p.if_version)?;
        self.run_on_session(cmd, p.dry_run)
    }

    fn command_batch(&mut self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct P {
            commands: Vec<Value>,
            #[serde(default)]
            label: String,
            #[serde(default)]
            if_version: Option<u64>,
            #[serde(default)]
            dry_run: bool,
        }
        let p: P = params(p)?;
        if p.commands.is_empty() {
            return Err(RpcError::invalid_params("commands", "must not be empty"));
        }
        let mut commands = Vec::with_capacity(p.commands.len());
        for (i, v) in p.commands.into_iter().enumerate() {
            let c = kanavos_engine::log::command_from_value(v)
                .map_err(|e| parse_error(&format!("commands[{i}]"), e))?;
            commands.push(c);
        }
        self.check_version(p.if_version)?;
        // Find the failing command, if any, so the error can name it.
        let mut scratch = self.doc()?.clone();
        for (i, c) in commands.iter().enumerate() {
            match c.apply(&scratch) {
                Ok((next, _)) => scratch = next,
                Err(e) => {
                    let mut err = self.command_error(&e);
                    if let Some(Value::Object(m)) = &mut err.data {
                        m.insert("index".into(), json!(i));
                        if let Some(f) = e.field() {
                            m.insert("path".into(), json!(format!("commands[{i}].args.{f}")));
                        }
                    }
                    err.message = self
                        .msg("cli.rpc.batch_failed", &[("index", &i.to_string()), ("reason", &err.message)]);
                    return Err(err);
                }
            }
        }
        self.run_on_session(Batch { label: p.label, commands }.into(), p.dry_run)
    }

    // ----- history --------------------------------------------------------------------------------------------

    fn history_step(&mut self, p: Value, undo: bool) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default = "one")]
            steps: usize,
        }
        fn one() -> usize {
            1
        }
        let p: P = params(p)?;
        let o = self.open_mut()?;
        let mut names = Vec::new();
        for _ in 0..p.steps {
            let step = if undo { o.session.undo() } else { o.session.redo() };
            match step {
                Some(n) => names.push(n),
                None => break,
            }
        }
        let version = o.session.revision();
        let (can_undo, can_redo) = (o.session.can_undo(), o.session.can_redo());
        let key = if undo { "undone" } else { "redone" };
        let result =
            json!({ key: names, "documentVersion": version, "canUndo": can_undo, "canRedo": can_redo });
        if !names.is_empty() {
            let cause = if undo { "undo" } else { "redo" };
            self.emit(
                "document.changed",
                json!({ "cause": cause, "steps": names, "documentVersion": version }),
            );
        }
        Ok(result)
    }

    fn history_list(&self) -> Result<Value, RpcError> {
        let o = self.open_ref()?;
        let locale = self.config.locale;
        let steps: Vec<Value> = o
            .session
            .history_entries(locale)
            .into_iter()
            .map(|e| json!({ "name": e.name, "label": e.label }))
            .collect();
        Ok(json!({
            "steps": steps,
            "redo": o.session.redo_labels(locale),
            "documentVersion": o.session.revision(),
            "dirty": o.session.is_dirty(),
        }))
    }

    // ----- views and exports ----------------------------------------------------------------------------------

    fn view_list(&self) -> Result<Value, RpcError> {
        let doc = self.doc()?;
        let views: Vec<Value> = doc
            .views()
            .map(|v| {
                let (kind, storey, marker) = match &v.kind {
                    ViewKind::Plan { storey } => ("plan", Some(*storey), None),
                    ViewKind::Section { marker, .. } => ("section", None, Some(marker.clone())),
                    ViewKind::Elevation { marker, .. } => ("elevation", None, Some(marker.clone())),
                    ViewKind::ThreeD { .. } => ("3d", None, None),
                    ViewKind::SitePlan => ("site_plan", None, None),
                    ViewKind::CoverageDiagram => ("coverage_diagram", None, None),
                    ViewKind::AreaTable => ("area_table", None, None),
                    ViewKind::Schedule { .. } => ("schedule", None, None),
                };
                json!({ "id": v.id, "name": v.name, "kind": kind, "scale": v.scale, "storey": storey,
                        "marker": marker, "hiddenLayers": v.hidden_layers })
            })
            .collect();
        let sheets: Vec<Value> = doc
            .sheets()
            .iter()
            .map(|s| {
                let (w, h) = s.paper.dimensions_mm();
                json!({ "id": s.id, "number": s.number, "name": s.name, "paperMm": [w, h],
                        "viewports": s.viewports.iter().map(|vp| json!({
                            "view": vp.view, "centre": vp.centre, "scale": vp.scale })).collect::<Vec<_>>() })
            })
            .collect();
        Ok(json!({ "views": views, "sheets": sheets, "documentVersion": self.version() }))
    }

    fn resolve_view(&self, doc: &Document, r: &Ref) -> Result<Id, RpcError> {
        let found = match r {
            Ref::Id(id) => doc.view(*id).map(|v| v.id),
            Ref::Name(n) => doc
                .views()
                .find(|v| v.name == *n)
                .or_else(|| doc.views().find(|v| v.name.to_lowercase() == n.to_lowercase()))
                .map(|v| v.id),
        };
        found.ok_or_else(|| {
            RpcError::new(codes::NOT_FOUND, self.msg("cli.rpc.no_view", &[("view", &ref_text(r))]))
                .with_data(json!({ "path": "view" }))
        })
    }

    fn resolve_sheet(&self, doc: &Document, r: &Ref) -> Result<Id, RpcError> {
        let found = match r {
            Ref::Id(id) => doc.sheet(*id).map(|s| s.id),
            Ref::Name(n) => {
                let sheets = doc.sheets();
                sheets
                    .iter()
                    .find(|s| s.number == *n)
                    .or_else(|| sheets.iter().find(|s| s.number.to_lowercase() == n.to_lowercase()))
                    .or_else(|| sheets.iter().find(|s| s.name.to_lowercase() == n.to_lowercase()))
                    .map(|s| s.id)
            }
        };
        found.ok_or_else(|| {
            RpcError::new(codes::NOT_FOUND, self.msg("cli.rpc.no_sheet", &[("sheet", &ref_text(r))]))
                .with_data(json!({ "path": "sheet" }))
        })
    }

    fn view_render(&self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default)]
            view: Option<Ref>,
            #[serde(default)]
            sheet: Option<Ref>,
            #[serde(default)]
            format: Option<String>,
            #[serde(default)]
            locale: Option<Locale>,
            #[serde(default)]
            dpi: Option<f64>,
        }
        let p: P = params(p)?;
        let doc = self.doc()?;
        let locale = p.locale.unwrap_or(self.config.locale);
        let (drawing, title, paper) = match (&p.view, &p.sheet) {
            (Some(v), None) => {
                let id = self.resolve_view(doc, v)?;
                let name = doc.view(id).map(|v| v.name.clone()).unwrap_or_default();
                (export::view_drawing(doc, id, locale).map_err(|e| self.export_error(&e))?, name, None)
            }
            (None, Some(s)) => {
                let id = self.resolve_sheet(doc, s)?;
                let sheet = doc.sheet(id);
                let name = sheet.map(|s| s.name.clone()).unwrap_or_default();
                let paper = sheet.map(|s| s.paper);
                (export::sheet_drawing(doc, id, locale).map_err(|e| self.export_error(&e))?, name, paper)
            }
            _ => return Err(RpcError::invalid_params("view", "give exactly one of `view` and `sheet`")),
        };
        let format = p.format.as_deref().unwrap_or("json");
        let version = self.version();
        match format {
            "json" => Ok(json!({ "format": "json", "documentVersion": version, "drawing": drawing })),
            "svg" => {
                let svg = export::svg(&drawing, &title, paper).map_err(|e| self.export_error(&e))?;
                Ok(json!({ "format": "svg", "documentVersion": version, "svg": svg }))
            }
            "png" => {
                let png = export::png(&drawing, p.dpi.unwrap_or(150.0), paper)
                    .map_err(|e| self.export_error(&e))?;
                Ok(json!({ "format": "png", "documentVersion": version, "png": crate::base64::encode(&png) }))
            }
            other => Err(RpcError::invalid_params(
                "format",
                format!("unknown format '{other}'; one of json, svg, png"),
            )),
        }
    }

    fn export_run(&self, p: Value) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            format: String,
            path: PathBuf,
            #[serde(default)]
            view: Option<Ref>,
            #[serde(default)]
            sheet: Option<Ref>,
            #[serde(default)]
            sheets: bool,
            #[serde(default)]
            locale: Option<Locale>,
        }
        let p: P = params(p)?;
        let format = ExportFormat::parse(&p.format).ok_or_else(|| {
            RpcError::invalid_params(
                "format",
                format!("unknown format '{}'; one of pdf, svg, dxf, png, ifc, gltf, csv, json", p.format),
            )
        })?;
        let doc = self.doc()?;
        let target = match (&p.view, &p.sheet, p.sheets) {
            (Some(v), None, false) => ExportTarget::View { id: self.resolve_view(doc, v)? },
            (None, Some(s), false) => ExportTarget::Sheet { id: self.resolve_sheet(doc, s)? },
            (None, None, true) => ExportTarget::Sheets,
            (None, None, false) => ExportTarget::Model,
            _ => {
                return Err(RpcError::invalid_params(
                    "view",
                    "give at most one of `view`, `sheet` and `sheets`",
                ));
            }
        };
        let locale = p.locale.unwrap_or(self.config.locale);
        let files = export::export(doc, &target, format, locale).map_err(|e| self.export_error(&e))?;
        let written = write_files(&p.path, &files, format).map_err(|(path, e)| {
            RpcError::new(
                codes::FILE_ERROR,
                self.msg("cli.rpc.write_failed", &[("path", &path), ("reason", &e)]),
            )
        })?;
        Ok(json!({ "format": format.name(), "files": written }))
    }

    // ----- events ---------------------------------------------------------------------------------------------

    fn subscribe(&mut self, p: Value, on: bool) -> Result<Value, RpcError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct P {
            #[serde(default)]
            events: Option<Vec<String>>,
        }
        let p: P = params(p)?;
        let names = p.events.unwrap_or_else(|| EVENTS.iter().map(|e| (*e).to_owned()).collect());
        for (i, n) in names.iter().enumerate() {
            if !EVENTS.contains(&n.as_str()) {
                return Err(RpcError::invalid_params(
                    &format!("events[{i}]"),
                    format!("unknown event '{n}'; one of {}", EVENTS.join(", ")),
                ));
            }
        }
        for n in names {
            if on {
                self.subscriptions.insert(n);
            } else {
                self.subscriptions.remove(&n);
            }
        }
        Ok(json!({ "subscribed": self.subscriptions }))
    }
}

fn ref_text(r: &Ref) -> String {
    match r {
        Ref::Id(id) => id.0.to_string(),
        Ref::Name(n) => n.clone(),
    }
}

/// Why a template could not be made.
#[derive(Debug)]
pub enum TemplateError {
    /// No such template.
    Unknown(String),
    /// Building it failed.
    Failed(CommandError),
}

/// A new document from a template name: `starter`, `empty` or `sample`.
pub fn template(name: &str) -> Result<Document, TemplateError> {
    match name {
        "starter" => Ok(kanavos_model::starter()),
        "empty" => Ok(Document::new()),
        "sample" => kanavos_engine::sample::sample_house().map_err(TemplateError::Failed),
        other => Err(TemplateError::Unknown(other.to_owned())),
    }
}

/// Writes exported files. One file goes to `path` (or into it, when `path` is a directory); several files go into
/// the directory `path`, created if needed, named by their stems. Returns the written paths.
pub fn write_files(
    path: &Path,
    files: &[export::ExportFile],
    format: ExportFormat,
) -> Result<Vec<String>, (String, String)> {
    let err = |p: &Path, e: std::io::Error| (p.display().to_string(), e.to_string());
    let name = |f: &export::ExportFile| format!("{}.{}", f.stem, format.extension());
    let mut out = Vec::with_capacity(files.len());
    match files {
        [one] if !path.is_dir() => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent).map_err(|e| err(parent, e))?;
            }
            std::fs::write(path, &one.bytes).map_err(|e| err(path, e))?;
            out.push(path.display().to_string());
        }
        many => {
            std::fs::create_dir_all(path).map_err(|e| err(path, e))?;
            for f in many {
                let p = path.join(name(f));
                std::fs::write(&p, &f.bytes).map_err(|e| err(&p, e))?;
                out.push(p.display().to_string());
            }
        }
    }
    Ok(out)
}

/// The "not available" message of a format, for clients that check before exporting.
pub fn not_available_message(format: ExportFormat, locale: Locale) -> String {
    tf(locale, "engine.export.not_available", &[("format", format.name())])
}

/// Label of a method in the catalogue (`cli.rpc.method.<name>`), falling back to the built-in description.
pub fn method_help(method: &str, locale: Locale) -> String {
    let key = format!("cli.rpc.method.{method}");
    if kanavos_i18n::has(&key) {
        t(locale, &key).into_owned()
    } else {
        METHODS.iter().find(|(m, _)| *m == method).map(|(_, d)| (*d).to_owned()).unwrap_or_default()
    }
}
