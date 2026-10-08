//! Session, command bus, undo/redo and background job scheduling.
//!
//! Layer L7. See `docs/architecture.md` and ADR-0005.
//!
//! Every change to a document is a [`Command`]. A [`Session`] applies commands and keeps every document version,
//! which makes undo and redo trivial and exact.

pub mod commands;

use std::fmt;

use domos_model::{Document, Id};

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
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::InvalidArgument { field, reason } => write!(f, "invalid {field}: {reason}"),
            CommandError::NotFound(id) => write!(f, "object {} not found", id.0),
        }
    }
}

impl std::error::Error for CommandError {}

/// Outcome of a successful command.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// Objects created by the command.
    pub created: Vec<Id>,
}

/// A change to a document. Commands never panic; they return a [`CommandError`] instead.
pub trait Command {
    /// Stable public name, e.g. `"wall.create"`.
    fn name(&self) -> &'static str;
    /// Applies the command, returning the new document and what was created.
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError>;
}

/// An editing session over one document, with full history.
#[derive(Debug, Default)]
pub struct Session {
    /// Earlier documents, each with the name of the command that was applied to it.
    past: Vec<(Document, &'static str)>,
    current: Document,
    /// Undone documents, each with the name of the command that produced it.
    future: Vec<(Document, &'static str)>,
}

impl Session {
    /// Starts a session on a document.
    pub fn new(doc: Document) -> Self {
        Session { past: Vec::new(), current: doc, future: Vec::new() }
    }

    /// The current document.
    pub fn document(&self) -> &Document {
        &self.current
    }

    /// Number of steps between the opened document and the current one.
    pub fn version(&self) -> usize {
        self.past.len()
    }

    /// Runs a command. On success it becomes one undo step and any redo steps are discarded.
    pub fn run(&mut self, command: &dyn Command) -> Result<Outcome, CommandError> {
        let (next, outcome) = command.apply(&self.current)?;
        let previous = std::mem::replace(&mut self.current, next);
        self.past.push((previous, command.name()));
        self.future.clear();
        Ok(outcome)
    }

    /// Undoes the last step. Returns its command name, or `None` if there is nothing to undo.
    pub fn undo(&mut self) -> Option<&'static str> {
        let (previous, name) = self.past.pop()?;
        let undone = std::mem::replace(&mut self.current, previous);
        self.future.push((undone, name));
        Some(name)
    }

    /// Redoes the next step. Returns its command name, or `None` if there is nothing to redo.
    pub fn redo(&mut self) -> Option<&'static str> {
        let (next, name) = self.future.pop()?;
        let previous = std::mem::replace(&mut self.current, next);
        self.past.push((previous, name));
        Some(name)
    }

    /// Names of the applied steps, oldest first.
    pub fn steps(&self) -> Vec<&'static str> {
        self.past.iter().map(|(_, n)| *n).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::commands::{CreateStorey, CreateWall};
    use super::*;
    use domos_geom::Point2;
    use domos_units::Length;

    fn storey(session: &mut Session) -> Id {
        let out = session
            .run(&CreateStorey {
                name: "Ισόγειο".into(), elevation: Length::ZERO, height: Length::m(3.2)
            })
            .unwrap();
        out.created[0]
    }

    #[test]
    fn undo_and_redo_restore_exact_versions() {
        let mut s = Session::default();
        let st = storey(&mut s);
        let wall = CreateWall::centred(st, Point2::new(0.0, 0.0), Point2::new(6.4, 0.0), Length::cm(25.0));
        s.run(&wall).unwrap();
        assert_eq!(s.document().walls_on(st).count(), 1);

        assert_eq!(s.undo(), Some("wall.create"));
        assert_eq!(s.document().walls_on(st).count(), 0);
        assert_eq!(s.redo(), Some("wall.create"));
        assert_eq!(s.document().walls_on(st).count(), 1);
        assert_eq!(s.redo(), None);
    }

    #[test]
    fn new_command_after_undo_discards_redo() {
        let mut s = Session::default();
        let st = storey(&mut s);
        let w = CreateWall::centred(st, Point2::new(0.0, 0.0), Point2::new(3.0, 0.0), Length::cm(20.0));
        s.run(&w).unwrap();
        s.undo();
        s.run(&w).unwrap();
        assert_eq!(s.redo(), None);
        assert_eq!(s.steps(), vec!["storey.create", "wall.create"]);
    }

    #[test]
    fn failed_command_leaves_history_untouched() {
        let mut s = Session::default();
        let bad = CreateWall::centred(Id(42), Point2::new(0.0, 0.0), Point2::new(3.0, 0.0), Length::cm(20.0));
        assert_eq!(s.run(&bad), Err(CommandError::NotFound(Id(42))));
        assert_eq!(s.version(), 0);
    }
}
