//! Splitting a coverage polygon into simple pieces whose areas can be checked by hand, as coverage diagrams
//! show them: rectangles `a × b` for orthogonal outlines, triangles `½ × b × h` otherwise.
//!
//! Orthogonal outlines (every edge parallel or perpendicular to the longest edge, within 0.01°) are cut into
//! strips between consecutive distinct vertex coordinates, and neighbouring strips with the same extent are
//! merged; both strip directions are tried and the one with fewer pieces wins. Other outlines are triangulated.
//! The pieces always add up to the polygon's area (to floating-point rounding).

use kanavos_geom::{Point2, Polygon2, PolygonWithHoles, Vec2};
use kanavos_units::{Area, Locale, format_decimal};

/// One piece of a decomposed polygon.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaPiece {
    /// Outline of the piece.
    pub outline: Polygon2,
    /// The factors of the area formula: two sides of a rectangle, or base and height of a triangle.
    pub factors: (f64, f64),
    /// Whether the piece is a triangle (area = ½ × base × height).
    pub triangle: bool,
    /// Area.
    pub area: Area,
}

impl AreaPiece {
    /// The formula with its result, e.g. `12,00 × 8,50 = 102,00` or `½ × 5,00 × 3,00 = 7,50`.
    pub fn formula(&self, locale: Locale) -> String {
        let f = |v: f64| format_decimal(v, 2, locale);
        let (a, b) = self.factors;
        let lhs = if self.triangle {
            format!("½ × {} × {}", f(a), f(b))
        } else {
            format!("{} × {}", f(a), f(b))
        };
        format!("{lhs} = {}", f(self.area.as_m2()))
    }
}

/// Angle tolerance for orthogonality, radians (0.01°).
const ANGLE_TOL: f64 = 1.75e-4;

fn axis(poly: &PolygonWithHoles) -> Option<Vec2> {
    poly.outer.edges().max_by(|a, b| a.length().total_cmp(&b.length())).and_then(|e| e.direction())
}

fn is_orthogonal(poly: &PolygonWithHoles, u: Vec2) -> bool {
    poly.rings().all(|r| {
        r.edges().all(|e| match e.direction() {
            Some(d) => d.cross(u).abs() < ANGLE_TOL || d.dot(u).abs() < ANGLE_TOL,
            None => true,
        })
    })
}

/// Rectangles of an orthogonal polygon, cut in strips along `a` (local coordinates: `a` along `u`, `b` along
/// `v`). Returns `(a0, a1, b0, b1)` boxes.
fn strips(poly: &PolygonWithHoles, u: Vec2, v: Vec2) -> Vec<(f64, f64, f64, f64)> {
    let local = |p: Point2| (p.to_vec().dot(u), p.to_vec().dot(v));
    let mut cuts: Vec<f64> = poly.rings().flat_map(|r| r.vertices.iter().map(|p| local(*p).0)).collect();
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();
    for w in cuts.windows(2) {
        let [a0, a1] = w else { continue };
        let mid = (a0 + a1) / 2.0;
        // Crossings of the line a = mid with edges running along a.
        let mut bs: Vec<f64> = poly
            .rings()
            .flat_map(|r| r.edges())
            .filter_map(|e| {
                let (p, q) = (local(e.start), local(e.end));
                let (lo, hi) = (p.0.min(q.0), p.0.max(q.0));
                (lo < mid && hi > mid).then(|| p.1 + (q.1 - p.1) * (mid - p.0) / (q.0 - p.0))
            })
            .collect();
        bs.sort_by(f64::total_cmp);
        for pair in bs.chunks(2) {
            if let [b0, b1] = pair
                && b1 - b0 > 1e-6
            {
                // Merge with a box of the previous strip that has the same extent.
                let merged = boxes.iter_mut().rev().find(|bx| {
                    (bx.1 - a0).abs() < 1e-6 && (bx.2 - b0).abs() < 1e-6 && (bx.3 - b1).abs() < 1e-6
                });
                match merged {
                    Some(bx) => bx.1 = *a1,
                    None => boxes.push((*a0, *a1, *b0, *b1)),
                }
            }
        }
    }
    boxes
}

fn triangles(poly: &PolygonWithHoles) -> Vec<AreaPiece> {
    let Ok((pts, tris)) = kanavos_solid::triangulate(poly) else { return Vec::new() };
    tris.iter()
        .filter_map(|t| {
            let [a, b, c] = t.map(|i| pts.get(i as usize).copied());
            let (a, b, c) = (a?, b?, c?);
            let outline = Polygon2::new(vec![a, b, c]);
            let area = outline.area();
            let base = [a.distance(b), b.distance(c), c.distance(a)]
                .into_iter()
                .map(|l| l.as_m())
                .fold(0.0, f64::max);
            (base > 1e-9 && area.as_m2() > 1e-9).then(|| AreaPiece {
                outline,
                factors: (base, 2.0 * area.as_m2() / base),
                triangle: true,
                area,
            })
        })
        .collect()
}

/// Splits a polygon into rectangles or triangles (see the module documentation).
pub fn decompose(poly: &PolygonWithHoles) -> Vec<AreaPiece> {
    let Some(u) = axis(poly) else { return Vec::new() };
    if !is_orthogonal(poly, u) {
        return triangles(poly);
    }
    let v = u.perp();
    let along_u = strips(poly, u, v);
    let along_v: Vec<(f64, f64, f64, f64)> =
        strips(poly, v, -u).into_iter().map(|(a0, a1, b0, b1)| (-b1, -b0, a0, a1)).collect();
    let boxes = if along_v.len() < along_u.len() { along_v } else { along_u };
    boxes
        .into_iter()
        .map(|(a0, a1, b0, b1)| {
            let p = |a: f64, b: f64| {
                let w = u * a + v * b;
                Point2::new(w.x, w.y)
            };
            let (da, db) = (a1 - a0, b1 - b0);
            let (long, short) = if da >= db { (da, db) } else { (db, da) };
            AreaPiece {
                outline: Polygon2::new(vec![p(a0, b0), p(a1, b0), p(a1, b1), p(a0, b1)]),
                factors: (long, short),
                triangle: false,
                area: Area::m2(da * db),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(p: &[AreaPiece]) -> f64 {
        p.iter().map(|x| x.area.as_m2()).sum()
    }

    #[test]
    fn rectangle_is_one_piece() {
        let r: PolygonWithHoles =
            Polygon2::from_m(&[(0.0, 0.0), (12.0, 0.0), (12.0, 8.5), (0.0, 8.5)]).into();
        let p = decompose(&r);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].formula(Locale::El), "12,00 × 8,50 = 102,00");
    }

    #[test]
    fn l_shape_is_two_rectangles() {
        let l: PolygonWithHoles =
            Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (4.0, 4.0), (4.0, 9.0), (0.0, 9.0)])
                .into();
        let p = decompose(&l);
        assert_eq!(p.len(), 2);
        assert!((total(&p) - l.area().as_m2()).abs() < 1e-9);
    }

    #[test]
    fn rotated_orthogonal_outline_stays_rectangular() {
        let t = kanavos_geom::Transform2::rotation(kanavos_units::Angle::deg(23.0));
        let l: PolygonWithHoles =
            Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (4.0, 4.0), (4.0, 9.0), (0.0, 9.0)])
                .transformed(&t)
                .into();
        let p = decompose(&l);
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|x| !x.triangle));
        assert!((total(&p) - 60.0).abs() < 1e-6);
    }

    #[test]
    fn slanted_outline_becomes_triangles() {
        let q: PolygonWithHoles = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (8.0, 6.0), (1.0, 5.0)]).into();
        let p = decompose(&q);
        assert!(p.len() >= 2 && p.iter().all(|x| x.triangle));
        assert!((total(&p) - q.area().as_m2()).abs() < 1e-6);
        assert!(p[0].formula(Locale::En).starts_with("½ × "));
    }
}
