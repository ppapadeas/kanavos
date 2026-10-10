//! `cargo xtask dist`: builds the release binaries and assembles a portable package under `dist/`.
//!
//! Layout of `dist/kanavos-<version>-<os>-<arch>/`:
//! - `kanavos[.exe]` (desktop app) and `kanavos-cli[.exe]` (headless tools)
//! - `sample-house.kanavos`, written by the CLI from the built-in sample
//! - `QUICKSTART.txt`, `README.md`, the guides under `docs/`, `LICENSE-*`, `NOTICE`, `ASSETS.md`
//!
//! The folder is also archived next to it: `.zip` on Windows (with the system `tar`, which writes zip archives
//! when the name ends in `.zip`), `.tar.gz` elsewhere.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{root, run};

const QUICKSTART: &str = "\
Kanavos — γρήγορη εκκίνηση / quick start
=======================================

Ελληνικά
--------
1. Εκτελέστε το kanavos (kanavos.exe στα Windows). Η πρώτη εκκίνηση ανοίγει ένα δείγμα διώροφης κατοικίας.
2. Αρχείο › Άνοιγμα… ανοίγει το sample-house.kanavos (το ίδιο δείγμα, ως αρχείο).
3. Αρχείο › Νέο ξεκινά καινούργιο έργο. Οδηγός: docs/guide/odigos.el.md
4. Η διεπαφή χρειάζεται εγκατεστημένη γραμματοσειρά με ελληνικά (π.χ. Segoe UI). Ορίστε άλλη με τη μεταβλητή
   περιβάλλοντος KANAVOS_FONT=<διαδρομή .ttf>.
5. Γραμμή εντολών: kanavos-cli help

English
-------
1. Run kanavos (kanavos.exe on Windows). The first launch opens a sample two-storey house.
2. File › Open… opens sample-house.kanavos (the same sample, as a file).
3. File › New starts a new project. Guide: docs/guide/getting-started.md
4. The interface needs an installed font with Greek coverage (e.g. Segoe UI). Choose another with the environment
   variable KANAVOS_FONT=<path to .ttf>.
5. Command line: kanavos-cli help

Status: early alpha. Licence: MIT OR Apache-2.0 (see LICENSE-MIT, LICENSE-APACHE, NOTICE).
";

fn workspace_version(root: &Path) -> String {
    fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|s| {
            s.lines().skip_while(|l| l.trim() != "[workspace.package]").find_map(|l| {
                l.trim().strip_prefix("version = \"").and_then(|r| r.strip_suffix('"')).map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "0.0.0".to_owned())
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::copy(from, to).map(|_| ()).map_err(|e| format!("copying {} to {}: {e}", from.display(), to.display()))
}

/// Builds and packages the desktop app and the command-line tools.
pub fn dist() -> Result<(), String> {
    let root = root();
    run("cargo", &["build", "--release", "-p", "kanavos", "-p", "kanavos-cli"])?;
    let version = workspace_version(&root);
    let name = format!("kanavos-{version}-{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let dist = root.join("dist");
    let dir = dist.join(&name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("clearing {}: {e}", dir.display()))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;

    let release = root.join("target").join("release");
    for bin in ["kanavos", "kanavos-cli"] {
        copy(&release.join(exe(bin)), &dir.join(exe(bin)))?;
    }
    for f in ["README.md", "LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ASSETS.md"] {
        copy(&root.join(f), &dir.join(f))?;
    }
    for f in ["getting-started.md", "odigos.el.md"] {
        copy(&root.join("docs").join("guide").join(f), &dir.join("docs").join("guide").join(f))?;
    }
    for f in ["control-protocol.md", "commands.md", "format.md"] {
        copy(&root.join("docs").join(f), &dir.join("docs").join(f))?;
    }
    fs::write(
        dir.join("QUICKSTART.txt"),
        QUICKSTART.replace('\n', if cfg!(windows) { "\r\n" } else { "\n" }),
    )
    .map_err(|e| format!("writing QUICKSTART.txt: {e}"))?;

    // The sample project, written by the freshly built CLI.
    let sample = dir.join("sample-house.kanavos");
    let status = Command::new(dir.join(exe("kanavos-cli")))
        .args(["--lang", "en", "sample"])
        .arg(&sample)
        .arg("--force")
        .status()
        .map_err(|e| format!("could not run kanavos-cli: {e}"))?;
    if !status.success() {
        return Err("kanavos-cli sample failed".into());
    }

    let archive: PathBuf =
        if cfg!(windows) { dist.join(format!("{name}.zip")) } else { dist.join(format!("{name}.tar.gz")) };
    if archive.exists() {
        fs::remove_file(&archive).map_err(|e| format!("removing {}: {e}", archive.display()))?;
    }
    let mut tar = Command::new("tar");
    if cfg!(windows) {
        tar.arg("-a").arg("-c").arg("-f");
    } else {
        tar.arg("-czf");
    }
    let status = tar
        .arg(&archive)
        .arg("-C")
        .arg(&dist)
        .arg(&name)
        .status()
        .map_err(|e| format!("could not run tar: {e}"))?;
    if !status.success() {
        return Err("archiving the package failed".into());
    }
    println!("dist: {}", dir.display());
    println!("dist: {}", archive.display());
    Ok(())
}
