//! Offsetting polygons and polylines: slab edges, roof overhangs, setback lines, wall faces.
//!
//! Polygons are offset edge by edge; at corners where the offset edges separate a join is inserted (miter,
//! round, square or bevel), and where they overlap the raw curve passes through the original vertex. The raw
//! rings are then cleaned with the positive winding rule, which removes the loops created at concave corners
//! and the parts that collapse when shrinking (X. Chen, S. McMains, "Polygon Offsetting by Computing Winding
//! Numbers", ASME IDETC 2005).

use crate::GeomError;
use crate::boolean::{resolve_positive, union_all};
use crate::d2::{Point2, Vec2};
use crate::polygon::{Polygon2, PolygonWithHoles};

/// How corners are joined where offset edges separate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JoinStyle {
    /// Sharp corners: the offset edges are extended until they meet. Corners whose miter would reach further
    /// than `limit × |distance|` from the original vertex are cut square at that distance (`limit >= 1`; 2 is a
    /// common choice).
    Miter {
        /// Miter limit as a multiple of the offset distance.
        limit: f64,
    },
    /// Circular arcs around the original vertex, with chords deviating at most `max_deviation` metres.
    Round {
        /// Largest gap between chord and arc, metres.
        max_deviation: f64,
    },
    /// The corner is cut perpendicular to its bisector at the offset distance from the vertex.
    Square,
    /// The two offset edge ends are joined directly.
    Bevel,
}

/// How the ends of an open polyline are finished when it is stroked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapStyle {
    /// The stroke ends flush with the end points.
    Butt,
    /// The stroke extends past the end points by the half width.
    Square,
    /// Half circles around the end points.
    Round,
}

fn round_points(centre: Point2, from: Vec2, to: Vec2, radius: f64, max_dev: f64, ccw: bool) -> Vec<Point2> {
    // Arc from direction `from` to direction `to` (unit vectors), turning counter-clockwise when `ccw`.
    let mut sweep = from.angle_to(to).as_rad();
    if ccw && sweep < 0.0 {
        sweep += std::f64::consts::TAU;
    }
    if !ccw && sweep > 0.0 {
        sweep -= std::f64::consts::TAU;
    }
    let r = radius.abs();
    let dev = max_dev.max(r * 1e-6).max(1e-9);
    let step = if r > dev { 2.0 * (1.0 - dev / r).acos() } else { std::f64::consts::FRAC_PI_2 };
    let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(1, 256);
    (1..n)
        .map(|i| {
            let a = from.angle().as_rad() + sweep * i as f64 / n as f64;
            centre + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Raw offset of a closed ring whose material is on the left of its edges (outer rings counter-clockwise,
/// holes clockwise), moving every edge to its right by `distance` (negative moves left).
fn raw_ring_offset(ring: &[Point2], distance: f64, join: JoinStyle) -> Vec<Point2> {
    let pts: Vec<Point2> = dedup_ring(ring);
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let dirs: Vec<Vec2> = (0..n)
        .map(|i| {
            let a = pts.get(i).copied().unwrap_or(Point2::ORIGIN);
            let b = pts.get((i + 1) % n).copied().unwrap_or(Point2::ORIGIN);
            (b - a).normalized().unwrap_or(Vec2::X)
        })
        .collect();
    let mut out = Vec::with_capacity(n * 3);
    for i in 0..n {
        let prev = dirs.get((i + n - 1) % n).copied().unwrap_or(Vec2::X);
        let next = dirs.get(i).copied().unwrap_or(Vec2::X);
        let p = pts.get(i).copied().unwrap_or(Point2::ORIGIN);
        push_corner(&mut out, p, prev, next, distance, join);
    }
    out
}

/// Appends the offset points at vertex `p` between edge directions `prev` and `next` (right-hand offset).
fn push_corner(out: &mut Vec<Point2>, p: Point2, prev: Vec2, next: Vec2, distance: f64, join: JoinStyle) {
    // Right normals.
    let r0 = Vec2::new(prev.y, -prev.x);
    let r1 = Vec2::new(next.y, -next.x);
    let a = p + r0 * distance;
    let b = p + r1 * distance;
    let turn = prev.cross(next); // > 0: left turn
    let cos = prev.dot(next);
    if turn.abs() < 1e-12 && cos > 0.0 {
        out.push(a);
        return;
    }
    // A gap opens on the offset side when turning away from it.
    let gap = turn * distance > 0.0 || (turn.abs() < 1e-12 && cos < 0.0);
    if !gap {
        out.push(a);
        out.push(p);
        out.push(b);
        return;
    }
    let d = distance.abs();
    let bis = (r0 + r1).normalized().map(|m| m * distance.signum()).unwrap_or(-prev);
    let square_at = |reach: f64, out: &mut Vec<Point2>| {
        // Cut perpendicular to the bisector at `reach` from p: intersect with both offset lines.
        let cut = p + bis * reach;
        let cut_dir = bis.perp();
        let hit = |q: Point2, dir: Vec2| -> Point2 {
            let den = dir.cross(cut_dir);
            if den.abs() < 1e-12 {
                return q;
            }
            let t = (cut - q).cross(cut_dir) / den;
            q + dir * t
        };
        out.push(hit(a, prev));
        out.push(hit(b, -next));
    };
    match join {
        JoinStyle::Miter { limit } => {
            let denom = 1.0 + r0.dot(r1);
            let ratio = if denom > 1e-12 { (2.0 / denom).sqrt() } else { f64::INFINITY };
            if ratio <= limit.max(1.0) {
                out.push(p + (r0 + r1) * (distance / denom));
            } else {
                square_at(limit.max(1.0) * d, out);
            }
        }
        JoinStyle::Square => {
            out.push(a);
            square_at(d, out);
            out.push(b);
        }
        JoinStyle::Bevel => {
            out.push(a);
            out.push(b);
        }
        JoinStyle::Round { max_deviation } => {
            out.push(a);
            // The arc turns with the path; a full reversal goes round the front.
            let ccw = if turn.abs() < 1e-12 { distance > 0.0 } else { turn > 0.0 };
            let from = (a - p).normalized().unwrap_or(r0);
            let to = (b - p).normalized().unwrap_or(r1);
            out.extend(round_points(p, from, to, d, max_deviation, ccw));
            out.push(b);
        }
    }
}

fn dedup_ring(ring: &[Point2]) -> Vec<Point2> {
    let mut v: Vec<Point2> = Vec::with_capacity(ring.len());
    for &p in ring {
        if v.last().is_none_or(|q| (*q - p).length() > 1e-12) {
            v.push(p);
        }
    }
    while v.len() > 1 && v.first().zip(v.last()).is_some_and(|(a, b)| (*a - *b).length() <= 1e-12) {
        v.pop();
    }
    v
}

/// Offsets polygons with holes: positive `distance` grows the material, negative shrinks it.
///
/// Tolerance: results are cleaned on the Boolean grid (see [`crate::boolean`]). Failure modes: non-finite
/// input or distance gives [`GeomError::Invalid`]. Shrinking a polygon away entirely returns an empty set.
pub fn offset_polygons(
    polys: &[PolygonWithHoles],
    distance: f64,
    join: JoinStyle,
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    if !distance.is_finite() {
        return Err(GeomError::Invalid("offset distance is not finite".into()));
    }
    if distance.abs() < 1e-12 {
        return union_all(polys);
    }
    let mut rings = Vec::new();
    for p in polys {
        let o = p.oriented();
        for r in o.rings() {
            let raw = raw_ring_offset(&r.vertices, distance, join);
            if raw.len() >= 3 {
                rings.push(Polygon2::new(raw));
            }
        }
    }
    resolve_positive(&rings)
}

/// Offsets a single polygon (see [`offset_polygons`]).
pub fn offset_polygon(
    poly: &Polygon2,
    distance: f64,
    join: JoinStyle,
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    offset_polygons(&[PolygonWithHoles::from(poly.clone())], distance, join)
}

/// A polyline parallel to `points` at `distance` (positive to the left of the direction of travel).
///
/// Corners where the parallel segments separate get the requested join; where they overlap they are trimmed
/// to their intersection. Loops created by offsets larger than a segment's local radius are not removed.
/// Failure modes: fewer than two distinct points or a non-finite distance give [`GeomError::Invalid`].
pub fn offset_polyline(points: &[Point2], distance: f64, join: JoinStyle) -> Result<Vec<Point2>, GeomError> {
    if !distance.is_finite() {
        return Err(GeomError::Invalid("offset distance is not finite".into()));
    }
    let pts = {
        let mut v: Vec<Point2> = Vec::with_capacity(points.len());
        for &p in points {
            if v.last().is_none_or(|q| (*q - p).length() > 1e-12) {
                v.push(p);
            }
        }
        v
    };
    if pts.len() < 2 {
        return Err(GeomError::Invalid("a polyline needs two distinct points".into()));
    }
    let dirs: Vec<Vec2> =
        pts.windows(2).filter_map(|w| if let [a, b] = w { (*b - *a).normalized() } else { None }).collect();
    let left = |d: Vec2| d.perp() * distance;
    let mut out = Vec::with_capacity(pts.len() * 2);
    let (Some(&first), Some(&d0)) = (pts.first(), dirs.first()) else {
        return Err(GeomError::Invalid("a polyline needs two distinct points".into()));
    };
    out.push(first + left(d0));
    for i in 1..pts.len() - 1 {
        let (Some(&p), Some(&prev), Some(&next)) = (pts.get(i), dirs.get(i - 1), dirs.get(i)) else {
            continue;
        };
        // Right-hand offset by -distance equals left-hand offset by distance.
        let mut corner = Vec::new();
        push_corner(&mut corner, p, prev, next, -distance, join);
        let overlap = corner.len() == 3 && corner.get(1).is_some_and(|q| (*q - p).length() < 1e-12);
        if overlap {
            // Trim: intersection of the two offset lines.
            let a = p + left(prev);
            let b = p + left(next);
            let den = prev.cross(next);
            if den.abs() > 1e-12 {
                let t = (b - a).cross(next) / den;
                out.push(a + prev * t);
            } else {
                out.push(a);
            }
        } else {
            out.extend(corner);
        }
    }
    if let (Some(&last), Some(&dl)) = (pts.last(), dirs.last()) {
        out.push(last + left(dl));
    }
    Ok(out)
}

/// The region covered by a stroke of half width `half_width` along an open polyline.
///
/// Failure modes: fewer than two distinct points, or a non-positive or non-finite half width, give
/// [`GeomError::Invalid`].
pub fn stroke_polyline(
    points: &[Point2],
    half_width: f64,
    join: JoinStyle,
    cap: CapStyle,
) -> Result<Vec<PolygonWithHoles>, GeomError> {
    if !(half_width.is_finite() && half_width > 0.0) {
        return Err(GeomError::Invalid("stroke width must be positive".into()));
    }
    let left = offset_polyline(points, half_width, join)?;
    let mut right = offset_polyline(points, -half_width, join)?;
    right.reverse();
    let pts: Vec<Point2> = dedup_ring(points);
    let (Some(&p0), Some(&p1), Some(&pn1), Some(&pn)) =
        (pts.first(), pts.get(1), pts.get(pts.len().saturating_sub(2)), pts.last())
    else {
        return Err(GeomError::Invalid("a polyline needs two distinct points".into()));
    };
    let d_start = (p1 - p0).normalized().unwrap_or(Vec2::X);
    let d_end = (pn - pn1).normalized().unwrap_or(Vec2::X);
    let mut ring: Vec<Point2> = Vec::with_capacity(left.len() + right.len() + 64);
    let cap_points = |at: Point2, dir: Vec2, ring: &mut Vec<Point2>| match cap {
        CapStyle::Butt => {}
        CapStyle::Square => {
            ring.push(at + dir.perp() * half_width + dir * half_width);
            ring.push(at - dir.perp() * half_width + dir * half_width);
        }
        CapStyle::Round => {
            ring.extend(round_points(at, dir.perp(), -dir.perp(), half_width, half_width * 0.01, false));
        }
    };
    // Start cap: travel from the right side to the left side around p0, facing backwards.
    // Ring order: left side forward, end cap, right side backward, start cap.
    let mut left_fixed = left.clone();
    if cap == CapStyle::Square {
        if let Some(f) = left_fixed.first_mut() {
            *f = *f - d_start * half_width;
        }
        if let Some(l) = left_fixed.last_mut() {
            *l = *l + d_end * half_width;
        }
        if let Some(f) = right.first_mut() {
            *f = *f + d_end * half_width;
        }
        if let Some(l) = right.last_mut() {
            *l = *l - d_start * half_width;
        }
        ring.extend(left_fixed);
        ring.extend(right);
    } else {
        ring.extend(left_fixed);
        cap_points(pn, d_end, &mut ring);
        ring.extend(right);
        cap_points(p0, -d_start, &mut ring);
    }
    // The ring runs counter-clockwise around the stroke when traced left side forward; normalise and clean.
    let poly = Polygon2::new(ring);
    let poly = if poly.signed_area() < 0.0 { poly.reversed() } else { poly };
    // Self-overlapping strokes (sharp turns) are merged by the non-zero union.
    resolve_positive(&[poly])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::total_area;

    fn square(s: f64) -> Polygon2 {
        Polygon2::from_m(&[(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)])
    }

    #[test]
    fn miter_grow_of_square() {
        let r = offset_polygon(&square(4.0), 0.5, JoinStyle::Miter { limit: 2.0 }).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].outer.len(), 4);
        assert!((total_area(&r) - 25.0).abs() < 1e-6);
    }

    #[test]
    fn shrink_of_square_and_collapse() {
        let r = offset_polygon(&square(4.0), -0.5, JoinStyle::Miter { limit: 2.0 }).unwrap();
        assert!((total_area(&r) - 9.0).abs() < 1e-6);
        let gone = offset_polygon(&square(4.0), -2.5, JoinStyle::Miter { limit: 2.0 }).unwrap();
        assert!(gone.is_empty());
    }

    #[test]
    fn round_and_square_joins() {
        let round = offset_polygon(&square(4.0), 1.0, JoinStyle::Round { max_deviation: 0.001 }).unwrap();
        let expected = 16.0 + 4.0 * 4.0 + std::f64::consts::PI;
        assert!((total_area(&round) - expected).abs() < 0.01, "round {}", total_area(&round));
        let sq = offset_polygon(&square(4.0), 1.0, JoinStyle::Square).unwrap();
        // Each corner loses a triangle relative to the miter: 4 * (1 - (sqrt2 - 1)^2 ... ) checked by bounds.
        let a = total_area(&sq);
        assert!(a < 36.0 && a > expected, "square {a}");
        let bevel = offset_polygon(&square(4.0), 1.0, JoinStyle::Bevel).unwrap();
        assert!((total_area(&bevel) - (16.0 + 16.0 + 2.0)).abs() < 1e-6);
    }

    #[test]
    fn concave_polygon_grows_without_loops() {
        // L shape grown: the reflex corner stays sharp, no self-intersections remain.
        let l = Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)]);
        let r = offset_polygon(&l, 0.5, JoinStyle::Miter { limit: 4.0 }).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].outer.len(), 6);
        // Grown L: (−0.5..4.5)×(−0.5..2.5) ∪ (−0.5..2.5)×(−0.5..4.5)
        let expected = 5.0 * 3.0 + 3.0 * 5.0 - 3.0 * 3.0;
        assert!((total_area(&r) - expected).abs() < 1e-6);
    }

    #[test]
    fn hole_shrinks_when_material_grows() {
        let p = PolygonWithHoles {
            outer: square(10.0),
            holes: vec![Polygon2::from_m(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)])],
        };
        let r = offset_polygons(&[p], 0.5, JoinStyle::Miter { limit: 2.0 }).unwrap();
        assert_eq!(r[0].holes.len(), 1);
        assert!((r[0].holes[0].area().as_m2() - 9.0).abs() < 1e-6);
        assert!((r[0].outer.area().as_m2() - 121.0).abs() < 1e-6);
    }

    #[test]
    fn polyline_offset_left_and_right() {
        let path = [Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), Point2::new(4.0, 4.0)];
        let l = offset_polyline(&path, 1.0, JoinStyle::Miter { limit: 4.0 }).unwrap();
        // Left of a left turn: inside corner, trimmed.
        let tol = kanavos_units::Tolerance::default();
        assert_eq!(l.len(), 3);
        assert!(l[1].coincides(Point2::new(3.0, 1.0), &tol));
        let r = offset_polyline(&path, -1.0, JoinStyle::Miter { limit: 4.0 }).unwrap();
        assert_eq!(r.len(), 3);
        assert!(r[1].coincides(Point2::new(5.0, -1.0), &tol));
    }

    #[test]
    fn stroke_of_l_path() {
        let path = [Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), Point2::new(4.0, 4.0)];
        let s = stroke_polyline(&path, 0.5, JoinStyle::Miter { limit: 4.0 }, CapStyle::Butt).unwrap();
        assert_eq!(s.len(), 1);
        // Two 1 m wide arms of centre length 4 + 4, minus double counted corner (mitre): 8 * 1 = 8.
        assert!((total_area(&s) - 8.0).abs() < 1e-6, "{}", total_area(&s));
        let sq = stroke_polyline(&path, 0.5, JoinStyle::Miter { limit: 4.0 }, CapStyle::Square).unwrap();
        assert!((total_area(&sq) - 9.0).abs() < 1e-6, "{}", total_area(&sq));
    }
}
