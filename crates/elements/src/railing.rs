//! Railings: posts, top rail and infill (bars, glass or solid panels) in 3D, and their plan representation.

use kanavos_geom::{
    CapStyle, JoinStyle, Point2, Point3, Polygon2, Segment2, Vec2, offset_polyline, stroke_polyline,
};
use kanavos_model::{Document, Element, ElementKind, Railing, RailingInfill};
use kanavos_solid::{Mesh, cuboid, rect_profile, sweep};

use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart, base_elevation};

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

/// Distinct path points.
fn path(r: &Railing) -> Vec<Point2> {
    let mut v: Vec<Point2> = Vec::with_capacity(r.path.len());
    for &p in &r.path {
        if v.last().is_none_or(|q| q.distance(p).as_m() > 1e-6) {
            v.push(p);
        }
    }
    v
}

/// Post positions: every path vertex, plus intermediate posts so that no span exceeds the post spacing.
pub(crate) fn post_positions(r: &Railing) -> Vec<Point2> {
    let pts = path(r);
    let spacing = r.post_spacing.as_m().max(0.1);
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let [a, b] = w else { continue };
        let len = a.distance(*b).as_m();
        let n = ((len / spacing).ceil() as usize).max(1);
        for i in 0..n {
            out.push(a.lerp(*b, i as f64 / n as f64));
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

/// The 3D solids of a railing: posts, rails and infill.
pub(crate) fn railing_solids(doc: &Document, e: &Element) -> Result<Vec<ElementSolid>, GeometryError> {
    let r = railing_of(e)?;
    let base = base_elevation(doc, e)?.as_m();
    let top = base + r.height.as_m();
    let pts = path(r);
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
    let rail_bottom = top - RAIL_HEIGHT;
    // Posts.
    let mut posts = Mesh::new();
    let h = POST_SIZE / 2.0;
    for p in post_positions(r) {
        let [x, y] = p.xy();
        posts.append(&cuboid(Point3::new(x - h, y - h, base), Point3::new(x + h, y + h, rail_bottom)));
    }
    // Top rail along the whole path.
    let path3: Vec<Point3> = pts.iter().map(|p| Point3::from_plan(*p, top)).collect();
    let mut rails = sweep(&path3, &rect_profile(RAIL_WIDTH, RAIL_HEIGHT, 0.0)).map_err(err)?;
    let mut infill = Mesh::new();
    match r.infill {
        RailingInfill::Bars => {
            let low: Vec<Point3> = pts.iter().map(|p| Point3::from_plan(*p, base + BOTTOM_RAIL)).collect();
            rails
                .append(&sweep(&low, &rect_profile(RAIL_WIDTH * 0.8, RAIL_HEIGHT * 0.75, 0.0)).map_err(err)?);
            let b = BAR_SIZE / 2.0;
            for w in pts.windows(2) {
                let [a, c] = w else { continue };
                let len = a.distance(*c).as_m();
                let step = BAR_SIZE + BAR_GAP;
                let n = ((len - POST_SIZE) / step).floor().max(0.0) as usize;
                let d = (*c - *a).normalized().unwrap_or(Vec2::X);
                // Centre the bars between the posts.
                let used = n.saturating_sub(1) as f64 * step;
                let first = (len - used) / 2.0;
                for i in 0..n {
                    let p = *a + d * (first + step * i as f64);
                    let [x, y] = p.xy();
                    infill.append(&cuboid(
                        Point3::new(x - b, y - b, base + BOTTOM_RAIL),
                        Point3::new(x + b, y + b, rail_bottom),
                    ));
                }
            }
        }
        RailingInfill::Glass => {
            for w in pts.windows(2) {
                let [a, c] = w else { continue };
                let len = a.distance(*c).as_m();
                if len <= POST_SIZE + 0.02 {
                    continue;
                }
                let d = (*c - *a).normalized().unwrap_or(Vec2::X);
                let inset = POST_SIZE / 2.0 + 0.01;
                if let Some(m) = box_along(
                    *a + d * inset,
                    *c - d * inset,
                    GLASS_THICKNESS,
                    base + 0.05,
                    rail_bottom - 0.03,
                ) {
                    infill.append(&m);
                }
            }
        }
        RailingInfill::Solid => {
            let low: Vec<Point3> = pts.iter().map(|p| Point3::from_plan(*p, rail_bottom)).collect();
            infill = sweep(&low, &rect_profile(PANEL_THICKNESS, rail_bottom - base, 0.0)).map_err(err)?;
        }
    }
    let mut out = vec![
        ElementSolid { mesh: posts, material: Some(r.material), part: SolidPart::Post },
        ElementSolid { mesh: rails, material: Some(r.material), part: SolidPart::Rail },
    ];
    if !infill.is_empty() {
        out.push(ElementSolid { mesh: infill, material: Some(r.material), part: SolidPart::Infill });
    }
    Ok(out)
}

/// How a railing appears in plan: its two edge lines (seen when it stands below the cut plane, dashed when it is
/// entirely above); a solid panel crossed by the cut plane is a cut band instead.
pub(crate) fn railing_plan_parts(doc: &Document, e: &Element, cut: f64) -> Result<PlanParts, GeometryError> {
    let r = railing_of(e)?;
    let base = base_elevation(doc, e)?.as_m();
    let top = base + r.height.as_m();
    let pts = path(r);
    let mut parts = PlanParts::default();
    let half = match r.infill {
        RailingInfill::Solid => PANEL_THICKNESS,
        _ => RAIL_WIDTH,
    } / 2.0;
    // Railings are drawn as lines; only a solid panel cut by the plane is filled like a low wall.
    if base < cut && cut < top && r.infill == RailingInfill::Solid {
        if let Ok(band) = stroke_polyline(&pts, half, JoinStyle::Miter { limit: 4.0 }, CapStyle::Butt) {
            parts.outline = band.clone();
            parts.cut.extend(band.into_iter().map(|region| CutArea { region, material: Some(r.material) }));
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
