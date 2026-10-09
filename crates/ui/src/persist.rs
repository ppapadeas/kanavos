//! Saving, opening and crash recovery: the adapter between the interface and the native file format.
//!
//! Projects are `.kanavos` containers written by `kanavos-format` (atomic writes, a `.bak` of the previous file,
//! schema migration on load). Files written by earlier preview builds of the interface (a JSON envelope around
//! the model) are still read, and saving writes them in the native format.
//!
//! Recovery: [`write_recovery`] stores a recovery copy in the data directory every autosave interval, and
//! [`read_recovery`] offers it on the next start when the previous session did not end cleanly.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use kanavos_format::{LoadOptions, Manifest, SaveOptions, Validation};
use kanavos_model::Document;
use serde::Deserialize;

/// File extension of Kanavos projects.
pub const EXTENSION: &str = kanavos_format::FILE_EXTENSION;

const PREVIEW_FORMAT: &str = "kanavos-preview-json";

/// Why a project could not be saved or opened. Messages come from the format crate and are written for users.
#[derive(Debug)]
pub enum PersistError {
    /// Reading or writing the file failed.
    Io(std::io::Error),
    /// The file is not a Kanavos project, is damaged, or is from a newer version.
    Format(String),
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistError::Io(e) => write!(f, "{e}"),
            PersistError::Format(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for PersistError {}

impl From<std::io::Error> for PersistError {
    fn from(e: std::io::Error) -> Self {
        PersistError::Io(e)
    }
}

impl From<kanavos_format::FormatError> for PersistError {
    fn from(e: kanavos_format::FormatError) -> Self {
        PersistError::Format(e.to_string())
    }
}

/// The current time as RFC 3339 (UTC), for file timestamps.
pub fn now_rfc3339() -> String {
    let d = kanavos_pdf::PdfDate::now();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", d.year, d.month, d.day, d.hour, d.minute, d.second)
}

/// A project read from disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    /// The document, upgraded to the current schema.
    pub document: Document,
    /// The manifest, when the file is a native container (keep it to preserve the creation time on save).
    pub manifest: Option<Manifest>,
    /// Things worth telling the user (upgrades, ignored entries).
    pub warnings: Vec<String>,
}

/// Saves a project. `previous` is the manifest of the file the document came from, so the creation time is kept.
/// Returns the manifest written.
pub fn save(path: &Path, doc: &Document, previous: Option<&Manifest>) -> Result<Manifest, PersistError> {
    let now = now_rfc3339();
    let options = match previous {
        Some(m) if m.created.is_some() => SaveOptions::resave(m, now),
        _ => SaveOptions { created: Some(now.clone()), modified: Some(now), ..SaveOptions::default() },
    };
    Ok(kanavos_format::save(path, doc, &options)?)
}

#[derive(Deserialize)]
struct PreviewEnvelope {
    format: String,
    document: Document,
}

/// Reads a JSON envelope written by preview builds.
fn decode_preview(bytes: &[u8]) -> Option<Document> {
    let env: PreviewEnvelope = serde_json::from_slice(bytes).ok()?;
    (env.format == PREVIEW_FORMAT && env.document.validate().is_ok()).then_some(env.document)
}

fn decode(bytes: &[u8], validation: Validation) -> Result<Opened, PersistError> {
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
        return decode_preview(bytes)
            .map(|document| Opened { document, manifest: None, warnings: Vec::new() })
            .ok_or_else(|| PersistError::Format("the file is not a Kanavos project".into()));
    }
    let loaded = kanavos_format::from_bytes_with(bytes, &LoadOptions { validation })?;
    Ok(Opened {
        document: loaded.document,
        manifest: Some(loaded.manifest),
        warnings: loaded.warnings.iter().map(ToString::to_string).collect(),
    })
}

/// Opens a project.
pub fn load(path: &Path) -> Result<Opened, PersistError> {
    decode(&std::fs::read(path)?, Validation::Strict)
}

/// `path` with the project extension added when it has none.
pub fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() { path } else { path.with_extension(EXTENSION) }
}

/// A recovery copy found at start-up.
#[derive(Debug, Clone, PartialEq)]
pub struct Recovery {
    /// The recovered document.
    pub document: Document,
    /// File the document belonged to, if it had been saved.
    pub original: Option<PathBuf>,
    /// When it was written (seconds since 1970; 0 when unknown).
    pub written: u64,
}

fn recovery_dir() -> Option<PathBuf> {
    crate::prefs::data_dir().map(|d| d.join("recovery"))
}

/// Writes a recovery copy into `dir` (blocking; call from a background thread).
pub fn write_recovery_in(dir: &Path, doc: &Document, original: Option<&Path>) -> Result<(), PersistError> {
    let options = SaveOptions {
        backup: false,
        modified: Some(now_rfc3339()),
        recovery_of: original.map(|p| p.display().to_string()),
        ..SaveOptions::default()
    };
    kanavos_format::autosave_with(dir, doc, &options)?;
    Ok(())
}

/// Reads a recovery copy from `dir`, if there is a readable one. Damaged documents are loaded leniently: a
/// partly consistent copy is better than none.
pub fn read_recovery_in(dir: &Path) -> Option<Recovery> {
    let found = kanavos_format::find_recovery(dir).ok().flatten()?;
    let opened = std::fs::read(&found.path).ok().and_then(|b| decode(&b, Validation::Lenient).ok())?;
    let original = found.manifest.as_ref().and_then(|m| m.recovery_of.clone()).map(PathBuf::from);
    let written =
        found.file_modified.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    Some(Recovery { document: opened.document, original, written })
}

/// Removes the recovery copy in `dir` (and copies left by preview builds).
pub fn clear_recovery_in(dir: &Path) {
    let _ = kanavos_format::discard_recovery(dir);
    let _ = std::fs::remove_file(dir.join("autosave.kanavos"));
    let _ = std::fs::remove_file(dir.join("autosave.json"));
}

/// Starts writing a recovery copy in the background. Errors are reported through the returned channel.
pub fn write_recovery(
    doc: Document,
    original: Option<PathBuf>,
) -> std::sync::mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    // When the thread cannot start, the sender is dropped with the closure and the receiver reports a
    // disconnect, which the caller treats as a finished autosave.
    let _ = std::thread::Builder::new().name("kanavos-autosave".into()).spawn(move || {
        let r = match recovery_dir() {
            Some(dir) => write_recovery_in(&dir, &doc, original.as_deref()).map_err(|e| e.to_string()),
            None => Err("no data directory".into()),
        };
        let _ = tx.send(r);
    });
    rx
}

/// The recovery copy left by a previous session, if any.
pub fn read_recovery() -> Option<Recovery> {
    recovery_dir().and_then(|d| read_recovery_in(&d))
}

/// Removes the recovery copy (after a clean exit, a save, or when the user discards it).
pub fn clear_recovery() {
    if let Some(d) = recovery_dir() {
        clear_recovery_in(&d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kanavos-persist-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn save_then_load_gives_the_same_document_and_keeps_the_creation_time() {
        let dir = temp("rt");
        let path = dir.join("house.kanavos");
        let doc = kanavos_model::starter();
        let first = save(&path, &doc, None).unwrap();
        let opened = load(&path).unwrap();
        assert_eq!(opened.document, doc);
        let manifest = opened.manifest.unwrap();
        assert_eq!(manifest.created, first.created);
        let again = save(&path, &opened.document, Some(&manifest)).unwrap();
        assert_eq!(again.created, first.created);
        assert!(path.with_extension("kanavos.bak").exists() || dir.join("house.kanavos.bak").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timestamps_are_rfc3339() {
        assert!(kanavos_format::is_rfc3339(&now_rfc3339()));
    }

    #[test]
    fn preview_json_files_still_open() {
        let dir = temp("preview");
        let path = dir.join("old.kanavos");
        let doc = kanavos_model::starter();
        let json = serde_json::json!({ "format": PREVIEW_FORMAT, "version": 1, "document": doc });
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let opened = load(&path).unwrap();
        assert_eq!(opened.document, doc);
        assert!(opened.manifest.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn foreign_and_damaged_files_are_rejected() {
        let dir = temp("bad");
        let path = dir.join("x.kanavos");
        std::fs::write(&path, b"PK\x03\x04zip").unwrap();
        assert!(matches!(load(&path), Err(PersistError::Format(_))));
        std::fs::write(&path, b"{\"format\":\"other\",\"version\":1,\"document\":{}}").unwrap();
        assert!(matches!(load(&path), Err(PersistError::Format(_))));
        assert!(matches!(load(&dir.join("missing.kanavos")), Err(PersistError::Io(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_round_trips_with_its_original_path() {
        let dir = temp("rec");
        let doc = kanavos_model::starter();
        write_recovery_in(&dir, &doc, Some(Path::new("C:/projects/house.kanavos"))).unwrap();
        let r = read_recovery_in(&dir).unwrap();
        assert_eq!(r.document, doc);
        assert_eq!(r.original.as_deref(), Some(Path::new("C:/projects/house.kanavos")));
        assert!(r.written > 0);
        clear_recovery_in(&dir);
        assert!(read_recovery_in(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extension_is_added_when_missing() {
        assert_eq!(with_extension(PathBuf::from("a")), PathBuf::from("a.kanavos"));
        assert_eq!(with_extension(PathBuf::from("a.json")), PathBuf::from("a.json"));
    }
}
