//! The result of a DXF import and helpers for survey underlays (τοπογραφικό).
//!
//! Survey drawings use ΕΓΣΑ87 grid coordinates: eastings around 400 000 m and northings around 4 000 000 m. The
//! project works in a local frame near the origin, linked to the grid by its georeference. [`DxfImport::to_project`]
//! moves everything into project coordinates; [`DxfImport::to_underlay`] and [`DxfImport::plot_polygon`] produce the
//! model types.

use std::collections::BTreeMap;

use kanavos_drawing::geometry::shape_bounds;
use kanavos_drawing::{Affine, Drawing, Rect, Rgba, pt};
use kanavos_geo::GridPlacement;
use kanavos_geom::{Point2, Point3, Polygon2};
use kanavos_model::{Georef, Underlay};

use crate::codepage::Codepage;

/// Where a survey point came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointSource {
    /// A POINT entity; its Z is the elevation (0 if the survey is 2D).
    Point,
    /// A block insert with an elevation attribute, or inserted at a non-zero elevation.
    Insert,
    /// A point without elevation that got one from an elevation text next to it.
    TextNearPoint,
    /// An elevation text with no point nearby; the position is the text's insertion point, so it is approximate.
    Text,
}

/// A surveyed point.
#[derive(Debug, Clone, PartialEq)]
pub struct SurveyPoint {
    /// X, Y, Z in metres (grid or project coordinates, see [`DxfImport::to_project`]).
    pub position: [f64; 3],
    /// Whether the elevation is known (false for 2D points without an elevation text).
    pub has_elevation: bool,
    /// How the point was found.
    pub source: PointSource,
    /// Point number or name, if found (block attribute or nearby text).
    pub label: Option<String>,
    /// DXF layer.
    pub layer: String,
}

/// A closed polyline from the file: a candidate plot boundary or building outline.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedPolyline {
    /// DXF layer.
    pub layer: String,
    /// Vertices in metres, first not repeated. Arcs are flattened.
    pub points: Vec<[f64; 2]>,
    /// Enclosed area, square metres.
    pub area: f64,
    /// Perimeter, metres.
    pub perimeter: f64,
}

impl ClosedPolyline {
    /// A closed polyline from vertices (a repeated last vertex is dropped).
    pub fn new(layer: impl Into<String>, mut points: Vec<[f64; 2]>) -> ClosedPolyline {
        if points.len() > 2 && points.first().zip(points.last()).is_some_and(|(a, b)| dist(*a, *b) < 1e-9) {
            points.pop();
        }
        let n = points.len();
        let mut area = 0.0;
        let mut perimeter = 0.0;
        // Relative to the first vertex: grid coordinates are millions of metres, and the shoelace products would
        // otherwise lose the area to cancellation.
        let o = points.first().copied().unwrap_or([0.0, 0.0]);
        for i in 0..n {
            let (Some(a), Some(b)) = (points.get(i), points.get((i + 1) % n)) else { continue };
            area += (a[0] - o[0]) * (b[1] - o[1]) - (b[0] - o[0]) * (a[1] - o[1]);
            perimeter += dist(*a, *b);
        }
        ClosedPolyline { layer: layer.into(), points, area: (area / 2.0).abs(), perimeter }
    }
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// A layer of the imported file.
#[derive(Debug, Clone, PartialEq)]
pub struct DxfLayer {
    /// Name as in the file.
    pub name: String,
    /// Colour.
    pub color: Rgba,
    /// Whether the layer is on and not frozen.
    pub visible: bool,
    /// Whether the layer is frozen.
    pub frozen: bool,
    /// Number of drawing items created on it.
    pub items: usize,
}

/// Everything read from a DXF file.
#[derive(Debug, Clone, PartialEq)]
pub struct DxfImport {
    /// The geometry as a model drawing in metres, at the import scale.
    pub drawing: Drawing,
    /// Layers, sorted by name.
    pub layers: Vec<DxfLayer>,
    /// Survey points with elevations.
    pub points: Vec<SurveyPoint>,
    /// Closed polylines at the top level of the file (not inside blocks).
    pub closed_polylines: Vec<ClosedPolyline>,
    /// Bounds of the geometry and points, metres.
    pub extents: Option<Rect>,
    /// Format version (`$ACADVER`), e.g. `AC1015`.
    pub version: String,
    /// Code page used for text.
    pub codepage: Codepage,
    /// Metres per drawing unit used for the conversion.
    pub metres_per_unit: f64,
    /// Things that could not be imported, in words.
    pub warnings: Vec<String>,
    /// Number of entities read, by type.
    pub entity_counts: BTreeMap<String, usize>,
}

/// The grid placement of a project georeference (rotation from project +Y to grid north, counter-clockwise).
pub fn placement_from_georef(g: &Georef) -> GridPlacement {
    GridPlacement {
        easting: g.easting,
        northing: g.northing,
        rotation: g.rotation.as_rad(),
        altitude: g.altitude,
    }
}

impl DxfImport {
    fn mapped(&self, m: &Affine, dz: f64) -> DxfImport {
        let map2 = |p: [f64; 2]| {
            let q = m.apply(pt(p[0], p[1]));
            [q.x, q.y]
        };
        let mut out = self.clone();
        out.drawing = self.drawing.transformed(m);
        for p in &mut out.points {
            let [x, y] = map2([p.position[0], p.position[1]]);
            p.position = [x, y, if p.has_elevation { p.position[2] - dz } else { p.position[2] }];
        }
        for c in &mut out.closed_polylines {
            c.points = c.points.iter().map(|p| map2(*p)).collect();
        }
        out.extents = extents(&out.drawing, &out.points);
        out
    }

    /// Everything moved so that `origin` (in the current coordinates) becomes (0, 0). Elevations are unchanged.
    pub fn translated(&self, origin: [f64; 2]) -> DxfImport {
        self.mapped(&Affine::scale_translate(1.0, -origin[0], -origin[1]), 0.0)
    }

    /// Grid coordinates to project coordinates with a georeference placement: plan coordinates through the
    /// placement's origin and rotation, elevations relative to the project's ±0.00.
    pub fn to_project(&self, placement: &GridPlacement) -> DxfImport {
        let t = Affine::scale_translate(1.0, -placement.easting, -placement.northing);
        let m = Affine::rotation(placement.rotation).then(&t);
        self.mapped(&m, placement.altitude)
    }

    /// A round origin near the south-west corner of the extents (multiples of 100 m), handy for
    /// [`DxfImport::translated`] when no georeference exists yet.
    pub fn suggested_origin(&self) -> Option<[f64; 2]> {
        let e = self.extents?;
        Some([(e.min.x / 100.0).floor() * 100.0, (e.min.y / 100.0).floor() * 100.0])
    }

    /// The drawing as a model underlay element.
    pub fn to_underlay(&self, name: impl Into<String>, source: impl Into<String>) -> Underlay {
        Underlay { name: name.into(), source: source.into(), drawing: self.drawing.clone() }
    }

    /// A closed polyline as a plot polygon (counter-clockwise), in the current coordinates.
    pub fn plot_polygon(&self, index: usize) -> Option<Polygon2> {
        let c = self.closed_polylines.get(index)?;
        if c.points.len() < 3 {
            return None;
        }
        Some(Polygon2::new(c.points.iter().map(|p| Point2::new(p[0], p[1])).collect()).to_ccw())
    }

    /// Index of the closed polyline with the largest area.
    pub fn largest_closed_polyline(&self) -> Option<usize> {
        self.closed_polylines.iter().enumerate().max_by(|a, b| a.1.area.total_cmp(&b.1.area)).map(|(i, _)| i)
    }

    /// Points with a known elevation, as terrain points (current coordinates).
    pub fn terrain_points(&self) -> Vec<Point3> {
        self.points
            .iter()
            .filter(|p| p.has_elevation)
            .map(|p| Point3 { x: p.position[0], y: p.position[1], z: p.position[2] })
            .collect()
    }
}

/// Bounds of drawing geometry (text by anchor) and points.
pub(crate) fn extents(d: &Drawing, points: &[SurveyPoint]) -> Option<Rect> {
    let mut acc: Option<Rect> = None;
    let mut add = |r: Rect| {
        acc = Some(match acc {
            Some(a) => a.union(&r),
            None => r,
        });
    };
    for item in &d.items {
        if let Some(b) = shape_bounds(&item.shape) {
            add(b);
        }
    }
    for p in points {
        let q = pt(p.position[0], p.position[1]);
        if q.x.is_finite() && q.y.is_finite() {
            add(Rect { min: q, max: q });
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_polyline_area_and_repeated_vertex() {
        let c = ClosedPolyline::new("x", vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0], [0.0, 0.0]]);
        assert_eq!(c.points.len(), 4);
        assert!((c.area - 50.0).abs() < 1e-12 && (c.perimeter - 30.0).abs() < 1e-12);
    }

    #[test]
    fn area_is_exact_at_grid_coordinates() {
        // A 0.1 m square at ΕΓΣΑ87 coordinates: the plain shoelace formula loses this to cancellation.
        let (e, n) = (480_000.123, 4_200_000.456);
        let c = ClosedPolyline::new("x", vec![[e, n], [e + 0.1, n], [e + 0.1, n + 0.1], [e, n + 0.1]]);
        assert!((c.area - 0.01).abs() < 1e-9, "area {}", c.area);
    }
}
