//! Stairs in plans (see `specs/views/plan-stairs.md`).
//!
//! On the storey where a stair starts, treads below the cut plane are solid and the rest dashed, separated by a
//! break line at 45° to the flight with a zig-zag in its middle. The walking line starts with a dot, carries an
//! arrow where it meets the break line and the label "ΑΝ" (up) at its start; a note gives risers × riser height /
//! going in centimetres. On the storey above, the part of the stair above the lower storey's cut plane is drawn
//! solid with the same break line, and the walking line points down to it with the label "ΚΑΤ" (down).

use kanavos_drawing::{Anchor, Drawing, Fill, Item, LinePattern, Rgba, Shape, Stroke, Text};
use kanavos_elements::{StairLayout, stair_layout};
use kanavos_geom::{Point2, Polygon2, Segment2, Vec2};
use kanavos_i18n::t;
use kanavos_model::{Document, Element, Stair, StairShape};
use kanavos_units::format_decimal;

use super::p;
use crate::annot::{Ctx, arrow_head};
use crate::style::phase_stroke;

/// Where the cut plane crosses a stair.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Break {
    /// A point on the break line (middle of the first riser above the cut).
    at: Point2,
    /// Unit normal of the break line, pointing up the stair.
    normal: Vec2,
    /// Walking direction of the flight at the break.
    dir: Vec2,
    /// Across the flight.
    across: Vec2,
    /// Index of the flight the break crosses.
    flight: usize,
}

impl Break {
    fn side(&self, q: Point2) -> f64 {
        (q - self.at).dot(self.normal)
    }
    /// The parts of a segment before (lower) and after (upper) the break line.
    fn split(&self, s: &Segment2) -> (Option<Segment2>, Option<Segment2>) {
        let (fa, fb) = (self.side(s.start), self.side(s.end));
        if fa <= 0.0 && fb <= 0.0 {
            return (Some(*s), None);
        }
        if fa >= 0.0 && fb >= 0.0 {
            return (None, Some(*s));
        }
        let x = s.at(fa / (fa - fb));
        if fa < 0.0 {
            (Some(Segment2::new(s.start, x)), Some(Segment2::new(x, s.end)))
        } else {
            (Some(Segment2::new(x, s.end)), Some(Segment2::new(s.start, x)))
        }
    }
}

fn v(poly: &Polygon2, i: usize) -> Option<Point2> {
    poly.vertices.get(i).copied()
}

fn break_of(layout: &StairLayout, s: &Stair, cut: f64) -> Option<Break> {
    let k = layout.tread_elevations.iter().filter(|z| **z <= cut).count();
    if k >= layout.tread_elevations.len() {
        return None;
    }
    let (at, dir, across, flight) = layout
        .flights
        .iter()
        .enumerate()
        .find_map(|(fi, f)| {
            let j = k.checked_sub(f.first_riser)?;
            let tread = f.treads.get(j)?;
            let (v0, v1, v3) = (v(tread, 0)?, v(tread, 1)?, v(tread, 3)?);
            Some((v0.midpoint(v3), (v1 - v0).normalized()?, (v3 - v0).normalized()?, fi))
        })
        .or_else(|| {
            // The landing is the first step above the cut: break at the end of the first flight.
            let f = layout.flights.first()?;
            let along = Vec2::from_angle(s.direction);
            match f.treads.last() {
                Some(last) => {
                    let (v0, v1, v2, v3) = (v(last, 0)?, v(last, 1)?, v(last, 2)?, v(last, 3)?);
                    Some((v1.midpoint(v2), (v1 - v0).normalized()?, (v3 - v0).normalized()?, 0))
                }
                None => Some((s.origin, along, along.perp(), 0)),
            }
        })?;
    let l = (dir + across).normalized()?;
    let mut normal = l.perp();
    if normal.dot(dir) < 0.0 {
        normal = -normal;
    }
    Some(Break { at, normal, dir, across, flight })
}

/// Lines of one flight: every riser, the last tread's back edge and the two sides.
fn flight_lines(f: &kanavos_elements::StairFlight) -> Vec<Segment2> {
    let mut out = Vec::new();
    for tread in &f.treads {
        if let (Some(a), Some(b)) = (v(tread, 0), v(tread, 3)) {
            out.push(Segment2::new(a, b));
        }
    }
    if let (Some(first), Some(last)) = (f.treads.first(), f.treads.last()) {
        if let (Some(a), Some(b)) = (v(last, 1), v(last, 2)) {
            out.push(Segment2::new(a, b));
        }
        if let (Some(a), Some(b)) = (v(first, 0), v(last, 1)) {
            out.push(Segment2::new(a, b));
        }
        if let (Some(a), Some(b)) = (v(first, 3), v(last, 2)) {
            out.push(Segment2::new(a, b));
        }
    }
    out
}

/// The walking line as segments with an order key: flight `i` has key `2·i`, the way across landings between
/// flights `i` and `i + 1` has key `2·i + 1`. It runs along the middle of each flight and through the landing
/// centres.
fn walking_line(layout: &StairLayout) -> Vec<(Segment2, usize)> {
    let mut out = Vec::new();
    let mut prev_end: Option<Point2> = None;
    let mut landing = 0;
    for (fi, f) in layout.flights.iter().enumerate() {
        let (Some(first), Some(last)) = (f.treads.first(), f.treads.last()) else { continue };
        let (Some(a0), Some(a3), Some(b1), Some(b2)) = (v(first, 0), v(first, 3), v(last, 1), v(last, 2))
        else {
            continue;
        };
        let (start, end) = (a0.midpoint(a3), b1.midpoint(b2));
        if let Some(pe) = prev_end {
            // Through the landings between the previous flight and this one.
            let mut pts = vec![pe];
            let n_landings = if layout.landings.len() >= 2 && fi == 1 { 2 } else { 1 };
            for _ in 0..n_landings {
                if let Some(l) = layout.landings.get(landing) {
                    if let Some(c) = l.centroid() {
                        pts.push(c);
                    }
                    landing += 1;
                }
            }
            pts.push(start);
            for w in pts.windows(2) {
                if let [a, b] = w
                    && a.distance(*b).as_m() > 1e-6
                {
                    out.push((Segment2::new(*a, *b), 2 * fi - 1));
                }
            }
        }
        if start.distance(end).as_m() > 1e-6 {
            out.push((Segment2::new(start, end), 2 * fi));
        }
        prev_end = Some(end);
    }
    out
}

fn push_line(d: &mut Drawing, s: &Segment2, stroke: &Stroke, layer: &str, src: u64) {
    d.push(Item::line(p(s.start), p(s.end), stroke.clone(), layer).with_source(src));
}

/// The break symbol: a line at 45° across the flight with a zig-zag in the middle.
fn break_symbol(d: &mut Drawing, ctx: &Ctx, b: &Break, width: f64, src: u64) {
    let ext = ctx.mm(2.0);
    let half = width / 2.0 + ext;
    let p1 = b.at - b.across * half - b.dir * half;
    let p2 = b.at + b.across * half + b.dir * half;
    let l = (p2 - p1).normalized().unwrap_or(b.dir);
    let len = p1.distance(p2).as_m();
    // The zig-zag sits off-centre so the walking line's arrow does not hide it.
    let z = ctx.mm(2.2);
    let at = len * 0.3;
    let m1 = p1 + l * (at - z * 0.7);
    let m2 = p1 + l * (at + z * 0.7);
    let k1 = p1 + l * (at - z * 0.25) + l.perp() * z;
    let k2 = p1 + l * (at + z * 0.25) - l.perp() * z;
    d.push(
        Item::polyline(
            vec![p(p1), p(m1), p(k1), p(k2), p(m2), p(p2)],
            false,
            Stroke::solid(ctx.pens.symbol),
            "stair.break",
        )
        .with_source(src),
    );
}

fn riser_note(ctx: &Ctx, s: &Stair) -> String {
    let cm = |v: f64| {
        if (v - v.round()).abs() < 0.05 {
            format!("{}", v.round() as i64)
        } else {
            format_decimal(v, 1, ctx.locale)
        }
    };
    format!("{}×{}/{}", s.risers, cm(s.riser_height().as_cm()), cm(s.going.as_cm()))
}

fn label(d: &mut Drawing, at: Point2, text: impl Into<String>, size: f64, bold: bool, angle: f64, src: u64) {
    let mut tx = Text::new(p(at), text, size).anchored(Anchor::Centre).rotated(angle);
    tx.bold = bold;
    d.push(Item::text(tx, "stair.label").with_source(src));
}

fn dot_at(d: &mut Drawing, ctx: &Ctx, at: Point2, src: u64) {
    d.push(
        Item {
            shape: Shape::Circle { centre: p(at), radius: ctx.mm(0.8) },
            stroke: Some(Stroke::solid(ctx.pens.symbol)),
            fill: Some(Fill::Solid(Rgba::BLACK)),
            layer: "stair.line".into(),
            source: None,
        }
        .with_source(src),
    );
}

/// A stair on the plan's own storey, going up. Lines go to `d`; labels go to `labels`, which the plan paints
/// after the cut areas so walls never hide them.
#[allow(clippy::too_many_arguments)]
pub(super) fn stair_up(
    d: &mut Drawing,
    labels: &mut Drawing,
    ctx: &Ctx,
    doc: &Document,
    e: &Element,
    s: &Stair,
    cut: f64,
    above: bool,
) {
    let Ok(layout) = stair_layout(doc, e.id) else { return };
    let src = e.id.0;
    let brk = break_of(&layout, s, cut);
    let solid = phase_stroke(Stroke::solid(ctx.pens.below), e.phase);
    let dashed = phase_stroke(Stroke::patterned(ctx.pens.above, LinePattern::Dashed), e.phase);
    for (fi, f) in layout.flights.iter().enumerate() {
        for l in flight_lines(f) {
            match &brk {
                None => push_line(d, &l, &solid, "stair.below", src),
                Some(b) if b.flight == fi => {
                    let (lo, hi) = b.split(&l);
                    if let Some(lo) = lo {
                        push_line(d, &lo, &solid, "stair.below", src);
                    }
                    if let (Some(hi), true) = (hi, above) {
                        push_line(d, &hi, &dashed, "stair.above", src);
                    }
                }
                Some(b) if fi < b.flight => push_line(d, &l, &solid, "stair.below", src),
                Some(_) if above => push_line(d, &l, &dashed, "stair.above", src),
                Some(_) => {}
            }
        }
    }
    for (l, z) in layout.landings.iter().zip(&layout.landing_elevations) {
        let below = *z <= cut;
        if !below && !above {
            continue;
        }
        for edge in l.edges() {
            if below {
                push_line(d, &edge, &solid, "stair.below", src);
            } else {
                push_line(d, &edge, &dashed, "stair.above", src);
            }
        }
    }
    if let Some(b) = &brk {
        break_symbol(d, ctx, b, s.width.as_m(), src);
    }
    // Walking line: solid up to the break, with the arrow there; dashed beyond.
    let walk = Stroke::solid(ctx.pens.symbol);
    let walk_above = Stroke::patterned(ctx.pens.above, LinePattern::Dashed);
    let segs = walking_line(&layout);
    let key = brk.map(|b| 2 * b.flight);
    let mut arrow: Option<(Point2, Vec2)> = None;
    let mut note_span: Option<Segment2> = None;
    for (seg, k) in &segs {
        match key {
            Some(bk) if *k == bk => {
                let Some(b) = &brk else { continue };
                let (lo, hi) = b.split(seg);
                if let Some(lo) = lo {
                    push_line(d, &lo, &walk, "stair.line", src);
                    arrow = seg.direction().map(|dir| (lo.end, dir));
                    if *k == 0 {
                        note_span = Some(lo);
                    }
                }
                if let (Some(hi), true) = (hi, above) {
                    push_line(d, &hi, &walk_above, "stair.line", src);
                }
            }
            Some(bk) if *k > bk => {
                if above {
                    push_line(d, seg, &walk_above, "stair.line", src);
                }
            }
            _ => {
                push_line(d, seg, &walk, "stair.line", src);
                arrow = seg.direction().map(|dir| (seg.end, dir));
                if *k == 0 {
                    note_span = Some(*seg);
                }
            }
        }
    }
    if let Some((tip, dir)) = arrow {
        d.push(arrow_head(tip, dir, ctx.mm(2.5), "stair.line").with_source(src));
    }
    if let Some((first, _)) = segs.first() {
        dot_at(d, ctx, first.start, src);
        let dir = first.direction().unwrap_or(Vec2::from_angle(s.direction));
        labels_up(labels, ctx, s, first.start, dir, note_span, src);
    }
}

/// «ΑΝ» beside the start of the walking line and the riser note along the first flight.
fn labels_up(
    labels: &mut Drawing,
    ctx: &Ctx,
    s: &Stair,
    start: Point2,
    dir: Vec2,
    span: Option<Segment2>,
    src: u64,
) {
    // The note goes beside the walking line on the side away from a second flight (stairs turning left put it on
    // the right); «ΑΝ» goes beside the start dot on the other side, just past the first riser.
    let side = if matches!(s.shape, StairShape::QuarterTurnRight | StairShape::HalfTurnRight) {
        dir.perp()
    } else {
        -dir.perp()
    };
    let up_at = start + dir * ctx.mm(2.0) - side * ctx.mm(3.5);
    label(labels, up_at, t(ctx.locale, "views.stair.up"), 2.5, true, 0.0, src);
    let Some(span) = span else { return };
    let at = span.midpoint() + side * (s.width.as_m() * 0.25);
    let (angle, _) = crate::annot::readable(dir.angle().as_rad());
    label(labels, at, riser_note(ctx, s), 2.0, false, angle, src);
}

/// A stair from the storey below that arrives on the plan's storey, going down.
pub(super) fn stair_down(
    d: &mut Drawing,
    labels: &mut Drawing,
    ctx: &Ctx,
    doc: &Document,
    e: &Element,
    s: &Stair,
    lower_cut: f64,
) {
    let Ok(layout) = stair_layout(doc, e.id) else { return };
    let Some(b) = break_of(&layout, s, lower_cut) else { return };
    let src = e.id.0;
    let solid = phase_stroke(Stroke::solid(ctx.pens.below), e.phase);
    for (fi, f) in layout.flights.iter().enumerate() {
        if fi < b.flight {
            continue;
        }
        for l in flight_lines(f) {
            let seg = if fi == b.flight { b.split(&l).1 } else { Some(l) };
            if let Some(seg) = seg {
                push_line(d, &seg, &solid, "stair.below", src);
            }
        }
    }
    for (l, z) in layout.landings.iter().zip(&layout.landing_elevations) {
        if *z > lower_cut {
            for edge in l.edges() {
                push_line(d, &edge, &solid, "stair.below", src);
            }
        }
    }
    break_symbol(d, ctx, &b, s.width.as_m(), src);
    // Walking line from the top down to the break, with the arrow at the break pointing down the stair.
    let walk = Stroke::solid(ctx.pens.symbol);
    let segs = walking_line(&layout);
    let bk = 2 * b.flight;
    for (seg, k) in &segs {
        if *k < bk {
            continue;
        }
        let seg = if *k == bk { b.split(seg).1 } else { Some(*seg) };
        let Some(seg) = seg else { continue };
        push_line(d, &seg, &walk, "stair.line", src);
        if *k == bk
            && let Some(dir) = seg.direction()
        {
            d.push(arrow_head(seg.start, -dir, ctx.mm(2.5), "stair.line").with_source(src));
        }
    }
    if let Some((last, _)) = segs.last()
        && let Some(dir) = last.direction()
    {
        dot_at(d, ctx, last.end, src);
        label(labels, last.end + dir * ctx.mm(3.5), t(ctx.locale, "views.stair.down"), 2.5, true, 0.0, src);
    }
}
