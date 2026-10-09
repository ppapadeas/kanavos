//! Application state shared by the shell, the canvases and the panels.
//!
//! The document lives in the [`Session`]; everything else here is interface state (selection, active storey,
//! tool settings, language). Changes to the document go through [`AppState::run`] (ADR-0005).

use std::collections::BTreeSet;
use std::path::PathBuf;

use kanavos_engine::commands::Batch;
use kanavos_engine::{AnyCommand, CommandError, Outcome, Session};
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
}

impl AppState {
    /// State over a document; the active storey is the lowest above-ground storey.
    pub fn new(doc: Document) -> AppState {
        let active_storey = doc
            .storeys()
            .into_iter()
            .find(|s| !s.below_ground)
            .or_else(|| doc.storeys().into_iter().next())
            .map(|s| s.id)
            .unwrap_or_default();
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
        }
    }

    /// The current document.
    pub fn doc(&self) -> &Document {
        self.session.document()
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
                    self.selection = out.created.iter().copied().collect();
                }
                self.notice = None;
                Ok(out)
            }
            Err(e) => {
                self.notice = Some(Notice { text: e.message(self.locale), error: true });
                Err(e)
            }
        }
    }

    /// Runs several commands as one undo step.
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
        self.session.undo();
        self.prune_selection();
    }

    /// Redo, keeping the selection valid.
    pub fn redo(&mut self) {
        self.session.redo();
        self.prune_selection();
    }

    /// Replaces the document (new/open).
    pub fn replace_document(&mut self, doc: Document, file: Option<PathBuf>) {
        let locale = self.locale;
        *self = AppState::new(doc);
        self.locale = locale;
        self.file = file;
    }

    /// Drops selected ids that no longer exist and fixes the active storey.
    pub fn prune_selection(&mut self) {
        let doc = self.session.document().clone();
        self.selection.retain(|id| doc.element(*id).is_some());
        if doc.storey(self.active_storey).is_none()
            && let Some(s) = doc.storeys().first()
        {
            self.active_storey = s.id;
        }
    }
}
