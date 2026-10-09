//! Line patterns applied to polylines.
//!
//! Most exporters draw dashes natively (SVG, PDF and the raster renderer accept dash arrays). This module is for
//! consumers that need the dashes as geometry: renderers without dash support and exports that must explode line
//! types into separate segments.
//!
//! The pattern runs **continuously along the whole polyline**: a dash that reaches a vertex carries on around the
//! corner instead of restarting, so corners do not collect short stubs and the rhythm stays even. Each dash is
//! returned as a polyline that may contain corner vertices.

use crate::geometry::{FLATTEN_TOLERANCE_MM, stroke_paths};
use crate::{Drawing, Pt, Shape, Stroke, pt};

/// Upper bound on dashes produced for one polyline; beyond it the line is returned solid.
pub const MAX_DASHES: usize = 1_000_000;

/// Normalises a dash array: an odd number of entries is repeated (as SVG and PDF do); negative or non-finite
/// lengths, or a pattern whose total length is not positive, give `None` (draw solid).
fn normalise(pattern: &[f64]) -> Option<Vec<f64>> {
    if pattern.is_empty() || pattern.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let mut p = pattern.to_vec();
    if p.len() % 2 == 1 {
        p.extend_from_slice(pattern);
    }
    let total: f64 = p.iter().sum();
    (total > 0.0).then_some(p)
}

/// Splits a polyline into dashes.
///
/// `pattern` holds on/off lengths in drawing units, starting with a dash; `phase` shifts the start of the
/// pattern along the line. A closed polyline includes its closing segment. Returns the dashes as polylines (a
/// zero-length dash becomes a two-point polyline with equal points, which renders as a dot with round caps).
/// An invalid pattern returns the whole polyline as one piece.
pub fn dash_polyline(points: &[Pt], closed: bool, pattern: &[f64], phase: f64) -> Vec<Vec<Pt>> {
    let mut path: Vec<Pt> = points.to_vec();
    if closed && let Some(first) = points.first().copied() {
        path.push(first);
    }
    if path.len() < 2 {
        return Vec::new();
    }
    let Some(pattern) = normalise(pattern) else { return vec![path] };
    let period: f64 = pattern.iter().sum();
    let total_len: f64 = path.windows(2).map(seg_len).sum();
    if !total_len.is_finite() || total_len / period > MAX_DASHES as f64 {
        return vec![path];
    }

    // Position in the pattern at the start of the line.
    let phase = if phase.is_finite() { phase.rem_euclid(period) } else { 0.0 };
    let mut idx = 0;
    let mut left = pattern.first().copied().unwrap_or(0.0);
    let mut skip = phase;
    while skip > 0.0 {
        if skip < left {
            left -= skip;
            skip = 0.0;
        } else {
            skip -= left;
            idx = (idx + 1) % pattern.len();
            left = pattern.get(idx).copied().unwrap_or(0.0);
        }
    }

    let mut out: Vec<Vec<Pt>> = Vec::new();
    let mut current: Vec<Pt> = Vec::new();
    let mut on = idx % 2 == 0;
    if on && let Some(p0) = path.first() {
        current.push(*p0);
    }
    for w in path.windows(2) {
        let (a, b) = match w {
            [a, b] => (*a, *b),
            _ => continue,
        };
        let len = seg_len(w);
        let mut t = 0.0; // distance travelled along this segment
        while len - t >= left {
            // The current pattern element ends inside this segment.
            t += left;
            let p = lerp(a, b, if len > 0.0 { t / len } else { 0.0 });
            if on {
                current.push(p);
                out.push(std::mem::take(&mut current));
            } else {
                current.push(p);
            }
            idx = (idx + 1) % pattern.len();
            left = pattern.get(idx).copied().unwrap_or(0.0);
            on = idx % 2 == 0;
            if !on {
                current.clear();
            }
            // Zero-length dash: a dot at this point.
            if on && left <= 0.0 {
                out.push(vec![p, p]);
                current.clear();
                idx = (idx + 1) % pattern.len();
                left = pattern.get(idx).copied().unwrap_or(0.0);
                on = idx % 2 == 0;
                if on {
                    current.push(p);
                }
            }
            if out.len() > MAX_DASHES {
                return vec![path];
            }
        }
        left -= len - t;
        if on {
            if current.is_empty() {
                current.push(a);
            }
            if current.last().is_none_or(|l| l.distance(b) > 0.0) {
                current.push(b);
            }
        }
    }
    // A dash that started exactly at the end of the line has no length: drop it.
    if on && current.len() >= 2 {
        out.push(current);
    }
    out.retain(|d| d.len() >= 2);
    out
}

fn seg_len(w: &[Pt]) -> f64 {
    match w {
        [a, b] => a.distance(*b),
        _ => 0.0,
    }
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// Dashes of a shape's outline for a stroke, in drawing units: the stroke's [`crate::LinePattern`] (paper mm) is
/// converted with [`Drawing::paper_mm`]. Solid strokes return the outline paths unchanged. Arcs and circles are
/// flattened first.
pub fn dash_shape(shape: &Shape, stroke: &Stroke, drawing: &Drawing) -> Vec<Vec<Pt>> {
    let tol = drawing.paper_mm(FLATTEN_TOLERANCE_MM);
    let dashes: Vec<f64> = stroke.pattern.dashes_mm().iter().map(|v| drawing.paper_mm(*v)).collect();
    stroke_paths(shape, tol)
        .into_iter()
        .flat_map(|(pts, closed)| {
            if dashes.is_empty() {
                let mut p = pts;
                if closed && let Some(f) = p.first().copied() {
                    p.push(f);
                }
                vec![p]
            } else {
                dash_polyline(&pts, closed, &dashes, 0.0)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LinePattern, weights};

    fn length(p: &[Pt]) -> f64 {
        p.windows(2).map(seg_len).sum()
    }

    #[test]
    fn straight_line_dashes_have_pattern_lengths() {
        let d = dash_polyline(&[pt(0.0, 0.0), pt(10.0, 0.0)], false, &[2.0, 1.0], 0.0);
        // 0-2, 3-5, 6-8, 9-10
        assert_eq!(d.len(), 4);
        assert!((length(&d[0]) - 2.0).abs() < 1e-12);
        assert!((d[1][0].x - 3.0).abs() < 1e-12);
        assert!((length(&d[3]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn dashes_continue_around_corners() {
        // L-shape 0→(3,0)→(3,3) with dash 4, gap 1: the first dash turns the corner.
        let d = dash_polyline(&[pt(0.0, 0.0), pt(3.0, 0.0), pt(3.0, 3.0)], false, &[4.0, 1.0], 0.0);
        assert_eq!(d[0].len(), 3, "dash keeps the corner vertex: {:?}", d[0]);
        assert!((length(&d[0]) - 4.0).abs() < 1e-12);
        assert!(d[0][2].distance(pt(3.0, 1.0)) < 1e-12);
        // Total dashed length: 4 + 1 (5..6 is the rest of the line) = 5.
        let total: f64 = d.iter().map(|x| length(x)).sum();
        assert!((total - 5.0).abs() < 1e-12);
    }

    #[test]
    fn closed_polylines_include_the_closing_segment() {
        let sq = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let d = dash_polyline(&sq, true, &[0.5, 0.5], 0.0);
        let total: f64 = d.iter().map(|x| length(x)).sum();
        assert!((total - 2.0).abs() < 1e-9, "half of the 4 m perimeter is drawn, got {total}");
    }

    #[test]
    fn phase_and_odd_patterns() {
        let d = dash_polyline(&[pt(0.0, 0.0), pt(10.0, 0.0)], false, &[2.0, 1.0], 1.5);
        assert!((d[0][1].x - 0.5).abs() < 1e-12);
        // An odd pattern [1] repeats to [1, 1].
        let d = dash_polyline(&[pt(0.0, 0.0), pt(4.0, 0.0)], false, &[1.0], 0.0);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn zero_length_dashes_become_dots() {
        let d = dash_polyline(&[pt(0.0, 0.0), pt(3.0, 0.0)], false, &[0.0, 1.0], 0.0);
        assert!(d.iter().all(|x| x.len() == 2 && x[0].distance(x[1]) < 1e-12));
        assert_eq!(d.len(), 4, "{d:?}");
    }

    #[test]
    fn invalid_patterns_draw_solid() {
        let line = [pt(0.0, 0.0), pt(1.0, 0.0)];
        assert_eq!(dash_polyline(&line, false, &[], 0.0), vec![line.to_vec()]);
        assert_eq!(dash_polyline(&line, false, &[0.0, 0.0], 0.0), vec![line.to_vec()]);
        assert_eq!(dash_polyline(&line, false, &[-1.0, 1.0], 0.0), vec![line.to_vec()]);
        assert_eq!(dash_polyline(&line, false, &[1e-12, 1e-12], 0.0), vec![line.to_vec()]);
        assert!(dash_polyline(&[pt(0.0, 0.0)], false, &[1.0, 1.0], 0.0).is_empty());
    }

    #[test]
    fn shapes_use_paper_millimetres() {
        // At 1:100 a 3 mm dash is 0.3 m.
        let d = Drawing::model(100.0);
        let s = Shape::Line { a: pt(0.0, 0.0), b: pt(10.0, 0.0) };
        let dashes = dash_shape(&s, &Stroke::patterned(weights::THIN, LinePattern::Dashed), &d);
        assert!((length(&dashes[0]) - 0.3).abs() < 1e-12);
        let solid = dash_shape(&s, &Stroke::solid(weights::THIN), &d);
        assert_eq!(solid.len(), 1);
    }
}
