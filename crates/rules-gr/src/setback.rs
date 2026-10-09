//! Setbacks (αποστάσεις Δ) from the plot boundaries: the buildable area and the distance of the building from
//! every boundary.
//!
//! Each plot edge gets a required distance (see `specs/rules-gr/setbacks.md`; ΝΟΚ 14 [ΚΧΠ 209] for the free-
//! standing building system):
//! - an edge on a street line (ρυμοτομική γραμμή) requires the depth of the front yard: the distance from the
//!   edge to the nearest building line (οικοδομική γραμμή) within 30 m, or zero when there is none;
//! - an edge on a building line requires zero (the building may stand on the building line);
//! - an edge facing away from the street (its outward direction within 45° of the opposite of the street
//!   edges' average outward direction) is a rear boundary and requires Δ;
//! - every other edge is a side boundary and requires δ;
//! - without street or building lines the front is unknown and every edge requires Δ.
//!
//! An edge lies "on" a line when its midpoint is within 0.10 m of it. The buildable area is the plot minus the
//! band within the required distance of each edge (round at the ends, so concave corners are handled).
//! Distances are exact segment-to-segment distances; a building is too close when it is nearer than the
//! requirement by more than 1 mm.

use kanavos_geom::{Point2, PolygonWithHoles, Segment2};
use kanavos_model::Site;
use kanavos_units::{Area, Length};

use crate::annotate::vertex_name;
use crate::ops;

/// Distance within which a plot edge is taken to lie on a street or building line, metres.
const ON_LINE: f64 = 0.10;
/// Farthest a building line may be from a street edge to define the front yard, metres.
const FRONT_YARD_MAX: f64 = 30.0;
/// Margin below which a shortfall is ignored, metres.
const SLACK: f64 = 0.001;

/// What kind of boundary a plot edge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// Along a street line: the front of the plot.
    Street,
    /// Along a building line.
    BuildingLine,
    /// A side boundary with a neighbouring plot (δ).
    Side,
    /// The rear boundary, or any boundary when the front is unknown (Δ).
    Rear,
}

/// The setback check of one plot edge.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeCheck {
    /// Index of the edge (from vertex `i` to vertex `i + 1` of the counter-clockwise plot ring).
    pub edge: usize,
    /// Edge name from its vertex letters, e.g. "Α-Β".
    pub name: String,
    /// Kind of boundary.
    pub kind: EdgeKind,
    /// Required distance of the building from the edge.
    pub required: Length,
    /// Smallest distance of the building from the edge; `None` without a building.
    pub actual: Option<Length>,
    /// Point of the building nearest to the edge.
    pub nearest: Option<Point2>,
}

impl EdgeCheck {
    /// Whether the building keeps the required distance (true without a building).
    pub fn ok(&self) -> bool {
        self.actual.is_none_or(|a| a.as_m() + SLACK >= self.required.as_m())
    }
}

/// Setbacks of a building on its plot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SetbackReport {
    /// The Δ applied to the rear boundary.
    pub delta: Length,
    /// The δ applied to side boundaries.
    pub small_delta: Length,
    /// The height H the setbacks were computed from.
    pub height: Length,
    /// Area where the building may stand.
    pub buildable: Vec<PolygonWithHoles>,
    /// Area of [`SetbackReport::buildable`].
    pub buildable_area: Area,
    /// One check per plot edge.
    pub edges: Vec<EdgeCheck>,
    /// Parts of the building outside the buildable area (empty when the building complies).
    pub outside: Vec<PolygonWithHoles>,
}

fn distance_to_polyline(p: Point2, line: &[Point2]) -> f64 {
    line.windows(2)
        .filter_map(|w| match w {
            [a, b] => Some(Segment2::new(*a, *b).distance_to(p).as_m()),
            _ => None,
        })
        .fold(f64::INFINITY, f64::min)
}

/// Distance between two segments and the point on `b` nearest to `a`.
fn segment_distance(a: &Segment2, b: &Segment2) -> (f64, Point2) {
    let tol = kanavos_units::Tolerance::default();
    if let Some((p, _, _)) = a.intersection(b, &tol) {
        return (0.0, p);
    }
    let candidates = [
        (a.distance_to(b.start).as_m(), b.start),
        (a.distance_to(b.end).as_m(), b.end),
        (b.distance_to(a.start).as_m(), b.closest_point(a.start)),
        (b.distance_to(a.end).as_m(), b.closest_point(a.end)),
    ];
    candidates.into_iter().fold((f64::INFINITY, b.start), |acc, c| if c.0 < acc.0 { c } else { acc })
}

/// Classifies the edges of the plot and their required distances.
fn requirements(site: &Site, delta: f64, small_delta: f64) -> Vec<(EdgeKind, f64)> {
    let Some(plot) = &site.plot else { return Vec::new() };
    let ring = plot.to_ccw();
    let on = |m: Point2, lines: &[Vec<Point2>]| lines.iter().any(|l| distance_to_polyline(m, l) <= ON_LINE);
    let fronts: Vec<(EdgeKind, f64)> = ring
        .edges()
        .map(|e| {
            let m = e.midpoint();
            if on(m, &site.street_lines) {
                let yard = site
                    .building_lines
                    .iter()
                    .map(|l| distance_to_polyline(m, l))
                    .filter(|d| *d <= FRONT_YARD_MAX)
                    .fold(f64::INFINITY, f64::min);
                (EdgeKind::Street, if yard.is_finite() { yard } else { 0.0 })
            } else if on(m, &site.building_lines) {
                (EdgeKind::BuildingLine, 0.0)
            } else {
                (EdgeKind::Rear, delta)
            }
        })
        .collect();
    // Average outward direction of the front edges, weighted by length.
    let mut front = kanavos_geom::Vec2::ZERO;
    for (e, (kind, _)) in ring.edges().zip(&fronts) {
        if matches!(kind, EdgeKind::Street | EdgeKind::BuildingLine)
            && let Some(u) = (e.end - e.start).normalized()
        {
            front += -u.perp() * e.length().as_m();
        }
    }
    let Some(front) = front.normalized() else { return fronts };
    ring.edges()
        .zip(fronts)
        .map(|(e, (kind, req))| match kind {
            EdgeKind::Rear => {
                let out = (e.end - e.start).normalized().map(|u| -u.perp());
                if out.is_some_and(|o| o.dot(front) < -std::f64::consts::FRAC_1_SQRT_2) {
                    (EdgeKind::Rear, delta)
                } else {
                    (EdgeKind::Side, small_delta)
                }
            }
            _ => (kind, req),
        })
        .collect()
}

/// Checks a building (its coverage polygons) against the plot with rear setback Δ and side setback δ, both
/// computed for height `height`.
pub fn setbacks(
    site: &Site,
    building: &[PolygonWithHoles],
    delta: Length,
    small_delta: Length,
    height: Length,
) -> SetbackReport {
    let Some(plot) = &site.plot else {
        return SetbackReport { delta, small_delta, height, ..SetbackReport::default() };
    };
    let ring = plot.to_ccw();
    let reqs = requirements(site, delta.as_m(), small_delta.as_m());
    let n = ring.vertices.len();
    let building_edges: Vec<Segment2> =
        building.iter().flat_map(|p| p.rings().flat_map(|r| r.edges())).collect();
    let mut bands = Vec::new();
    let mut edges = Vec::with_capacity(n);
    for (i, (e, (kind, req))) in ring.edges().zip(reqs).enumerate() {
        if req > 1e-4 {
            bands.extend(ops::buffer_segment(e.start, e.end, req));
        }
        let nearest = building_edges.iter().map(|b| segment_distance(&e, b)).fold(
            None,
            |acc: Option<(f64, Point2)>, c| match acc {
                Some(a) if a.0 <= c.0 => Some(a),
                _ => Some(c),
            },
        );
        edges.push(EdgeCheck {
            edge: i,
            name: format!("{}-{}", vertex_name(i), vertex_name((i + 1) % n.max(1))),
            kind,
            required: Length::m(req),
            actual: nearest.map(|(d, _)| Length::m(d)),
            nearest: nearest.map(|(_, p)| p),
        });
    }
    let plot_poly: PolygonWithHoles = ring.into();
    let buildable = ops::difference(std::slice::from_ref(&plot_poly), &bands);
    let outside: Vec<PolygonWithHoles> =
        ops::difference(building, &buildable).into_iter().filter(|p| p.area().as_m2() > 0.01).collect();
    SetbackReport {
        delta,
        small_delta,
        height,
        buildable_area: ops::total_area(&buildable),
        buildable,
        edges,
        outside,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Polygon2;

    fn site() -> Site {
        Site {
            plot: Some(Polygon2::from_m(&[(0.0, 0.0), (20.0, 0.0), (20.0, 30.0), (0.0, 30.0)])),
            street_lines: vec![vec![Point2::new(-5.0, 0.0), Point2::new(25.0, 0.0)]],
            building_lines: vec![vec![Point2::new(-5.0, 4.0), Point2::new(25.0, 4.0)]],
            ..Site::default()
        }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> PolygonWithHoles {
        Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]).into()
    }

    fn check(site: &Site, building: &[PolygonWithHoles]) -> SetbackReport {
        setbacks(site, building, Length::m(3.0), Length::m(2.5), Length::m(7.0))
    }

    #[test]
    fn buildable_area_respects_front_yard_sides_and_rear() {
        let r = check(&site(), &[]);
        // 20 × 30 plot: front yard 4 m, δ = 2.5 m on the sides, Δ = 3 m at the rear: 15 × 23.
        assert!((r.buildable_area.as_m2() - 15.0 * 23.0).abs() < 0.05, "{}", r.buildable_area.as_m2());
        let kinds: Vec<EdgeKind> = r.edges.iter().map(|e| e.kind).collect();
        assert_eq!(kinds, [EdgeKind::Street, EdgeKind::Side, EdgeKind::Rear, EdgeKind::Side]);
        assert_eq!(r.edges.first().map(|e| e.name.as_str()), Some("Α-Β"));
    }

    #[test]
    fn without_a_street_every_edge_takes_delta() {
        let s = Site { street_lines: vec![], building_lines: vec![], ..site() };
        let r = check(&s, &[]);
        assert!(r.edges.iter().all(|e| e.kind == EdgeKind::Rear));
        assert!((r.buildable_area.as_m2() - 14.0 * 24.0).abs() < 0.05);
    }

    #[test]
    fn a_building_too_close_to_a_side_is_reported() {
        let building = [rect(1.5, 6.0, 12.0, 16.0)];
        let r = check(&site(), &building);
        let west = r.edges.iter().find(|e| e.edge == 3).unwrap();
        assert!((west.actual.unwrap().as_m() - 1.5).abs() < 1e-9);
        assert!(!west.ok());
        assert!(r.edges.iter().filter(|e| e.edge != 3).all(EdgeCheck::ok));
        let outside = ops::total_area(&r.outside).as_m2();
        assert!((outside - 1.0 * 10.0).abs() < 0.05, "{outside}");
    }
}
