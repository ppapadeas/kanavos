//! Wall layout (faces and layer bands after junction clean-up), opening placement, plan parts and 3D solids
//! of walls.

use kanavos_geom::{Point2, Polygon2, PolygonWithHoles, Segment2, Vec2, union_all};
use kanavos_model::{Document, Element, ElementKind, Id, OpeningKind};
use kanavos_solid::{ColumnPiece, ZPlane, column_solid};
use kanavos_units::Length;

use crate::junction::{JUNCTION_TOLERANCE, WallInfo, WallJoin, WallRules, resolve};
use crate::region::{Clip, CutShape, Region, apply_clip, apply_cut, apply_either};
use crate::roof::RoofCap;
use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart, dir, element};

/// One layer of a wall in plan. A layer interrupted by a crossing wall or by another wall's layer running
/// through it has several bands with the same `layer` index.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerBand {
    /// Plan region of the layer (counter-clockwise).
    pub region: Polygon2,
    /// Material.
    pub material: Id,
    /// Whether the layer is part of the structural core.
    pub core: bool,
    /// Index of the layer in the wall's build-up (left face first).
    pub layer: usize,
}

/// Plan geometry of a wall.
#[derive(Debug, Clone, PartialEq)]
pub struct WallLayout {
    /// Footprint of the whole thickness after junction clean-up: ends mitred, extended or trimmed to the faces of
    /// connected walls. Crossing walls do not split it.
    pub outline: Polygon2,
    /// Layer bands, left face to right face, after junction clean-up (layers of higher-priority materials of
    /// abutting walls run through lower-priority layers of this wall, which are cut there).
    pub layers: Vec<LayerBand>,
    /// Total thickness.
    pub thickness: Length,
    /// Left face polyline (start → end), as far as the outline reaches.
    pub left_face: Vec<Point2>,
    /// Right face polyline (start → end).
    pub right_face: Vec<Point2>,
    /// Absolute elevation of the wall base.
    pub base: f64,
    /// Absolute elevation of the wall top (before any clipping by a roof).
    pub top: f64,
    /// Junctions with other walls.
    pub joins: Vec<WallJoin>,
}

fn clip_value(clip: &Clip, p: Point2) -> f64 {
    match clip {
        Clip::Half(hp) => hp.signed_distance(p),
        Clip::Disc { centre, radius, inside } => {
            let r = (p - *centre).length();
            if *inside { radius - r } else { r - radius }
        }
    }
}

/// The part of a polyline inside a clip (the longest stretch if it leaves and re-enters).
fn clip_polyline(points: &[Point2], clip: &Clip) -> Vec<Point2> {
    let mut parts: Vec<Vec<Point2>> = Vec::new();
    let mut cur: Vec<Point2> = Vec::new();
    for w in points.windows(2) {
        let [a, b] = w else { continue };
        let (fa, fb) = (clip_value(clip, *a), clip_value(clip, *b));
        if fa >= 0.0 && cur.is_empty() {
            cur.push(*a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            // Crossing: refine by bisection for circles, linear for lines.
            let mut lo = 0.0;
            let mut hi = 1.0;
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                if (clip_value(clip, a.lerp(*b, mid)) >= 0.0) == (fa >= 0.0) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let x = a.lerp(*b, (lo + hi) / 2.0);
            if fa >= 0.0 {
                cur.push(x);
                parts.push(std::mem::take(&mut cur));
            } else {
                cur.push(x);
            }
        }
        if fb >= 0.0 {
            cur.push(*b);
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    let len = |p: &Vec<Point2>| -> f64 {
        p.windows(2).map(|w| if let [a, b] = w { a.distance(*b).as_m() } else { 0.0 }).sum()
    };
    let mut best = parts.into_iter().max_by(|a, b| len(a).total_cmp(&len(b))).unwrap_or_default();
    best.dedup_by(|a, b| a.distance(*b).as_m() < 1e-12);
    best
}

/// Computes the layout of a wall from its junction rules.
pub(crate) fn layout_from(w: &WallInfo, rules: &WallRules) -> WallLayout {
    let u0 = -rules.start.extension;
    let u1 = w.len + rules.end.extension;
    let end_clips: Vec<&Clip> =
        [&rules.start, &rules.end].iter().flat_map(|e| e.all.iter().chain(e.outline.iter())).collect();
    let mut outline: Region = vec![w.band(w.left, w.right, u0, u1)];
    for c in &end_clips {
        outline = apply_clip(outline, c);
    }
    let outline = outline
        .into_iter()
        .max_by(|a, b| a.area().as_m2().total_cmp(&b.area().as_m2()))
        .unwrap_or_else(|| w.band(w.left, w.right, 0.0, w.len));
    let mut layers = Vec::with_capacity(w.layers.len());
    for (k, l) in w.layers.iter().enumerate() {
        let mut r: Region = vec![w.band(l.d_left, l.d_right, u0, u1)];
        for end in [&rules.start, &rules.end] {
            for c in &end.all {
                r = apply_clip(r, c);
            }
            if let Some(Some(c)) = end.per_layer.get(k) {
                r = apply_clip(r, c);
            }
        }
        for (lk, sides) in &rules.splits {
            if *lk == k {
                r = apply_either(r, sides);
            }
        }
        for (lk, cut) in &rules.cuts {
            if *lk == k {
                r = apply_cut(r, cut);
            }
        }
        layers.extend(r.into_iter().map(|region| LayerBand {
            region: region.to_ccw(),
            material: l.material,
            core: l.core,
            layer: k,
        }));
    }
    let face = |d: f64| {
        let mut pts = w.offset_points(d, u0, u1);
        for c in &end_clips {
            pts = clip_polyline(&pts, c);
        }
        pts
    };
    WallLayout {
        outline,
        layers,
        thickness: Length::m(w.thickness),
        left_face: face(w.left),
        right_face: face(w.right),
        base: w.base,
        top: w.top,
        joins: rules.joins.clone(),
    }
}

/// The walls of the same storey that can meet `me`, with `me` first.
pub(crate) fn context(doc: &Document, e: &Element, me: WallInfo) -> Vec<WallInfo> {
    let mut out = vec![me];
    let Some(storey) = e.storey else { return out };
    let grow = kanavos_units::Length::m(JUNCTION_TOLERANCE * 2.0);
    let Some(mine) = out.first().map(|w| w.bbox.expanded(grow)) else { return out };
    for other in doc.elements_on(storey) {
        if other.id == e.id {
            continue;
        }
        let ElementKind::Wall(w) = &other.kind else { continue };
        // Cheap test on the reference line grown by the thickness before building the full description.
        let t = doc.structure_thickness(&w.structure).map(|t| t.as_m()).unwrap_or(0.0);
        let rough = match w.arc() {
            Some(a) => kanavos_geom::Bbox2::from_points(a.tessellate(0.05, 8)),
            None => kanavos_geom::Bbox2::from_points([w.start, w.end]),
        };
        let Some(rough) = rough else { continue };
        if !rough.expanded(kanavos_units::Length::m(t + 0.01)).intersects(&mine) {
            continue;
        }
        if let Ok(info) = WallInfo::new(doc, other) {
            out.push(info);
        }
    }
    out
}

/// Computes the plan layout of a wall, with its junctions to the other walls of its storey.
pub fn wall_layout(doc: &Document, id: Id) -> Result<WallLayout, GeometryError> {
    let e = element(doc, id)?;
    let me = WallInfo::new(doc, e)?;
    let ctx = context(doc, e, me);
    let (mut rules, _) = resolve(&ctx);
    match (ctx.first(), rules.first_mut()) {
        (Some(w), Some(r)) => {
            r.cuts.extend(column_cuts(doc, w));
            r.canonicalize();
            Ok(layout_from(w, r))
        }
        _ => Err(GeometryError::NotFound(id)),
    }
}

/// Columns on the wall's storey that stand in it: layers of lower junction priority than the column's material
/// are cut by the column's profile.
pub(crate) fn column_cuts(doc: &Document, w: &WallInfo) -> Vec<(usize, CutShape)> {
    let Some(storey) = doc.element(w.id).and_then(|e| e.storey) else { return Vec::new() };
    let mut out = Vec::new();
    for e in doc.elements_on(storey) {
        let ElementKind::Column(c) = &e.kind else { continue };
        let profile = crate::build::column_profile(c);
        let Some(b) = profile.bbox() else { continue };
        if !b.intersects(&w.bbox) {
            continue;
        }
        let Ok(base) = crate::base_elevation(doc, e) else { continue };
        let (z0, z1) = (base.as_m(), base.as_m() + c.height.as_m());
        if z1 <= w.base + 1e-6 || z0 >= w.top - 1e-6 {
            continue;
        }
        let priority = doc.material(c.material).map(|m| m.priority).unwrap_or(0);
        for (k, l) in w.layers.iter().enumerate() {
            if l.priority < priority {
                out.push((k, CutShape::Region(profile.clone())));
            }
        }
    }
    out
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

/// An opening in a wall, along the wall and in absolute heights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpeningSpan {
    pub id: Id,
    pub kind: OpeningKind,
    /// Start and end along the reference line.
    pub u0: f64,
    pub u1: f64,
    /// Absolute sill and head.
    pub sill: f64,
    pub head: f64,
}

/// The openings of a wall.
pub(crate) fn openings_of(doc: &Document, w: &WallInfo) -> Vec<OpeningSpan> {
    let storey = doc
        .element(w.id)
        .and_then(|e| e.storey)
        .and_then(|s| doc.storey(s))
        .map(|s| s.elevation.as_m())
        .unwrap_or(0.0);
    let mut v: Vec<OpeningSpan> = doc
        .openings_in(w.id)
        .map(|(id, o)| {
            let c = o.offset.as_m();
            let h = o.width.as_m() / 2.0;
            let sill = storey + o.sill.as_m();
            OpeningSpan {
                id,
                kind: o.kind,
                u0: (c - h).max(0.0),
                u1: (c + h).min(w.len),
                sill,
                head: sill + o.height.as_m(),
            }
        })
        .filter(|s| s.u1 - s.u0 > 1e-6 && s.head - s.sill > 1e-6)
        .collect();
    v.sort_by(|a, b| a.u0.total_cmp(&b.u0));
    v
}

/// The region of the wall between `u0` and `u1` along it, as a cut shape (the strip between the two square
/// cuts).
fn span_cut(w: &WallInfo, u0: f64, u1: f64) -> Option<CutShape> {
    let (Some(Clip::Half(a)), Some(Clip::Half(b))) = (w.across_clip(u0, true), w.across_clip(u1, false))
    else {
        return None;
    };
    Some(CutShape::Strip(a, b))
}

fn ring_segments(p: &Polygon2) -> Vec<Segment2> {
    p.edges().collect()
}

fn polyline_segments(pts: &[Point2]) -> Vec<Segment2> {
    pts.windows(2).filter_map(|w| if let [a, b] = w { Some(Segment2::new(*a, *b)) } else { None }).collect()
}

/// How a wall appears in a plan cut at absolute elevation `cut`.
pub(crate) fn wall_plan_parts(doc: &Document, w: &WallInfo, layout: &WallLayout, cut: f64) -> PlanParts {
    let mut parts = PlanParts::default();
    let caps = RoofCap::for_wall(doc, w);
    let top = w.top;
    if cut <= w.base {
        parts.above.extend(ring_segments(&layout.outline));
        return parts;
    }
    if cut >= top {
        parts.below.extend(ring_segments(&layout.outline));
        return parts;
    }
    let openings = openings_of(doc, w);
    // Gaps where openings span the cut plane.
    let gaps: Vec<CutShape> = openings
        .iter()
        .filter(|o| o.sill < cut && cut < o.head)
        .filter_map(|o| span_cut(w, o.u0, o.u1))
        .collect();
    // Parts of the wall whose top is pulled below the cut by a roof are seen from above, not cut.
    let low: Vec<Polygon2> = caps.iter().flat_map(|c| c.below(cut, w.base)).collect();
    for band in &layout.layers {
        let mut r: Region = vec![band.region.clone()];
        for g in &gaps {
            r = apply_cut(r, g);
        }
        for l in &low {
            r = apply_cut(r, &CutShape::Region(l.clone()));
        }
        parts.cut.extend(r.into_iter().map(|p| CutArea { region: p.into(), material: Some(band.material) }));
    }
    if !low.is_empty() {
        let fp: Vec<PolygonWithHoles> = vec![layout.outline.clone().into()];
        let lows: Vec<PolygonWithHoles> = low.iter().cloned().map(Into::into).collect();
        if let Ok(seen) = kanavos_geom::intersection(&fp, &lows) {
            for p in seen {
                for ring in p.rings() {
                    parts.below.extend(ring_segments(ring));
                }
            }
        }
    }
    let polys: Vec<PolygonWithHoles> = parts.cut.iter().map(|c| c.region.clone()).collect();
    parts.outline = union_all(&polys).unwrap_or(polys);
    // Seen through openings: the wall below the sill (window boards and sills), and openings above the cut.
    for o in &openings {
        if o.sill < cut && cut < o.head && o.sill > w.base + 1e-6 {
            parts.below.extend(polyline_segments(&w.offset_points(w.left, o.u0, o.u1)));
            parts.below.extend(polyline_segments(&w.offset_points(w.right, o.u0, o.u1)));
        } else if o.sill >= cut && o.sill < top {
            parts.above.extend(ring_segments(&w.band(w.left, w.right, o.u0, o.u1)));
        }
    }
    parts
}

/// Splits pieces by the square cut at `u` (only pieces that straddle it).
fn split_at(w: &WallInfo, pieces: Region, u: f64) -> Region {
    let (Some(fwd), Some(back)) = (w.across_clip(u, true), w.across_clip(u, false)) else { return pieces };
    let mut out = Vec::with_capacity(pieces.len() + 2);
    for p in pieces {
        let us: Vec<f64> = p.vertices.iter().map(|q| w.project(*q).0).collect();
        let lo = us.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = us.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if lo < u - 1e-7 && hi > u + 1e-7 {
            out.extend(apply_clip(vec![p.clone()], &back));
            out.extend(apply_clip(vec![p], &fwd));
        } else {
            out.push(p);
        }
    }
    out
}

/// The 3D solids of a wall: one closed mesh per layer, with true holes for its openings, clipped to the
/// underside of roofs on its storey.
pub(crate) fn wall_solids(
    doc: &Document,
    w: &WallInfo,
    layout: &WallLayout,
) -> Result<Vec<ElementSolid>, GeometryError> {
    let openings = openings_of(doc, w);
    let caps = RoofCap::for_wall(doc, w);
    let mut out = Vec::new();
    for (k, l) in w.layers.iter().enumerate() {
        let mut pieces: Region =
            layout.layers.iter().filter(|b| b.layer == k).map(|b| b.region.clone()).collect();
        if pieces.is_empty() {
            continue;
        }
        for o in &openings {
            pieces = split_at(w, pieces, o.u0);
            pieces = split_at(w, pieces, o.u1);
        }
        // Roof clipping: split by the plan regions where a roof underside is lower than the wall top.
        let mut tops: Vec<(Polygon2, ZPlane)> =
            pieces.into_iter().map(|p| (p, ZPlane::constant(w.top))).collect();
        for cap in &caps {
            tops = cap.split(tops, w.base);
        }
        let mut column = Vec::with_capacity(tops.len());
        for (region, top) in tops {
            let Some(c) = region.centroid() else { continue };
            let (uc, _) = w.project(c);
            let mut intervals = vec![(ZPlane::constant(w.base), top)];
            for o in &openings {
                if uc > o.u0 && uc < o.u1 {
                    intervals = subtract_interval(&intervals, o.sill, o.head, c);
                }
            }
            intervals.retain(|(lo, hi)| region.vertices.iter().any(|p| hi.at(*p) - lo.at(*p) > 1e-6));
            if !intervals.is_empty() {
                column.push(ColumnPiece { region, intervals });
            }
        }
        if column.is_empty() {
            continue;
        }
        let mesh = column_solid(&column, 1e-6).map_err(|e| GeometryError::Invalid {
            id: Some(w.id),
            field: "geometry",
            reason: e.to_string(),
        })?;
        out.push(ElementSolid { mesh, material: Some(l.material), part: SolidPart::Layer(k) });
    }
    Ok(out)
}

fn subtract_interval(intervals: &[(ZPlane, ZPlane)], lo: f64, hi: f64, at: Point2) -> Vec<(ZPlane, ZPlane)> {
    let mut out = Vec::with_capacity(intervals.len() + 1);
    for &(a, b) in intervals {
        let (za, zb) = (a.at(at), b.at(at));
        if hi <= za + 1e-9 || lo >= zb - 1e-9 {
            out.push((a, b));
            continue;
        }
        if lo > za + 1e-9 {
            out.push((a, ZPlane::constant(lo)));
        }
        if hi < zb - 1e-9 {
            out.push((ZPlane::constant(hi), b));
        }
    }
    out
}

/// Net side areas of a wall's two faces (gross face area minus openings), as `(left, right, gross_left,
/// gross_right)` in square metres. Arc faces scale openings by their radius.
pub(crate) fn face_areas(doc: &Document, w: &WallInfo, layout: &WallLayout) -> (f64, f64, f64, f64) {
    let len = |pts: &[Point2]| -> f64 {
        pts.windows(2).map(|p| if let [a, b] = p { a.distance(*b).as_m() } else { 0.0 }).sum()
    };
    let h = w.top - w.base;
    let gl = len(&layout.left_face) * h;
    let gr = len(&layout.right_face) * h;
    let (mut ol, mut or) = (0.0, 0.0);
    for o in openings_of(doc, w) {
        let height = (o.head.min(w.top) - o.sill.max(w.base)).max(0.0);
        let width = o.u1 - o.u0;
        let (sl, sr) = match &w.arc {
            Some(a) => {
                let r = a.radius.as_m().max(1e-9);
                let s = if a.sweep.as_rad() > 0.0 { 1.0 } else { -1.0 };
                ((r - s * w.left) / r, (r - s * w.right) / r)
            }
            None => (1.0, 1.0),
        };
        ol += width * sl * height;
        or += width * sr * height;
    }
    ((gl - ol).max(0.0), (gr - or).max(0.0), gl, gr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyline_clipping_keeps_inside_part() {
        let pts = [Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)];
        let hp = kanavos_geom::HalfPlane::new(Point2::new(4.0, 0.0), -Vec2::X).unwrap();
        let r = clip_polyline(&pts, &Clip::Half(hp));
        assert_eq!(r.len(), 2);
        assert!((r[1].x.as_m() - 4.0).abs() < 1e-9);
        let disc = Clip::Disc { centre: Point2::new(10.0, 0.0), radius: 3.0, inside: false };
        let r = clip_polyline(&pts, &disc);
        assert!((r[1].x.as_m() - 7.0).abs() < 1e-9);
    }
}
