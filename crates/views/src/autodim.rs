//! Automatic exterior dimension chains for a storey (see `specs/views/annotation.md` §Automatic dimensions).
//!
//! On each side of the building, seen along its main axes, two chains are proposed: the façade chain (every
//! corner and every opening jamb that can be seen from that side) and the overall dimension. Tools turn them
//! into dimension elements through a command.

use kanavos_elements::{footprint, opening_placement, wall_layout};
use kanavos_geom::{Angle, Point2, Vec2};
use kanavos_model::{Dimension, Document, ElementKind, Id};

use crate::poly::{self, Contour, Shape};

/// Paper distance (mm) from the building to the first chain.
const FIRST_MM: f64 = 12.0;
/// Paper distance (mm) between chains.
const SPACING_MM: f64 = 8.0;
/// Points closer than this along a chain are merged, metres.
const MERGE: f64 = 0.005;

fn rotate(p: [f64; 2], a: f64) -> [f64; 2] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// Exterior dimension chains for a storey's walls, at distances suited to the storey's plan scale.
pub fn auto_dimensions(doc: &Document, storey: Id) -> Vec<Dimension> {
    let scale = doc.plan_view_of(storey).map(|v| f64::from(v.scale)).unwrap_or(50.0);
    let mm = |v: f64| v * scale / 1000.0;
    let mut shapes: Vec<Shape> = Vec::new();
    for e in doc.elements_on(storey) {
        match &e.kind {
            ElementKind::Wall(_) => {
                if let Ok(l) = wall_layout(doc, e.id) {
                    shapes.push(poly::shape_of_polygon(&l.outline));
                }
            }
            ElementKind::Column(_) => {
                if let Ok(Some(f)) = footprint(doc, e.id) {
                    shapes.push(poly::shape_of(&f));
                }
            }
            _ => {}
        }
    }
    let union = poly::union(&shapes);
    let outers: Vec<Contour> = union.iter().filter_map(|s| s.first().cloned()).collect();
    if outers.is_empty() {
        return Vec::new();
    }
    // Main axis: direction of the longest outline edge.
    let mut best = (0.0, 0.0);
    for c in &outers {
        let n = c.len();
        for i in 0..n {
            let (Some(a), Some(b)) = (c.get(i), c.get((i + 1) % n)) else { continue };
            let len = (b[0] - a[0]).hypot(b[1] - a[1]);
            if len > best.0 {
                best = (len, (b[1] - a[1]).atan2(b[0] - a[0]));
            }
        }
    }
    let axis = best.1.rem_euclid(std::f64::consts::FRAC_PI_2);
    let local: Vec<Contour> = outers.iter().map(|c| c.iter().map(|p| rotate(*p, -axis)).collect()).collect();
    let edges: Vec<([f64; 2], [f64; 2])> = local
        .iter()
        .flat_map(|c| {
            let n = c.len();
            (0..n).filter_map(move |i| Some((*c.get(i)?, *c.get((i + 1) % n)?)))
        })
        .collect();
    let on_outline = |q: [f64; 2]| edges.iter().any(|(a, b)| seg_dist(q, *a, *b) < 0.002);
    // Opening jambs on the outline.
    let mut jambs: Vec<[f64; 2]> = Vec::new();
    for e in doc.elements_on(storey) {
        if !matches!(e.kind, ElementKind::Opening(_)) {
            continue;
        }
        let Ok(pl) = opening_placement(doc, e.id) else { continue };
        let h = pl.width.as_m() / 2.0;
        for face in [pl.left_face, pl.right_face] {
            for s in [-h, h] {
                let q = pl.centre + pl.along * s + pl.across * face;
                let lq = rotate(q.xy(), -axis);
                if on_outline(lq) {
                    jambs.push(lq);
                }
            }
        }
    }
    let (mut xmin, mut ymin, mut xmax, mut ymax) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in local.iter().flatten() {
        xmin = xmin.min(p[0]);
        ymin = ymin.min(p[1]);
        xmax = xmax.max(p[0]);
        ymax = ymax.max(p[1]);
    }
    let mut out = Vec::new();
    // Sides: outward normal, coordinate measured along the side (0 = x, 1 = y), position of the first line.
    let sides: [([f64; 2], usize, f64); 4] = [
        ([0.0, -1.0], 0, ymin - mm(FIRST_MM)),
        ([1.0, 0.0], 1, xmax + mm(FIRST_MM)),
        ([0.0, 1.0], 0, ymax + mm(FIRST_MM)),
        ([-1.0, 0.0], 1, xmin - mm(FIRST_MM)),
    ];
    for (normal, axis_i, first) in sides {
        let step = if normal.get(axis_i ^ 1).copied().unwrap_or(0.0) < 0.0 {
            -mm(SPACING_MM)
        } else {
            mm(SPACING_MM)
        };
        // Façade points: vertices of edges facing this side, and jambs, that can be seen from outside.
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for (a, b) in &edges {
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = dx.hypot(dy);
            if len < 1e-9 {
                continue;
            }
            let out_n = [dy / len, -dx / len];
            if out_n[0] * normal[0] + out_n[1] * normal[1] > 0.7 {
                pts.push(*a);
                pts.push(*b);
            }
        }
        pts.extend(jambs.iter().copied());
        pts.retain(|q| visible_from(*q, normal, &edges));
        let coord = |q: &[f64; 2]| q.get(axis_i).copied().unwrap_or(0.0);
        pts.sort_by(|a, b| coord(a).total_cmp(&coord(b)));
        pts.dedup_by(|a, b| (coord(a) - coord(b)).abs() < MERGE);
        // Overall: the outermost outline points along the side.
        let all: Vec<[f64; 2]> = local.iter().flatten().copied().collect();
        let lo = all.iter().copied().min_by(|a, b| coord(a).total_cmp(&coord(b)));
        let hi = all.iter().copied().max_by(|a, b| coord(a).total_cmp(&coord(b)));
        let mut line = first;
        let push = |points: &[[f64; 2]], at: f64, out: &mut Vec<Dimension>| {
            let line_local = if axis_i == 0 { [0.0, at] } else { [at, 0.0] };
            let dir = if axis_i == 0 { axis } else { axis + std::f64::consts::FRAC_PI_2 };
            let w = |q: [f64; 2]| {
                let r = rotate(q, axis);
                Point2::new(r[0], r[1])
            };
            out.push(Dimension {
                points: points.iter().map(|q| w(*q)).collect(),
                line_point: w(line_local),
                direction: Angle::rad(dir),
            });
        };
        let overall: Vec<[f64; 2]> = lo.into_iter().chain(hi).collect();
        if pts.len() > 2 {
            push(&pts, line, &mut out);
            line += step;
        }
        if overall.len() == 2
            && overall.first().zip(overall.last()).is_some_and(|(a, b)| (coord(a) - coord(b)).abs() > MERGE)
        {
            push(&overall, line, &mut out);
        }
    }
    out
}

fn seg_dist(q: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((q[0] - a[0]) * dx + (q[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (q[0] - a[0] - dx * t).hypot(q[1] - a[1] - dy * t)
}

/// Whether a ray from `q` in direction `n` leaves the outline without crossing it.
fn visible_from(q: [f64; 2], n: [f64; 2], edges: &[([f64; 2], [f64; 2])]) -> bool {
    let start = [q[0] + n[0] * 1e-4, q[1] + n[1] * 1e-4];
    let dir = Vec2::new(n[0], n[1]);
    for (a, b) in edges {
        let e = Vec2::new(b[0] - a[0], b[1] - a[1]);
        let den = dir.cross(e);
        if den.abs() < 1e-12 {
            continue;
        }
        let w = Vec2::new(a[0] - start[0], a[1] - start[1]);
        let t = w.cross(e) / den;
        let u = w.cross(dir) / den;
        if t > 0.0 && (0.0..=1.0).contains(&u) {
            return false;
        }
    }
    true
}
