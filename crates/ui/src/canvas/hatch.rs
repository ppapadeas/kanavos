//! Hatching for display: the segments come from the shared `kanavos_drawing::hatch` (so the screen shows exactly
//! what prints), and this module adds the density figures the canvas needs for its level of detail (line spacing
//! and ink coverage, to show a flat tone when the lines would merge on screen).

use kanavos_drawing::hatch::{HatchOptions, hatch_segments, line_families, pattern_weight_mm};
use kanavos_drawing::{HatchPattern, Pt};

/// Most segments expanded for one area on screen; denser areas show their tone at every zoom.
pub const MAX_SEGMENTS: usize = 60_000;

/// Hatch geometry of one area.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hatch {
    /// Segments in drawing units, clipped to the area (even-odd). Dots are zero-length segments.
    pub segments: Vec<[Pt; 2]>,
    /// Typical distance between neighbouring lines or marks, drawing units.
    pub spacing: f64,
    /// Fraction of the area covered by ink when printed (0..1).
    pub coverage: f64,
    /// Pen width of the pattern, paper millimetres.
    pub weight_mm: f64,
}

/// Typical spacing (paper mm at scale 1) of patterns that are not line families: the size of their marks' cells.
fn motif_spacing_mm(pattern: HatchPattern) -> f64 {
    match pattern {
        HatchPattern::Concrete => 1.0,
        HatchPattern::Earth => 1.3,
        HatchPattern::Gravel => 1.5,
        HatchPattern::Insulation => 2.0,
        _ => 0.0,
    }
}

/// Spacing (paper mm at scale 1) and ink coverage of a pattern.
pub fn pattern_density(pattern: HatchPattern) -> (f64, f64) {
    let w = pattern_weight_mm(pattern);
    match line_families(pattern) {
        Some(families) => {
            let spacing = families.iter().map(|f| f.spacing).fold(f64::INFINITY, f64::min);
            let coverage: f64 = families
                .iter()
                .map(|f| {
                    let on: f64 = f.dashes.iter().step_by(2).sum();
                    let period: f64 = f.dashes.iter().sum();
                    let dash = if period > 0.0 { (on.max(w) / period).min(1.0) } else { 1.0 };
                    w / f.spacing.max(1e-9) * dash
                })
                .sum();
            (if spacing.is_finite() { spacing } else { 0.0 }, coverage)
        }
        None => match pattern {
            HatchPattern::None => (0.0, 0.0),
            HatchPattern::Concrete => (motif_spacing_mm(pattern), 0.07),
            HatchPattern::Earth => (motif_spacing_mm(pattern), 0.08),
            HatchPattern::Gravel => (motif_spacing_mm(pattern), 0.12),
            _ => (motif_spacing_mm(pattern), 0.10),
        },
    }
}

/// Hatches an area for display.
///
/// `rings` are closed rings in drawing units (even-odd), `paper_unit` is one paper millimetre in drawing units,
/// `scale` and `angle` come from the fill. Areas that would need more than [`MAX_SEGMENTS`] segments get none but
/// still report spacing and coverage, so the canvas shows their tone.
pub fn hatch_area(
    pattern: HatchPattern,
    scale: f64,
    angle: f64,
    rings: &[Vec<Pt>],
    paper_unit: f64,
) -> Hatch {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let unit = if paper_unit.is_finite() && paper_unit > 0.0 { paper_unit } else { 1.0 };
    let (spacing_mm, coverage) = pattern_density(pattern);
    let segments = if pattern == HatchPattern::None {
        Vec::new()
    } else {
        hatch_segments(pattern, rings, unit, scale, angle, &HatchOptions { max_segments: MAX_SEGMENTS })
            .unwrap_or_default()
    };
    Hatch {
        segments,
        spacing: spacing_mm * scale * unit,
        coverage: (coverage / scale).clamp(0.0, 0.6),
        weight_mm: pattern_weight_mm(pattern),
    }
}

/// Even-odd point-in-area test over rings.
pub fn inside(rings: &[&Vec<Pt>], p: Pt) -> bool {
    let mut c = false;
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (Some(a), Some(b)) = (r.get(i), r.get((i + 1) % n)) else { continue };
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if p.x < x {
                    c = !c;
                }
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::pt;

    fn square(size: f64) -> Vec<Vec<Pt>> {
        vec![vec![pt(0.0, 0.0), pt(size, 0.0), pt(size, size), pt(0.0, size)]]
    }

    #[test]
    fn segments_come_from_the_shared_hatch_and_stay_inside() {
        let rings = square(10.0);
        let r: Vec<&Vec<Pt>> = rings.iter().collect();
        for pattern in [
            HatchPattern::Diagonal,
            HatchPattern::Masonry,
            HatchPattern::CrossDiagonal,
            HatchPattern::Concrete,
            HatchPattern::Insulation,
            HatchPattern::Gravel,
            HatchPattern::Earth,
            HatchPattern::Dots,
        ] {
            let h = hatch_area(pattern, 1.0, 0.0, &rings, 1.0);
            assert!(!h.segments.is_empty(), "{pattern:?}");
            assert!(h.spacing > 0.0 && h.coverage > 0.0, "{pattern:?}");
            for [a, b] in &h.segments {
                let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                assert!(inside(&r, m), "{pattern:?}: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn spacing_follows_scale_and_paper_unit() {
        // 1:50 model drawing: one paper mm is 0.05 m; diagonal lines every 2 mm at scale 2 are 0.2 m apart.
        let h = hatch_area(HatchPattern::Diagonal, 2.0, 0.0, &square(2.0), 0.05);
        assert!((h.spacing - 0.2).abs() < 1e-12);
    }

    #[test]
    fn too_dense_areas_get_a_tone_only() {
        let h = hatch_area(HatchPattern::Masonry, 1.0, 0.0, &square(1000.0), 0.01);
        assert!(h.segments.is_empty());
        assert!(h.coverage > 0.0);
    }

    #[test]
    fn no_pattern_no_lines() {
        let h = hatch_area(HatchPattern::None, 1.0, 0.0, &square(1.0), 1.0);
        assert!(h.segments.is_empty() && h.coverage <= 0.0);
    }
}
