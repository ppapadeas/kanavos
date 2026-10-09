//! Writing files safely: atomic replacement, backups, autosave and crash recovery.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use kanavos_model::Document;

use crate::container::{Archive, MANIFEST_ENTRY};
use crate::{FormatError, Manifest, SaveOptions, parse_manifest, to_bytes};

/// File name of the crash-recovery copy inside a recovery directory.
pub const RECOVERY_FILE_NAME: &str = "recovery.kanavos";

/// `path` with `suffix` appended to its file name (`house.kanavos` → `house.kanavos.bak`).
pub fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Path of the backup kept by [`crate::save`] (`house.kanavos.bak`).
pub fn backup_path(path: &Path) -> PathBuf {
    sibling(path, ".bak")
}

/// Writes `bytes` to `path` so that `path` always holds either the old or the new complete file.
///
/// The data goes to `path.tmp` first and is flushed to disk; then, if `backup` is set and `path` exists, the old
/// file is copied to `path.bak`; finally `path.tmp` is renamed over `path`. A crash at any point leaves `path`
/// intact (old or new), and at worst a stray `.tmp`.
pub fn write_atomic(path: &Path, bytes: &[u8], backup: bool) -> Result<(), FormatError> {
    let tmp = sibling(path, ".tmp");
    let result = write_atomic_inner(path, &tmp, bytes, backup);
    if result.is_err() {
        // Best effort: do not leave a half-written temporary file behind.
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_atomic_inner(path: &Path, tmp: &Path, bytes: &[u8], backup: bool) -> Result<(), FormatError> {
    {
        let mut file = File::create(tmp).map_err(|e| FormatError::io("creating", tmp, e))?;
        file.write_all(bytes).map_err(|e| FormatError::io("writing", tmp, e))?;
        file.sync_all().map_err(|e| FormatError::io("flushing", tmp, e))?;
    }
    if backup && path.is_file() {
        let bak = backup_path(path);
        fs::copy(path, &bak).map_err(|e| FormatError::io("backing up to", &bak, e))?;
    }
    fs::rename(tmp, path).map_err(|e| FormatError::io("replacing", path, e))?;
    sync_parent(path);
    Ok(())
}

/// Flushes the directory entry after a rename, where the platform supports it. Failure is not an error: the data
/// itself is already on disk.
fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent()
        && let Ok(d) = File::open(dir)
    {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Writes a crash-recovery copy of `document` into `dir` (created if missing) with default options and returns its
/// path. Call it periodically while a project has unsaved changes; call [`discard_recovery`] after a normal save
/// or close.
pub fn autosave(dir: &Path, document: &Document) -> Result<PathBuf, FormatError> {
    autosave_with(dir, document, &SaveOptions { backup: false, ..SaveOptions::default() })
}

/// Like [`autosave`], with explicit options (timestamps, the project path in `recovery_of`). The recovery file
/// never gets a `.bak`.
pub fn autosave_with(dir: &Path, document: &Document, options: &SaveOptions) -> Result<PathBuf, FormatError> {
    fs::create_dir_all(dir).map_err(|e| FormatError::io("creating", dir, e))?;
    let path = dir.join(RECOVERY_FILE_NAME);
    let bytes = to_bytes(document, options)?;
    write_atomic(&path, &bytes, false)?;
    Ok(path)
}

/// A crash-recovery file found by [`find_recovery`].
#[derive(Debug, Clone, PartialEq)]
pub struct Recovery {
    /// Path of the recovery file; open it with [`crate::load`].
    pub path: PathBuf,
    /// When the file system last modified it.
    pub file_modified: Option<SystemTime>,
    /// Its manifest, or `None` if the file is damaged (it may still be worth trying to load).
    pub manifest: Option<Manifest>,
}

/// Looks for a crash-recovery file in `dir`. Returns `Ok(None)` when there is none (including when `dir` does
/// not exist).
pub fn find_recovery(dir: &Path) -> Result<Option<Recovery>, FormatError> {
    let path = dir.join(RECOVERY_FILE_NAME);
    let meta = match fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(FormatError::io("checking", path, e)),
    };
    let manifest = read_manifest(&path).ok();
    Ok(Some(Recovery { path, file_modified: meta.modified().ok(), manifest }))
}

/// Deletes the crash-recovery file in `dir`, if any.
pub fn discard_recovery(dir: &Path) -> Result<(), FormatError> {
    let path = dir.join(RECOVERY_FILE_NAME);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(FormatError::io("deleting", path, e)),
    }
}

/// Reads only the manifest of a project file. Cheap: only the archive directory and the manifest entry are read,
/// not the model. For file browsers and recent-file lists.
pub fn read_manifest(path: &Path) -> Result<Manifest, FormatError> {
    let file = File::open(path).map_err(|e| FormatError::io("reading", path, e))?;
    let mut archive = Archive::open_reader(std::io::BufReader::new(file))?;
    let raw = archive
        .read(MANIFEST_ENTRY)?
        .ok_or_else(|| FormatError::WrongFormat("the archive has no manifest.json".into()))?;
    parse_manifest(&raw)
}
