//! Wall layout (faces and layer bands) and opening placement.

use kanavos_geom::{Arc2, Point2, Polygon2, Segment2, Vec2};
use kanavos_model::{Document, ElementKind, Id, Wall};
use kanavos_units::Length;

use crate::{GeometryError, base_elevation, dir, element};

/// One layer of a wall in plan.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerBand {
    /// Plan region of the layer.
    pub region: Polygon2,
    /// Material.
    pub material: Id,
    /// Whether the layer is part of the structural core.
    pub core: bool,
}

/// Plan geometry of a wall.
#[derive(Debug, Clone, PartialEq)]
pub struct WallLayout {
    /// Footprint of the whole thickness, after junction clean-up with connected walls.
    pub outline: Polygon2,
    /// Layer bands, left face to right face.
    pub layers: Vec<LayerBand>,
    /// Total thickness.
    pub thickness: Length,
    /// Left face polyline (start → end).
    pub left_face: Vec<Point2>,
    /// Right face polyline (start → end).
    pub right_face: Vec<Point2>,
    /// Absolute elevation of the wall base.
    pub base: f64,
    /// Absolute elevation of the wall top.
    pub top: f64,
}

/// Points of a wall's reference line offset sideways by `d` metres (positive = left), tessellated for arcs.
pub(crate) fn offset_line(w: &Wall, d: f64) -> Vec<Point2> {
    match w.arc() {
        Some(arc) => {
            // For a counter-clockwise arc the left side is towards the centre.
            let sign = if arc.sweep.as_rad() > 0.0 { -1.0 } else { 1.0 };
            let r = arc.radius.as_m() + sign * d;
            let a = Arc2 { radius: Length::m(r.max(1e-6)), ..arc };
            a.tessellate(0.002, 8)
        }
        None => {
            let n = dir(w.start, w.end).perp() * d;
            vec![w.start + n, w.end + n]
        }
    }
}

/// Band polygon between two offset lines.
pub(crate) fn band(w: &Wall, d_left: f64, d_right: f64) -> Polygon2 {
    let mut v = offset_line(w, d_left);
    let mut r = offset_line(w, d_right);
    r.reverse();
    v.extend(r);
    Polygon2::new(v).to_ccw()
}

/// Computes the plan layout of a wall.
pub fn wall_layout(doc: &Document, id: Id) -> Result<WallLayout, GeometryError> {
    let e = element(doc, id)?;
    let ElementKind::Wall(w) = &e.kind else { return Err(GeometryError::WrongKind { id, expected: "wall" }) };
    let layers = doc
        .structure_layers(&w.structure)
        .ok_or_else(|| GeometryError::MissingReference { id, what: "build-up".into() })?;
    let thickness: Length = layers.iter().map(|l| l.thickness).sum();
    if !thickness.is_positive(&doc.tolerance()) {
        return Err(GeometryError::Invalid {
            id: Some(id),
            field: "structure",
            reason: "has no thickness".into(),
        });
    }
    let (left, right) = w.face_offsets(thickness);
    let mut bands = Vec::with_capacity(layers.len());
    let mut d = left;
    for l in &layers {
        let next = d - l.thickness.as_m();
        bands.push(LayerBand { region: band(w, d, next), material: l.material, core: l.core });
        d = next;
    }
    let base = base_elevation(doc, e)?.as_m();
    Ok(WallLayout {
        outline: band(w, left, right),
        layers: bands,
        thickness,
        left_face: offset_line(w, left),
        right_face: offset_line(w, right),
        base,
        top: base + w.height.as_m(),
    })
}

/// Where an opening sits in its host wall, in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpeningPlacement {
    /// Opening centre on the host's reference line.
    pub centre: Point2,
    /// Unit vector along the wall (start → end direction at the opening).
    pub along: Vec2,
    /// Unit vector across the wall, pointing to the wall's left side.
    pub across: Vec2,
    /// Offset of the wall's left face from the reference line, along `across` (metres).
    pub left_face: f64,
    /// Offset of the wall's right face from the reference line, along `across` (metres, usually negative).
    pub right_face: f64,
    /// Absolute elevation of the opening's sill.
    pub sill_elevation: f64,
    /// Opening width.
    pub width: Length,
    /// Opening height.
    pub height: Length,
    /// Host wall id.
    pub host: Id,
}

/// Computes the world placement of a door, window or void.
pub fn opening_placement(doc: &Document, id: Id) -> Result<OpeningPlacement, GeometryError> {
    let e = element(doc, id)?;
    let ElementKind::Opening(o) = &e.kind else {
        return Err(GeometryError::WrongKind { id, expected: "opening" });
    };
    let host = doc
        .element(o.host)
        .ok_or_else(|| GeometryError::MissingReference { id, what: "host wall".into() })?;
    let ElementKind::Wall(w) = &host.kind else {
        return Err(GeometryError::MissingReference { id, what: "host wall".into() });
    };
    let thickness = doc
        .structure_thickness(&w.structure)
        .ok_or_else(|| GeometryError::MissingReference { id: o.host, what: "build-up".into() })?;
    let (left_face, right_face) = w.face_offsets(thickness);
    let (centre, along) = match w.arc() {
        Some(arc) => {
            let len = arc.length().as_m().max(1e-9);
            let t = (o.offset.as_m() / len).clamp(0.0, 1.0);
            let p = arc.at(t);
            let radial = (p - arc.centre).normalized().unwrap_or(Vec2::X);
            let tangent = if arc.sweep.as_rad() > 0.0 { radial.perp() } else { -radial.perp() };
            (p, tangent)
        }
        None => {
            let d = dir(w.start, w.end);
            (w.start + d * o.offset.as_m(), d)
        }
    };
    let storey_elevation = host
        .storey
        .and_then(|s| doc.storey(s))
        .map(|s| s.elevation)
        .ok_or_else(|| GeometryError::MissingReference { id, what: "storey".into() })?;
    Ok(OpeningPlacement {
        centre,
        along,
        across: along.perp(),
        left_face,
        right_face,
        sill_elevation: (storey_elevation + o.sill).as_m(),
        width: o.width,
        height: o.height,
        host: o.host,
    })
}

/// Intervals along a straight wall's length (metres from start) that remain solid at a cut elevation, after
/// removing openings that span it.
pub(crate) fn solid_intervals(doc: &Document, wall: Id, w: &Wall, cut: f64) -> Vec<(f64, f64)> {
    let len = w.length().as_m();
    let storey_elevation = doc
        .element(wall)
        .and_then(|e| e.storey)
        .and_then(|s| doc.storey(s))
        .map(|s| s.elevation.as_m())
        .unwrap_or(0.0);
    let mut gaps: Vec<(f64, f64)> = doc
        .openings_in(wall)
        .filter(|(_, o)| {
            let sill = storey_elevation + o.sill.as_m();
            let head = sill + o.height.as_m();
            sill < cut && cut < head
        })
        .map(|(_, o)| {
            let c = o.offset.as_m();
            let h = o.width.as_m() / 2.0;
            ((c - h).max(0.0), (c + h).min(len))
        })
        .collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut t = 0.0;
    for (a, b) in gaps {
        if a > t {
            out.push((t, a));
        }
        t = t.max(b);
    }
    if t < len {
        out.push((t, len));
    }
    out
}

/// A segment of a straight wall's band between two lengths along the wall.
pub(crate) fn band_piece(w: &Wall, d_left: f64, d_right: f64, t0: f64, t1: f64) -> Polygon2 {
    let d = dir(w.start, w.end);
    let n = d.perp();
    let a = w.start + d * t0;
    let b = w.start + d * t1;
    Polygon2::new(vec![a + n * d_right, b + n * d_right, b + n * d_left, a + n * d_left])
}

/// Face segments of a straight wall over an interval, for drawing what is seen through openings.
pub(crate) fn face_segments(w: &Wall, d_left: f64, d_right: f64, t0: f64, t1: f64) -> [Segment2; 2] {
    let d = dir(w.start, w.end);
    let n = d.perp();
    let a = w.start + d * t0;
    let b = w.start + d * t1;
    [Segment2::new(a + n * d_left, b + n * d_left), Segment2::new(a + n * d_right, b + n * d_right)]
}

pub use crate::build::plan_parts;
