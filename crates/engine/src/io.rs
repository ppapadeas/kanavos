//! Opening and saving documents: the single place where the native file format is connected.
//!
//! Documents are `.kanavos` files written by `kanavos-format` (a ZIP container with a manifest and the model as
//! JSON, see `docs/format.md`); saves are atomic and keep the previous file as a backup. Nothing else in the
//! engine, the control protocol or the CLI reads or writes project files directly.

use std::fmt;
use std::path::Path;

use kanavos_format::{FormatError, SaveOptions};
use kanavos_model::Document;
use kanavos_units::Locale;

/// File extension of Kanavos documents.
pub const EXTENSION: &str = kanavos_format::FILE_EXTENSION;

/// Why a document could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoError {
    /// The file system refused (missing file, permissions, disk full).
    Io {
        /// File.
        path: String,
        /// Reason from the operating system.
        reason: String,
    },
    /// The data is not a valid Kanavos document.
    Format(String),
    /// The file uses a format this build cannot read or write.
    NotAvailable(String),
}

impl IoError {
    /// The message in a language.
    pub fn message(&self, locale: Locale) -> String {
        use kanavos_i18n::tf;
        match self {
            IoError::Io { path, reason } => {
                tf(locale, "engine.io.error.io", &[("path", path), ("reason", reason)])
            }
            IoError::Format(reason) => tf(locale, "engine.io.error.format", &[("reason", reason)]),
            IoError::NotAvailable(what) => tf(locale, "engine.io.error.not_available", &[("what", what)]),
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Locale::En))
    }
}

impl std::error::Error for IoError {}

impl From<FormatError> for IoError {
    fn from(e: FormatError) -> Self {
        match e {
            FormatError::Io { path, source, .. } => {
                IoError::Io { path: path.display().to_string(), reason: source.to_string() }
            }
            FormatError::NewerVersion { .. } => IoError::NotAvailable(e.to_string()),
            other => IoError::Format(other.to_string()),
        }
    }
}

/// A document read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The document.
    pub document: Document,
    /// When the project was first created (RFC 3339), to keep when saving it again.
    pub created: Option<String>,
    /// Things the reader fixed or ignored, for the user.
    pub warnings: Vec<String>,
}

impl From<kanavos_format::Loaded> for Loaded {
    fn from(l: kanavos_format::Loaded) -> Self {
        Loaded {
            document: l.document,
            created: l.manifest.created.clone(),
            warnings: l.warnings.iter().map(ToString::to_string).collect(),
        }
    }
}

/// The current time as RFC 3339 in UTC, e.g. `2026-10-10T08:30:00Z`.
pub fn now_rfc3339() -> String {
    let d = kanavos_pdf::PdfDate::now();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", d.year, d.month, d.day, d.hour, d.minute, d.second)
}

fn options(created: Option<&str>) -> SaveOptions {
    let now = now_rfc3339();
    SaveOptions {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        created: Some(created.map(str::to_owned).unwrap_or_else(|| now.clone())),
        modified: Some(now),
        ..SaveOptions::default()
    }
}

/// Serialises a document to file bytes.
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, IoError> {
    Ok(kanavos_format::to_bytes(doc, &options(None))?)
}

/// Reads a document from file bytes and checks its consistency.
pub fn from_bytes(bytes: &[u8]) -> Result<Loaded, IoError> {
    Ok(kanavos_format::from_bytes(bytes)?.into())
}

/// Saves a document (atomically, keeping the previous file as `<name>.bak`).
pub fn save(path: &Path, doc: &Document) -> Result<(), IoError> {
    save_with(path, doc, None)
}

/// Saves a document, keeping the creation time of a project that was loaded from a file.
pub fn save_with(path: &Path, doc: &Document, created: Option<&str>) -> Result<(), IoError> {
    kanavos_format::save(path, doc, &options(created))?;
    Ok(())
}

/// Loads a document.
pub fn load(path: &Path) -> Result<Loaded, IoError> {
    Ok(kanavos_format::load(path)?.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::starter;

    #[test]
    fn documents_round_trip_through_bytes_and_files() {
        let doc = starter();
        let bytes = to_bytes(&doc).unwrap();
        assert!(bytes.starts_with(b"PK"));
        assert_eq!(from_bytes(&bytes).unwrap().document, doc);
        let dir = std::env::temp_dir().join(format!("kanavos-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.kanavos");
        save(&path, &doc).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.document, doc);
        let created = loaded.created.clone().unwrap();
        save_with(&path, &doc, Some(&created)).unwrap();
        assert_eq!(load(&path).unwrap().created, Some(created));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_files_are_reported() {
        assert!(matches!(from_bytes(b"not a project"), Err(IoError::Format(_))));
        let missing = load(Path::new("no/such/file.kanavos")).unwrap_err();
        assert!(matches!(missing, IoError::Io { .. }));
        assert!(!missing.message(Locale::El).is_empty());
    }

    #[test]
    fn timestamps_are_rfc_3339() {
        let t = now_rfc3339();
        assert_eq!(t.len(), 20);
        assert!(kanavos_format::is_rfc3339(&t));
    }
}
