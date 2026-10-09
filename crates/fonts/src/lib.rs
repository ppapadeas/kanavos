//! System font discovery, glyph outlines and a built-in single-stroke lettering font.
//!
//! Layer L0. See `docs/architecture.md`.
//!
//! Kanavos does not bundle fonts. Exports and the interface use a Greek-capable sans-serif font found on the
//! machine at run time (see [`discover_system_font`]); when none is found, text falls back to the built-in
//! single-stroke font in [`stroke`], which covers digits, Latin and Greek letters (with tonos and dialytika) and
//! the symbols used on drawings, so an export never fails for lack of a font.
//!
//! All geometry is returned in a local text frame: the pen starts at the origin on the baseline, X runs along
//! the text and Y points up. Sizes are given as a **cap height** (height of capital letters), the convention used
//! for lettering on technical drawings.

mod discover;
mod face;
pub mod stroke;

use std::sync::{Arc, OnceLock};

pub use discover::{FONT_BOLD_ENV_VAR, FONT_ENV_VAR, FontCandidate, candidate_fonts, discover_system_font};
pub use face::{FontError, FontFace};

/// A point in the local text frame `[x, y]`, Y up.
pub type P = [f64; 2];

/// One element of a filled outline path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathEl {
    /// Starts a new closed contour.
    MoveTo(P),
    /// Straight line to a point.
    LineTo(P),
    /// Quadratic Bézier curve: control point, end point.
    QuadTo(P, P),
    /// Cubic Bézier curve: two control points, end point.
    CubicTo(P, P, P),
    /// Closes the current contour.
    Close,
}

impl PathEl {
    /// The element with every point mapped through `f`.
    pub fn map(self, f: impl Fn(P) -> P) -> PathEl {
        match self {
            PathEl::MoveTo(p) => PathEl::MoveTo(f(p)),
            PathEl::LineTo(p) => PathEl::LineTo(f(p)),
            PathEl::QuadTo(c, p) => PathEl::QuadTo(f(c), f(p)),
            PathEl::CubicTo(c1, c2, p) => PathEl::CubicTo(f(c1), f(c2), f(p)),
            PathEl::Close => PathEl::Close,
        }
    }
}

/// Flattens outline paths into closed polygons (rings), approximating curves with line segments so that no
/// point of a curve is further than `tolerance` from its chords.
pub fn flatten_paths(paths: &[PathEl], tolerance: f64) -> Vec<Vec<P>> {
    let tol = if tolerance.is_finite() && tolerance > 0.0 { tolerance } else { 1e-3 };
    let mut rings: Vec<Vec<P>> = Vec::new();
    let mut current: Vec<P> = Vec::new();
    let mut last: P = [0.0, 0.0];
    let flush = |current: &mut Vec<P>, rings: &mut Vec<Vec<P>>| {
        if current.len() >= 3 {
            rings.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    for el in paths {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut current, &mut rings);
                current.push(p);
                last = p;
            }
            PathEl::LineTo(p) => {
                current.push(p);
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                let n = curve_steps(last, &[c], p, tol);
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let u = 1.0 - t;
                    current.push([
                        u * u * last[0] + 2.0 * u * t * c[0] + t * t * p[0],
                        u * u * last[1] + 2.0 * u * t * c[1] + t * t * p[1],
                    ]);
                }
                last = p;
            }
            PathEl::CubicTo(c1, c2, p) => {
                let n = curve_steps(last, &[c1, c2], p, tol);
                for i in 1..=n {
                    let t = i as f64 / n as f64;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    current.push([
                        a * last[0] + b * c1[0] + c * c2[0] + d * p[0],
                        a * last[1] + b * c1[1] + c * c2[1] + d * p[1],
                    ]);
                }
                last = p;
            }
            PathEl::Close => flush(&mut current, &mut rings),
        }
    }
    flush(&mut current, &mut rings);
    rings
}

/// Number of line segments needed for a Bézier curve: based on the control polygon's deviation from the chord.
fn curve_steps(start: P, controls: &[P], end: P, tol: f64) -> usize {
    let dev = controls.iter().map(|c| point_line_distance(*c, start, end)).fold(0.0_f64, f64::max);
    // A Bézier curve stays within 3/4 of its control polygon's deviation; subdividing into n pieces divides the
    // deviation by about n².
    let n = (0.75 * dev / tol).sqrt().ceil();
    if n.is_finite() { (n as usize).clamp(1, 64) } else { 1 }
}

fn point_line_distance(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-300 {
        return (p[0] - a[0]).hypot(p[1] - a[1]);
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
}

/// Geometry of a line of text in the local text frame (pen at the origin on the baseline, Y up).
///
/// Characters found in the outline font become filled contours in [`fills`](TextGeometry::fills) (non-zero
/// winding, as TrueType defines); characters drawn with the single-stroke font become polylines in
/// [`strokes`](TextGeometry::strokes), to be stroked with round caps and joins at
/// [`stroke_width`](TextGeometry::stroke_width).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextGeometry {
    /// Filled outlines (non-zero winding).
    pub fills: Vec<PathEl>,
    /// Width of an extra outline stroke drawn over [`fills`](TextGeometry::fills) to simulate bold when no bold
    /// face is available; `0.0` for none.
    pub fill_embolden: f64,
    /// Single-stroke polylines.
    pub strokes: Vec<Vec<P>>,
    /// Pen width for [`strokes`](TextGeometry::strokes).
    pub stroke_width: f64,
    /// Total advance width of the line.
    pub advance: f64,
    /// Cap height the text was laid out at.
    pub cap_height: f64,
    /// Depth of descenders below the baseline (positive).
    pub descent: f64,
}

impl TextGeometry {
    /// Whether the text produced no visible geometry.
    pub fn is_empty(&self) -> bool {
        self.fills.is_empty() && self.strokes.is_empty()
    }
}

/// The fonts used to draw text: an optional outline font (regular and bold) with the built-in stroke font as
/// fallback for missing faces and missing characters.
#[derive(Debug, Clone, Default)]
pub struct Fonts {
    regular: Option<FontFace>,
    bold: Option<FontFace>,
}

static SYSTEM: OnceLock<Fonts> = OnceLock::new();

impl Fonts {
    /// The fonts found on this machine (discovered once, then cached for the life of the process).
    ///
    /// Falls back to [`Fonts::builtin`] when no Greek-capable font is installed.
    pub fn system() -> &'static Fonts {
        SYSTEM.get_or_init(|| match discover_system_font() {
            Some(c) => Fonts { regular: Some(c.regular), bold: c.bold },
            None => Fonts::builtin(),
        })
    }

    /// Only the built-in single-stroke font. Output is identical on every machine, which makes it the right
    /// choice for tests and reproducible exports.
    pub fn builtin() -> Fonts {
        Fonts { regular: None, bold: None }
    }

    /// An explicit outline font, with an optional bold face.
    pub fn with_faces(regular: FontFace, bold: Option<FontFace>) -> Fonts {
        Fonts { regular: Some(regular), bold }
    }

    /// The outline font used for regular text, if any.
    pub fn regular(&self) -> Option<&FontFace> {
        self.regular.as_ref()
    }

    /// The outline font used for bold text, if any.
    pub fn bold(&self) -> Option<&FontFace> {
        self.bold.as_ref()
    }

    /// Bytes of the regular outline font and its index in a collection, for handing the same font to the
    /// interface toolkit.
    pub fn regular_bytes(&self) -> Option<(Arc<[u8]>, u32)> {
        self.regular.as_ref().map(|f| (f.data().clone(), f.index()))
    }

    /// Cap height as a fraction of the font size (em) for the regular face; the stroke font's ratio otherwise.
    pub fn cap_height_ratio(&self) -> f64 {
        self.regular.as_ref().map(FontFace::cap_height_em).unwrap_or(stroke::CAP_HEIGHT_EM)
    }

    /// Lays out one line of text at a cap height. Never fails: characters missing from the outline font are drawn
    /// with the stroke font, and characters missing there with a box.
    pub fn layout(&self, text: &str, cap_height: f64, bold: bool) -> TextGeometry {
        let cap_height = if cap_height.is_finite() && cap_height > 0.0 { cap_height } else { 0.0 };
        let (face, embolden) = match (bold, &self.regular, &self.bold) {
            (true, _, Some(b)) => (Some(b), 0.0),
            (true, Some(r), None) => (Some(r), cap_height * 0.06),
            (false, Some(r), _) => (Some(r), 0.0),
            (_, None, _) => (None, 0.0),
        };
        let mut geo = TextGeometry {
            cap_height,
            stroke_width: stroke::pen_width(cap_height, bold),
            descent: stroke::descent(cap_height),
            ..TextGeometry::default()
        };
        let mut x = 0.0;
        let parsed = face.and_then(FontFace::parsed);
        for c in text.chars().map(normalise_char) {
            if c.is_control() {
                continue;
            }
            if let (Some(f), Some(p)) = (face, parsed.as_ref())
                && let Some(adv) = f.outline_into(p, c, cap_height, x, &mut geo.fills)
            {
                x += adv;
                continue;
            }
            x += stroke::glyph_into(c, cap_height, x, &mut geo.strokes);
        }
        if let Some(f) = face {
            geo.descent = geo.descent.max(f.descent_em() * cap_height / f.cap_height_em().max(1e-9));
        }
        if !geo.fills.is_empty() {
            geo.fill_embolden = embolden;
        }
        geo.advance = x;
        geo
    }

    /// Advance width of one line of text at a cap height, without building outlines.
    pub fn measure(&self, text: &str, cap_height: f64, bold: bool) -> f64 {
        let cap_height = if cap_height.is_finite() && cap_height > 0.0 { cap_height } else { 0.0 };
        let face = if bold { self.bold.as_ref().or(self.regular.as_ref()) } else { self.regular.as_ref() };
        let parsed = face.and_then(FontFace::parsed);
        let mut x = 0.0;
        for c in text.chars().map(normalise_char) {
            if c.is_control() {
                continue;
            }
            if let (Some(f), Some(p)) = (face, parsed.as_ref())
                && let Some(adv) = f.advance_with(p, c, cap_height)
            {
                x += adv;
                continue;
            }
            x += stroke::advance(c, cap_height);
        }
        x
    }
}

/// Replaces characters that should be drawn as another one (tabs as spaces, non-breaking spaces as spaces).
fn normalise_char(c: char) -> char {
    match c {
        '\t' | '\u{a0}' | '\u{2007}' | '\u{202f}' => ' ',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_layout_is_deterministic_and_never_empty() {
        let f = Fonts::builtin();
        let a = f.layout("Κάτοψη ισογείου 1:50", 2.5, false);
        let b = f.layout("Κάτοψη ισογείου 1:50", 2.5, false);
        assert_eq!(a, b);
        assert!(a.fills.is_empty());
        assert!(!a.strokes.is_empty());
        assert!(a.advance > 20.0 && a.advance < 60.0, "advance {}", a.advance);
        assert!((a.stroke_width - 0.25).abs() < 1e-12);
        assert!((f.measure("Κάτοψη ισογείου 1:50", 2.5, false) - a.advance).abs() < 1e-9);
    }

    #[test]
    fn strokes_stay_within_the_line_box() {
        let f = Fonts::builtin();
        let g = f.layout("ΑΒΓΔ abcdefghijklmnopqrstuvwxyz ά ΐ Ώ 0123456789 ±°²³", 10.0, false);
        for s in &g.strokes {
            for p in s {
                assert!(p[1] >= -g.descent - 1e-9 && p[1] <= 12.0, "point {p:?} outside line box");
                assert!(p[0] >= -2.0 && p[0] <= g.advance + 1e-9, "point {p:?} outside advance");
            }
        }
    }

    #[test]
    fn invalid_heights_give_empty_geometry() {
        let f = Fonts::builtin();
        let g = f.layout("abc", f64::NAN, false);
        assert!(g.advance.abs() < 1e-12);
        assert!(g.strokes.iter().flatten().all(|p| p[0].abs() < 1e-12 && p[1].abs() < 1e-12));
    }

    #[test]
    fn flattening_a_quadratic_stays_close_to_the_curve() {
        let paths = [
            PathEl::MoveTo([0.0, 0.0]),
            PathEl::QuadTo([5.0, 10.0], [10.0, 0.0]),
            PathEl::LineTo([0.0, 0.0]),
            PathEl::Close,
        ];
        let rings = flatten_paths(&paths, 0.01);
        assert_eq!(rings.len(), 1);
        let ring = &rings[0];
        // The apex of the parabola is at (5, 5).
        let apex = ring.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        assert!((apex - 5.0).abs() < 0.02, "apex {apex}");
        assert!(ring.len() > 8);
    }

    #[test]
    fn system_fonts_render_greek_without_failing() {
        // Whatever is installed on the machine, layout must produce geometry for Greek text.
        let g = Fonts::system().layout("Κάτοψη ισογείου", 2.5, true);
        assert!(!g.is_empty());
        assert!(g.advance > 0.0);
    }
}
