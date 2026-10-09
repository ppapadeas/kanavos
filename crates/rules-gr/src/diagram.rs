//! The coverage diagram (διάγραμμα κάλυψης) and the site plan, as model-space drawings.
//!
//! Coverage diagram contents (see `specs/rules-gr/coverage-diagram.md`): plot boundary with vertex letters and
//! side lengths; street lines (Ρ.Γ.) and building lines (Ο.Γ.); the setback line Δ bounding the buildable area;
//! the building footprint hatched, split into the pieces of the coverage calculation, with its side lengths and
//! its distances from the plot boundary; a north arrow; and, to the right of the plot, the building data table,
//! the coverage calculation and the plot coordinates in ΕΓΣΑ87.

use kanavos_drawing::{
    Anchor, Drawing, Fill, HatchPattern, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt, weights,
};
use kanavos_geom::{Point2, Polygon2, PolygonWithHoles, Segment2};
use kanavos_i18n::{t, tf};
use kanavos_model::{Document, ElementKind, OpeningKind};
use kanavos_units::{Locale, format_decimal, format_level};

use crate::annotate::{dimension, edge_label, inset_table, north_arrow, outward, p, vertex_letters};
use crate::decompose::decompose;
use crate::report::{NokReport, nok_report};
use crate::tables::{
    building_coordinates_table_of, coordinates_table, coverage_table_of, setback_table_of, terms_table_of,
};

fn ring_pts(r: &Polygon2) -> Vec<Pt> {
    r.vertices.iter().map(|q| p(*q)).collect()
}

fn rings(poly: &PolygonWithHoles) -> Vec<Vec<Pt>> {
    poly.rings().map(ring_pts).collect()
}

fn len_text(l: f64, locale: Locale) -> String {
    format_decimal(l, 2, locale)
}

/// Plot boundary with vertex letters and side lengths.
fn plot(d: &mut Drawing, doc: &Document, locale: Locale) {
    let Some(plot) = &doc.site().plot else { return };
    let ring = plot.to_ccw();
    d.push(Item::polyline(ring_pts(&ring), true, Stroke::solid(weights::THICK), "site.plot"));
    for e in ring.edges() {
        edge_label(
            d,
            p(e.start),
            p(e.end),
            len_text(e.length().as_m(), locale),
            2.5,
            2.5,
            "site.plot.dimension",
        );
    }
    vertex_letters(d, &ring, "site.plot.vertex");
}

/// Street and building lines with their labels.
fn lines(d: &mut Drawing, doc: &Document, locale: Locale) {
    let site = doc.site();
    let sets = [
        (&site.street_lines, weights::THICK, "site.street_line", "rules.label.street_line"),
        (&site.building_lines, weights::MEDIUM, "site.building_line", "rules.label.building_line"),
    ];
    for (set, w, layer, key) in sets {
        for line in set.iter() {
            if line.len() < 2 {
                continue;
            }
            let pts: Vec<Pt> = line.iter().map(|q| p(*q)).collect();
            d.push(Item::polyline(pts.clone(), false, Stroke::patterned(w, LinePattern::DashDot), layer));
            // Label on the longest segment, above it.
            let seg = line
                .windows(2)
                .filter_map(|w| match w {
                    [a, b] => Some((*a, *b)),
                    _ => None,
                })
                .max_by(|x, y| x.0.distance(x.1).total_cmp(&y.0.distance(y.1)));
            if let Some((a, b)) = seg {
                // Labels sit on the side facing away from the plot centre would need the plot; use the left side.
                edge_label(d, p(b), p(a), t(locale, key).into_owned(), 2.5, 2.5, layer);
            }
        }
    }
}

/// Distances from the building's outer edges to the plot boundary, measured square to each building edge.
fn distances(d: &mut Drawing, doc: &Document, coverage: &[PolygonWithHoles], locale: Locale) {
    let Some(plot) = &doc.site().plot else { return };
    let boundary: Vec<Segment2> = plot.edges().collect();
    let building: Vec<Segment2> = coverage.iter().flat_map(|p| p.rings().flat_map(|r| r.edges())).collect();
    for poly in coverage {
        let ring = poly.outer.to_ccw();
        for e in ring.edges() {
            if e.length().as_m() < 2.0 {
                continue;
            }
            let Some(n) = outward(e.start, e.end) else { continue };
            let Some(u) = (e.end - e.start).normalized() else { continue };
            // Measure a quarter of the edge from its middle towards its upper-right end, off the side-length
            // label and clear of the setback note in the upper-left corner.
            let u = if u.x + u.y < 0.0 { -u } else { u };
            let m = e.midpoint() + u * (e.length().as_m() * 0.25);
            let ray = Segment2::new(m, m + n * 200.0);
            let tol = kanavos_units::Tolerance::default();
            let hit = boundary
                .iter()
                .filter_map(|b| ray.intersection(b, &tol).map(|(q, _, _)| q))
                .min_by(|a, b| m.distance(*a).total_cmp(&m.distance(*b)));
            if let Some(q) = hit {
                let dist = m.distance(q).as_m();
                // Skip edges that face another part of the building rather than the boundary.
                let clear = Segment2::new(m + n * 0.01, q);
                let blocked = building.iter().any(|b| clear.intersection(b, &tol).is_some());
                if !blocked && dist > 0.05 && dist < 60.0 {
                    dimension(d, p(m), p(q), len_text(dist, locale), "coverage.distance");
                }
            }
        }
    }
}

/// The building footprint: hatch, outline, calculation pieces and side lengths.
fn footprint(d: &mut Drawing, coverage: &[PolygonWithHoles], locale: Locale) {
    let mut i = 0;
    for poly in coverage {
        d.push(Item::polygon(
            rings(poly),
            Some(Fill::Hatch {
                pattern: HatchPattern::Diagonal,
                color: Rgba::rgb(90, 90, 90),
                background: Some(Rgba::rgb(235, 235, 235)),
                scale: 1.5,
                angle: 0.0,
            }),
            None,
            "coverage.area",
        ));
        let pieces = decompose(poly);
        if pieces.len() > 1 {
            for piece in &pieces {
                d.push(Item::polyline(
                    ring_pts(&piece.outline),
                    true,
                    Stroke::patterned(weights::FINE, LinePattern::Dashed),
                    "coverage.piece",
                ));
            }
        }
        for piece in &pieces {
            i += 1;
            if let Some(c) = piece.outline.centroid() {
                let mut tx = Text::new(p(c), format!("Ε{i}"), 3.0).anchored(Anchor::Centre);
                tx.bold = true;
                d.push(Item::text(tx, "coverage.piece"));
            }
        }
        for r in poly.rings() {
            d.push(Item::polyline(
                ring_pts(r),
                true,
                Stroke::solid(weights::EXTRA_THICK),
                "coverage.outline",
            ));
            // Side lengths inside the outline.
            let ring = r.to_ccw();
            for e in ring.edges() {
                if e.length().as_m() >= 0.5 {
                    edge_label(
                        d,
                        p(e.end),
                        p(e.start),
                        len_text(e.length().as_m(), locale),
                        2.5,
                        2.0,
                        "coverage.dimension",
                    );
                }
            }
        }
    }
}

/// The setback line bounding the buildable area, with its Δ label.
fn setback_line(d: &mut Drawing, r: &NokReport, locale: Locale) {
    for poly in &r.setbacks.buildable {
        for ring in poly.rings() {
            d.push(Item::polyline(
                ring_pts(ring),
                true,
                Stroke::patterned(weights::THIN, LinePattern::Dashed).with_color(Rgba::rgb(40, 40, 160)),
                "coverage.setback",
            ));
        }
        // Two-line note inside the upper-left corner of the buildable area.
        if let Some(corner) = poly.outer.vertices.iter().max_by(|a, b| {
            let ka = a.y.as_m() - a.x.as_m();
            let kb = b.y.as_m() - b.x.as_m();
            ka.total_cmp(&kb)
        }) {
            let values = tf(
                locale,
                "rules.label.setback_values",
                &[
                    ("d", &len_text(r.setbacks.delta.as_m(), locale)),
                    ("sd", &len_text(r.setbacks.small_delta.as_m(), locale)),
                ],
            );
            let at = p(*corner).offset(d.paper_mm(2.0), -d.paper_mm(2.5));
            let color = Rgba::rgb(40, 40, 160);
            for (i, line) in [t(locale, "rules.label.setback").into_owned(), values].into_iter().enumerate() {
                let mut tx = Text::new(at.offset(0.0, -d.paper_mm(3.5 * i as f64)), line, 2.2)
                    .anchored(Anchor::TopLeft);
                tx.color = color;
                d.push(Item::text(tx, "coverage.setback"));
            }
        }
    }
}

/// Outlines of levels that reach beyond the coverage (basements, overhanging floors), dashed and named, as
/// coverage diagrams show the outline of every level.
fn level_outlines(d: &mut Drawing, doc: &Document, r: &NokReport) {
    for st in doc.storeys() {
        let outline = crate::footprint::storey_outline(doc, st.id, &r.rules);
        let beyond = crate::ops::difference(&outline.enclosed, &r.coverage);
        if crate::ops::total_area(&beyond).as_m2() < 0.1 {
            continue;
        }
        for poly in &outline.enclosed {
            d.push(Item::polyline(
                ring_pts(&poly.outer),
                true,
                Stroke::patterned(weights::THIN, LinePattern::Hidden),
                "coverage.level",
            ));
            let top = poly.outer.vertices.iter().max_by(|a, b| (a.x + a.y).total_cmp(&(b.x + b.y)));
            if let Some(v) = top {
                let at = p(*v).offset(d.paper_mm(1.0), d.paper_mm(1.0));
                d.push(Item::text(Text::new(at, st.name.clone(), 2.0), "coverage.level"));
            }
        }
    }
}

/// Labels Κ1, Κ2, … at the building's vertices (the coordinates table lists them).
fn building_vertex_labels(d: &mut Drawing, r: &NokReport) {
    for (name, v) in crate::tables::building_vertices(r) {
        let at = p(v).offset(-d.paper_mm(1.2), d.paper_mm(1.2));
        let t = Text::new(at, name, 1.8).anchored(Anchor::BottomRight);
        d.push(Item::text(t, "coverage.vertex"));
    }
}

/// Bounds of what is drawn about the plot (plot, lines, building), in metres.
fn site_bounds(doc: &Document, coverage: &[PolygonWithHoles]) -> Option<(Pt, Pt)> {
    let site = doc.site();
    let mut pts: Vec<Point2> = site.plot.iter().flat_map(|p| p.vertices.iter().copied()).collect();
    pts.extend(coverage.iter().flat_map(|c| c.outer.vertices.iter().copied()));
    if pts.is_empty() {
        pts.extend(site.street_lines.iter().chain(site.building_lines.iter()).flatten().copied());
    }
    let bb = kanavos_geom::Bbox2::from_points(pts)?;
    let [x0, y0] = bb.min.xy();
    let [x1, y1] = bb.max.xy();
    Some((pt(x0, y0), pt(x1, y1)))
}

/// Coverage diagram (διάγραμμα κάλυψης) in model space at 1:`scale` (see the module documentation).
pub fn coverage_diagram(doc: &Document, locale: Locale, scale: f64) -> Drawing {
    let r = nok_report(doc);
    let mut d = Drawing::model(scale);
    footprint(&mut d, &r.coverage, locale);
    level_outlines(&mut d, doc, &r);
    setback_line(&mut d, &r, locale);
    plot(&mut d, doc, locale);
    lines(&mut d, doc, locale);
    distances(&mut d, doc, &r.coverage, locale);
    building_vertex_labels(&mut d, &r);
    let Some((min, max)) = site_bounds(doc, &r.coverage) else { return d };
    let mm = |v: f64| v * scale / 1000.0;
    // North arrow above the top-left corner.
    let rotation = doc.site().georef.rotation.as_rad();
    d.extend(north_arrow(pt(min.x + mm(8.0), max.y + mm(16.0)), mm(7.0), rotation, locale));
    // Inset tables in two columns to the right, top-aligned above the plot: the building data and coverage
    // calculation, then setbacks and coordinates.
    let top = max.y + mm(12.0);
    let x1 = max.x + mm(18.0);
    let mut y = top;
    let (w1, h) = inset_table(&mut d, &terms_table_of(doc, &r, locale), pt(x1, y), locale);
    y -= h + mm(8.0);
    let (w2, _) = inset_table(&mut d, &coverage_table_of(&r, locale), pt(x1, y), locale);
    if doc.site().plot.is_some() {
        let x2 = x1 + w1.max(w2) + mm(10.0);
        let mut y = top;
        y -= inset_table(&mut d, &setback_table_of(&r, locale), pt(x2, y), locale).1 + mm(8.0);
        y -= inset_table(&mut d, &coordinates_table(doc, locale), pt(x2, y), locale).1 + mm(8.0);
        inset_table(&mut d, &building_coordinates_table_of(doc, &r, locale), pt(x2, y), locale);
    }
    d
}

/// Site plan in model space at 1:`scale`: plot, street and building lines, the building outline with its number
/// of floors, roof outlines, surveyed terrain points with their levels, survey underlays, the access to the
/// plot from the street, the ±0.00 reference and a north arrow.
pub fn site_plan(doc: &Document, locale: Locale, scale: f64) -> Drawing {
    let r = nok_report(doc);
    let site = doc.site();
    let mut d = Drawing::model(scale);
    let mm = |v: f64| v * scale / 1000.0;
    // Survey underlays first.
    for e in doc.elements() {
        if let ElementKind::Underlay(u) = &e.kind {
            for item in &u.drawing.items {
                let mut item = item.clone();
                item.layer = format!("underlay.{}", item.layer);
                item.source = Some(e.id.0);
                d.push(item);
            }
        }
    }
    for poly in &r.coverage {
        d.push(Item::polygon(
            rings(poly),
            Some(Fill::Solid(Rgba::rgb(225, 225, 225))),
            Some(Stroke::solid(weights::EXTRA_THICK)),
            "site.building",
        ));
    }
    // One label, in the largest part of the building.
    let largest = r.coverage.iter().max_by(|a, b| a.area().total_cmp(&b.area()));
    if let Some(c) = largest.and_then(|poly| poly.outer.centroid()) {
        let label = tf(locale, "rules.label.floors", &[("n", &r.floors.used.to_string())]);
        let mut tx = Text::new(p(c), label, 3.0).anchored(Anchor::Centre);
        tx.bold = true;
        d.push(Item::text(tx, "site.building"));
    }
    for e in doc.elements() {
        if let ElementKind::Roof(roof) = &e.kind {
            d.push(Item::polyline(
                ring_pts(&roof.outline),
                true,
                Stroke::patterned(weights::FINE, LinePattern::Dashed),
                "site.roof",
            ));
        }
    }
    plot(&mut d, doc, locale);
    lines(&mut d, doc, locale);
    // Terrain points: a small cross and the level relative to ±0.00.
    for q in &site.terrain {
        let c = pt(q.x, q.y);
        let k = mm(1.0);
        let s = Stroke::solid(weights::FINE);
        d.push(Item::line(c.offset(-k, -k), c.offset(k, k), s.clone(), "site.terrain"));
        d.push(Item::line(c.offset(-k, k), c.offset(k, -k), s, "site.terrain"));
        let label = format_level(kanavos_units::Length::m(q.z), locale);
        d.push(Item::text(Text::new(c.offset(mm(1.5), mm(0.5)), label, 1.8), "site.terrain"));
    }
    // Access: an arrow into the plot at the middle of each street edge.
    if let Some(plot) = &site.plot {
        let ring = plot.to_ccw();
        for e in r.setbacks.edges.iter().filter(|e| e.kind == crate::setback::EdgeKind::Street) {
            let Some(seg) = ring.edge(e.edge) else { continue };
            let Some(n) = outward(seg.start, seg.end) else { continue };
            // Prefer the door nearest to this edge as the access point.
            let m = nearest_entrance(doc, &seg).unwrap_or_else(|| seg.midpoint());
            let on_edge = seg.closest_point(m);
            let tail = on_edge + n * mm(14.0);
            let head = on_edge - n * mm(2.0);
            arrow(&mut d, p(tail), p(head), mm(2.5), "site.access");
            let tx = Text::new(p(tail + n * mm(3.0)), t(locale, "rules.label.access"), 2.5)
                .anchored(Anchor::Centre);
            d.push(Item::text(tx, "site.access"));
        }
    }
    let Some((min, max)) = site_bounds(doc, &r.coverage) else { return d };
    d.extend(north_arrow(
        pt(max.x + mm(14.0), max.y - mm(8.0)),
        mm(7.0),
        site.georef.rotation.as_rad(),
        locale,
    ));
    let zero =
        tf(locale, "rules.label.zero_level", &[("alt", &format_decimal(site.georef.altitude, 2, locale))]);
    d.push(Item::text(Text::new(pt(min.x, max.y + mm(10.0)), zero, 2.5), "site.note"));
    d
}

/// The ground-floor door nearest to a plot edge, projected to plan.
fn nearest_entrance(doc: &Document, edge: &Segment2) -> Option<Point2> {
    let ground = doc.storeys().into_iter().find(|s| !s.below_ground)?;
    doc.elements_on(ground.id)
        .filter_map(|e| match &e.kind {
            ElementKind::Opening(o) if o.kind == OpeningKind::Door => {
                kanavos_elements::opening_placement(doc, e.id).ok().map(|pl| pl.centre)
            }
            _ => None,
        })
        .min_by(|a, b| edge.distance_to(*a).total_cmp(&edge.distance_to(*b)))
}

/// A line with a filled arrow head at `head`.
fn arrow(d: &mut Drawing, tail: Pt, head: Pt, size: f64, layer: &str) {
    let len = tail.distance(head);
    if len < 1e-9 {
        return;
    }
    let (ux, uy) = ((head.x - tail.x) / len, (head.y - tail.y) / len);
    d.push(Item::line(tail, head, Stroke::solid(weights::MEDIUM), layer));
    let base = pt(head.x - ux * size, head.y - uy * size);
    let w = size * 0.35;
    d.push(Item {
        shape: Shape::Polygon {
            rings: vec![vec![
                head,
                pt(base.x - uy * w, base.y + ux * w),
                pt(base.x + uy * w, base.y - ux * w),
            ]],
        },
        stroke: None,
        fill: Some(Fill::Solid(Rgba::BLACK)),
        layer: layer.to_owned(),
        source: None,
    });
}
