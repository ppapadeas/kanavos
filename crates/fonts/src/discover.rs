//! Finding a Greek-capable sans-serif font installed on the machine.

use std::path::{Path, PathBuf};

use crate::FontFace;

/// Environment variable naming a font file to use instead of discovery.
pub const FONT_ENV_VAR: &str = "KANAVOS_FONT";

/// Environment variable naming the bold companion of [`FONT_ENV_VAR`] (optional).
pub const FONT_BOLD_ENV_VAR: &str = "KANAVOS_FONT_BOLD";

/// Characters a font must contain to be used: Latin, digits and Greek with tonos.
const REQUIRED: &str = "Aa0ΑΩαωάΆΐ";

/// A font found on the machine: a regular face and, when available, its bold companion.
#[derive(Debug, Clone)]
pub struct FontCandidate {
    /// Regular face.
    pub regular: FontFace,
    /// Bold face, if found.
    pub bold: Option<FontFace>,
}

/// Font files to try, in order of preference, as `(regular, bold)` pairs of absolute paths.
///
/// The list depends on the operating system and starts with the files named by [`FONT_ENV_VAR`] and
/// [`FONT_BOLD_ENV_VAR`] when set. Files that do not exist are included; [`discover_system_font`] skips them.
pub fn candidate_fonts() -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os(FONT_ENV_VAR).filter(|v| !v.is_empty()) {
        let bold = std::env::var_os(FONT_BOLD_ENV_VAR).filter(|v| !v.is_empty()).map(PathBuf::from);
        out.push((PathBuf::from(p), bold));
    }
    if cfg!(windows) {
        let mut dirs: Vec<PathBuf> = Vec::new();
        for var in ["WINDIR", "SystemRoot"] {
            if let Some(w) = std::env::var_os(var) {
                let d = PathBuf::from(w).join("Fonts");
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
        }
        if dirs.is_empty() {
            dirs.push(PathBuf::from(r"C:\Windows\Fonts"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        }
        let names = [
            ("segoeui.ttf", "segoeuib.ttf"),
            ("arial.ttf", "arialbd.ttf"),
            ("tahoma.ttf", "tahomabd.ttf"),
            ("calibri.ttf", "calibrib.ttf"),
            ("verdana.ttf", "verdanab.ttf"),
        ];
        push_pairs(&mut out, &dirs, &names);
    } else if cfg!(target_os = "macos") {
        let dirs = [
            PathBuf::from("/System/Library/Fonts/Supplemental"),
            PathBuf::from("/Library/Fonts"),
            PathBuf::from("/System/Library/Fonts"),
        ];
        let names = [
            ("Arial.ttf", "Arial Bold.ttf"),
            ("Tahoma.ttf", "Tahoma Bold.ttf"),
            ("Verdana.ttf", "Verdana Bold.ttf"),
            ("Helvetica.ttc", ""),
            ("Geneva.ttf", ""),
        ];
        push_pairs(&mut out, &dirs, &names);
    } else {
        let mut dirs = vec![PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")];
        if let Some(h) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(&h).join(".local/share/fonts"));
            dirs.push(PathBuf::from(&h).join(".fonts"));
        }
        let names = [
            ("truetype/dejavu/DejaVuSans.ttf", "truetype/dejavu/DejaVuSans-Bold.ttf"),
            ("dejavu/DejaVuSans.ttf", "dejavu/DejaVuSans-Bold.ttf"),
            ("dejavu-sans-fonts/DejaVuSans.ttf", "dejavu-sans-fonts/DejaVuSans-Bold.ttf"),
            ("TTF/DejaVuSans.ttf", "TTF/DejaVuSans-Bold.ttf"),
            ("truetype/noto/NotoSans-Regular.ttf", "truetype/noto/NotoSans-Bold.ttf"),
            ("noto/NotoSans-Regular.ttf", "noto/NotoSans-Bold.ttf"),
            ("google-noto/NotoSans-Regular.ttf", "google-noto/NotoSans-Bold.ttf"),
            ("truetype/liberation/LiberationSans-Regular.ttf", "truetype/liberation/LiberationSans-Bold.ttf"),
            ("liberation-sans/LiberationSans-Regular.ttf", "liberation-sans/LiberationSans-Bold.ttf"),
            ("TTF/LiberationSans-Regular.ttf", "TTF/LiberationSans-Bold.ttf"),
            ("truetype/freefont/FreeSans.ttf", "truetype/freefont/FreeSansBold.ttf"),
        ];
        push_pairs(&mut out, &dirs, &names);
    }
    out
}

fn push_pairs(out: &mut Vec<(PathBuf, Option<PathBuf>)>, dirs: &[PathBuf], names: &[(&str, &str)]) {
    for (regular, bold) in names {
        for d in dirs {
            let b = (!bold.is_empty()).then(|| d.join(bold));
            out.push((d.join(regular), b));
        }
    }
}

/// Loads a face and checks it covers Latin, digits and Greek.
fn load_checked(path: &Path) -> Option<FontFace> {
    if !path.is_file() {
        return None;
    }
    let face = FontFace::from_file(path).ok()?;
    face.covers(REQUIRED).then_some(face)
}

/// Finds the first installed font from [`candidate_fonts`] that parses and covers Greek, with its bold face when
/// present. Returns `None` when no such font exists (text then uses the built-in stroke font).
pub fn discover_system_font() -> Option<FontCandidate> {
    candidate_fonts().into_iter().find_map(|(regular, bold)| {
        let regular = load_checked(&regular)?;
        let bold = bold.as_deref().and_then(load_checked);
        Some(FontCandidate { regular, bold })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_are_absolute_and_platform_specific() {
        let c = candidate_fonts();
        assert!(!c.is_empty());
        for (r, _) in &c {
            assert!(r.is_absolute() || std::env::var_os(FONT_ENV_VAR).is_some(), "{}", r.display());
        }
        if cfg!(windows) {
            assert!(c.iter().any(|(r, _)| r.ends_with("segoeui.ttf")));
        }
    }

    #[test]
    fn missing_files_are_skipped() {
        assert!(load_checked(Path::new("/no/such/font.ttf")).is_none());
    }
}
