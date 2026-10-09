//! Saving, opening and crash recovery: the adapter between the interface and the native file format.
//!
//! **Temporary implementation.** Until `kanavos-format` (the `.kanavos` ZIP container) is merged, projects are
//! written as a JSON envelope around the model (`{"format": "kanavos-preview-json", "version": 1, "document":
//! …}`). To switch, replace the bodies of [`save`] and [`load`] with `kanavos_format::save` / `kanavos_format::load`;
//! nothing else in the interface reads or writes project files.
//!
//! Recovery: [`write_recovery`] stores the document every autosave interval in the data directory, and
//! [`read_recovery`] offers it on the next start when the previous session did not end cleanly.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kanavos_model::Document;
use serde::{Deserialize, Serialize};

/// File extension of Kanavos projects.
pub const EXTENSION: &str = "kanavos";

const FORMAT: &str = "kanavos-preview-json";

/// Why a project could not be saved or opened.
#[derive(Debug)]
pub enum PersistError {
    /// Reading or writing the file failed.
    Io(std::io::Error),
    /// The file is not a Kanavos project or is damaged.
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

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    document: Document,
}

#[derive(Serialize)]
struct EnvelopeRef<'a> {
    format: &'a str,
    version: u32,
    document: &'a Document,
}

fn encode(doc: &Document) -> Result<Vec<u8>, PersistError> {
    serde_json::to_vec_pretty(&EnvelopeRef { format: FORMAT, version: 1, document: doc })
        .map_err(|e| PersistError::Format(e.to_string()))
}

fn decode(bytes: &[u8]) -> Result<Document, PersistError> {
    if bytes.starts_with(b"PK") {
        return Err(PersistError::Format(
            "this project uses the packaged .kanavos format, which this build cannot read yet".into(),
        ));
    }
    let env: Envelope = serde_json::from_slice(bytes).map_err(|e| PersistError::Format(e.to_string()))?;
    if env.format != FORMAT {
        return Err(PersistError::Format(format!("unknown format '{}'", env.format)));
    }
    env.document.validate().map_err(|e| PersistError::Format(e.to_string()))?;
    Ok(env.document)
}

/// Writes bytes next to `path` and renames them into place, so a failed write never damages the old file.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Saves a project.
pub fn save(path: &Path, doc: &Document) -> Result<(), PersistError> {
    write_atomically(path, &encode(doc)?)
}

/// Opens a project.
pub fn load(path: &Path) -> Result<Document, PersistError> {
    decode(&std::fs::read(path)?)
}

/// `path` with the project extension added when it has none.
pub fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() { path } else { path.with_extension(EXTENSION) }
}

/// A recovery file found at start-up.
#[derive(Debug, Clone, PartialEq)]
pub struct Recovery {
    /// The recovered document.
    pub document: Document,
    /// File the document belonged to, if it had been saved.
    pub original: Option<PathBuf>,
    /// When it was written (seconds since 1970).
    pub written: u64,
}

#[derive(Serialize, Deserialize)]
struct RecoveryMeta {
    original: Option<PathBuf>,
    written: u64,
}

fn recovery_dir() -> Option<PathBuf> {
    crate::prefs::data_dir().map(|d| d.join("recovery"))
}

/// Writes a recovery copy into `dir` (blocking; call from a background thread).
pub fn write_recovery_in(dir: &Path, doc: &Document, original: Option<&Path>) -> Result<(), PersistError> {
    std::fs::create_dir_all(dir)?;
    let written = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let meta = RecoveryMeta { original: original.map(Path::to_path_buf), written };
    let meta = serde_json::to_vec(&meta).map_err(|e| PersistError::Format(e.to_string()))?;
    write_atomically(&dir.join("autosave.kanavos"), &encode(doc)?)?;
    write_atomically(&dir.join("autosave.json"), &meta)
}

/// Reads a recovery copy from `dir`, if there is a readable one.
pub fn read_recovery_in(dir: &Path) -> Option<Recovery> {
    let document = load(&dir.join("autosave.kanavos")).ok()?;
    let meta: Option<RecoveryMeta> =
        std::fs::read(dir.join("autosave.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let (original, written) = meta.map(|m| (m.original, m.written)).unwrap_or((None, 0));
    Some(Recovery { document, original, written })
}

/// Removes the recovery copy in `dir`.
pub fn clear_recovery_in(dir: &Path) {
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
    // disconnect, which the caller treats as a failed autosave.
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
    fn save_then_load_gives_the_same_document() {
        let dir = temp("rt");
        let path = dir.join("house.kanavos");
        let doc = kanavos_model::starter();
        save(&path, &doc).unwrap();
        assert_eq!(load(&path).unwrap(), doc);
        // Saving again is byte-identical (deterministic output).
        let first = std::fs::read(&path).unwrap();
        save(&path, &load(&path).unwrap()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), first);
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
