//! The permit drawing set: the views a Greek building permit needs, created when missing, laid out on sheets in
//! the usual order.
//!
//! Order of the set: site plan and coverage diagram; one plan per storey; sections; elevations; tables (area
//! table and schedules). Sheets are numbered `Α-01`, `Α-02`, … after any existing `Α-` sheets.

use std::collections::BTreeMap;

use kanavos_geom::{Point2, PolygonWithHoles, Vec2};
use kanavos_i18n::{t, tf};
use kanavos_model::{Document, ElementKind, Id, Paper, PaperSize, ScheduleKind, Sheet, View, ViewKind};
use kanavos_units::{Length, Locale};

use crate::SheetError;
use crate::layout::{PAPER_CANDIDATES, place, place_overflowing, sizes_of, viewports};

/// The views of a permit set, found or created by [`ensure_permit_views`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PermitViews {
    /// Site plan (τοπογραφικό / διάγραμμα δόμησης).
    pub site_plan: Option<Id>,
    /// Coverage diagram (διάγραμμα κάλυψης).
    pub coverage: Option<Id>,
    /// Plans of the storeys that hold elements, lowest first.
    pub plans: Vec<Id>,
    /// Sections, Α-Α first.
    pub sections: Vec<Id>,
    /// Elevations, by orientation (north, east, south, west order of the façade normals).
    pub elevations: Vec<Id>,
    /// Area table and schedules.
    pub tables: Vec<Id>,
}

/// Distance of elevation lines from the building, metres.
const ELEVATION_OFFSET: f64 = 3.0;
/// How far section and elevation lines run past the building on each side, metres.
const LINE_OVERRUN: f64 = 2.0;

/// The building's main axes and extent: unit vectors `u` (along the longest footprint edge) and `v` (90° to the
/// left), and the extent of the footprint in that frame.
struct Frame {
    u: Vec2,
    v: Vec2,
    umin: f64,
    umax: f64,
    vmin: f64,
    vmax: f64,
}

impl Frame {
    fn of(footprint: &[PolygonWithHoles]) -> Option<Frame> {
        let largest = footprint.iter().max_by(|a, b| a.area().total_cmp(&b.area()))?;
        let u = largest
            .outer
            .edges()
            .max_by(|a, b| a.length().total_cmp(&b.length()))
            .and_then(|e| e.direction())?;
        // Keep the axis pointing into the first half-plane so results do not depend on ring orientation.
        let u = if u.x < -1e-9 || (u.x.abs() <= 1e-9 && u.y < 0.0) { -u } else { u };
        let v = u.perp();
        let pts: Vec<Point2> = footprint.iter().flat_map(|p| p.outer.vertices.iter().copied()).collect();
        let us = pts.iter().map(|p| p.to_vec().dot(u));
        let vs = pts.iter().map(|p| p.to_vec().dot(v));
        let (umin, umax) = us.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        let (vmin, vmax) = vs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        (umin <= umax && vmin <= vmax).then_some(Frame { u, v, umin, umax, vmin, vmax })
    }
    fn point(&self, a: f64, b: f64) -> Point2 {
        let p = self.u * a + self.v * b;
        Point2::new(p.x, p.y)
    }
}

/// Compass sector (0 = north, 1 = north-east, … 7 = north-west) a façade with outward normal `n` faces, with grid
/// north at `rotation` counter-clockwise from project +Y.
pub(crate) fn compass_sector(n: Vec2, rotation: f64) -> usize {
    let north = Vec2::new(-rotation.sin(), rotation.cos());
    let east = Vec2::new(rotation.cos(), rotation.sin());
    let bearing = n.dot(east).atan2(n.dot(north)).to_degrees().rem_euclid(360.0);
    ((bearing / 45.0).round() as usize) % 8
}

const SECTOR_KEYS: [&str; 8] = [
    "sheets.elev.n",
    "sheets.elev.ne",
    "sheets.elev.e",
    "sheets.elev.se",
    "sheets.elev.s",
    "sheets.elev.sw",
    "sheets.elev.w",
    "sheets.elev.nw",
];

fn most_common_plan_scale(doc: &Document) -> u32 {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    for v in doc.views() {
        if let ViewKind::Plan { .. } = v.kind {
            *counts.entry(v.scale).or_insert(0) += 1;
        }
    }
    counts.into_iter().max_by_key(|(s, n)| (*n, *s)).map(|(s, _)| s).unwrap_or(100)
}

fn add_view(doc: &mut Document, name: String, kind: ViewKind, scale: u32) -> Id {
    let (d, id) = doc.with_new_id();
    *doc = d.with_view(View { id, name, kind, scale, hidden_layers: Default::default() });
    id
}

fn find_view(doc: &Document, f: impl Fn(&ViewKind) -> bool) -> Option<Id> {
    doc.views().find(|v| f(&v.kind)).map(|v| v.id)
}

fn is_marker(m: &str, greek: &str, latin: &str) -> bool {
    let m = m.trim();
    m == greek || m.eq_ignore_ascii_case(latin)
}

/// Finds the permit views and creates the missing ones: site plan, coverage diagram, area table, sections Α-Α
/// (along the building's long axis, through a stair when there is one, else through the middle) and Β-Β (across
/// it, through the middle), and four elevations named by the compass direction their façade faces.
///
/// Sections and elevations take the most common plan scale. Without a building footprint no section or
/// elevation is created.
pub fn ensure_permit_views(doc: &Document, locale: Locale) -> (Document, PermitViews) {
    let mut doc = doc.clone();
    let mut out = PermitViews::default();
    let scale = most_common_plan_scale(&doc);

    out.site_plan = find_view(&doc, |k| matches!(k, ViewKind::SitePlan));
    if out.site_plan.is_none() && doc.site().plot.is_some() {
        out.site_plan = Some(add_view(
            &mut doc,
            t(locale, "sheets.view.site_plan").into_owned(),
            ViewKind::SitePlan,
            200,
        ));
    }
    out.coverage = find_view(&doc, |k| matches!(k, ViewKind::CoverageDiagram)).or_else(|| {
        Some(add_view(&mut doc, t(locale, "sheets.coverage").into_owned(), ViewKind::CoverageDiagram, 200))
    });

    // Plans of storeys that hold elements (all plans when nothing is modelled yet).
    let storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
    let used: Vec<Id> = storeys.iter().copied().filter(|s| doc.elements_on(*s).next().is_some()).collect();
    let wanted = if used.is_empty() { storeys } else { used };
    out.plans = wanted.iter().filter_map(|s| doc.plan_view_of(*s).map(|v| v.id)).collect();

    let footprint = kanavos_rules_gr::building_footprint(&doc);
    if let Some(f) = Frame::of(&footprint) {
        let (lu, lv) = (f.umax - f.umin, f.vmax - f.vmin);
        let (umid, vmid) = ((f.umin + f.umax) / 2.0, (f.vmin + f.vmax) / 2.0);
        // Α-Α runs along the longer side; through the first stair if one lies inside the building.
        let stair = doc.elements().find_map(|e| match &e.kind {
            ElementKind::Stair(_) => kanavos_elements::footprint(&doc, e.id).ok().flatten(),
            _ => None,
        });
        let stair_c =
            stair.and_then(|s| s.outer.centroid()).map(|c| (c.to_vec().dot(f.u), c.to_vec().dot(f.v)));
        let long_along_u = lu >= lv;
        let sections = [("Α", "A", long_along_u), ("Β", "B", !long_along_u)];
        for (greek, latin, along_u) in sections {
            let existing = doc
                .views()
                .find(|v| matches!(&v.kind, ViewKind::Section { marker, .. } if is_marker(marker, greek, latin)))
                .map(|v| v.id);
            let id = existing.unwrap_or_else(|| {
                let (start, end, depth) = if along_u {
                    let at = match (greek, stair_c) {
                        ("Α", Some((_, sv))) if sv > f.vmin && sv < f.vmax => sv,
                        _ => vmid,
                    };
                    // Looks towards +v (left of +u).
                    (
                        f.point(f.umin - LINE_OVERRUN, at),
                        f.point(f.umax + LINE_OVERRUN, at),
                        f.vmax - at + 1.0,
                    )
                } else {
                    let at = match (greek, stair_c) {
                        ("Α", Some((su, _))) if su > f.umin && su < f.umax => su,
                        _ => umid,
                    };
                    // Runs along −v so that it looks towards +u... the left of −v is +u.
                    (
                        f.point(at, f.vmax + LINE_OVERRUN),
                        f.point(at, f.vmin - LINE_OVERRUN),
                        f.umax - at + 1.0,
                    )
                };
                let name = tf(locale, "sheets.view.section", &[("m", greek)]);
                let kind = ViewKind::Section {
                    start,
                    end,
                    depth: Length::m(depth.max(1.0)),
                    flip: false,
                    marker: greek.to_owned(),
                };
                add_view(&mut doc, name, kind, scale)
            });
            out.sections.push(id);
        }

        // Elevations: one per façade normal ±u, ±v, ordered by compass bearing.
        let rotation = doc.site().georef.rotation.as_rad();
        let (o, r) = (ELEVATION_OFFSET, LINE_OVERRUN);
        // (outward normal, line start, line end, depth); each line runs along perp(n) so it looks along −n.
        let lines = [
            (f.u, f.point(f.umax + o, f.vmin - r), f.point(f.umax + o, f.vmax + r), o + lu + 1.0),
            (-f.u, f.point(f.umin - o, f.vmax + r), f.point(f.umin - o, f.vmin - r), o + lu + 1.0),
            (f.v, f.point(f.umax + r, f.vmax + o), f.point(f.umin - r, f.vmax + o), o + lv + 1.0),
            (-f.v, f.point(f.umin - r, f.vmin - o), f.point(f.umax + r, f.vmin - o), o + lv + 1.0),
        ];
        let mut facades: Vec<(usize, Vec2, Point2, Point2, f64)> =
            lines.into_iter().map(|(n, s, e, depth)| (compass_sector(n, rotation), n, s, e, depth)).collect();
        facades.sort_by_key(|(s, ..)| *s);
        for (i, (sector, n, start, end, depth)) in facades.into_iter().enumerate() {
            let look = -n;
            let existing = doc.views().find_map(|v| match &v.kind {
                ViewKind::Elevation { start, end, flip, .. } => {
                    let d = (*end - *start).normalized()?;
                    let l = if *flip { -d.perp() } else { d.perp() };
                    (l.dot(look) > 0.866).then_some(v.id)
                }
                _ => None,
            });
            let id = existing.unwrap_or_else(|| {
                let key = SECTOR_KEYS.get(sector).copied().unwrap_or("sheets.elev.n");
                let kind = ViewKind::Elevation {
                    start,
                    end,
                    depth: Length::m(depth),
                    flip: false,
                    marker: format!("{}", i + 1),
                };
                add_view(&mut doc, t(locale, key).into_owned(), kind, scale)
            });
            if !out.elevations.contains(&id) {
                out.elevations.push(id);
            }
        }
    }

    // Tables: area table first, then door, window and space schedules.
    let area = find_view(&doc, |k| matches!(k, ViewKind::AreaTable)).unwrap_or_else(|| {
        add_view(&mut doc, t(locale, "sheets.areas").into_owned(), ViewKind::AreaTable, 100)
    });
    out.tables.push(area);
    for kind in [ScheduleKind::Doors, ScheduleKind::Windows, ScheduleKind::Spaces] {
        if let Some(v) =
            find_view(&doc, |k| matches!(k, ViewKind::Schedule { schedule } if *schedule == kind))
        {
            out.tables.push(v);
        }
    }
    (doc, out)
}

/// Splits views into sheets: each sheet takes as many consecutive views as fit on an A1 sheet; a view too large
/// for A1 gets its own sheet of the size it needs. Every sheet then takes the smallest paper that holds it.
fn paginate(
    doc: &Document,
    views: &[Id],
    locale: Locale,
) -> Result<Vec<(Paper, Vec<kanavos_model::Viewport>)>, SheetError> {
    let sizes = sizes_of(doc, views, locale)?;
    let max_index = PAPER_CANDIDATES
        .iter()
        .position(|p| p.size == PaperSize::A1 && !p.landscape)
        .unwrap_or(PAPER_CANDIDATES.len());
    let upto = PAPER_CANDIDATES.get(..=max_index).unwrap_or(&PAPER_CANDIDATES);
    /// Views of one sheet with their paper sizes.
    type Page = (Vec<Id>, Vec<(f64, f64)>);
    let mut pages: Vec<Page> = Vec::new();
    for (v, s) in views.iter().zip(sizes) {
        let grow = pages.last().is_some_and(|(_, ss)| {
            let mut trial = ss.clone();
            trial.push(s);
            place(&trial, upto).is_some()
        });
        match pages.last_mut() {
            Some((vs, ss)) if grow => {
                vs.push(*v);
                ss.push(s);
            }
            _ => pages.push((vec![*v], vec![s])),
        }
    }
    Ok(pages
        .into_iter()
        .map(|(vs, ss)| {
            let (paper, centres) = place(&ss, &PAPER_CANDIDATES).unwrap_or_else(|| place_overflowing(&ss));
            (paper, viewports(&vs, centres))
        })
        .collect())
}

/// Next free sheet number with the `Α-` prefix.
fn next_number(doc: &Document) -> u32 {
    doc.sheets()
        .iter()
        .filter_map(|s| s.number.strip_prefix("Α-").and_then(|n| n.trim().parse::<u32>().ok()))
        .max()
        .map(|n| n + 1)
        .unwrap_or(1)
}

/// Adds the permit drawing set as sheets, creating missing views first (see [`ensure_permit_views`]). Returns the
/// new document; existing sheets are kept and the new ones are numbered after them.
pub fn permit_set(doc: &Document, locale: Locale) -> Result<Document, SheetError> {
    let (mut doc, views) = ensure_permit_views(doc, locale);
    let mut groups: Vec<(String, Vec<Id>)> = Vec::new();
    let site: Vec<Id> = views.site_plan.into_iter().chain(views.coverage).collect();
    if !site.is_empty() {
        groups.push((t(locale, "sheets.coverage_sheet").into_owned(), site));
    }
    for p in &views.plans {
        let name = doc.view(*p).map(|v| v.name.clone()).unwrap_or_default();
        groups.push((name, vec![*p]));
    }
    if !views.sections.is_empty() {
        groups.push((t(locale, "sheets.sections").into_owned(), views.sections.clone()));
    }
    if !views.elevations.is_empty() {
        groups.push((t(locale, "sheets.elevations").into_owned(), views.elevations.clone()));
    }
    if !views.tables.is_empty() {
        groups.push((t(locale, "sheets.tables").into_owned(), views.tables.clone()));
    }
    let mut number = next_number(&doc);
    for (name, group) in groups {
        let pages = paginate(&doc, &group, locale)?;
        let count = pages.len();
        for (i, (paper, viewports)) in pages.into_iter().enumerate() {
            let (d, id) = doc.with_new_id();
            let name = if count > 1 { format!("{name} ({}/{count})", i + 1) } else { name.clone() };
            doc = d.with_sheet(Sheet {
                id,
                number: format!("Α-{number:02}"),
                name,
                paper,
                viewports,
                title_block: true,
            });
            number += 1;
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compass_sectors_follow_grid_north() {
        assert_eq!(compass_sector(Vec2::new(0.0, -1.0), 0.0), 4); // south
        assert_eq!(compass_sector(Vec2::new(1.0, 0.0), 0.0), 2); // east
        // Grid north rotated 90° counter-clockwise: project +X now faces south.
        assert_eq!(compass_sector(Vec2::new(1.0, 0.0), std::f64::consts::FRAC_PI_2), 4);
        assert_eq!(compass_sector(Vec2::new(1.0, 1.0), 0.0), 1); // north-east
    }
}
