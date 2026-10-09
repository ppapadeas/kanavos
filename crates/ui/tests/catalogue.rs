//! Every interface string key used in the source exists in the catalogue, in both languages.

use std::path::Path;

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// String literals that look like complete catalogue keys of the interface (`"ui.…"`).
fn keys_in(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for piece in text.split('"') {
        // A piece made only of key characters and starting with `ui.` can only be a key literal.
        let is_key = piece.starts_with("ui.")
            && !piece.ends_with('.')
            && piece.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_');
        if is_key {
            keys.push(piece.to_owned());
        }
    }
    keys
}

#[test]
fn every_interface_key_is_in_the_catalogue() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(!files.is_empty());
    let mut missing = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for key in keys_in(&text) {
            if !kanavos_i18n::has(&key) {
                missing.push(format!("{}: {key}", f.display()));
            }
        }
    }
    assert!(missing.is_empty(), "keys missing from the catalogue:\n{}", missing.join("\n"));
}

#[test]
fn the_catalogue_is_well_formed() {
    assert!(kanavos_i18n::check().is_empty(), "{:#?}", kanavos_i18n::check());
}
