//! Installing a verified package over the running installation.
//!
//! The installation folder is the folder of the running program. It must hold `QUICKSTART.txt` (every release
//! package does), so that a program started from a build folder or copied into a system folder never rewrites
//! that folder.
//!
//! Steps, each recorded in a journal file (`.kanavos-update.json`) in the installation folder:
//!
//! 1. **Staging**: every file of the package is copied next to its destination as `<file>.new` and flushed to
//!    disk. Nothing of the running installation changes yet; a failure removes the staged files.
//! 2. **Swapping**: for each file, the current one is renamed to `<file>.old` (a running program can be renamed on
//!    Windows, not replaced) and the staged one is renamed into place. Renames stay within one folder, so each
//!    is atomic. If a rename fails, the files already swapped are put back.
//! 3. **Installed**: the `.old` files are deleted, now or, for programs still running, by [`cleanup_old_in`] on a
//!    later start.
//!
//! If the computer stops during step 2, the next start finds the journal in the swapping state and completes the
//! installation from the staged files, which were complete before any rename began. A lock file
//! (`.kanavos-update.lock`) keeps two processes from working on the same folder at once.
//!
//! What is never touched: files that are not in the package (user files), and project files (`*.kanavos`)
//! other than the bundled `sample-house.kanavos`. Only `.old`/`.new` files recorded in the journal are deleted.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::archive::{self, safe_components};
use crate::{Downloaded, UpdateError};

/// File that marks a folder as an installation from a release package.
pub const MARKER: &str = "QUICKSTART.txt";
/// The bundled sample project, the only project file an update replaces.
pub const SAMPLE: &str = "sample-house.kanavos";
/// Journal of the installation in progress or of files left to delete.
pub const JOURNAL: &str = ".kanavos-update.json";
/// Lock held while a process installs or cleans up.
pub const LOCK: &str = ".kanavos-update.lock";
/// A lock older than this is left over from a process that stopped.
const LOCK_STALE: Duration = Duration::from_secs(120);
const STAGED: &str = ".new";

/// File name of the desktop program on this platform.
pub fn app_file() -> String {
    format!("kanavos{}", std::env::consts::EXE_SUFFIX)
}

/// File name of the command-line program on this platform.
pub fn cli_file() -> String {
    format!("kanavos-cli{}", std::env::consts::EXE_SUFFIX)
}

/// A finished installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The version now installed.
    pub version: Version,
    /// The installation folder.
    pub dir: PathBuf,
    /// The desktop program.
    pub app: PathBuf,
    /// The command-line program.
    pub cli: PathBuf,
    /// Number of files written.
    pub files: usize,
}

/// What [`cleanup_old_in`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cleanup {
    /// Leftover files deleted.
    pub removed: usize,
    /// Leftover files that could not be deleted yet (a program still running); tried again next time.
    pub pending: usize,
    /// An interrupted installation was completed.
    pub completed: bool,
    /// Another process is installing; nothing was done.
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum State {
    Staging,
    Swapping,
    Installed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    /// Destination, relative to the installation folder, `/`-separated.
    path: String,
    /// Where the current file goes, when there is one.
    #[serde(default)]
    backup: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Journal {
    format: u32,
    state: State,
    version: String,
    #[serde(default)]
    files: Vec<Entry>,
    /// Leftovers of earlier updates still to delete.
    #[serde(default)]
    trash: Vec<String>,
}

impl Journal {
    fn read(dir: &Path) -> Result<Option<Journal>, UpdateError> {
        let path = dir.join(JOURNAL);
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| UpdateError::BadPackage(format!("{JOURNAL}: {e}"))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(UpdateError::io(&path, e)),
        }
    }

    fn write(&self, dir: &Path) -> Result<(), UpdateError> {
        let path = dir.join(JOURNAL);
        let tmp = dir.join(format!("{JOURNAL}.tmp"));
        let text = serde_json::to_string_pretty(self).map_err(|e| UpdateError::BadPackage(e.to_string()))?;
        let mut f = fs::File::create(&tmp).map_err(|e| UpdateError::io(&tmp, e))?;
        f.write_all(text.as_bytes()).and_then(|()| f.sync_all()).map_err(|e| UpdateError::io(&tmp, e))?;
        drop(f);
        rename(&tmp, &path).map_err(|e| UpdateError::io(&path, e))
    }

    /// Records what is left to delete, or removes the journal when nothing is.
    fn finish(dir: &Path, version: String, trash: Vec<String>) {
        if trash.is_empty() {
            let _ = remove_file(&dir.join(JOURNAL));
        } else {
            let _ =
                Journal { format: 1, state: State::Installed, version, files: Vec::new(), trash }.write(dir);
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The lock file of a folder, removed when dropped.
struct Lock {
    path: PathBuf,
}

impl Lock {
    fn acquire(dir: &Path) -> Result<Lock, UpdateError> {
        let path = dir.join(LOCK);
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = write!(f, "{} {}", std::process::id(), now_secs());
                    return Ok(Lock { path });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if lock_is_fresh(&path) {
                        return Err(UpdateError::Busy);
                    }
                    let _ = remove_file(&path);
                }
                Err(_) => return Err(UpdateError::NotWritable(dir.to_path_buf())),
            }
        }
        Err(UpdateError::Busy)
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn lock_is_fresh(path: &Path) -> bool {
    let now = now_secs();
    let started = fs::read_to_string(path)
        .ok()
        .and_then(|t| t.split_whitespace().nth(1).and_then(|s| s.parse::<u64>().ok()));
    match started {
        Some(t) => t <= now.saturating_add(60) && now.saturating_sub(t) < LOCK_STALE.as_secs(),
        // Not written yet, or unreadable: judge by the file time.
        None => fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.elapsed().ok())
            .is_some_and(|age| age < LOCK_STALE),
    }
}

// ----- File operations ---------------------------------------------------------------------------------------

/// Runs a file operation, retrying for a short while: on Windows, virus scanners and indexers often hold a new
/// file open for a moment.
fn retry<T>(mut op: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut attempt = 0;
    loop {
        match op() {
            Ok(v) => return Ok(v),
            Err(e) if e.kind() == io::ErrorKind::NotFound || attempt >= 6 => return Err(e),
            Err(_) => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn rename(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    if fault::rename_fails(from) {
        return Err(io::Error::other("injected failure"));
    }
    retry(|| fs::rename(from, to))
}

/// Deletes a file; a file that does not exist counts as deleted.
fn remove_file(path: &Path) -> io::Result<()> {
    let r = retry(|| fs::remove_file(path));
    #[cfg(windows)]
    let r = match r {
        // A read-only file cannot be deleted on Windows until the attribute is cleared.
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => match fs::metadata(path) {
            Ok(m) if m.permissions().readonly() => {
                let mut p = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                p.set_readonly(false);
                fs::set_permissions(path, p).and_then(|()| fs::remove_file(path))
            }
            _ => Err(e),
        },
        other => other,
    };
    match r {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Whether something exists at a path (without following links).
fn present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// A relative `/`-separated path under `dir`, when it is safe.
fn under(dir: &Path, rel: &str) -> Option<PathBuf> {
    let parts = safe_components(rel).ok()?;
    Some(parts.iter().fold(dir.to_path_buf(), |p, c| p.join(c)))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Whether a relative path names an update leftover (`x.old`, `x.old.N` or `x.new`), the only files deletion
/// is allowed for.
fn is_leftover(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let numbered = name
        .rsplit_once(".old.")
        .is_some_and(|(stem, n)| !stem.is_empty() && n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()));
    safe_components(rel).is_ok()
        && (numbered
            || name.strip_suffix(".old").is_some_and(|s| !s.is_empty())
            || name.strip_suffix(STAGED).is_some_and(|s| !s.is_empty()))
}

// ----- Checks ------------------------------------------------------------------------------------------------

/// The folder of the running program.
pub fn install_dir() -> Result<PathBuf, UpdateError> {
    let exe = std::env::current_exe().map_err(|e| UpdateError::io(Path::new("."), e))?;
    exe.parent().map(Path::to_path_buf).ok_or_else(|| UpdateError::NotPackaged(exe.clone()))
}

/// Checks that `dir` is an installation from a release package and that this user can write to it.
pub fn check_dir(dir: &Path) -> Result<(), UpdateError> {
    if !dir.join(MARKER).is_file() {
        return Err(UpdateError::NotPackaged(dir.to_path_buf()));
    }
    let probe = dir.join(format!(".kanavos-write-test-{}", std::process::id()));
    let _ = fs::remove_file(&probe);
    let created =
        OpenOptions::new().write(true).create_new(true).open(&probe).and_then(|mut f| f.write_all(b"k"));
    let _ = fs::remove_file(&probe);
    created.map_err(|_| UpdateError::NotWritable(dir.to_path_buf()))
}

// ----- Planning and staging ----------------------------------------------------------------------------------

/// Whether a file of the package is installed: project files only when it is the bundled sample, and never
/// anything named like an update leftover or the journal.
fn wanted(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if name.to_ascii_lowercase().ends_with(".kanavos") {
        return rel == SAMPLE;
    }
    !(is_leftover(rel) || name.starts_with(".kanavos-update") || name.starts_with(".kanavos-write-test"))
}

/// The files of an extracted package, relative and `/`-separated, sorted.
fn package_files(root: &Path) -> Result<Vec<String>, UpdateError> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), UpdateError> {
        let entries = fs::read_dir(dir).map_err(|e| UpdateError::io(dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| UpdateError::io(dir, e))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| UpdateError::BadPackage("a file name is not UTF-8".into()))?;
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            let kind = entry.file_type().map_err(|e| UpdateError::io(&entry.path(), e))?;
            if kind.is_dir() {
                walk(&entry.path(), &rel, out)?;
            } else if kind.is_file() && wanted(&rel) {
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, "", &mut out)?;
    out.sort();
    Ok(out)
}

/// A free name for the backup of `rel`: `rel.old`, deleting an older leftover of that name when possible, else
/// `rel.old.1` … `rel.old.9`.
fn backup_name(dir: &Path, rel: &str) -> Result<String, UpdateError> {
    for n in 0..10 {
        let name = if n == 0 { format!("{rel}.old") } else { format!("{rel}.old.{n}") };
        let path = under(dir, &name).ok_or_else(|| UpdateError::UnsafeEntry(name.clone()))?;
        if !present(&path) || remove_file(&path).is_ok() {
            return Ok(name);
        }
    }
    Err(UpdateError::io(&dir.join(format!("{rel}.old")), io::Error::other("no free name for the backup")))
}

fn target(dir: &Path, rel: &str) -> Result<PathBuf, UpdateError> {
    under(dir, rel).ok_or_else(|| UpdateError::UnsafeEntry(rel.to_owned()))
}

fn stage(root: &Path, dir: &Path, entries: &[Entry]) -> Result<(), UpdateError> {
    for e in entries {
        let src = target(root, &e.path)?;
        let dst = with_suffix(&target(dir, &e.path)?, STAGED);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|err| UpdateError::io(parent, err))?;
        }
        remove_file(&dst).map_err(|err| UpdateError::io(&dst, err))?;
        fs::copy(&src, &dst).map_err(|err| UpdateError::io(&dst, err))?;
        OpenOptions::new()
            .write(true)
            .open(&dst)
            .and_then(|f| f.sync_all())
            .map_err(|err| UpdateError::io(&dst, err))?;
    }
    Ok(())
}

fn unstage(dir: &Path, entries: &[Entry]) {
    for e in entries {
        if let Some(t) = under(dir, &e.path) {
            let _ = remove_file(&with_suffix(&t, STAGED));
        }
    }
}

// ----- Swapping, rollback and completion ---------------------------------------------------------------------

/// Moves the staged files into place. On failure, returns the error and the index of the entry being swapped.
fn swap(dir: &Path, entries: &[Entry]) -> Result<(), (UpdateError, usize)> {
    for (i, e) in entries.iter().enumerate() {
        let dest = target(dir, &e.path).map_err(|err| (err, i))?;
        let staged = with_suffix(&dest, STAGED);
        if let Some(b) = &e.backup {
            let backup = target(dir, b).map_err(|err| (err, i))?;
            match rename(&dest, &backup) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err((UpdateError::io(&dest, err), i)),
            }
        }
        if let Err(err) = rename(&staged, &dest) {
            return Err((UpdateError::io(&dest, err), i));
        }
    }
    Ok(())
}

/// Puts the previous files back for entries `0..=last`, moving each new file back to its staged name. Returns
/// whether everything was restored. Because every new file returns to `<file>.new`, a rollback that stops half
/// way still leaves a state the completion step can finish (every entry is either staged or in place).
fn rollback(dir: &Path, entries: &[Entry], last: usize) -> bool {
    let mut restored = true;
    for e in entries.iter().take(last.saturating_add(1)).rev() {
        let Some(dest) = under(dir, &e.path) else {
            restored = false;
            continue;
        };
        let staged = with_suffix(&dest, STAGED);
        let backup = e.backup.as_deref().and_then(|b| under(dir, b));
        // The new file is in place when its staged copy is gone.
        if !present(&staged) && present(&dest) && rename(&dest, &staged).is_err() {
            restored = false;
            continue;
        }
        // The backup exists only if the previous file was moved there.
        if let Some(backup) = backup
            && present(&backup)
            && rename(&backup, &dest).is_err()
        {
            restored = false;
        }
    }
    restored
}

/// Completes an interrupted swap from the staged files.
fn roll_forward(dir: &Path, entries: &[Entry]) -> Result<(), UpdateError> {
    for e in entries {
        let dest = target(dir, &e.path)?;
        let staged = with_suffix(&dest, STAGED);
        if !present(&staged) {
            continue;
        }
        if let Some(b) = &e.backup {
            let backup = target(dir, b)?;
            if present(&dest) && !present(&backup) {
                rename(&dest, &backup).map_err(|err| UpdateError::io(&dest, err))?;
            }
        }
        rename(&staged, &dest).map_err(|err| UpdateError::io(&dest, err))?;
    }
    Ok(())
}

/// Brings the folder to a settled state (lock held): completes or abandons an interrupted installation and
/// deletes the leftovers that can be deleted.
fn settle(dir: &Path, report: &mut Cleanup) -> Result<(), UpdateError> {
    let mut journal = match Journal::read(dir) {
        Ok(Some(j)) => j,
        Ok(None) => return Ok(()),
        Err(_) => {
            // A damaged journal cannot be acted on safely; leftovers stay where they are.
            let _ = remove_file(&dir.join(JOURNAL));
            return Ok(());
        }
    };
    match journal.state {
        State::Staging => {
            unstage(dir, &journal.files);
            journal.files.clear();
        }
        State::Swapping => {
            roll_forward(dir, &journal.files)?;
            report.completed = true;
            journal.state = State::Installed;
            journal.write(dir)?;
        }
        State::Installed => {}
    }
    let leftovers =
        journal.files.iter().filter_map(|e| e.backup.clone()).chain(journal.trash.iter().cloned());
    let mut pending = Vec::new();
    for rel in leftovers {
        let Some(path) = under(dir, &rel).filter(|_| is_leftover(&rel)) else { continue };
        if !present(&path) {
            continue;
        }
        match remove_file(&path) {
            Ok(()) => report.removed += 1,
            Err(_) => pending.push(rel),
        }
    }
    report.pending = pending.len();
    Journal::finish(dir, journal.version, pending);
    Ok(())
}

// ----- Public operations -------------------------------------------------------------------------------------

/// Deletes the leftovers of an earlier update in the installation folder `dir` (the `.old` files recorded in the
/// journal) and completes an installation that was interrupted. Does nothing when another process is installing.
pub fn cleanup_old_in(dir: &Path) -> Result<Cleanup, UpdateError> {
    let mut report = Cleanup::default();
    if !present(&dir.join(JOURNAL)) {
        return Ok(report);
    }
    let _lock = match Lock::acquire(dir) {
        Ok(lock) => lock,
        Err(UpdateError::Busy) => {
            report.busy = true;
            return Ok(report);
        }
        Err(e) => return Err(e),
    };
    settle(dir, &mut report)?;
    Ok(report)
}

/// Installs a downloaded and verified package into the installation folder `dir`.
///
/// Fails without changing anything when `dir` is not a packaged installation, cannot be written, is being
/// updated by another process, or the package is unsafe or incomplete. When replacing files fails, the previous
/// files are restored ([`UpdateError::InstallFailed`]).
pub fn install_into(d: &Downloaded, dir: &Path) -> Result<Installed, UpdateError> {
    check_dir(dir)?;
    let _lock = Lock::acquire(dir)?;
    settle(dir, &mut Cleanup::default())?;
    let trash = Journal::read(dir).ok().flatten().map(|j| j.trash).unwrap_or_default();

    let extracted = d.workdir.join("pkg");
    if present(&extracted) {
        fs::remove_dir_all(&extracted).map_err(|e| UpdateError::io(&extracted, e))?;
    }
    let root = archive::extract(&d.archive, &extracted, &d.available.package)?;
    let files = package_files(&root)?;
    for required in [app_file(), cli_file(), MARKER.to_owned()] {
        if !files.contains(&required) {
            return Err(UpdateError::BadPackage(format!("{required} is missing")));
        }
    }
    let mut entries = Vec::with_capacity(files.len());
    for f in files {
        let backup = if present(&target(dir, &f)?) { Some(backup_name(dir, &f)?) } else { None };
        entries.push(Entry { path: f, backup });
    }
    let version = d.available.version.to_string();
    let mut journal =
        Journal { format: 1, state: State::Staging, version: version.clone(), files: entries, trash };
    journal.write(dir)?;
    let staged = stage(&root, dir, &journal.files).and_then(|()| {
        journal.state = State::Swapping;
        journal.write(dir)
    });
    if let Err(e) = staged {
        unstage(dir, &journal.files);
        Journal::finish(dir, version, journal.trash);
        let _ = fs::remove_dir_all(&extracted);
        return Err(e);
    }
    if let Err((e, at)) = swap(dir, &journal.files) {
        let restored = rollback(dir, &journal.files, at);
        if restored {
            unstage(dir, &journal.files);
            Journal::finish(dir, version, journal.trash);
        }
        // Otherwise the journal stays in the swapping state and the next start completes the installation.
        let _ = fs::remove_dir_all(&extracted);
        return Err(UpdateError::InstallFailed { cause: Box::new(e), restored });
    }
    journal.state = State::Installed;
    let _ = journal.write(dir);
    let count = journal.files.len();
    // Delete what can be deleted now; programs still running keep their backups until a later start.
    let _ = settle(dir, &mut Cleanup::default());
    let _ = fs::remove_dir_all(&extracted);
    Ok(Installed {
        version: d.available.version.clone(),
        dir: dir.to_path_buf(),
        app: dir.join(app_file()),
        cli: dir.join(cli_file()),
        files: count,
    })
}

#[cfg(test)]
pub(crate) mod fault {
    //! Test-only fault injection: make renames of given files fail, as a locked file would on Windows.

    use std::cell::RefCell;
    use std::path::Path;

    thread_local! {
        static FAIL_RENAMES_OF: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Renames of files with these names fail on this thread (empty to stop).
    pub fn fail_renames_of(names: &[&str]) {
        FAIL_RENAMES_OF.with(|f| *f.borrow_mut() = names.iter().map(|n| (*n).to_owned()).collect());
    }

    pub(super) fn rename_fails(from: &Path) -> bool {
        let Some(name) = from.file_name().and_then(|n| n.to_str()) else { return false };
        FAIL_RENAMES_OF.with(|f| f.borrow().iter().any(|n| n == name))
    }
}
