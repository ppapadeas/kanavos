//! The select tool (`specs/tools/selection.md`).
//!
//! - Click selects the element under the pointer (Shift adds, Ctrl toggles); clicking empty space clears.
//! - Dragging over empty space selects with a rectangle: left to right selects what is fully inside (window),
//!   right to left also what it touches (crossing).
//! - The element under the pointer is pre-highlighted.
//! - Handles on the selection: wall ends (connected wall ends follow) and wall midpoints (move the wall
//!   sideways), opening position and jambs (width), outline and path vertices (drag) and edge midpoints (drag to
//!   insert a vertex), and the position or ends of columns, objects, stairs, text, level marks, beams, grid lines,
//!   drafting and dimensions.
//! - Dragging the selection moves it with snapping (from the grabbed point); typing while dragging sets the
//!   distance (Enter commits); Ctrl copies.
//! - The rotate key rotates the selection (centre, reference direction, target direction, or typed degrees); the
//!   mirror key mirrors it about an axis through two points; Ctrl at the last click keeps the original.
//! - Delete removes the selection; the flip key flips selected doors and windows to the other side of their wall
//!   (with Shift: the other hand); Escape backs out, then clears the selection.

use std::collections::BTreeSet;

use kanavos_drawing::{Fill, Item, LinePattern, Rgba, Stroke, pt, weights};
use kanavos_engine::commands::{Batch, DeleteElements, TransformElements, TransformSpec, UpdateElement};
use kanavos_engine::{AnyCommand, Command};
use kanavos_geom::{Bbox2, Point2, PolygonWithHoles, Vec2};
use kanavos_i18n::{t, tf};
use kanavos_model::{DraftShape, Element, ElementKind, Id, Wall};

use crate::common::{
    Field, Fields, Pointer, angle_text, arc_through, length_text, one_step, tr, typed_target,
};
use crate::preview::{self, BLUE, RED};
use crate::snap::Snapper;
use crate::{
    CursorHint, Handle, HandleRole, Key, Modifiers, MouseButton, Tool, ToolContext, ToolEvent, ToolId,
    ToolOutput, hit_test,
};

/// Handles are shown when at most this many elements are selected.
const MAX_HANDLE_ELEMENTS: usize = 30;
/// Moves of more elements than this preview as outlines instead of ghosts.
const MAX_GHOST_ELEMENTS: usize = 150;

/// What a handle edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grip {
    /// A wall end; connected wall ends follow.
    WallEnd { wall: Id, start: bool },
    /// A wall's midpoint: moves the wall sideways; connected wall ends follow.
    WallSide { wall: Id },
    /// An opening's centre: slides it along its wall.
    OpeningMove { opening: Id },
    /// An opening's jamb (`low` = the jamb nearer the wall start): changes the width.
    OpeningJamb { opening: Id, low: bool },
    /// A vertex of an outline ring or path.
    Vertex { element: Id, ring: usize, index: usize },
    /// The midpoint of edge `index` of a ring or path: inserts a vertex after `index`.
    Insert { element: Id, ring: usize, index: usize },
    /// Point `which` of an element (see [`points_of`]).
    Point { element: Id, which: usize },
}

impl Grip {
    fn element(&self) -> Id {
        match *self {
            Grip::WallEnd { wall, .. } | Grip::WallSide { wall } => wall,
            Grip::OpeningMove { opening } | Grip::OpeningJamb { opening, .. } => opening,
            Grip::Vertex { element, .. } | Grip::Insert { element, .. } | Grip::Point { element, .. } => {
                element
            }
        }
    }
}

/// What releasing the button without dragging does to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickAction {
    /// Nothing more (the press already changed the selection).
    Keep,
    /// Select only the clicked element.
    Narrow,
    /// Remove the clicked element (Ctrl-click on a selected element; a Ctrl-drag copies instead).
    Toggle,
}

#[derive(Debug, Clone, Default)]
enum State {
    #[default]
    Idle,
    /// Button down, not dragging yet; `click` is what a release without dragging does.
    Pressed { at: Point2, grab: Point2, element: Option<Id>, click: ClickAction },
    /// Rectangle selection.
    Box { from: Point2, to: Point2 },
    /// Moving the selection. `waiting`: released with typed input, waiting for Enter.
    Move { grab: Point2, to: Point2, waiting: bool },
    /// Dragging a handle.
    Grip { grip: Grip, origin: Point2, to: Point2 },
    /// Rotating: centre, then reference direction, then target.
    Rotate { centre: Option<Point2>, reference: Option<Point2> },
    /// Mirroring: first axis point, then second.
    Mirror { a: Option<Point2> },
}

/// Selection tool.
#[derive(Debug, Clone, Default)]
pub struct SelectTool {
    state: State,
    pointer: Pointer,
    fields: Fields,
}

fn update(id: Id, kind: ElementKind) -> AnyCommand {
    UpdateElement { id, kind: Some(kind), ..UpdateElement::default() }.into()
}

/// Rings (closed outlines) and paths of an element, with whether each is closed.
fn rings_of(kind: &ElementKind) -> Vec<(Vec<Point2>, bool)> {
    let region = |r: &PolygonWithHoles| -> Vec<(Vec<Point2>, bool)> {
        r.rings().map(|p| (p.vertices.clone(), true)).collect()
    };
    match kind {
        ElementKind::Slab(s) => region(&s.outline),
        ElementKind::Roof(r) => {
            std::iter::once(&r.outline).chain(&r.holes).map(|p| (p.vertices.clone(), true)).collect()
        }
        ElementKind::Space(s) => region(&s.boundary),
        ElementKind::Railing(r) => vec![(r.path.clone(), false)],
        ElementKind::Drafting(d) => match &d.shape {
            DraftShape::Fill { region: r } => region(r),
            DraftShape::Polyline { points, closed } => vec![(points.clone(), *closed)],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn region_ring(r: &mut PolygonWithHoles, ring: usize) -> Option<&mut Vec<Point2>> {
    if ring == 0 { Some(&mut r.outer.vertices) } else { r.holes.get_mut(ring - 1).map(|h| &mut h.vertices) }
}

fn ring_mut(kind: &mut ElementKind, ring: usize) -> Option<&mut Vec<Point2>> {
    let region = region_ring;
    match kind {
        ElementKind::Slab(s) => region(&mut s.outline, ring),
        ElementKind::Roof(r) => match ring.checked_sub(1) {
            None => Some(&mut r.outline.vertices),
            Some(hole) => r.holes.get_mut(hole).map(|h| &mut h.vertices),
        },
        ElementKind::Space(s) => region(&mut s.boundary, ring),
        ElementKind::Railing(r) if ring == 0 => Some(&mut r.path),
        ElementKind::Drafting(d) => match &mut d.shape {
            DraftShape::Fill { region: r } => region(r, ring),
            DraftShape::Polyline { points, .. } if ring == 0 => Some(points),
            _ => None,
        },
        _ => None,
    }
}

/// Editable points of an element with their handle roles.
fn points_of(kind: &ElementKind) -> Vec<(Point2, HandleRole)> {
    use HandleRole::{End, Move, Vertex};
    match kind {
        ElementKind::Column(c) => vec![(c.position, Move)],
        ElementKind::Object(o) => vec![(o.position, Move)],
        ElementKind::Stair(s) => vec![(s.origin, Move)],
        ElementKind::Text(x) => vec![(x.position, Move)],
        ElementKind::Level(l) => vec![(l.position, Move)],
        ElementKind::Beam(b) => vec![(b.start, End), (b.end, End)],
        ElementKind::Grid(g) => vec![(g.start, End), (g.end, End)],
        ElementKind::Space(s) => {
            s.stamp.or_else(|| s.boundary.outer.centroid()).map(|p| vec![(p, Move)]).unwrap_or_default()
        }
        ElementKind::Dimension(d) => {
            let u = Vec2::from_angle(d.direction);
            let mid = if d.points.is_empty() {
                d.line_point
            } else {
                let mean =
                    d.points.iter().map(|p| (*p - d.line_point).dot(u)).sum::<f64>() / d.points.len() as f64;
                d.line_point + u * mean
            };
            let mut v = vec![(mid, Move)];
            v.extend(d.points.iter().map(|p| (*p, Vertex)));
            v
        }
        ElementKind::Drafting(df) => match &df.shape {
            DraftShape::Line { a, b } => vec![(*a, End), (*b, End)],
            DraftShape::Circle { centre, radius } => {
                vec![(*centre, Move), (*centre + Vec2::X * radius.as_m(), End)]
            }
            DraftShape::Arc { arc } => vec![(arc.at(0.0), End), (arc.at(1.0), End), (arc.at(0.5), Move)],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The element with point `which` (see [`points_of`]) moved to `p`.
fn set_point(kind: &ElementKind, which: usize, p: Point2) -> Option<ElementKind> {
    let mut k = kind.clone();
    match &mut k {
        ElementKind::Column(c) => c.position = p,
        ElementKind::Object(o) => o.position = p,
        ElementKind::Stair(s) => s.origin = p,
        ElementKind::Text(x) => x.position = p,
        ElementKind::Level(l) => l.position = p,
        ElementKind::Beam(b) => *(if which == 0 { &mut b.start } else { &mut b.end }) = p,
        ElementKind::Grid(g) => *(if which == 0 { &mut g.start } else { &mut g.end }) = p,
        ElementKind::Space(s) => s.stamp = Some(p),
        ElementKind::Dimension(d) => {
            if which == 0 {
                let (old, _) = points_of(kind).first().copied()?;
                d.line_point = d.line_point + (p - old);
            } else {
                *d.points.get_mut(which - 1)? = p;
            }
        }
        ElementKind::Drafting(df) => match &mut df.shape {
            DraftShape::Line { a, b } => *(if which == 0 { a } else { b }) = p,
            DraftShape::Circle { centre, radius } => {
                if which == 0 {
                    *centre = p;
                } else {
                    *radius = centre.distance(p);
                }
            }
            DraftShape::Arc { arc } => {
                let (mut s, mut e, mut m) = (arc.at(0.0), arc.at(1.0), arc.at(0.5));
                match which {
                    0 => s = p,
                    1 => e = p,
                    _ => m = p,
                }
                *arc = arc_through(s, m, e)?;
            }
            _ => return None,
        },
        _ => return None,
    }
    Some(k)
}

/// Commands that move one end of a wall, re-placing its openings so they keep their distance from the fixed end.
fn move_wall_ends(
    doc: &kanavos_model::Document,
    id: Id,
    w: &Wall,
    start: Option<Point2>,
    end: Option<Point2>,
) -> Vec<AnyCommand> {
    let mut w2 = w.clone();
    if let Some(s) = start {
        w2.start = s;
    }
    if let Some(e) = end {
        w2.end = e;
    }
    let mut cmds = vec![update(id, ElementKind::Wall(w2.clone()))];
    let (old_len, new_len) = (w.length(), w2.length());
    for (oid, o) in doc.openings_in(id) {
        let mut o2 = o.clone();
        if start.is_some() && end.is_none() {
            o2.offset = new_len - (old_len - o.offset);
        }
        cmds.push(update(oid, ElementKind::Opening(o2)));
    }
    cmds
}

/// Walls (other than `except`) with an end at `at`: (wall, wall data, whether it is the start).
fn walls_ending_at(ctx: &ToolContext<'_>, at: Point2, except: Id) -> Vec<(Id, Wall, bool)> {
    let tol = kanavos_units::Tolerance { length: kanavos_units::Length::mm(1.0), ..ctx.doc.tolerance() };
    ctx.doc
        .walls_on(ctx.storey)
        .filter(|(id, _)| *id != except)
        .filter_map(|(id, w)| {
            if w.start.coincides(at, &tol) {
                Some((id, w.clone(), true))
            } else if w.end.coincides(at, &tol) {
                Some((id, w.clone(), false))
            } else {
                None
            }
        })
        .collect()
}

impl SelectTool {
    fn radius(ctx: &ToolContext<'_>) -> f64 {
        6.0 * ctx.pixel_size
    }

    /// Handles of the current selection.
    fn handles(ctx: &ToolContext<'_>) -> Vec<(Handle, Grip)> {
        let mut out = Vec::new();
        if ctx.selection.len() > MAX_HANDLE_ELEMENTS {
            return out;
        }
        let mut push = |at: Point2, role: HandleRole, element: Id, grip: Grip| {
            out.push((Handle { at, role, element, hot: false }, grip));
        };
        for id in ctx.selection {
            let Some(e) = ctx.doc.element(*id) else { continue };
            match &e.kind {
                ElementKind::Wall(w) => {
                    push(w.start, HandleRole::End, *id, Grip::WallEnd { wall: *id, start: true });
                    push(w.end, HandleRole::End, *id, Grip::WallEnd { wall: *id, start: false });
                    let mid = w.arc().map(|a| a.at(0.5)).unwrap_or_else(|| w.start.midpoint(w.end));
                    push(mid, HandleRole::Move, *id, Grip::WallSide { wall: *id });
                }
                ElementKind::Opening(_) => {
                    if let Ok(pl) = kanavos_elements::opening_placement(ctx.doc, *id) {
                        let h = pl.width.as_m() / 2.0;
                        push(pl.centre, HandleRole::Move, *id, Grip::OpeningMove { opening: *id });
                        push(
                            pl.centre - pl.along * h,
                            HandleRole::Width,
                            *id,
                            Grip::OpeningJamb { opening: *id, low: true },
                        );
                        push(
                            pl.centre + pl.along * h,
                            HandleRole::Width,
                            *id,
                            Grip::OpeningJamb { opening: *id, low: false },
                        );
                    }
                }
                kind => {
                    for (ring, (pts, closed)) in rings_of(kind).iter().enumerate() {
                        let n = pts.len();
                        for (index, p) in pts.iter().enumerate() {
                            push(*p, HandleRole::Vertex, *id, Grip::Vertex { element: *id, ring, index });
                            let next = if index + 1 < n {
                                pts.get(index + 1)
                            } else if *closed {
                                pts.first()
                            } else {
                                None
                            };
                            if let Some(q) = next {
                                push(
                                    p.midpoint(*q),
                                    HandleRole::InsertVertex,
                                    *id,
                                    Grip::Insert { element: *id, ring, index },
                                );
                            }
                        }
                    }
                    for (which, (p, role)) in points_of(kind).into_iter().enumerate() {
                        push(p, role, *id, Grip::Point { element: *id, which });
                    }
                }
            }
        }
        out
    }

    fn handle_at(ctx: &ToolContext<'_>, at: Point2) -> Option<(Handle, Grip)> {
        let r = Self::radius(ctx);
        Self::handles(ctx)
            .into_iter()
            .filter(|(h, _)| h.at.distance(at).as_m() <= r)
            .min_by(|a, b| a.0.at.distance(at).total_cmp(&b.0.at.distance(at)))
    }

    /// The fixed point typed lengths of a handle drag are measured from.
    fn grip_anchor(ctx: &ToolContext<'_>, grip: Grip, origin: Point2) -> Point2 {
        let e = ctx.doc.element(grip.element());
        match (grip, e.map(|e| &e.kind)) {
            (Grip::WallEnd { start, .. }, Some(ElementKind::Wall(w))) => {
                if start {
                    w.end
                } else {
                    w.start
                }
            }
            (Grip::Point { which, .. }, Some(k)) => {
                let pts = points_of(k);
                if pts.len() == 2 && matches!(pts.first(), Some((_, HandleRole::End))) {
                    pts.get(1 - which.min(1)).map(|(p, _)| *p).unwrap_or(origin)
                } else {
                    origin
                }
            }
            _ => origin,
        }
    }

    /// Commands for dragging a handle from `origin` to `to`.
    fn grip_commands(ctx: &ToolContext<'_>, grip: Grip, origin: Point2, to: Point2) -> Vec<AnyCommand> {
        let doc = ctx.doc;
        let Some(e) = doc.element(grip.element()) else { return Vec::new() };
        let mut cmds = Vec::new();
        match (grip, &e.kind) {
            (Grip::WallEnd { wall, start }, ElementKind::Wall(w)) => {
                let old = if start { w.start } else { w.end };
                cmds.extend(if start {
                    move_wall_ends(doc, wall, w, Some(to), None)
                } else {
                    move_wall_ends(doc, wall, w, None, Some(to))
                });
                for (id, other, is_start) in walls_ending_at(ctx, old, wall) {
                    cmds.extend(if is_start {
                        move_wall_ends(doc, id, &other, Some(to), None)
                    } else {
                        move_wall_ends(doc, id, &other, None, Some(to))
                    });
                }
            }
            (Grip::WallSide { wall }, ElementKind::Wall(w)) => {
                let Some(dir) = w.direction() else { return cmds };
                let n = dir.perp();
                let v = n * (to - origin).dot(n);
                let (s, e2) = (w.start + v, w.end + v);
                cmds.extend(move_wall_ends(doc, wall, w, Some(s), Some(e2)));
                for (old, new) in [(w.start, s), (w.end, e2)] {
                    for (id, other, is_start) in walls_ending_at(ctx, old, wall) {
                        cmds.extend(if is_start {
                            move_wall_ends(doc, id, &other, Some(new), None)
                        } else {
                            move_wall_ends(doc, id, &other, None, Some(new))
                        });
                    }
                }
            }
            (Grip::OpeningMove { opening }, ElementKind::Opening(o)) => {
                if let Some(w) = doc.element(o.host).and_then(Element::as_wall) {
                    let (a, _) = crate::opening::wall_param(w, origin);
                    let (b, _) = crate::opening::wall_param(w, to);
                    let mut o2 = o.clone();
                    o2.offset += kanavos_units::Length::m(b - a);
                    cmds.push(update(opening, ElementKind::Opening(o2)));
                }
            }
            (Grip::OpeningJamb { opening, low }, ElementKind::Opening(o)) => {
                if let Some(w) = doc.element(o.host).and_then(Element::as_wall) {
                    let (p, _) = crate::opening::wall_param(w, to);
                    let h = o.width.as_m() / 2.0;
                    let (mut lo, mut hi) = (o.offset.as_m() - h, o.offset.as_m() + h);
                    if low {
                        lo = p.min(hi - 0.05);
                    } else {
                        hi = p.max(lo + 0.05);
                    }
                    let mut o2 = o.clone();
                    o2.offset = kanavos_units::Length::m((lo + hi) / 2.0);
                    o2.width = kanavos_units::Length::m(hi - lo);
                    cmds.push(update(opening, ElementKind::Opening(o2)));
                }
            }
            (Grip::Vertex { element, ring, index }, kind) => {
                let mut k = kind.clone();
                if let Some(v) = ring_mut(&mut k, ring).and_then(|r| r.get_mut(index)) {
                    *v = to;
                    cmds.push(update(element, k));
                }
            }
            (Grip::Insert { element, ring, index }, kind) => {
                let mut k = kind.clone();
                if let Some(r) = ring_mut(&mut k, ring)
                    && index < r.len()
                {
                    r.insert(index + 1, to);
                    // Only the outline has a slope per edge; holes have none.
                    if ring == 0
                        && let ElementKind::Roof(roof) = &mut k
                        && let kanavos_model::RoofShape::Pitched { slopes } = &mut roof.shape
                    {
                        let s = slopes.get(index).copied().flatten();
                        slopes.insert((index + 1).min(slopes.len()), s);
                    }
                    // A railing that follows a slope gets a base at the new point, interpolated along the edge.
                    if let ElementKind::Railing(rl) = &mut k
                        && !rl.elevations.is_empty()
                        && let (Some(a), Some(b)) =
                            (rl.path.get(index).copied(), rl.path.get(index + 2).copied())
                        && let (Some(za), Some(zb)) =
                            (rl.elevations.get(index).copied(), rl.elevations.get(index + 1).copied())
                    {
                        let t = kanavos_geom::Segment2::new(a, b).project_param(to).clamp(0.0, 1.0);
                        rl.elevations.insert(index + 1, za + (zb - za) * t);
                    }
                    cmds.push(update(element, k));
                }
            }
            (Grip::Point { element, which }, kind) => {
                if let Some(k) = set_point(kind, which, to) {
                    cmds.push(update(element, k));
                }
            }
            _ => {}
        }
        cmds
    }

    /// Elements whose geometry a command list changes (for previews and snapping exclusions).
    fn touched(cmds: &[AnyCommand]) -> BTreeSet<Id> {
        cmds.iter()
            .filter_map(|c| match c {
                AnyCommand::UpdateElement(u) => Some(u.id),
                _ => None,
            })
            .collect()
    }

    fn transform(ctx: &ToolContext<'_>, transform: TransformSpec, copy: bool) -> AnyCommand {
        TransformElements { ids: ctx.selection.iter().copied().collect(), transform, copy, count: 1 }.into()
    }

    /// Previews a command: the changed (or created) elements as ghosts, or why it fails.
    fn preview_command(ctx: &ToolContext<'_>, out: &mut ToolOutput, cmd: &AnyCommand) -> bool {
        // Large selections preview as transformed outlines of their extents, without applying the command.
        if let AnyCommand::TransformElements(t) = cmd
            && t.ids.len() > MAX_GHOST_ELEMENTS
        {
            let index = Snapper::cached(ctx.doc, ctx.storey);
            let m = t.transform.to_transform();
            let mut d = preview::drawing(ctx);
            for id in &t.ids {
                if let Some(b) = index.element_box(*id) {
                    let corners = [
                        b.min,
                        Point2::from_lengths(b.max.x, b.min.y),
                        b.max,
                        Point2::from_lengths(b.min.x, b.max.y),
                    ]
                    .map(|q| m.apply(q));
                    preview::polyline(&mut d, &corners, true, BLUE);
                }
            }
            out.preview.extend(d);
            return true;
        }
        match cmd.apply(ctx.doc) {
            Ok((next, outcome)) => {
                let mut ids: BTreeSet<Id> = outcome.changed.into_iter().chain(outcome.created).collect();
                ids.retain(|id| next.element(*id).is_some());
                out.preview.extend(preview::ghost_of(ctx, &next, &ids, BLUE));
                true
            }
            Err(e) => {
                out.warning = Some(e.message(ctx.locale));
                false
            }
        }
    }

    fn move_target(&self, ctx: &ToolContext<'_>, grab: Point2, to: Point2) -> Point2 {
        typed_target(&self.fields, Some(grab), Some(to), ctx.doc).unwrap_or(to)
    }

    fn commit_move(
        &mut self,
        ctx: &ToolContext<'_>,
        out: &mut ToolOutput,
        grab: Point2,
        to: Point2,
        copy: bool,
    ) {
        let target = self.move_target(ctx, grab, to);
        let v = target - grab;
        if v.length() > ctx.doc.tolerance().length.as_m() && !ctx.selection.is_empty() {
            let cmd = Self::transform(ctx, TransformSpec::Move { dx: v.x, dy: v.y }, copy);
            match cmd.apply(ctx.doc) {
                Ok((_, outcome)) => {
                    out.commands.push(cmd);
                    if copy {
                        out.selection = Some(outcome.created.into_iter().collect());
                    }
                }
                Err(e) => out.warning = Some(e.message(ctx.locale)),
            }
        }
        self.state = State::Idle;
        self.fields.clear();
    }

    fn commit_grip(
        &mut self,
        ctx: &ToolContext<'_>,
        out: &mut ToolOutput,
        grip: Grip,
        origin: Point2,
        to: Point2,
    ) {
        let target =
            typed_target(&self.fields, Some(Self::grip_anchor(ctx, grip, origin)), Some(to), ctx.doc)
                .unwrap_or(to);
        if !target.coincides(origin, &ctx.doc.tolerance()) {
            let cmds = Self::grip_commands(ctx, grip, origin, target);
            let cmds = one_step(cmds, tr(ctx, "tools.history.handle"));
            let all = Batch { label: String::new(), commands: cmds.clone() };
            match all.apply(ctx.doc) {
                Ok(_) => out.commands.extend(cmds),
                Err(e) => out.warning = Some(e.message(ctx.locale)),
            }
        }
        self.state = State::Idle;
        self.fields.clear();
    }

    fn rotation_degrees(&self, centre: Point2, reference: Point2, target: Point2) -> f64 {
        if let Some(d) = self.fields.angle(Field::Rotation) {
            return d;
        }
        let a = reference - centre;
        let b = target - centre;
        a.angle_to(b).as_deg()
    }

    fn flip_openings(ctx: &ToolContext<'_>, out: &mut ToolOutput, hand: bool) {
        let mut cmds = Vec::new();
        for id in ctx.selection {
            if let Some(ElementKind::Opening(o)) = ctx.doc.element(*id).map(|e| &e.kind) {
                let mut o2 = o.clone();
                if hand {
                    o2.flip_hand = !o2.flip_hand;
                } else {
                    o2.flip_side = !o2.flip_side;
                }
                cmds.push(update(*id, ElementKind::Opening(o2)));
            }
        }
        out.commands.extend(one_step(cmds, tr(ctx, "tools.history.flip")));
    }

    fn select_box(ctx: &ToolContext<'_>, out: &mut ToolOutput, from: Point2, to: Point2, mods: Modifiers) {
        let Some(b) = Bbox2::from_points([from, to]) else { return };
        let r = Self::radius(ctx);
        if b.width().as_m() <= r && b.height().as_m() <= r {
            return;
        }
        let crossing = to.x < from.x;
        let found = Snapper::cached(ctx.doc, ctx.storey).elements_in_box(b, crossing);
        let mut sel = if mods.shift || mods.ctrl { ctx.selection.clone() } else { BTreeSet::new() };
        for id in found {
            if mods.ctrl && sel.contains(&id) {
                sel.remove(&id);
            } else {
                sel.insert(id);
            }
        }
        out.selection = Some(sel);
    }

    fn press(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, at: Point2, mods: Modifiers) {
        let tol = ctx.doc.tolerance();
        match self.state.clone() {
            State::Rotate { centre: None, .. } => {
                let s = self.pointer.update(ctx, at, mods, None);
                self.state = State::Rotate { centre: Some(s.point), reference: None };
                self.fields.clear();
            }
            State::Rotate { centre: Some(c), reference: None } => {
                let s = self.pointer.update(ctx, at, mods, Some(c));
                if self.fields.angle(Field::Rotation).is_some() {
                    self.finish_rotate(ctx, out, c, s.point, s.point, mods.ctrl);
                } else if !s.point.coincides(c, &tol) {
                    self.state = State::Rotate { centre: Some(c), reference: Some(s.point) };
                }
            }
            State::Rotate { centre: Some(c), reference: Some(r) } => {
                let s = self.pointer.update(ctx, at, mods, Some(c));
                self.finish_rotate(ctx, out, c, r, s.point, mods.ctrl);
            }
            State::Mirror { a: None } => {
                let s = self.pointer.update(ctx, at, mods, None);
                self.state = State::Mirror { a: Some(s.point) };
            }
            State::Mirror { a: Some(a) } => {
                let s = self.pointer.update(ctx, at, mods, Some(a));
                if !s.point.coincides(a, &tol) {
                    let cmd = Self::transform(ctx, TransformSpec::Mirror { a, b: s.point }, mods.ctrl);
                    self.finish_transform(ctx, out, cmd, mods.ctrl);
                }
            }
            State::Move { grab, to, waiting: true } => {
                self.commit_move(ctx, out, grab, to, mods.ctrl);
            }
            _ => {
                // Handles take plain presses; with Shift or Ctrl the press changes the selection instead.
                if !mods.shift
                    && !mods.ctrl
                    && let Some((h, grip)) = Self::handle_at(ctx, at)
                {
                    self.state = State::Grip { grip, origin: h.at, to: h.at };
                    return;
                }
                let r = Self::radius(ctx);
                let grab = self.pointer.update(ctx, at, Modifiers::default(), None).point;
                match hit_test(ctx.doc, ctx.storey, at, r).first().copied() {
                    Some(id) if ctx.selection.contains(&id) => {
                        let click = if mods.ctrl {
                            ClickAction::Toggle
                        } else if !mods.shift && ctx.selection.len() > 1 {
                            ClickAction::Narrow
                        } else {
                            ClickAction::Keep
                        };
                        self.state = State::Pressed { at, grab, element: Some(id), click };
                    }
                    Some(id) => {
                        let mut sel =
                            if mods.shift || mods.ctrl { ctx.selection.clone() } else { BTreeSet::new() };
                        sel.insert(id);
                        out.selection = Some(sel);
                        self.state = State::Pressed { at, grab, element: Some(id), click: ClickAction::Keep };
                    }
                    None => {
                        if !mods.shift && !mods.ctrl {
                            out.selection = Some(BTreeSet::new());
                        }
                        self.state = State::Box { from: at, to: at };
                    }
                }
            }
        }
    }

    fn finish_rotate(
        &mut self,
        ctx: &ToolContext<'_>,
        out: &mut ToolOutput,
        c: Point2,
        r: Point2,
        target: Point2,
        copy: bool,
    ) {
        let degrees = self.rotation_degrees(c, r, target);
        if degrees.abs() > 1e-9 {
            let cmd = Self::transform(ctx, TransformSpec::Rotate { centre: c, degrees }, copy);
            self.finish_transform(ctx, out, cmd, copy);
        }
    }

    fn finish_transform(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, cmd: AnyCommand, copy: bool) {
        match cmd.apply(ctx.doc) {
            Ok((_, outcome)) => {
                out.commands.push(cmd);
                if copy {
                    out.selection = Some(outcome.created.into_iter().collect());
                }
                self.state = State::Idle;
                self.fields.clear();
                out.finished = true;
            }
            Err(e) => out.warning = Some(e.message(ctx.locale)),
        }
    }

    fn release(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, at: Point2, mods: Modifiers) {
        match self.state.clone() {
            State::Pressed { element: Some(id), click, .. } => {
                match click {
                    ClickAction::Narrow => out.selection = Some(std::iter::once(id).collect()),
                    ClickAction::Toggle => {
                        let mut sel = ctx.selection.clone();
                        sel.remove(&id);
                        out.selection = Some(sel);
                    }
                    ClickAction::Keep => {}
                }
                self.state = State::Idle;
            }
            State::Pressed { .. } => self.state = State::Idle,
            State::Box { from, .. } => {
                Self::select_box(ctx, out, from, at, mods);
                self.state = State::Idle;
            }
            State::Move { grab, to, waiting: false } => {
                if self.fields.is_empty() {
                    self.commit_move(ctx, out, grab, to, mods.ctrl);
                } else {
                    self.state = State::Move { grab, to, waiting: true };
                }
            }
            State::Grip { grip, origin, to } if self.fields.is_empty() => {
                self.commit_grip(ctx, out, grip, origin, to);
            }
            _ => {}
        }
    }

    fn pointer_moved(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, at: Point2, mods: Modifiers) {
        let r = Self::radius(ctx);
        match self.state.clone() {
            State::Idle => {
                self.pointer.raw = Some(at);
                self.pointer.snap = None;
                if Self::handle_at(ctx, at).is_some() {
                    out.cursor = CursorHint::Grab;
                } else if let Some(id) = hit_test(ctx.doc, ctx.storey, at, r).first().copied() {
                    out.highlight = vec![id];
                    out.cursor =
                        if ctx.selection.contains(&id) { CursorHint::Move } else { CursorHint::Arrow };
                }
            }
            State::Pressed { at: start, grab, element, .. } => {
                if start.distance(at).as_m() > r * 0.7 {
                    self.state = match element {
                        Some(_) => State::Move { grab, to: grab, waiting: false },
                        None => State::Box { from: start, to: at },
                    };
                    self.pointer_moved(ctx, out, at, mods);
                }
            }
            State::Box { from, .. } => self.state = State::Box { from, to: at },
            State::Move { grab, waiting, .. } => {
                let s = self.pointer.update_excluding(ctx, at, mods, Some(grab), ctx.selection);
                self.state = State::Move { grab, to: s.point, waiting };
            }
            State::Grip { grip, origin, .. } => {
                let mut exclude: BTreeSet<Id> =
                    Self::touched(&Self::grip_commands(ctx, grip, origin, origin));
                exclude.insert(grip.element());
                let anchor = Self::grip_anchor(ctx, grip, origin);
                let s = self.pointer.update_excluding(ctx, at, mods, Some(anchor), &exclude);
                self.state = State::Grip { grip, origin, to: s.point };
            }
            State::Rotate { centre, .. } => {
                self.pointer.update(ctx, at, mods, centre);
            }
            State::Mirror { a } => {
                self.pointer.update(ctx, at, mods, a);
            }
        }
    }

    fn draw(&self, ctx: &ToolContext<'_>, out: &mut ToolOutput, mods: Modifiers) {
        out.preview = preview::drawing(ctx);
        let cursor = self.pointer.point();
        match &self.state {
            State::Box { from, to } => {
                let crossing = to.x < from.x;
                box_outline(
                    &mut out.preview,
                    *from,
                    *to,
                    if crossing { Rgba::rgb(40, 160, 90) } else { BLUE },
                    crossing,
                );
                out.prompt = tr(ctx, if crossing { "tools.select.crossing" } else { "tools.select.window" });
            }
            State::Move { grab, to, waiting } => {
                let target = self.move_target(ctx, *grab, *to);
                let v = target - *grab;
                preview::rubber(&mut out.preview, *grab, target);
                preview::length_label(&mut out.preview, *grab, target, ctx.locale);
                let cmd = Self::transform(ctx, TransformSpec::Move { dx: v.x, dy: v.y }, mods.ctrl);
                if v.length() > 1e-9 {
                    Self::preview_command(ctx, out, &cmd);
                }
                let live = vec![
                    length_text(kanavos_units::Length::m(v.length()), ctx.locale),
                    angle_text(v.angle().as_deg(), ctx.locale),
                ];
                out.fields = self.fields.view(&live, ctx.locale);
                out.prompt = tr(
                    ctx,
                    match (waiting, mods.ctrl) {
                        (true, _) => "tools.select.move_enter",
                        (false, true) => "tools.select.copying",
                        (false, false) => "tools.select.moving",
                    },
                );
                out.cursor = CursorHint::Move;
            }
            State::Grip { grip, origin, to } => {
                let anchor = Self::grip_anchor(ctx, *grip, *origin);
                let target = typed_target(&self.fields, Some(anchor), Some(*to), ctx.doc).unwrap_or(*to);
                let cmds = Self::grip_commands(ctx, *grip, *origin, target);
                if !target.coincides(*origin, &ctx.doc.tolerance()) {
                    let batch: AnyCommand = Batch { label: String::new(), commands: cmds }.into();
                    Self::preview_command(ctx, out, &batch);
                }
                preview::length_label(&mut out.preview, anchor, target, ctx.locale);
                out.fields = self.fields.view(
                    &[
                        length_text(anchor.distance(target), ctx.locale),
                        angle_text((target - anchor).angle().as_deg(), ctx.locale),
                    ],
                    ctx.locale,
                );
                out.prompt = tr(ctx, "tools.select.handle");
                out.cursor = CursorHint::Grab;
            }
            State::Rotate { centre, reference } => {
                out.prompt = tr(
                    ctx,
                    match (centre, reference) {
                        (None, _) => "tools.select.rotate_centre",
                        (Some(_), None) => "tools.select.rotate_reference",
                        _ => "tools.select.rotate_target",
                    },
                );
                if let (Some(c), Some(p)) = (centre, cursor) {
                    preview::rubber(&mut out.preview, *c, p);
                    let r = reference.unwrap_or(p);
                    let typed = self.fields.angle(Field::Rotation).is_some();
                    if reference.is_some() || typed {
                        let deg = self.rotation_degrees(*c, r, p);
                        if let Some(r) = reference {
                            preview::line(&mut out.preview, *c, *r, BLUE);
                        }
                        let cmd = Self::transform(
                            ctx,
                            TransformSpec::Rotate { centre: *c, degrees: deg },
                            mods.ctrl,
                        );
                        Self::preview_command(ctx, out, &cmd);
                        out.fields = self.fields.view(&[angle_text(deg, ctx.locale)], ctx.locale);
                    } else {
                        out.fields = self.fields.view(&[String::new()], ctx.locale);
                    }
                }
            }
            State::Mirror { a } => {
                out.prompt =
                    tr(ctx, if a.is_some() { "tools.select.mirror_b" } else { "tools.select.mirror_a" });
                if let (Some(a), Some(p)) = (a, cursor)
                    && !a.coincides(p, &ctx.doc.tolerance())
                {
                    preview::line(&mut out.preview, *a, p, RED);
                    let cmd = Self::transform(ctx, TransformSpec::Mirror { a: *a, b: p }, mods.ctrl);
                    Self::preview_command(ctx, out, &cmd);
                }
            }
            State::Idle | State::Pressed { .. } => {
                out.prompt = if ctx.selection.is_empty() {
                    tr(ctx, "tools.select.prompt")
                } else {
                    tf(ctx.locale, "tools.select.selected", &[("n", &ctx.selection.len().to_string())])
                };
            }
        }
        // Handles of the selection, the one under the pointer highlighted.
        let hot = match (&self.state, self.pointer.raw) {
            (State::Idle, Some(raw)) => Self::handle_at(ctx, raw).map(|(h, _)| h),
            _ => None,
        };
        let dragging = match &self.state {
            State::Grip { grip, .. } => Some(*grip),
            _ => None,
        };
        if !matches!(self.state, State::Move { .. }) {
            out.handles = Self::handles(ctx)
                .into_iter()
                .filter(|(_, g)| dragging.is_none_or(|d| d == *g))
                .map(|(mut h, _)| {
                    h.hot = hot.is_some_and(|x| {
                        x.at.coincides(h.at, &ctx.doc.tolerance()) && x.element == h.element
                    });
                    h
                })
                .collect();
        }
    }
}

fn box_outline(d: &mut kanavos_drawing::Drawing, a: Point2, b: Point2, color: Rgba, dashed: bool) {
    let [x0, y0] = a.xy();
    let [x1, y1] = b.xy();
    let stroke = if dashed {
        Stroke::patterned(weights::FINE, LinePattern::Dashed).with_color(color)
    } else {
        Stroke::solid(weights::FINE).with_color(color)
    };
    d.push(Item::polygon(
        vec![vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)]],
        Some(Fill::Solid(Rgba::new(color.r, color.g, color.b, 25))),
        Some(stroke),
        "tool.box",
    ));
}

impl Tool for SelectTool {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let fields: &[Field] = match &self.state {
            State::Move { .. } | State::Grip { .. } => &[Field::Length, Field::Angle],
            State::Rotate { centre: Some(_), .. } => &[Field::Rotation],
            _ => &[],
        };
        self.fields.set(fields);
        let mut out = ToolOutput { cursor: CursorHint::Arrow, ..ToolOutput::default() };
        let mut mods = Modifiers::default();
        match event {
            ToolEvent::Press { at, button: MouseButton::Primary, mods: m } => {
                mods = *m;
                self.press(ctx, &mut out, *at, *m);
            }
            ToolEvent::Move { at, mods: m } => {
                mods = *m;
                self.pointer_moved(ctx, &mut out, *at, *m);
            }
            ToolEvent::Release { at, button: MouseButton::Primary, mods: m } => {
                mods = *m;
                self.release(ctx, &mut out, *at, *m);
            }
            ToolEvent::Text(s) => {
                if !fields.is_empty() {
                    self.fields.type_text(s);
                }
            }
            ToolEvent::Key { key, mods: m } => {
                mods = *m;
                match key {
                    Key::Delete if !ctx.selection.is_empty() && matches!(self.state, State::Idle) => {
                        out.commands
                            .push(DeleteElements { ids: ctx.selection.iter().copied().collect() }.into());
                        out.selection = Some(BTreeSet::new());
                    }
                    Key::Escape => {
                        if !self.fields.is_empty() {
                            self.fields.clear();
                        } else if matches!(self.state, State::Idle) {
                            out.selection = Some(BTreeSet::new());
                        } else {
                            self.state = State::Idle;
                        }
                    }
                    Key::Rotate if !ctx.selection.is_empty() => {
                        self.state = State::Rotate { centre: None, reference: None };
                        self.fields.clear();
                    }
                    Key::Mirror if !ctx.selection.is_empty() => {
                        self.state = State::Mirror { a: None };
                        self.fields.clear();
                    }
                    Key::Flip if matches!(self.state, State::Idle) => {
                        Self::flip_openings(ctx, &mut out, m.shift)
                    }
                    Key::Tab => self.fields.tab(),
                    Key::Backspace => {
                        self.fields.backspace();
                    }
                    Key::Enter => match self.state.clone() {
                        State::Move { grab, to, .. } => self.commit_move(ctx, &mut out, grab, to, m.ctrl),
                        State::Grip { grip, origin, to } => self.commit_grip(ctx, &mut out, grip, origin, to),
                        State::Rotate { centre: Some(c), reference }
                            if self.fields.angle(Field::Rotation).is_some() =>
                        {
                            let r = reference.unwrap_or(c);
                            self.finish_rotate(ctx, &mut out, c, r, r, m.ctrl);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            _ => {}
        }
        let fields: &[Field] = match &self.state {
            State::Move { .. } | State::Grip { .. } => &[Field::Length, Field::Angle],
            State::Rotate { centre: Some(_), .. } => &[Field::Rotation],
            _ => &[],
        };
        self.fields.set(fields);
        let warning = out.warning.take();
        self.draw(ctx, &mut out, mods);
        if warning.is_some() {
            out.warning = warning;
        }
        if !matches!(self.state, State::Idle | State::Pressed { .. } | State::Box { .. }) {
            out.snap = self.pointer.snap.clone();
            out.tracked = self.pointer.tracked();
        }
        if self.fields.has_error() {
            out.warning = Some(t(ctx.locale, "tools.input.unreadable").into_owned());
        }
        out.typing = self.fields.is_shown();
        out
    }

    fn cancel(&mut self) {
        self.state = State::Idle;
        self.fields.clear();
    }

    fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }
}
