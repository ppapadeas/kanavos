//! Repository automation. Run with `cargo xtask <command>`.

mod layers;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const HELP: &str = "\
cargo xtask <COMMAND>

COMMANDS:
    ci                    fmt check, clippy (-D warnings), tests, layering, provenance
    layering              check that crates depend only on lower layers
    provenance [RANGE]    check Signed-off-by and Clean-Room trailers (default: commits not on origin/main)
    bench                 run performance budgets (not yet implemented)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("layering") => layering(),
        Some("provenance") => provenance(args.get(1).map(String::as_str)),
        Some("bench") => {
            println!("No benchmarks yet; budgets are defined in docs/architecture.md §7.");
            Ok(())
        }
        _ => {
            print!("{HELP}");
            Ok(())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(root())
        .status()
        .map_err(|e| format!("could not start {program}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{program} {} failed", args.join(" "))) }
}

fn ci() -> Result<(), String> {
    run("cargo", &["fmt", "--all", "--check"])?;
    run("cargo", &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])?;
    run("cargo", &["test", "--workspace", "--quiet"])?;
    layering()?;
    provenance(None)?;
    println!("ci: all checks passed");
    Ok(())
}

/// Reads `domos-*` (and app) dependency names from a manifest's `[dependencies]` and `[dev-dependencies]`.
fn internal_deps(manifest: &str) -> (Vec<String>, Vec<String>) {
    let mut deps = Vec::new();
    let mut dev = Vec::new();
    let mut section = "";
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = match line {
                "[dependencies]" => "deps",
                "[dev-dependencies]" => "dev",
                _ => "",
            };
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim();
            if name.starts_with("domos") {
                match section {
                    "deps" => deps.push(name.to_owned()),
                    "dev" => dev.push(name.to_owned()),
                    _ => {}
                }
            }
        }
    }
    (deps, dev)
}

fn layering() -> Result<(), String> {
    let layer: HashMap<&str, u32> = layers::LAYERS.iter().copied().collect();
    let mut errors = Vec::new();
    let mut checked = 0;
    for dir in ["crates", "apps"] {
        let entries = fs::read_dir(root().join(dir)).map_err(|e| format!("reading {dir}: {e}"))?;
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("Cargo.toml");
            let Ok(manifest) = fs::read_to_string(&manifest_path) else { continue };
            let Some(name) = manifest
                .lines()
                .find_map(|l| l.trim().strip_prefix("name = \"").and_then(|r| r.strip_suffix('"')))
            else {
                continue;
            };
            let Some(&own) = layer.get(name) else {
                errors.push(format!("{name}: not in xtask/src/layers.rs"));
                continue;
            };
            checked += 1;
            let (deps, dev) = internal_deps(&manifest);
            for dep in deps {
                match layer.get(dep.as_str()) {
                    Some(&l) if l < own => {}
                    Some(&l) => errors.push(format!("{name} (L{own}) depends on {dep} (L{l})")),
                    None => errors.push(format!("{name} depends on unknown crate {dep}")),
                }
            }
            for dep in dev {
                match layer.get(dep.as_str()) {
                    Some(_) if dep == "domos-testkit" => {}
                    Some(&l) if l < own => {}
                    Some(&l) => errors.push(format!("{name} (L{own}) dev-depends on {dep} (L{l})")),
                    None => errors.push(format!("{name} dev-depends on unknown crate {dep}")),
                }
            }
        }
    }
    if errors.is_empty() {
        println!("layering: {checked} crates OK");
        Ok(())
    } else {
        Err(format!("layering violations:\n  {}", errors.join("\n  ")))
    }
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root())
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

fn provenance(range: Option<&str>) -> Result<(), String> {
    let range = match range {
        Some(r) => r.to_owned(),
        None if git(&["rev-parse", "--verify", "--quiet", "origin/main"]).is_ok() => {
            "origin/main..HEAD".to_owned()
        }
        None => "HEAD".to_owned(),
    };
    let log = match git(&["log", "--format=%H%x00%B%x1e", &range]) {
        Ok(log) => log,
        Err(e) if e.contains("does not have any commits") || e.contains("unknown revision") => {
            println!("provenance: no commits to check");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let mut errors = Vec::new();
    let mut checked = 0;
    for record in log.split('\u{1e}') {
        let record = record.trim();
        let Some((hash, body)) = record.split_once('\0') else { continue };
        checked += 1;
        let short = hash.get(..10).unwrap_or(hash);
        if !body.lines().any(|l| l.starts_with("Signed-off-by: ")) {
            errors.push(format!("{short}: missing Signed-off-by (use git commit -s)"));
        }
        let clean = body.lines().find_map(|l| l.strip_prefix("Clean-Room: ")).map(str::trim);
        match clean {
            Some("yes" | "same-person" | "n/a") => {}
            Some(other) => errors.push(format!("{short}: invalid Clean-Room value '{other}'")),
            None => errors.push(format!("{short}: missing Clean-Room trailer")),
        }
    }
    if errors.is_empty() {
        println!("provenance: {checked} commits OK");
        Ok(())
    } else {
        Err(format!("provenance problems (see docs/legal/clean-room.md §6):\n  {}", errors.join("\n  ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_internal_dependencies() {
        let m = "[package]\nname = \"x\"\n[dependencies]\ndomos-geom = { path = \"../geom\" }\nserde = \"1\"\n\
                 [dev-dependencies]\ndomos-testkit = { path = \"../testkit\" }\n";
        let (deps, dev) = internal_deps(m);
        assert_eq!(deps, vec!["domos-geom"]);
        assert_eq!(dev, vec!["domos-testkit"]);
    }

    #[test]
    fn layer_table_is_consistent() {
        layering().unwrap();
    }
}
