//! Floor plans.
//!
//! A plan is the storey cut horizontally at its cut plane and seen from above (see `specs/views/plan-drawing.md`).
//! Painting order, back to front: underlays, space tints, what lies below the cut, what lies above it (dashed),
//! cut areas (opaque, so they hide lines under walls), material boundaries and the heavy cut outline, door and
//! window symbols, space stamps, annotation, view markers and the north arrow.

mod space;
mod stair;

use std::collections::BTreeSet;

use kanavos_drawing::{Affine, Drawing, Item, LinePattern, Pt, Shape, Stroke};
use kanavos_elements::{OpeningPlacement, opening_placement, plan_parts};
use kanavos_geom::{Point2, Polygon2, PolygonWithHoles, Segment2, Vec2};
use kanavos_model::{Document, Element, ElementKind, Id, ViewKind};
use kanavos_objects::{OpeningContext, object_symbol, opening_symbol};
use kanavos_units::Locale;

use crate::ViewError;
use crate::annot::{self, Ctx};
use crate::cut::{CutPiece, draw_cut};
use crate::meshops::Eye;
use crate::poly;
use crate::project::{ProjItem, Projection, merge_collinear, project, visible};
use crate::scene::element_scene_items;
use crate::style::{UNDERLAY, phase_stroke};

/// Options for drawing a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanOptions {
    /// Scale denominator (sizes of text and symbols follow it).
    pub scale: f64,
    /// Language of generated text.
    pub locale: Locale,
    /// Hidden layer prefixes.
    pub hidden_layers: BTreeSet<String>,
    /// Fill spaces with a light tint by use category.
    pub space_fill: bool,
    /// Add the perimeter to space stamps.
    pub space_perimeter: bool,
    /// Add the clear height to space stamps.
    pub space_height: bool,
    /// Draw stair treads above the cut plane, dashed.
    pub stair_above_cut: bool,
    /// Draw section and elevation markers.
    pub view_markers: bool,
    /// Draw a north arrow.
    pub north_arrow: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            scale: 50.0,
            locale: Locale::El,
            hidden_layers: BTreeSet::new(),
            space_fill: true,
            space_perimeter: false,
            space_height: false,
            stair_above_cut: true,
            view_markers: true,
            north_arrow: true,
        }
    }
}

pub(crate) fn p(q: Point2) -> Pt {
    annot::p(q)
}

fn ring(poly: &Polygon2) -> Vec<Pt> {
    poly.vertices.iter().map(|q| p(*q)).collect()
}

pub(crate) fn rings(poly: &PolygonWithHoles) -> Vec<Vec<Pt>> {
    poly.rings().map(ring).collect()
}

fn opening_affine(pl: &OpeningPlacement) -> Affine {
    let [cx, cy] = pl.centre.xy();
    Affine { a: pl.along.x, b: pl.across.x, c: pl.along.y, d: pl.across.y, tx: cx, ty: cy }
}

fn line(d: &mut Drawing, s: &Segment2, stroke: &Stroke, layer: &str, source: Id) {
    d.push(Item::line(p(s.start), p(s.end), stroke.clone(), layer).with_source(source.0));
}

/// Draws the plan of a storey.
pub fn plan(doc: &Document, storey: Id, opts: &PlanOptions) -> Result<Drawing, ViewError> {
    let st = doc.storey(storey).ok_or(ViewError::NotFound(storey))?;
    let cut = st.cut_elevation();
    let ctx = Ctx::new(opts.scale, opts.locale);
    let pens = ctx.pens;
    let mut d = Drawing::model(opts.scale);
    let on: Vec<&Element> = doc.elements_on(storey).collect();

    // Underlays first, greyed, so everything draws over them.
    for e in &on {
        if let ElementKind::Underlay(u) = &e.kind {
            for item in &u.drawing.items {
                let mut item = item.clone();
                item.layer = format!("underlay.{}", item.layer);
                item.source = Some(e.id.0);
                if let Some(s) = &mut item.stroke {
                    s.color = UNDERLAY;
                }
                if let Shape::Text(t) = &mut item.shape {
                    t.color = UNDERLAY;
                }
                d.push(item);
            }
        }
    }

    // Space tints.
    if opts.space_fill {
        for e in &on {
            if let ElementKind::Space(s) = &e.kind {
                space::tint(&mut d, e, s);
            }
        }
    }

    // Below and above the cut plane.
    let below = Stroke::solid(pens.below);
    let above = Stroke::patterned(pens.above, LinePattern::Dashed);
    let mut pieces: Vec<CutPiece> = Vec::new();
    for e in &on {
        if !e.category().is_building_part() || matches!(e.kind, ElementKind::Stair(_) | ElementKind::Roof(_))
        {
            continue;
        }
        let Ok(parts) = plan_parts(doc, e.id, cut) else { continue };
        let key = e.category().key();
        let (b, a) = (phase_stroke(below.clone(), e.phase), phase_stroke(above.clone(), e.phase));
        for s in &parts.below {
            line(&mut d, s, &b, &format!("{key}.below"), e.id);
        }
        for s in &parts.above {
            line(&mut d, s, &a, &format!("{key}.above"), e.id);
        }
        for c in &parts.cut {
            pieces.push(CutPiece {
                element: e.id,
                category: e.category(),
                phase: e.phase,
                material: c.material,
                shape: poly::shape_of(&c.region),
            });
        }
    }

    // Free objects (furniture, fixtures) below the cut.
    for e in &on {
        if let ElementKind::Object(o) = &e.kind {
            let [x, y] = o.position.xy();
            let mut m = Affine::rotation(o.rotation.as_rad());
            if o.mirrored {
                m = m.then(&Affine { a: -1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 });
            }
            m.tx = x;
            m.ty = y;
            let mut local = Drawing::model(opts.scale);
            local.items = object_symbol(o);
            for mut item in local.transformed(&m).items {
                item.source = Some(e.id.0);
                if let Some(s) = item.stroke.take() {
                    item.stroke = Some(phase_stroke(s, e.phase));
                }
                d.push(item);
            }
        }
    }

    // Stairs: this storey's stairs going up, and stairs from the storey below arriving here. Their labels are
    // painted after the cut areas.
    let mut stair_labels = Drawing::model(opts.scale);
    for e in &on {
        if let ElementKind::Stair(s) = &e.kind {
            stair::stair_up(&mut d, &mut stair_labels, &ctx, doc, e, s, cut.as_m(), opts.stair_above_cut);
        }
    }
    if let Some(lower) = doc.storey_below(storey) {
        for e in doc.elements_on(lower.id) {
            if let ElementKind::Stair(s) = &e.kind {
                let top = lower.elevation.as_m() + s.base_offset.as_m() + s.rise.as_m();
                if top >= st.elevation.as_m() - 0.05 {
                    stair::stair_down(
                        &mut d,
                        &mut stair_labels,
                        &ctx,
                        doc,
                        e,
                        s,
                        lower.cut_elevation().as_m(),
                    );
                }
            }
        }
    }

    // Roofs on this storey, and slabs and roofs of the storey above that show from below.
    let mut roofs: Vec<&Element> =
        on.iter().copied().filter(|e| matches!(e.kind, ElementKind::Roof(_))).collect();
    if let Some(up) = doc.storey_above(storey) {
        let up_has_walls = doc.walls_on(up.id).next().is_some();
        for e in doc.elements_on(up.id) {
            match &e.kind {
                ElementKind::Slab(_) => {
                    let Ok(parts) = plan_parts(doc, e.id, cut) else { continue };
                    let a = phase_stroke(above.clone(), e.phase);
                    for s in parts.above.iter().chain(&parts.below) {
                        line(&mut d, s, &a, "slab.above", e.id);
                    }
                }
                ElementKind::Roof(r) if !up_has_walls || r.base.as_m() < 0.5 => roofs.push(e),
                _ => {}
            }
        }
    }
    roof_lines(&mut d, &ctx, doc, &roofs, cut.as_m());

    // Cut areas.
    draw_cut(&mut d, doc, &pieces, &pens, opts.scale);

    // Openings.
    for e in &on {
        if let ElementKind::Opening(o) = &e.kind
            && let Ok(pl) = opening_placement(doc, e.id)
        {
            let sill = pl.sill_elevation;
            let head = sill + pl.height.as_m();
            if head <= cut.as_m() {
                continue; // Wholly below the cut: the wall above it is cut solid.
            }
            let overhead = sill > cut.as_m();
            let ctx_o = OpeningContext { opening: o, left_face: pl.left_face, right_face: pl.right_face };
            let m = opening_affine(&pl);
            let mut local = Drawing::model(opts.scale);
            local.items = opening_symbol(&ctx_o);
            for mut item in local.transformed(&m).items {
                item.source = Some(e.id.0);
                if overhead {
                    item.layer = format!("{}.above", e.category().key());
                    if let Some(s) = &mut item.stroke {
                        s.pattern = LinePattern::Dashed;
                        s.weight_mm = pens.above;
                    }
                }
                if let Some(s) = item.stroke.take() {
                    item.stroke = Some(phase_stroke(s, e.phase));
                }
                d.push(item);
            }
        }
    }

    d.extend(stair_labels);

    // Space stamps.
    for e in &on {
        if let ElementKind::Space(s) = &e.kind {
            space::stamp(&mut d, &ctx, st, e, s, opts);
        }
    }

    // Annotation and drafting on this storey, and grid lines everywhere.
    for e in doc.elements() {
        let on_storey = e.storey == Some(storey) && e.view.is_none();
        let src = Some(e.id.0);
        match &e.kind {
            ElementKind::Grid(g) if e.storey.is_none() || on_storey => annot::grid(&mut d, &ctx, g, src),
            _ if !on_storey => {}
            ElementKind::Dimension(dim) => annot::dimension(&mut d, &ctx, dim, src),
            ElementKind::Text(t) => annot::text_note(&mut d, &ctx, t, src),
            ElementKind::Level(l) => {
                annot::level_plan(&mut d, &ctx, l.position, l.value.unwrap_or(st.elevation), &l.label, src)
            }
            ElementKind::Drafting(df) => annot::drafting(&mut d, df, src),
            _ => {}
        }
    }

    // Section and elevation markers.
    if opts.view_markers {
        for v in doc.views() {
            if let ViewKind::Section { start, end, marker, flip, .. }
            | ViewKind::Elevation { start, end, marker, flip, .. } = &v.kind
            {
                let Some(u) = (*end - *start).normalized() else { continue };
                let look = if *flip { -u.perp() } else { u.perp() };
                let is_section = matches!(v.kind, ViewKind::Section { .. });
                annot::view_marker(&mut d, &ctx, *start, *end, marker, look, is_section, v.id.0);
            }
        }
    }

    // North arrow at the top right of the drawing.
    if opts.north_arrow
        && let Some(b) = d.bounds()
    {
        let rot = doc.site().georef.rotation;
        let north = Vec2::Y.rotated(rot);
        let at = Point2::new(b.max.x + ctx.mm(14.0), b.max.y - ctx.mm(10.0));
        annot::north_arrow(&mut d, &ctx, at, north, &kanavos_i18n::t(opts.locale, "views.north"));
    }

    crate::filter_layers(&mut d, &opts.hidden_layers);
    Ok(d)
}

/// Roof edges seen from above (eaves, ridges, hips, valleys, parapets): solid where the roof is below the cut,
/// dashed above it.
fn roof_lines(d: &mut Drawing, ctx: &Ctx, doc: &Document, roofs: &[&Element], cut: f64) {
    if roofs.is_empty() {
        return;
    }
    let items: Vec<Vec<crate::SceneItem>> = roofs.iter().map(|e| element_scene_items(doc, e.id)).collect();
    let proj_items: Vec<ProjItem<'_>> = roofs
        .iter()
        .zip(&items)
        .map(|(e, its)| ProjItem {
            element: e.id,
            category: e.category(),
            phase: e.phase,
            meshes: its.iter().map(|i| &i.mesh).collect(),
        })
        .collect();
    // Looking down: x = X, y = Y, depth = −Z.
    let proj = Projection {
        eye: Eye::Parallel([0.0, 0.0, -1.0]),
        rows: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, -1.0, 0.0]],
        clips: Vec::new(),
        perspective: None,
    };
    let projected = project(&proj_items, &proj, 15.0);
    let (vis, _) = visible(&projected, &[], &[], 1e-6);
    for s in merge_collinear(vis) {
        let Some(it) = proj_items.get(s.tag as usize) else { continue };
        let z = -s.depth;
        let (stroke, layer) = if z > cut {
            (Stroke::patterned(ctx.pens.above, LinePattern::Dashed), "roof.above")
        } else {
            (Stroke::solid(ctx.pens.below), "roof.below")
        };
        let stroke = phase_stroke(stroke, it.phase);
        d.push(
            Item::line(
                kanavos_drawing::pt(s.a[0], s.a[1]),
                kanavos_drawing::pt(s.b[0], s.b[1]),
                stroke,
                layer,
            )
            .with_source(it.element.0),
        );
    }
}
