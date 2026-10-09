//! Hatching: turning a hatch fill of a polygon into line segments clipped to the polygon.
//!
//! Local stand-in for the `hatch` module planned in `kanavos-drawing`; [`hatch_segments`] is the single entry point
//! so that it can be swapped for the shared one later.
//!
//! Pattern sizes are defined in paper millimetres (so hatches read the same at every scale) and follow general
//! drafting practice for cut materials (parallel lines for masonry, crossed lines for reinforced concrete, scattered
//! marks for plain concrete, a wave for soft insulation …). The definitions here are our own.

use kanavos_drawing::{HatchPattern, Pt, pt};

/// Most lines of one line family per polygon; denser families are dropped (the caller then shows a tone).
const MAX_LINES: usize = 4000;
/// Most marks (dots, triangles, circles) per polygon.
const MAX_MARKS: usize = 20_000;

/// Hatch geometry of one polygon.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hatch {
    /// Segments in drawing units, clipped to the polygon (even-odd rule). Dots are zero-length segments.
    pub segments: Vec<[Pt; 2]>,
    /// Smallest distance between neighbouring lines or marks, in drawing units. Callers show a flat tone instead
    /// of the segments when this gets too small on screen.
    pub spacing: f64,
    /// Fraction of the area covered by ink when printed (0..1), for the tone shown at small zoom.
    pub coverage: f64,
}

/// Line weight of hatch lines, in paper millimetres.
pub const HATCH_WEIGHT_MM: f64 = kanavos_drawing::weights::EXTRA_FINE;

/// One family of parallel lines: direction, spacing and optional dashes, all in paper millimetres.
#[derive(Debug, Clone, Copy)]
struct Family {
    angle_deg: f64,
    spacing: f64,
    /// On/off dash lengths (`[0.0, 0.0]` for solid lines).
    dash: [f64; 2],
    /// Shift of the dash phase from one line to the next.
    stagger: f64,
}

const fn solid(angle_deg: f64, spacing: f64) -> Family {
    Family { angle_deg, spacing, dash: [0.0, 0.0], stagger: 0.0 }
}

/// Scattered small marks.
#[derive(Debug, Clone, Copy)]
enum Marks {
    /// Dots on a jittered grid with this spacing.
    Dots(f64),
    /// Small triangles and dot pairs on a jittered grid with this spacing.
    Aggregate(f64),
    /// Small circles on a jittered grid with this spacing.
    Pebbles(f64),
}

/// Hatches a polygon.
///
/// - `pattern`, `scale` and `angle` come from `Fill::Hatch` (`scale` multiplies the spacing, `angle` adds a
///   rotation in radians).
/// - `rings` are closed rings in drawing units; the first is the outline, the rest are holes (even-odd rule).
/// - `paper_unit` is the length of one paper millimetre in drawing units (`Drawing::paper_mm(1.0)`).
///
/// Failure modes: degenerate rings (fewer than three points), non-finite input or non-positive sizes give an
/// empty hatch. Very dense patterns are capped (see [`MAX_LINES`]) and then return no segments but still report
/// spacing and coverage, so callers fall back to a tone.
pub fn hatch_segments(
    pattern: HatchPattern,
    scale: f64,
    angle: f64,
    rings: &[Vec<Pt>],
    paper_unit: f64,
) -> Hatch {
    let rings: Vec<&Vec<Pt>> = rings.iter().filter(|r| r.len() >= 3).collect();
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let angle = if angle.is_finite() { angle } else { 0.0 };
    if rings.is_empty() || !(paper_unit.is_finite() && paper_unit > 0.0) {
        return Hatch::default();
    }
    let unit = paper_unit * scale;
    let w = HATCH_WEIGHT_MM;
    let families: &[Family] = match pattern {
        HatchPattern::None => &[],
        HatchPattern::Diagonal => &[solid(45.0, 2.0)],
        HatchPattern::Masonry => &[solid(45.0, 1.0)],
        HatchPattern::CrossDiagonal => &[solid(45.0, 1.5), solid(-45.0, 1.5)],
        HatchPattern::Wood => &[solid(0.0, 1.2)],
        HatchPattern::Grid => &[solid(0.0, 3.0), solid(90.0, 3.0)],
        HatchPattern::Horizontal => &[solid(0.0, 0.8)],
        HatchPattern::Earth => &[Family { angle_deg: 45.0, spacing: 2.0, dash: [1.2, 1.6], stagger: 1.4 }],
        HatchPattern::Concrete | HatchPattern::Dots | HatchPattern::Gravel | HatchPattern::Insulation => &[],
    };
    let mut out = Hatch { segments: Vec::new(), spacing: f64::INFINITY, coverage: 0.0 };
    for f in families {
        let spacing = f.spacing * unit;
        out.spacing = out.spacing.min(spacing);
        let dash_frac = if f.dash[0] > 0.0 { f.dash[0] / (f.dash[0] + f.dash[1]) } else { 1.0 };
        out.coverage += w / f.spacing / scale * dash_frac;
        let dash = [f.dash[0] * unit, f.dash[1] * unit];
        line_family(
            &rings,
            f.angle_deg.to_radians() + angle,
            spacing,
            dash,
            f.stagger * unit,
            &mut out.segments,
        );
    }
    let marks = match pattern {
        HatchPattern::Dots => Some(Marks::Dots(1.0)),
        HatchPattern::Concrete => Some(Marks::Aggregate(3.0)),
        HatchPattern::Gravel => Some(Marks::Pebbles(2.2)),
        _ => None,
    };
    if let Some(m) = marks {
        let (spacing, coverage) = match m {
            Marks::Dots(s) => (s, 0.05),
            Marks::Aggregate(s) => (s, 0.06),
            Marks::Pebbles(s) => (s, 0.12),
        };
        out.spacing = out.spacing.min(spacing * unit);
        out.coverage += coverage;
        marks_in(&rings, m, unit, angle, &mut out.segments);
    }
    if pattern == HatchPattern::Insulation {
        let band = wave(&rings, unit, &mut out.segments);
        out.spacing = out.spacing.min(band);
        out.coverage += 0.1;
    }
    if !out.spacing.is_finite() {
        out.spacing = 0.0;
    }
    out.coverage = out.coverage.clamp(0.0, 0.6);
    out
}

/// Dot product of a point with a direction.
fn dot(p: Pt, d: (f64, f64)) -> f64 {
    p.x * d.0 + p.y * d.1
}

/// Edges of all rings as point pairs.
fn edges<'a>(rings: &'a [&'a Vec<Pt>]) -> impl Iterator<Item = (Pt, Pt)> + 'a {
    rings.iter().flat_map(|r| {
        let n = r.len();
        (0..n).filter_map(move |i| Some((*r.get(i)?, *r.get((i + 1) % n)?)))
    })
}

/// Even-odd point-in-polygon test over all rings.
pub fn inside(rings: &[&Vec<Pt>], p: Pt) -> bool {
    let mut c = false;
    for (a, b) in edges(rings) {
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x {
                c = !c;
            }
        }
    }
    c
}

/// Parallel lines at `angle` (radians) every `spacing`, clipped to the rings.
fn line_family(
    rings: &[&Vec<Pt>],
    angle: f64,
    spacing: f64,
    dash: [f64; 2],
    stagger: f64,
    out: &mut Vec<[Pt; 2]>,
) {
    if !(spacing.is_finite() && spacing > 0.0) {
        return;
    }
    let u = (angle.cos(), angle.sin());
    let n = (-u.1, u.0);
    let (mut smin, mut smax) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in rings {
        for p in r.iter() {
            let s = dot(*p, n);
            smin = smin.min(s);
            smax = smax.max(s);
        }
    }
    if !(smin.is_finite() && smax.is_finite()) {
        return;
    }
    // Lines sit half a step off the grid so that they never run exactly through grid-aligned vertices.
    let first = (smin / spacing - 0.5).ceil() as i64;
    let last = (smax / spacing - 0.5).floor() as i64;
    if last < first || (last - first) as usize > MAX_LINES {
        return;
    }
    let period = dash[0] + dash[1];
    let mut ts: Vec<f64> = Vec::new();
    for k in first..=last {
        let s = (k as f64 + 0.5) * spacing;
        ts.clear();
        for (a, b) in edges(rings) {
            let (sa, sb) = (dot(a, n), dot(b, n));
            if (sa > s) != (sb > s) {
                let f = (s - sa) / (sb - sa);
                let (ta, tb) = (dot(a, u), dot(b, u));
                ts.push(ta + (tb - ta) * f);
            }
        }
        ts.sort_by(f64::total_cmp);
        let at = |t: f64| pt(n.0 * s + u.0 * t, n.1 * s + u.1 * t);
        for pair in ts.chunks_exact(2) {
            let (Some(&t0), Some(&t1)) = (pair.first(), pair.get(1)) else { continue };
            if period > 0.0 && dash[0] > 0.0 {
                // Dashes aligned to a fixed phase along the line, shifted per row.
                let phase = (k as f64) * stagger;
                let mut d = ((t0 - phase) / period).floor() * period + phase;
                while d < t1 {
                    let (a, b) = (d.max(t0), (d + dash[0]).min(t1));
                    if b > a {
                        out.push([at(a), at(b)]);
                    }
                    d += period;
                }
            } else {
                out.push([at(t0), at(t1)]);
            }
        }
    }
}

/// A deterministic hash of grid coordinates to `[0, 1)`, for jitter.
fn jitter(i: i64, j: i64, salt: u64) -> f64 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Scattered marks on a jittered grid, kept only where the whole mark lies inside the rings.
fn marks_in(rings: &[&Vec<Pt>], marks: Marks, unit: f64, angle: f64, out: &mut Vec<[Pt; 2]>) {
    let spacing_mm = match marks {
        Marks::Dots(s) | Marks::Aggregate(s) | Marks::Pebbles(s) => s,
    };
    let spacing = spacing_mm * unit;
    let Some((min, max)) = bounds(rings) else { return };
    let i0 = (min.x / spacing).floor() as i64;
    let i1 = (max.x / spacing).ceil() as i64;
    let j0 = (min.y / spacing).floor() as i64;
    let j1 = (max.y / spacing).ceil() as i64;
    let count = (i1 - i0 + 1).saturating_mul(j1 - j0 + 1);
    if count <= 0 || count as usize > MAX_MARKS {
        return;
    }
    let (sa, ca) = angle.sin_cos();
    for i in i0..=i1 {
        for j in j0..=j1 {
            let c = pt(
                (i as f64 + 0.15 + 0.7 * jitter(i, j, 1)) * spacing,
                (j as f64 + 0.15 + 0.7 * jitter(i, j, 2)) * spacing,
            );
            let r = jitter(i, j, 3);
            let mark: Vec<[Pt; 2]> = match marks {
                Marks::Dots(_) => vec![[c, c]],
                Marks::Aggregate(_) => {
                    if r < 0.45 {
                        // A small triangle, randomly turned.
                        let size = (0.45 + 0.35 * jitter(i, j, 4)) * unit;
                        let turn = jitter(i, j, 5) * std::f64::consts::TAU;
                        let v: Vec<Pt> = (0..3)
                            .map(|k| {
                                let a = turn + f64::from(k) * std::f64::consts::TAU / 3.0;
                                pt(c.x + size * a.cos(), c.y + size * a.sin())
                            })
                            .collect();
                        match (v.first(), v.get(1), v.get(2)) {
                            (Some(&a), Some(&b), Some(&d)) => vec![[a, b], [b, d], [d, a]],
                            _ => Vec::new(),
                        }
                    } else if r < 0.8 {
                        let o = 0.3 * unit;
                        vec![[c, c], [pt(c.x + o * ca, c.y + o * sa), pt(c.x + o * ca, c.y + o * sa)]]
                    } else {
                        vec![[c, c]]
                    }
                }
                Marks::Pebbles(_) => {
                    let rad = (0.3 + 0.25 * r) * unit;
                    let n = 8;
                    (0..n)
                        .map(|k| {
                            let a0 = f64::from(k) * std::f64::consts::TAU / f64::from(n);
                            let a1 = f64::from(k + 1) * std::f64::consts::TAU / f64::from(n);
                            [
                                pt(c.x + rad * a0.cos(), c.y + rad * 0.8 * a0.sin()),
                                pt(c.x + rad * a1.cos(), c.y + rad * 0.8 * a1.sin()),
                            ]
                        })
                        .collect()
                }
            };
            if mark.iter().all(|[a, b]| inside(rings, *a) && inside(rings, *b)) {
                out.extend(mark);
            }
        }
    }
}

/// Bounding box of the rings.
fn bounds(rings: &[&Vec<Pt>]) -> Option<(Pt, Pt)> {
    let mut it = rings.iter().flat_map(|r| r.iter());
    let first = *it.next()?;
    let (mut min, mut max) = (first, first);
    for p in it {
        min = pt(min.x.min(p.x), min.y.min(p.y));
        max = pt(max.x.max(p.x), max.y.max(p.y));
    }
    Some((min, max))
}

/// Clips a segment to the rings (even-odd) and appends the inside pieces.
fn clip_segment(rings: &[&Vec<Pt>], a: Pt, b: Pt, out: &mut Vec<[Pt; 2]>) {
    let d = (b.x - a.x, b.y - a.y);
    let mut ts = vec![0.0, 1.0];
    for (p, q) in edges(rings) {
        let e = (q.x - p.x, q.y - p.y);
        let den = d.0 * e.1 - d.1 * e.0;
        if den.abs() < 1e-18 {
            continue;
        }
        let w = (p.x - a.x, p.y - a.y);
        let t = (w.0 * e.1 - w.1 * e.0) / den;
        let s = (w.0 * d.1 - w.1 * d.0) / den;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&s) {
            ts.push(t);
        }
    }
    ts.sort_by(f64::total_cmp);
    let at = |t: f64| pt(a.x + d.0 * t, a.y + d.1 * t);
    for w in ts.windows(2) {
        let (Some(&t0), Some(&t1)) = (w.first(), w.get(1)) else { continue };
        if t1 - t0 <= 1e-12 {
            continue;
        }
        if inside(rings, at((t0 + t1) / 2.0)) {
            out.push([at(t0), at(t1)]);
        }
    }
}

/// The soft-insulation wave: rows of a smooth zigzag running along the polygon's longest edge, each row as high
/// as the polygon is thick (at most 6 paper mm). Returns the row height in drawing units.
fn wave(rings: &[&Vec<Pt>], unit: f64, out: &mut Vec<[Pt; 2]>) -> f64 {
    let Some(outer) = rings.first() else { return 0.0 };
    let n = outer.len();
    let mut dir = (1.0, 0.0);
    let mut best = 0.0;
    for i in 0..n {
        let (Some(a), Some(b)) = (outer.get(i), outer.get((i + 1) % n)) else { continue };
        let len = a.distance(*b);
        if len > best {
            best = len;
            dir = ((b.x - a.x) / len, (b.y - a.y) / len);
        }
    }
    let nrm = (-dir.1, dir.0);
    let (mut tmin, mut tmax, mut smin, mut smax) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for p in outer.iter() {
        let (t, s) = (dot(*p, dir), dot(*p, nrm));
        tmin = tmin.min(t);
        tmax = tmax.max(t);
        smin = smin.min(s);
        smax = smax.max(s);
    }
    let thick = smax - smin;
    if !(thick.is_finite() && thick > 0.0 && tmax > tmin) {
        return 0.0;
    }
    let rows = (thick / (6.0 * unit)).ceil().max(1.0);
    let h = thick / rows;
    let period = h;
    let steps_per_period = 8;
    let total_steps = ((tmax - tmin) / period * f64::from(steps_per_period)).ceil();
    if !total_steps.is_finite() || total_steps * rows > MAX_MARKS as f64 {
        return h;
    }
    let total_steps = total_steps as i64;
    let at = |t: f64, s: f64| pt(dir.0 * t + nrm.0 * s, dir.1 * t + nrm.1 * s);
    for row in 0..rows as i64 {
        let s0 = smin + h * row as f64;
        let mut prev: Option<Pt> = None;
        for k in 0..=total_steps {
            let t = tmin + period * k as f64 / f64::from(steps_per_period);
            let phase = (t - tmin) / period * std::f64::consts::TAU;
            // A rounded zigzag: the sine pushed towards its peaks.
            let v = phase.sin();
            let shaped = v.signum() * v.abs().powf(0.6);
            let s = s0 + h * (0.5 + 0.42 * shaped);
            let p = at(t, s);
            if let Some(q) = prev {
                clip_segment(rings, q, p, out);
            }
            prev = Some(p);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(size: f64) -> Vec<Vec<Pt>> {
        vec![vec![pt(0.0, 0.0), pt(size, 0.0), pt(size, size), pt(0.0, size)]]
    }

    #[test]
    fn diagonal_lines_are_clipped_to_the_square() {
        // 10 × 10 mm square on paper, 2 mm spacing at 45°.
        let rings = square(10.0);
        let h = hatch_segments(HatchPattern::Diagonal, 1.0, 0.0, &rings, 1.0);
        assert!(!h.segments.is_empty());
        for [a, b] in &h.segments {
            for p in [a, b] {
                assert!(p.x > -1e-9 && p.x < 10.0 + 1e-9 && p.y > -1e-9 && p.y < 10.0 + 1e-9, "{p:?}");
            }
            // 45°: dx == dy.
            assert!(((b.x - a.x).abs() - (b.y - a.y).abs()).abs() < 1e-9);
        }
        // Total length ≈ area / spacing.
        let total: f64 = h.segments.iter().map(|[a, b]| a.distance(*b)).sum();
        assert!((total - 50.0).abs() < 6.0, "total {total}");
        assert!((h.spacing - 2.0).abs() < 1e-12);
    }

    #[test]
    fn holes_stay_empty() {
        let rings = vec![
            vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)],
            vec![pt(3.0, 3.0), pt(3.0, 7.0), pt(7.0, 7.0), pt(7.0, 3.0)],
        ];
        let h = hatch_segments(HatchPattern::Horizontal, 1.0, 0.0, &rings, 1.0);
        for [a, b] in &h.segments {
            let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            assert!(!(m.x > 3.0 && m.x < 7.0 && m.y > 3.0 && m.y < 7.0), "segment in the hole: {a:?} {b:?}");
        }
        assert!(!h.segments.is_empty());
    }

    #[test]
    fn spacing_follows_scale_and_paper_unit() {
        // 1:50 model drawing: 1 paper mm = 0.05 m.
        let rings = square(2.0);
        let h = hatch_segments(HatchPattern::Masonry, 2.0, 0.0, &rings, 0.05);
        assert!((h.spacing - 0.1).abs() < 1e-12);
    }

    #[test]
    fn concave_polygons_are_clipped_correctly() {
        // An L shape.
        let rings = vec![vec![
            pt(0.0, 0.0),
            pt(10.0, 0.0),
            pt(10.0, 4.0),
            pt(4.0, 4.0),
            pt(4.0, 10.0),
            pt(0.0, 10.0),
        ]];
        let h = hatch_segments(HatchPattern::CrossDiagonal, 1.0, 0.0, &rings, 1.0);
        let r: Vec<&Vec<Pt>> = rings.iter().collect();
        for [a, b] in &h.segments {
            let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            assert!(inside(&r, m), "outside: {a:?} {b:?}");
        }
    }

    #[test]
    fn marks_and_waves_stay_inside() {
        let rings = vec![vec![pt(0.0, 0.0), pt(30.0, 0.0), pt(30.0, 4.0), pt(0.0, 4.0)]];
        let r: Vec<&Vec<Pt>> = rings.iter().collect();
        for pattern in
            [HatchPattern::Concrete, HatchPattern::Gravel, HatchPattern::Dots, HatchPattern::Insulation]
        {
            let h = hatch_segments(pattern, 1.0, 0.0, &rings, 1.0);
            assert!(!h.segments.is_empty(), "{pattern:?}");
            for [a, b] in &h.segments {
                let m = pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                assert!(inside(&r, m), "{pattern:?} outside: {a:?} {b:?}");
            }
            assert!(h.coverage > 0.0 && h.spacing > 0.0);
        }
    }

    #[test]
    fn degenerate_input_gives_nothing() {
        let h = hatch_segments(HatchPattern::Diagonal, 1.0, 0.0, &[vec![pt(0.0, 0.0), pt(1.0, 1.0)]], 1.0);
        assert!(h.segments.is_empty());
        let h = hatch_segments(HatchPattern::Diagonal, 1.0, 0.0, &square(1.0), f64::NAN);
        assert!(h.segments.is_empty());
        let h = hatch_segments(HatchPattern::None, 1.0, 0.0, &square(1.0), 1.0);
        assert!(h.segments.is_empty());
    }

    #[test]
    fn dense_patterns_are_capped_but_report_spacing() {
        // 100 m square at 1 paper mm = 0.001 m: 100 000 lines would be needed.
        let h = hatch_segments(HatchPattern::Masonry, 1.0, 0.0, &square(100.0), 0.001);
        assert!(h.segments.is_empty());
        assert!(h.spacing > 0.0 && h.coverage > 0.0);
    }

    #[test]
    fn earth_lines_are_dashed() {
        let h = hatch_segments(HatchPattern::Earth, 1.0, 0.0, &square(20.0), 1.0);
        assert!(!h.segments.is_empty());
        assert!(h.segments.iter().all(|[a, b]| a.distance(*b) <= 1.2 + 1e-9));
    }
}
