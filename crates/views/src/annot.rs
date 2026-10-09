//! Annotation graphics shared by plans, sections and elevations: dimension chains, level marks, grid bubbles,
//! text notes, drafting items, section and elevation markers and the north arrow.
//!
//! Sizes are paper millimetres converted to model metres with the drawing scale. Conventions follow ISO 129-1
//! (dimensioning: oblique ticks, text above the line, extension lines with a gap) and ISO 7519 (level indication
//! in plans and sections), as used in Greek building-permit drawings (see `specs/views/annotation.md`).

use std::f64::consts::{FRAC_PI_2, PI};

use kanavos_drawing::{Anchor, Drawing, Fill, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt};
use kanavos_geom::{Point2, Vec2};
use kanavos_model::{Dimension, DraftShape, Drafting, GridLine, TextAlign, TextNote};
use kanavos_units::{Length, Locale, format_decimal, format_level};

use crate::style::Pens;

/// Drawing context: scale, language and pens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ctx {
    /// Scale denominator.
    pub scale: f64,
    /// Language of generated text.
    pub locale: Locale,
    /// Pens for the scale.
    pub pens: Pens,
}

impl Ctx {
    pub(crate) fn new(scale: f64, locale: Locale) -> Ctx {
        Ctx { scale, locale, pens: Pens::for_scale(scale) }
    }
    /// Model metres for paper millimetres.
    pub(crate) fn mm(&self, v: f64) -> f64 {
        v * self.scale / 1000.0
    }
}

pub(crate) fn p(q: Point2) -> Pt {
    let [x, y] = q.xy();
    pt(x, y)
}

fn push(d: &mut Drawing, item: Item, source: Option<u64>) {
    d.push(match source {
        Some(s) => item.with_source(s),
        None => item,
    });
}

/// Dimension text size, paper millimetres.
pub(crate) const DIM_TEXT: f64 = 2.0;

/// A text angle that reads from the bottom or from the right, and the matching "up" vector.
pub(crate) fn readable(angle: f64) -> (f64, Vec2) {
    let mut a = angle;
    while a > PI {
        a -= 2.0 * PI;
    }
    while a <= -PI {
        a += 2.0 * PI;
    }
    if a > FRAC_PI_2 + 1e-9 {
        a -= PI;
    } else if a <= -FRAC_PI_2 + 1e-9 {
        a += PI;
    }
    (a, Vec2::new(-a.sin(), a.cos()))
}

/// Draws a linear dimension chain.
pub(crate) fn dimension(d: &mut Drawing, ctx: &Ctx, dim: &Dimension, source: Option<u64>) {
    let u = Vec2::from_angle(dim.direction);
    let n = u.perp();
    let base = dim.line_point;
    let mut pts: Vec<(f64, Point2, Point2)> = dim
        .points
        .iter()
        .map(|q| {
            let t = (*q - base).dot(u);
            (t, base + u * t, *q)
        })
        .collect();
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    if pts.len() < 2 {
        return;
    }
    let line = Stroke::solid(ctx.pens.annotation);
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else { return };
    let over = ctx.mm(2.0);
    push(d, Item::line(p(first.1 - u * over), p(last.1 + u * over), line.clone(), "dimension.line"), source);
    for (_, on, q) in &pts {
        let off = *on - *q;
        let dist = off.dot(n);
        if dist.abs() > ctx.mm(1.5) {
            let s = if dist > 0.0 { n } else { -n };
            push(
                d,
                Item::line(
                    p(*q + s * ctx.mm(1.5)),
                    p(*on + s * ctx.mm(2.0)),
                    line.clone(),
                    "dimension.extension",
                ),
                source,
            );
        }
        let t = (u + n).normalized().unwrap_or(u) * ctx.mm(1.25);
        push(d, Item::line(p(*on - t), p(*on + t), Stroke::solid(ctx.pens.tick), "dimension.tick"), source);
    }
    let (angle, up) = readable(dim.direction.as_rad());
    let mut stagger = false;
    for w in pts.windows(2) {
        let [a, b] = w else { continue };
        let len = b.0 - a.0;
        let text = format_decimal(len, 2, ctx.locale);
        let width_mm = text.chars().count() as f64 * DIM_TEXT * 0.6;
        let fits = len * 1000.0 / ctx.scale >= width_mm + 1.0;
        let lift = if fits {
            stagger = false;
            ctx.mm(0.8)
        } else {
            stagger = !stagger;
            if stagger { ctx.mm(0.8 + DIM_TEXT + 1.0) } else { ctx.mm(0.8) }
        };
        let mid = a.1.midpoint(b.1) + up * lift;
        push(
            d,
            Item::text(
                Text::new(p(mid), text, DIM_TEXT).anchored(Anchor::BottomCentre).rotated(angle),
                "dimension.text",
            ),
            source,
        );
    }
}

/// Draws a text note (several lines).
pub(crate) fn text_note(d: &mut Drawing, ctx: &Ctx, t: &TextNote, source: Option<u64>) {
    let anchor = match t.align {
        TextAlign::Left => Anchor::BottomLeft,
        TextAlign::Centre => Anchor::BottomCentre,
        TextAlign::Right => Anchor::BottomRight,
    };
    let down = Vec2::from_angle(t.rotation).perp() * (-ctx.mm(t.size_mm * 1.6));
    for (i, line) in t.content.lines().enumerate() {
        let at = t.position + down * i as f64;
        let mut text = Text::new(p(at), line, t.size_mm).anchored(anchor).rotated(t.rotation.as_rad());
        text.bold = t.bold;
        push(d, Item::text(text, "text"), source);
    }
}

fn quarter(c: Pt, r: f64, start: f64) -> Vec<Pt> {
    let mut v = vec![c];
    for i in 0..=8 {
        let a = start + FRAC_PI_2 * f64::from(i) / 8.0;
        v.push(pt(c.x + r * a.cos(), c.y + r * a.sin()));
    }
    v
}

/// Plan level mark: a circle with two opposite quadrants filled, and the level to its right (`+0,00`).
pub(crate) fn level_plan(
    d: &mut Drawing,
    ctx: &Ctx,
    at: Point2,
    value: Length,
    label: &str,
    source: Option<u64>,
) {
    let c = p(at);
    let r = ctx.mm(1.6);
    let stroke = Stroke::solid(ctx.pens.annotation);
    push(
        d,
        Item {
            shape: Shape::Circle { centre: c, radius: r },
            stroke: Some(stroke.clone()),
            fill: Some(Fill::Solid(Rgba::WHITE)),
            layer: "level.symbol".into(),
            source: None,
        },
        source,
    );
    for start in [0.0, PI] {
        push(
            d,
            Item::polygon(vec![quarter(c, r, start)], Some(Fill::Solid(Rgba::BLACK)), None, "level.symbol"),
            source,
        );
    }
    push(
        d,
        Item::line(c.offset(-r * 1.4, 0.0), c.offset(r * 1.4, 0.0), stroke.clone(), "level.symbol"),
        source,
    );
    push(d, Item::line(c.offset(0.0, -r * 1.4), c.offset(0.0, r * 1.4), stroke, "level.symbol"), source);
    let mut text = format_level(value, ctx.locale);
    if !label.is_empty() {
        text = format!("{label} {text}");
    }
    push(
        d,
        Item::text(
            Text::new(c.offset(r + ctx.mm(1.2), 0.0), text, 2.5).anchored(Anchor::MiddleLeft),
            "level.text",
        ),
        source,
    );
}

/// Section or elevation level mark at `(x, y)`: a filled triangle with its tip on the level, a short level line
/// and the value above it, to the right (or the left when `left`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn level_section(
    d: &mut Drawing,
    ctx: &Ctx,
    x: f64,
    y: f64,
    value: Length,
    label: &str,
    left: bool,
    source: Option<u64>,
) {
    let h = ctx.mm(2.2);
    let hw = ctx.mm(1.3);
    let stroke = Stroke::solid(ctx.pens.annotation);
    push(
        d,
        Item::polygon(
            vec![vec![pt(x, y), pt(x + hw, y + h), pt(x - hw, y + h)]],
            Some(Fill::Solid(Rgba::BLACK)),
            Some(stroke.clone()),
            "level.symbol",
        ),
        source,
    );
    let mut text = format_level(value, ctx.locale);
    if !label.is_empty() {
        text = format!("{text} {label}");
    }
    let tw = ctx.mm(text.chars().count() as f64 * 2.5 * 0.62 + 1.5);
    let sign = if left { -1.0 } else { 1.0 };
    push(
        d,
        Item::line(pt(x - sign * ctx.mm(1.5), y), pt(x + sign * (hw + tw), y), stroke, "level.symbol"),
        source,
    );
    let anchor = if left { Anchor::BottomRight } else { Anchor::BottomLeft };
    push(
        d,
        Item::text(
            Text::new(pt(x + sign * (hw + ctx.mm(0.8)), y + ctx.mm(0.8)), text, 2.5).anchored(anchor),
            "level.text",
        ),
        source,
    );
}

/// A grid line with bubbles at both ends.
pub(crate) fn grid(d: &mut Drawing, ctx: &Ctx, g: &GridLine, source: Option<u64>) {
    push(
        d,
        Item::line(
            p(g.start),
            p(g.end),
            Stroke::patterned(ctx.pens.annotation, LinePattern::DashDot),
            "grid.line",
        ),
        source,
    );
    let r = ctx.mm(4.0);
    let dir = (g.end - g.start).normalized().unwrap_or(Vec2::X);
    for (end, sign) in [(g.start, -1.0), (g.end, 1.0)] {
        let c = end + dir * (r * sign);
        push(
            d,
            Item {
                shape: Shape::Circle { centre: p(c), radius: r },
                stroke: Some(Stroke::solid(ctx.pens.symbol)),
                fill: Some(Fill::Solid(Rgba::WHITE)),
                layer: "grid.bubble".into(),
                source: None,
            },
            source,
        );
        push(
            d,
            Item::text(Text::new(p(c), g.label.clone(), 3.5).anchored(Anchor::Centre), "grid.label"),
            source,
        );
    }
}

/// A drafting item.
pub(crate) fn drafting(d: &mut Drawing, df: &Drafting, source: Option<u64>) {
    let stroke =
        Stroke { weight_mm: df.style.weight_mm, color: df.style.color, pattern: df.style.pattern.clone() };
    let item = match &df.shape {
        DraftShape::Line { a, b } => Item::line(p(*a), p(*b), stroke, "drafting"),
        DraftShape::Polyline { points, closed } => {
            Item::polyline(points.iter().map(|q| p(*q)).collect(), *closed, stroke, "drafting")
        }
        DraftShape::Arc { arc } => Item {
            shape: Shape::Arc {
                centre: p(arc.centre),
                radius: arc.radius.as_m(),
                start: arc.start.as_rad(),
                sweep: arc.sweep.as_rad(),
            },
            stroke: Some(stroke),
            fill: None,
            layer: "drafting".into(),
            source: None,
        },
        DraftShape::Circle { centre, radius } => Item {
            shape: Shape::Circle { centre: p(*centre), radius: radius.as_m() },
            stroke: Some(stroke),
            fill: None,
            layer: "drafting".into(),
            source: None,
        },
        DraftShape::Fill { region } => Item::polygon(
            crate::plan::rings(region),
            Some(Fill::Hatch {
                pattern: df.style.hatch,
                color: df.style.color,
                background: df.style.background,
                scale: 1.0,
                angle: 0.0,
            }),
            Some(stroke),
            "drafting.fill",
        ),
    };
    push(d, item, source);
}

/// A filled arrow head with its tip at `tip`, pointing along `dir`.
pub(crate) fn arrow_head(tip: Point2, dir: Vec2, length: f64, layer: &str) -> Item {
    let back = tip - dir * length;
    let side = dir.perp() * (length * 0.35);
    Item::polygon(
        vec![vec![p(tip), p(back + side), p(back - side)]],
        Some(Fill::Solid(Rgba::BLACK)),
        None,
        layer,
    )
}

/// Section marker (heavy ends, thin chain line, arrows and letters) or elevation marker (a triangle and a letter)
/// for a view line in plan.
#[allow(clippy::too_many_arguments)]
pub(crate) fn view_marker(
    d: &mut Drawing,
    ctx: &Ctx,
    start: Point2,
    end: Point2,
    marker: &str,
    look: Vec2,
    is_section: bool,
    source: u64,
) {
    let Some(u) = (end - start).normalized() else { return };
    let src = Some(source);
    if is_section {
        let layer = "section.marker";
        let len = start.distance(end).as_m();
        let stub = ctx.mm(8.0).min(len / 2.0);
        push(
            d,
            Item::line(p(start), p(end), Stroke::patterned(ctx.pens.annotation, LinePattern::DashDot), layer),
            src,
        );
        for (q, out) in [(start, -u), (end, u)] {
            let inner = q - out * stub;
            push(d, Item::line(p(q), p(inner), Stroke::solid(ctx.pens.cut), layer), src);
            let tip = q + look * ctx.mm(7.0);
            push(
                d,
                Item::line(p(q), p(tip - look * ctx.mm(1.5)), Stroke::solid(ctx.pens.symbol), layer),
                src,
            );
            push(d, arrow_head(tip, look, ctx.mm(3.0), layer), src);
            let mut t = Text::new(p(q + look * ctx.mm(4.5) + out * ctx.mm(3.5)), marker, 5.0)
                .anchored(Anchor::Centre);
            t.bold = true;
            push(d, Item::text(t, layer), src);
        }
    } else {
        let layer = "elevation.marker";
        let mid = start.midpoint(end);
        let hw = ctx.mm(3.5);
        let tip = mid + look * ctx.mm(5.0);
        push(
            d,
            Item::polygon(
                vec![vec![p(mid - u * hw), p(mid + u * hw), p(tip)]],
                Some(Fill::Solid(Rgba::BLACK)),
                None,
                layer,
            ),
            src,
        );
        let mut t = Text::new(p(mid - look * ctx.mm(4.0)), marker, 5.0).anchored(Anchor::Centre);
        t.bold = true;
        push(d, Item::text(t, layer), src);
    }
}

/// North arrow centred at `at`, pointing to `north`, labelled with the localised letter for north.
pub(crate) fn north_arrow(d: &mut Drawing, ctx: &Ctx, at: Point2, north: Vec2, letter: &str) {
    let layer = "north.arrow";
    let r = ctx.mm(7.0);
    let c = p(at);
    d.push(Item {
        shape: Shape::Circle { centre: c, radius: r },
        stroke: Some(Stroke::solid(ctx.pens.symbol)),
        fill: None,
        layer: layer.into(),
        source: None,
    });
    let tip = at + north * r;
    let notch = at - north * (r * 0.45);
    let side = north.perp() * (r * 0.42);
    let back = at - north * (r * 0.85);
    // A kite: the left half filled, the right half outlined.
    let outline = Some(Stroke::solid(ctx.pens.symbol));
    d.push(Item::polygon(
        vec![vec![p(tip), p(back + side), p(notch)]],
        Some(Fill::Solid(Rgba::BLACK)),
        outline.clone(),
        layer,
    ));
    d.push(Item::polygon(
        vec![vec![p(tip), p(notch), p(back - side)]],
        Some(Fill::Solid(Rgba::WHITE)),
        outline,
        layer,
    ));
    let mut t = Text::new(p(at + north * (r + ctx.mm(4.0))), letter, 4.0).anchored(Anchor::Centre);
    t.bold = true;
    d.push(Item::text(t, layer));
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Angle;

    #[test]
    fn dimension_text_uses_the_decimal_comma() {
        let ctx = Ctx::new(50.0, Locale::El);
        let mut d = Drawing::model(50.0);
        let dim = Dimension {
            points: vec![Point2::new(0.0, 0.0), Point2::new(3.25, 0.0), Point2::new(4.0, 0.0)],
            line_point: Point2::new(0.0, -1.0),
            direction: Angle::ZERO,
        };
        dimension(&mut d, &ctx, &dim, None);
        let texts: Vec<String> = d
            .items
            .iter()
            .filter_map(|i| match &i.shape {
                Shape::Text(t) => Some(t.content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["3,25".to_string(), "0,75".to_string()]);
        assert_eq!(d.items.iter().filter(|i| i.layer == "dimension.tick").count(), 3);
        assert_eq!(d.items.iter().filter(|i| i.layer == "dimension.extension").count(), 3);
    }

    #[test]
    fn readable_angles_stay_upright() {
        let (a, up) = readable(PI);
        assert!(a.abs() < 1e-9 && up.y > 0.99);
        let (a, _) = readable(-FRAC_PI_2);
        assert!((a - FRAC_PI_2).abs() < 1e-9);
    }
}
