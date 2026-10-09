//! Imports: the adapter between the File ▸ Import menu and the importer crates.
//!
//! **Pending merges** — [`import`] is the one place to call the importers once they land:
//! - DXF survey (τοπογραφικό) as an underlay on the active storey: `kanavos_dxf` survey import, producing an
//!   `element.create` command for an `ElementKind::Underlay` (and optionally the plot outline via `site.set`).
//! - IFC 4.3: `kanavos_ifc`, producing a new document that replaces the open one.

use std::fmt;
use std::path::Path;

use kanavos_engine::AnyCommand;
use kanavos_model::{Document, Id};

/// What to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// A DXF survey drawing, placed as an underlay.
    DxfSurvey,
    /// An IFC model, opened as a new project.
    Ifc,
}

impl ImportKind {
    /// Every import, in menu order.
    pub const ALL: [ImportKind; 2] = [ImportKind::DxfSurvey, ImportKind::Ifc];

    /// File extensions accepted.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            ImportKind::DxfSurvey => &["dxf"],
            ImportKind::Ifc => &["ifc"],
        }
    }

    /// Catalogue key of the menu label.
    pub fn label_key(self) -> &'static str {
        match self {
            ImportKind::DxfSurvey => "ui.import.dxf",
            ImportKind::Ifc => "ui.import.ifc",
        }
    }

    /// Whether this build can perform the import.
    pub fn available(self) -> bool {
        false
    }
}

/// The result of an import.
#[derive(Debug, Clone, PartialEq)]
pub enum Imported {
    /// Commands that add the imported content to the open project (one undo step).
    Commands(Vec<AnyCommand>),
    /// A new document that replaces the open project.
    Document(Document),
}

/// Why an import failed.
#[derive(Debug)]
pub enum ImportError {
    /// The importer is not part of this build yet.
    NotAvailable,
    /// Reading the file failed.
    Io(std::io::Error),
    /// The file could not be understood.
    Format(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::NotAvailable => f.write_str("not available yet"),
            ImportError::Io(e) => write!(f, "{e}"),
            ImportError::Format(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for ImportError {}

/// Imports a file into the open project (`doc`, active storey `storey`).
pub fn import(kind: ImportKind, path: &Path, doc: &Document, storey: Id) -> Result<Imported, ImportError> {
    let _ = (path, doc, storey);
    match kind {
        // Pending merges: see the module documentation.
        ImportKind::DxfSurvey | ImportKind::Ifc => Err(ImportError::NotAvailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_importers_report_not_available() {
        for k in ImportKind::ALL {
            assert!(!k.available());
            let r = import(k, Path::new("x"), &Document::new(), Id(1));
            assert!(matches!(r, Err(ImportError::NotAvailable)));
        }
    }
}
