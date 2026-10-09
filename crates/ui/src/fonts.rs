//! Interface fonts, loaded from the system at run time (no fonts are bundled; see ASSETS.md).
//!
//! Families:
//! - `Proportional`: the regular interface face (with Greek), then a symbol face for arrows and marks.
//! - `Monospace`: a fixed-width face for coordinates, then the regular face.
//! - `Name("strong")`: a semibold face for headings and totals, falling back to the regular face.
//!
//! `KANAVOS_FONT` overrides the regular face.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Regular faces with Greek coverage, most preferred first.
const REGULAR: &[&str] = &[
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/tahoma.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
];

/// Semibold faces.
const STRONG: &[&str] = &[
    "C:/Windows/Fonts/seguisb.ttf",
    "C:/Windows/Fonts/arialbd.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf",
    "/usr/share/fonts/noto/NotoSans-SemiBold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
];

/// Fixed-width faces.
const MONO: &[&str] = &[
    "C:/Windows/Fonts/consola.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    "/System/Library/Fonts/Menlo.ttc",
];

/// Symbol faces (arrows, geometric marks).
const SYMBOLS: &[&str] = &[
    "C:/Windows/Fonts/seguisym.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

fn first_readable<'a>(candidates: impl IntoIterator<Item = &'a str>) -> Option<(String, Vec<u8>)> {
    candidates.into_iter().find_map(|p| std::fs::read(p).ok().map(|b| (p.to_owned(), b)))
}

/// Installs system fonts for every egui font family Kanavos uses. Returns the regular face used, or `None`
/// when no usable font was found (the interface then shows no text).
pub fn install(ctx: &egui::Context) -> Option<String> {
    let custom = std::env::var("KANAVOS_FONT").ok();
    let regular = first_readable(custom.iter().map(String::as_str).chain(REGULAR.iter().copied()))?;
    let mut fonts = FontDefinitions::empty();
    let mut add = |name: &str, bytes: Vec<u8>| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
    };
    add("ui", regular.1);
    let strong = first_readable(STRONG.iter().copied()).map(|(_, b)| b);
    let has_strong = strong.is_some();
    if let Some(b) = strong {
        add("ui-strong", b);
    }
    let mono = first_readable(MONO.iter().copied()).map(|(_, b)| b);
    let has_mono = mono.is_some();
    if let Some(b) = mono {
        add("ui-mono", b);
    }
    let symbols = first_readable(SYMBOLS.iter().copied()).map(|(_, b)| b);
    let has_symbols = symbols.is_some();
    if let Some(b) = symbols {
        add("ui-symbols", b);
    }
    let with_symbols = |mut v: Vec<String>| {
        if has_symbols {
            v.push("ui-symbols".into());
        }
        v
    };
    fonts.families.insert(FontFamily::Proportional, with_symbols(vec!["ui".into()]));
    let mono_list = if has_mono { vec!["ui-mono".into(), "ui".into()] } else { vec!["ui".into()] };
    fonts.families.insert(FontFamily::Monospace, with_symbols(mono_list));
    let strong_list = if has_strong { vec!["ui-strong".into(), "ui".into()] } else { vec!["ui".into()] };
    fonts.families.insert(FontFamily::Name("strong".into()), with_symbols(strong_list));
    ctx.set_fonts(fonts);
    Some(regular.0)
}

/// The semibold font family.
pub fn strong() -> FontFamily {
    FontFamily::Name("strong".into())
}
