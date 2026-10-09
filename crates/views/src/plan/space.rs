//! Spaces in plans: tint and stamp.

use kanavos_drawing::{Anchor, Drawing, Fill, Item, Text, pt};
use kanavos_geom::Point2;
use kanavos_i18n::t;
use kanavos_model::{Element, Space, Storey};
use kanavos_units::{AreaDisplay, LengthDisplay};

use super::{PlanOptions, p, rings};
use crate::annot::Ctx;
use crate::poly::{self, Shape};
use crate::style::space_tint;

/// Light tint of a space by its use category, under everything else.
pub(super) fn tint(d: &mut Drawing, e: &Element, s: &Space) {
    d.push(
        Item::polygon(rings(&s.boundary), Some(Fill::Solid(space_tint(s.category))), None, "space.fill")
            .with_source(e.id.0),
    );
}

/// The space stamp: name (bold), number, net area and optionally perimeter and clear height, centred on the
/// stamp point (or the boundary's centroid).
#[allow(clippy::too_many_arguments)]
pub(super) fn stamp(
    d: &mut Drawing,
    ctx: &Ctx,
    storey: &Storey,
    e: &Element,
    s: &Space,
    opts: &PlanOptions,
    avoid: &[Shape],
) {
    let loc = ctx.locale;
    // Text heights (paper mm): smaller at 1:100 and below, where rooms are half as large on paper.
    let (name_h, area_h, small_h) = if ctx.scale <= 50.0 { (3.0, 2.5, 2.0) } else { (2.5, 2.0, 1.8) };
    let mut lines: Vec<(String, f64, bool)> = Vec::new();
    if !s.name.is_empty() {
        lines.push((s.name.clone(), name_h, true));
    }
    if !s.number.is_empty() {
        lines.push((s.number.clone(), small_h, false));
    }
    lines.push((
        format!("{} = {}", t(loc, "views.space.area"), AreaDisplay(s.boundary.area(), loc)),
        area_h,
        false,
    ));
    if opts.space_perimeter {
        lines.push((
            format!("{} = {}", t(loc, "views.space.perimeter"), LengthDisplay(s.boundary.perimeter(), loc)),
            small_h,
            false,
        ));
    }
    if opts.space_height {
        let h = s.height.unwrap_or(storey.height);
        lines.push((format!("{} = {}", t(loc, "views.space.height"), LengthDisplay(h, loc)), small_h, false));
    }
    let gap = |size: f64| size * 1.55;
    let total: f64 = lines.iter().map(|(_, size, _)| gap(*size)).sum();
    // Size of the stamp in model units, for placing it clear of stairs.
    let width = lines.iter().map(|(t, size, _)| t.chars().count() as f64 * size * 0.6).fold(0.0, f64::max);
    let size = (ctx.mm(width), ctx.mm(total));
    let at = s.stamp.unwrap_or_else(|| default_position(s, avoid, size));
    let c = p(at);
    let mut y = c.y + ctx.mm(total / 2.0);
    for (text, size, bold) in lines {
        y -= ctx.mm(gap(size));
        let mut tx =
            Text::new(pt(c.x, y + ctx.mm(gap(size) - size) / 2.0), text, size).anchored(Anchor::BottomCentre);
        tx.bold = bold;
        d.push(Item::text(tx, "space.stamp").with_source(e.id.0));
    }
}

/// Where a stamp goes when the space has no stamp point: its centroid, unless a stamp of `size` (width, height)
/// there would overlap `avoid` (stairs). Then the point of a grid over the space that overlaps least, and among
/// equals lies nearest the centroid, is used.
fn default_position(s: &Space, avoid: &[Shape], size: (f64, f64)) -> Point2 {
    let centroid = s.boundary.outer.centroid().unwrap_or(Point2::ORIGIN);
    let here = poly::shape_of(&s.boundary);
    let Some(bb) = poly::shape_bbox(&here) else { return centroid };
    let blocking: Vec<Shape> = avoid
        .iter()
        .filter(|a| poly::shape_bbox(a).is_some_and(|x| poly::boxes_overlap(&x, &bb, 0.0)))
        .cloned()
        .collect();
    let (hw, hh) = (size.0 / 2.0, size.1 / 2.0);
    // Overlap of the stamp box with obstacles and with the outside of the space.
    let cost = |c: [f64; 2]| -> f64 {
        let r = poly::rect(c[0] - hw, c[1] - hh, c[0] + hw, c[1] + hh);
        let blocked: f64 =
            poly::intersection(std::slice::from_ref(&r), &blocking).iter().map(poly::shape_area).sum();
        let outside: f64 = poly::difference(std::slice::from_ref(&r), std::slice::from_ref(&here))
            .iter()
            .map(poly::shape_area)
            .sum();
        blocked + outside
    };
    let c0 = centroid.xy();
    if blocking.is_empty() || cost(c0) < 1e-9 {
        return centroid;
    }
    let n = 12;
    let mut best = (cost(c0), 0.0, c0);
    for i in 0..=n {
        for j in 0..=n {
            let q = [
                bb[0] + (bb[2] - bb[0]) * f64::from(i) / f64::from(n),
                bb[1] + (bb[3] - bb[1]) * f64::from(j) / f64::from(n),
            ];
            if !poly::contains(&here, q) {
                continue;
            }
            let k = cost(q);
            let dist = (q[0] - c0[0]).hypot(q[1] - c0[1]);
            if k < best.0 - 1e-9 || ((k - best.0).abs() <= 1e-9 && dist < best.1) {
                best = (k, dist, q);
            }
        }
    }
    Point2::new(best.2[0], best.2[1])
}
