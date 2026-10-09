//! `manifest.json`: what the archive is, which schema it uses, and a summary of its contents.

use std::collections::BTreeMap;

use kanavos_model::{Category, Document};
use serde::{Deserialize, Serialize};

use crate::FormatError;

/// Value of [`Manifest::format`] in every Kanavos project file.
pub const FORMAT_NAME: &str = "kanavos";

/// Object counts recorded in the manifest, so file browsers can describe a project without parsing the model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Counts {
    /// Storeys.
    pub storeys: usize,
    /// All elements, including annotation.
    pub elements: usize,
    /// Materials.
    pub materials: usize,
    /// Build-ups.
    pub build_ups: usize,
    /// Views.
    pub views: usize,
    /// Sheets.
    pub sheets: usize,
    /// Elements per category key (`wall`, `slab`, …); categories with no elements are left out.
    pub by_category: BTreeMap<String, usize>,
}

impl Counts {
    /// Counts the contents of a document.
    pub fn of(doc: &Document) -> Counts {
        let mut by_category: BTreeMap<String, usize> = BTreeMap::new();
        for e in doc.elements() {
            *by_category.entry(category_key(e.category()).to_owned()).or_default() += 1;
        }
        Counts {
            storeys: doc.storeys().len(),
            elements: doc.element_count(),
            materials: doc.materials().count(),
            build_ups: doc.build_ups().count(),
            views: doc.views().count(),
            sheets: doc.sheets().len(),
            by_category,
        }
    }

    /// Named totals, for comparing two counts.
    pub(crate) fn totals(&self) -> [(&'static str, usize); 6] {
        [
            ("storeys", self.storeys),
            ("elements", self.elements),
            ("materials", self.materials),
            ("build-ups", self.build_ups),
            ("views", self.views),
            ("sheets", self.sheets),
        ]
    }
}

fn category_key(c: Category) -> &'static str {
    c.key()
}

/// Contents of `manifest.json`.
///
/// `format` and `schema_version` are the stable contract: every past and future version of the format keeps them
/// with this meaning, so any build can tell whether it can read a file. Other fields may be added in later
/// versions; readers ignore fields they do not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always [`FORMAT_NAME`].
    pub format: String,
    /// Schema version of `model.json`.
    pub schema_version: u32,
    /// Version of the application that wrote the file.
    #[serde(default)]
    pub app_version: String,
    /// When the project was first created (RFC 3339), as given by the caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// When the project was last saved (RFC 3339), as given by the caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    /// Contents summary.
    #[serde(default)]
    pub counts: Counts,
    /// Whether the archive holds `thumbnail.png`.
    #[serde(default)]
    pub thumbnail: bool,
    /// For crash-recovery files: the project file this is a recovery copy of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_of: Option<String>,
}

/// Checks that a string is an RFC 3339 date-time: `YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`.
///
/// Only the shape and field ranges are checked (month 01–12, day 01–31, hour 00–23, minute 00–59, second 00–60).
pub fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let num = |r: std::ops::Range<usize>| -> Option<u32> { s.get(r)?.parse().ok() };
    if b.len() < 20 || !digits(0..4) || !digits(5..7) || !digits(8..10) {
        return false;
    }
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || !matches!(b.get(10), Some(b'T' | b't' | b' ')) {
        return false;
    }
    if !digits(11..13) || b.get(13) != Some(&b':') || !digits(14..16) || b.get(16) != Some(&b':') {
        return false;
    }
    if !digits(17..19) {
        return false;
    }
    let in_range = |v: Option<u32>, lo: u32, hi: u32| v.is_some_and(|v| (lo..=hi).contains(&v));
    if !(in_range(num(5..7), 1, 12)
        && in_range(num(8..10), 1, 31)
        && in_range(num(11..13), 0, 23)
        && in_range(num(14..16), 0, 59)
        && in_range(num(17..19), 0, 60))
    {
        return false;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match b.get(i) {
        Some(b'Z' | b'z') => i + 1 == b.len(),
        Some(b'+' | b'-') => {
            i + 6 == b.len()
                && digits(i + 1..i + 3)
                && b.get(i + 3) == Some(&b':')
                && digits(i + 4..i + 6)
                && in_range(num(i + 1..i + 3), 0, 23)
                && in_range(num(i + 4..i + 6), 0, 59)
        }
        _ => false,
    }
}

pub(crate) fn check_timestamp(t: &Option<String>) -> Result<(), FormatError> {
    match t {
        Some(s) if !is_rfc3339(s) => Err(FormatError::InvalidTimestamp(s.clone())),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_rfc3339() {
        for ok in [
            "2026-10-09T12:00:00Z",
            "2026-10-09T12:00:00.123Z",
            "2026-10-09T12:00:00+03:00",
            "2026-10-09t23:59:60-01:30",
        ] {
            assert!(is_rfc3339(ok), "{ok}");
        }
        for bad in [
            "",
            "2026-10-09",
            "2026-13-09T12:00:00Z",
            "2026-10-09T24:00:00Z",
            "2026-10-09T12:00:00",
            "2026-10-09T12:00:00.Z",
            "2026-10-09T12:00:00+0300",
            "2026-10-09T12:00:00Zjunk",
            "２026-10-09T12:00:00Z",
        ] {
            assert!(!is_rfc3339(bad), "{bad}");
        }
    }

    #[test]
    fn counts_the_starter() {
        let doc = kanavos_model::starter();
        let c = Counts::of(&doc);
        assert_eq!(c.storeys, 2);
        assert_eq!(c.elements, 0);
        assert!(c.materials > 10);
        assert!(c.by_category.is_empty());
    }
}
