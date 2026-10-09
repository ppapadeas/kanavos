//! Opening and saving documents: the single place where the native file format is connected.
//!
//! **Adapter.** Until the `.kanavos` container (`kanavos-format`) is merged, documents are stored as a JSON
//! envelope `{"format": "kanavos-json", "version": 1, "document": {…}}`, written atomically. To switch, replace
//! the bodies of [`save`], [`load`], [`to_bytes`] and [`from_bytes`] with calls to `kanavos_format::save(path,
//! doc, &SaveOptions { app_version, .. })`, `kanavos_format::load(path)` (→ `Loaded { document, warnings, .. }`),
//! `kanavos_format::to_bytes` and `kanavos_format::from_bytes`, and map `FormatError` into [`IoError::Format`].
//! Nothing else in the engine, the control protocol or the CLI reads or writes files directly.

use std::fmt;
use std::path::Path;

use kanavos_model::Document;
use kanavos_units::Locale;
use serde::{Deserialize, Serialize};

/// Format name written in the temporary JSON envelope.
pub const JSON_FORMAT: &str = "kanavos-json";
/// Version of the temporary JSON envelope.
pub const JSON_VERSION: u32 = 1;
/// File extension of Kanavos documents.
pub const EXTENSION: &str = "kanavos";

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
    /// The file uses a format this build cannot read or write yet.
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

/// A document read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The document.
    pub document: Document,
    /// Things the reader fixed or ignored, for the user.
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    app_version: &'static str,
    document: &'a Document,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    format: String,
    version: u32,
    document: Document,
}

fn io(path: &Path, e: &std::io::Error) -> IoError {
    IoError::Io { path: path.display().to_string(), reason: e.to_string() }
}

/// Serialises a document to file bytes.
pub fn to_bytes(doc: &Document) -> Result<Vec<u8>, IoError> {
    let env = EnvelopeOut {
        format: JSON_FORMAT,
        version: JSON_VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
        document: doc,
    };
    let mut bytes = serde_json::to_vec_pretty(&env).map_err(|e| IoError::Format(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Reads a document from file bytes and checks its consistency.
pub fn from_bytes(bytes: &[u8]) -> Result<Loaded, IoError> {
    if bytes.starts_with(b"PK\x03\x04") {
        return Err(IoError::NotAvailable("the .kanavos container (ZIP)".into()));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| IoError::Format(e.to_string()))?;
    let text = text.trim_start_matches('\u{feff}');
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| IoError::Format(e.to_string()))?;
    let mut warnings = Vec::new();
    let document = if value.get("format").is_some() {
        let env: EnvelopeIn = serde_path_to_error::deserialize(value)
            .map_err(|e| IoError::Format(format!("{}: {}", e.path(), e.inner())))?;
        if env.format != JSON_FORMAT {
            return Err(IoError::Format(format!("unknown format '{}'", env.format)));
        }
        if env.version > JSON_VERSION {
            return Err(IoError::Format(format!(
                "written by a newer version (format version {})",
                env.version
            )));
        }
        env.document
    } else {
        warnings.push("bare document JSON without an envelope".to_owned());
        serde_path_to_error::deserialize(value)
            .map_err(|e| IoError::Format(format!("{}: {}", e.path(), e.inner())))?
    };
    document.validate().map_err(|e| IoError::Format(e.to_string()))?;
    Ok(Loaded { document, warnings })
}

/// Saves a document. The file is written next to its destination and renamed into place, so a failed save never
/// leaves a half-written file.
pub fn save(path: &Path, doc: &Document) -> Result<(), IoError> {
    let bytes = to_bytes(doc)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, &bytes).map_err(|e| io(&tmp, &e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(path, &e)
    })
}

/// Loads a document.
pub fn load(path: &Path) -> Result<Loaded, IoError> {
    let bytes = std::fs::read(path).map_err(|e| io(path, &e))?;
    from_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::starter;

    #[test]
    fn documents_round_trip_through_bytes_and_files() {
        let doc = starter();
        let bytes = to_bytes(&doc).unwrap();
        assert_eq!(from_bytes(&bytes).unwrap().document, doc);
        let dir = std::env::temp_dir().join(format!("kanavos-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.kanavos");
        save(&path, &doc).unwrap();
        assert_eq!(load(&path).unwrap().document, doc);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_files_are_reported() {
        assert!(matches!(from_bytes(b"PK\x03\x04rest"), Err(IoError::NotAvailable(_))));
        assert!(matches!(
            from_bytes(b"{\"format\":\"other\",\"version\":1,\"document\":{}}"),
            Err(IoError::Format(_))
        ));
        assert!(matches!(from_bytes(b"not json"), Err(IoError::Format(_))));
        let missing = load(Path::new("no/such/file.kanavos")).unwrap_err();
        assert!(matches!(missing, IoError::Io { .. }));
        assert!(!missing.message(Locale::El).is_empty());
    }
}
