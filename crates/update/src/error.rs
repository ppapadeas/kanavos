//! Typed update errors with Greek and English messages from the catalogue (`update.tsv`).

use std::fmt;
use std::path::{Path, PathBuf};

use kanavos_i18n::{Locale, t, tf};

/// Why checking, downloading or installing an update failed.
#[derive(Debug)]
pub enum UpdateError {
    /// Update checks are turned off with the `KANAVOS_NO_UPDATE_CHECK` environment variable.
    Disabled,
    /// This build carries no release public key, so it cannot trust any package.
    NotConfigured,
    /// The update server could not be reached (offline, name lookup, refused connection, timeout, TLS).
    Network(String),
    /// The server limits requests from this network; `minutes` until it accepts more, when it said.
    RateLimited {
        /// Minutes to wait, when known.
        minutes: Option<u64>,
    },
    /// The server answered with an unexpected HTTP status.
    Http {
        /// HTTP status code.
        status: u16,
        /// The address asked for.
        url: String,
    },
    /// The list of releases could not be read.
    BadResponse(String),
    /// The download was cancelled.
    Cancelled,
    /// A download was larger than announced or than allowed.
    TooLarge {
        /// File name.
        name: String,
        /// Largest accepted size in bytes.
        limit: u64,
    },
    /// The release has no checksum file (`.sha256`) for the package.
    ChecksumMissing,
    /// The checksum file cannot be read or names another file.
    BadChecksumFile,
    /// The package does not match its checksum.
    ChecksumMismatch,
    /// The release has no signature file (`.sig`) for the package.
    SignatureMissing,
    /// The signature file cannot be read.
    BadSignatureFile,
    /// The signature does not verify with the release public key.
    BadSignature,
    /// The archive holds an entry that could land outside the package folder, or is not a plain file or folder.
    UnsafeEntry(String),
    /// The archive is damaged or does not have the expected layout.
    BadPackage(String),
    /// The running program was not installed from a release package (its folder has no `QUICKSTART.txt`).
    NotPackaged(PathBuf),
    /// The installation folder cannot be written by this user.
    NotWritable(PathBuf),
    /// Another update is being installed into the same folder.
    Busy,
    /// A file operation failed.
    Io {
        /// The file or folder.
        path: PathBuf,
        /// What went wrong.
        error: std::io::Error,
    },
    /// Replacing the files failed; `restored` tells whether every previous file was put back. When it is false,
    /// the staged files are kept and the next start completes the installation.
    InstallFailed {
        /// What went wrong.
        cause: Box<UpdateError>,
        /// Every file of the previous version is back in place.
        restored: bool,
    },
    /// The new version is installed but could not be started.
    Restart(String),
    /// The worker doing the update stopped without reporting (it should not happen; try again).
    Stopped,
}

impl UpdateError {
    /// An I/O error on a path.
    pub(crate) fn io(path: &Path, error: std::io::Error) -> UpdateError {
        UpdateError::Io { path: path.to_path_buf(), error }
    }

    /// The catalogue key of the message.
    pub fn key(&self) -> &'static str {
        match self {
            UpdateError::Disabled => "update.err.disabled",
            UpdateError::NotConfigured => "update.err.not_configured",
            UpdateError::Network(_) => "update.err.network",
            UpdateError::RateLimited { minutes: Some(_) } => "update.err.rate_limited_minutes",
            UpdateError::RateLimited { minutes: None } => "update.err.rate_limited",
            UpdateError::Http { .. } => "update.err.http",
            UpdateError::BadResponse(_) => "update.err.bad_response",
            UpdateError::Cancelled => "update.err.cancelled",
            UpdateError::TooLarge { .. } => "update.err.too_large",
            UpdateError::ChecksumMissing => "update.err.checksum_missing",
            UpdateError::BadChecksumFile => "update.err.bad_checksum_file",
            UpdateError::ChecksumMismatch => "update.err.checksum_mismatch",
            UpdateError::SignatureMissing => "update.err.signature_missing",
            UpdateError::BadSignatureFile => "update.err.bad_signature_file",
            UpdateError::BadSignature => "update.err.bad_signature",
            UpdateError::UnsafeEntry(_) => "update.err.unsafe_entry",
            UpdateError::BadPackage(_) => "update.err.bad_package",
            UpdateError::NotPackaged(_) => "update.err.not_packaged",
            UpdateError::NotWritable(_) => "update.err.not_writable",
            UpdateError::Busy => "update.err.busy",
            UpdateError::Io { .. } => "update.err.io",
            UpdateError::InstallFailed { restored: true, .. } => "update.err.install_failed_restored",
            UpdateError::InstallFailed { restored: false, .. } => "update.err.install_failed",
            UpdateError::Restart(_) => "update.err.restart",
            UpdateError::Stopped => "update.err.stopped",
        }
    }

    /// The message for people, in a language.
    pub fn message(&self, locale: Locale) -> String {
        let key = self.key();
        match self {
            UpdateError::Network(why) | UpdateError::BadResponse(why) | UpdateError::BadPackage(why) => {
                tf(locale, key, &[("why", why)])
            }
            UpdateError::Restart(why) => tf(locale, key, &[("why", why)]),
            UpdateError::InstallFailed { cause, .. } => tf(locale, key, &[("why", &cause.message(locale))]),
            UpdateError::RateLimited { minutes: Some(m) } => tf(locale, key, &[("n", &m.to_string())]),
            UpdateError::Http { status, url } => {
                tf(locale, key, &[("status", &status.to_string()), ("url", url)])
            }
            UpdateError::TooLarge { name, limit } => {
                tf(locale, key, &[("name", name), ("limit", &limit.to_string())])
            }
            UpdateError::UnsafeEntry(entry) => tf(locale, key, &[("entry", entry)]),
            UpdateError::NotPackaged(dir) | UpdateError::NotWritable(dir) => {
                tf(locale, key, &[("dir", &dir.display().to_string())])
            }
            UpdateError::Io { path, error } => {
                tf(locale, key, &[("path", &path.display().to_string()), ("why", &error.to_string())])
            }
            _ => t(locale, key).into_owned(),
        }
    }

    /// Whether the error means updates cannot work in this build or environment (as opposed to a failure that
    /// may go away when tried again).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, UpdateError::Disabled | UpdateError::NotConfigured)
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Locale::En))
    }
}

impl std::error::Error for UpdateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UpdateError::Io { error, .. } => Some(error),
            UpdateError::InstallFailed { cause, .. } => Some(cause.as_ref()),
            _ => None,
        }
    }
}
