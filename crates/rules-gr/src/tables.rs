//! Regulation tables in the form Greek permit drawings use: the area table per storey (πίνακας εμβαδών), the
//! building data table against the terms (επιτρεπόμενα / πραγματοποιούμενα), the coverage calculation and the
//! plot coordinates in ΕΓΣΑ87.

use kanavos_drawing::table::{Align, Cell, Column, Table};
use kanavos_i18n::t;
use kanavos_model::Document;
use kanavos_units::{Locale, format_decimal, format_level};

use crate::annotate::vertex_name;
use crate::decompose::decompose;
use crate::georef::to_grid;
use crate::report::{NokReport, nok_report, percent};

fn col(locale: Locale, key: &str, align: Align, width: f64) -> Column {
    Column::new(t(locale, key), align, width)
}

/// Area table (πίνακας εμβαδών) per storey: main, auxiliary, circulation, semi-open and balcony areas, the
/// δόμηση and κάλυψη of each storey; then the totals achieved (πραγματοποιούμενα) and the limits
/// (επιτρεπόμενα).
pub fn area_table(doc: &Document, locale: Locale) -> Table {
    area_table_of(doc, &nok_report(doc), locale)
}

/// [`area_table`] from an already computed report.
pub fn area_table_of(doc: &Document, r: &NokReport, locale: Locale) -> Table {
    let _ = doc;
    let mut table = Table {
        title: t(locale, "rules.area_table").into_owned(),
        columns: vec![
            col(locale, "rules.col.storey", Align::Left, 34.0),
            col(locale, "rules.col.level", Align::Right, 14.0),
            col(locale, "rules.col.main", Align::Right, 20.0),
            col(locale, "rules.col.auxiliary", Align::Right, 20.0),
            col(locale, "rules.col.circulation", Align::Right, 20.0),
            col(locale, "rules.col.semi_open", Align::Right, 22.0),
            col(locale, "rules.col.balconies", Align::Right, 18.0),
            col(locale, "rules.col.counted", Align::Right, 20.0),
            col(locale, "rules.col.covered", Align::Right, 20.0),
        ],
        ..Table::default()
    };
    let (mut main, mut aux, mut circ, mut semi, mut balc) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for s in &r.storeys {
        main += s.main.as_m2();
        aux += s.auxiliary.as_m2();
        circ += s.circulation.as_m2();
        semi += s.semi_open.as_m2();
        balc += s.balconies.as_m2();
        table.rows.push(vec![
            Cell::text(s.name.clone()),
            Cell::text(format_level(s.elevation, locale)),
            Cell::num2(s.main.as_m2()),
            Cell::num2(s.auxiliary.as_m2()),
            Cell::num2(s.circulation.as_m2()),
            Cell::num2(s.semi_open.as_m2()),
            Cell::num2(s.balconies.as_m2()),
            Cell::num2(s.counted.as_m2()),
            if s.below_ground { Cell::Empty } else { Cell::num2(s.covered.as_m2()) },
        ]);
    }
    if r.open_spaces.counted.as_m2() > 0.005 {
        table.rows.push(vec![
            Cell::text(t(locale, "rules.row.open_space_counted")),
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::num2(r.open_spaces.counted.as_m2()),
            Cell::Empty,
        ]);
    }
    table.footer.push(vec![
        Cell::text(t(locale, "rules.row.achieved")),
        Cell::Empty,
        Cell::num2(main),
        Cell::num2(aux),
        Cell::num2(circ),
        Cell::num2(semi),
        Cell::num2(balc),
        Cell::num2(r.meters.floor_area.used.as_m2()),
        Cell::num2(r.meters.coverage.used.as_m2()),
    ]);
    if r.plot_area.is_some() {
        table.footer.push(vec![
            Cell::text(t(locale, "rules.row.allowed")),
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::Empty,
            Cell::num2(r.open_spaces.semi_open_cap.as_m2()),
            Cell::Empty,
            Cell::num2(r.meters.floor_area.allowed.as_m2()),
            Cell::num2(r.meters.coverage.allowed.as_m2()),
        ]);
    }
    table
}

/// Building data against the terms (πίνακας στοιχείων δόμησης): plot area, coverage, building coefficient and
/// floor area, volume, height, floors, semi-open spaces and the uncovered plot area.
pub fn terms_table(doc: &Document, locale: Locale) -> Table {
    terms_table_of(doc, &nok_report(doc), locale)
}

/// [`terms_table`] from an already computed report.
pub fn terms_table_of(doc: &Document, r: &NokReport, locale: Locale) -> Table {
    let terms = &doc.site().terms;
    let mut table = Table {
        title: t(locale, "rules.terms_table").into_owned(),
        columns: vec![
            col(locale, "rules.col.item", Align::Left, 56.0),
            col(locale, "rules.col.allowed", Align::Right, 32.0),
            col(locale, "rules.col.achieved", Align::Right, 36.0),
        ],
        ..Table::default()
    };
    let plot = r.plot_area.map(|a| a.as_m2()).unwrap_or(0.0);
    let ratio = |x: f64| if plot > 0.0 { x / plot } else { 0.0 };
    let num = |v: f64| Cell::num2(v);
    let text = |s: String| Cell::text(s);
    let row = |key: &str, allowed: Cell, achieved: Cell| vec![Cell::text(t(locale, key)), allowed, achieved];
    let plot_cell = r.plot_area.map(|a| num(a.as_m2())).unwrap_or(Cell::Empty);
    table.rows.push(row("rules.item.plot_area", Cell::Empty, plot_cell));
    table.rows.push(row(
        "rules.item.coverage_ratio",
        text(percent(ratio(r.meters.coverage.allowed.as_m2()), locale)),
        text(percent(ratio(r.meters.coverage.used.as_m2()), locale)),
    ));
    table.rows.push(row(
        "rules.item.coverage",
        num(r.meters.coverage.allowed.as_m2()),
        num(r.meters.coverage.used.as_m2()),
    ));
    table.rows.push(row(
        "rules.item.coefficient",
        text(format_decimal(terms.building_coefficient, 2, locale)),
        text(format_decimal(ratio(r.meters.floor_area.used.as_m2()), 2, locale)),
    ));
    table.rows.push(row(
        "rules.item.floor_area",
        num(r.meters.floor_area.allowed.as_m2()),
        num(r.meters.floor_area.used.as_m2()),
    ));
    if let Some(v) = &r.volume {
        table.rows.push(row(
            "rules.item.volume_coefficient",
            r.terms.volume_coefficient.map(|k| text(format_decimal(k, 2, locale))).unwrap_or(Cell::Empty),
            text(format_decimal(ratio(v.used.as_m3()), 2, locale)),
        ));
        table.rows.push(row("rules.item.volume", num(v.allowed.as_m3()), num(v.used.as_m3())));
    } else {
        table.rows.push(row("rules.item.volume", Cell::Empty, num(r.volume_used.as_m3())));
    }
    table.rows.push(row("rules.item.height", num(r.height.1.as_m()), num(r.height.0.as_m())));
    if let Some(ridge) = r.ridge {
        let allowed = r.height.1.as_m() + r.rules.roof_allowance;
        table.rows.push(row("rules.item.ridge", num(allowed), num(ridge.as_m())));
    }
    table.rows.push(row(
        "rules.item.floors",
        r.floors.allowed.map(|n| Cell::int(i64::from(n))).unwrap_or(Cell::Empty),
        Cell::int(i64::from(r.floors.used)),
    ));
    let o = &r.open_spaces;
    table.rows.push(row("rules.item.semi_open", num(o.semi_open_cap.as_m2()), num(o.semi_open.as_m2())));
    table.rows.push(row(
        "rules.item.open_spaces",
        num(o.allowance.as_m2()),
        num((o.semi_open + o.balconies).as_m2()),
    ));
    table.rows.push(row(
        "rules.item.uncovered",
        Cell::Empty,
        r.uncovered.map(|a| num(a.as_m2())).unwrap_or(Cell::Empty),
    ));
    table
}

/// Coverage calculation: the coverage polygons split into rectangles and triangles, one row per piece
/// (`Ε1 = 12,00 × 8,50 = 102,00`) and the total.
pub fn coverage_table(doc: &Document, locale: Locale) -> Table {
    coverage_table_of(&nok_report(doc), locale)
}

/// [`coverage_table`] from an already computed report.
pub fn coverage_table_of(r: &NokReport, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "rules.coverage_calc").into_owned(),
        columns: vec![
            col(locale, "rules.col.piece", Align::Left, 12.0),
            col(locale, "rules.col.formula", Align::Left, 56.0),
            col(locale, "rules.col.area", Align::Right, 22.0),
        ],
        ..Table::default()
    };
    let mut i = 0;
    let mut total = 0.0;
    for poly in &r.coverage {
        for piece in decompose(poly) {
            i += 1;
            total += piece.area.as_m2();
            table.rows.push(vec![
                Cell::text(format!("Ε{i}")),
                Cell::text(piece.formula(locale)),
                Cell::num2(piece.area.as_m2()),
            ]);
        }
    }
    table.footer.push(vec![Cell::text(t(locale, "rules.total")), Cell::Empty, Cell::num2(total)]);
    table
}

/// Distances of the building from each plot boundary against the required setbacks, with the formulas of Δ
/// and δ and the height they use.
pub fn setback_table(doc: &Document, locale: Locale) -> Table {
    setback_table_of(&nok_report(doc), locale)
}

/// [`setback_table`] from an already computed report.
pub fn setback_table_of(r: &NokReport, locale: Locale) -> Table {
    use crate::setback::EdgeKind;
    let mut table = Table {
        title: t(locale, "rules.setbacks").into_owned(),
        columns: vec![
            col(locale, "rules.col.boundary", Align::Left, 14.0),
            col(locale, "rules.col.kind", Align::Left, 32.0),
            col(locale, "rules.col.required", Align::Right, 28.0),
            col(locale, "rules.col.actual", Align::Right, 34.0),
        ],
        ..Table::default()
    };
    for e in &r.setbacks.edges {
        let kind = match e.kind {
            EdgeKind::Street => "rules.edge.street",
            EdgeKind::BuildingLine => "rules.edge.building_line",
            EdgeKind::Side => "rules.edge.side",
            EdgeKind::Rear => "rules.edge.rear",
        };
        table.rows.push(vec![
            Cell::text(e.name.clone()),
            Cell::text(t(locale, kind)),
            Cell::num2(e.required.as_m()),
            e.actual.map(|a| Cell::num2(a.as_m())).unwrap_or(Cell::Empty),
        ]);
    }
    let f = |v: f64| format_decimal(v, 2, locale);
    let h = r.setbacks.height.as_m();
    table.footer.push(vec![
        Cell::text("Δ"),
        Cell::text(format!("{} + {}·H", f(r.rules.delta_base), f(r.rules.delta_factor))),
        Cell::num2(r.setbacks.delta.as_m()),
        Cell::text(format!("H = {}", f(h))),
    ]);
    table.footer.push(vec![
        Cell::text("δ"),
        Cell::text(format!("{} + {}·H", f(r.rules.small_delta_base), f(r.rules.small_delta_factor))),
        Cell::num2(r.setbacks.small_delta.as_m()),
        Cell::Empty,
    ]);
    table
}

/// Coordinates of the building outline's vertices (Κ1, Κ2, … counter-clockwise from the vertex nearest the
/// south-west) in the national grid.
pub fn building_coordinates_table(doc: &Document, locale: Locale) -> Table {
    building_coordinates_table_of(doc, &nok_report(doc), locale)
}

/// The labelled vertices of the coverage outline, in table order.
pub fn building_vertices(r: &NokReport) -> Vec<(String, kanavos_geom::Point2)> {
    let mut out = Vec::new();
    for poly in &r.coverage {
        let ring = poly.outer.to_ccw();
        let n = ring.vertices.len();
        let start = ring
            .vertices
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a.x + a.y).total_cmp(&(b.x + b.y)))
            .map_or(0, |(i, _)| i);
        for k in 0..n {
            if let Some(v) = ring.vertices.get((start + k) % n) {
                out.push((format!("Κ{}", out.len() + 1), *v));
            }
        }
    }
    out
}

/// [`building_coordinates_table`] from an already computed report.
pub fn building_coordinates_table_of(doc: &Document, r: &NokReport, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "rules.building_coordinates").into_owned(),
        columns: vec![
            col(locale, "rules.col.vertex", Align::Centre, 14.0),
            col(locale, "rules.col.easting", Align::Right, 28.0),
            col(locale, "rules.col.northing", Align::Right, 30.0),
        ],
        ..Table::default()
    };
    for (name, v) in building_vertices(r) {
        let (e, n) = to_grid(&doc.site().georef, v);
        table.rows.push(vec![Cell::text(name), Cell::num2(e), Cell::num2(n)]);
    }
    table
}

/// Plot vertex coordinates in the national grid (ΕΓΣΑ87 when the project uses EPSG:2100), with the length of
/// the side to the next vertex and the plot area from the coordinates.
pub fn coordinates_table(doc: &Document, locale: Locale) -> Table {
    let site = doc.site();
    let mut table = Table {
        title: t(locale, "rules.coordinates").into_owned(),
        columns: vec![
            col(locale, "rules.col.vertex", Align::Centre, 14.0),
            col(locale, "rules.col.easting", Align::Right, 28.0),
            col(locale, "rules.col.northing", Align::Right, 30.0),
            col(locale, "rules.col.side", Align::Centre, 16.0),
            col(locale, "rules.col.length", Align::Right, 18.0),
        ],
        ..Table::default()
    };
    let Some(plot) = &site.plot else { return table };
    let ring = plot.to_ccw();
    let n = ring.vertices.len();
    for (i, v) in ring.vertices.iter().enumerate() {
        let (e, nn) = to_grid(&site.georef, *v);
        let next = ring.vertices.get((i + 1) % n).copied().unwrap_or(*v);
        table.rows.push(vec![
            Cell::text(vertex_name(i)),
            Cell::num2(e),
            Cell::num2(nn),
            Cell::text(format!("{}-{}", vertex_name(i), vertex_name((i + 1) % n))),
            Cell::num2(v.distance(next).as_m()),
        ]);
    }
    table.footer.push(vec![
        Cell::text("Ε ="),
        Cell::Empty,
        Cell::Empty,
        Cell::Empty,
        Cell::num2(ring.area().as_m2()),
    ]);
    table
}
