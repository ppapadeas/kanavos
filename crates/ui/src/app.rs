//! The application shell (docs/ux.md §2): menu bar, context bar, tool palette, project tree, view tabs with
//! split view, inspector, status bar, attribute managers, command palette, dialogs, preferences, autosave and
//! recovery, and over-the-air updates.
//!
//! Panels push [`Action`]s; [`KanavosApp::perform`] carries them out at the end of the frame. Document changes
//! always go through engine commands on [`AppState`].

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Frame, Key, KeyboardShortcut, Modifiers, RichText, Stroke, UiBuilder};
use kanavos_engine::commands::{CopyToStorey, CreateSheet, DeleteElements, PermitSet};
use kanavos_model::{Document, Id, Paper, PaperSize, ScheduleKind, ViewKind};
use kanavos_rules_gr::NokReport;
use kanavos_tools::ToolId;
use kanavos_units::Locale;

use crate::actions::{Action, Panel, TabTarget, Window};
use crate::ctxbar::ContextBar;
use crate::dialogs::{self, Answer, Dialogs, Pending};
use crate::exports::{self, ExportKind, ExportSource, ExportSummary};
use crate::icons::{self, Icon};
use crate::imports::{self, ImportError, ImportSummary, Imported};
use crate::inspector::{Inspector, NokView, Page};
use crate::jobs::Background;
use crate::menu::{self, MenuContext};
use crate::mgr::{Managers, UpdateSettings};
use crate::names::{tr, trf};
use crate::persist;
use crate::prefs::Prefs;
use crate::screenshot::ScreenshotHook;
use crate::search::CommandPalette;
use crate::state::{AppState, Notice};
use crate::status::{self, StatusInfo};
use crate::tabs::{self, Pane, TabBody, Workspace};
use crate::theme::{self, Theme, Tokens};
use crate::tree::ProjectTree;
use crate::update::{Done, Stage, UpdateEvent, Updates};

/// How the application starts.
#[derive(Debug, Clone, Default)]
pub struct Startup {
    /// A project to open (from the command line).
    pub file: Option<PathBuf>,
    /// A document to start with instead (tests, embedding).
    pub document: Option<Document>,
}

struct Autosave {
    last: Instant,
    revision: u64,
    running: Option<Receiver<Result<(), String>>>,
}

/// The application.
pub struct KanavosApp {
    state: AppState,
    prefs: Prefs,
    work: Workspace,
    tree: ProjectTree,
    inspector: Inspector,
    ctxbar: ContextBar,
    palette: CommandPalette,
    managers: Managers,
    dialogs: Dialogs,
    nok: Background<NokReport>,
    actions: Vec<Action>,
    screenshot: Option<ScreenshotHook>,
    applied_style: Option<(Theme, f32)>,
    autosave: Autosave,
    /// Exports running on a worker thread.
    exporting: Vec<Receiver<(PathBuf, Result<ExportSummary, String>)>>,
    /// Manifest of the open project file (keeps its creation time when saving again).
    manifest: Option<kanavos_format::Manifest>,
    notice_seen: Option<(Notice, Instant)>,
    /// Update checks, downloads and installation.
    updates: Updates,
    /// An installed update waiting for running exports and autosaves before the restart.
    pending_restart: Option<kanavos_update::Installed>,
    allow_close: bool,
    persist: bool,
    first_frame: bool,
    seen_revision: u64,
}

impl KanavosApp {
    /// Starts the shell.
    pub fn new(startup: Startup) -> KanavosApp {
        let screenshot = ScreenshotHook::from_env();
        let persist = screenshot.is_none();
        let mut prefs = if persist { Prefs::load() } else { Prefs::default() };
        let mut notice = None;
        let mut file = None;
        let mut manifest = None;
        let doc = if let Some(doc) = startup.document {
            doc
        } else if let Some(path) = startup.file.filter(|_| persist) {
            match persist::load(&path) {
                Ok(opened) => {
                    prefs.add_recent(&path);
                    file = Some(path);
                    manifest = opened.manifest;
                    opened.document
                }
                Err(e) => {
                    notice = Some(Notice::error(trf(
                        prefs.locale,
                        "ui.notice.open_failed",
                        &[("why", &e.to_string())],
                    )));
                    kanavos_model::starter()
                }
            }
        } else if !persist || !prefs.first_run_done {
            prefs.first_run_done = true;
            crate::sample::sample_house(prefs.locale)
        } else {
            kanavos_model::starter()
        };
        let mut state = AppState::new(doc);
        state.locale = prefs.locale;
        state.file = file;
        state.notice = notice;
        let work = Workspace::with_defaults(state.doc(), state.active_storey, prefs.split);
        let mut dialogs = Dialogs::default();
        if persist {
            dialogs.recovery = persist::read_recovery();
            prefs.save();
        }
        let revision = state.session.revision();
        KanavosApp {
            state,
            prefs,
            work,
            tree: ProjectTree::default(),
            inspector: Inspector::default(),
            ctxbar: ContextBar::default(),
            palette: CommandPalette::default(),
            managers: Managers::default(),
            dialogs,
            nok: Background::new("nok"),
            actions: Vec::new(),
            screenshot,
            applied_style: None,
            autosave: Autosave { last: Instant::now(), revision, running: None },
            exporting: Vec::new(),
            manifest,
            notice_seen: None,
            // The screenshot hook never touches the network.
            updates: Updates::new(persist),
            pending_restart: None,
            allow_close: false,
            persist,
            first_frame: true,
            seen_revision: revision,
        }
    }

    /// The shared state (for tests and embedding).
    pub fn state(&self) -> &AppState {
        &self.state
    }

    fn notify(&mut self, text: String) {
        self.state.notice = Some(Notice::info(text));
    }

    fn fail(&mut self, text: String) {
        self.state.notice = Some(Notice::error(text));
    }

    fn save_prefs(&self) {
        if self.persist {
            self.prefs.save();
        }
    }

    // ----- Documents ---------------------------------------------------------------------------------------

    fn set_document(
        &mut self,
        doc: Document,
        file: Option<PathBuf>,
        manifest: Option<kanavos_format::Manifest>,
    ) {
        self.manifest = manifest;
        self.state.replace_document(doc, file);
        self.work = Workspace::with_defaults(self.state.doc(), self.state.active_storey, self.prefs.split);
        self.nok.invalidate();
        self.autosave.revision = self.state.session.revision();
        self.seen_revision = self.state.session.revision();
        if self.persist {
            persist::clear_recovery();
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match persist::load(&path) {
            Ok(opened) => {
                self.prefs.add_recent(&path);
                self.save_prefs();
                let name = path.display().to_string();
                self.set_document(opened.document, Some(path), opened.manifest);
                let mut text = trf(self.state.locale, "ui.notice.opened", &[("name", &name)]);
                for w in opened.warnings {
                    text.push_str(" · ");
                    text.push_str(&w);
                }
                self.notify(text);
            }
            Err(e) => {
                self.prefs.recent.retain(|p| p != &path);
                self.fail(trf(self.state.locale, "ui.notice.open_failed", &[("why", &e.to_string())]));
            }
        }
    }

    /// Saves to `path`; returns whether it worked.
    fn save_to(&mut self, path: PathBuf) -> bool {
        // A project without an id (an older file or an import) gets its permanent id with its first save.
        self.state.session.ensure_project_id();
        match persist::save(&path, self.state.doc(), self.manifest.as_ref()) {
            Ok(manifest) => {
                self.manifest = Some(manifest);
                self.state.session.mark_saved();
                self.state.recovered = false;
                self.prefs.add_recent(&path);
                self.save_prefs();
                let name = path.display().to_string();
                self.state.file = Some(path);
                if self.persist {
                    persist::clear_recovery();
                }
                self.notify(trf(self.state.locale, "ui.notice.saved", &[("name", &name)]));
                true
            }
            Err(e) => {
                self.fail(trf(self.state.locale, "ui.notice.save_failed", &[("why", &e.to_string())]));
                false
            }
        }
    }

    /// Saves (asking for a file when there is none); returns whether the project was saved.
    fn save(&mut self, ask: bool) -> bool {
        let path = match (&self.state.file, ask) {
            (Some(p), false) => Some(p.clone()),
            _ if self.persist => dialogs::pick_save(self.state.locale, &self.state.title()),
            _ => None,
        };
        match path {
            Some(p) => self.save_to(p),
            None => false,
        }
    }

    fn continue_pending(&mut self, ctx: &egui::Context, pending: Pending) {
        match pending {
            Pending::New => {
                self.set_document(kanavos_model::starter(), None, None);
                self.notify(tr(self.state.locale, "ui.notice.new_project"));
            }
            Pending::Open(None) => {
                if self.persist
                    && let Some(p) = dialogs::pick_open(self.state.locale)
                {
                    self.open_path(p);
                }
            }
            Pending::Open(Some(p)) => self.open_path(p),
            Pending::OpenSample => {
                let doc = crate::sample::sample_house(self.state.locale);
                self.set_document(doc, None, None);
            }
            Pending::Exit => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Pending::Update => self.install_update(ctx),
        }
    }

    // ----- Updates -----------------------------------------------------------------------------------------

    /// Installs the downloaded update, after the unsaved-changes question. A recovery copy is written first in
    /// every case, so that no work is lost if installing or restarting fails; when the document has unsaved
    /// changes and the copy cannot be written, nothing is installed.
    fn install_update(&mut self, ctx: &egui::Context) {
        if !self.updates.ready() {
            return;
        }
        if self.persist
            && let Err(e) = persist::write_recovery_now(self.state.doc(), self.state.file.as_deref())
            && self.state.is_dirty()
        {
            self.fail(trf(self.state.locale, "ui.update.recovery_failed", &[("why", &e.to_string())]));
            return;
        }
        self.updates.install(ctx);
    }

    /// Once the update is installed and no export or autosave is still writing, starts the new version with the
    /// open project and closes this one.
    fn restart_when_idle(&mut self, ctx: &egui::Context) {
        if self.pending_restart.is_none() {
            return;
        }
        if !self.exporting.is_empty() || self.autosave.running.is_some() {
            // Closing now would cut a file short; look again shortly.
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        let Some(installed) = self.pending_restart.take() else { return };
        if self.persist {
            if self.state.is_dirty() {
                // Changes made since the copy written before installing must be in the copy the new version offers.
                if let Err(e) = persist::write_recovery_now(self.state.doc(), self.state.file.as_deref()) {
                    // The new version is in place; it starts the next time Kanavos is opened.
                    let why = e.to_string();
                    self.fail(trf(self.state.locale, "ui.update.restart_recovery_failed", &[("why", &why)]));
                    self.updates.available = None;
                    self.updates.stage = Stage::Idle;
                    self.updates.open = false;
                    return;
                }
            } else {
                // Nothing unsaved: the new version does not need the safety copy.
                persist::clear_recovery();
            }
        }
        let args: Vec<OsString> = self.state.file.iter().map(|p| p.clone().into_os_string()).collect();
        match kanavos_update::restart(&installed, &args) {
            Ok(()) => {
                self.prefs.split = self.work.split;
                self.save_prefs();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.updates.fail(e),
        }
    }

    /// Acts on a choice made in the update window.
    fn update_event(&mut self, ctx: &egui::Context, event: UpdateEvent) {
        match event {
            UpdateEvent::Install if self.updates.ready() => self.guard(ctx, Pending::Update),
            UpdateEvent::Install => self.updates.download(ctx),
            UpdateEvent::Later | UpdateEvent::Close => {
                self.updates.open = false;
                if matches!(self.updates.stage, Stage::Failed(_) | Stage::UpToDate) {
                    self.updates.stage =
                        if self.updates.available.is_some() { Stage::Offer } else { Stage::Idle };
                }
            }
            UpdateEvent::Skip => {
                if let Some(a) = self.updates.available.take() {
                    self.prefs.skipped_version = Some(a.version.to_string());
                    self.save_prefs();
                }
                self.updates.stage = Stage::Idle;
                self.updates.open = false;
            }
            UpdateEvent::Cancel => self.updates.cancel(),
            UpdateEvent::Retry if self.updates.available.is_some() => self.updates.download(ctx),
            UpdateEvent::Retry => self.updates.check(ctx, self.prefs.update_channel, true),
        }
    }

    /// Runs `pending` now, or asks about unsaved changes first.
    fn guard(&mut self, ctx: &egui::Context, pending: Pending) {
        if self.state.is_dirty() && self.persist {
            self.dialogs.unsaved = Some(pending);
        } else {
            self.continue_pending(ctx, pending);
        }
    }

    // ----- Actions -----------------------------------------------------------------------------------------

    fn focused_table(&mut self) -> Option<kanavos_drawing::table::Table> {
        let state = &self.state;
        match self.work.focused_mut() {
            Some(tabs::Tab { body: TabBody::Table(tv), .. }) => Some(tv.table(state).clone()),
            _ => None,
        }
    }

    fn ensure_plan_focused(&mut self) {
        let is_plan = matches!(
            self.work.focused().map(|t| t.target),
            Some(TabTarget::View(id)) if matches!(self.state.doc().view(id).map(|v| &v.kind), Some(ViewKind::Plan { .. }))
        );
        if !is_plan && let Some(v) = self.state.doc().plan_view_of(self.state.active_storey).map(|v| v.id) {
            let doc = self.state.doc().clone();
            self.work.open(&doc, TabTarget::View(v));
        }
    }

    fn activate_storey(&mut self, id: Id) {
        if self.state.doc().storey(id).is_none() {
            return;
        }
        self.state.active_storey = id;
        if let Some(v) = self.state.doc().plan_view_of(id).map(|v| v.id) {
            let doc = self.state.doc().clone();
            self.work.open(&doc, TabTarget::View(v));
        }
    }

    fn export(&mut self, ctx: &egui::Context, kind: ExportKind) {
        let locale = self.state.locale;
        if !kind.available() {
            self.notify(trf(locale, "ui.notice.not_available", &[("what", &tr(locale, kind.label_key()))]));
            return;
        }
        let source = match kind {
            ExportKind::Csv => match self.focused_table() {
                Some(t) => ExportSource::Table(t),
                None => {
                    self.notify(tr(locale, "ui.notice.csv_needs_table"));
                    return;
                }
            },
            ExportKind::SheetsPdf => match self.work.focused().map(|t| t.target) {
                Some(TabTarget::Sheet(id)) => ExportSource::Sheet(id),
                _ => ExportSource::Sheets(self.state.doc().sheets().iter().map(|s| s.id).collect()),
            },
            ExportKind::Ifc | ExportKind::Gltf => ExportSource::Model,
            ExportKind::Dxf | ExportKind::Svg | ExportKind::Png => {
                match self.work.focused().map(|t| t.target) {
                    Some(TabTarget::View(id)) => ExportSource::View(id),
                    Some(TabTarget::Sheet(id)) => ExportSource::Sheet(id),
                    _ => ExportSource::Model,
                }
            }
        };
        let suggested = match self.work.focused() {
            Some(t) => tabs::title(&self.state, t.target),
            None => self.state.title(),
        };
        if !self.persist {
            return;
        }
        let Some(path) = dialogs::pick_export(locale, kind, &suggested) else { return };
        let wake = ctx.clone();
        let doc = self.state.doc().clone();
        let rx = exports::spawn(kind, doc, source, locale, path, move || wake.request_repaint());
        self.exporting.push(rx);
        self.notify(tr(locale, "ui.notice.exporting"));
    }

    /// Reports finished exports.
    fn poll_exports(&mut self) {
        let locale = self.state.locale;
        let mut done = Vec::new();
        self.exporting.retain(|rx| match rx.try_recv() {
            Ok(r) => {
                done.push(r);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        for (path, r) in done {
            match r {
                Ok(summary) => {
                    let mut text =
                        trf(locale, "ui.notice.exported", &[("name", &path.display().to_string())]);
                    if summary.warnings > 0 {
                        let n = summary.warnings.to_string();
                        text.push_str(" · ");
                        text.push_str(&trf(locale, "ui.notice.export_warnings", &[("n", &n)]));
                    }
                    self.notify(text);
                }
                Err(why) => self.fail(trf(locale, "ui.notice.export_failed", &[("why", &why)])),
            }
        }
    }

    fn import(&mut self, ctx: &egui::Context, kind: imports::ImportKind) {
        let locale = self.state.locale;
        if !kind.available() {
            self.notify(trf(locale, "ui.notice.not_available", &[("what", &tr(locale, kind.label_key()))]));
            return;
        }
        if !self.persist {
            return;
        }
        let Some(path) = dialogs::pick_import(locale, kind) else { return };
        match imports::import(kind, &path, self.state.doc(), self.state.active_storey) {
            Ok(Imported::Commands(c, summary)) => {
                let label = tr(locale, kind.label_key());
                if self.state.run_all(c, label).is_ok() {
                    self.notify(import_notice(locale, &summary));
                    self.work.zoom_extents();
                }
            }
            Ok(Imported::Document(doc, summary)) => {
                let _ = ctx;
                self.set_document(*doc, None, None);
                // An imported model is not saved anywhere yet.
                self.state.recovered = true;
                self.notify(import_notice(locale, &summary));
            }
            Err(ImportError::NotAvailable) => self.notify(trf(
                locale,
                "ui.notice.not_available",
                &[("what", &tr(locale, kind.label_key()))],
            )),
            Err(e) => self.fail(trf(locale, "ui.notice.import_failed", &[("why", &e.to_string())])),
        }
    }

    /// Carries out one action.
    pub fn perform(&mut self, ctx: &egui::Context, action: Action) {
        let locale = self.state.locale;
        match action {
            Action::New => self.guard(ctx, Pending::New),
            Action::Open => self.guard(ctx, Pending::Open(None)),
            Action::OpenPath(p) => self.guard(ctx, Pending::Open(Some(p))),
            Action::OpenSample => self.guard(ctx, Pending::OpenSample),
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::Export(k) => self.export(ctx, k),
            Action::Import(k) => self.import(ctx, k),
            Action::Exit => self.guard(ctx, Pending::Exit),
            Action::Undo => self.state.undo(),
            Action::Redo => self.state.redo(),
            Action::UndoTo(n) => self.state.undo_to(n),
            Action::RedoSteps(n) => self.state.redo_steps(n),
            Action::Delete => {
                let ids: Vec<Id> = self.state.selection.iter().copied().collect();
                if !ids.is_empty() {
                    let _ = self.state.run(DeleteElements { ids }.into(), false);
                }
            }
            Action::SelectAll => {
                let storey = self.state.active_storey;
                self.state.selection = self.state.doc().elements_on(storey).map(|e| e.id).collect();
            }
            Action::SelectNone => self.state.selection.clear(),
            Action::SelectElement(id) => {
                if let Some(storey) = self.state.doc().element(id).and_then(|e| e.storey) {
                    self.activate_storey(storey);
                }
                self.state.selection = [id].into_iter().collect();
                self.inspector.page = Page::Properties;
                if !self.prefs.show_inspector {
                    self.prefs.show_inspector = true;
                }
            }
            Action::CopyToStorey(storey) => {
                let ids: Vec<Id> = self.state.selection.iter().copied().collect();
                if !ids.is_empty() && self.state.run(CopyToStorey { ids, storey }.into(), false).is_ok() {
                    let name = self.state.doc().storey(storey).map(|s| s.name.clone()).unwrap_or_default();
                    self.notify(trf(locale, "ui.notice.copied_to", &[("name", &name)]));
                }
            }
            Action::Tool(id) => {
                self.state.tool = id;
                if id != ToolId::Select && id != ToolId::Measure {
                    self.ensure_plan_focused();
                }
            }
            Action::Theme(t) => {
                self.prefs.theme = t;
                self.save_prefs();
            }
            Action::Language(l) => {
                self.state.locale = l;
                self.prefs.locale = l;
                self.save_prefs();
            }
            Action::Scale(s) => {
                self.prefs.ui_scale = s;
                self.save_prefs();
            }
            Action::TogglePanels => {
                let any = self.prefs.show_tree || self.prefs.show_inspector;
                self.prefs.show_tree = !any;
                self.prefs.show_inspector = !any;
                self.save_prefs();
            }
            Action::TogglePanel(p) => {
                match p {
                    Panel::Tree => self.prefs.show_tree = !self.prefs.show_tree,
                    Panel::Inspector => self.prefs.show_inspector = !self.prefs.show_inspector,
                    Panel::History => {
                        if self.prefs.show_inspector && self.inspector.page == Page::History {
                            self.inspector.page = Page::Properties;
                        } else {
                            self.prefs.show_inspector = true;
                            self.inspector.page = Page::History;
                        }
                    }
                }
                self.save_prefs();
            }
            Action::ToggleSplit => {
                let doc = self.state.doc().clone();
                self.work.set_split(&doc, !self.work.split);
                self.prefs.split = self.work.split;
                self.save_prefs();
            }
            Action::ZoomExtents => self.work.zoom_extents(),
            Action::OpenTab(target) => {
                let doc = self.state.doc().clone();
                if let TabTarget::View(id) = target
                    && let Some(ViewKind::Plan { storey }) = doc.view(id).map(|v| v.kind.clone())
                {
                    self.state.active_storey = storey;
                }
                if !self.work.open(&doc, target) {
                    self.fail(tr(locale, "ui.notice.view_missing"));
                }
            }
            Action::ActivateStorey(id) => self.activate_storey(id),
            Action::NewSection => {
                self.state.tool = ToolId::Section;
                self.ensure_plan_focused();
                self.notify(tr(locale, "ui.notice.draw_section"));
            }
            Action::NewElevation => {
                self.state.tool = ToolId::Elevation;
                self.ensure_plan_focused();
                self.notify(tr(locale, "ui.notice.draw_elevation"));
            }
            Action::NewSheet => {
                let n = self.state.doc().sheets().len() + 1;
                let sheet = CreateSheet {
                    number: format!("Α-{n:02}"),
                    name: tr(locale, "ui.tree.new_sheet_name"),
                    paper: Paper { size: PaperSize::A3, landscape: true },
                    viewports: Vec::new(),
                };
                if let Ok(out) = self.state.run(sheet.into(), false)
                    && let Some(id) = out.created.first().copied()
                {
                    let doc = self.state.doc().clone();
                    self.work.open(&doc, TabTarget::Sheet(id));
                }
            }
            Action::PermitSet => {
                if let Ok(out) = self.state.run(PermitSet { locale }.into(), false) {
                    let n = out.created.len();
                    self.notify(trf(locale, "ui.notice.permit_set", &[("n", &n.to_string())]));
                    if let Some(id) = out.created.first().copied() {
                        let doc = self.state.doc().clone();
                        self.work.open(&doc, TabTarget::Sheet(id));
                    }
                }
            }
            Action::DetectSpaces => {
                let storey = self.state.active_storey;
                let cmd = kanavos_engine::commands::DetectAllSpaces {
                    storey,
                    category: kanavos_model::SpaceCategory::MainUse,
                    name: None,
                    skip_existing: true,
                    numbered: true,
                    locale,
                };
                if let Ok(out) = self.state.run(cmd.into(), true) {
                    self.notify(trf(
                        locale,
                        "ui.notice.spaces_detected",
                        &[("n", &out.created.len().to_string())],
                    ));
                }
            }
            Action::OpenWindow(w) => self.managers.open(w),
            Action::CommandPalette => self.palette.show(""),
            Action::CheckForUpdates => self.updates.check(ctx, self.prefs.update_channel, true),
            Action::ShowUpdate => self.updates.open = true,
            Action::SetUpdateCheck(on) => {
                self.prefs.update_check = on;
                self.save_prefs();
            }
            Action::SetUpdateChannel(c) => {
                if self.prefs.update_channel != c {
                    self.prefs.update_channel = c;
                    self.save_prefs();
                    // An offer found on the other channel no longer applies; check again to see this one.
                    if !self.updates.busy() && !self.updates.ready() {
                        self.updates.available = None;
                        self.updates.stage = Stage::Idle;
                    }
                }
            }
        }
    }

    // ----- Frame parts -------------------------------------------------------------------------------------

    fn apply_scene(&mut self, ctx: &egui::Context) {
        let Some(hook) = &self.screenshot else { return };
        let scene = hook.scene.clone();
        for item in scene {
            let v = item.value.as_str();
            match item.key.as_str() {
                "lang" => self.state.locale = if v == "en" { Locale::En } else { Locale::El },
                "theme" => self.prefs.theme = if v == "dark" { Theme::Dark } else { Theme::Light },
                "scale" => {
                    if let Ok(s) = v.parse::<f32>() {
                        self.prefs.ui_scale = s;
                    }
                }
                "split" => {
                    let doc = self.state.doc().clone();
                    self.work.set_split(&doc, v != "off");
                }
                "panels" if v == "off" => {
                    self.prefs.show_tree = false;
                    self.prefs.show_inspector = false;
                }
                "history" => self.inspector.page = Page::History,
                "palette" => self.palette.show(v),
                "update" => {
                    // A sample offer, to check the update window; nothing is downloaded.
                    self.updates.available = Some(crate::update::sample_offer());
                    self.updates.stage = Stage::Offer;
                    self.updates.open = true;
                }
                "doc" if v == "new" => {
                    let locale = self.state.locale;
                    self.set_document(kanavos_model::starter(), None, None);
                    self.state.locale = locale;
                }
                "edit" => {
                    // A few named steps for the history page: rename the project, add a storey, undo one.
                    let mut info = self.state.doc().info().clone();
                    info.name = format!("{} ·", info.name);
                    let _ = self.state.run(kanavos_engine::commands::SetProjectInfo { info }.into(), false);
                    crate::tree::add_storey_above(&mut self.state);
                    crate::tree::add_storey_below(&mut self.state);
                    self.state.undo();
                }
                "tool" => {
                    if let Some(t) = ToolId::ALL.into_iter().find(|t| t.key() == v) {
                        self.perform(ctx, Action::Tool(t));
                    }
                }
                "open" => {
                    let w = match v {
                        "materials" => Some(Window::Materials),
                        "buildups" => Some(Window::BuildUps),
                        "storeys" => Some(Window::Storeys),
                        "project" => Some(Window::Project),
                        "site" => Some(Window::Site),
                        "defaults" => Some(Window::Defaults),
                        "about" => Some(Window::About),
                        "shortcuts" => Some(Window::Shortcuts),
                        _ => None,
                    };
                    if let Some(w) = w {
                        self.managers.open(w);
                    }
                }
                "tab" => {
                    let doc = self.state.doc().clone();
                    let target = match v {
                        "doors" => Some(TabTarget::Schedule(ScheduleKind::Doors)),
                        "windows" => Some(TabTarget::Schedule(ScheduleKind::Windows)),
                        "spaces" => Some(TabTarget::Schedule(ScheduleKind::Spaces)),
                        "walls" => Some(TabTarget::Schedule(ScheduleKind::Walls)),
                        "areas" => Some(TabTarget::AreaTable),
                        "3d" => doc
                            .views()
                            .find(|x| matches!(x.kind, ViewKind::ThreeD { .. }))
                            .map(|x| TabTarget::View(x.id)),
                        "section" => doc
                            .views()
                            .find(|x| matches!(x.kind, ViewKind::Section { .. }))
                            .map(|x| TabTarget::View(x.id)),
                        "sheet" => doc.sheets().first().map(|s| TabTarget::Sheet(s.id)),
                        _ => None,
                    };
                    if let Some(t) = target {
                        self.perform(ctx, Action::OpenTab(t));
                    }
                }
                "select" => {
                    let doc = self.state.doc().clone();
                    let storey = self.state.active_storey;
                    let pick = |want: &str, e: &kanavos_model::Element| match want {
                        "door" => e.category() == kanavos_model::Category::Door,
                        "window" => e.category() == kanavos_model::Category::Window,
                        other => e.kind.type_name() == other.trim_end_matches('s'),
                    };
                    let mut ids: Vec<Id> = doc
                        .elements()
                        .filter(|e| e.storey == Some(storey) && pick(v, e))
                        .map(|e| e.id)
                        .collect();
                    if ids.is_empty() {
                        ids = doc.elements().filter(|e| pick(v, e)).map(|e| e.id).collect();
                    }
                    if !v.ends_with('s') {
                        ids.truncate(1);
                    }
                    if let Some(first) = ids.first().copied() {
                        self.perform(ctx, Action::SelectElement(first));
                        self.state.selection = ids.into_iter().collect();
                    }
                }
                _ => {}
            }
        }
    }

    fn apply_style(&mut self, ctx: &egui::Context) {
        // Keyboard zoom (Ctrl+= / Ctrl+-) changes egui's zoom factor: keep the preference in step.
        if let Some((_, applied)) = self.applied_style {
            let z = ctx.zoom_factor();
            if (z - applied).abs() > 0.001 && (self.prefs.ui_scale - applied).abs() < 0.001 {
                self.prefs.ui_scale = z;
                self.applied_style = Some((self.prefs.theme, z));
            }
        }
        let wanted = (self.prefs.theme, self.prefs.ui_scale);
        if self.applied_style != Some(wanted) {
            theme::apply(ctx, wanted.0, wanted.1);
            self.applied_style = Some(wanted);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialogs.active() || self.palette.open {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let text_focus = ctx.text_edit_focused();
        let any_focus = ctx.egui_wants_keyboard_input();
        let typing = self.state.canvas_typing;
        let mut out = Vec::new();
        ctx.input_mut(|i| {
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::K)) {
                out.push(Action::CommandPalette);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::S)) {
                out.push(Action::SaveAs);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::S)) {
                out.push(Action::Save);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::O)) {
                out.push(Action::Open);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::N)) {
                out.push(Action::New);
            }
            if i.consume_key(Modifiers::NONE, Key::F1) {
                out.push(Action::OpenWindow(Window::Shortcuts));
            }
            if text_focus {
                return;
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::Z))
                || i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Y))
            {
                out.push(Action::Redo);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Z)) {
                out.push(Action::Undo);
            }
            if i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::A)) {
                out.push(Action::SelectAll);
            }
            if i.consume_key(Modifiers::NONE, Key::Delete) {
                out.push(Action::Delete);
            }
            if i.consume_key(Modifiers::NONE, Key::Home) {
                out.push(Action::ZoomExtents);
            }
            if typing || any_focus {
                return;
            }
            if i.consume_key(Modifiers::NONE, Key::Tab) {
                out.push(Action::TogglePanels);
            }
            for tool in ToolId::ALL {
                let Some(c) = tool.shortcut() else { continue };
                let Some(key) = Key::from_name(&c.to_string()) else { continue };
                if i.modifiers.command || i.modifiers.alt {
                    continue;
                }
                if i.consume_key(Modifiers::NONE, key) {
                    // The letter must not also reach the canvas as typed text.
                    i.events.retain(
                        |e| !matches!(e, egui::Event::Text(s) if s.eq_ignore_ascii_case(&c.to_string())),
                    );
                    out.push(Action::Tool(tool));
                }
            }
        });
        self.actions.extend(out);
    }

    fn close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && self.persist
            && self.state.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.dialogs.unsaved.is_none() {
                self.dialogs.unsaved = Some(Pending::Exit);
            }
        }
    }

    fn autosave(&mut self) {
        if let Some(rx) = &self.autosave.running {
            match rx.try_recv() {
                Ok(Ok(())) => self.autosave.running = None,
                Ok(Err(e)) => {
                    self.autosave.running = None;
                    self.fail(trf(self.state.locale, "ui.notice.autosave_failed", &[("why", &e)]));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.autosave.running = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let interval = self.prefs.autosave_secs;
        if !self.persist || interval == 0 || self.autosave.running.is_some() {
            return;
        }
        let rev = self.state.session.revision();
        if self.autosave.last.elapsed() >= Duration::from_secs(interval)
            && rev != self.autosave.revision
            && self.state.is_dirty()
        {
            self.autosave.running =
                Some(persist::write_recovery(self.state.doc().clone(), self.state.file.clone()));
            self.autosave.revision = rev;
            self.autosave.last = Instant::now();
        }
    }

    fn notice_visible(&mut self) -> bool {
        let Some(n) = &self.state.notice else {
            self.notice_seen = None;
            return false;
        };
        let fresh = !matches!(&self.notice_seen, Some((seen, _)) if seen == n);
        if fresh {
            self.notice_seen = Some((n.clone(), Instant::now()));
        }
        let limit = if n.error { 12.0 } else { 6.0 };
        self.notice_seen.as_ref().is_some_and(|(_, at)| at.elapsed().as_secs_f32() < limit)
    }

    fn sync_after_change(&mut self, ctx: &egui::Context) {
        let rev = self.state.session.revision();
        if rev != self.seen_revision {
            self.seen_revision = rev;
            let doc = self.state.doc().clone();
            self.work.prune(&doc);
        }
        // Active storey and scale follow the focused tab.
        let focused = self.work.focused().map(|t| t.target);
        self.state.view_scale = match focused {
            Some(TabTarget::View(id)) => self.state.doc().view(id).and_then(|v| match v.kind {
                ViewKind::ThreeD { .. } | ViewKind::AreaTable | ViewKind::Schedule { .. } => None,
                _ => Some(v.scale),
            }),
            _ => None,
        };
        if let Some(TabTarget::View(id)) = focused
            && let Some(ViewKind::Plan { storey }) = self.state.doc().view(id).map(|v| v.kind.clone())
        {
            self.state.active_storey = storey;
        }
        // Keep the ΝΟΚ report current in the background.
        let doc = self.state.doc().clone();
        let wake = ctx.clone();
        self.nok.request(rev, move || kanavos_rules_gr::nok_report(&doc), move || wake.request_repaint());
    }
}

/// The slim rail shown when a side panel is collapsed.
fn rail(ui: &mut egui::Ui, locale: Locale, icon_set: &[(Icon, &str)], expand: &mut bool) {
    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        if icons::button(ui, Icon::ChevronRight, false, 24.0, &tr(locale, "ui.panel.expand")).clicked() {
            *expand = true;
        }
        ui.add_space(6.0);
        let t = Tokens::of(ui);
        for (icon, key) in icon_set {
            let r = icons::button(ui, *icon, false, 24.0, &tr(locale, key));
            if r.clicked() {
                *expand = true;
            }
            let _ = t;
        }
    });
}

impl eframe::App for KanavosApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.first_frame {
            self.first_frame = false;
            self.apply_scene(&ctx);
            if self.persist {
                self.updates.startup(&ctx, self.prefs.update_check, self.prefs.update_channel);
            }
        }
        self.apply_style(&ctx);
        self.nok.poll();
        self.poll_exports();
        match self.updates.poll(self.prefs.skipped_version.as_deref()) {
            // Another question is open: the update waits in its window until the person installs it.
            Some(Done::Downloaded) if self.dialogs.active() => self.updates.open = true,
            Some(Done::Downloaded) => self.guard(&ctx, Pending::Update),
            Some(Done::Installed(installed)) => self.pending_restart = Some(installed),
            None => {}
        }
        self.close_request(&ctx);
        self.shortcuts(&ctx);
        self.sync_after_change(&ctx);
        let locale = self.state.locale;
        let t = *Tokens::of_theme(self.prefs.theme);
        let mut actions = std::mem::take(&mut self.actions);

        // Menu bar.
        egui::Panel::top("menu")
            .frame(Frame::new().fill(t.panel).inner_margin(egui::Margin::symmetric(8, 3)))
            .show(ui, |ui| {
                let table_focused = matches!(self.work.focused().map(|t| &t.body), Some(TabBody::Table(_)));
                let mc = MenuContext {
                    recent: &self.prefs.recent,
                    theme: self.prefs.theme,
                    ui_scale: self.prefs.ui_scale,
                    tree: self.prefs.show_tree,
                    inspector: self.prefs.show_inspector,
                    split: self.work.split,
                    table_focused,
                    update_check: self.prefs.update_check,
                    update_channel: self.prefs.update_channel,
                    update_available: self.updates.available.as_ref().map(|a| a.version.to_string()),
                };
                menu::bar(ui, &self.state, &mc, &mut actions);
            });
        // Context bar.
        egui::Panel::top("context")
            .exact_size(40.0)
            .frame(
                Frame::new()
                    .fill(t.raised)
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .stroke(Stroke::new(1.0, t.border)),
            )
            .show(ui, |ui| self.ctxbar.ui(ui, &mut self.state, &mut actions));
        // Status bar.
        let show_notice = self.notice_visible();
        let busy = self.nok.busy() || self.autosave.running.is_some() || !self.exporting.is_empty();
        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(
                Frame::new()
                    .fill(t.panel)
                    .inner_margin(egui::Margin::symmetric(10, 2))
                    .stroke(Stroke::new(1.0, t.border)),
            )
            .show(ui, |ui| {
                let info = StatusInfo { busy, show_notice, update: self.updates.status_text(locale) };
                status::bar(ui, &mut self.state, &info, &mut actions);
            });
        // Project tree, collapsible to a rail.
        let focused_target = self.work.focused().map(|t| t.target);
        let side = Frame::new()
            .fill(t.panel)
            .inner_margin(egui::Margin::symmetric(6, 4))
            .stroke(Stroke::new(1.0, t.border));
        let mut show_tree = self.prefs.show_tree;
        egui::Panel::show_switched(
            ui,
            &mut show_tree,
            egui::Panel::left("tree-rail").exact_size(38.0).resizable(false).frame(side),
            egui::Panel::left("tree")
                .default_size(260.0)
                .size_range(200.0..=440.0)
                .resizable(true)
                .frame(side),
            |ui, expanded| {
                if expanded {
                    self.tree.ui(ui, &mut self.state, &mut actions, focused_target);
                } else {
                    let mut expand = false;
                    rail(
                        ui,
                        locale,
                        &[
                            (Icon::Site, "ui.tree.site"),
                            (Icon::Storey, "ui.tree.storeys"),
                            (Icon::Views, "ui.tree.views"),
                            (Icon::Sheet, "ui.tree.sheets"),
                            (Icon::Table, "ui.tree.schedules"),
                        ],
                        &mut expand,
                    );
                    if expand {
                        actions.push(Action::TogglePanel(Panel::Tree));
                    }
                }
            },
        );
        if show_tree != self.prefs.show_tree {
            self.prefs.show_tree = show_tree;
        }
        // Tool palette.
        egui::Panel::left("tools")
            .exact_size(crate::palette::WIDTH)
            .resizable(false)
            .frame(Frame::new().fill(t.panel).stroke(Stroke::new(1.0, t.border)))
            .show(ui, |ui| crate::palette::ui(ui, self.state.tool, locale, &mut actions));
        // Inspector, collapsible to a rail.
        let mut show_inspector = self.prefs.show_inspector;
        let nok_view = NokView { report: self.nok.latest().map(|(_, r)| r), updating: self.nok.busy() };
        egui::Panel::show_switched(
            ui,
            &mut show_inspector,
            egui::Panel::right("inspector-rail").exact_size(38.0).resizable(false).frame(side),
            egui::Panel::right("inspector")
                .default_size(340.0)
                .size_range(280.0..=520.0)
                .resizable(true)
                .frame(side),
            |ui, expanded| {
                if expanded {
                    self.inspector.ui(ui, &mut self.state, nok_view, &mut actions);
                } else {
                    let mut expand = false;
                    rail(
                        ui,
                        locale,
                        &[
                            (Icon::Properties, "ui.insp.page.properties"),
                            (Icon::History, "ui.insp.page.history"),
                        ],
                        &mut expand,
                    );
                    if expand {
                        actions.push(Action::TogglePanel(Panel::Inspector));
                    }
                }
            },
        );
        if show_inspector != self.prefs.show_inspector {
            self.prefs.show_inspector = show_inspector;
        }
        // Views.
        egui::CentralPanel::default().frame(Frame::new().fill(t.app)).show(ui, |ui| {
            Frame::new()
                .fill(t.panel)
                .inner_margin(egui::Margin { left: 6, right: 6, top: 3, bottom: 0 })
                .show(ui, |ui| {
                    self.work.bar(ui, &self.state, &mut actions);
                });
            let area = ui.available_rect_before_wrap().shrink(4.0);
            let panes: Vec<(Pane, egui::Rect)> = if self.work.split && self.work.shown(Pane::Right).is_some()
            {
                let mid = area.center().x;
                vec![
                    (Pane::Left, egui::Rect::from_min_max(area.min, egui::pos2(mid - 3.0, area.max.y))),
                    (Pane::Right, egui::Rect::from_min_max(egui::pos2(mid + 3.0, area.min.y), area.max)),
                ]
            } else {
                vec![(Pane::Left, area)]
            };
            let split = panes.len() > 1;
            let focused_pane = self.work.focused_pane();
            for (pane, rect) in panes {
                let focused = pane == focused_pane;
                let border = if focused && split { t.accent } else { t.border };
                ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, border), egui::StrokeKind::Outside);
                let pressed_here = ui.input(|i| {
                    i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
                });
                if pressed_here && !focused {
                    self.work.focus = pane;
                }
                let mut child =
                    ui.new_child(UiBuilder::new().max_rect(rect).id_salt(("pane", pane == Pane::Left)));
                child.set_clip_rect(rect);
                match self.work.shown(pane) {
                    Some(i) => {
                        if let Some(tab) = self.work.tabs.get_mut(i) {
                            tabs::pane_ui(
                                &mut child,
                                tab,
                                focused,
                                split,
                                &mut self.state,
                                frame,
                                &mut actions,
                            );
                            empty_storey_hint(&mut child, rect, &self.state, tab.target);
                        }
                    }
                    None => {
                        child.painter().rect_filled(rect, 0.0, t.raised);
                        child.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(rect.height() / 2.0 - 30.0);
                                ui.label(RichText::new(tr(locale, "ui.empty.no_tab")).color(t.text_muted));
                            });
                        });
                    }
                }
            }
        });

        // Floating windows and dialogs.
        let update_settings =
            UpdateSettings { check_at_start: self.prefs.update_check, channel: self.prefs.update_channel };
        self.managers.ui(&ctx, &mut self.state, update_settings, &mut actions);
        if let Some(event) = self.updates.window(&ctx, locale) {
            self.update_event(&ctx, event);
        }
        let rev = self.state.session.revision();
        if let Some(a) = self.palette.ui(&ctx, self.state.doc(), rev, locale) {
            actions.push(a);
        }
        if let Some((answer, pending)) = self.dialogs.unsaved_ui(&ctx, locale, &self.state.title()) {
            match answer {
                Answer::Save => {
                    if self.save(false) {
                        self.continue_pending(&ctx, pending);
                    }
                }
                Answer::Discard => {
                    if pending == Pending::Exit {
                        persist::clear_recovery();
                    }
                    self.continue_pending(&ctx, pending);
                }
                Answer::Cancel => {}
            }
        }
        if let Some((recover, rec)) = self.dialogs.recovery_ui(&ctx, locale) {
            if recover {
                // The recovery copy stays on disk until the project is saved.
                self.manifest = None;
                self.state.replace_document(rec.document, rec.original);
                self.state.recovered = true;
                self.work =
                    Workspace::with_defaults(self.state.doc(), self.state.active_storey, self.prefs.split);
                self.nok.invalidate();
                self.seen_revision = self.state.session.revision();
                self.notify(tr(locale, "ui.notice.recovered"));
            } else {
                persist::clear_recovery();
            }
        }

        for a in actions {
            self.perform(&ctx, a);
        }
        self.autosave();
        self.restart_when_idle(&ctx);
        if let Some(hook) = &mut self.screenshot {
            hook.frame(&ctx);
        }
    }

    fn on_exit(&mut self) {
        if self.persist {
            self.prefs.split = self.work.split;
            self.prefs.save();
            if !self.state.is_dirty() {
                persist::clear_recovery();
            }
        }
    }
}

/// The notice after an import.
fn import_notice(locale: Locale, s: &ImportSummary) -> String {
    let mut text = trf(locale, "ui.notice.imported", &[("n", &s.items.to_string())]);
    if s.skipped > 0 {
        text.push_str(" · ");
        text.push_str(&trf(locale, "ui.notice.import_skipped", &[("n", &s.skipped.to_string())]));
    }
    if s.plot {
        text.push_str(" · ");
        text.push_str(&tr(locale, "ui.notice.import_plot"));
    }
    if s.terrain_points > 0 {
        text.push_str(" · ");
        text.push_str(&trf(locale, "ui.notice.import_terrain", &[("n", &s.terrain_points.to_string())]));
    }
    text
}

/// The empty-storey hint over a plan with no elements (docs/ux.md §5).
fn empty_storey_hint(ui: &mut egui::Ui, rect: egui::Rect, state: &AppState, target: TabTarget) {
    let TabTarget::View(id) = target else { return };
    let Some(ViewKind::Plan { storey }) = state.doc().view(id).map(|v| v.kind.clone()) else { return };
    if state.doc().elements_on(storey).next().is_some() {
        return;
    }
    let t = Tokens::of(ui);
    let text = tr(state.locale, "ui.empty.storey");
    let wrap = (rect.width() - 56.0).clamp(120.0, 420.0);
    let galley =
        ui.painter().layout(text, egui::FontId::proportional(crate::theme::size::BODY), t.text, wrap);
    let size = galley.size() + egui::vec2(28.0, 20.0);
    let card =
        egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 60.0 - size.y / 2.0), size);
    ui.painter().rect_filled(card, 8.0, t.raised);
    ui.painter().rect_stroke(card, 8.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    ui.painter().galley(card.min + egui::vec2(14.0, 10.0), galley, t.text);
}
