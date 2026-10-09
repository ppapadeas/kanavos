//! Drawing of cut areas (poché), shared by plans and sections.
//!
//! Rules (see `specs/views/plan-drawing.md` §Cut areas):
//! 1. Where cut areas of different elements overlap, the material with the higher junction priority runs
//!    through and the other is trimmed; equal priorities keep the element created first. This is a drawing-level
//!    stand-in until `kanavos-elements` resolves junctions; it changes nothing once the inputs no longer overlap.
//! 2. Each area is filled with its material's hatch over an opaque background, so lines below are hidden.
//!    Finish layers thinner than [`FINISH_MM`] on paper are left white.
//! 3. Boundaries are drawn only between different materials: thin inside one element kind, a heavier minor
//!    outline where kinds meet (a column inside a wall). Same-material areas that touch merge without a line.
//! 4. The union of everything cut gets one continuous heavy outline per phase, split into pieces per element
//!    so each piece keeps its element's layer and selection source.

use kanavos_drawing::{Drawing, Fill, HatchPattern, Item, Pt, Rgba, Stroke, pt};
use kanavos_model::{Category, Document, Id, Phase};

use crate::poly::{self, Shape};
use crate::style::{Pens, phase_fill, phase_stroke};

/// Paper thickness (mm) below which a cut layer is drawn as a plain white finish without its own boundary.
pub(crate) const FINISH_MM: f64 = 0.5;

/// One area cut by a plan or section plane.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CutPiece {
    /// Element.
    pub element: Id,
    /// Element category.
    pub category: Category,
    /// Construction phase.
    pub phase: Phase,
    /// Material, for the hatch and the junction priority.
    pub material: Option<Id>,
    /// Region in drawing coordinates.
    pub shape: Shape,
}

/// Material fill for a cut area, opaque.
pub(crate) fn material_fill(doc: &Document, material: Option<Id>) -> Fill {
    match material.and_then(|m| doc.material(m)) {
        Some(m) => Fill::Hatch {
            pattern: m.cut_pattern,
            color: Rgba::BLACK,
            background: Some(m.cut_background.unwrap_or(Rgba::WHITE)),
            scale: 1.0,
            angle: 0.0,
        },
        None => Fill::hatch(HatchPattern::None, Some(Rgba::LIGHT_GREY)),
    }
}

pub(crate) fn rings_of(s: &Shape) -> Vec<Vec<Pt>> {
    s.iter().map(|c| c.iter().map(|p| pt(p[0], p[1])).collect()).collect()
}

struct Clean {
    piece: usize,
    shapes: Vec<Shape>,
    bbox: [f64; 4],
}

struct Group {
    category: Category,
    material: Option<Id>,
    phase: Phase,
    priority: u8,
    thin: bool,
    union: Vec<Shape>,
    bbox: [f64; 4],
}

fn bbox_of(shapes: &[Shape]) -> Option<[f64; 4]> {
    shapes
        .iter()
        .filter_map(poly::shape_bbox)
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
}

fn in_box(b: &[f64; 4], p: [f64; 2]) -> bool {
    p[0] >= b[0] && p[0] <= b[2] && p[1] >= b[1] && p[1] <= b[3]
}

/// Edges of every ring of the shapes with the outward normal of each edge.
fn ring_edges(shapes: &[Shape]) -> Vec<Vec<([f64; 2], [f64; 2])>> {
    let mut out = Vec::new();
    for s in shapes {
        for c in s {
            let n = c.len();
            let mut ring = Vec::with_capacity(n);
            for i in 0..n {
                if let (Some(a), Some(b)) = (c.get(i), c.get((i + 1) % n)) {
                    ring.push((*a, *b));
                }
            }
            out.push(ring);
        }
    }
    out
}

const PROBE: f64 = 2e-5;

fn probe(a: [f64; 2], b: [f64; 2], outside: bool) -> Option<[f64; 2]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-9 {
        return None;
    }
    // Rings are counter-clockwise outside and clockwise for holes, so the right-hand normal points out.
    let (nx, ny) = (dy / len, -dx / len);
    let k = if outside { PROBE } else { -PROBE };
    Some([(a[0] + b[0]) / 2.0 + nx * k, (a[1] + b[1]) / 2.0 + ny * k])
}

/// Runs of consecutive ring edges with the same label, as polylines; `closed` when one label covers the ring.
fn runs<L: PartialEq + Clone>(
    ring: &[([f64; 2], [f64; 2])],
    labels: &[Option<L>],
) -> Vec<(L, Vec<Pt>, bool)> {
    let n = ring.len();
    if n == 0 || labels.len() != n {
        return Vec::new();
    }
    let first = labels.first().cloned().flatten();
    if labels.iter().all(|l| *l == first.clone()) {
        return match first {
            Some(l) => vec![(l, ring.iter().map(|(a, _)| pt(a[0], a[1])).collect(), true)],
            None => Vec::new(),
        };
    }
    // Start where the label changes.
    let start = (0..n).find(|&i| labels.get(i) != labels.get((i + n - 1) % n)).unwrap_or(0);
    let mut out = Vec::new();
    let mut cur: Option<(L, Vec<Pt>)> = None;
    for k in 0..n {
        let i = (start + k) % n;
        let (Some((a, b)), Some(label)) = (ring.get(i), labels.get(i)) else { continue };
        match (label, &mut cur) {
            (Some(l), Some((cl, pts))) if l == cl => pts.push(pt(b[0], b[1])),
            (Some(l), _) => {
                if let Some((cl, pts)) = cur.take() {
                    out.push((cl, pts, false));
                }
                cur = Some((l.clone(), vec![pt(a[0], a[1]), pt(b[0], b[1])]));
            }
            (None, _) => {
                if let Some((cl, pts)) = cur.take() {
                    out.push((cl, pts, false));
                }
            }
        }
    }
    if let Some((cl, pts)) = cur {
        out.push((cl, pts, false));
    }
    out
}

/// Draws cut areas: fills, material boundaries and the heavy outline. `scale` is the drawing scale denominator.
pub(crate) fn draw_cut(d: &mut Drawing, doc: &Document, pieces: &[CutPiece], pens: &Pens, scale: f64) {
    if pieces.is_empty() {
        return;
    }
    let priority = |p: &CutPiece| p.material.and_then(|m| doc.material(m)).map(|m| m.priority).unwrap_or(0);
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by(|&a, &b| {
        let (Some(pa), Some(pb)) = (pieces.get(a), pieces.get(b)) else { return a.cmp(&b) };
        priority(pb).cmp(&priority(pa)).then(pa.element.cmp(&pb.element)).then(a.cmp(&b))
    });

    // 1. Trim overlaps by priority.
    let mut cleaned: Vec<Clean> = Vec::with_capacity(pieces.len());
    for &i in &order {
        let Some(p) = pieces.get(i) else { continue };
        let Some(bb) = poly::shape_bbox(&p.shape) else { continue };
        let blockers: Vec<Shape> = cleaned
            .iter()
            .filter(|c| pieces.get(c.piece).is_some_and(|q| q.element != p.element))
            .filter(|c| poly::boxes_overlap(&bb, &c.bbox, 1e-9))
            .flat_map(|c| c.shapes.iter().cloned())
            .collect();
        let shapes = if blockers.is_empty() {
            vec![p.shape.clone()]
        } else {
            poly::difference(std::slice::from_ref(&p.shape), &blockers)
        };
        if let Some(b) = bbox_of(&shapes) {
            cleaned.push(Clean { piece: i, shapes, bbox: b });
        }
    }

    // 2. Groups by (category, material, phase).
    let mut groups: Vec<Group> = Vec::new();
    let mut group_of: Vec<usize> = Vec::with_capacity(cleaned.len());
    for c in &cleaned {
        let Some(p) = pieces.get(c.piece) else { continue };
        let gi = match groups
            .iter()
            .position(|g| g.category == p.category && g.material == p.material && g.phase == p.phase)
        {
            Some(gi) => gi,
            None => {
                groups.push(Group {
                    category: p.category,
                    material: p.material,
                    phase: p.phase,
                    priority: priority(p),
                    thin: true,
                    union: Vec::new(),
                    bbox: c.bbox,
                });
                groups.len() - 1
            }
        };
        group_of.push(gi);
        if let Some(g) = groups.get_mut(gi) {
            for s in &c.shapes {
                let per = poly::shape_perimeter(s);
                let width = if per > 0.0 { 2.0 * poly::shape_area(s) / per } else { 0.0 };
                if width * 1000.0 / scale >= FINISH_MM {
                    g.thin = false;
                }
            }
            g.bbox = [
                g.bbox[0].min(c.bbox[0]),
                g.bbox[1].min(c.bbox[1]),
                g.bbox[2].max(c.bbox[2]),
                g.bbox[3].max(c.bbox[3]),
            ];
        }
    }
    for (gi, g) in groups.iter_mut().enumerate() {
        let shapes: Vec<Shape> = cleaned
            .iter()
            .zip(&group_of)
            .filter(|(_, g2)| **g2 == gi)
            .flat_map(|(c, _)| c.shapes.iter().cloned())
            .collect();
        g.union = poly::union(&shapes);
    }

    // 3. Fills.
    for (c, gi) in cleaned.iter().zip(&group_of) {
        let (Some(p), Some(g)) = (pieces.get(c.piece), groups.get(*gi)) else { continue };
        let fill = if g.thin { Fill::Solid(Rgba::WHITE) } else { material_fill(doc, p.material) };
        let fill = phase_fill(fill, p.phase);
        for s in &c.shapes {
            d.push(
                Item::polygon(rings_of(s), Some(fill.clone()), None, format!("{}.layer", p.category.key()))
                    .with_source(p.element.0),
            );
        }
    }

    // 4. Boundaries between different groups.
    let group_at = |q: [f64; 2], skip: usize| -> Option<usize> {
        groups.iter().enumerate().position(|(i, g)| {
            i != skip && in_box(&g.bbox, q) && g.union.iter().any(|s| poly::contains(s, q))
        })
    };
    for (gi, g) in groups.iter().enumerate() {
        for ring in ring_edges(&g.union) {
            let labels: Vec<Option<(Category, f64)>> = ring
                .iter()
                .map(|(a, b)| {
                    let q = probe(*a, *b, true)?;
                    let hi = group_at(q, gi)?;
                    let h = groups.get(hi)?;
                    if hi < gi || g.thin || h.thin {
                        return None;
                    }
                    if h.category != g.category {
                        let owner = if g.priority >= h.priority { g.category } else { h.category };
                        Some((owner, pens.cut_minor))
                    } else {
                        Some((g.category, pens.boundary))
                    }
                })
                .collect();
            for ((cat, w), pts, closed) in runs(&ring, &labels) {
                let layer = if (w - pens.boundary).abs() < 1e-12 {
                    format!("{}.layer", cat.key())
                } else {
                    format!("{}.cut", cat.key())
                };
                d.push(Item::polyline(pts, closed, phase_stroke(Stroke::solid(w), g.phase), layer));
            }
        }
    }

    // 5. Heavy outline per phase.
    for phase in [Phase::New, Phase::Existing, Phase::Demolished] {
        let shapes: Vec<Shape> = cleaned
            .iter()
            .filter(|c| pieces.get(c.piece).is_some_and(|p| p.phase == phase))
            .flat_map(|c| c.shapes.iter().cloned())
            .collect();
        if shapes.is_empty() {
            continue;
        }
        let union = poly::union(&shapes);
        let owner = |q: [f64; 2]| -> Option<(Category, Id)> {
            cleaned.iter().find_map(|c| {
                let p = pieces.get(c.piece)?;
                if p.phase != phase || !in_box(&c.bbox, q) || !c.shapes.iter().any(|s| poly::contains(s, q)) {
                    return None;
                }
                Some((p.category, p.element))
            })
        };
        let weight = if phase == Phase::Demolished { pens.cut_minor } else { pens.cut };
        for ring in ring_edges(&union) {
            let labels: Vec<Option<(Category, Id)>> =
                ring.iter().map(|(a, b)| probe(*a, *b, false).and_then(owner)).collect();
            // Edges no piece claims (numerical slivers) join the neighbouring run.
            let labels = fill_gaps(labels);
            for ((cat, id), pts, closed) in runs(&ring, &labels) {
                d.push(
                    Item::polyline(
                        pts,
                        closed,
                        phase_stroke(Stroke::solid(weight), phase),
                        format!("{}.cut", cat.key()),
                    )
                    .with_source(id.0),
                );
            }
        }
    }
}

/// Replaces missing labels by the previous label in ring order (or the next one at the start).
fn fill_gaps<L: Clone>(labels: Vec<Option<L>>) -> Vec<Option<L>> {
    let Some(any) = labels.iter().flatten().next().cloned() else { return labels };
    let n = labels.len();
    let start = labels.iter().position(Option::is_some).unwrap_or(0);
    let mut out = labels.clone();
    let mut last = labels.get(start).cloned().flatten().unwrap_or(any);
    for k in 0..n {
        let i = (start + k) % n;
        match labels.get(i).cloned().flatten() {
            Some(l) => last = l,
            None => {
                if let Some(slot) = out.get_mut(i) {
                    *slot = Some(last.clone());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::Shape as DShape;
    use kanavos_model::starter;

    fn piece(element: u64, material: Id, shape: Shape) -> CutPiece {
        CutPiece {
            element: Id(element),
            category: Category::Wall,
            phase: Phase::New,
            material: Some(material),
            shape,
        }
    }

    #[test]
    fn touching_walls_of_one_material_get_one_outline() {
        let doc = starter();
        let brick = doc.materials().find(|m| m.name == "Οπτοπλινθοδομή").unwrap().id;
        let pieces = vec![
            piece(1, brick, poly::rect(0.0, 0.0, 5.0, 0.25)),
            piece(2, brick, poly::rect(4.75, 0.0, 5.0, 4.0)),
        ];
        let mut d = Drawing::model(50.0);
        draw_cut(&mut d, &doc, &pieces, &Pens::for_scale(50.0), 50.0);
        let outlines: Vec<&Item> = d.items.iter().filter(|i| i.layer == "wall.cut").collect();
        // One L-shaped ring, split between the two walls.
        assert_eq!(outlines.len(), 2);
        let pts: usize = outlines
            .iter()
            .map(|i| match &i.shape {
                DShape::Polyline { points, .. } => points.len(),
                _ => 0,
            })
            .sum();
        assert!(pts >= 6);
        // No boundary line between the two brick areas.
        assert!(d.items.iter().all(|i| i.layer != "wall.layer" || i.fill.is_some()));
    }

    #[test]
    fn concrete_runs_through_brick() {
        let doc = starter();
        let brick = doc.materials().find(|m| m.name == "Οπτοπλινθοδομή").unwrap().id;
        let concrete = doc.materials().find(|m| m.name == "Οπλισμένο σκυρόδεμα").unwrap().id;
        let pieces = vec![
            piece(1, brick, poly::rect(0.0, 0.0, 5.0, 0.25)),
            CutPiece {
                element: Id(2),
                category: Category::Column,
                phase: Phase::New,
                material: Some(concrete),
                shape: poly::rect(2.0, -0.05, 2.4, 0.3),
            },
        ];
        let mut d = Drawing::model(50.0);
        draw_cut(&mut d, &doc, &pieces, &Pens::for_scale(50.0), 50.0);
        // The wall fill is split in two by the column.
        let wall_fills = d.items.iter().filter(|i| i.layer == "wall.layer" && i.fill.is_some()).count();
        assert_eq!(wall_fills, 2);
        // The column keeps a minor outline inside the wall.
        assert!(d.items.iter().any(|i| i.layer == "column.cut"));
    }
}
