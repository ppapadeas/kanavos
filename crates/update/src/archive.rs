//! Safe extraction of release packages (`.zip` and `.tar.gz`).
//!
//! Every entry name is checked before anything is written: absolute names, drive or stream names (`C:`, `a:b`),
//! backslashes, `..` components, empty components, Windows device names and names ending in a dot or space are
//! rejected, and so are symbolic links, hard links and special files. Every entry must lie inside the expected
//! top-level folder. Files are created with `create_new` in a fresh folder, so an archive cannot overwrite what it
//! wrote earlier. Sizes and entry counts are bounded so that a damaged or hostile archive cannot fill the disk.
//!
//! Any rejected entry rejects the whole archive.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::UpdateError;

/// Most entries accepted in one package.
pub const MAX_ENTRIES: usize = 20_000;
/// Largest single file accepted, in bytes.
pub const MAX_FILE_BYTES: u64 = 1 << 30;
/// Largest total of all files accepted, in bytes.
pub const MAX_TOTAL_BYTES: u64 = 2 << 30;

/// Archive formats of release packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// ZIP (Windows packages).
    Zip,
    /// Gzip-compressed tar (Linux and other packages).
    TarGz,
}

impl Kind {
    /// The format of a file, from its name.
    pub fn of(name: &str) -> Option<Kind> {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".zip") {
            Some(Kind::Zip)
        } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Some(Kind::TarGz)
        } else {
            None
        }
    }
}

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn safe_component(c: &str) -> bool {
    let stem = c.split('.').next().unwrap_or(c).to_ascii_uppercase();
    !c.is_empty()
        && c != ".."
        && !c.ends_with('.')
        && !c.ends_with(' ')
        && !c.chars().any(|ch| matches!(ch, ':' | '\\' | '\0') || ch.is_control())
        && !RESERVED.contains(&stem.as_str())
}

/// Splits an archive entry name into safe relative components, or rejects it.
///
/// A trailing `/` (folder entries) and `.` components are allowed and dropped.
pub fn safe_components(name: &str) -> Result<Vec<String>, UpdateError> {
    let unsafe_entry = || UpdateError::UnsafeEntry(name.to_owned());
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        return Err(unsafe_entry());
    }
    let body = name.strip_suffix('/').unwrap_or(name);
    let mut out = Vec::new();
    for c in body.split('/') {
        if c == "." {
            continue;
        }
        if !safe_component(c) {
            return Err(unsafe_entry());
        }
        out.push(c.to_owned());
    }
    if out.is_empty() {
        return Err(unsafe_entry());
    }
    Ok(out)
}

/// What an archive entry is.
enum EntryKind {
    File,
    Folder,
    Skip,
}

struct Writer<'a> {
    dest: &'a Path,
    root: &'a str,
    entries: usize,
    total: u64,
}

impl Writer<'_> {
    /// The path of an entry inside `dest`, after checking its name and that it lies under the root folder.
    fn target(&mut self, name: &str) -> Result<PathBuf, UpdateError> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(UpdateError::BadPackage(format!("more than {MAX_ENTRIES} entries")));
        }
        let parts = safe_components(name)?;
        if parts.first().map(String::as_str) != Some(self.root) {
            return Err(UpdateError::BadPackage(format!("'{name}' is outside the folder '{}'", self.root)));
        }
        Ok(parts.iter().fold(self.dest.to_path_buf(), |p, c| p.join(c)))
    }

    fn folder(&mut self, name: &str) -> Result<(), UpdateError> {
        let path = self.target(name)?;
        fs::create_dir_all(&path).map_err(|e| UpdateError::io(&path, e))
    }

    fn file(
        &mut self,
        name: &str,
        declared: u64,
        executable: bool,
        data: &mut dyn Read,
    ) -> Result<(), UpdateError> {
        let path = self.target(name)?;
        if declared > MAX_FILE_BYTES || self.total.saturating_add(declared) > MAX_TOTAL_BYTES {
            return Err(UpdateError::TooLarge { name: name.to_owned(), limit: MAX_FILE_BYTES });
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| UpdateError::io(parent, e))?;
        }
        let mut out = OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists {
                UpdateError::BadPackage(format!("'{name}' appears twice"))
            } else {
                UpdateError::io(&path, e)
            }
        })?;
        let written = io::copy(&mut data.take(MAX_FILE_BYTES + 1), &mut out)
            .map_err(|e| UpdateError::BadPackage(format!("{name}: {e}")))?;
        if written > MAX_FILE_BYTES {
            return Err(UpdateError::TooLarge { name: name.to_owned(), limit: MAX_FILE_BYTES });
        }
        self.total = self.total.saturating_add(written);
        if self.total > MAX_TOTAL_BYTES {
            return Err(UpdateError::TooLarge { name: name.to_owned(), limit: MAX_TOTAL_BYTES });
        }
        out.flush().map_err(|e| UpdateError::io(&path, e))?;
        drop(out);
        set_mode(&path, executable)
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, executable: bool) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|e| UpdateError::io(path, e))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _executable: bool) -> Result<(), UpdateError> {
    Ok(())
}

/// Whether a file of the package must be executable: an execute bit in the archive, or one of the programs.
fn executable(name: &str, mode: Option<u32>) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    mode.is_some_and(|m| m & 0o111 != 0) || matches!(file, "kanavos" | "kanavos-cli")
}

fn extract_zip(archive: &Path, w: &mut Writer<'_>) -> Result<(), UpdateError> {
    let file = File::open(archive).map_err(|e| UpdateError::io(archive, e))?;
    let mut zip =
        zip::ZipArchive::new(io::BufReader::new(file)).map_err(|e| UpdateError::BadPackage(e.to_string()))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| UpdateError::BadPackage(e.to_string()))?;
        let name = entry.name().map_err(|e| UpdateError::BadPackage(e.to_string()))?.into_owned();
        let kind = if entry.is_symlink() {
            return Err(UpdateError::UnsafeEntry(name));
        } else if entry.is_dir() {
            EntryKind::Folder
        } else if entry.is_file() {
            EntryKind::File
        } else {
            return Err(UpdateError::UnsafeEntry(name));
        };
        match kind {
            EntryKind::Folder => w.folder(&name)?,
            EntryKind::File => {
                let declared = entry.size();
                let exec = executable(&name, entry.unix_mode());
                w.file(&name, declared, exec, &mut entry)?;
            }
            EntryKind::Skip => {}
        }
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, w: &mut Writer<'_>) -> Result<(), UpdateError> {
    use tar::EntryType;
    let file = File::open(archive).map_err(|e| UpdateError::io(archive, e))?;
    let gz = flate2::read::GzDecoder::new(io::BufReader::new(file));
    let mut tar = tar::Archive::new(gz);
    let entries = tar.entries().map_err(|e| UpdateError::BadPackage(e.to_string()))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| UpdateError::BadPackage(e.to_string()))?;
        let name = String::from_utf8(entry.path_bytes().into_owned())
            .map_err(|_| UpdateError::BadPackage("an entry name is not UTF-8".into()))?;
        let kind = match entry.header().entry_type() {
            EntryType::Regular | EntryType::Continuous => EntryKind::File,
            EntryType::Directory => EntryKind::Folder,
            EntryType::XGlobalHeader
            | EntryType::XHeader
            | EntryType::GNULongName
            | EntryType::GNULongLink => EntryKind::Skip,
            _ => return Err(UpdateError::UnsafeEntry(name)),
        };
        match kind {
            EntryKind::Folder => w.folder(&name)?,
            EntryKind::File => {
                let declared = entry.size();
                let exec = executable(&name, entry.header().mode().ok());
                w.file(&name, declared, exec, &mut entry)?;
            }
            EntryKind::Skip => {}
        }
    }
    Ok(())
}

/// Extracts `archive` into the folder `dest`, which must not exist yet, and returns the package folder
/// `dest/<root>`. Every entry must lie under `root`. On any error the partly extracted folder is removed.
pub fn extract(archive: &Path, dest: &Path, root: &str) -> Result<PathBuf, UpdateError> {
    let kind = archive
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(Kind::of)
        .ok_or_else(|| UpdateError::BadPackage(format!("unknown archive type: {}", archive.display())))?;
    if !safe_components(root).is_ok_and(|c| c.len() == 1) {
        return Err(UpdateError::BadPackage(format!("bad package folder name '{root}'")));
    }
    fs::create_dir(dest).map_err(|e| UpdateError::io(dest, e))?;
    let mut w = Writer { dest, root, entries: 0, total: 0 };
    let result = match kind {
        Kind::Zip => extract_zip(archive, &mut w),
        Kind::TarGz => extract_tar_gz(archive, &mut w),
    };
    let package = dest.join(root);
    let result = result.and_then(|()| {
        if package.is_dir() {
            Ok(package)
        } else {
            Err(UpdateError::BadPackage(format!("the folder '{root}' is missing")))
        }
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(dest);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_are_checked() {
        assert_eq!(safe_components("a/b.txt").unwrap(), vec!["a", "b.txt"]);
        assert_eq!(safe_components("./a/").unwrap(), vec!["a"]);
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a\\..\\x",
            "C:/x",
            "a/b:stream",
            "a//b",
            "a/NUL.txt",
            "a/con",
            "a/b.",
            "a/b ",
            "a/\u{0}b",
            ".",
        ] {
            assert!(matches!(safe_components(bad), Err(UpdateError::UnsafeEntry(_))), "{bad:?}");
        }
    }

    #[test]
    fn archive_kinds_follow_the_name() {
        assert_eq!(Kind::of("kanavos-1.0.0-windows-x86_64.zip"), Some(Kind::Zip));
        assert_eq!(Kind::of("kanavos-1.0.0-linux-x86_64.tar.gz"), Some(Kind::TarGz));
        assert_eq!(Kind::of("kanavos.exe"), None);
    }
}
