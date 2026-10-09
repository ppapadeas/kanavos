//! Hatch patterns expanded into line segments.
//!
//! A [`Fill::Hatch`] names a [`HatchPattern`]; this module turns it into straight segments clipped to the filled
//! area, so every exporter draws exactly the same hatch. The pattern definitions are in
//! `specs/drawing/hatch-patterns.md`. Summary:
//!
//! - **Line families** (diagonal, masonry, cross-diagonal, wood, grid, dots, horizontal): parallel lines at an
//!   angle with a spacing, optionally dashed and staggered. They are clipped with an exact even-odd scanline, so a
//!   line through a vertex is never doubled or lost.
//! - **Motifs** (concrete, earth, gravel): small figures repeated on a tile grid. Concrete and gravel vary each tile
//!   with a fixed hash of the tile's grid position, so they look irregular but the output is identical on every
//!   run and every machine. Each figure is clipped to the area.
//! - **Insulation band**: a zigzag that runs along the area's longest edge and spans its full thickness, the usual
//!   way to show soft insulation in a build-up. Thick areas get several rows.
//!
//! All lengths are paper millimetres, converted to drawing units by the caller's `unit` (drawing units per paper
//! millimetre, see [`Drawing::paper_mm`]) and multiplied by the fill's `scale`. Patterns are anchored at the drawing
//! origin, so hatches of neighbouring areas line up.
//!
//! Areas follow the even-odd rule: every ring toggles inside/outside, so holes need no special orientation.

use std::f64::consts::{FRAC_PI_4, TAU};
use std::fmt;

use crate::geometry::{FLATTEN_TOLERANCE_MM, fill_rings};
use crate::{Drawing, Fill, HatchPattern, Item, Pt, Rgba, pt, weights};

/// Default pen width of hatch lines, paper millimetres.
pub const HATCH_WEIGHT_MM: f64 = weights::EXTRA_FINE;

/// Default limit on segments for one hatched area.
pub const DEFAULT_MAX_SEGMENTS: usize = 250_000;

/// Error from hatch expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HatchError {
    /// The hatch would need more segments than allowed (area huge compared with the pattern spacing).
    TooDense {
        /// The limit that was exceeded.
        limit: usize,
    },
}

impl fmt::Display for HatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HatchError::TooDense { limit } => {
                write!(f, "hatch too dense for the area: more than {limit} segments")
            }
        }
    }
}

impl std::error::Error for HatchError {}

/// Limits for hatch expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HatchOptions {
    /// Maximum segments for one area; beyond it expansion fails with [`HatchError::TooDense`].
    pub max_segments: usize,
}

impl Default for HatchOptions {
    fn default() -> Self {
        HatchOptions { max_segments: DEFAULT_MAX_SEGMENTS }
    }
}

/// A family of parallel hatch lines, in paper millimetres at scale 1.
#[derive(Debug, Clone, PartialEq)]
pub struct LineFamily {
    /// Direction of the lines, degrees counter-clockwise from +X.
    pub angle_deg: f64,
    /// Perpendicular distance between lines.
    pub spacing: f64,
    /// Shift along the line direction from one line to the next (staggers dashes).
    pub shift: f64,
    /// On/off lengths starting with a dash; empty for continuous lines.
    pub dashes: Vec<f64>,
}

impl LineFamily {
    fn solid(angle_deg: f64, spacing: f64) -> LineFamily {
        LineFamily { angle_deg, spacing, shift: 0.0, dashes: Vec::new() }
    }
}

/// The line families of a pattern, or `None` for patterns that are not made of parallel lines (none, concrete,
/// insulation, earth, gravel).
pub fn line_families(pattern: HatchPattern) -> Option<Vec<LineFamily>> {
    use HatchPattern as H;
    Some(match pattern {
        H::Diagonal => vec![LineFamily::solid(45.0, 2.0)],
        H::Masonry => vec![LineFamily::solid(45.0, 0.8)],
        H::CrossDiagonal => vec![LineFamily::solid(45.0, 1.6), LineFamily::solid(135.0, 1.6)],
        H::Wood => vec![LineFamily { angle_deg: 0.0, spacing: 1.2, shift: 3.1, dashes: vec![7.0, 1.0] }],
        H::Grid => vec![LineFamily::solid(0.0, 3.0), LineFamily::solid(90.0, 3.0)],
        H::Dots => vec![LineFamily { angle_deg: 0.0, spacing: 1.2, shift: 0.6, dashes: vec![0.2, 1.0] }],
        H::Horizontal => vec![LineFamily::solid(0.0, 1.5)],
        H::None | H::Concrete | H::Insulation | H::Earth | H::Gravel => return None,
    })
}

/// Pen width of a pattern's lines, paper millimetres.
pub fn pattern_weight_mm(pattern: HatchPattern) -> f64 {
    match pattern {
        HatchPattern::Dots | HatchPattern::Concrete => weights::FINE,
        _ => HATCH_WEIGHT_MM,
    }
}

type P = [f64; 2];

fn rot(p: P, c: f64, s: f64) -> P {
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// Expands a hatch pattern over an area into segments in drawing coordinates.
///
/// - `rings`: closed rings (first point not repeated) combined with the even-odd rule;
/// - `unit`: drawing units per paper millimetre ([`Drawing::paper_mm`]`(1.0)`);
/// - `scale`: spacing multiplier (1 = standard); `angle`: extra rotation in radians.
///
/// Rings with fewer than three points or non-finite coordinates are ignored. Invalid `unit`, `scale` or `angle`
/// values fall back to 1, 1 and 0. Output is deterministic.
pub fn hatch_segments(
    pattern: HatchPattern,
    rings: &[Vec<Pt>],
    unit: f64,
    scale: f64,
    angle: f64,
    opts: &HatchOptions,
) -> Result<Vec<[Pt; 2]>, HatchError> {
    let unit = if unit.is_finite() && unit > 0.0 { unit } else { 1.0 };
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let angle = if angle.is_finite() { angle } else { 0.0 };
    let s = unit * scale;
    let rings: Vec<Vec<P>> = rings
        .iter()
        .filter(|r| r.len() >= 3 && r.iter().all(|p| p.x.is_finite() && p.y.is_finite()))
        .map(|r| r.iter().map(|p| [p.x, p.y]).collect())
        .collect();
    if rings.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<[P; 2]> = Vec::new();
    let limit = opts.max_segments;
    if let Some(families) = line_families(pattern) {
        for f in &families {
            let theta = angle + f.angle_deg.to_radians();
            let dashes: Vec<f64> = f.dashes.iter().map(|d| d * s).collect();
            family(&rings, theta, f.spacing * s, f.shift * s, &dashes, &mut out, limit)?;
        }
    } else {
        match pattern {
            HatchPattern::Concrete => motif(&rings, angle, 5.0, 5.0, s, concrete_tile, &mut out, limit)?,
            HatchPattern::Gravel => motif(&rings, angle, 3.0, 3.0, s, gravel_tile, &mut out, limit)?,
            HatchPattern::Earth => motif(&rings, angle, 4.0, 4.0, s, earth_tile, &mut out, limit)?,
            HatchPattern::Insulation => insulation(&rings, angle, s, &mut out, limit)?,
            _ => {}
        }
    }
    Ok(out.into_iter().map(|[a, b]| [pt(a[0], a[1]), pt(b[0], b[1])]).collect())
}

/// Rings rotated by `-theta` and their bounding box.
fn rotate_rings(rings: &[Vec<P>], theta: f64) -> (Vec<Vec<P>>, [f64; 4]) {
    let (s, c) = (-theta).sin_cos();
    let mut bb = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let rr = rings
        .iter()
        .map(|r| {
            r.iter()
                .map(|p| {
                    let q = rot(*p, c, s);
                    bb = [bb[0].min(q[0]), bb[1].min(q[1]), bb[2].max(q[0]), bb[3].max(q[1])];
                    q
                })
                .collect()
        })
        .collect();
    (rr, bb)
}

fn edges(rings: &[Vec<P>]) -> impl Iterator<Item = (P, P)> + '_ {
    rings.iter().flat_map(|r| {
        let n = r.len();
        (0..n).filter_map(move |i| Some((*r.get(i)?, *r.get((i + 1) % n)?)))
    })
}

/// One family of parallel lines at `theta`, clipped by exact even-odd scanlines.
fn family(
    rings: &[Vec<P>],
    theta: f64,
    spacing: f64,
    shift: f64,
    dashes: &[f64],
    out: &mut Vec<[P; 2]>,
    limit: usize,
) -> Result<(), HatchError> {
    if !(spacing.is_finite() && spacing > 0.0) {
        return Ok(());
    }
    let (rr, bb) = rotate_rings(rings, theta);
    let k_lo = (bb[1] / spacing).ceil();
    let k_hi = (bb[3] / spacing).floor();
    if !(k_lo.is_finite() && k_hi.is_finite()) || k_hi < k_lo {
        return Ok(());
    }
    let count = k_hi - k_lo + 1.0;
    if count > limit as f64 {
        return Err(HatchError::TooDense { limit });
    }
    let n = count as usize;
    let k0 = k_lo as i64;
    let mut crossings: Vec<Vec<f64>> = vec![Vec::new(); n];
    for (a, b) in edges(&rr) {
        let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
        if y1 <= y0 {
            continue; // horizontal edges never cross a scanline under the half-open rule
        }
        let first = ((y0 / spacing).ceil() as i64 - 1).max(k0);
        let last = ((y1 / spacing).ceil() as i64).min(k0 + n as i64 - 1);
        for k in first..=last {
            let y = k as f64 * spacing;
            // Half-open rule: an edge covers [min, max), so a vertex on the line counts once.
            if (a[1] <= y) != (b[1] <= y) {
                let x = a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                if let Some(c) = usize::try_from(k - k0).ok().and_then(|i| crossings.get_mut(i)) {
                    c.push(x);
                }
            }
        }
    }
    let (s, c) = theta.sin_cos();
    let period: f64 = dashes.iter().sum();
    let dashed = dashes.len() >= 2 && period > 0.0 && dashes.iter().all(|d| d.is_finite() && *d >= 0.0);
    for (i, xs) in crossings.iter_mut().enumerate() {
        if xs.len() < 2 {
            continue;
        }
        xs.sort_by(f64::total_cmp);
        let k = k0 + i as i64;
        let y = k as f64 * spacing;
        let phase = (k as f64 * shift).rem_euclid(if dashed { period } else { 1.0 });
        for pair in xs.chunks_exact(2) {
            let [x0, x1] = [pair.first().copied().unwrap_or(0.0), pair.get(1).copied().unwrap_or(0.0)];
            if x1 - x0 <= 0.0 {
                continue;
            }
            if !dashed {
                out.push([rot([x0, y], c, s), rot([x1, y], c, s)]);
            } else {
                // Dash i of period n covers [n·P + a_i − phase, n·P + b_i − phase].
                let mut n = ((x0 + phase) / period).floor() - 1.0;
                loop {
                    let base = n * period - phase;
                    if base > x1 {
                        break;
                    }
                    let mut a = 0.0;
                    for (j, len) in dashes.iter().enumerate() {
                        let b = a + len;
                        if j % 2 == 0 {
                            let (d0, d1) = ((base + a).max(x0), (base + b).min(x1));
                            if d1 > d0 {
                                out.push([rot([d0, y], c, s), rot([d1, y], c, s)]);
                            }
                        }
                        a = b;
                    }
                    n += 1.0;
                    if out.len() > limit {
                        return Err(HatchError::TooDense { limit });
                    }
                }
            }
        }
        if out.len() > limit {
            return Err(HatchError::TooDense { limit });
        }
    }
    Ok(())
}

/// Spatial index of ring edges in horizontal bands, for clipping and point-in-area tests.
struct EdgeIndex {
    edges: Vec<(P, P)>,
    y0: f64,
    y1: f64,
    x0: f64,
    x1: f64,
    band: f64,
    bands: Vec<Vec<usize>>,
    stamp: Vec<u32>,
    epoch: u32,
}

impl EdgeIndex {
    fn new(rings: &[Vec<P>]) -> EdgeIndex {
        let edges: Vec<(P, P)> = edges(rings).collect();
        let mut bb = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for (a, _) in &edges {
            bb = [bb[0].min(a[0]), bb[1].min(a[1]), bb[2].max(a[0]), bb[3].max(a[1])];
        }
        let nb = (edges.len() / 2).clamp(1, 2048);
        let height = (bb[3] - bb[1]).max(1e-300);
        let band = height / nb as f64;
        let mut bands = vec![Vec::new(); nb];
        for (i, (a, b)) in edges.iter().enumerate() {
            let lo = Self::index(bb[1], band, nb, a[1].min(b[1]));
            let hi = Self::index(bb[1], band, nb, a[1].max(b[1]));
            for slot in bands.iter_mut().take(hi + 1).skip(lo) {
                slot.push(i);
            }
        }
        let stamp = vec![0; edges.len()];
        EdgeIndex { edges, y0: bb[1], y1: bb[3], x0: bb[0], x1: bb[2], band, bands, stamp, epoch: 0 }
    }

    fn index(y0: f64, band: f64, nb: usize, y: f64) -> usize {
        let i = ((y - y0) / band).floor();
        if i.is_finite() && i > 0.0 { (i as usize).min(nb - 1) } else { 0 }
    }

    /// Even-odd point-in-area test (horizontal ray towards +X).
    fn inside(&self, p: P) -> bool {
        if p[1] < self.y0 || p[1] > self.y1 || p[0] < self.x0 || p[0] > self.x1 {
            return false;
        }
        let Some(band) = self.bands.get(Self::index(self.y0, self.band, self.bands.len(), p[1])) else {
            return false;
        };
        let mut inside = false;
        for &i in band {
            let Some(&(a, b)) = self.edges.get(i) else { continue };
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                if x > p[0] {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Clips a segment to the area: splits it at every boundary crossing and keeps the pieces whose midpoint is
    /// inside. Adjacent kept pieces are merged.
    fn clip(&mut self, a: P, b: P, out: &mut Vec<[P; 2]>) {
        let (ymin, ymax) = (a[1].min(b[1]), a[1].max(b[1]));
        let (xmin, xmax) = (a[0].min(b[0]), a[0].max(b[0]));
        if ymax < self.y0 || ymin > self.y1 || xmax < self.x0 || xmin > self.x1 {
            return;
        }
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamp.iter_mut().for_each(|s| *s = 0);
            self.epoch = 1;
        }
        let nb = self.bands.len();
        let lo = Self::index(self.y0, self.band, nb, ymin.max(self.y0));
        let hi = Self::index(self.y0, self.band, nb, ymax.min(self.y1));
        let d = [b[0] - a[0], b[1] - a[1]];
        let mut ts: Vec<f64> = vec![0.0, 1.0];
        for band in self.bands.iter().take(hi + 1).skip(lo) {
            for &i in band {
                let Some(st) = self.stamp.get_mut(i) else { continue };
                if *st == self.epoch {
                    continue;
                }
                *st = self.epoch;
                let Some(&(q0, q1)) = self.edges.get(i) else { continue };
                let e = [q1[0] - q0[0], q1[1] - q0[1]];
                let den = d[0] * e[1] - d[1] * e[0];
                if den.abs() <= 1e-300 {
                    continue;
                }
                let w = [q0[0] - a[0], q0[1] - a[1]];
                let t = (w[0] * e[1] - w[1] * e[0]) / den;
                let u = (w[0] * d[1] - w[1] * d[0]) / den;
                if t > 0.0 && t < 1.0 && (0.0..=1.0).contains(&u) {
                    ts.push(t);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        let at = |t: f64| [a[0] + d[0] * t, a[1] + d[1] * t];
        let mut open: Option<f64> = None;
        let mut prev = 0.0;
        for &t in ts.iter().skip(1) {
            if t - prev > 1e-12 {
                let inside = self.inside(at((prev + t) / 2.0));
                match (inside, open) {
                    (true, None) => open = Some(prev),
                    (false, Some(s)) => {
                        out.push([at(s), at(prev)]);
                        open = None;
                    }
                    _ => {}
                }
            }
            prev = t;
        }
        if let Some(s) = open {
            out.push([at(s), at(1.0)]);
        }
    }
}

/// Small deterministic random sequence (SplitMix64), seeded from a tile position.
struct Rng(u64);

impl Rng {
    fn new(i: i64, j: i64, salt: u64) -> Rng {
        let seed = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as u64).rotate_left(29) ^ salt;
        let mut r = Rng(seed);
        r.next_u64();
        r
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

/// Concrete: in a 5 mm tile of 3 × 3 cells, each cell holds a dot (60 %), a small triangle (25 %) or nothing.
fn concrete_tile(i: i64, j: i64, out: &mut Vec<[P; 2]>) {
    let mut rng = Rng::new(i, j, 0xC0C0);
    let cell = 5.0 / 3.0;
    for cy in 0..3 {
        for cx in 0..3 {
            let (ox, oy) = (cx as f64 * cell, cy as f64 * cell);
            let kind = rng.unit();
            if kind < 0.25 {
                let side = rng.range(0.6, 0.9);
                let r = side / 3f64.sqrt();
                let m = r + 0.15;
                let c = [ox + rng.range(m, cell - m), oy + rng.range(m, cell - m)];
                let a0 = rng.range(0.0, TAU);
                let v: Vec<P> = (0..3)
                    .map(|k| {
                        let a = a0 + k as f64 * TAU / 3.0;
                        [c[0] + r * a.cos(), c[1] + r * a.sin()]
                    })
                    .collect();
                if let [p0, p1, p2] = v.as_slice() {
                    out.extend([[*p0, *p1], [*p1, *p2], [*p2, *p0]]);
                }
            } else if kind < 0.85 {
                let c = [ox + rng.range(0.3, cell - 0.3), oy + rng.range(0.3, cell - 0.3)];
                let a = rng.range(0.0, TAU);
                let h = [0.1 * a.cos(), 0.1 * a.sin()];
                out.push([[c[0] - h[0], c[1] - h[1]], [c[0] + h[0], c[1] + h[1]]]);
            } else {
                // Leave the cell empty, but keep the sequence aligned.
                rng.unit();
            }
        }
    }
}

/// Gravel: in a 3 mm tile of 2 × 2 cells, one small circle per cell with varying size and position.
fn gravel_tile(i: i64, j: i64, out: &mut Vec<[P; 2]>) {
    let mut rng = Rng::new(i, j, 0x6A7E);
    let cell = 1.5;
    for cy in 0..2 {
        for cx in 0..2 {
            let r = rng.range(0.28, 0.5);
            let m = r + 0.08;
            let c = [cx as f64 * cell + rng.range(m, cell - m), cy as f64 * cell + rng.range(m, cell - m)];
            let a0 = rng.range(0.0, TAU);
            let n = 10;
            let pts: Vec<P> = (0..=n)
                .map(|k| {
                    let a = a0 + k as f64 * TAU / n as f64;
                    [c[0] + r * a.cos(), c[1] + r * a.sin()]
                })
                .collect();
            out.extend(pts.windows(2).filter_map(|w| match w {
                [p, q] => Some([*p, *q]),
                _ => None,
            }));
        }
    }
}

/// Earth: in a 4 mm tile, two groups of three short strokes at 45°, on the tile's diagonal.
fn earth_tile(_i: i64, _j: i64, out: &mut Vec<[P; 2]>) {
    // Strokes 1.3 mm long at 45°.
    let v = [1.3 * FRAC_PI_4.cos(), 1.3 * FRAC_PI_4.sin()];
    for (gx, gy) in [(0.3, 0.3), (2.3, 2.3)] {
        for m in 0..3 {
            let s = [gx + 0.45 * m as f64, gy];
            out.push([s, [s[0] + v[0], s[1] + v[1]]]);
        }
    }
}

/// Motif patterns: figures from `tile(i, j)` on a grid of `w × h` tiles (paper mm, scaled by `s`), rotated by
/// `theta` about the origin, each segment clipped to the area.
#[allow(clippy::too_many_arguments)]
fn motif(
    rings: &[Vec<P>],
    theta: f64,
    w: f64,
    h: f64,
    s: f64,
    tile: fn(i64, i64, &mut Vec<[P; 2]>),
    out: &mut Vec<[P; 2]>,
    limit: usize,
) -> Result<(), HatchError> {
    let (rr, bb) = rotate_rings(rings, theta);
    let (tw, th) = (w * s, h * s);
    let (i0, i1) = ((bb[0] / tw).floor(), (bb[2] / tw).floor());
    let (j0, j1) = ((bb[1] / th).floor(), (bb[3] / th).floor());
    if ![i0, i1, j0, j1].iter().all(|v| v.is_finite()) {
        return Ok(());
    }
    // Every tile yields at most ~40 segments.
    if (i1 - i0 + 1.0) * (j1 - j0 + 1.0) * 10.0 > limit as f64 {
        return Err(HatchError::TooDense { limit });
    }
    let mut index = EdgeIndex::new(&rr);
    let (sn, cs) = theta.sin_cos();
    let mut figs = Vec::new();
    let mut clipped = Vec::new();
    for j in (j0 as i64)..=(j1 as i64) {
        for i in (i0 as i64)..=(i1 as i64) {
            figs.clear();
            tile(i, j, &mut figs);
            let (ox, oy) = (i as f64 * tw, j as f64 * th);
            for [a, b] in &figs {
                let a = [ox + a[0] * s, oy + a[1] * s];
                let b = [ox + b[0] * s, oy + b[1] * s];
                clipped.clear();
                index.clip(a, b, &mut clipped);
                out.extend(clipped.iter().map(|[p, q]| [rot(*p, cs, sn), rot(*q, cs, sn)]));
            }
            if out.len() > limit {
                return Err(HatchError::TooDense { limit });
            }
        }
    }
    Ok(())
}

/// Thickest band drawn as a single zigzag row, paper mm.
const INSULATION_MAX_ROW_MM: f64 = 15.0;
/// Row height for thick insulation areas, paper mm.
const INSULATION_ROW_MM: f64 = 8.0;
/// Zigzag period as a fraction of the row height.
const INSULATION_PERIOD: f64 = 0.6;

/// Insulation: a zigzag along the area's longest edge spanning its thickness.
fn insulation(
    rings: &[Vec<P>],
    angle: f64,
    s: f64,
    out: &mut Vec<[P; 2]>,
    limit: usize,
) -> Result<(), HatchError> {
    // Band direction: the longest edge (wall layers and slab layers are long thin quadrilaterals).
    let dir = edges(rings)
        .map(|(a, b)| ((b[0] - a[0]).hypot(b[1] - a[1]), (b[1] - a[1]).atan2(b[0] - a[0])))
        .filter(|(l, _)| l.is_finite())
        .max_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, a)| a)
        .unwrap_or(0.0);
    let theta = dir + angle;
    let (rr, bb) = rotate_rings(rings, theta);
    let thickness = bb[3] - bb[1];
    if !(thickness.is_finite() && thickness > 0.0) {
        return Ok(());
    }
    let rows = if thickness <= INSULATION_MAX_ROW_MM * s {
        1.0
    } else {
        (thickness / (INSULATION_ROW_MM * s)).ceil()
    };
    let row_h = thickness / rows;
    // A zig every 0.6 × the band height: steep enough to read as insulation, open enough to stay legible.
    let period = row_h * INSULATION_PERIOD;
    let half = period / 2.0;
    let steps = ((bb[2] - bb[0]) / half).ceil() + 4.0;
    if !steps.is_finite() || steps * rows > limit as f64 {
        return Err(HatchError::TooDense { limit });
    }
    let mut index = EdgeIndex::new(&rr);
    let (sn, cs) = theta.sin_cos();
    let mut clipped = Vec::new();
    // Anchor the zigzag on the period grid so that neighbouring areas in the same direction line up.
    let x_start = (bb[0] / period).floor() * period - period;
    for r in 0..(rows as i64) {
        let y_lo = bb[1] + r as f64 * row_h;
        let y_hi = y_lo + row_h;
        let mut prev: Option<P> = None;
        for k in 0..(steps as i64 + 1) {
            let x = x_start + k as f64 * half;
            let p = [x, if k % 2 == 0 { y_lo } else { y_hi }];
            if let Some(q) = prev {
                clipped.clear();
                index.clip(q, p, &mut clipped);
                out.extend(clipped.iter().map(|[a, b]| [rot(*a, cs, sn), rot(*b, cs, sn)]));
            }
            prev = Some(p);
            if x > bb[2] + period {
                break;
            }
        }
        if out.len() > limit {
            return Err(HatchError::TooDense { limit });
        }
    }
    Ok(())
}

/// Hatch lines ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct HatchLines {
    /// Segments in drawing coordinates.
    pub segments: Vec<[Pt; 2]>,
    /// Line colour.
    pub color: Rgba,
    /// Pen width, paper millimetres.
    pub weight_mm: f64,
}

/// Expands a fill over rings in a drawing. Returns `None` for solid fills and for hatches without lines
/// ([`HatchPattern::None`]); the background colour of a hatch is drawn by the caller.
pub fn expand_fill(
    fill: &Fill,
    rings: &[Vec<Pt>],
    drawing: &Drawing,
    opts: &HatchOptions,
) -> Result<Option<HatchLines>, HatchError> {
    let Fill::Hatch { pattern, color, scale, angle, .. } = fill else { return Ok(None) };
    if *pattern == HatchPattern::None {
        return Ok(None);
    }
    let segments = hatch_segments(*pattern, rings, drawing.paper_mm(1.0), *scale, *angle, opts)?;
    Ok(Some(HatchLines { segments, color: *color, weight_mm: pattern_weight_mm(*pattern) }))
}

/// Expands the hatch of an item (polygon, closed polyline or circle with a hatch fill).
pub fn expand_item(
    item: &Item,
    drawing: &Drawing,
    opts: &HatchOptions,
) -> Result<Option<HatchLines>, HatchError> {
    let Some(fill) = &item.fill else { return Ok(None) };
    let rings = fill_rings(&item.shape, drawing.paper_mm(FLATTEN_TOLERANCE_MM));
    expand_fill(fill, &rings, drawing, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
        vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)]
    }

    fn opts() -> HatchOptions {
        HatchOptions::default()
    }

    /// Even-odd containment for checking results.
    fn inside(rings: &[Vec<Pt>], p: Pt) -> bool {
        let mut c = false;
        for r in rings {
            for i in 0..r.len() {
                let (a, b) = (r[i], r[(i + 1) % r.len()]);
                if (a.y > p.y) != (b.y > p.y) && a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) > p.x {
                    c = !c;
                }
            }
        }
        c
    }

    fn all_inside(rings: &[Vec<Pt>], segs: &[[Pt; 2]]) {
        for [a, b] in segs {
            let m = a.midpoint(*b);
            assert!(inside(rings, m), "segment midpoint {m:?} outside");
        }
    }

    #[test]
    fn every_pattern_hatches_a_square_with_a_hole() {
        let rings = vec![rect(0.0, 0.0, 40.0, 40.0), rect(10.0, 10.0, 30.0, 30.0)];
        use HatchPattern as H;
        for p in [
            H::Diagonal,
            H::Masonry,
            H::CrossDiagonal,
            H::Concrete,
            H::Insulation,
            H::Wood,
            H::Grid,
            H::Dots,
            H::Earth,
            H::Gravel,
            H::Horizontal,
        ] {
            let segs = hatch_segments(p, &rings, 1.0, 1.0, 0.0, &opts()).unwrap();
            assert!(segs.len() > 10, "{p:?} gave {} segments", segs.len());
            all_inside(&rings, &segs);
            // Nothing inside the hole.
            assert!(segs.iter().all(|[a, b]| !(a.midpoint(*b).x > 10.01
                && a.midpoint(*b).x < 29.99
                && a.midpoint(*b).y > 10.01
                && a.midpoint(*b).y < 29.99)));
            // Deterministic.
            assert_eq!(segs, hatch_segments(p, &rings, 1.0, 1.0, 0.0, &opts()).unwrap());
        }
        assert!(hatch_segments(H::None, &rings, 1.0, 1.0, 0.0, &opts()).unwrap().is_empty());
    }

    #[test]
    fn diagonal_lines_have_the_right_spacing_and_cover_the_area() {
        // 45° lines at 2 mm over a 20 mm square: total length = area / spacing = 200 mm.
        let rings = vec![rect(0.0, 0.0, 20.0, 20.0)];
        let segs = hatch_segments(HatchPattern::Diagonal, &rings, 1.0, 1.0, 0.0, &opts()).unwrap();
        let total: f64 = segs.iter().map(|[a, b]| a.distance(*b)).sum();
        assert!((total - 200.0).abs() < 3.0, "total {total}");
        for [a, b] in &segs {
            let ang = (b.y - a.y).atan2(b.x - a.x).to_degrees().rem_euclid(180.0);
            assert!((ang - 45.0).abs() < 1e-9);
        }
    }

    #[test]
    fn scanline_through_vertices_is_not_doubled() {
        // A diamond whose vertices lie exactly on horizontal hatch lines (spacing 1.5).
        let rings = vec![vec![pt(0.0, 0.0), pt(3.0, 3.0), pt(0.0, 6.0), pt(-3.0, 3.0)]];
        let segs = hatch_segments(HatchPattern::Horizontal, &rings, 1.0, 1.0, 0.0, &opts()).unwrap();
        // Lines at y = 1.5, 3.0, 4.5 (y = 0 and 6 touch single vertices and give nothing).
        assert_eq!(segs.len(), 3, "{segs:?}");
        let mid = segs.iter().find(|[a, _]| (a.y - 3.0).abs() < 1e-9).unwrap();
        assert!((mid[0].distance(mid[1]) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn spacing_follows_scale_and_unit() {
        let rings = vec![rect(0.0, 0.0, 1.0, 1.0)];
        // 1:50 in metres: 1.5 mm → 0.075 m; scale 2 → 0.15 m.
        let segs = hatch_segments(HatchPattern::Horizontal, &rings, 0.05, 2.0, 0.0, &opts()).unwrap();
        let mut ys: Vec<f64> = segs.iter().map(|[a, _]| a.y).collect();
        ys.sort_by(f64::total_cmp);
        assert!((ys[1] - ys[0] - 0.15).abs() < 1e-9);
        // Extra rotation turns horizontal lines vertical.
        let segs =
            hatch_segments(HatchPattern::Horizontal, &rings, 0.05, 1.0, FRAC_PI_4 * 2.0, &opts()).unwrap();
        assert!(segs.iter().all(|[a, b]| (a.x - b.x).abs() < 1e-9));
    }

    #[test]
    fn dashed_families_are_staggered() {
        let rings = vec![rect(0.0, 0.0, 30.0, 10.0)];
        let segs = hatch_segments(HatchPattern::Wood, &rings, 1.0, 1.0, 0.0, &opts()).unwrap();
        assert!(segs.iter().all(|[a, b]| a.distance(*b) <= 7.0 + 1e-9));
        let starts = |y: f64| -> Vec<f64> {
            let mut v: Vec<f64> =
                segs.iter().filter(|[a, _]| (a.y - y).abs() < 1e-9 && a.x > 0.0).map(|[a, _]| a.x).collect();
            v.sort_by(f64::total_cmp);
            v
        };
        assert_ne!(starts(1.2).first(), starts(2.4).first());
    }

    #[test]
    fn insulation_follows_the_band() {
        // A 10 m × 0.1 m layer at 1:20 (unit 0.02 m per mm) sloping at 30°.
        let a = 30f64.to_radians();
        let (c, s) = (a.cos(), a.sin());
        let r = |x: f64, y: f64| pt(x * c - y * s, x * s + y * c);
        let rings = vec![vec![r(0.0, 0.0), r(10.0, 0.0), r(10.0, 0.1), r(0.0, 0.1)]];
        let segs = hatch_segments(HatchPattern::Insulation, &rings, 0.02, 1.0, 0.0, &opts()).unwrap();
        all_inside(&rings, &segs);
        // One row: every zig spans the full 0.1 m thickness, so most segments are long diagonals.
        let full = segs
            .iter()
            .filter(|[p, q]| {
                let (pp, qq) =
                    ([p.x * c + p.y * s, -p.x * s + p.y * c], [q.x * c + q.y * s, -q.x * s + q.y * c]);
                (pp[1] - qq[1]).abs() > 0.099
            })
            .count();
        assert!(full > 150, "{full} full-height zigs");
        // A thick area gets several rows.
        let thick = vec![rect(0.0, 0.0, 100.0, 50.0)];
        let segs = hatch_segments(HatchPattern::Insulation, &thick, 1.0, 1.0, 0.0, &opts()).unwrap();
        let heights: Vec<f64> = segs.iter().map(|[a, b]| (a.y - b.y).abs()).collect();
        assert!(heights.iter().cloned().fold(0.0, f64::max) < 8.0 + 1e-9);
    }

    #[test]
    fn concave_and_degenerate_input() {
        // An L-shape.
        let l = vec![vec![
            pt(0.0, 0.0),
            pt(20.0, 0.0),
            pt(20.0, 5.0),
            pt(5.0, 5.0),
            pt(5.0, 20.0),
            pt(0.0, 20.0),
        ]];
        for p in [HatchPattern::Diagonal, HatchPattern::Gravel, HatchPattern::Concrete] {
            let segs = hatch_segments(p, &l, 1.0, 1.0, 0.3, &opts()).unwrap();
            all_inside(&l, &segs);
        }
        // Degenerate rings are ignored.
        let bad = vec![vec![pt(0.0, 0.0), pt(1.0, 1.0)], vec![pt(f64::NAN, 0.0), pt(1.0, 0.0), pt(1.0, 1.0)]];
        assert!(hatch_segments(HatchPattern::Diagonal, &bad, 1.0, 1.0, 0.0, &opts()).unwrap().is_empty());
        // Invalid parameters fall back to defaults.
        let sq = vec![rect(0.0, 0.0, 10.0, 10.0)];
        let a = hatch_segments(HatchPattern::Diagonal, &sq, f64::NAN, -1.0, f64::INFINITY, &opts()).unwrap();
        let b = hatch_segments(HatchPattern::Diagonal, &sq, 1.0, 1.0, 0.0, &opts()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn huge_areas_are_refused() {
        let big = vec![rect(0.0, 0.0, 1e6, 1e6)];
        let o = HatchOptions { max_segments: 10_000 };
        for p in [HatchPattern::Masonry, HatchPattern::Concrete, HatchPattern::Insulation, HatchPattern::Dots]
        {
            assert_eq!(
                hatch_segments(p, &big, 1.0, 1.0, 0.0, &o),
                Err(HatchError::TooDense { limit: 10_000 })
            );
        }
    }

    #[test]
    fn items_expand_in_drawing_units() {
        let d = Drawing::model(50.0);
        let item = Item::polygon(
            vec![rect(0.0, 0.0, 1.0, 0.25)],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            None,
            "wall.cut",
        );
        let h = expand_item(&item, &d, &opts()).unwrap().unwrap();
        assert_eq!(h.color, Rgba::BLACK);
        assert!((h.weight_mm - HATCH_WEIGHT_MM).abs() < 1e-12);
        // 45° lines at 0.8 mm × 0.05 m/mm = 0.04 m over 0.25 m²: ≈ 6.25 m of line.
        let total: f64 = h.segments.iter().map(|[a, b]| a.distance(*b)).sum();
        assert!((total - 6.25).abs() < 0.2, "{total}");
        let solid = Item::polygon(vec![rect(0.0, 0.0, 1.0, 1.0)], Some(Fill::Solid(Rgba::BLACK)), None, "x");
        assert!(expand_item(&solid, &d, &opts()).unwrap().is_none());
    }

    #[test]
    fn neighbouring_areas_line_up() {
        // Two squares side by side: lines continue across the shared edge.
        let a = hatch_segments(HatchPattern::Diagonal, &[rect(0.0, 0.0, 10.0, 10.0)], 1.0, 1.0, 0.0, &opts())
            .unwrap();
        let b =
            hatch_segments(HatchPattern::Diagonal, &[rect(10.0, 0.0, 20.0, 10.0)], 1.0, 1.0, 0.0, &opts())
                .unwrap();
        // Points on the shared edge, excluding its corners (a line through a corner only touches the neighbour).
        let ends: Vec<f64> = a
            .iter()
            .flat_map(|s| s.iter())
            .filter(|p| (p.x - 10.0).abs() < 1e-9 && p.y > 0.01 && p.y < 9.99)
            .map(|p| p.y)
            .collect();
        let starts: Vec<f64> =
            b.iter().flat_map(|s| s.iter()).filter(|p| (p.x - 10.0).abs() < 1e-9).map(|p| p.y).collect();
        assert!(!ends.is_empty());
        for y in ends {
            assert!(starts.iter().any(|s| (s - y).abs() < 1e-9), "line ending at y = {y} does not continue");
        }
    }
}
