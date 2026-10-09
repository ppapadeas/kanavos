//! The message catalogue: every user-facing string in Greek and English.
//!
//! Layer L1. See `docs/architecture.md`.
//!
//! Messages live in tab-separated files under `catalogue/`, one file per area so that areas can be edited
//! independently: `key<TAB>el<TAB>en`. Lines starting with `#` and blank lines are ignored. Placeholders are
//! written `{name}` and filled by [`tf`].
//!
//! Every key must have both an `el` and an `en` text (enforced by a test). A missing key returns the key itself,
//! so a forgotten entry is visible but never crashes.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

pub use kanavos_units::Locale;

/// The catalogue sources, by area. Add new areas here.
const SOURCES: &[(&str, &str)] = &[
    ("common", include_str!("../catalogue/common.tsv")),
    ("engine", include_str!("../catalogue/engine.tsv")),
    ("elements", include_str!("../catalogue/elements.tsv")),
    ("objects", include_str!("../catalogue/objects.tsv")),
    ("views", include_str!("../catalogue/views.tsv")),
    ("rules", include_str!("../catalogue/rules.tsv")),
    ("schedules", include_str!("../catalogue/schedules.tsv")),
    ("sheets", include_str!("../catalogue/sheets.tsv")),
    ("drawing", include_str!("../catalogue/drawing.tsv")),
    ("files", include_str!("../catalogue/files.tsv")),
    ("tools", include_str!("../catalogue/tools.tsv")),
    ("ui", include_str!("../catalogue/ui.tsv")),
    ("canvas", include_str!("../catalogue/canvas.tsv")),
    ("cli", include_str!("../catalogue/cli.tsv")),
];

/// One catalogue entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Greek text.
    pub el: String,
    /// English text.
    pub en: String,
}

/// A problem found while parsing the catalogue (reported by [`check`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueIssue {
    /// Area file.
    pub area: &'static str,
    /// 1-based line number.
    pub line: usize,
    /// What is wrong.
    pub problem: String,
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\t", "\t")
}

fn parse() -> (HashMap<String, Entry>, Vec<CatalogueIssue>) {
    let mut map = HashMap::new();
    let mut issues = Vec::new();
    for (area, text) in SOURCES {
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('\t');
            let (Some(key), Some(el), Some(en)) = (parts.next(), parts.next(), parts.next()) else {
                issues.push(CatalogueIssue {
                    area,
                    line: i + 1,
                    problem: "expected key<TAB>el<TAB>en".into(),
                });
                continue;
            };
            if parts.next().is_some() {
                issues.push(CatalogueIssue { area, line: i + 1, problem: "too many columns".into() });
            }
            let key = key.trim();
            if key.is_empty() || el.trim().is_empty() || en.trim().is_empty() {
                issues.push(CatalogueIssue { area, line: i + 1, problem: format!("empty field in '{key}'") });
                continue;
            }
            if map.insert(key.to_owned(), Entry { el: unescape(el), en: unescape(en) }).is_some() {
                issues.push(CatalogueIssue { area, line: i + 1, problem: format!("duplicate key '{key}'") });
            }
        }
    }
    (map, issues)
}

fn catalogue() -> &'static HashMap<String, Entry> {
    static CATALOGUE: OnceLock<HashMap<String, Entry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| parse().0)
}

/// The text for a key in a locale, or the key itself when it is missing.
pub fn t(locale: Locale, key: &str) -> Cow<'static, str> {
    match catalogue().get(key) {
        Some(e) => Cow::Borrowed(match locale {
            Locale::El => e.el.as_str(),
            Locale::En => e.en.as_str(),
        }),
        None => Cow::Owned(key.to_owned()),
    }
}

/// The text for a key with `{name}` placeholders filled from `args`.
pub fn tf(locale: Locale, key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(locale, key).into_owned();
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

/// Whether a key exists.
pub fn has(key: &str) -> bool {
    catalogue().contains_key(key)
}

/// All keys, sorted (for tests and tooling).
pub fn keys() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = catalogue().keys().map(String::as_str).collect();
    v.sort_unstable();
    v
}

/// Parses the catalogue and returns every problem found (empty when the catalogue is valid).
pub fn check() -> Vec<CatalogueIssue> {
    parse().1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_valid() {
        let issues = check();
        assert!(issues.is_empty(), "catalogue problems: {issues:#?}");
    }

    #[test]
    fn known_keys_translate_and_unknown_keys_echo() {
        assert_eq!(t(Locale::El, "common.ok"), "Εντάξει");
        assert_eq!(t(Locale::En, "common.ok"), "OK");
        assert_eq!(t(Locale::En, "no.such.key"), "no.such.key");
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(tf(Locale::En, "common.n_selected", &[("n", "3")]), "3 selected");
        assert_eq!(tf(Locale::El, "common.n_selected", &[("n", "3")]), "3 επιλεγμένα");
    }
}
