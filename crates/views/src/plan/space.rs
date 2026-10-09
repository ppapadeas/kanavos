//! Spaces in plans: tint and stamp.

use kanavos_drawing::{Anchor, Drawing, Fill, Item, Text, pt};
use kanavos_geom::Point2;
use kanavos_i18n::t;
use kanavos_model::{Element, Space, Storey};
use kanavos_units::{AreaDisplay, LengthDisplay};

use super::{PlanOptions, p, rings};
use crate::annot::Ctx;
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
pub(super) fn stamp(d: &mut Drawing, ctx: &Ctx, storey: &Storey, e: &Element, s: &Space, opts: &PlanOptions) {
    let at = s.stamp.or_else(|| s.boundary.outer.centroid()).unwrap_or(Point2::ORIGIN);
    let loc = ctx.locale;
    let mut lines: Vec<(String, f64, bool)> = Vec::new();
    if !s.name.is_empty() {
        lines.push((s.name.clone(), 3.0, true));
    }
    if !s.number.is_empty() {
        lines.push((s.number.clone(), 2.0, false));
    }
    lines.push((
        format!("{} = {}", t(loc, "views.space.area"), AreaDisplay(s.boundary.area(), loc)),
        2.5,
        false,
    ));
    if opts.space_perimeter {
        lines.push((
            format!("{} = {}", t(loc, "views.space.perimeter"), LengthDisplay(s.boundary.perimeter(), loc)),
            2.0,
            false,
        ));
    }
    if opts.space_height {
        let h = s.height.unwrap_or(storey.height);
        lines.push((format!("{} = {}", t(loc, "views.space.height"), LengthDisplay(h, loc)), 2.0, false));
    }
    let gap = |size: f64| size * 1.55;
    let total: f64 = lines.iter().map(|(_, size, _)| gap(*size)).sum();
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
