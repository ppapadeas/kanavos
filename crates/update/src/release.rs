//! The release list of the repository (GitHub REST API) and the choice of the release to offer.
//!
//! Only the fields Kanavos needs are read; unknown fields are ignored, and a release whose entry cannot be read is
//! skipped rather than failing the whole check.

use std::cmp::Ordering;

use semver::Version;
use serde::Deserialize;

use crate::{Channel, Config, UpdateError};

/// One release as listed by the API.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    /// Git tag, `vX.Y.Z` or `vX.Y.Z-pre.N`.
    #[serde(default)]
    pub tag_name: String,
    /// Release title.
    #[serde(default)]
    pub name: Option<String>,
    /// Not published yet.
    #[serde(default)]
    pub draft: bool,
    /// Marked as a pre-release (test build).
    #[serde(default)]
    pub prerelease: bool,
    /// Publication time (RFC 3339).
    #[serde(default)]
    pub published_at: Option<String>,
    /// Release notes as written on the release page.
    #[serde(default)]
    pub body: Option<String>,
    /// The release page.
    #[serde(default)]
    pub html_url: String,
    /// Files attached to the release.
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

/// One file attached to a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReleaseAsset {
    /// File name.
    pub name: String,
    /// Size in bytes.
    #[serde(default)]
    pub size: u64,
    /// Download address.
    pub browser_download_url: String,
    /// `uploaded` once the file is complete.
    #[serde(default)]
    pub state: Option<String>,
}

/// A file to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// File name.
    pub name: String,
    /// Download address (HTTPS, on github.com).
    pub url: String,
    /// Size in bytes as announced by the server (0 when unknown).
    pub size: u64,
}

/// A newer release that can be installed on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    /// Its version.
    pub version: Version,
    /// Its tag.
    pub tag: String,
    /// Release title (the tag when there is none).
    pub title: String,
    /// Publication time (RFC 3339), when known.
    pub published_at: Option<String>,
    /// Release notes, as plain text.
    pub notes: String,
    /// A test build.
    pub prerelease: bool,
    /// The release page.
    pub page: String,
    /// The package for this operating system and architecture.
    pub archive: Asset,
    /// Its checksum file (`<archive>.sha256`).
    pub checksum: Asset,
    /// Its signature file (`<archive>.sig`).
    pub signature: Asset,
    /// The top-level folder inside the package, `kanavos-<version>-<os>-<arch>`.
    pub package: String,
}

impl Available {
    /// Publication date as `YYYY-MM-DD`, when known.
    pub fn date(&self) -> Option<&str> {
        self.published_at.as_deref().and_then(|s| s.get(..10))
    }
}

/// Parses a release tag: `v` followed by a semantic version (`v1.2.3`, `v1.2.3-pre.4`).
pub fn parse_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.trim().strip_prefix('v')?).ok()
}

/// The package base name for a version string (the tag without `v`), operating system and architecture.
pub fn package_name(version: &str, os: &str, arch: &str) -> String {
    format!("kanavos-{version}-{os}-{arch}")
}

/// The archive extension used for an operating system: `zip` on Windows, `tar.gz` elsewhere.
pub fn archive_extension(os: &str) -> &'static str {
    if os == "windows" { "zip" } else { "tar.gz" }
}

/// Reads the JSON array returned by `GET /repos/{owner}/{repo}/releases`.
pub fn parse_releases(json: &str) -> Result<Vec<Release>, UpdateError> {
    let values: Vec<serde_json::Value> =
        serde_json::from_str(json).map_err(|e| UpdateError::BadResponse(e.to_string()))?;
    Ok(values.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect())
}

/// A download address must be an HTTPS release download on github.com for that very file.
fn trusted_url(url: &str, name: &str) -> bool {
    url.starts_with("https://github.com/")
        && url.contains("/releases/download/")
        && url.rsplit('/').next() == Some(name)
}

fn asset(release: &Release, name: &str) -> Option<Asset> {
    release
        .assets
        .iter()
        .find(|a| a.name == name && a.state.as_deref().is_none_or(|s| s == "uploaded"))
        .filter(|a| trusted_url(&a.browser_download_url, name))
        .map(|a| Asset { name: a.name.clone(), url: a.browser_download_url.clone(), size: a.size })
}

/// The release this computer can install, if `release` qualifies: published, on the channel, newer than the
/// running version, with a package, checksum and signature for this operating system and architecture.
pub fn candidate(release: &Release, config: &Config) -> Option<Available> {
    if release.draft {
        return None;
    }
    let version = parse_tag(&release.tag_name)?;
    let stable = !release.prerelease && version.pre.is_empty();
    if config.channel == Channel::Stable && !stable {
        return None;
    }
    // Precedence as defined by semver.org §11 (build metadata does not count).
    if version.cmp_precedence(&config.current) != Ordering::Greater {
        return None;
    }
    let version_text = release.tag_name.trim().strip_prefix('v')?;
    let package = package_name(version_text, &config.os, &config.arch);
    let archive_name = format!("{package}.{}", archive_extension(&config.os));
    let archive = asset(release, &archive_name)?;
    let checksum = asset(release, &format!("{archive_name}.sha256"))?;
    let signature = asset(release, &format!("{archive_name}.sig"))?;
    let title = release
        .name
        .as_deref()
        .map(|n| plain_text(n, false))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| release.tag_name.clone());
    Some(Available {
        version,
        tag: release.tag_name.clone(),
        title,
        published_at: release.published_at.clone(),
        notes: plain_text(release.body.as_deref().unwrap_or_default(), true),
        prerelease: !stable,
        page: release.html_url.clone(),
        archive,
        checksum,
        signature,
        package,
    })
}

/// The newest installable release, if any (see [`candidate`]).
pub fn select(releases: &[Release], config: &Config) -> Option<Available> {
    releases.iter().filter_map(|r| candidate(r, config)).max_by(|a, b| a.version.cmp_precedence(&b.version))
}

/// Release text as plain text: `\r\n` becomes `\n`, and control characters (terminal escape sequences among
/// them) are dropped, keeping line breaks and tabs when `multiline`.
pub fn plain_text(text: &str, multiline: bool) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
        .collect()
}

/// Whether `a` is newer than `b` by semantic-version precedence (semver.org §11).
pub fn is_newer(a: &Version, b: &Version) -> bool {
    a.cmp_precedence(b) == Ordering::Greater
}
