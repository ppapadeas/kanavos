//! Quantities and schedules.
//!
//! Layer L5. See `docs/architecture.md`.
//!
//! A schedule is a [`Table`] computed from the document; it can be shown in the interface, placed on a sheet
//! (`Table::to_drawing`) or exported as CSV (`Table::to_csv`).

use kanavos_drawing::table::{Align, Cell, Column, Table};
use kanavos_elements::quantities;
use kanavos_i18n::t;
use kanavos_model::{Document, ElementKind, OpeningKind, ScheduleKind};
use kanavos_units::Locale;

/// Computes a schedule.
pub fn schedule(doc: &Document, kind: ScheduleKind, locale: Locale) -> Table {
    match kind {
        ScheduleKind::Doors => openings(doc, OpeningKind::Door, locale),
        ScheduleKind::Windows => openings(doc, OpeningKind::Window, locale),
        ScheduleKind::Spaces => spaces(doc, locale),
        ScheduleKind::Walls => walls(doc, locale),
        _ => Table { title: t(locale, "schedules.not_available").into_owned(), ..Table::default() },
    }
}

fn storey_name(doc: &Document, s: Option<kanavos_model::Id>) -> String {
    s.and_then(|s| doc.storey(s)).map(|s| s.name.clone()).unwrap_or_default()
}

fn openings(doc: &Document, kind: OpeningKind, locale: Locale) -> Table {
    let title = match kind {
        OpeningKind::Door => t(locale, "schedules.doors"),
        _ => t(locale, "schedules.windows"),
    };
    let mut table = Table {
        title: title.into_owned(),
        columns: vec![
            Column::new(t(locale, "schedules.col.tag"), Align::Left, 15.0),
            Column::new(t(locale, "schedules.col.storey"), Align::Left, 30.0),
            Column::new(t(locale, "schedules.col.width"), Align::Right, 18.0),
            Column::new(t(locale, "schedules.col.height"), Align::Right, 18.0),
            Column::new(t(locale, "schedules.col.sill"), Align::Right, 18.0),
        ],
        ..Table::default()
    };
    for e in doc.elements() {
        if let ElementKind::Opening(o) = &e.kind
            && o.kind == kind
        {
            table.rows.push(vec![
                Cell::text(e.tag.clone()),
                Cell::text(storey_name(doc, e.storey)),
                Cell::num2(o.width.as_m()),
                Cell::num2(o.height.as_m()),
                Cell::num2(o.sill.as_m()),
            ]);
        }
    }
    let n = table.rows.len() as i64;
    table.footer.push(vec![Cell::text(t(locale, "schedules.total")), Cell::int(n)]);
    table
}

fn spaces(doc: &Document, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "schedules.spaces").into_owned(),
        columns: vec![
            Column::new(t(locale, "schedules.col.number"), Align::Left, 15.0),
            Column::new(t(locale, "schedules.col.name"), Align::Left, 40.0),
            Column::new(t(locale, "schedules.col.storey"), Align::Left, 30.0),
            Column::new(t(locale, "schedules.col.area"), Align::Right, 20.0),
        ],
        ..Table::default()
    };
    let mut total = 0.0;
    for e in doc.elements() {
        if let ElementKind::Space(s) = &e.kind {
            let a = s.boundary.area().as_m2();
            total += a;
            table.rows.push(vec![
                Cell::text(s.number.clone()),
                Cell::text(s.name.clone()),
                Cell::text(storey_name(doc, e.storey)),
                Cell::num2(a),
            ]);
        }
    }
    table.footer.push(vec![
        Cell::text(t(locale, "schedules.total")),
        Cell::Empty,
        Cell::Empty,
        Cell::num2(total),
    ]);
    table
}

fn walls(doc: &Document, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "schedules.walls").into_owned(),
        columns: vec![
            Column::new(t(locale, "schedules.col.storey"), Align::Left, 30.0),
            Column::new(t(locale, "schedules.col.length"), Align::Right, 18.0),
            Column::new(t(locale, "schedules.col.thickness"), Align::Right, 18.0),
            Column::new(t(locale, "schedules.col.area"), Align::Right, 18.0),
        ],
        ..Table::default()
    };
    for e in doc.elements() {
        if let ElementKind::Wall(_) = &e.kind
            && let Ok(q) = quantities(doc, e.id)
        {
            table.rows.push(vec![
                Cell::text(storey_name(doc, e.storey)),
                Cell::num2(q.length.map(|l| l.as_m()).unwrap_or(0.0)),
                Cell::num2(q.thickness.map(|l| l.as_m()).unwrap_or(0.0)),
                Cell::num2(q.area.map(|a| a.as_m2()).unwrap_or(0.0)),
            ]);
        }
    }
    table
}
