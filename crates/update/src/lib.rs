//! Over-the-air updates: find a newer release on GitHub, download its package, verify it, install it over the
//! running installation and restart.
//!
//! Layer L8. See `docs/architecture.md`, `docs/updates.md` and `specs/updates/over-the-air.md`.
//!
//! The steps, each a function the desktop application and the command-line tool call in turn:
//!
//! 1. [`check`] asks the GitHub REST API for the latest releases of [`REPOSITORY`] and picks the newest one
//!    above the running version on the chosen [`Channel`] that has a package for this operating system and
//!    architecture ([`release::select`]). The request carries only the version, operating system and
//!    architecture in its `User-Agent`.
//! 2. [`download`] fetches the package with its `.sha256` and `.sig` files into a temporary folder and accepts
//!    it only when the SHA-256 matches and the Ed25519 signature verifies with [`UPDATE_PUBLIC_KEY`]
//!    ([`verify::verify`]).
//! 3. [`install`] unpacks it safely ([`archive::extract`]) and replaces the files of the installation next to
//!    the running program, with a journal and rollback ([`install`](mod@install) module).
//! 4. [`restart`] starts the new version; [`cleanup_old`] removes the previous files on the next start.
//!
//! Nothing here runs by itself: callers decide when to check, and `KANAVOS_NO_UPDATE_CHECK=1` turns every
//! network operation off. Builds whose [`UPDATE_PUBLIC_KEY`] is the placeholder refuse to check or install
//! ([`UpdateError::NotConfigured`]).

#![forbid(unsafe_code)]

pub mod archive;
mod error;
pub mod install;
mod net;
pub mod release;
pub mod verify;

use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use error::UpdateError;
pub use install::{Cleanup, Installed};
pub use release::{Asset, Available};
pub use semver::Version;
use serde::{Deserialize, Serialize};

/// The release public key: standard base64 of the 32-byte Ed25519 public key whose private half signs the
/// release packages. Builds with the placeholder value refuse to check for or install updates.
///
/// Created with `cargo xtask keygen`; changing it is described in `docs/releasing.md`.
pub const UPDATE_PUBLIC_KEY: &str = "4dyfPC0DrHFPxgVW7w4jSlebT07z72cUGZt1JFGxdVY=";

/// The placeholder value of [`UPDATE_PUBLIC_KEY`] in builds without a release key.
pub const PLACEHOLDER_PUBLIC_KEY: &str = "REPLACE-WITH-RELEASE-PUBLIC-KEY";

/// The GitHub repository whose releases are offered.
pub const REPOSITORY: &str = "ppapadeas/kanavos";

/// The GitHub REST API.
pub const API_BASE: &str = "https://api.github.com";

/// The version of this build (the workspace version, which equals the release tag without `v`).
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Environment variable that turns every update check and download off when set to anything but `0` or empty.
pub const NO_CHECK_ENV: &str = "KANAVOS_NO_UPDATE_CHECK";

/// Which releases are offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Published releases only.
    #[default]
    Stable,
    /// Also test builds (releases marked as pre-releases).
    Test,
}

impl Channel {
    /// Both channels.
    pub const ALL: [Channel; 2] = [Channel::Stable, Channel::Test];

    /// The name used in preferences and on the command line.
    pub fn key(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Test => "test",
        }
    }

    /// Parses `stable` or `test`.
    pub fn parse(text: &str) -> Option<Channel> {
        Channel::ALL.into_iter().find(|c| c.key() == text.trim().to_ascii_lowercase())
    }
}

/// What a check looks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `owner/name` of the GitHub repository.
    pub repository: String,
    /// Base address of the GitHub REST API.
    pub api_base: String,
    /// The running version.
    pub current: Version,
    /// Which releases to offer.
    pub channel: Channel,
    /// Operating system, as `std::env::consts::OS`.
    pub os: String,
    /// Architecture, as `std::env::consts::ARCH`.
    pub arch: String,
}

impl Config {
    /// The configuration of this build: [`REPOSITORY`], [`CURRENT_VERSION`] and this operating system and
    /// architecture.
    pub fn new(channel: Channel) -> Config {
        Config {
            repository: REPOSITORY.to_owned(),
            api_base: API_BASE.to_owned(),
            current: Version::parse(CURRENT_VERSION).unwrap_or(Version::new(0, 0, 0)),
            channel,
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }

    /// `Kanavos/<version> (<os>; <arch>)`: everything a request says about this computer.
    pub fn user_agent(&self) -> String {
        format!("Kanavos/{} ({}; {})", self.current, self.os, self.arch)
    }

    /// The address of the release list.
    pub fn releases_url(&self) -> String {
        format!("{}/repos/{}/releases?per_page=20", self.api_base.trim_end_matches('/'), self.repository)
    }
}

/// Whether `KANAVOS_NO_UPDATE_CHECK` turns updates off.
pub fn disabled_by_env() -> bool {
    std::env::var_os(NO_CHECK_ENV).is_some_and(|v| !v.is_empty() && v != "0")
}

/// The release public key of this build, or [`UpdateError::NotConfigured`] when it is the placeholder or not a
/// valid key.
pub fn public_key() -> Result<[u8; 32], UpdateError> {
    if UPDATE_PUBLIC_KEY == PLACEHOLDER_PUBLIC_KEY {
        return Err(UpdateError::NotConfigured);
    }
    verify::parse_public_key(UPDATE_PUBLIC_KEY)
        .filter(|k| k.iter().any(|b| *b != 0))
        .ok_or(UpdateError::NotConfigured)
}

/// Whether this build can install updates (it has a release public key).
pub fn is_configured() -> bool {
    public_key().is_ok()
}

/// Whether updates can run here: configured and not turned off by the environment. Errors say why not.
pub fn available_here() -> Result<(), UpdateError> {
    if disabled_by_env() {
        return Err(UpdateError::Disabled);
    }
    public_key().map(|_| ())
}

/// Asks GitHub for the releases and returns the newest one this computer can install, or `None` when the
/// running version is the newest on the channel.
///
/// Sends one HTTPS request to [`Config::releases_url`] with the `User-Agent` of [`Config::user_agent`].
pub fn check(config: &Config) -> Result<Option<Available>, UpdateError> {
    available_here()?;
    let agent = net::agent(&config.user_agent(), Duration::from_secs(60))?;
    let text = net::get_release_list(&agent, &config.releases_url())?;
    let releases = release::parse_releases(&text)?;
    Ok(release::select(&releases, config))
}

/// A downloaded and verified package in a temporary folder, deleted when this value is dropped.
#[derive(Debug)]
pub struct Downloaded {
    /// The release it belongs to.
    pub available: Available,
    /// The archive.
    pub archive: PathBuf,
    workdir: PathBuf,
}

impl Downloaded {
    /// The temporary folder holding the archive.
    pub fn workdir(&self) -> &std::path::Path {
        &self.workdir
    }
}

impl Drop for Downloaded {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workdir);
    }
}

/// Makes a new private temporary folder for a download.
fn new_workdir() -> Result<PathBuf, UpdateError> {
    let base = std::env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let mut last = None;
    for i in 0..50u32 {
        let dir = base.join(format!("kanavos-update-{}-{nanos}-{i}", std::process::id()));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(UpdateError::io(&dir, e)),
        }
    }
    Err(UpdateError::io(&base, last.unwrap_or_else(|| std::io::Error::other("no free folder name"))))
}

/// Downloads the package of `available` with its checksum and signature files and verifies both against the
/// release public key. `progress(done, total)` is called as bytes arrive (`total` is 0 when unknown); setting
/// `cancel` stops the download with [`UpdateError::Cancelled`].
pub fn download(
    available: &Available,
    mut progress: impl FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<Downloaded, UpdateError> {
    available_here()?;
    let key = public_key()?;
    let name = &available.archive.name;
    if !archive::safe_components(name).is_ok_and(|c| c.len() == 1) || archive::Kind::of(name).is_none() {
        return Err(UpdateError::BadPackage(format!("unexpected package name '{name}'")));
    }
    let user_agent = Config::new(Channel::Stable).user_agent();
    let agent = net::agent(&user_agent, Duration::from_secs(30 * 60))?;
    // The small files first, so that a release without them fails before the large download.
    let checksum =
        net::get_small_text(&agent, &available.checksum.url)?.ok_or(UpdateError::ChecksumMissing)?;
    verify::parse_checksum(&checksum, name)?;
    let signature =
        net::get_small_text(&agent, &available.signature.url)?.ok_or(UpdateError::SignatureMissing)?;
    verify::parse_signature(&signature)?;
    let workdir = new_workdir()?;
    let downloaded = Downloaded { available: available.clone(), archive: workdir.join(name), workdir };
    net::download_file(
        &agent,
        &available.archive.url,
        name,
        available.archive.size,
        &downloaded.archive,
        &mut progress,
        cancel,
    )?;
    if cancel.load(Ordering::Relaxed) {
        return Err(UpdateError::Cancelled);
    }
    let bytes = fs::read(&downloaded.archive).map_err(|e| UpdateError::io(&downloaded.archive, e))?;
    verify::verify(&bytes, name, Some(&checksum), Some(&signature), &key)?;
    Ok(downloaded)
}

/// Checks, before downloading, that the running program can update itself: it was installed from a release
/// package and its folder can be written. Returns the installation folder.
pub fn preflight() -> Result<PathBuf, UpdateError> {
    let dir = install::install_dir()?;
    install::check_dir(&dir)?;
    Ok(dir)
}

/// Installs a verified package over the installation of the running program (the folder of
/// `std::env::current_exe()`); see [`install::install_into`].
pub fn install(downloaded: &Downloaded) -> Result<Installed, UpdateError> {
    install::install_into(downloaded, &install::install_dir()?)
}

/// Deletes the files the previous version left behind (and completes an interrupted installation) in the
/// installation of the running program. Call it at start; it is quick when there is nothing to do.
pub fn cleanup_old() -> Result<Cleanup, UpdateError> {
    install::cleanup_old_in(&install::install_dir()?)
}

/// Starts the newly installed desktop program with `args` (for example the open project), detached from this
/// process. The caller then exits.
pub fn restart(installed: &Installed, args: &[OsString]) -> Result<(), UpdateError> {
    Command::new(&installed.app)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| UpdateError::Restart(e.to_string()))
}

#[cfg(test)]
mod tests;
