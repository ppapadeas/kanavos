//! Editing commands added for the full workflow: joining walls and aligning elements.

use kanavos_geom::{Bbox2, Line2, Point2, Transform2, Vec2};
use kanavos_model::{Document, Element, ElementKind, Id};
use kanavos_units::{Length, Locale};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::geometry;
use crate::labels::{counted, noun_key};
use crate::{Command, CommandError, Outcome};

fn invalid(field: &'static str, reason: impl Into<String>) -> CommandError {
    CommandError::InvalidArgument { field, reason: reason.into() }
}

// ----- wall.join -----------------------------------------------------------------------------------------------

/// `wall.join` — extends or trims two straight walls so that their reference lines meet at a corner. The end of
/// each wall nearer to the corner moves; doors and windows keep their place in the world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct JoinWalls {
    /// First wall.
    pub first: Id,
    /// Second wall.
    pub second: Id,
}

/// Moves the end of a straight wall nearer to `corner` onto it; returns the changed wall and its openings.
fn move_end_to(
    doc: &Document,
    id: Id,
    corner: Point2,
    field: &'static str,
) -> Result<Vec<Element>, CommandError> {
    let e = doc.element(id).ok_or(CommandError::NotFound(id))?;
    let w = e.as_wall().ok_or_else(|| invalid(field, "is not a wall"))?;
    if w.arc().is_some() {
        return Err(invalid(field, "must be a straight wall"));
    }
    let tol = doc.tolerance();
    let move_start = corner.distance(w.start) < corner.distance(w.end);
    let mut nw = w.clone();
    if move_start {
        if w.start.coincides(corner, &tol) {
            return Ok(Vec::new());
        }
        nw.start = corner;
    } else {
        if w.end.coincides(corner, &tol) {
            return Ok(Vec::new());
        }
        nw.end = corner;
    }
    if nw.start.coincides(nw.end, &tol) {
        return Err(invalid(field, "would shrink to nothing"));
    }
    let old_dir = w.direction().unwrap_or(Vec2::X);
    if nw.direction().is_none_or(|d| d.dot(old_dir) < 0.0) {
        return Err(invalid(field, "would be turned around; the corner lies beyond its other end"));
    }
    let mut changed = vec![Element { kind: ElementKind::Wall(nw.clone()), ..e.clone() }];
    if move_start {
        // Keep openings in place: their offsets are measured from the start.
        let shift = (w.start - nw.start).dot(old_dir);
        for (oid, o) in doc.openings_in(id) {
            let Some(oe) = doc.element(oid) else { continue };
            let mut o2 = o.clone();
            o2.offset += Length::m(shift);
            changed.push(Element { kind: ElementKind::Opening(o2), ..oe.clone() });
        }
    }
    Ok(changed)
}

impl Command for JoinWalls {
    fn name(&self) -> &'static str {
        "wall.join"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if self.first == self.second {
            return Err(invalid("second", "must be another wall"));
        }
        let a = doc.element(self.first).ok_or(CommandError::NotFound(self.first))?;
        let b = doc.element(self.second).ok_or(CommandError::NotFound(self.second))?;
        let (Some(wa), Some(wb)) = (a.as_wall(), b.as_wall()) else {
            let field = if a.as_wall().is_none() { "first" } else { "second" };
            return Err(invalid(field, "is not a wall"));
        };
        if a.storey != b.storey {
            return Err(invalid("second", "must be on the same storey as the first wall"));
        }
        let corner = Line2::from_segment(&wa.segment())
            .intersection(&Line2::from_segment(&wb.segment()))
            .ok_or_else(|| invalid("second", "is parallel to the first wall"))?;
        let mut changed = move_end_to(doc, self.first, corner, "first")?;
        changed.extend(move_end_to(doc, self.second, corner, "second")?);
        if changed.is_empty() {
            return Err(invalid("second", "already meets the first wall"));
        }
        let mut next = doc.clone();
        for e in &changed {
            next = next.with_element(e.clone());
        }
        for e in &changed {
            kanavos_elements::validate(&next, e).map_err(geometry)?;
        }
        Ok((next, Outcome { changed: changed.iter().map(|e| e.id).collect(), ..Outcome::default() }))
    }
}

// ----- element.align -------------------------------------------------------------------------------------------

/// Which side of the elements' bounding boxes to line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlignEdge {
    /// Smallest x (west).
    #[default]
    Left,
    /// Largest x (east).
    Right,
    /// Smallest y (south).
    Bottom,
    /// Largest y (north).
    Top,
    /// Centre in x.
    CentreX,
    /// Centre in y.
    CentreY,
}

/// `element.align` — moves elements so that one side (or centre) of their plan extents lines up with the first
/// element's, or with a given coordinate. Doors and windows follow their walls and cannot be aligned themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AlignElements {
    /// Elements; the first is the reference unless `to` is given.
    pub ids: Vec<Id>,
    /// What to line up.
    #[serde(default)]
    pub edge: AlignEdge,
    /// Coordinate to align to (x for left/right/centre_x, y otherwise), metres.
    #[serde(default)]
    pub to: Option<f64>,
}

/// Plan extents of an element: its footprint for building parts, its snap points for annotation.
fn extents(doc: &Document, id: Id) -> Option<Bbox2> {
    if let Ok(Some(fp)) = kanavos_elements::footprint(doc, id) {
        return fp.bbox();
    }
    let g = kanavos_elements::snap_geometry(doc, id);
    let pts = g.points.iter().map(|(p, _)| *p).chain(g.segments.iter().flat_map(|s| [s.start, s.end]));
    Bbox2::from_points(pts)
}

fn value(b: &Bbox2, edge: AlignEdge) -> f64 {
    match edge {
        AlignEdge::Left => b.min.x.as_m(),
        AlignEdge::Right => b.max.x.as_m(),
        AlignEdge::Bottom => b.min.y.as_m(),
        AlignEdge::Top => b.max.y.as_m(),
        AlignEdge::CentreX => b.centre().x.as_m(),
        AlignEdge::CentreY => b.centre().y.as_m(),
    }
}

impl Command for AlignElements {
    fn name(&self) -> &'static str {
        "element.align"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if self.ids.is_empty() || (self.ids.len() < 2 && self.to.is_none()) {
            return Err(invalid("ids", "needs two elements, or one element and `to`"));
        }
        let mut boxes = Vec::with_capacity(self.ids.len());
        for id in &self.ids {
            let e = doc.element(*id).ok_or(CommandError::NotFound(*id))?;
            if matches!(e.kind, ElementKind::Opening(_)) {
                return Err(invalid("ids", "doors and windows move with their wall"));
            }
            let b = extents(doc, *id)
                .ok_or_else(|| invalid("ids", format!("element {id} has no plan position")))?;
            boxes.push((e, b));
        }
        if let Some(to) = self.to
            && !to.is_finite()
        {
            return Err(invalid("to", "must be a finite coordinate"));
        }
        let target = match (self.to, boxes.first()) {
            (Some(t), _) => t,
            (None, Some((_, b))) => value(b, self.edge),
            (None, None) => return Err(invalid("ids", "must not be empty")),
        };
        let horizontal = matches!(self.edge, AlignEdge::Left | AlignEdge::Right | AlignEdge::CentreX);
        let tol = doc.tolerance();
        let mut next = doc.clone();
        let mut out = Outcome::default();
        for (e, b) in boxes {
            let delta = target - value(&b, self.edge);
            if tol.is_zero(Length::m(delta)) {
                continue;
            }
            let v = if horizontal { Vec2::new(delta, 0.0) } else { Vec2::new(0.0, delta) };
            let moved = Element { kind: e.kind.transformed(&Transform2::translation(v)), ..e.clone() };
            kanavos_elements::validate(&next, &moved).map_err(geometry)?;
            next = next.with_element(moved);
            out.changed.push(e.id);
        }
        if out.changed.is_empty() {
            return Err(invalid("ids", "are already aligned"));
        }
        Ok((next, out))
    }
    fn label(&self, locale: Locale) -> String {
        counted(locale, "align", self.ids.len(), "element")
    }
    fn describe(&self, doc: &Document, locale: Locale) -> String {
        counted(locale, "align", self.ids.len(), noun_key(Some(doc), &self.ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::commands::CreateElement;
    use kanavos_model::{Column, Opening, OpeningKind, Profile, ReferenceLine, Structure, Wall, starter};
    use kanavos_units::Angle;

    fn wall(s: &mut Session, st: Id, a: (f64, f64), b: (f64, f64)) -> Id {
        let bu = s.document().defaults().interior_wall.unwrap();
        s.run(CreateElement::on(
            st,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Layered { build_up: bu },
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        ))
        .unwrap()
        .created[0]
    }

    #[test]
    fn join_extends_and_trims_to_the_corner() {
        let mut s = Session::new(starter());
        let st = s.document().storeys()[0].id;
        let a = wall(&mut s, st, (0.0, 0.0), (4.0, 0.0));
        let b = wall(&mut s, st, (5.0, 1.0), (5.0, 6.0));
        // A door on b, 2 m from its start; b's start moves down by 1 m so the offset grows by 1 m.
        let door = s
            .run(CreateElement::on(
                st,
                ElementKind::Opening(Opening {
                    host: b,
                    kind: OpeningKind::Door,
                    style: "door.single_swing".into(),
                    offset: Length::m(2.0),
                    width: Length::m(0.9),
                    height: Length::m(2.1),
                    sill: Length::ZERO,
                    flip_side: false,
                    flip_hand: false,
                    params: Default::default(),
                }),
            ))
            .unwrap()
            .created[0];
        s.run(JoinWalls { first: a, second: b }).unwrap();
        let doc = s.document();
        let wa = doc.element(a).unwrap().as_wall().unwrap();
        let wb = doc.element(b).unwrap().as_wall().unwrap();
        let tol = doc.tolerance();
        assert!(wa.end.coincides(Point2::new(5.0, 0.0), &tol));
        assert!(wb.start.coincides(Point2::new(5.0, 0.0), &tol));
        let o = doc.element(door).unwrap().as_opening().unwrap();
        assert!(tol.equal(o.offset, Length::m(3.0)));
        assert!(s.run(JoinWalls { first: a, second: b }).is_err(), "already joined");
    }

    #[test]
    fn join_rejects_parallel_walls() {
        let mut s = Session::new(starter());
        let st = s.document().storeys()[0].id;
        let a = wall(&mut s, st, (0.0, 0.0), (4.0, 0.0));
        let b = wall(&mut s, st, (0.0, 2.0), (4.0, 2.0));
        let err = s.run(JoinWalls { first: a, second: b }).unwrap_err();
        assert_eq!(err.field(), Some("second"));
    }

    #[test]
    fn align_lines_up_columns() {
        let mut s = Session::new(starter());
        let st = s.document().storeys()[0].id;
        let concrete = s.document().defaults().structural_material.unwrap();
        let mut ids = Vec::new();
        for (x, y) in [(0.0, 0.0), (3.0, 0.4), (6.0, -0.2)] {
            ids.push(
                s.run(CreateElement::on(
                    st,
                    ElementKind::Column(Column {
                        position: Point2::new(x, y),
                        rotation: Angle::ZERO,
                        profile: Profile::Rectangle { width: Length::cm(30.0), depth: Length::cm(30.0) },
                        material: concrete,
                        height: Length::m(3.0),
                        base_offset: Length::ZERO,
                    }),
                ))
                .unwrap()
                .created[0],
            );
        }
        s.run(AlignElements { ids: ids.clone(), edge: AlignEdge::Bottom, to: None }).unwrap();
        for id in &ids {
            let b = extents(s.document(), *id).unwrap();
            assert!((b.min.y.as_m() + 0.15).abs() < 1e-9);
        }
        assert_eq!(s.history_labels(Locale::El).last().unwrap(), "Ευθυγράμμιση 3 υποστυλωμάτων");
        assert!(s.run(AlignElements { ids, edge: AlignEdge::Bottom, to: None }).is_err());
    }
}
