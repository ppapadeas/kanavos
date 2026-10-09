//! Reading geometry out of an IFC instance graph: units, placements, representations, profiles and curves.
//!
//! Everything here follows references lazily from the parsed file and works for IFC 2x3, IFC 4 and IFC 4.3:
//! the attributes used have the same positions in all three.

use std::collections::HashMap;

use kanavos_geom::{Point2, Point3, Polygon2, PolygonWithHoles, Vec2, Vec3};

use crate::math::{Frame2, Frame3};
use crate::step::{Entity, StepFile, Value};

/// Deepest chain of placements, mapped items or nested curves followed before giving up (guards against cycles).
const MAX_DEPTH: u32 = 64;
/// Segments used for full circles.
const CIRCLE_SEGMENTS: usize = 32;

/// Scale factors from file units to SI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Units {
    /// Metres per file length unit.
    pub length: f64,
    /// Radians per file plane-angle unit.
    pub angle: f64,
}

impl Default for Units {
    fn default() -> Self {
        Units { length: 1.0, angle: 1.0 }
    }
}

fn prefix_factor(prefix: Option<&str>) -> f64 {
    match prefix {
        Some("EXA") => 1e18,
        Some("PETA") => 1e15,
        Some("TERA") => 1e12,
        Some("GIGA") => 1e9,
        Some("MEGA") => 1e6,
        Some("KILO") => 1e3,
        Some("HECTO") => 1e2,
        Some("DECA") => 1e1,
        Some("DECI") => 1e-1,
        Some("CENTI") => 1e-2,
        Some("MILLI") => 1e-3,
        Some("MICRO") => 1e-6,
        Some("NANO") => 1e-9,
        Some("PICO") => 1e-12,
        Some("FEMTO") => 1e-15,
        Some("ATTO") => 1e-18,
        _ => 1.0,
    }
}

/// The unit type and SI factor of a named unit (`IfcSIUnit`, `IfcConversionBasedUnit`), following conversion
/// factors recursively.
pub fn unit_factor(f: &StepFile, id: u64, depth: u32) -> Option<(String, f64)> {
    if depth > MAX_DEPTH {
        return None;
    }
    let e = f.get(id)?;
    let kind = e.enum_at(1)?.to_owned();
    match e.name.as_ref() {
        "IFCSIUNIT" => {
            let p = prefix_factor(e.enum_at(2));
            let factor = match e.enum_at(3) {
                Some("SQUARE_METRE") => p * p,
                Some("CUBIC_METRE") => p * p * p,
                _ => p,
            };
            Some((kind, factor))
        }
        "IFCCONVERSIONBASEDUNIT" | "IFCCONVERSIONBASEDUNITWITHOFFSET" => {
            let m = f.get(e.ref_at(3)?)?;
            let value = m.f64_at(0)?;
            let base = m.ref_at(1).and_then(|u| unit_factor(f, u, depth + 1)).map(|(_, k)| k).unwrap_or(1.0);
            Some((kind, value * base))
        }
        _ => None,
    }
}

/// Length and angle units of a project (`IfcProject.UnitsInContext`); metres and radians when absent.
pub fn project_units(f: &StepFile, project: &Entity) -> Units {
    let mut units = Units::default();
    let Some(assignment) = project.ref_at(8).and_then(|id| f.get(id)) else { return units };
    for u in assignment.refs_at(0) {
        match unit_factor(f, u, 0) {
            Some((kind, k)) if kind == "LENGTHUNIT" && k > 0.0 && k.is_finite() => units.length = k,
            Some((kind, k)) if kind == "PLANEANGLEUNIT" && k > 0.0 && k.is_finite() => units.angle = k,
            _ => {}
        }
    }
    units
}

/// Geometry reader with memoised placements.
pub struct Geometry<'a> {
    /// The file.
    pub f: &'a StepFile,
    /// Units.
    pub units: Units,
    placements: HashMap<u64, Frame3>,
}

/// One extruded solid found in a representation.
#[derive(Debug, Clone)]
pub struct Extrusion {
    /// Frame of the swept area (profile plane = local XY), relative to the product placement.
    pub frame: Frame3,
    /// The profile.
    pub profile: Profile,
    /// Extrusion vector in `frame` coordinates (direction times depth), metres.
    pub vector: Vec3,
}

/// The shape of a profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProfileKind {
    /// A rectangle centred on the profile position.
    Rectangle {
        /// Size along the position's X axis.
        xdim: f64,
        /// Size along the position's Y axis.
        ydim: f64,
    },
    /// A circle centred on the profile position.
    Circle {
        /// Radius.
        radius: f64,
    },
    /// Anything else.
    Other,
}

/// An area profile.
#[derive(Debug, Clone)]
pub struct Profile {
    /// Parametric kind.
    pub kind: ProfileKind,
    /// Position of the parametric shape in the profile plane.
    pub position: Frame2,
    /// The area as polygons, in profile-plane coordinates (position applied), metres.
    pub area: PolygonWithHoles,
}

impl<'a> Geometry<'a> {
    /// A reader over a file.
    pub fn new(f: &'a StepFile, units: Units) -> Geometry<'a> {
        Geometry { f, units, placements: HashMap::new() }
    }

    fn coords(&self, point: u64) -> Option<Vec<f64>> {
        let e = self.f.get(point)?;
        if e.name.as_ref() != "IFCCARTESIANPOINT" {
            return None;
        }
        let c: Vec<f64> = e.list_at(0)?.iter().filter_map(Value::as_f64).collect();
        if c.iter().all(|v| v.is_finite()) { Some(c) } else { None }
    }

    /// A point in metres (2D points get z = 0).
    pub fn point3(&self, id: u64) -> Option<Point3> {
        let c = self.coords(id)?;
        let s = self.units.length;
        match c.as_slice() {
            [x, y] => Some(Point3::new(x * s, y * s, 0.0)),
            [x, y, z] => Some(Point3::new(x * s, y * s, z * s)),
            _ => None,
        }
    }

    /// A 2D point in metres (the z of 3D points is dropped).
    pub fn point2(&self, id: u64) -> Option<Point2> {
        self.point3(id).map(|p| Point2::new(p.x, p.y))
    }

    /// A direction (unitless, normalised).
    pub fn direction(&self, id: u64) -> Option<Vec3> {
        let e = self.f.get(id)?;
        if e.name.as_ref() != "IFCDIRECTION" {
            return None;
        }
        let c: Vec<f64> = e.list_at(0)?.iter().filter_map(Value::as_f64).collect();
        match c.as_slice() {
            [x, y] => Vec3::new(*x, *y, 0.0).normalized(),
            [x, y, z] => Vec3::new(*x, *y, *z).normalized(),
            _ => None,
        }
    }

    /// `IfcAxis2Placement3D` or `IfcAxis2Placement2D` as a frame.
    pub fn axis_placement(&self, id: u64) -> Option<Frame3> {
        let e = self.f.get(id)?;
        let origin = e.ref_at(0).and_then(|p| self.point3(p)).unwrap_or(Point3::ORIGIN);
        match e.name.as_ref() {
            "IFCAXIS2PLACEMENT3D" => Some(Frame3::from_axes(
                origin,
                e.ref_at(1).and_then(|d| self.direction(d)),
                e.ref_at(2).and_then(|d| self.direction(d)),
            )),
            "IFCAXIS2PLACEMENT2D" => {
                Some(Frame3::from_axes(origin, None, e.ref_at(1).and_then(|d| self.direction(d))))
            }
            _ => None,
        }
    }

    fn axis_placement2(&self, id: Option<u64>) -> Frame2 {
        let Some(e) = id.and_then(|id| self.f.get(id)) else { return Frame2::IDENTITY };
        let o = e.ref_at(0).and_then(|p| self.point2(p)).map(Point2::to_vec).unwrap_or(Vec2::ZERO);
        let x = e.ref_at(1).and_then(|d| self.direction(d)).map(|v| Vec2::new(v.x, v.y));
        Frame2::from_axes(o, x)
    }

    /// World frame of an object placement (`IfcLocalPlacement` chains). Unknown placements are the identity.
    pub fn placement(&mut self, id: u64) -> Frame3 {
        self.placement_at(id, 0)
    }

    fn placement_at(&mut self, id: u64, depth: u32) -> Frame3 {
        if let Some(f) = self.placements.get(&id) {
            return *f;
        }
        if depth > MAX_DEPTH {
            return Frame3::IDENTITY;
        }
        let Some(e) = self.f.get(id) else { return Frame3::IDENTITY };
        let frame = match e.name.as_ref() {
            "IFCLOCALPLACEMENT" => {
                let parent = e.ref_at(0).map(|p| self.placement_at(p, depth + 1)).unwrap_or(Frame3::IDENTITY);
                let local = e.ref_at(1).and_then(|a| self.axis_placement(a)).unwrap_or(Frame3::IDENTITY);
                parent.then(&local)
            }
            _ => Frame3::IDENTITY,
        };
        self.placements.insert(id, frame);
        frame
    }

    /// World frame of a product (`ObjectPlacement`, attribute 5).
    pub fn product_frame(&mut self, product: &Entity) -> Frame3 {
        product.ref_at(5).map(|p| self.placement(p)).unwrap_or(Frame3::IDENTITY)
    }

    /// The representation of a product with the given identifier (case-insensitive), e.g. `Body` or `Axis`.
    /// For `Body`, representations without an identifier but with a solid representation type also qualify.
    pub fn representation(&self, product: &Entity, identifier: &str) -> Option<u64> {
        let shape = self.f.get(product.ref_at(6)?)?;
        let reps = shape.refs_at(2);
        let found = reps.iter().copied().find(|&r| {
            self.f.get(r).and_then(|e| e.str_at(1)).is_some_and(|i| i.eq_ignore_ascii_case(identifier))
        });
        found.or_else(|| {
            if !identifier.eq_ignore_ascii_case("Body") {
                return None;
            }
            reps.iter().copied().find(|&r| {
                self.f.get(r).is_some_and(|e| {
                    e.str_at(1).is_none()
                        && matches!(
                            e.str_at(2),
                            Some(
                                "SweptSolid"
                                    | "Clipping"
                                    | "CSG"
                                    | "MappedRepresentation"
                                    | "AdvancedSweptSolid"
                            )
                        )
                })
            })
        })
    }

    /// Extruded solids in a shape representation, with frames relative to the product placement. Boolean results
    /// contribute their first operand; mapped items are followed. `notes` collects what was simplified.
    pub fn extrusions(&self, representation: u64, notes: &mut Vec<String>) -> Vec<Extrusion> {
        let mut out = Vec::new();
        if let Some(rep) = self.f.get(representation) {
            for item in rep.refs_at(3) {
                self.item_extrusions(item, Frame3::IDENTITY, 0, &mut out, notes);
            }
        }
        out
    }

    fn item_extrusions(
        &self,
        item: u64,
        at: Frame3,
        depth: u32,
        out: &mut Vec<Extrusion>,
        notes: &mut Vec<String>,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        let Some(e) = self.f.get(item) else { return };
        match e.name.as_ref() {
            "IFCEXTRUDEDAREASOLID" | "IFCEXTRUDEDAREASOLIDTAPERED" => {
                let Some(profile) = e.ref_at(0).and_then(|p| self.profile(p, 0)) else {
                    notes.push("unsupported profile".into());
                    return;
                };
                let position = e.ref_at(1).and_then(|p| self.axis_placement(p)).unwrap_or(Frame3::IDENTITY);
                let Some(dir) = e.ref_at(2).and_then(|d| self.direction(d)) else { return };
                let Some(depth) = e.f64_at(3).map(|d| d * self.units.length) else { return };
                if !(depth > 0.0 && depth.is_finite()) {
                    return;
                }
                out.push(Extrusion { frame: at.then(&position), profile, vector: dir * depth });
            }
            "IFCBOOLEANCLIPPINGRESULT" | "IFCBOOLEANRESULT" => {
                if e.enum_at(0) != Some("DIFFERENCE") {
                    notes.push("boolean union or intersection read as its first operand".into());
                } else {
                    notes.push("clipping or subtraction ignored".into());
                }
                if let Some(first) = e.ref_at(1) {
                    self.item_extrusions(first, at, depth + 1, out, notes);
                }
            }
            "IFCMAPPEDITEM" => {
                let Some(source) = e.ref_at(0).and_then(|s| self.f.get(s)) else { return };
                let origin =
                    source.ref_at(0).and_then(|o| self.axis_placement(o)).unwrap_or(Frame3::IDENTITY);
                let target = e.ref_at(1).and_then(|t| self.transformation(t)).unwrap_or(Frame3::IDENTITY);
                let frame = at.then(&target).then(&origin);
                let Some(rep) = source.ref_at(1).and_then(|r| self.f.get(r)) else { return };
                for sub in rep.refs_at(3) {
                    self.item_extrusions(sub, frame, depth + 1, out, notes);
                }
            }
            _ => notes.push(format!("{} body read without its geometry", e.name.to_ascii_lowercase())),
        }
    }

    /// `IfcCartesianTransformationOperator3D` (uniform or non-uniform) as a frame.
    fn transformation(&self, id: u64) -> Option<Frame3> {
        let e = self.f.get(id)?;
        let axis1 = e.ref_at(0).and_then(|d| self.direction(d));
        let axis2 = e.ref_at(1).and_then(|d| self.direction(d));
        let origin = e.ref_at(2).and_then(|p| self.point3(p)).unwrap_or(Point3::ORIGIN);
        let scale = e.f64_at(3).unwrap_or(1.0);
        let axis3 = e.ref_at(4).and_then(|d| self.direction(d));
        let (s2, s3) = match e.name.as_ref() {
            "IFCCARTESIANTRANSFORMATIONOPERATOR3DNONUNIFORM" => {
                (e.f64_at(5).unwrap_or(scale), e.f64_at(6).unwrap_or(scale))
            }
            "IFCCARTESIANTRANSFORMATIONOPERATOR3D" => (scale, scale),
            _ => return None,
        };
        // Base axes: Z from axis3, X from axis1 made orthogonal, Y completes the frame (axis2 only fixes sense).
        let base = Frame3::from_axes(origin, axis3, axis1);
        let y = match axis2 {
            Some(a2) if a2.dot(base.y) < 0.0 => base.y * -1.0,
            _ => base.y,
        };
        Some(Frame3 { x: base.x * scale, y: y * s2, z: base.z * s3, origin })
    }

    /// An area profile.
    pub fn profile(&self, id: u64, depth: u32) -> Option<Profile> {
        if depth > MAX_DEPTH {
            return None;
        }
        let e = self.f.get(id)?;
        let s = self.units.length;
        let name = e.name.as_ref();
        match name {
            "IFCRECTANGLEPROFILEDEF" | "IFCRECTANGLEHOLLOWPROFILEDEF" | "IFCROUNDEDRECTANGLEPROFILEDEF" => {
                let position = self.axis_placement2(e.ref_at(2));
                let (xdim, ydim) = (e.f64_at(3)? * s, e.f64_at(4)? * s);
                if !(xdim > 0.0 && ydim > 0.0) {
                    return None;
                }
                let (hx, hy) = (xdim / 2.0, ydim / 2.0);
                let corners = [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
                    .iter()
                    .map(|&(x, y)| {
                        let v = position.apply(Vec2::new(x, y));
                        Point2::new(v.x, v.y)
                    })
                    .collect();
                Some(Profile {
                    kind: ProfileKind::Rectangle { xdim, ydim },
                    position,
                    area: PolygonWithHoles::from(Polygon2::new(corners)),
                })
            }
            "IFCCIRCLEPROFILEDEF" | "IFCCIRCLEHOLLOWPROFILEDEF" => {
                let position = self.axis_placement2(e.ref_at(2));
                let radius = e.f64_at(3)? * s;
                if radius.is_nan() || radius <= 0.0 {
                    return None;
                }
                let ring = ellipse_points(radius, radius, CIRCLE_SEGMENTS)
                    .into_iter()
                    .map(|v| {
                        let w = position.apply(v);
                        Point2::new(w.x, w.y)
                    })
                    .collect();
                Some(Profile {
                    kind: ProfileKind::Circle { radius },
                    position,
                    area: Polygon2::new(ring).into(),
                })
            }
            "IFCELLIPSEPROFILEDEF" => {
                let position = self.axis_placement2(e.ref_at(2));
                let (a, b) = (e.f64_at(3)? * s, e.f64_at(4)? * s);
                let ring = ellipse_points(a, b, CIRCLE_SEGMENTS)
                    .into_iter()
                    .map(|v| {
                        let w = position.apply(v);
                        Point2::new(w.x, w.y)
                    })
                    .collect();
                Some(Profile { kind: ProfileKind::Other, position, area: Polygon2::new(ring).into() })
            }
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let outer = Polygon2::new(self.curve_points(e.ref_at(2)?, 0)?);
                let holes = if name == "IFCARBITRARYPROFILEDEFWITHVOIDS" {
                    e.refs_at(3)
                        .into_iter()
                        .filter_map(|c| self.curve_points(c, 0))
                        .map(Polygon2::new)
                        .collect()
                } else {
                    Vec::new()
                };
                if outer.len() < 3 {
                    return None;
                }
                Some(Profile {
                    kind: ProfileKind::Other,
                    position: Frame2::IDENTITY,
                    area: PolygonWithHoles { outer, holes }.oriented(),
                })
            }
            "IFCDERIVEDPROFILEDEF" => {
                let parent = self.profile(e.ref_at(2)?, depth + 1)?;
                let op = self.f.get(e.ref_at(3)?)?;
                let axis1 = op.ref_at(0).and_then(|d| self.direction(d)).map(|v| Vec2::new(v.x, v.y));
                let origin =
                    op.ref_at(2).and_then(|p| self.point2(p)).map(Point2::to_vec).unwrap_or(Vec2::ZERO);
                let scale = op.f64_at(3).unwrap_or(1.0);
                let mut t = Frame2::from_axes(origin, axis1);
                t.x = t.x * scale;
                t.y = t.y * op.f64_at(4).unwrap_or(scale);
                let map = |p: &Polygon2| {
                    Polygon2::new(
                        p.vertices
                            .iter()
                            .map(|q| {
                                let v = t.apply(q.to_vec());
                                Point2::new(v.x, v.y)
                            })
                            .collect(),
                    )
                };
                let area = PolygonWithHoles {
                    outer: map(&parent.area.outer),
                    holes: parent.area.holes.iter().map(map).collect(),
                }
                .oriented();
                Some(Profile { kind: ProfileKind::Other, position: t.then(&parent.position), area })
            }
            _ => None,
        }
    }

    /// Points of a bounded 2D curve, in metres (closing duplicate removed).
    pub fn curve_points(&self, id: u64, depth: u32) -> Option<Vec<Point2>> {
        if depth > MAX_DEPTH {
            return None;
        }
        let e = self.f.get(id)?;
        let mut pts: Vec<Point2> = match e.name.as_ref() {
            "IFCPOLYLINE" => e.refs_at(0).into_iter().filter_map(|p| self.point2(p)).collect(),
            "IFCINDEXEDPOLYCURVE" => self.indexed_poly_curve(e)?,
            "IFCCOMPOSITECURVE" | "IFCCOMPOSITECURVEONSURFACE" => {
                let mut all: Vec<Point2> = Vec::new();
                for seg in e.refs_at(0) {
                    let Some(s) = self.f.get(seg) else { continue };
                    let Some(parent) = s.ref_at(2) else { continue };
                    let Some(mut part) = self.curve_points(parent, depth + 1) else { continue };
                    if s.attr(1).and_then(Value::as_bool) == Some(false) {
                        part.reverse();
                    }
                    for p in part {
                        if all.last().is_none_or(|q| q.distance(p).as_m() > 1e-9) {
                            all.push(p);
                        }
                    }
                }
                all
            }
            "IFCTRIMMEDCURVE" => self.trimmed_curve(e)?,
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let pos = self.axis_placement(e.ref_at(0)?)?;
                let a = e.f64_at(1)? * self.units.length;
                let b = if e.name.as_ref() == "IFCELLIPSE" { e.f64_at(2)? * self.units.length } else { a };
                ellipse_points(a, b, CIRCLE_SEGMENTS)
                    .into_iter()
                    .map(|v| {
                        let p = pos.apply2(v.x, v.y);
                        Point2::new(p.x, p.y)
                    })
                    .collect()
            }
            _ => return None,
        };
        while pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b).as_m() < 1e-9)
        {
            pts.pop();
        }
        if pts.len() >= 2 { Some(pts) } else { None }
    }

    fn indexed_poly_curve(&self, e: &Entity) -> Option<Vec<Point2>> {
        let list = self.f.get(e.ref_at(0)?)?;
        let s = self.units.length;
        let coords: Vec<Point2> = list
            .list_at(0)?
            .iter()
            .filter_map(|c| {
                let c: Vec<f64> = c.as_list()?.iter().filter_map(Value::as_f64).collect();
                match c.as_slice() {
                    [x, y] | [x, y, _] => Some(Point2::new(x * s, y * s)),
                    _ => None,
                }
            })
            .collect();
        let at = |i: i64| -> Option<Point2> { coords.get(usize::try_from(i).ok()?.checked_sub(1)?).copied() };
        let Some(segments) = e.list_at(1) else { return Some(coords) };
        let mut out: Vec<Point2> = Vec::new();
        let push = |p: Point2, out: &mut Vec<Point2>| {
            if out.last().is_none_or(|q| q.distance(p).as_m() > 1e-9) {
                out.push(p);
            }
        };
        for seg in segments {
            let idx: Vec<i64> =
                seg.as_list().map(|l| l.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
            match seg.type_name() {
                Some("IFCARCINDEX") => {
                    if let [a, b, c] = idx.as_slice()
                        && let (Some(p), Some(q), Some(r)) = (at(*a), at(*b), at(*c))
                    {
                        for x in arc_through(p, q, r) {
                            push(x, &mut out);
                        }
                    }
                }
                _ => {
                    for i in idx {
                        if let Some(p) = at(i) {
                            push(p, &mut out);
                        }
                    }
                }
            }
        }
        Some(out)
    }

    fn trimmed_curve(&self, e: &Entity) -> Option<Vec<Point2>> {
        let basis = self.f.get(e.ref_at(0)?)?;
        let sense = e.attr(3).and_then(Value::as_bool).unwrap_or(true);
        let trim = |i: usize| -> (Option<Point2>, Option<f64>) {
            let list = e.list_at(i).unwrap_or(&[]);
            let point = list.iter().find_map(|v| v.as_ref_id()).and_then(|p| self.point2(p));
            let param =
                list.iter().find(|v| v.type_name() == Some("IFCPARAMETERVALUE")).and_then(Value::as_f64);
            (point, param)
        };
        let (t1, t2) = (trim(1), trim(2));
        match basis.name.as_ref() {
            "IFCLINE" => {
                let origin = self.point2(basis.ref_at(0)?)?;
                let vector = self.f.get(basis.ref_at(1)?)?;
                let dir = self.direction(vector.ref_at(0)?)?;
                let magnitude = vector.f64_at(1).unwrap_or(1.0) * self.units.length;
                let along = |t: f64| origin + Vec2::new(dir.x, dir.y) * (t * magnitude);
                let a = t1.0.or_else(|| t1.1.map(along))?;
                let b = t2.0.or_else(|| t2.1.map(along))?;
                Some(if sense { vec![a, b] } else { vec![b, a] })
            }
            "IFCCIRCLE" => {
                let pos = self.axis_placement(basis.ref_at(0)?)?;
                let r = basis.f64_at(1)? * self.units.length;
                let inv = pos.inverse()?;
                let angle = |t: (Option<Point2>, Option<f64>)| -> Option<f64> {
                    match t {
                        (Some(p), _) => {
                            let l = inv.apply(Point3::from_plan(p, 0.0));
                            Some(l.y.atan2(l.x))
                        }
                        (None, Some(v)) => Some(v * self.units.angle),
                        _ => None,
                    }
                };
                let (a1, a2) = (angle(t1)?, angle(t2)?);
                let tau = std::f64::consts::TAU;
                let sweep = if sense { (a2 - a1).rem_euclid(tau) } else { -((a1 - a2).rem_euclid(tau)) };
                let n = ((sweep.abs() / tau * CIRCLE_SEGMENTS as f64).ceil() as usize).max(2);
                Some(
                    (0..=n)
                        .map(|i| {
                            let a = a1 + sweep * i as f64 / n as f64;
                            let p = pos.apply2(r * a.cos(), r * a.sin());
                            Point2::new(p.x, p.y)
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }
}

/// Whether a trimmed curve or indexed curve is curved (has an arc).
pub fn is_curved(f: &StepFile, curve: u64) -> bool {
    let Some(e) = f.get(curve) else { return false };
    match e.name.as_ref() {
        "IFCTRIMMEDCURVE" => e.ref_at(0).and_then(|b| f.type_of(b)) == Some("IFCCIRCLE"),
        "IFCINDEXEDPOLYCURVE" => {
            e.list_at(1).is_some_and(|segs| segs.iter().any(|s| s.type_name() == Some("IFCARCINDEX")))
        }
        _ => false,
    }
}

fn ellipse_points(a: f64, b: f64, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            Vec2::new(a * t.cos(), b * t.sin())
        })
        .collect()
}

/// Points along the circular arc through three points (start, middle, end).
fn arc_through(p: Point2, q: Point2, r: Point2) -> Vec<Point2> {
    let (a, b, c) = (p.to_vec(), q.to_vec(), r.to_vec());
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 {
        return vec![p, q, r];
    }
    let sq = |v: Vec2| v.length_squared();
    let ux = (sq(a) * (b.y - c.y) + sq(b) * (c.y - a.y) + sq(c) * (a.y - b.y)) / d;
    let uy = (sq(a) * (c.x - b.x) + sq(b) * (a.x - c.x) + sq(c) * (b.x - a.x)) / d;
    let centre = Vec2::new(ux, uy);
    let radius = (a - centre).length();
    let angle = |v: Vec2| (v.y - uy).atan2(v.x - ux);
    let (a0, am, a1) = (angle(a), angle(b), angle(c));
    let tau = std::f64::consts::TAU;
    let ccw = (am - a0).rem_euclid(tau) < (a1 - a0).rem_euclid(tau);
    let sweep = if ccw { (a1 - a0).rem_euclid(tau) } else { -((a0 - a1).rem_euclid(tau)) };
    let n = ((sweep.abs() / tau * CIRCLE_SEGMENTS as f64).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let t = a0 + sweep * i as f64 / n as f64;
            Point2::new(ux + radius * t.cos(), uy + radius * t.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::parse;

    fn file(data: &str) -> StepFile {
        let text = format!(
            "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\n{data}\nENDSEC;END-ISO-10303-21;"
        );
        parse(text.as_bytes()).unwrap()
    }

    #[test]
    fn units_with_prefixes_and_conversions() {
        let f = file(
            "#1=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n\
             #2=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);\n\
             #3=IFCMEASUREWITHUNIT(IFCLENGTHMEASURE(0.0254),#2);\n\
             #4=IFCDIMENSIONALEXPONENTS(1,0,0,0,0,0,0);\n\
             #5=IFCCONVERSIONBASEDUNIT(#4,.LENGTHUNIT.,'INCH',#3);\n\
             #6=IFCSIUNIT(*,.AREAUNIT.,.CENTI.,.SQUARE_METRE.);",
        );
        assert_eq!(unit_factor(&f, 1, 0).map(|(_, k)| k), Some(1e-3));
        let (kind, inch) = unit_factor(&f, 5, 0).unwrap();
        assert_eq!(kind, "LENGTHUNIT");
        assert!((inch - 0.0254).abs() < 1e-15);
        assert!((unit_factor(&f, 6, 0).unwrap().1 - 1e-4).abs() < 1e-18);
    }

    #[test]
    fn placement_chains_compose() {
        let f = file(
            "#1=IFCCARTESIANPOINT((1000.,0.,500.));\n#2=IFCDIRECTION((0.,1.,0.));\n\
             #3=IFCAXIS2PLACEMENT3D(#1,$,#2);\n#4=IFCLOCALPLACEMENT($,#3);\n\
             #5=IFCCARTESIANPOINT((2000.,0.,0.));\n#6=IFCAXIS2PLACEMENT3D(#5,$,$);\n#7=IFCLOCALPLACEMENT(#4,#6);",
        );
        let mut g = Geometry::new(&f, Units { length: 1e-3, angle: 1.0 });
        let w = g.placement(7);
        // Parent rotated 90°: the child's +2 m along local X is +2 m along world Y.
        assert!(w.origin.distance(Point3::new(1.0, 2.0, 0.5)) < 1e-12);
    }

    #[test]
    fn profiles_and_curves() {
        let f = file(
            "#1=IFCCARTESIANPOINT((0.,0.));#2=IFCCARTESIANPOINT((4.,0.));#3=IFCCARTESIANPOINT((4.,3.));\n\
             #4=IFCPOLYLINE((#1,#2,#3,#1));\n#5=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#4);\n\
             #6=IFCCARTESIANPOINTLIST2D(((0.,0.),(2.,0.),(3.,1.),(2.,2.),(0.,2.)));\n\
             #7=IFCINDEXEDPOLYCURVE(#6,(IFCLINEINDEX((1,2)),IFCARCINDEX((2,3,4)),IFCLINEINDEX((4,5,1))),$);\n\
             #8=IFCCARTESIANPOINT((1.,1.));#9=IFCAXIS2PLACEMENT2D(#8,$);\n\
             #10=IFCRECTANGLEPROFILEDEF(.AREA.,$,#9,2.,0.5);",
        );
        let g = Geometry::new(&f, Units::default());
        let p = g.profile(5, 0).unwrap();
        assert!((p.area.area().as_m2() - 6.0).abs() < 1e-12);
        let pts = g.curve_points(7, 0).unwrap();
        // Two straight edges, a half circle of radius 1 and the closing edge: area 4 + π/2.
        let area = Polygon2::new(pts).area().as_m2();
        assert!((area - (4.0 + std::f64::consts::FRAC_PI_2)).abs() < 0.02, "{area}");
        let r = g.profile(10, 0).unwrap();
        assert_eq!(r.kind, ProfileKind::Rectangle { xdim: 2.0, ydim: 0.5 });
        let bb = r.area.bbox().unwrap();
        assert!((bb.min.x.as_m()).abs() < 1e-12 && (bb.max.y.as_m() - 1.25).abs() < 1e-12);
    }
}
