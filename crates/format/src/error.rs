//! Errors and load warnings.

use std::fmt;
use std::path::PathBuf;

use kanavos_model::InvalidDocument;

/// Error from reading or writing a `.kanavos` file. Messages are written for users.
#[derive(Debug)]
pub enum FormatError {
    /// A file system operation failed.
    Io {
        /// What was being done, e.g. "reading".
        action: &'static str,
        /// The file or directory involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The bytes are not a ZIP archive at all.
    NotAnArchive,
    /// The file is a ZIP archive but it is damaged.
    CorruptArchive(String),
    /// The archive is not a Kanavos project (for example another application's ZIP-based file).
    WrongFormat(String),
    /// The file was written by a newer Kanavos with a schema this build cannot read.
    NewerVersion {
        /// Schema version in the file.
        found: u32,
        /// Highest schema version this build reads.
        supported: u32,
    },
    /// A required entry is missing from the archive.
    MissingEntry(&'static str),
    /// An entry is larger than this build accepts.
    TooLarge {
        /// Entry name.
        entry: String,
        /// Declared uncompressed size in bytes.
        size: u64,
    },
    /// A JSON entry could not be read. `path` points at the failing field, e.g. `elements.12.kind.height`.
    CorruptJson {
        /// Entry name, e.g. `model.json`.
        entry: String,
        /// Path to the failing field (empty when the JSON text itself is malformed).
        path: String,
        /// Line in the entry (1-based; 0 when unknown).
        line: usize,
        /// Column in the entry (1-based; 0 when unknown).
        column: usize,
        /// Parser message.
        message: String,
    },
    /// Upgrading an older schema failed.
    Migration {
        /// Schema version the failing step starts from.
        from: u32,
        /// Reason.
        message: String,
    },
    /// The document loaded but is internally inconsistent.
    Invalid(InvalidDocument),
    /// The document holds a number that JSON cannot represent (NaN or infinity), so it cannot be saved.
    NonFinite {
        /// Path to the field, e.g. `elements.12.kind.start.0`.
        path: String,
    },
    /// A manifest timestamp is not an RFC 3339 date-time.
    InvalidTimestamp(String),
    /// The thumbnail is not a PNG image.
    InvalidThumbnail,
    /// Serialising the document failed.
    Serialize(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Io { action, path, source } => {
                write!(f, "error {action} {}: {source}", path.display())
            }
            FormatError::NotAnArchive => write!(f, "this is not a Kanavos project file (not a ZIP archive)"),
            FormatError::CorruptArchive(why) => write!(f, "the project file is damaged: {why}"),
            FormatError::WrongFormat(why) => write!(f, "this is not a Kanavos project file: {why}"),
            FormatError::NewerVersion { found, supported } => write!(
                f,
                "the project was saved by a newer version of Kanavos (file schema {found}, this version reads up \
                 to {supported}); update Kanavos to open it"
            ),
            FormatError::MissingEntry(name) => write!(f, "the project file is incomplete: {name} is missing"),
            FormatError::TooLarge { entry, size } => {
                write!(f, "the project file entry {entry} is too large ({size} bytes)")
            }
            FormatError::CorruptJson { entry, path, line, column, message } => {
                write!(f, "the project file is damaged: {entry}")?;
                if !path.is_empty() {
                    write!(f, ", field {path}")?;
                }
                if *line > 0 {
                    write!(f, " (line {line}, column {column})")?;
                }
                write!(f, ": {message}")
            }
            FormatError::Migration { from, message } => {
                write!(f, "could not upgrade the project from schema {from}: {message}")
            }
            FormatError::Invalid(e) => write!(f, "the project is inconsistent: {}", e.0),
            FormatError::NonFinite { path } => {
                write!(f, "cannot save: the value at {path} is not a finite number")
            }
            FormatError::InvalidTimestamp(t) => {
                write!(f, "invalid timestamp {t:?}: expected RFC 3339, e.g. 2026-10-09T12:00:00Z")
            }
            FormatError::InvalidThumbnail => write!(f, "the thumbnail is not a PNG image"),
            FormatError::Serialize(why) => write!(f, "could not serialise the project: {why}"),
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FormatError::Io { source, .. } => Some(source),
            FormatError::Invalid(e) => Some(e),
            _ => None,
        }
    }
}

impl FormatError {
    pub(crate) fn io(action: &'static str, path: impl Into<PathBuf>, source: std::io::Error) -> FormatError {
        FormatError::Io { action, path: path.into(), source }
    }
}

/// Something worth telling the user about a file that loaded successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWarning {
    /// The file used an older schema and was upgraded in memory; saving writes the current schema.
    Migrated {
        /// Schema version in the file.
        from: u32,
        /// Schema version after the upgrade.
        to: u32,
    },
    /// The archive holds an entry this build does not know; it is not kept when saving.
    UnknownEntry(String),
    /// The element counts in the manifest do not match the model (the file was edited by hand or damaged).
    CountMismatch {
        /// What was counted, e.g. `elements`.
        what: String,
        /// Count in the manifest.
        manifest: usize,
        /// Count in the model.
        model: usize,
    },
    /// The document failed its consistency check but was loaded because validation was relaxed.
    Invalid(String),
    /// The thumbnail entry is not a PNG image and was ignored.
    BadThumbnail,
}

impl fmt::Display for LoadWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadWarning::Migrated { from, to } => {
                write!(f, "the project was upgraded from schema {from} to {to}; saving will use schema {to}")
            }
            LoadWarning::UnknownEntry(name) => write!(f, "unknown entry {name} ignored"),
            LoadWarning::CountMismatch { what, manifest, model } => {
                write!(f, "the manifest lists {manifest} {what} but the model has {model}")
            }
            LoadWarning::Invalid(why) => write!(f, "the project is inconsistent: {why}"),
            LoadWarning::BadThumbnail => write!(f, "the thumbnail is not a PNG image and was ignored"),
        }
    }
}
