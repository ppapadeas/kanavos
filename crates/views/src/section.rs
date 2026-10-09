//! Sections and elevations with exact hidden-line removal (see `specs/views/sections-elevations.md`).
//!
//! Section coordinates: `x` runs along the view line from the end the viewer sees on the left, `y` is the
//! absolute elevation. A section cuts every solid that crosses its plane (closed cut polygons, filled with the
//! material hatch, one heavy outline) and draws what lies beyond, up to the view depth, with hidden lines
//! removed. An elevation draws the same without cutting.

use std::collections::{BTreeMap, BTreeSet};

use kanavos_drawing::{Drawing, Fill, HatchPattern, Item, Rgba, Stroke, pt};
use kanavos_geom::{Point2, Point3, Vec2};
use kanavos_i18n::t;
use kanavos_model::{Document, ElementKind, Id, Phase, ViewKind};
use kanavos_units::{Length, Locale};

use crate::annot::{self, Ctx};
use crate::cut::{CutPiece, draw_cut};
use crate::hlr::{Edge, Visible};
use crate::meshops::{Eye, V3, slice_loops};
use crate::poly::{self, Shape};
use crate::project::{ProjItem, Projection, is_profile, merge_collinear, project, visible};
use crate::style::phase_stroke;
use crate::{Scene, SceneItem, SceneOptions, ViewError, scene};

/// The frame of a section or elevation view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionFrame {
    /// Point of the view line at drawing `x = 0`: the line's start, or its end for flipped views, so that the
    /// drawing is never mirrored.
    pub origin: Point2,
    /// Unit vector along the line (drawing +X).
    pub along: Vec2,
    /// Unit vector pointing into the view (depth); always `along.perp()`.
    pub look: Vec2,
    /// How far the view sees, metres.
    pub depth: f64,
    /// Whether the plane cuts (section) or only projects (elevation).
    pub cuts: bool,
    /// Length of the line, metres.
    pub length: f64,
}

impl SectionFrame {
    /// Section-plane coordinates of a world point: (x along, y elevation, depth into the view).
    pub fn to_view(&self, q: Point3) -> (f64, f64, f64) {
        let d = Point2::new(q.x, q.y) - self.origin;
        (d.dot(self.along), q.z, d.dot(self.look))
    }

    /// World plan point of a drawing `x`.
    pub fn plan_point(&self, x: f64) -> Point2 {
        self.origin + self.along * x
    }

    fn projection(&self) -> Projection {
        let [ox, oy] = self.origin.xy();
        let (a, l) = (self.along, self.look);
        Projection {
            eye: Eye::Parallel([l.x, l.y, 0.0]),
            rows: [
                [a.x, a.y, 0.0, -(a.x * ox + a.y * oy)],
                [0.0, 0.0, 1.0, 0.0],
                [l.x, l.y, 0.0, -(l.x * ox + l.y * oy)],
            ],
            clips: vec![
                [1.0, 0.0, 0.0, 0.0],
                [-1.0, 0.0, 0.0, self.length],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, -1.0, self.depth],
            ],
            perspective: None,
        }
    }
}

/// The frame of a section or elevation view.
pub fn section_frame(doc: &Document, view: Id) -> Result<SectionFrame, ViewError> {
    let v = doc.view(view).ok_or(ViewError::NotFound(view))?;
    let (start, end, depth, flip, cuts) = match &v.kind {
        ViewKind::Section { start, end, depth, flip, .. } => (*start, *end, *depth, *flip, true),
        ViewKind::Elevation { start, end, depth, flip, .. } => (*start, *end, *depth, *flip, false),
        _ => return Err(ViewError::WrongKind(view)),
    };
    let dir = (end - start).normalized().ok_or(ViewError::WrongKind(view))?;
    let (origin, along) = if flip { (end, -dir) } else { (start, dir) };
    let depth = if depth > Length::ZERO { depth.as_m() } else { 1e6 };
    Ok(SectionFrame { origin, along, look: along.perp(), depth, cuts, length: start.distance(end).as_m() })
}

/// Options for sections and elevations.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionOptions {
    /// Scale denominator; `None` uses the view's scale.
    pub scale: Option<f64>,
    /// Language of generated text.
    pub locale: Locale,
    /// Hidden layer prefixes (in addition to the view's own).
    pub hidden_layers: BTreeSet<String>,
    /// Edges between faces meeting at more than this angle (degrees) are drawn.
    pub crease_angle_deg: f64,
    /// Draw lines further from the plane thinner.
    pub depth_fade: bool,
    /// Draw the ground line (and earth hatch in sections).
    pub ground: bool,
    /// Add level marks for every storey floor and the top of the building.
    pub level_marks: bool,
    /// Draw the outer contour of an elevation heavier.
    pub profile_lines: bool,
}

impl Default for SectionOptions {
    fn default() -> Self {
        SectionOptions {
            scale: None,
            locale: Locale::El,
            hidden_layers: BTreeSet::new(),
            crease_angle_deg: 15.0,
            depth_fade: true,
            ground: true,
            level_marks: true,
            profile_lines: true,
        }
    }
}

/// Draws a section or elevation with the default options.
pub fn section(doc: &Document, view: Id, locale: Locale) -> Result<Drawing, ViewError> {
    section_with(doc, view, &SectionOptions { locale, ..SectionOptions::default() }, None)
}

fn view_positions(frame: &SectionFrame, m: &kanavos_solid::Mesh) -> Vec<V3> {
    m.positions
        .iter()
        .map(|q| {
            let (x, y, d) = frame.to_view(*q);
            [x, y, d]
        })
        .collect()
}

/// Draws a section or elevation, optionally reusing a prepared scene (for example from a [`crate::SceneCache`]).
pub fn section_with(
    doc: &Document,
    view: Id,
    opts: &SectionOptions,
    scene_in: Option<&Scene>,
) -> Result<Drawing, ViewError> {
    let v = doc.view(view).ok_or(ViewError::NotFound(view))?;
    let frame = section_frame(doc, view)?;
    let scale = opts.scale.unwrap_or(f64::from(v.scale));
    let ctx = Ctx::new(scale, opts.locale);
    let pens = ctx.pens;
    let owned;
    let sc = match scene_in {
        Some(s) => s,
        None => {
            owned = scene(doc, &SceneOptions::default());
            &owned
        }
    };
    let mut d = Drawing::model(scale);

    // Group the scene by element.
    let mut by_element: BTreeMap<Id, Vec<&SceneItem>> = BTreeMap::new();
    for it in &sc.items {
        by_element.entry(it.element).or_default().push(it);
    }
    let phase_of = |id: Id| doc.element(id).map(|e| e.phase).unwrap_or(Phase::New);
    let proj_items: Vec<ProjItem<'_>> = by_element
        .iter()
        .filter_map(|(id, its)| {
            let first = its.first()?;
            Some(ProjItem {
                element: *id,
                category: first.category,
                phase: phase_of(*id),
                meshes: its.iter().map(|i| &i.mesh).collect(),
            })
        })
        .collect();
    let proj = frame.projection();
    let projected = project(&proj_items, &proj, opts.crease_angle_deg);

    // Cut polygons and their caps.
    let mut pieces: Vec<CutPiece> = Vec::new();
    let mut caps: Vec<[V3; 3]> = Vec::new();
    if frame.cuts {
        let clip = poly::rect(0.0, -1e4, frame.length, 1e4);
        for it in &sc.items {
            let pos = view_positions(&frame, &it.mesh);
            let (mut dmin, mut dmax, mut xmin, mut xmax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for q in &pos {
                dmin = dmin.min(q[2]);
                dmax = dmax.max(q[2]);
                xmin = xmin.min(q[0]);
                xmax = xmax.max(q[0]);
            }
            if !(dmin < 0.0 && dmax >= 0.0) || xmax < 0.0 || xmin > frame.length {
                continue;
            }
            let (loops, _open) = slice_loops(&pos, &it.mesh.triangles);
            if loops.is_empty() {
                continue;
            }
            let shapes = poly::intersection(&poly::even_odd(&loops), std::slice::from_ref(&clip));
            for s in shapes {
                pieces.push(CutPiece {
                    element: it.element,
                    category: it.category,
                    phase: phase_of(it.element),
                    material: it.material,
                    shape: s,
                });
            }
        }
        for pc in &pieces {
            let Some(pg) = poly::polygon_of(&pc.shape) else { continue };
            let Ok((pts, tris)) = kanavos_solid::triangulate(&pg) else { continue };
            for t in tris {
                let q =
                    |i: u32| pts.get(i as usize).map(|p| [p.x.as_m(), p.y.as_m(), 0.0]).unwrap_or([0.0; 3]);
                caps.push([q(t[0]), q(t[1]), q(t[2])]);
            }
        }
    }

    // The outlines of the cut faces split the lines behind them.
    let cap_outline: Vec<Edge> = pieces
        .iter()
        .flat_map(|pc| pc.shape.iter())
        .flat_map(|c| {
            let n = c.len();
            (0..n).filter_map(move |i| {
                let (a, b) = (c.get(i)?, c.get((i + 1) % n)?);
                Some(Edge { a: [a[0], a[1], 0.0], b: [b[0], b[1], 0.0], tag: u32::MAX })
            })
        })
        .collect();
    let (vis, occ) = visible(&projected, &caps, &cap_outline, 1e-6);
    let vis = merge_collinear(vis);

    // Extents of the building in the drawing.
    let mut ext = Extents::default();
    for s in &vis {
        ext.add(s.a);
        ext.add(s.b);
    }
    for pc in &pieces {
        if let Some(b) = poly::shape_bbox(&pc.shape) {
            ext.add([b[0], b[1]]);
            ext.add([b[2], b[3]]);
        }
    }
    let ground = doc.site().ground_level.as_m();
    // Building extent along the ground, for the earth hatch and the ground line in sections.
    let cut_span = pieces
        .iter()
        .filter_map(|pc| poly::shape_bbox(&pc.shape))
        .filter(|b| b[1] <= ground + 0.5 && b[3] >= ground - 1.0)
        .fold(None, |acc: Option<(f64, f64)>, b| {
            Some(acc.map_or((b[0], b[2]), |(a0, a1)| (a0.min(b[0]), a1.max(b[2]))))
        });

    // Beyond lines.
    let front = vis.iter().map(|s| s.depth).fold(f64::INFINITY, f64::min);
    for s in &vis {
        let Some(it) = proj_items.get(s.tag as usize) else { continue };
        for piece in clip_ground(s, ground, if frame.cuts { cut_span } else { None }, opts.ground) {
            let rel = s.depth - front;
            let mut w = if !opts.depth_fade || rel < 1.5 {
                pens.beyond
            } else if rel < 6.0 {
                pens.far
            } else {
                pens.far.min(kanavos_drawing::weights::EXTRA_FINE)
            };
            if !frame.cuts && opts.profile_lines && is_profile(&occ, &piece, ctx.mm(0.4)) {
                w = pens.profile;
            }
            let stroke = phase_stroke(Stroke::solid(w), it.phase);
            d.push(
                Item::line(
                    pt(piece.a[0], piece.a[1]),
                    pt(piece.b[0], piece.b[1]),
                    stroke,
                    format!("{}.beyond", it.category.key()),
                )
                .with_source(it.element.0),
            );
        }
    }

    // Ground: earth hatch (sections) and the ground line.
    if opts.ground && ext.valid() {
        let margin = ctx.mm(12.0);
        let x0 = (ext.min[0] - margin).max(if frame.length > 0.0 { 0.0 } else { f64::MIN });
        let x1 = (ext.max[0] + margin).min(if frame.length > 0.0 { frame.length } else { f64::MAX });
        let spans: Vec<(f64, f64)> = match (frame.cuts, cut_span) {
            (true, Some((b0, b1))) => vec![(x0, b0), (b1, x1)],
            _ => vec![(x0, x1)],
        };
        for (a, b) in spans {
            if b - a < 1e-6 {
                continue;
            }
            if frame.cuts {
                let band: Shape = poly::rect(a, ground - ctx.mm(5.0), b, ground);
                d.push(Item::polygon(
                    crate::cut::rings_of(&band),
                    Some(Fill::Hatch {
                        pattern: HatchPattern::Earth,
                        color: Rgba::BLACK,
                        background: Some(Rgba::WHITE),
                        scale: 1.0,
                        angle: 0.0,
                    }),
                    None,
                    "ground.earth",
                ));
            }
            d.push(Item::line(pt(a, ground), pt(b, ground), Stroke::solid(pens.ground), "ground.line"));
        }
    }

    // Cut areas.
    draw_cut(&mut d, doc, &pieces, &pens, scale);

    // Level marks for storey floors, the top of the building and the ground.
    if opts.level_marks && ext.valid() {
        let x = ext.max[0] + ctx.mm(14.0);
        let mut levels: Vec<(f64, String)> =
            doc.storeys().iter().map(|s| (s.elevation.as_m(), String::new())).collect();
        let top = ext.max[1];
        if levels.iter().all(|(z, _)| (z - top).abs() > 0.05) {
            levels.push((top, String::new()));
        }
        if opts.ground && levels.iter().all(|(z, _)| (z - ground).abs() > 0.05) {
            levels.push((ground, t(opts.locale, "views.level.ground").into_owned()));
        }
        for (z, label) in levels {
            if z < ext.min[1] - 1.0 || z > ext.max[1] + 1.0 {
                continue;
            }
            d.push(Item::line(
                pt(ext.max[0] + ctx.mm(3.0), z),
                pt(x - ctx.mm(1.5), z),
                Stroke::solid(pens.annotation),
                "level.line",
            ));
            annot::level_section(&mut d, &ctx, x, z, Length::m(z), &label, false, None);
        }
    }

    // Annotation owned by the view.
    for e in doc.elements_in_view(view) {
        let src = Some(e.id.0);
        match &e.kind {
            ElementKind::Dimension(dim) => annot::dimension(&mut d, &ctx, dim, src),
            ElementKind::Text(tn) => annot::text_note(&mut d, &ctx, tn, src),
            ElementKind::Level(l) => {
                let [x, y] = l.position.xy();
                annot::level_section(
                    &mut d,
                    &ctx,
                    x,
                    y,
                    l.value.unwrap_or(Length::m(y)),
                    &l.label,
                    false,
                    src,
                );
            }
            ElementKind::Drafting(df) => annot::drafting(&mut d, df, src),
            _ => {}
        }
    }

    crate::filter_layers(&mut d, &v.hidden_layers);
    crate::filter_layers(&mut d, &opts.hidden_layers);
    Ok(d)
}

#[derive(Debug, Clone, Copy)]
struct Extents {
    min: [f64; 2],
    max: [f64; 2],
}

impl Default for Extents {
    fn default() -> Self {
        Extents { min: [f64::INFINITY; 2], max: [f64::NEG_INFINITY; 2] }
    }
}

impl Extents {
    fn add(&mut self, p: [f64; 2]) {
        self.min = [self.min[0].min(p[0]), self.min[1].min(p[1])];
        self.max = [self.max[0].max(p[0]), self.max[1].max(p[1])];
    }
    fn valid(&self) -> bool {
        self.min[0] <= self.max[0] && self.min[1] <= self.max[1]
    }
}

/// The parts of a visible piece that are not under the ground: everything above `ground`, plus, in sections, what
/// lies inside the building's cut extent (basements).
fn clip_ground(s: &Visible, ground: f64, inside: Option<(f64, f64)>, enabled: bool) -> Vec<Visible> {
    if !enabled || (s.a[1] >= ground - 1e-9 && s.b[1] >= ground - 1e-9) {
        return vec![*s];
    }
    let mut ts = vec![0.0, 1.0];
    let (dx, dy) = (s.b[0] - s.a[0], s.b[1] - s.a[1]);
    if dy.abs() > 1e-15 {
        ts.push((ground - s.a[1]) / dy);
    }
    if let Some((x0, x1)) = inside
        && dx.abs() > 1e-15
    {
        ts.push((x0 - s.a[0]) / dx);
        ts.push((x1 - s.a[0]) / dx);
    }
    ts.retain(|t| (0.0..=1.0).contains(t));
    ts.sort_by(f64::total_cmp);
    let at = |t: f64| [s.a[0] + dx * t, s.a[1] + dy * t];
    let mut out: Vec<Visible> = Vec::new();
    for w in ts.windows(2) {
        let [t0, t1] = w else { continue };
        if t1 - t0 < 1e-12 {
            continue;
        }
        let m = at(0.5 * (t0 + t1));
        let keep = m[1] >= ground - 1e-9 || inside.is_some_and(|(x0, x1)| m[0] > x0 && m[0] < x1);
        if !keep {
            continue;
        }
        match out.last_mut() {
            Some(last)
                if (last.b[0] - at(*t0)[0]).abs() < 1e-12 && (last.b[1] - at(*t0)[1]).abs() < 1e-12 =>
            {
                last.b = at(*t1);
            }
            _ => out.push(Visible { a: at(*t0), b: at(*t1), ..*s }),
        }
    }
    out
}
