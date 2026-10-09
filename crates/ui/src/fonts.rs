//! Interface fonts, loaded from the system at run time (no fonts are bundled; see ASSETS.md).

/// Candidate font files with Greek coverage, most preferred first.
const CANDIDATES: &[&str] = &[
    "C:/Windows/Fonts/segoeui.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "C:/Windows/Fonts/tahoma.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
];

/// Installs the first available system font for every egui font family. Returns the path used.
pub fn install(ctx: &egui::Context) -> Option<String> {
    let path = std::env::var("KANAVOS_FONT")
        .ok()
        .into_iter()
        .chain(CANDIDATES.iter().map(|s| (*s).to_owned()))
        .find(|p| std::path::Path::new(p).exists())?;
    let bytes = std::fs::read(&path).ok()?;
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert("ui".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("ui".into());
    }
    ctx.set_fonts(fonts);
    Some(path)
}
