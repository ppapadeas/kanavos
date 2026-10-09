//! IFC 4.3 import and export.
//!
//! Layer L6. See `docs/architecture.md` §6 and the mapping tables in `docs/formats/ifc.md`.
//!
//! - [`export()`] writes a document as IFC 4.3 (`IFC4X3_ADD2`) in the ISO 10303-21 clear-text encoding.
//! - [`import()`] reads IFC 2x3, IFC 4 and IFC 4.3 files and maps what it can into a new document, with a report
//!   of everything it could not map.
//! - [`step`] is the schema-independent Part 21 reader and writer both are built on.
//!
//! Sources: ISO 16739-1:2024 (IFC 4.3), the buildingSMART IFC 4.3 and IFC 2x3 documentation, and ISO 10303-21.

#![forbid(unsafe_code)]

pub mod guid;
pub mod math;
pub mod step;

use std::fmt;
use std::path::PathBuf;

pub use guid::Guid;
pub use step::{StepError, StepFile};

/// Error from reading or writing IFC.
#[derive(Debug)]
pub enum IfcError {
    /// A file system operation failed.
    Io {
        /// What was being done.
        action: &'static str,
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file is not valid ISO 10303-21.
    Syntax(StepError),
    /// The file uses a schema this build does not read.
    UnsupportedSchema(String),
    /// The file has no `IfcProject`.
    NoProject,
}

impl fmt::Display for IfcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IfcError::Io { action, path, source } => write!(f, "error {action} {}: {source}", path.display()),
            IfcError::Syntax(e) => write!(f, "the IFC file is not valid ISO 10303-21 ({e})"),
            IfcError::UnsupportedSchema(s) => {
                write!(f, "unsupported IFC schema {s:?}; IFC2X3, IFC4 and IFC4X3 can be imported")
            }
            IfcError::NoProject => write!(f, "the IFC file has no IfcProject"),
        }
    }
}

impl std::error::Error for IfcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IfcError::Io { source, .. } => Some(source),
            IfcError::Syntax(e) => Some(e),
            _ => None,
        }
    }
}

impl From<StepError> for IfcError {
    fn from(e: StepError) -> Self {
        IfcError::Syntax(e)
    }
}
