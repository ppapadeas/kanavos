//! Railings: posts, top rail and infill (bars, glass or solid panels) in 3D, and their plan representation.
//!
//! A railing stands either at one base height (`base_offset`) or, when it has one elevation per path point, on a
//! base that runs straight between those points, so that it can follow a stair flight or a ramp. On a slope the
//! posts and bars stay vertical, the rails follow the slope (their section stays square to the rail), and glass
//! and solid panels are cut parallel to the slope at the top and bottom.

use kanavos_geom::{
    CapStyle, JoinStyle, Point2, Point3, Polygon2, PolygonWithHoles, Segment2, Vec2, offset_polyline,
    stroke_polyline,
};
use kanavos_model::{Document, Element, ElementKind, Railing, RailingInfill};
use kanavos_solid::{Mesh, cuboid, extrude, rect_profile, sweep};
use kanavos_units::Length;

use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart};

/// Post section (square, metres).
pub const POST_SIZE: f64 = 0.04;
/// Top rail section: width and height (metres).
pub const RAIL_WIDTH: f64 = 0.05;
/// Top rail height (metres).
pub const RAIL_HEIGHT: f64 = 0.04;
/// Bar section (square, metres).
pub const BAR_SIZE: f64 = 0.016;
/// Clear gap between bars (metres); below the 0.15 m limit of Κτιριοδομικός Κανονισμός (ΦΕΚ Β΄ 3985/2023) άρθρο 13.
pub const BAR_GAP: f64 = 0.10;
/// Height of the bottom rail above the base (metres).
pub const BOTTOM_RAIL: f64 = 0.08;
/// Glass panel thickness (metres).
pub const GLASS_THICKNESS: f64 = 0.012;
/// Solid panel thickness (metres).
pub const PANEL_THICKNESS: f64 = 0.05;

fn railing_of(e: &Element) -> Result<&Railing, GeometryError> {
    match &e.kind {
        ElementKind::Railing(r) => Ok(r),
        _ => Err(GeometryError::WrongKind { id: e.id, expected: "railing" }),
    }
}

fn storey_floor(doc: &Document, e: &Element) -> Result<f64, GeometryError> {
    match e.storey {
        Some(s) => doc
            .storey(s)
            .map(|st| st.elevation.as_m())
            .ok_or_else(|| GeometryError::MissingReference { id: e.id, what: "storey".into() }),
        None => Ok(0.0),
    }
}

/// Distinct path points with the absolute base height at each (a point repeating the previous one in plan is
/// dropped).
fn path(r: &Railing, floor: f64) -> Vec<(Point2, f64)> {
    let mut v: Vec<(Point2, f64)> = Vec::with_capacity(r.path.len());
    for (i, &p) in r.path.iter().enumerate() {
        if v.last().is_none_or(|(q, _)| q.distance(p).as_m() > 1e-6) {
            v.push((p, floor + r.base_at(i).as_m()));
        }
    }
    v
}

/// Post positions: every path vertex, plus intermediate posts so that no span exceeds the post spacing, with the
/// absolute base height at each.
fn posts_along(r: &Railing, pts: &[(Point2, f64)]) -> Vec<(Point2, f64)> {
    let spacing = r.post_spacing.as_m().max(0.1);
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let [(a, za), (b, zb)] = w else { continue };
        let len = a.distance(*b).as_m();
        let n = ((len / spacing).ceil() as usize).max(1);
        for i in 0..n {
            let t = i as f64 / n as f64;
            out.push((a.lerp(*b, t), za + (zb - za) * t));
        }
    }
    if let Some(&last) = pts.last() {
        out.push(last);
    }
    out
}

fn box_along(a: Point2, b: Point2, thickness: f64, z0: f64, z1: f64) -> Option<Mesh> {
    let d = (b - a).normalized()?;
    let n = d.perp() * (thickness / 2.0);
    let poly = Polygon2::new(vec![a - n, b - n, b + n, a + n]);
    kanavos_solid::extrude_polygon(&poly, z0, z1).ok()
}

/// A panel in the vertical plane of segment `a → b`, `thickness` across and centred on it, between a bottom line
/// running from `bottom.0` (at `a`) to `bottom.1` (at `b`) and a top line from `top.0` to `top.1`.
fn panel_along(a: Point2, b: Point2, thickness: f64, bottom: (f64, f64), top: (f64, f64)) -> Option<Mesh> {
    let d = (b - a).normalized()?;
    let len = a.distance(b).as_m();
    // Profile in the panel's plane: x along the segment, y up.
    let profile = Polygon2::new(vec![
        Point2::new(0.0, bottom.0),
        Point2::new(len, bottom.1),
        Point2::new(len, top.1),
        Point2::new(0.0, top.0),
    ]);
    if profile.signed_area() <= 1e-9 {
        return None;
    }
    let local = extrude(&PolygonWithHoles::from(profile), -thickness / 2.0, thickness / 2.0).ok()?;
    let across = d.perp();
    let mesh = local.map_positions(|q| Point3::from_plan(a + d * q.x + across * q.z, q.y));
    // (x, y, z) → (along, up, left) reverses orientation.
    Some(mesh.flipped())
}

/// The 3D solids of a railing: posts, rails and infill. Posts and rails use the railing's material; the infill
/// its infill material (or the railing's material when it has none).
pub(crate) fn railing_solids(doc: &Document, e: &Element) -> Result<Vec<ElementSolid>, GeometryError> {
    let r = railing_of(e)?;
    let pts = path(r, storey_floor(doc, e)?);
    if pts.len() < 2 {
        return Err(GeometryError::Invalid {
            id: Some(e.id),
            field: "path",
            reason: "needs two distinct points".into(),
        });
    }
    let err = |x: kanavos_solid::SolidError| GeometryError::Invalid {
        id: Some(e.id),
        field: "geometry",
        reason: x.to_string(),
    };
    let h = r.height.as_m();
    let sloped = r.is_sloped();
    // Posts.
    let mut posts = Mesh::new();
    let half = POST_SIZE / 2.0;
    for (p, base) in posts_along(r, &pts) {
        let [x, y] = p.xy();
        let top = base + h - RAIL_HEIGHT;
        posts.append(&cuboid(Point3::new(x - half, y - half, base), Point3::new(x + half, y + half, top)));
    }
    // Top rail along the whole path.
    let path3: Vec<Point3> = pts.iter().map(|(p, base)| Point3::from_plan(*p, base + h)).collect();
    let mut rails = sweep(&path3, &rect_profile(RAIL_WIDTH, RAIL_HEIGHT, 0.0)).map_err(err)?;
    let mut infill = Mesh::new();
    let segments = || {
        pts.windows(2).filter_map(|w| match w {
            [(a, za), (c, zc)] => Some((*a, *za, *c, *zc)),
            _ => None,
        })
    };
    match r.infill {
        RailingInfill::Bars => {
            let low: Vec<Point3> =
                pts.iter().map(|(p, base)| Point3::from_plan(*p, base + BOTTOM_RAIL)).collect();
            rails
                .append(&sweep(&low, &rect_profile(RAIL_WIDTH * 0.8, RAIL_HEIGHT * 0.75, 0.0)).map_err(err)?);
            let b = BAR_SIZE / 2.0;
            for (a, za, c, zc) in segments() {
                let len = a.distance(c).as_m();
                let step = BAR_SIZE + BAR_GAP;
                let n = ((len - POST_SIZE) / step).floor().max(0.0) as usize;
                let d = (c - a).normalized().unwrap_or(Vec2::X);
                // Centre the bars between the posts.
                let used = n.saturating_sub(1) as f64 * step;
                let first = (len - used) / 2.0;
                for i in 0..n {
                    let s = first + step * i as f64;
                    let base = za + (zc - za) * (s / len.max(1e-9));
                    let p = a + d * s;
                    let [x, y] = p.xy();
                    infill.append(&cuboid(
                        Point3::new(x - b, y - b, base + BOTTOM_RAIL),
                        Point3::new(x + b, y + b, base + h - RAIL_HEIGHT),
                    ));
                }
            }
        }
        RailingInfill::Glass => {
            for (a, za, c, zc) in segments() {
                let len = a.distance(c).as_m();
                if len <= POST_SIZE + 0.02 {
                    continue;
                }
                let d = (c - a).normalized().unwrap_or(Vec2::X);
                let inset = POST_SIZE / 2.0 + 0.01;
                let (a2, c2) = (a + d * inset, c - d * inset);
                let k = (zc - za) / len;
                let (z_a, z_c) = (za + k * inset, zc - k * inset);
                let panel = if sloped {
                    panel_along(
                        a2,
                        c2,
                        GLASS_THICKNESS,
                        (z_a + 0.05, z_c + 0.05),
                        (z_a + h - RAIL_HEIGHT - 0.03, z_c + h - RAIL_HEIGHT - 0.03),
                    )
                } else {
                    box_along(a2, c2, GLASS_THICKNESS, za + 0.05, za + h - RAIL_HEIGHT - 0.03)
                };
                if let Some(m) = panel {
                    infill.append(&m);
                }
            }
        }
        RailingInfill::Solid => {
            if sloped {
                for (a, za, c, zc) in segments() {
                    let top = (za + h - RAIL_HEIGHT, zc + h - RAIL_HEIGHT);
                    if let Some(m) = panel_along(a, c, PANEL_THICKNESS, (za, zc), top) {
                        infill.append(&m);
                    }
                }
            } else {
                let base = pts.first().map(|(_, z)| *z).unwrap_or(0.0);
                let rail_bottom = base + h - RAIL_HEIGHT;
                let low: Vec<Point3> = pts.iter().map(|(p, _)| Point3::from_plan(*p, rail_bottom)).collect();
                infill = sweep(&low, &rect_profile(PANEL_THICKNESS, rail_bottom - base, 0.0)).map_err(err)?;
            }
        }
    }
    let mut out = vec![
        ElementSolid { mesh: posts, material: Some(r.material), part: SolidPart::Post },
        ElementSolid { mesh: rails, material: Some(r.material), part: SolidPart::Rail },
    ];
    if !infill.is_empty() {
        out.push(ElementSolid {
            mesh: infill,
            material: Some(r.infill_material_or_frame()),
            part: SolidPart::Infill,
        });
    }
    Ok(out)
}

/// Length of a railing along its rail: the plan length of each segment combined with the rise of its base.
pub fn railing_length(r: &Railing) -> Length {
    let pts = path(r, 0.0);
    Length::m(
        pts.windows(2)
            .map(|w| match w {
                [(a, za), (b, zb)] => {
                    let run = a.distance(*b).as_m();
                    (run * run + (zb - za) * (zb - za)).sqrt()
                }
                _ => 0.0,
            })
            .sum(),
    )
}

/// How a railing appears in plan: its two edge lines (seen when it starts below the cut plane, dashed when it is
/// entirely above); a solid panel crossed by the cut plane is a cut band instead. A sloped railing is drawn the
/// same way, judged by its lowest base and highest top.
pub(crate) fn railing_plan_parts(doc: &Document, e: &Element, cut: f64) -> Result<PlanParts, GeometryError> {
    let r = railing_of(e)?;
    let with_z = path(r, storey_floor(doc, e)?);
    let base = with_z.iter().map(|(_, z)| *z).fold(f64::INFINITY, f64::min);
    let top = with_z.iter().map(|(_, z)| *z).fold(f64::NEG_INFINITY, f64::max) + r.height.as_m();
    let pts: Vec<Point2> = with_z.iter().map(|(p, _)| *p).collect();
    let mut parts = PlanParts::default();
    let half = match r.infill {
        RailingInfill::Solid => PANEL_THICKNESS,
        _ => RAIL_WIDTH,
    } / 2.0;
    // Railings are drawn as lines; only a solid panel cut by the plane is filled like a low wall.
    if base < cut && cut < top && r.infill == RailingInfill::Solid {
        if let Ok(band) = stroke_polyline(&pts, half, JoinStyle::Miter { limit: 4.0 }, CapStyle::Butt) {
            parts.outline = band.clone();
            parts.cut.extend(
                band.into_iter()
                    .map(|region| CutArea { region, material: Some(r.infill_material_or_frame()) }),
            );
        }
        return Ok(parts);
    }
    let mut lines = Vec::new();
    for d in [half, -half] {
        if let Ok(side) = offset_polyline(&pts, d, JoinStyle::Miter { limit: 4.0 }) {
            lines.extend(
                side.windows(2)
                    .filter_map(|w| if let [a, b] = w { Some(Segment2::new(*a, *b)) } else { None }),
            );
        }
    }
    if let (Ok(l), Ok(rr)) = (
        offset_polyline(&pts, half, JoinStyle::Miter { limit: 4.0 }),
        offset_polyline(&pts, -half, JoinStyle::Miter { limit: 4.0 }),
    ) {
        if let (Some(a), Some(b)) = (l.first(), rr.first()) {
            lines.push(Segment2::new(*a, *b));
        }
        if let (Some(a), Some(b)) = (l.last(), rr.last()) {
            lines.push(Segment2::new(*a, *b));
        }
    }
    if base < cut {
        parts.below = lines;
    } else {
        parts.above = lines;
    }
    Ok(parts)
}
