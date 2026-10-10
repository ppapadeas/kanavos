//! Tests of the whole update path without network: versions, release selection from a recorded release list,
//! verification, safe extraction, installation with rollback and completion, and clean-up.

use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use kanavos_i18n::Locale;
use ring::signature::{Ed25519KeyPair, KeyPair};

use super::*;
use crate::install::{
    JOURNAL, LOCK, MARKER, SAMPLE, app_file, cleanup_old_in, cli_file, fault, install_into,
};
use crate::release::{is_newer, parse_releases, parse_tag, select};
use crate::verify::{sha256_hex, verify};

const RELEASES: &str = include_str!("../tests/data/github-releases.json");

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn config(current: &str, channel: Channel, os: &str, arch: &str) -> Config {
    Config { current: v(current), channel, os: os.into(), arch: arch.into(), ..Config::new(channel) }
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kanavos-upd-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

// ----- Versions ------------------------------------------------------------------------------------------------

#[test]
fn versions_follow_semver_precedence() {
    // semver.org §11.4 example, in increasing order.
    let ordered = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
        "1.0.1",
        "1.1.0",
        "2.0.0",
    ];
    for pair in ordered.windows(2) {
        assert!(is_newer(&v(pair[1]), &v(pair[0])), "{} > {}", pair[1], pair[0]);
        assert!(!is_newer(&v(pair[0]), &v(pair[1])));
    }
    // Numeric identifiers compare as numbers, and a release is newer than its test builds.
    assert!(is_newer(&v("0.3.0-pre.10"), &v("0.3.0-pre.2")));
    assert!(is_newer(&v("0.3.0"), &v("0.3.0-pre.10")));
    // Build metadata does not count.
    assert!(!is_newer(&v("1.0.0+build.2"), &v("1.0.0+build.1")));
}

#[test]
fn tags_need_the_v_prefix_and_a_semantic_version() {
    assert_eq!(parse_tag("v1.2.3"), Some(v("1.2.3")));
    assert_eq!(parse_tag("v0.3.0-pre.4"), Some(v("0.3.0-pre.4")));
    for bad in ["1.2.3", "v1.2", "nightly", "v1.2.3.4", "", "v"] {
        assert_eq!(parse_tag(bad), None, "{bad}");
    }
}

#[test]
fn channels_parse_and_round_trip() {
    assert_eq!(Channel::parse("stable"), Some(Channel::Stable));
    assert_eq!(Channel::parse(" Test "), Some(Channel::Test));
    assert_eq!(Channel::parse("nightly"), None);
    assert_eq!(serde_json::to_string(&Channel::Test).unwrap(), "\"test\"");
    assert_eq!(serde_json::from_str::<Channel>("\"stable\"").unwrap(), Channel::Stable);
}

#[test]
fn requests_name_only_version_os_and_architecture() {
    let c = config("0.1.0", Channel::Stable, "windows", "x86_64");
    assert_eq!(c.user_agent(), "Kanavos/0.1.0 (windows; x86_64)");
    assert_eq!(c.releases_url(), "https://api.github.com/repos/ppapadeas/kanavos/releases?per_page=20");
}

// ----- Release list ------------------------------------------------------------------------------------------

#[test]
fn the_recorded_release_list_parses() {
    let releases = parse_releases(RELEASES).unwrap();
    assert_eq!(releases.len(), 9);
    let first = &releases[0];
    assert_eq!(first.tag_name, "v0.4.0");
    assert!(first.draft);
    assert_eq!(first.published_at, None);
    let r021 = releases.iter().find(|r| r.tag_name == "v0.2.1").unwrap();
    assert_eq!(r021.name.as_deref(), Some(""));
    assert_eq!(r021.assets.len(), 5);
    assert!(parse_releases("{\"message\":\"Not Found\"}").is_err());
    assert!(parse_releases("not json").is_err());
    // An entry that cannot be read is skipped, not fatal.
    assert_eq!(parse_releases("[{\"tag_name\": 5}, {\"tag_name\": \"v1.0.0\"}]").unwrap().len(), 1);
}

#[test]
fn the_newest_complete_release_on_the_channel_is_selected() {
    let releases = parse_releases(RELEASES).unwrap();
    let pick = |current: &str, channel, os: &str, arch: &str| {
        select(&releases, &config(current, channel, os, arch)).map(|a| a.version.to_string())
    };
    // 0.4.0 is a draft, 0.2.3 has an untrusted address on Windows and an incomplete upload on Linux, and 0.2.2
    // has no signature for Windows.
    assert_eq!(pick("0.1.0", Channel::Stable, "windows", "x86_64").as_deref(), Some("0.2.1"));
    assert_eq!(pick("0.1.0", Channel::Stable, "linux", "x86_64").as_deref(), Some("0.2.2"));
    // Test builds: pre.10 is newer than pre.2 (numeric), and exists only for Windows.
    assert_eq!(pick("0.1.0", Channel::Test, "windows", "x86_64").as_deref(), Some("0.3.0-pre.10"));
    assert_eq!(pick("0.1.0", Channel::Test, "linux", "x86_64").as_deref(), Some("0.3.0-pre.2"));
    assert_eq!(pick("0.3.0-pre.2", Channel::Test, "windows", "x86_64").as_deref(), Some("0.3.0-pre.10"));
    // Nothing newer.
    assert_eq!(pick("0.2.1", Channel::Stable, "windows", "x86_64"), None);
    assert_eq!(pick("0.3.0-pre.2", Channel::Stable, "windows", "x86_64"), None);
    assert_eq!(pick("0.3.0", Channel::Test, "windows", "x86_64"), None);
    // No packages for this platform.
    assert_eq!(pick("0.1.0", Channel::Test, "macos", "aarch64"), None);
    assert_eq!(pick("0.1.0", Channel::Stable, "windows", "aarch64"), None);
}

#[test]
fn the_selected_release_carries_its_files_and_notes() {
    let releases = parse_releases(RELEASES).unwrap();
    let a = select(&releases, &config("0.1.0", Channel::Stable, "windows", "x86_64")).unwrap();
    assert_eq!(a.tag, "v0.2.1");
    // An empty title falls back to the tag.
    assert_eq!(a.title, "v0.2.1");
    assert_eq!(a.date(), Some("2026-10-02"));
    assert_eq!(a.notes, "Fixes:\n- Door swing in sections\n- Greek decimal comma in schedules");
    assert!(!a.prerelease);
    assert_eq!(a.package, "kanavos-0.2.1-windows-x86_64");
    assert_eq!(a.archive.name, "kanavos-0.2.1-windows-x86_64.zip");
    assert_eq!(a.archive.size, 31_457_280);
    assert_eq!(
        a.archive.url,
        "https://github.com/ppapadeas/kanavos/releases/download/v0.2.1/kanavos-0.2.1-windows-x86_64.zip"
    );
    assert_eq!(a.checksum.name, "kanavos-0.2.1-windows-x86_64.zip.sha256");
    assert_eq!(a.signature.name, "kanavos-0.2.1-windows-x86_64.zip.sig");
    assert_eq!(a.page, "https://github.com/ppapadeas/kanavos/releases/tag/v0.2.1");
    let linux = select(&releases, &config("0.1.0", Channel::Test, "linux", "x86_64")).unwrap();
    assert!(linux.prerelease);
    assert_eq!(linux.archive.name, "kanavos-0.3.0-pre.2-linux-x86_64.tar.gz");
}

#[test]
fn release_text_is_shown_as_plain_text() {
    let notes = "Fixes:\r\n- one\t(two)\u{1b}[2J\u{7}\n- three";
    assert_eq!(crate::release::plain_text(notes, true), "Fixes:\n- one\t(two)[2J\n- three");
    assert_eq!(crate::release::plain_text("Kanavos\n0.2\u{1b}", false), "Kanavos0.2");
}

// ----- Verification --------------------------------------------------------------------------------------------

/// A key pair made from a fixed seed, for tests only.
fn test_keys(seed: u8) -> (Ed25519KeyPair, [u8; 32]) {
    let pair = Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap();
    let public: [u8; 32] = pair.public_key().as_ref().try_into().unwrap();
    (pair, public)
}

fn sign(pair: &Ed25519KeyPair, bytes: &[u8]) -> String {
    format!("{}\n", STANDARD.encode(pair.sign(bytes).as_ref()))
}

fn checksum(bytes: &[u8], name: &str) -> String {
    format!("{}  {name}\n", sha256_hex(bytes))
}

#[test]
fn a_signed_package_verifies() {
    let (pair, public) = test_keys(7);
    let archive = b"PK\x03\x04 a package".to_vec();
    let name = "kanavos-1.0.0-windows-x86_64.zip";
    verify(&archive, name, Some(&checksum(&archive, name)), Some(&sign(&pair, &archive)), &public).unwrap();
}

#[test]
fn tampered_packages_wrong_keys_and_missing_files_are_rejected() {
    let (pair, public) = test_keys(7);
    let (_, other) = test_keys(8);
    let name = "kanavos-1.0.0-linux-x86_64.tar.gz";
    let archive = b"\x1f\x8b the original package".to_vec();
    let sum = checksum(&archive, name);
    let sig = sign(&pair, &archive);
    let mut tampered = archive.clone();
    tampered[3] ^= 1;
    // Changed bytes no longer match the checksum.
    assert!(matches!(
        verify(&tampered, name, Some(&sum), Some(&sig), &public),
        Err(UpdateError::ChecksumMismatch)
    ));
    // Even with a matching checksum (an attacker can compute one), the signature fails.
    let forged = checksum(&tampered, name);
    assert!(matches!(
        verify(&tampered, name, Some(&forged), Some(&sig), &public),
        Err(UpdateError::BadSignature)
    ));
    // A key other than the release key.
    assert!(matches!(verify(&archive, name, Some(&sum), Some(&sig), &other), Err(UpdateError::BadSignature)));
    // A signature by another key.
    let (pair2, _) = test_keys(9);
    let sig2 = sign(&pair2, &archive);
    assert!(matches!(
        verify(&archive, name, Some(&sum), Some(&sig2), &public),
        Err(UpdateError::BadSignature)
    ));
    // Missing or broken files.
    assert!(matches!(verify(&archive, name, Some(&sum), None, &public), Err(UpdateError::SignatureMissing)));
    assert!(matches!(verify(&archive, name, None, Some(&sig), &public), Err(UpdateError::ChecksumMissing)));
    assert!(matches!(
        verify(&archive, name, Some(&sum), Some("c2lnbmF0dXJl\n"), &public),
        Err(UpdateError::BadSignatureFile)
    ));
    let other_file = checksum(&archive, "kanavos-0.9.0-linux-x86_64.tar.gz");
    assert!(matches!(
        verify(&archive, name, Some(&other_file), Some(&sig), &public),
        Err(UpdateError::BadChecksumFile)
    ));
}

#[test]
fn builds_without_a_release_key_refuse_updates() {
    if UPDATE_PUBLIC_KEY == PLACEHOLDER_PUBLIC_KEY {
        assert!(matches!(public_key(), Err(UpdateError::NotConfigured)));
        assert!(!is_configured());
        assert_eq!(UpdateError::NotConfigured.to_string(), "Updates are not configured in this build.");
    } else {
        assert!(public_key().is_ok(), "UPDATE_PUBLIC_KEY must be the base64 of a 32-byte Ed25519 public key");
    }
}

#[test]
fn every_error_has_both_languages() {
    let samples = [
        UpdateError::Disabled,
        UpdateError::NotConfigured,
        UpdateError::Network("x".into()),
        UpdateError::RateLimited { minutes: Some(3) },
        UpdateError::RateLimited { minutes: None },
        UpdateError::Http { status: 500, url: "u".into() },
        UpdateError::BadResponse("x".into()),
        UpdateError::Cancelled,
        UpdateError::TooLarge { name: "a".into(), limit: 1 },
        UpdateError::ChecksumMissing,
        UpdateError::BadChecksumFile,
        UpdateError::ChecksumMismatch,
        UpdateError::SignatureMissing,
        UpdateError::BadSignatureFile,
        UpdateError::BadSignature,
        UpdateError::UnsafeEntry("e".into()),
        UpdateError::BadPackage("x".into()),
        UpdateError::NotPackaged("d".into()),
        UpdateError::NotWritable("d".into()),
        UpdateError::Busy,
        UpdateError::Io { path: "p".into(), error: std::io::Error::other("x") },
        UpdateError::InstallFailed { cause: Box::new(UpdateError::Busy), restored: true },
        UpdateError::InstallFailed { cause: Box::new(UpdateError::Busy), restored: false },
        UpdateError::Restart("x".into()),
        UpdateError::Stopped,
    ];
    for e in samples {
        assert!(kanavos_i18n::has(e.key()), "{}", e.key());
        let (el, en) = (e.message(Locale::El), e.message(Locale::En));
        assert!(!el.contains('{') && !en.contains('{'), "{el} / {en}");
        // A file error is the path and the system's own message, the same in both languages.
        if !matches!(e, UpdateError::Io { .. }) {
            assert_ne!(el, en);
        }
    }
    let minutes = UpdateError::RateLimited { minutes: Some(12) }.message(Locale::En);
    assert!(minutes.contains("12 minutes"), "{minutes}");
}

// ----- Archives ------------------------------------------------------------------------------------------------

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in entries {
        if name.ends_with('/') {
            w.add_directory(*name, options).unwrap();
        } else {
            w.start_file(*name, options).unwrap();
            w.write_all(data).unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

/// A tar header with a raw name (the builder's own path checks are bypassed on purpose).
fn tar_header(name: &str, size: u64, kind: tar::EntryType) -> tar::Header {
    let mut h = tar::Header::new_gnu();
    let raw = &mut h.as_gnu_mut().unwrap().name;
    raw[..name.len()].copy_from_slice(name.as_bytes());
    h.set_size(size);
    h.set_mode(if kind == tar::EntryType::Directory { 0o755 } else { 0o644 });
    h.set_entry_type(kind);
    h.set_cksum();
    h
}

fn tar_gz_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(gz);
    for (name, data) in entries {
        let kind = if name.ends_with('/') { tar::EntryType::Directory } else { tar::EntryType::Regular };
        let h = tar_header(name, data.len() as u64, kind);
        b.append(&h, *data).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap()
}

fn tar_gz_with_link(name: &str, target: &str) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(gz);
    let mut h = tar_header(name, 0, tar::EntryType::Symlink);
    h.set_link_name(target).unwrap();
    h.set_cksum();
    b.append(&h, &[][..]).unwrap();
    b.into_inner().unwrap().finish().unwrap()
}

fn extract_bytes(dir: &Path, file: &str, bytes: &[u8], root: &str) -> Result<PathBuf, UpdateError> {
    let archive = dir.join(file);
    fs::write(&archive, bytes).unwrap();
    let dest = dir.join("out");
    let _ = fs::remove_dir_all(&dest);
    archive::extract(&archive, &dest, root)
}

#[test]
fn packages_extract_into_their_folder() {
    let dir = temp("extract-ok");
    let entries: &[(&str, &[u8])] =
        &[("pkg/", b""), ("pkg/kanavos.exe", b"app"), ("pkg/docs/guide/a.md", b"guide")];
    for (file, bytes) in [("p.zip", zip_bytes(entries)), ("p.tar.gz", tar_gz_bytes(entries))] {
        let root = extract_bytes(&dir, file, &bytes, "pkg").unwrap();
        assert_eq!(fs::read(root.join("kanavos.exe")).unwrap(), b"app");
        assert_eq!(fs::read(root.join("docs").join("guide").join("a.md")).unwrap(), b"guide");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn path_traversal_and_links_are_rejected_in_zip_and_tar() {
    let dir = temp("extract-bad");
    let bad_names =
        ["pkg/../evil.txt", "../evil.txt", "pkg/../../evil.txt", "/evil.txt", "pkg/a\\..\\..\\evil.txt"];
    for name in bad_names {
        let entries: &[(&str, &[u8])] = &[("pkg/ok.txt", b"ok"), (name, b"evil")];
        for (file, bytes) in [("p.zip", zip_bytes(entries)), ("p.tar.gz", tar_gz_bytes(entries))] {
            let r = extract_bytes(&dir, file, &bytes, "pkg");
            assert!(matches!(r, Err(UpdateError::UnsafeEntry(_))), "{file} {name}: {r:?}");
            assert!(!dir.join("out").exists(), "partial output is removed");
            assert!(!dir.join("evil.txt").exists() && !dir.parent().unwrap().join("evil.txt").exists());
        }
    }
    // Entries outside the expected top-level folder.
    let entries: &[(&str, &[u8])] = &[("pkg/ok.txt", b"ok"), ("other/x.txt", b"x")];
    let r = extract_bytes(&dir, "p.zip", &zip_bytes(entries), "pkg");
    assert!(matches!(r, Err(UpdateError::BadPackage(_))), "{r:?}");
    // The wrong top-level folder (for example an older version's package under a newer name).
    let entries: &[(&str, &[u8])] = &[("kanavos-0.1.0-linux-x86_64/kanavos", b"old")];
    let r = extract_bytes(&dir, "p.tar.gz", &tar_gz_bytes(entries), "kanavos-0.2.0-linux-x86_64");
    assert!(matches!(r, Err(UpdateError::BadPackage(_))), "{r:?}");
    // Symbolic links.
    let r = extract_bytes(&dir, "p.tar.gz", &tar_gz_with_link("pkg/link", "/etc/passwd"), "pkg");
    assert!(matches!(r, Err(UpdateError::UnsafeEntry(_))), "{r:?}");
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    w.add_symlink("pkg/link", "../../evil", zip::write::SimpleFileOptions::default()).unwrap();
    let r = extract_bytes(&dir, "p.zip", &w.finish().unwrap().into_inner(), "pkg");
    assert!(matches!(r, Err(UpdateError::UnsafeEntry(_))), "{r:?}");
    // Damaged archives.
    let r = extract_bytes(&dir, "p.zip", b"PK\x03\x04 broken", "pkg");
    assert!(matches!(r, Err(UpdateError::BadPackage(_))), "{r:?}");
    let r = extract_bytes(&dir, "p.tar.gz", b"\x1f\x8b broken", "pkg");
    assert!(matches!(r, Err(UpdateError::BadPackage(_))), "{r:?}");
    let _ = fs::remove_dir_all(&dir);
}

// ----- Installation --------------------------------------------------------------------------------------------

const VERSION: &str = "0.2.0";

fn package_root() -> String {
    format!("kanavos-{VERSION}-windows-x86_64")
}

fn old_files() -> Vec<(String, &'static str)> {
    vec![
        (MARKER.into(), "old quickstart"),
        (app_file(), "old app"),
        (cli_file(), "old cli"),
        ("README.md".into(), "old readme"),
        ("docs/guide/getting-started.md".into(), "old guide"),
        (SAMPLE.into(), "old sample"),
    ]
}

fn user_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("my-house.kanavos", "my project"),
        ("notes.txt", "my notes"),
        ("notes.old", "my old notes"),
        ("evil.kanavos", "my other project"),
    ]
}

fn new_files() -> Vec<(String, &'static str)> {
    vec![
        (MARKER.into(), "new quickstart"),
        (app_file(), "new app"),
        (cli_file(), "new cli"),
        ("README.md".into(), "new readme"),
        ("LICENSE-MIT".into(), "new licence"),
        ("docs/guide/getting-started.md".into(), "new guide"),
        ("docs/updates.md".into(), "new updates guide"),
        (SAMPLE.into(), "new sample"),
    ]
}

fn write_rel(dir: &Path, rel: &str, text: &str) {
    let p = rel.split('/').fold(dir.to_path_buf(), |p, c| p.join(c));
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn read_rel(dir: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(rel.split('/').fold(dir.to_path_buf(), |p, c| p.join(c))).ok()
}

/// An installation folder with the previous version and some user files.
fn installation(name: &str) -> PathBuf {
    let dir = temp(name).join("app");
    for (rel, text) in old_files() {
        write_rel(&dir, &rel, text);
    }
    for (rel, text) in user_files() {
        write_rel(&dir, rel, text);
    }
    dir
}

/// Every file under `dir`, relative, sorted.
fn listing(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let name = e.file_name().into_string().unwrap();
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            if e.file_type().unwrap().is_dir() {
                walk(&e.path(), &rel, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort();
    out
}

fn available() -> Available {
    let asset = |name: &str| Asset {
        name: name.into(),
        url: format!("https://github.com/x/y/releases/download/v/{name}"),
        size: 0,
    };
    let archive = format!("{}.zip", package_root());
    Available {
        version: v(VERSION),
        tag: format!("v{VERSION}"),
        title: "Kanavos".into(),
        published_at: None,
        notes: String::new(),
        prerelease: false,
        page: String::new(),
        archive: asset(&archive),
        checksum: asset(&format!("{archive}.sha256")),
        signature: asset(&format!("{archive}.sig")),
        package: package_root(),
    }
}

/// A downloaded package (as after verification) holding `files` under the package folder.
fn downloaded(base: &Path, files: &[(String, &str)], tar: bool) -> Downloaded {
    let workdir = base.join("work");
    let _ = fs::remove_dir_all(&workdir);
    fs::create_dir_all(&workdir).unwrap();
    let root = package_root();
    let paths: Vec<(String, &[u8])> =
        files.iter().map(|(rel, text)| (format!("{root}/{rel}"), text.as_bytes())).collect();
    let entries: Vec<(&str, &[u8])> = paths.iter().map(|(p, d)| (p.as_str(), *d)).collect();
    let (name, bytes) = if tar {
        (format!("{root}.tar.gz"), tar_gz_bytes(&entries))
    } else {
        (format!("{root}.zip"), zip_bytes(&entries))
    };
    let archive = workdir.join(&name);
    fs::write(&archive, bytes).unwrap();
    let mut available = available();
    available.archive.name = name;
    Downloaded { available, archive, workdir }
}

fn package_with_extras() -> Vec<(String, &'static str)> {
    let mut files = new_files();
    // A project file other than the sample must never be installed over the user's.
    files.push(("evil.kanavos".into(), "from the package"));
    files
}

#[test]
fn installing_replaces_the_package_files_and_keeps_user_files() {
    for tar in [false, true] {
        let dir = installation(if tar { "inst-tar" } else { "inst-zip" });
        let d = downloaded(dir.parent().unwrap(), &package_with_extras(), tar);
        let installed = install_into(&d, &dir).unwrap();
        assert_eq!(installed.version, v(VERSION));
        assert_eq!(installed.files, new_files().len());
        assert_eq!(installed.app, dir.join(app_file()));
        for (rel, text) in new_files() {
            assert_eq!(read_rel(&dir, &rel).as_deref(), Some(text), "{rel}");
        }
        for (rel, text) in user_files() {
            assert_eq!(read_rel(&dir, rel).as_deref(), Some(text), "user file {rel} must not change");
        }
        // Nothing in use, so the previous files are already gone; no journal or lock is left.
        let mut expected: Vec<String> = new_files().into_iter().map(|(r, _)| r).collect();
        expected.extend(user_files().into_iter().map(|(r, _)| r.to_owned()));
        expected.sort();
        assert_eq!(listing(&dir), expected);
        // The extracted copy is gone; the download folder goes with the value.
        assert!(!d.workdir().join("pkg").exists());
        let work = d.workdir().to_path_buf();
        drop(d);
        assert!(!work.exists());
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }
}

#[test]
fn a_failed_replacement_restores_the_previous_version() {
    let dir = installation("rollback");
    let before = listing(&dir);
    let d = downloaded(dir.parent().unwrap(), &new_files(), false);
    // The new README cannot be moved into place (as when a file is locked).
    fault::fail_renames_of(&["README.md.new"]);
    let r = install_into(&d, &dir);
    fault::fail_renames_of(&[]);
    assert!(matches!(r, Err(UpdateError::InstallFailed { restored: true, .. })), "{r:?}");
    assert_eq!(listing(&dir), before, "no staged, backup, journal or new files are left");
    for (rel, text) in old_files() {
        assert_eq!(read_rel(&dir, &rel).as_deref(), Some(text), "{rel}");
    }
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn an_incomplete_rollback_is_completed_on_the_next_start() {
    let dir = installation("forward");
    let d = downloaded(dir.parent().unwrap(), &new_files(), false);
    // The swap fails at README.md, and the rollback cannot restore QUICKSTART.txt either.
    fault::fail_renames_of(&["README.md.new", "QUICKSTART.txt.old"]);
    let r = install_into(&d, &dir);
    fault::fail_renames_of(&[]);
    assert!(matches!(r, Err(UpdateError::InstallFailed { restored: false, .. })), "{r:?}");
    assert!(dir.join(JOURNAL).exists());
    // The next start completes the installation from the staged files.
    let report = cleanup_old_in(&dir).unwrap();
    assert!(report.completed);
    assert_eq!(report.pending, 0);
    for (rel, text) in new_files() {
        assert_eq!(read_rel(&dir, &rel).as_deref(), Some(text), "{rel}");
    }
    for (rel, text) in user_files() {
        assert_eq!(read_rel(&dir, rel).as_deref(), Some(text), "{rel}");
    }
    let mut expected: Vec<String> = new_files().into_iter().map(|(r, _)| r).collect();
    expected.extend(user_files().into_iter().map(|(r, _)| r.to_owned()));
    expected.sort();
    assert_eq!(listing(&dir), expected);
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn cleanup_deletes_only_recorded_leftovers() {
    let dir = installation("cleanup");
    write_rel(&dir, &format!("{}.old", app_file()), "previous app, was running");
    write_rel(&dir, "docs/guide/getting-started.md.old.2", "previous guide");
    let journal = serde_json::json!({
        "format": 1,
        "state": "installed",
        "version": VERSION,
        "files": [
            { "path": app_file(), "backup": format!("{}.old", app_file()) },
            { "path": "README.md", "backup": null }
        ],
        // Paths that are not update leftovers are never deleted, whatever the journal says.
        "trash": ["docs/guide/getting-started.md.old.2", "notes.txt", "my-house.kanavos", "../outside.old"]
    });
    fs::write(dir.join(JOURNAL), journal.to_string()).unwrap();
    let report = cleanup_old_in(&dir).unwrap();
    assert_eq!(report.removed, 2);
    assert_eq!(report.pending, 0);
    assert!(!dir.join(JOURNAL).exists());
    assert!(!dir.join(format!("{}.old", app_file())).exists());
    for (rel, text) in user_files() {
        assert_eq!(read_rel(&dir, rel).as_deref(), Some(text), "{rel}");
    }
    // Without a journal there is nothing to do.
    assert_eq!(cleanup_old_in(&dir).unwrap(), Cleanup::default());
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn an_interrupted_staging_is_abandoned() {
    let dir = installation("staging");
    write_rel(&dir, "README.md.new", "half-copied");
    let journal = serde_json::json!({
        "format": 1, "state": "staging", "version": VERSION,
        "files": [{ "path": "README.md", "backup": "README.md.old" }]
    });
    fs::write(dir.join(JOURNAL), journal.to_string()).unwrap();
    let report = cleanup_old_in(&dir).unwrap();
    assert!(!report.completed);
    assert!(!dir.join("README.md.new").exists());
    assert!(!dir.join(JOURNAL).exists());
    assert_eq!(read_rel(&dir, "README.md").as_deref(), Some("old readme"));
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn a_busy_folder_is_left_alone_and_a_stale_lock_is_taken_over() {
    let dir = installation("lock");
    let d = downloaded(dir.parent().unwrap(), &new_files(), false);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    fs::write(dir.join(LOCK), format!("1 {now}")).unwrap();
    assert!(matches!(install_into(&d, &dir), Err(UpdateError::Busy)));
    fs::write(dir.join(JOURNAL), r#"{"format":1,"state":"installed","version":"0.2.0"}"#).unwrap();
    assert!(cleanup_old_in(&dir).unwrap().busy);
    assert!(dir.join(JOURNAL).exists());
    // A lock from a process that stopped long ago.
    fs::write(dir.join(LOCK), "1 1000").unwrap();
    install_into(&d, &dir).unwrap();
    assert!(!dir.join(LOCK).exists());
    assert!(!dir.join(JOURNAL).exists());
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn unsafe_incomplete_or_foreign_packages_change_nothing() {
    let dir = installation("reject");
    let before = listing(&dir);
    let base = dir.parent().unwrap().to_path_buf();
    // Path traversal inside the package.
    let mut files = new_files();
    files.push(("../../escape.txt".into(), "evil"));
    let r = install_into(&downloaded(&base, &files, false), &dir);
    assert!(matches!(r, Err(UpdateError::UnsafeEntry(_))), "{r:?}");
    // No command-line program.
    let files: Vec<(String, &str)> = new_files().into_iter().filter(|(r, _)| *r != cli_file()).collect();
    let r = install_into(&downloaded(&base, &files, true), &dir);
    assert!(matches!(r, Err(UpdateError::BadPackage(_))), "{r:?}");
    assert_eq!(listing(&dir), before);
    assert!(!base.join("escape.txt").exists());
    // A folder that is not a packaged installation (a build folder, a system folder).
    fs::remove_file(dir.join(MARKER)).unwrap();
    let r = install_into(&downloaded(&base, &new_files(), false), &dir);
    assert!(matches!(r, Err(UpdateError::NotPackaged(_))), "{r:?}");
    assert_eq!(read_rel(&dir, &app_file()).as_deref(), Some("old app"));
    let _ = fs::remove_dir_all(&base);
}

#[cfg(unix)]
#[test]
fn a_read_only_folder_is_reported() {
    use std::os::unix::fs::PermissionsExt;
    let dir = installation("readonly");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    let writable = fs::write(dir.join("probe"), b"x").is_ok();
    let _ = fs::remove_file(dir.join("probe"));
    if !writable {
        // (Skipped when running as root, which can write anyway.)
        let r = install_into(&downloaded(dir.parent().unwrap(), &new_files(), true), &dir);
        assert!(matches!(r, Err(UpdateError::NotWritable(_))), "{r:?}");
    }
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let _ = fs::remove_dir_all(dir.parent().unwrap());
}
