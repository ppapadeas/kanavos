//! Application state shared by the shell, the canvases and the panels.
//!
//! The document lives in the [`Session`]; everything else here is interface state (selection, active storey,
//! tool settings, language, pointer position). Changes to the document go through [`AppState::run`] or
//! [`AppState::run_all`] (ADR-0005); nothing in the interface mutates a document directly.
//!
//! Fields the canvas writes every frame: [`AppState::cursor`], [`AppState::prompt`],
//! [`AppState::canvas_typing`], [`AppState::hover`] and, through tools, [`AppState::selection`].

use std::collections::BTreeSet;
use std::path::PathBuf;

use kanavos_engine::commands::Batch;
use kanavos_engine::{AnyCommand, Command, CommandError, Outcome, Session};
use kanavos_geom::Point2;
use kanavos_model::{Document, Id};
use kanavos_tools::{SnapSettings, ToolId, ToolSettings};
use kanavos_units::Locale;

/// A message for the status bar or a notification.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// Text.
    pub text: String,
    /// Whether it reports an error.
    pub error: bool,
}

impl Notice {
    /// An informational notice.
    pub fn info(text: impl Into<String>) -> Notice {
        Notice { text: text.into(), error: false }
    }
    /// An error notice.
    pub fn error(text: impl Into<String>) -> Notice {
        Notice { text: text.into(), error: true }
    }
}

/// Shared application state.
pub struct AppState {
    /// The editing session (document and history).
    pub session: Session,
    /// File the document was loaded from or saved to.
    pub file: Option<PathBuf>,
    /// Selected elements.
    pub selection: BTreeSet<Id>,
    /// Element under the pointer, for hover highlighting in every view.
    pub hover: Option<Id>,
    /// Storey shown in plan canvases and used by tools.
    pub active_storey: Id,
    /// Active tool.
    pub tool: ToolId,
    /// Tool parameters (context bar).
    pub tool_settings: ToolSettings,
    /// Snap settings.
    pub snap: SnapSettings,
    /// Interface language.
    pub locale: Locale,
    /// Latest notice.
    pub notice: Option<Notice>,
    /// Pointer position in project coordinates (metres) while it is over a plan, section or elevation canvas;
    /// `None` elsewhere. Set by the canvas, shown in the status bar (with ΕΓΣΑ87 coordinates).
    pub cursor: Option<Point2>,
    /// Hint of the active tool for the status bar ("Click the start of the wall"). Set by the canvas from
    /// the tool output; empty when there is nothing to say.
    pub prompt: String,
    /// Set by the canvas while the user is typing into a tool's input field (length, angle …). While it is
    /// set, the shell leaves single-letter tool shortcuts and `Tab` to the canvas.
    pub canvas_typing: bool,
    /// Scale denominator of the view in the focused tab (100 means 1:100), for the status bar.
    pub view_scale: Option<u32>,
    /// The document came from a recovery copy and has not been saved since.
    pub recovered: bool,
    /// Commands undone and available for redo, most recent last. Kept here so that the history panel and the
    /// Edit menu can name redo steps; cleared when a new command runs.
    redo_stack: Vec<AnyCommand>,
}

impl AppState {
    /// State over a document; the active storey is the lowest above-ground storey.
    pub fn new(doc: Document) -> AppState {
        let active_storey = default_storey(&doc);
        AppState {
            session: Session::new(doc),
            file: None,
            selection: BTreeSet::new(),
            hover: None,
            active_storey,
            tool: ToolId::Select,
            tool_settings: ToolSettings::default(),
            snap: SnapSettings::default(),
            locale: Locale::El,
            notice: None,
            cursor: None,
            prompt: String::new(),
            canvas_typing: false,
            view_scale: None,
            recovered: false,
            redo_stack: Vec::new(),
        }
    }

    /// The current document.
    pub fn doc(&self) -> &Document {
        self.session.document()
    }

    /// Whether there are changes that are not in the project file (including a recovered document).
    pub fn is_dirty(&self) -> bool {
        self.session.is_dirty() || self.recovered
    }

    /// Runs a command; on failure the error becomes the notice. Created elements become the selection when
    /// `select_created` is set.
    pub fn run(&mut self, command: AnyCommand, select_created: bool) -> Result<Outcome, CommandError> {
        match self.session.run(command) {
            Ok(out) => {
                for id in &out.deleted {
                    self.selection.remove(id);
                }
                if select_created && !out.created.is_empty() {
                    self.selection = out
                        .created
                        .iter()
                        .copied()
                        .filter(|id| self.session.document().element(*id).is_some())
                        .collect();
                }
                self.redo_stack.clear();
                self.notice = None;
                self.prune_selection();
                Ok(out)
            }
            Err(e) => {
                self.notice = Some(Notice { text: e.message(self.locale), error: true });
                Err(e)
            }
        }
    }

    /// Runs several commands as one undo step labelled `label` in the history.
    pub fn run_all(&mut self, commands: Vec<AnyCommand>, label: String) -> Result<Outcome, CommandError> {
        match commands.len() {
            0 => Ok(Outcome::default()),
            1 => {
                let mut it = commands.into_iter();
                match it.next() {
                    Some(c) => self.run(c, false),
                    None => Ok(Outcome::default()),
                }
            }
            _ => self.run(Batch { label, commands }.into(), false),
        }
    }

    /// Undo, keeping the selection valid.
    pub fn undo(&mut self) {
        let last = self.session.history().last().cloned();
        if self.session.undo().is_some()
            && let Some(c) = last
        {
            self.redo_stack.push(c);
        }
        self.prune_selection();
    }

    /// Redo, keeping the selection valid.
    pub fn redo(&mut self) {
        if self.session.redo().is_some() {
            self.redo_stack.pop();
        }
        self.prune_selection();
    }

    /// Undoes until `steps` steps remain (history panel).
    pub fn undo_to(&mut self, steps: usize) {
        while self.session.version() > steps && self.session.can_undo() {
            self.undo();
        }
    }

    /// Redoes `count` steps (history panel).
    pub fn redo_steps(&mut self, count: usize) {
        for _ in 0..count {
            if !self.session.can_redo() {
                break;
            }
            self.redo();
        }
    }

    /// Label of the step that undo would revert.
    pub fn undo_label(&self) -> Option<String> {
        self.session.history().last().map(|c| c.label(self.locale))
    }

    /// Label of the step that redo would apply again.
    pub fn redo_label(&self) -> Option<String> {
        self.redo_stack.last().map(|c| c.label(self.locale))
    }

    /// Labels of the redo steps, next first.
    pub fn redo_labels(&self) -> Vec<String> {
        self.redo_stack.iter().rev().map(|c| c.label(self.locale)).collect()
    }

    /// Replaces the document (new/open), keeping interface preferences (language, snap, tool settings).
    pub fn replace_document(&mut self, doc: Document, file: Option<PathBuf>) {
        let locale = self.locale;
        let snap = self.snap.clone();
        let settings = self.tool_settings.clone();
        *self = AppState::new(doc);
        self.locale = locale;
        self.snap = snap;
        self.tool_settings = settings;
        self.file = file;
    }

    /// Drops selected ids that no longer exist and fixes the active storey.
    pub fn prune_selection(&mut self) {
        let doc = self.session.document();
        let selection: BTreeSet<Id> =
            self.selection.iter().copied().filter(|id| doc.element(*id).is_some()).collect();
        let storey_ok = doc.storey(self.active_storey).is_some();
        let fallback = default_storey(doc);
        self.selection = selection;
        if self.hover.is_some_and(|h| self.session.document().element(h).is_none()) {
            self.hover = None;
        }
        if !storey_ok {
            self.active_storey = fallback;
        }
    }

    /// Display name of the open project: file stem, else the project title.
    pub fn title(&self) -> String {
        self.file
            .as_ref()
            .and_then(|f| f.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.doc().info().name.clone())
    }
}

/// The lowest above-ground storey, else the lowest storey, else id 0.
fn default_storey(doc: &Document) -> Id {
    doc.storeys()
        .into_iter()
        .find(|s| !s.below_ground)
        .or_else(|| doc.storeys().into_iter().next())
        .map(|s| s.id)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_engine::commands::{CreateStorey, DeleteElements, SetProjectInfo};
    use kanavos_units::Length;

    fn state() -> AppState {
        AppState::new(kanavos_model::starter())
    }

    fn info(name: &str) -> AnyCommand {
        let info = kanavos_model::ProjectInfo { name: name.into(), ..Default::default() };
        SetProjectInfo { info }.into()
    }

    #[test]
    fn active_storey_is_the_lowest_above_ground() {
        let s = state();
        let ground = s.doc().storeys().first().map(|s| s.id);
        assert_eq!(Some(s.active_storey), ground);
    }

    #[test]
    fn undo_and_redo_name_their_steps() {
        let mut s = state();
        s.run(info("A"), false).unwrap();
        s.run(info("B"), false).unwrap();
        assert!(s.undo_label().is_some());
        assert_eq!(s.redo_label(), None);
        s.undo();
        assert_eq!(s.doc().info().name, "A");
        assert_eq!(s.redo_labels().len(), 1);
        s.redo();
        assert_eq!(s.doc().info().name, "B");
        assert!(s.redo_labels().is_empty());
    }

    #[test]
    fn a_new_command_clears_the_redo_steps() {
        let mut s = state();
        s.run(info("A"), false).unwrap();
        s.undo();
        assert_eq!(s.redo_labels().len(), 1);
        s.run(info("C"), false).unwrap();
        assert!(s.redo_labels().is_empty());
    }

    #[test]
    fn undo_to_and_redo_steps_jump_through_history() {
        let mut s = state();
        for n in ["1", "2", "3"] {
            s.run(info(n), false).unwrap();
        }
        s.undo_to(1);
        assert_eq!(s.doc().info().name, "1");
        assert_eq!(s.redo_labels().len(), 2);
        s.redo_steps(2);
        assert_eq!(s.doc().info().name, "3");
        s.undo_to(0);
        assert_eq!(s.session.version(), 0);
    }

    #[test]
    fn failures_become_error_notices_and_keep_history() {
        let mut s = state();
        let r = s.run(DeleteElements { ids: vec![Id(987_654)] }.into(), false);
        assert!(r.is_err());
        assert!(s.notice.as_ref().is_some_and(|n| n.error));
        assert_eq!(s.session.version(), 0);
    }

    #[test]
    fn created_storeys_do_not_become_the_element_selection() {
        let mut s = state();
        let out = s.run(CreateStorey::new("Δώμα", Length::m(6.4), Length::m(3.0)).into(), true).unwrap();
        assert!(!out.created.is_empty());
        // Storeys and views are not elements, so the selection stays empty.
        assert!(s.selection.is_empty());
    }

    #[test]
    fn replacing_the_document_keeps_language() {
        let mut s = state();
        s.locale = Locale::En;
        s.run(info("X"), false).unwrap();
        s.replace_document(kanavos_model::starter(), None);
        assert_eq!(s.locale, Locale::En);
        assert_eq!(s.session.version(), 0);
        assert!(!s.session.is_dirty());
    }
}
