//! Quantities and schedules.
//!
//! Layer L5. See `docs/architecture.md`.
//!
//! A schedule is a [`Table`] computed from the document; it can be shown in the interface, placed on a sheet
//! (`Table::to_drawing`) or exported as CSV ([`schedule_csv`], which uses `Table::to_csv`). Every schedule is a
//! pure function of the document and is specified in `specs/schedules/`.
//!
//! Numbers are rounded only for display: totals are sums of the unrounded values, so a total can differ from
//! the sum of the displayed rows by at most half a unit of the last decimal per row.

mod materials;
mod openings;
mod quantities;
mod spaces;

use kanavos_drawing::table::{Align, Cell, Column, Table};
use kanavos_i18n::t;
use kanavos_model::{Document, Id, ScheduleKind};
use kanavos_units::Locale;

pub use materials::{MaterialQuantity, material_quantities};
pub use openings::{OpeningType, opening_types};
pub use quantities::{SlabGroup, WallGroup, slab_groups, wall_groups};
pub use spaces::{SpaceRow, space_category_key, space_rows};

/// Computes a schedule.
pub fn schedule(doc: &Document, kind: ScheduleKind, locale: Locale) -> Table {
    match kind {
        ScheduleKind::Doors => openings::table(doc, kanavos_model::OpeningKind::Door, locale),
        ScheduleKind::Windows => openings::table(doc, kanavos_model::OpeningKind::Window, locale),
        ScheduleKind::Spaces => spaces::table(doc, locale),
        ScheduleKind::Walls => quantities::walls_table(doc, locale),
        ScheduleKind::Slabs => quantities::slabs_table(doc, locale),
        ScheduleKind::Materials => materials::table(doc, locale),
        ScheduleKind::Objects => objects_table(doc, locale),
    }
}

/// A schedule as CSV text (see `Table::to_csv` for separators and encoding).
pub fn schedule_csv(doc: &Document, kind: ScheduleKind, locale: Locale) -> String {
    schedule(doc, kind, locale).to_csv(locale)
}

/// Catalogue key of a schedule's title.
pub fn schedule_title_key(kind: ScheduleKind) -> &'static str {
    match kind {
        ScheduleKind::Doors => "schedules.doors",
        ScheduleKind::Windows => "schedules.windows",
        ScheduleKind::Spaces => "schedules.spaces",
        ScheduleKind::Walls => "schedules.walls",
        ScheduleKind::Slabs => "schedules.slabs",
        ScheduleKind::Materials => "schedules.materials",
        ScheduleKind::Objects => "schedules.objects",
    }
}

/// Name of a storey, or empty when the element has none.
pub(crate) fn storey_name(doc: &Document, s: Option<Id>) -> String {
    s.and_then(|s| doc.storey(s)).map(|s| s.name.clone()).unwrap_or_default()
}

/// Position of a storey in elevation order (storeys without one sort last).
pub(crate) fn storey_rank(doc: &Document, s: Option<Id>) -> usize {
    let order = doc.storeys();
    s.and_then(|s| order.iter().position(|x| x.id == s)).unwrap_or(order.len())
}

/// A localised column.
pub(crate) fn col(locale: Locale, key: &str, align: Align, width_mm: f64) -> Column {
    Column::new(t(locale, key), align, width_mm)
}

/// Display name of a catalogue object or opening style, localised; falls back to the style id when the
/// catalogue has no text for it.
pub fn style_name(style: &str, locale: Locale) -> String {
    match kanavos_objects::find(style) {
        Some(def) if kanavos_i18n::has(def.name) => t(locale, def.name).into_owned(),
        _ => style.to_owned(),
    }
}

/// Placed catalogue objects counted by type.
fn objects_table(doc: &Document, locale: Locale) -> Table {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<(String, String), i64> = BTreeMap::new();
    for e in doc.elements() {
        if let kanavos_model::ElementKind::Object(o) = &e.kind {
            let category = kanavos_objects::find(&o.definition)
                .map(|d| d.category.name_key())
                .filter(|k| kanavos_i18n::has(k))
                .map(|k| t(locale, k).into_owned())
                .unwrap_or_default();
            *counts.entry((category, style_name(&o.definition, locale))).or_insert(0) += 1;
        }
    }
    let mut table = Table {
        title: t(locale, "schedules.objects").into_owned(),
        columns: vec![
            col(locale, "schedules.col.category", Align::Left, 40.0),
            col(locale, "schedules.col.object", Align::Left, 55.0),
            col(locale, "schedules.col.count", Align::Right, 15.0),
        ],
        ..Table::default()
    };
    let mut total = 0;
    for ((category, name), n) in counts {
        total += n;
        table.rows.push(vec![Cell::text(category), Cell::text(name), Cell::int(n)]);
    }
    table.footer.push(vec![Cell::text(t(locale, "schedules.total")), Cell::Empty, Cell::int(total)]);
    table
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Small builders for test documents.

    use kanavos_geom::{Point2, Polygon2};
    use kanavos_model::{
        Document, Element, ElementKind, Id, Opening, OpeningKind, ReferenceLine, Space, SpaceCategory,
        Storey, Structure, Wall, starter,
    };
    use kanavos_units::Length;

    /// The starter document and its ground storey.
    pub fn base() -> (Document, Id) {
        let doc = starter();
        let ground = doc.storeys().first().map(|s| s.id).unwrap_or_default();
        (doc, ground)
    }

    /// Adds an element and returns its id.
    pub fn add(doc: &mut Document, storey: Id, kind: ElementKind) -> Id {
        let (d, id) = doc.with_new_id();
        *doc = d.with_element(Element::on_storey(id, storey, kind));
        id
    }

    /// A straight wall with the default exterior build-up.
    pub fn wall(doc: &Document, a: (f64, f64), b: (f64, f64)) -> ElementKind {
        let build_up = doc.defaults().exterior_wall.unwrap_or_default();
        ElementKind::Wall(Wall {
            start: Point2::new(a.0, a.1),
            end: Point2::new(b.0, b.1),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Layered { build_up },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        })
    }

    /// Adds a straight wall with the default exterior build-up and returns its id.
    pub fn add_wall(doc: &mut Document, storey: Id, a: (f64, f64), b: (f64, f64)) -> Id {
        let kind = wall(doc, a, b);
        add(doc, storey, kind)
    }

    /// An opening in a wall.
    pub fn opening(host: Id, kind: OpeningKind, style: &str, w: f64, h: f64, sill: f64) -> ElementKind {
        ElementKind::Opening(Opening {
            host,
            kind,
            style: style.into(),
            offset: Length::m(1.5),
            width: Length::m(w),
            height: Length::m(h),
            sill: Length::m(sill),
            flip_side: false,
            flip_hand: false,
            params: Default::default(),
        })
    }

    /// A rectangular space.
    pub fn space(name: &str, number: &str, category: SpaceCategory, w: f64, d: f64) -> ElementKind {
        ElementKind::Space(Space {
            boundary: Polygon2::from_m(&[(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)]).into(),
            name: name.into(),
            number: number.into(),
            category,
            height: None,
            stamp: None,
            finish: String::new(),
        })
    }

    /// The first storey above the ground storey.
    pub fn upper(doc: &Document) -> Id {
        doc.storeys().get(1).map(|s: &&Storey| s.id).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Point2;
    use kanavos_model::{ElementKind, Object};
    use testutil::*;

    #[test]
    fn objects_are_counted_by_type() {
        let (mut doc, g) = base();
        for def in ["sanitary.wc", "sanitary.wc", "furniture.table"] {
            add(
                &mut doc,
                g,
                ElementKind::Object(Object {
                    definition: def.into(),
                    position: Point2::new(1.0, 1.0),
                    rotation: Default::default(),
                    mirrored: false,
                    elevation: Default::default(),
                    params: Default::default(),
                }),
            );
        }
        let table = schedule(&doc, ScheduleKind::Objects, Locale::En);
        assert_eq!(table.rows.len(), 2);
        let counts: Vec<String> =
            table.rows.iter().filter_map(|r| r.get(2)).map(|c| c.display(Locale::En)).collect();
        assert!(counts.contains(&"2".to_owned()) && counts.contains(&"1".to_owned()));
        let total = table.footer.first().and_then(|r| r.get(2)).map(|c| c.display(Locale::En));
        assert_eq!(total.as_deref(), Some("3"));
    }

    #[test]
    fn every_schedule_has_a_title_and_columns() {
        let (doc, _) = base();
        for kind in [
            ScheduleKind::Doors,
            ScheduleKind::Windows,
            ScheduleKind::Spaces,
            ScheduleKind::Walls,
            ScheduleKind::Slabs,
            ScheduleKind::Materials,
            ScheduleKind::Objects,
        ] {
            for locale in [Locale::El, Locale::En] {
                let table = schedule(&doc, kind, locale);
                assert!(!table.title.is_empty() && kanavos_i18n::has(schedule_title_key(kind)));
                assert!(!table.columns.is_empty());
                for c in &table.columns {
                    assert!(!c.header.starts_with("schedules."), "missing text for {}", c.header);
                }
                let csv = schedule_csv(&doc, kind, locale);
                assert!(csv.starts_with('\u{feff}'));
            }
        }
    }
}
