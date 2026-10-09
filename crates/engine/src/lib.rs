//! Session, command bus, undo/redo and background job scheduling.
//!
//! Layer L7. See `docs/architecture.md` and ADR-0005.
//!
//! Every change to a document is a [`Command`]. A [`Session`] applies commands and keeps every document version,
//! which makes undo and redo trivial and exact. The history keeps the commands themselves, so a session can be
//! replayed ([`Session::replay`]) or exported as a command log ([`Session::log_jsonl`]) and attached to a bug
//! report.
//!
//! Other modules:
//! - [`derived`]: plans, sections, sheets, the 3D scene, the ΝΟΚ report and schedules computed off the interface
//!   thread, keyed by document revision.
//! - [`sample`]: a complete two-storey sample house built only through commands.
//! - [`io`] and [`export`]: the single place where file formats and exporters are connected.
//! - [`geo`]: plan geometry derived from walls (rooms, outlines, dimension chains) that commands need.

pub mod commands;
pub mod derived;
pub mod export;
pub mod geo;
pub mod io;
mod labels;
pub mod log;
pub mod sample;

use std::fmt;

use kanavos_model::{Document, Id};
use kanavos_units::Locale;

pub use commands::{AnyCommand, command_schema};

/// Error returned when a command cannot be applied. The message is shown to the user.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandError {
    /// An argument is invalid; `field` names it.
    InvalidArgument {
        /// Argument name.
        field: &'static str,
        /// Human-readable reason.
        reason: String,
    },
    /// A referenced object does not exist.
    NotFound(Id),
    /// The object is still used and cannot be deleted.
    InUse(Id),
    /// Geometry could not be computed.
    Geometry(String),
}

impl CommandError {
    /// The message in a language.
    pub fn message(&self, locale: Locale) -> String {
        use kanavos_i18n::tf;
        match self {
            CommandError::InvalidArgument { field, reason } => {
                tf(locale, "engine.error.invalid", &[("field", field), ("reason", reason)])
            }
            CommandError::NotFound(id) => tf(locale, "engine.error.not_found", &[("id", &id.0.to_string())]),
            CommandError::InUse(id) => tf(locale, "engine.error.in_use", &[("id", &id.0.to_string())]),
            CommandError::Geometry(why) => tf(locale, "engine.error.geometry", &[("reason", why)]),
        }
    }

    /// The argument at fault, for errors about one argument.
    pub fn field(&self) -> Option<&'static str> {
        match self {
            CommandError::InvalidArgument { field, .. } => Some(field),
            _ => None,
        }
    }

    /// A stable machine-readable kind: `invalid_argument`, `not_found`, `in_use` or `geometry`.
    pub fn kind(&self) -> &'static str {
        match self {
            CommandError::InvalidArgument { .. } => "invalid_argument",
            CommandError::NotFound(_) => "not_found",
            CommandError::InUse(_) => "in_use",
            CommandError::Geometry(_) => "geometry",
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Locale::En))
    }
}

impl std::error::Error for CommandError {}

/// Outcome of a successful command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// Objects created by the command.
    pub created: Vec<Id>,
    /// Objects changed by the command.
    pub changed: Vec<Id>,
    /// Objects removed by the command.
    pub deleted: Vec<Id>,
}

/// A change to a document. Commands never panic; they return a [`CommandError`] instead.
pub trait Command {
    /// Stable public name, e.g. `"element.create"`.
    fn name(&self) -> &'static str;
    /// Applies the command, returning the new document and what changed.
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError>;
    /// Label for the history panel, e.g. "Διαγραφή 3 στοιχείων". Counts come from the command's arguments.
    fn label(&self, locale: Locale) -> String {
        kanavos_i18n::t(locale, &format!("engine.cmd.{}", self.name())).into_owned()
    }
    /// Label that may also look at the document the command applies to, e.g. "Μετακίνηση 2 τοίχων" when the
    /// moved elements are walls. Defaults to [`Command::label`].
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        let _ = doc;
        self.label(locale)
    }
}

struct Step {
    before: Document,
    command: AnyCommand,
}

/// One history entry, for the history panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Command name.
    pub name: &'static str,
    /// Label in the requested language.
    pub label: String,
}

/// Error from [`Session::replay`]: which command failed and why.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayError {
    /// Zero-based index of the failing command.
    pub index: usize,
    /// Its error.
    pub error: CommandError,
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "command {}: {}", self.index + 1, self.error)
    }
}

impl std::error::Error for ReplayError {}

/// An editing session over one document, with full history.
pub struct Session {
    past: Vec<Step>,
    current: Document,
    future: Vec<(Document, AnyCommand)>,
    revision: u64,
    saved_revision: u64,
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Document::default())
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("steps", &self.past.len())
            .field("redo", &self.future.len())
            .field("revision", &self.revision)
            .finish()
    }
}

impl Session {
    /// Starts a session on a document.
    pub fn new(doc: Document) -> Self {
        Session { past: Vec::new(), current: doc, future: Vec::new(), revision: 0, saved_revision: 0 }
    }

    /// Starts a session on `doc` and runs `commands` in order, each as one undo step. Stops at the first failing
    /// command and reports its index.
    pub fn replay(
        doc: Document,
        commands: impl IntoIterator<Item = AnyCommand>,
    ) -> Result<Session, ReplayError> {
        let mut s = Session::new(doc);
        for (index, c) in commands.into_iter().enumerate() {
            s.run(c).map_err(|error| ReplayError { index, error })?;
        }
        Ok(s)
    }

    /// The current document.
    pub fn document(&self) -> &Document {
        &self.current
    }

    /// The document the session started from (before the oldest step still in the history).
    pub fn initial_document(&self) -> &Document {
        self.past.first().map(|s| &s.before).unwrap_or(&self.current)
    }

    /// Number of steps between the opened document and the current one.
    pub fn version(&self) -> usize {
        self.past.len()
    }

    /// A counter that changes on every run, undo and redo; use it to invalidate derived data.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Runs a command. On success it becomes one undo step and any redo steps are discarded.
    pub fn run(&mut self, command: impl Into<AnyCommand>) -> Result<Outcome, CommandError> {
        let command = command.into();
        let (next, outcome) = command.apply(&self.current)?;
        let before = std::mem::replace(&mut self.current, next);
        self.past.push(Step { before, command });
        self.future.clear();
        self.revision += 1;
        Ok(outcome)
    }

    /// Undoes the last step. Returns its command name, or `None` if there is nothing to undo.
    pub fn undo(&mut self) -> Option<&'static str> {
        let step = self.past.pop()?;
        let undone = std::mem::replace(&mut self.current, step.before);
        let name = step.command.name();
        self.future.push((undone, step.command));
        self.revision += 1;
        Some(name)
    }

    /// Redoes the next step. Returns its command name, or `None` if there is nothing to redo.
    pub fn redo(&mut self) -> Option<&'static str> {
        let (next, command) = self.future.pop()?;
        let before = std::mem::replace(&mut self.current, next);
        let name = command.name();
        self.past.push(Step { before, command });
        self.revision += 1;
        Some(name)
    }

    /// Whether there is a step to undo.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether there is a step to redo.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Names of the applied steps, oldest first.
    pub fn steps(&self) -> Vec<&'static str> {
        self.past.iter().map(|s| s.command.name()).collect()
    }

    /// The applied commands, oldest first.
    pub fn history(&self) -> impl Iterator<Item = &AnyCommand> {
        self.past.iter().map(|s| &s.command)
    }

    /// History labels in a language, oldest first, e.g. "Μετακίνηση 2 τοίχων".
    pub fn history_labels(&self, locale: Locale) -> Vec<String> {
        self.past.iter().map(|s| s.command.describe(&s.before, locale)).collect()
    }

    /// History entries (name and label), oldest first.
    pub fn history_entries(&self, locale: Locale) -> Vec<HistoryEntry> {
        self.past
            .iter()
            .map(|s| HistoryEntry { name: s.command.name(), label: s.command.describe(&s.before, locale) })
            .collect()
    }

    /// Labels of the steps that can be redone, next one first.
    pub fn redo_labels(&self, locale: Locale) -> Vec<String> {
        let mut out = Vec::with_capacity(self.future.len());
        let mut before = &self.current;
        for (after, command) in self.future.iter().rev() {
            out.push(command.describe(before, locale));
            before = after;
        }
        out
    }

    /// Undoes until `steps` steps remain.
    pub fn undo_to(&mut self, steps: usize) {
        while self.past.len() > steps {
            if self.undo().is_none() {
                break;
            }
        }
    }

    /// Redoes until `steps` steps are applied or nothing is left to redo.
    pub fn redo_to(&mut self, steps: usize) {
        while self.past.len() < steps {
            if self.redo().is_none() {
                break;
            }
        }
    }

    /// The applied commands as a JSON-lines command log (one `{"name": …, "args": …}` per line), oldest first.
    /// [`log::parse_jsonl`] reads it back; [`Session::replay`] on [`Session::initial_document`] reproduces the
    /// session.
    pub fn log_jsonl(&self) -> String {
        log::to_jsonl(self.history())
    }

    /// Replaces the document (open, new), clearing history.
    pub fn replace_document(&mut self, doc: Document) {
        self.past.clear();
        self.future.clear();
        self.current = doc;
        self.revision += 1;
        self.saved_revision = self.revision;
    }

    /// Records that the current revision has been saved.
    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Whether there are unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.saved_revision != self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::commands::{CreateElement, CreateStorey, DeleteElements, TransformElements, TransformSpec};
    use super::*;
    use kanavos_geom::Point2;
    use kanavos_model::{ElementKind, ReferenceLine, Structure, Wall};
    use kanavos_units::Length;

    fn storey(session: &mut Session) -> Id {
        let out = session.run(CreateStorey::new("Ισόγειο", Length::ZERO, Length::m(3.2))).unwrap();
        out.created[0]
    }

    fn wall(st: Id, len: f64) -> CreateElement {
        CreateElement::on(
            st,
            ElementKind::Wall(Wall {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(len, 0.0),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Basic { material: Id(1), thickness: Length::cm(25.0) },
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        )
    }

    fn session() -> Session {
        // Material 1 must exist for basic structures: use the starter template.
        Session::new(kanavos_model::starter())
    }

    #[test]
    fn undo_and_redo_restore_exact_versions() {
        let mut s = session();
        let st = storey(&mut s);
        s.run(wall(st, 6.4)).unwrap();
        assert_eq!(s.document().walls_on(st).count(), 1);
        assert_eq!(s.undo(), Some("element.create"));
        assert_eq!(s.document().walls_on(st).count(), 0);
        assert_eq!(s.redo(), Some("element.create"));
        assert_eq!(s.document().walls_on(st).count(), 1);
        assert_eq!(s.redo(), None);
    }

    #[test]
    fn new_command_after_undo_discards_redo() {
        let mut s = session();
        let st = storey(&mut s);
        s.run(wall(st, 3.0)).unwrap();
        s.undo();
        s.run(wall(st, 3.0)).unwrap();
        assert_eq!(s.redo(), None);
        assert_eq!(s.steps(), vec!["storey.create", "element.create"]);
    }

    #[test]
    fn failed_command_leaves_history_untouched() {
        let mut s = session();
        assert!(s.run(wall(Id(4242), 3.0)).is_err());
        assert_eq!(s.version(), 0);
        assert!(!s.is_dirty());
    }

    #[test]
    fn dirty_tracks_saves() {
        let mut s = session();
        storey(&mut s);
        assert!(s.is_dirty());
        s.mark_saved();
        assert!(!s.is_dirty());
        s.undo();
        assert!(s.is_dirty());
    }

    #[test]
    fn history_labels_name_the_moved_elements() {
        let mut s = session();
        let st = storey(&mut s);
        let a = s.run(wall(st, 3.0)).unwrap().created[0];
        let b = s.run(wall(st, 4.0)).unwrap().created[0];
        s.run(TransformElements {
            ids: vec![a, b],
            transform: TransformSpec::Move { dx: 1.0, dy: 0.0 },
            copy: false,
            count: 1,
        })
        .unwrap();
        s.run(DeleteElements { ids: vec![a] }).unwrap();
        let el = s.history_labels(Locale::El);
        assert_eq!(el[1], "Νέος τοίχος");
        assert_eq!(el[3], "Μετακίνηση 2 τοίχων");
        assert_eq!(el[4], "Διαγραφή τοίχου");
        let en = s.history_labels(Locale::En);
        assert_eq!(en[3], "Move 2 walls");
        assert_eq!(en[4], "Delete wall");
        s.undo();
        assert_eq!(s.redo_labels(Locale::En), vec!["Delete wall".to_owned()]);
    }

    #[test]
    fn replay_reproduces_the_session() {
        let mut s = session();
        let st = storey(&mut s);
        s.run(wall(st, 5.0)).unwrap();
        s.run(wall(st, 2.0)).unwrap();
        let log = s.log_jsonl();
        assert_eq!(log.lines().count(), 3);
        let commands = log::parse_jsonl(&log).unwrap();
        let replayed = Session::replay(s.initial_document().clone(), commands).unwrap();
        assert_eq!(replayed.document(), s.document());
        assert_eq!(replayed.steps(), s.steps());
    }

    #[test]
    fn replay_reports_the_failing_command() {
        let commands: Vec<AnyCommand> = vec![wall(Id(4242), 3.0).into()];
        let err = Session::replay(kanavos_model::starter(), commands).unwrap_err();
        assert_eq!(err.index, 0);
    }

    #[test]
    fn undo_to_and_redo_to_move_through_history() {
        let mut s = session();
        let st = storey(&mut s);
        s.run(wall(st, 5.0)).unwrap();
        s.run(wall(st, 2.0)).unwrap();
        s.undo_to(1);
        assert_eq!(s.version(), 1);
        s.redo_to(3);
        assert_eq!(s.version(), 3);
    }
}
