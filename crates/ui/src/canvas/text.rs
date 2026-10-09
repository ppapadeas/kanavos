//! Drawing text: anchored, rotated single-line text at its true printed size.
//!
//! Text is laid out every frame (egui caches the layouts) at a font size quantised to small steps, so that zooming
//! does not fill the font atlas with every possible size. Text smaller than [`MIN_TEXT_PT`] on screen is hidden.

use std::sync::{Arc, OnceLock};

use egui::epaint::TextShape;
use egui::{Color32, FontFamily, FontId, Pos2, Vec2, pos2, vec2};
use kanavos_drawing::Text;

/// Cap height as a fraction of the font size (typical for sans-serif faces).
pub const CAP_RATIO: f64 = 0.7;
/// Text whose cap height is below this many points is not drawn.
pub const MIN_TEXT_PT: f64 = 2.8;
/// Text is never drawn larger than this font size (points).
pub const MAX_FONT_PT: f64 = 400.0;

/// Name of the bold family registered by [`install_bold`].
const BOLD_FAMILY: &str = "kanavos-drawing-bold";

/// Candidate bold faces with Greek coverage.
const BOLD_CANDIDATES: &[&str] = &[
    "C:/Windows/Fonts/segoeuib.ttf",
    "C:/Windows/Fonts/arialbd.ttf",
    "C:/Windows/Fonts/tahomabd.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
];

/// Registers a bold system face (once per process) for bold drawing text. Without one, bold text uses the
/// regular face.
pub fn install_bold(ctx: &egui::Context) {
    static BYTES: OnceLock<Option<Arc<Vec<u8>>>> = OnceLock::new();
    let bytes = BYTES.get_or_init(|| {
        BOLD_CANDIDATES
            .iter()
            .find(|p| std::path::Path::new(p).exists())
            .and_then(|p| std::fs::read(p).ok())
            .map(Arc::new)
    });
    let Some(bytes) = bytes else { return };
    let has = ctx.fonts(|f| f.definitions().font_data.contains_key(BOLD_FAMILY));
    if !has {
        ctx.add_font(egui::epaint::text::FontInsert::new(
            BOLD_FAMILY,
            egui::FontData::from_owned(bytes.as_ref().clone()),
            vec![egui::epaint::text::InsertFontFamily {
                family: FontFamily::Name(BOLD_FAMILY.into()),
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
    }
}

/// The bold family, if it is available in this frame's fonts.
pub fn bold_family(ctx: &egui::Context) -> Option<FontFamily> {
    let fam = FontFamily::Name(BOLD_FAMILY.into());
    ctx.fonts(|f| f.definitions().families.contains_key(&fam)).then_some(fam)
}

/// Font size in points for a cap height in points, quantised: half-point steps up to 16 pt, then steps of about
/// 4 %.
pub fn quantised_font_size(cap_pt: f64) -> f32 {
    let size = (cap_pt / CAP_RATIO).min(MAX_FONT_PT);
    let q = if size <= 16.0 { (size * 2.0).round() / 2.0 } else { (size.ln() / 0.04).round() * 0.04 };
    let q = if size > 16.0 { q.exp() } else { q };
    q.max(1.0) as f32
}

/// The position of a text galley's top-left corner such that the anchor point of the text box (from the
/// baseline to the cap height) lands on `at`, with the text rotated `angle` radians clockwise on screen.
pub fn galley_origin(
    at: Pos2,
    size: Vec2,
    baseline: f32,
    cap: f32,
    h_frac: f32,
    v_frac: f32,
    angle: f32,
) -> Pos2 {
    let local = vec2(h_frac * size.x, baseline - v_frac * cap);
    let (s, c) = angle.sin_cos();
    let rotated = vec2(local.x * c - local.y * s, local.x * s + local.y * c);
    at - rotated
}

/// Paints one text item at `at` (screen) with the given cap height in points. Returns `false` when it is too
/// small to draw.
pub fn paint_text(
    painter: &egui::Painter,
    t: &Text,
    at: Pos2,
    cap_pt: f64,
    color: Color32,
    bold: Option<&FontFamily>,
) -> bool {
    if !(cap_pt.is_finite() && cap_pt >= MIN_TEXT_PT) {
        return false;
    }
    let size = quantised_font_size(cap_pt);
    let family = match (t.bold, bold) {
        (true, Some(f)) => f.clone(),
        _ => FontFamily::Proportional,
    };
    let galley = painter.layout_no_wrap(t.content.clone(), FontId::new(size, family), color);
    let baseline = galley
        .rows
        .first()
        .map(|r| r.pos.y + r.glyphs.first().map(|g| g.pos.y).unwrap_or(r.size.y * 0.8))
        .unwrap_or(size * 0.8);
    let cap = cap_pt as f32;
    let angle = if t.rotation.is_finite() { -(t.rotation as f32) } else { 0.0 };
    let origin = galley_origin(
        at,
        galley.size(),
        baseline,
        cap,
        t.anchor.h_fraction() as f32,
        t.anchor.v_fraction() as f32,
        angle,
    );
    painter.add(TextShape::new(pos2(origin.x, origin.y), galley, color).with_angle(angle));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_sizes_are_quantised() {
        let a = quantised_font_size(7.0);
        let b = quantised_font_size(7.1);
        assert!((a - 10.0).abs() < 1e-3 && (b - 10.0).abs() < 0.6);
        let big = quantised_font_size(70.0);
        assert!((big - 100.0).abs() < 3.0, "{big}");
        assert!(quantised_font_size(1e9) <= MAX_FONT_PT as f32 * 1.05);
    }

    #[test]
    fn anchors_land_on_the_point_unrotated() {
        // Text 100 × 20 with baseline at 16 and cap height 10: bottom-centre anchor.
        let o = galley_origin(pos2(500.0, 300.0), vec2(100.0, 20.0), 16.0, 10.0, 0.5, 0.0, 0.0);
        assert!((o.x - 450.0).abs() < 1e-4 && (o.y - 284.0).abs() < 1e-4);
        // Centre anchor: half the cap height above the baseline.
        let o = galley_origin(pos2(500.0, 300.0), vec2(100.0, 20.0), 16.0, 10.0, 0.5, 0.5, 0.0);
        assert!((o.y - 289.0).abs() < 1e-4);
    }

    #[test]
    fn rotated_anchor_stays_on_the_point() {
        let at = pos2(10.0, 10.0);
        let angle = -std::f32::consts::FRAC_PI_2;
        let o = galley_origin(at, vec2(40.0, 12.0), 9.0, 6.0, 0.0, 0.0, angle);
        // Rotating the local anchor (0, 9) by the angle and adding it to the origin gives the point back.
        let (s, c) = angle.sin_cos();
        let back = pos2(o.x + (0.0 * c - 9.0 * s), o.y + (0.0 * s + 9.0 * c));
        assert!((back.x - at.x).abs() < 1e-4 && (back.y - at.y).abs() < 1e-4);
    }
}
