//! Roofs: roof planes (flat, lower envelope for convex outlines, weighted straight skeleton otherwise), ridge,
//! hip, valley, eave and verge lines, plan parts and 3D solids with thickness measured perpendicular to the
//! planes.

use kanavos_geom::{
    Angle, HalfPlane, JoinStyle, Plane, Point2, Point3, Polygon2, PolygonWithHoles, Segment2, Tolerance,
    Vec2, Vec3, clip_convex, offset_polygon, shared_boundaries, weighted_straight_skeleton,
};
use kanavos_model::{Document, Element, ElementKind, Id, Roof, RoofShape};
use kanavos_solid::{ColumnPiece, ZPlane, column_solid, extrude};

use crate::junction::WallInfo;
use crate::region::{Clip, apply_clip};
use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart, element};

/// One plane of a roof.
#[derive(Debug, Clone, PartialEq)]
pub struct RoofPlane {
    /// Outline edges the plane rises from (edge `i` runs from vertex `i` to `i + 1` of the roof outline); empty
    /// for a flat roof.
    pub edges: Vec<usize>,
    /// Plan region of the plane.
    pub region: Vec<PolygonWithHoles>,
    /// The roof's top surface on this plane, in world coordinates (normal pointing up).
    pub plane: Plane,
    /// The top surface as a height function.
    pub height: ZPlane,
    /// Pitch.
    pub pitch: Angle,
    /// Unit plan direction pointing down the slope, towards the eave; zero for a flat roof.
    pub downhill: Vec2,
}

impl RoofPlane {
    /// Plan area of the plane.
    pub fn plan_area(&self) -> f64 {
        self.region.iter().map(|r| r.area().as_m2()).sum()
    }

    /// True (sloped) area of the top surface.
    pub fn true_area(&self) -> f64 {
        let c = self.pitch.cos();
        if c > 1e-9 { self.plan_area() / c } else { self.plan_area() }
    }
}

/// Kind of a roof line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoofLineKind {
    /// Horizontal line where two planes meet at the top.
    Ridge,
    /// Sloping line where two planes meet over an outside corner.
    Hip,
    /// Sloping line where two planes meet over an inside corner.
    Valley,
    /// Horizontal lower edge of a plane, along the outline.
    Eave,
    /// Sloping edge of a plane over a gable.
    Verge,
    /// Line where two planes of different pitch meet at a vertical step.
    Step,
}

/// A line of a roof (an edge of its top surface).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoofLine {
    /// Kind.
    pub kind: RoofLineKind,
    /// Plan segment.
    pub segment: Segment2,
    /// Absolute heights of the top surface at the segment's start and end.
    pub z: (f64, f64),
}

/// Computed geometry of a roof.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoofGeometry {
    pub planes: Vec<RoofPlane>,
    pub lines: Vec<RoofLine>,
    /// Total thickness, measured perpendicular to the planes.
    pub thickness: f64,
    /// Layers (material, thickness) from top to bottom.
    pub layers: Vec<(Id, f64)>,
    /// Core material.
    pub core: Option<Id>,
    /// Absolute height of the top surface at the eaves.
    pub base: f64,
}

fn storey_elevation(doc: &Document, e: &Element) -> f64 {
    e.storey.and_then(|s| doc.storey(s)).map(|s| s.elevation.as_m()).unwrap_or(0.0)
}

fn zplane_to_plane(z: &ZPlane, at: Point2) -> Plane {
    let n = Vec3::new(-z.gx, -z.gy, 1.0).normalized().unwrap_or(Vec3::Z);
    Plane { origin: Point3::from_plan(at, z.at(at)), normal: n }
}

/// Height function of the plane rising inwards from edge `a → b` of a counter-clockwise outline.
fn edge_plane(a: Point2, b: Point2, base: f64, pitch: f64) -> Option<(ZPlane, Vec2)> {
    let d = (b - a).normalized()?;
    let n = d.perp();
    let t = pitch.tan();
    Some((ZPlane::through(a, base, n * t), -n))
}

/// Roof planes and lines of a roof element.
pub(crate) fn roof_geometry(doc: &Document, e: &Element) -> Result<RoofGeometry, GeometryError> {
    let id = e.id;
    let ElementKind::Roof(r) = &e.kind else { return Err(GeometryError::WrongKind { id, expected: "roof" }) };
    let layers = doc
        .structure_layers(&r.structure)
        .ok_or_else(|| GeometryError::MissingReference { id, what: "build-up".into() })?;
    let thickness: f64 = layers.iter().map(|l| l.thickness.as_m()).sum();
    let base = storey_elevation(doc, e) + r.base.as_m();
    let tol = doc.tolerance();
    if !r.outline.is_valid(&tol) {
        return Err(GeometryError::Invalid {
            id: Some(id),
            field: "outline",
            reason: "needs at least three points enclosing an area".into(),
        });
    }
    let planes = match &r.shape {
        RoofShape::Flat { .. } => {
            let z = ZPlane::constant(base);
            let at = r.outline.vertices.first().copied().unwrap_or(Point2::ORIGIN);
            vec![RoofPlane {
                edges: Vec::new(),
                region: vec![r.outline.to_ccw().into()],
                plane: zplane_to_plane(&z, at),
                height: z,
                pitch: Angle::ZERO,
                downhill: Vec2::ZERO,
            }]
        }
        RoofShape::Pitched { slopes } => pitched_planes(r, slopes, base, id)?,
    };
    let lines = roof_lines_of(r, &planes, base, &tol);
    Ok(RoofGeometry {
        planes,
        lines,
        thickness,
        layers: layers.iter().map(|l| (l.material, l.thickness.as_m())).collect(),
        core: doc.structure_core_material(&r.structure),
        base,
    })
}

fn pitched_planes(
    r: &Roof,
    slopes: &[Option<Angle>],
    base: f64,
    id: Id,
) -> Result<Vec<RoofPlane>, GeometryError> {
    let n = r.outline.len();
    if slopes.len() != n {
        return Err(GeometryError::Invalid {
            id: Some(id),
            field: "slopes",
            reason: "needs one entry per outline edge".into(),
        });
    }
    // Work counter-clockwise; edge `k` of the ccw ring is input edge `map[k]`.
    let ccw = r.outline.signed_area() > 0.0;
    let (ring, map): (Vec<Point2>, Vec<usize>) = if ccw {
        (r.outline.vertices.clone(), (0..n).collect())
    } else {
        (r.outline.vertices.iter().rev().copied().collect(), (0..n).map(|k| (2 * n - 2 - k) % n).collect())
    };
    let slope_of = |k: usize| map.get(k).and_then(|&i| slopes.get(i).copied().flatten());
    let poly = Polygon2::new(ring.clone());
    let edge = |k: usize| -> Option<(Point2, Point2)> { Some((*ring.get(k)?, *ring.get((k + 1) % n)?)) };
    let mut planes = Vec::new();
    if poly.is_convex() {
        // Lower envelope of the edge planes, clipped to the outline. Collinear edges with equal pitch share a
        // plane, owned by the first.
        let mut owners: Vec<(Vec<usize>, ZPlane, Vec2, f64)> = Vec::new();
        for k in 0..n {
            let (Some(pitch), Some((a, b))) = (slope_of(k), edge(k)) else { continue };
            let Some((z, down)) = edge_plane(a, b, base, pitch.as_rad()) else { continue };
            let input = map.get(k).copied().unwrap_or(k);
            match owners.iter_mut().find(|(_, oz, _, _)| {
                (oz.z0 - z.z0).abs() < 1e-9 && (oz.gx - z.gx).abs() < 1e-9 && (oz.gy - z.gy).abs() < 1e-9
            }) {
                Some(o) => o.0.push(input),
                None => owners.push((vec![input], z, down, pitch.as_rad())),
            }
        }
        for (i, (edges, z, down, pitch)) in owners.iter().enumerate() {
            let mut region = Some(poly.clone());
            for (j, (_, other, _, _)) in owners.iter().enumerate() {
                if i == j {
                    continue;
                }
                // Keep z_i <= z_j:  (∇z_j − ∇z_i)·p + (z0_j − z0_i) >= 0.
                let g = Vec2::new(other.gx - z.gx, other.gy - z.gy);
                let c = other.z0 - z.z0;
                let len = g.length();
                if len < 1e-12 {
                    continue;
                }
                let foot = Point2::new(-c * g.x / (len * len), -c * g.y / (len * len));
                region =
                    region.and_then(|p| HalfPlane::new(foot, g).and_then(|hp| clip_convex(&p, &hp, 1e-9)));
            }
            let region: Vec<PolygonWithHoles> = region.into_iter().map(Into::into).collect();
            let at =
                edges.first().and_then(|&e| r.outline.vertices.get(e).copied()).unwrap_or(Point2::ORIGIN);
            planes.push(RoofPlane {
                edges: edges.clone(),
                region,
                plane: zplane_to_plane(z, at),
                height: *z,
                pitch: Angle::rad(*pitch),
                downhill: *down,
            });
        }
        planes.retain(|p| p.plan_area() > 1e-9);
        return Ok(planes);
    }
    // Non-convex: weighted straight skeleton with speed cot(pitch) (time = height above the eaves).
    let speeds: Vec<f64> =
        slopes.iter().map(|s| s.map(|a| 1.0 / a.as_rad().tan().max(1e-6)).unwrap_or(0.0)).collect();
    let sk = weighted_straight_skeleton(&r.outline, &speeds).map_err(|e| GeometryError::Invalid {
        id: Some(id),
        field: "slopes",
        reason: format!("the roof planes cannot be resolved: {e}"),
    })?;
    for f in sk.faces {
        if f.speed <= 0.0 || f.region.is_empty() {
            continue;
        }
        let t = 1.0 / f.speed;
        // z = base + (n·p − c)·t
        let z = ZPlane { z0: base - f.offset * t, gx: f.normal.x * t, gy: f.normal.y * t };
        let at = f.edges.first().and_then(|&e| r.outline.vertices.get(e).copied()).unwrap_or(Point2::ORIGIN);
        planes.push(RoofPlane {
            edges: f.edges,
            region: f.region,
            plane: zplane_to_plane(&z, at),
            height: z,
            pitch: Angle::rad(t.atan()),
            downhill: -f.normal,
        });
    }
    Ok(planes)
}

fn roof_lines_of(r: &Roof, planes: &[RoofPlane], base: f64, tol: &Tolerance) -> Vec<RoofLine> {
    let mut lines = Vec::new();
    let eps = 1e-3;
    // Internal lines: boundaries shared by two planes.
    let regions: Vec<Vec<PolygonWithHoles>> = planes.iter().map(|p| p.region.clone()).collect();
    let shared = shared_boundaries(&regions, &Tolerance { length: kanavos_units::Length::m(1e-5), ..*tol });
    for s in &shared {
        let (Some(pa), Some(pb)) = (planes.get(s.a), planes.get(s.b)) else { continue };
        let (p, q) = (s.segment.start, s.segment.end);
        let (za, zb) = (pa.height.at(p), pa.height.at(q));
        let step = (pb.height.at(p) - za).abs() > eps || (pb.height.at(q) - zb).abs() > eps;
        let kind = if step {
            RoofLineKind::Step
        } else if (za - zb).abs() < eps {
            RoofLineKind::Ridge
        } else {
            // Convex fold (hip) when each plane is the lower one on its own side.
            let m = p.midpoint(q);
            let n = s.segment.direction().map(Vec2::perp).unwrap_or(Vec2::X) * 0.01;
            let tol0 = Tolerance::default();
            let side_a = if pa.region.iter().any(|r| r.contains(m + n, &tol0)) { m + n } else { m - n };
            if pa.height.at(side_a) <= pb.height.at(side_a) + 1e-9 {
                RoofLineKind::Hip
            } else {
                RoofLineKind::Valley
            }
        };
        lines.push(RoofLine { kind, segment: s.segment, z: (za, zb) });
    }
    // Outline lines: eaves on sloped edges, verges over gables, split where the planes along them change.
    let outline = r.outline.to_ccw();
    let slopes: Option<&Vec<Option<Angle>>> = match &r.shape {
        RoofShape::Pitched { slopes } => Some(slopes),
        RoofShape::Flat { .. } => None,
    };
    let ccw = r.outline.signed_area() > 0.0;
    let n = r.outline.len();
    for (k, e) in outline.edges().enumerate() {
        let input = if ccw { k } else { (2 * n - 2 - k) % n };
        let sloped = slopes.is_none_or(|s| s.get(input).copied().flatten().is_some());
        let mut pieces: Vec<RoofLine> = Vec::new();
        for p in planes {
            for region in &p.region {
                for ring in region.rings() {
                    for pe in ring.edges() {
                        if let kanavos_geom::SegmentIntersection::Overlap { segment, .. } =
                            kanavos_geom::segment_intersection(
                                &e,
                                &pe,
                                &Tolerance { length: kanavos_units::Length::m(1e-5), ..*tol },
                            )
                            && segment.length().as_m() > 1e-4
                        {
                            let (za, zb) = (p.height.at(segment.start), p.height.at(segment.end));
                            let horizontal = (za - zb).abs() < eps && (za - base).abs() < eps;
                            let kind =
                                if sloped && horizontal { RoofLineKind::Eave } else { RoofLineKind::Verge };
                            pieces.push(RoofLine { kind, segment, z: (za, zb) });
                        }
                    }
                }
            }
        }
        lines.extend(pieces);
    }
    lines
}

/// The planes of a roof: plan region, plane equation and slope direction of each.
///
/// Convex outlines use the lower envelope of the edge planes; other outlines the weighted straight skeleton
/// (pitch-weighted hips and valleys). Failure modes: not a roof, missing build-up, invalid outline or slopes, or
/// a slope set that leaves part of the outline unreachable by any plane ([`GeometryError::Invalid`]).
pub fn roof_planes(doc: &Document, id: Id) -> Result<Vec<RoofPlane>, GeometryError> {
    Ok(roof_geometry(doc, element(doc, id)?)?.planes)
}

/// The lines of a roof's top surface: ridges, hips, valleys, steps, eaves and verges.
pub fn roof_lines(doc: &Document, id: Id) -> Result<Vec<RoofLine>, GeometryError> {
    Ok(roof_geometry(doc, element(doc, id)?)?.lines)
}

/// Vertical distance from the top surface to a depth `d` measured perpendicular to a plane of this pitch.
fn vertical(d: f64, pitch: Angle) -> f64 {
    let c = pitch.cos();
    if c > 1e-6 { d / c } else { d }
}

/// How a roof appears in a plan cut at absolute elevation `cut`.
pub(crate) fn roof_plan_parts(doc: &Document, e: &Element, cut: f64) -> Result<PlanParts, GeometryError> {
    let g = roof_geometry(doc, e)?;
    let mut parts = PlanParts::default();
    for p in &g.planes {
        let bottom = p.height.shifted(-vertical(g.thickness, p.pitch));
        // Region where the roof slab is cut: bottom < cut < top.
        let mut region: Vec<Polygon2> = p.region.iter().map(|r| r.outer.clone()).collect();
        for (z, keep_above) in [(p.height, true), (bottom, false)] {
            region = clip_by_height(region, &z, cut, keep_above);
        }
        parts.cut.extend(region.into_iter().map(|r| CutArea { region: r.into(), material: g.core }));
    }
    for l in &g.lines {
        let (below, above) = split_line(l, cut);
        parts.below.extend(below);
        parts.above.extend(above);
    }
    let polys: Vec<PolygonWithHoles> = parts.cut.iter().map(|c| c.region.clone()).collect();
    parts.outline = kanavos_geom::union_all(&polys).unwrap_or(polys);
    Ok(parts)
}

/// Keeps the part of the polygons where `z(p) > cut` (`above`) or `z(p) < cut`.
fn clip_by_height(region: Vec<Polygon2>, z: &ZPlane, cut: f64, above: bool) -> Vec<Polygon2> {
    let g = z.gradient();
    let len = g.length();
    if len < 1e-12 {
        let inside = if above { z.z0 > cut } else { z.z0 < cut };
        return if inside { region } else { Vec::new() };
    }
    // z(p) = cut on the line g·p = cut − z0.
    let k = (cut - z.z0) / (len * len);
    let foot = Point2::new(g.x * k, g.y * k);
    let Some(hp) = HalfPlane::new(foot, if above { g } else { -g }) else { return region };
    apply_clip(region, &Clip::Half(hp))
}

/// Splits a roof line into the parts below and above the cut.
fn split_line(l: &RoofLine, cut: f64) -> (Vec<Segment2>, Vec<Segment2>) {
    let (za, zb) = l.z;
    let s = l.segment;
    if za <= cut && zb <= cut {
        return (vec![s], Vec::new());
    }
    if za >= cut && zb >= cut {
        return (Vec::new(), vec![s]);
    }
    let t = ((cut - za) / (zb - za)).clamp(0.0, 1.0);
    let m = s.at(t);
    if za < cut {
        (vec![Segment2::new(s.start, m)], vec![Segment2::new(m, s.end)])
    } else {
        (vec![Segment2::new(m, s.end)], vec![Segment2::new(s.start, m)])
    }
}

/// The 3D solids of a roof: one closed mesh per layer (pitched: the planes with thickness measured
/// perpendicular to them, vertical cuts at eaves and gables; flat: stacked layers), plus a parapet.
pub(crate) fn roof_solids(doc: &Document, e: &Element) -> Result<Vec<ElementSolid>, GeometryError> {
    let ElementKind::Roof(r) = &e.kind else {
        return Err(GeometryError::WrongKind { id: e.id, expected: "roof" });
    };
    let g = roof_geometry(doc, e)?;
    let err = |x: kanavos_solid::SolidError| GeometryError::Invalid {
        id: Some(e.id),
        field: "geometry",
        reason: x.to_string(),
    };
    let mut out = Vec::new();
    let mut depth = 0.0;
    for (k, (material, t)) in g.layers.iter().enumerate() {
        if *t <= 1e-6 {
            continue;
        }
        let mut pieces = Vec::new();
        for p in &g.planes {
            let top = p.height.shifted(-vertical(depth, p.pitch));
            let bottom = p.height.shifted(-vertical(depth + t, p.pitch));
            for region in &p.region {
                pieces.push(ColumnPiece { region: region.outer.clone(), intervals: vec![(bottom, top)] });
            }
        }
        let mesh = column_solid(&pieces, 1e-6).map_err(err)?;
        out.push(ElementSolid { mesh, material: Some(*material), part: SolidPart::Layer(k) });
        depth += t;
    }
    if let RoofShape::Flat { parapet: Some(par) } = &r.shape
        && par.height.as_m() > 1e-6
        && par.thickness.as_m() > 1e-6
    {
        let outer = r.outline.to_ccw();
        let inner =
            offset_polygon(&outer, -par.thickness.as_m(), JoinStyle::Miter { limit: 4.0 }).map_err(|x| {
                GeometryError::Invalid { id: Some(e.id), field: "parapet", reason: x.to_string() }
            })?;
        let ring = PolygonWithHoles { outer, holes: inner.into_iter().map(|p| p.outer.to_cw()).collect() };
        let mesh = extrude(&ring, g.base, g.base + par.height.as_m()).map_err(err)?;
        out.push(ElementSolid { mesh, material: g.core, part: SolidPart::Parapet });
    }
    Ok(out)
}

/// A roof plane seen from a wall below it: where the roof's underside may cap the wall.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RoofCap {
    region: Polygon2,
    underside: ZPlane,
}

impl RoofCap {
    /// Roof planes on the wall's storey whose underside dips below the wall's top over the wall.
    pub(crate) fn for_wall(doc: &Document, w: &WallInfo) -> Vec<RoofCap> {
        let Some(storey) = doc.element(w.id).and_then(|e| e.storey) else { return Vec::new() };
        let mut out = Vec::new();
        for e in doc.elements_on(storey) {
            let ElementKind::Roof(r) = &e.kind else { continue };
            let Some(b) = r.outline.bbox() else { continue };
            if !b.intersects(&w.bbox) {
                continue;
            }
            // Quick reject: the lowest possible underside is above the wall.
            let t = doc.structure_thickness(&r.structure).map(|t| t.as_m()).unwrap_or(0.0);
            let steepest = match &r.shape {
                RoofShape::Flat { .. } => 0.0,
                RoofShape::Pitched { slopes } => {
                    slopes.iter().flatten().map(|a| a.as_rad()).fold(0.0f64, f64::max)
                }
            };
            let lowest = storey_elevation(doc, e) + r.base.as_m() - vertical(t, Angle::rad(steepest));
            if lowest >= w.top - 1e-6 {
                continue;
            }
            let Ok(g) = roof_geometry(doc, e) else { continue };
            for p in &g.planes {
                let underside = p.height.shifted(-vertical(g.thickness, p.pitch));
                for region in &p.region {
                    out.push(RoofCap { region: region.outer.clone(), underside });
                }
            }
        }
        out
    }

    /// Plan region (within this cap) where the underside is below `cut`: the wall there ends below the cut.
    pub(crate) fn below(&self, cut: f64, _base: f64) -> Vec<Polygon2> {
        clip_by_height(vec![self.region.clone()], &self.underside, cut, false)
    }

    /// Splits wall pieces (with their current tops) where this cap lowers the top, and drops parts where the
    /// underside is below `base`.
    pub(crate) fn split(&self, pieces: Vec<(Polygon2, ZPlane)>, base: f64) -> Vec<(Polygon2, ZPlane)> {
        let mut out = Vec::with_capacity(pieces.len());
        let cap: PolygonWithHoles = self.region.clone().into();
        for (piece, top) in pieces {
            let pp: PolygonWithHoles = piece.clone().into();
            let (Some(pb), Some(cb)) = (piece.bbox(), self.region.bbox()) else {
                out.push((piece, top));
                continue;
            };
            if !pb.intersects(&cb) {
                out.push((piece, top));
                continue;
            }
            let inside = kanavos_geom::intersection(std::slice::from_ref(&pp), std::slice::from_ref(&cap))
                .unwrap_or_default();
            if inside.is_empty() {
                out.push((piece, top));
                continue;
            }
            let outside = kanavos_geom::difference(std::slice::from_ref(&pp), std::slice::from_ref(&cap))
                .unwrap_or_default();
            out.extend(outside.into_iter().map(|p| (p.outer, top)));
            for part in inside {
                // Where the underside is lower than the current top, it becomes the top.
                let diff = ZPlane {
                    z0: top.z0 - self.underside.z0,
                    gx: top.gx - self.underside.gx,
                    gy: top.gy - self.underside.gy,
                };
                let lowered = clip_by_height(vec![part.outer.clone()], &diff, 0.0, true);
                let kept = clip_by_height(vec![part.outer], &diff, 0.0, false);
                out.extend(kept.into_iter().map(|p| (p, top)));
                for l in lowered {
                    // Drop what lies where the underside is below the wall base.
                    for q in clip_by_height(vec![l], &self.underside, base + 1e-6, true) {
                        out.push((q, self.underside));
                    }
                }
            }
        }
        out
    }
}
