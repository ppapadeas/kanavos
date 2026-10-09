//! Door and window schedules: openings grouped into types.

use std::collections::BTreeMap;

use kanavos_drawing::table::{Align, Cell, Table};
use kanavos_i18n::t;
use kanavos_model::{Document, ElementKind, Id, OpeningKind};
use kanavos_units::{Length, Locale};

use crate::{col, storey_name, storey_rank, style_name};

/// A group of openings of the same style and size (one row of a door or window schedule).
#[derive(Debug, Clone, PartialEq)]
pub struct OpeningType {
    /// Type mark: the instances' common tag, or an automatic mark (`Θ1`, `Π1` …) when they have none.
    pub mark: String,
    /// Object style id, e.g. `"door.single_swing"`.
    pub style: String,
    /// Clear width.
    pub width: Length,
    /// Clear height.
    pub height: Length,
    /// Sill height above the storey floor.
    pub sill: Length,
    /// Storeys where the type occurs, lowest first.
    pub storeys: Vec<Id>,
    /// The openings of this type.
    pub instances: Vec<Id>,
}

/// Rounds a length to whole millimetres for grouping (sizes within 0.5 mm are the same type).
fn mm(l: Length) -> i64 {
    (l.as_mm()).round() as i64
}

/// Groups the openings of one kind into types: same style, width, height and sill (to the millimetre).
///
/// Types are ordered by style, then width, height and sill. A type takes the tag its instances share; when they
/// have different or no tags, it gets an automatic mark from the catalogue prefix (`schedules.mark.door` or
/// `schedules.mark.window`) and its position.
pub fn opening_types(doc: &Document, kind: OpeningKind, locale: Locale) -> Vec<OpeningType> {
    type Key = (String, i64, i64, i64);
    let mut groups: BTreeMap<Key, (OpeningType, Vec<String>)> = BTreeMap::new();
    for e in doc.elements() {
        let ElementKind::Opening(o) = &e.kind else { continue };
        if o.kind != kind {
            continue;
        }
        let key = (o.style.clone(), mm(o.width), mm(o.height), mm(o.sill));
        let entry = groups.entry(key).or_insert_with(|| {
            (
                OpeningType {
                    mark: String::new(),
                    style: o.style.clone(),
                    width: o.width,
                    height: o.height,
                    sill: o.sill,
                    storeys: Vec::new(),
                    instances: Vec::new(),
                },
                Vec::new(),
            )
        });
        entry.0.instances.push(e.id);
        if let Some(s) = e.storey
            && !entry.0.storeys.contains(&s)
        {
            entry.0.storeys.push(s);
        }
        entry.1.push(e.tag.clone());
    }
    let prefix = match kind {
        OpeningKind::Door => t(locale, "schedules.mark.door"),
        OpeningKind::Window => t(locale, "schedules.mark.window"),
        OpeningKind::Void => t(locale, "schedules.mark.void"),
    };
    let mut out = Vec::with_capacity(groups.len());
    for (i, (_, (mut ty, tags))) in groups.into_iter().enumerate() {
        let first = tags.first().cloned().unwrap_or_default();
        ty.mark = if !first.is_empty() && tags.iter().all(|t| *t == first) {
            first
        } else {
            format!("{prefix}{}", i + 1)
        };
        ty.storeys.sort_by_key(|s| storey_rank(doc, Some(*s)));
        out.push(ty);
    }
    out
}

/// The door or window schedule.
pub(crate) fn table(doc: &Document, kind: OpeningKind, locale: Locale) -> Table {
    let title = match kind {
        OpeningKind::Door => t(locale, "schedules.doors"),
        OpeningKind::Window => t(locale, "schedules.windows"),
        OpeningKind::Void => t(locale, "schedules.voids"),
    };
    let mut table = Table {
        title: title.into_owned(),
        columns: vec![
            col(locale, "schedules.col.tag", Align::Left, 14.0),
            col(locale, "schedules.col.type", Align::Left, 45.0),
            col(locale, "schedules.col.width", Align::Right, 16.0),
            col(locale, "schedules.col.height", Align::Right, 16.0),
            col(locale, "schedules.col.sill", Align::Right, 16.0),
            col(locale, "schedules.col.storeys", Align::Left, 45.0),
            col(locale, "schedules.col.count", Align::Right, 14.0),
        ],
        ..Table::default()
    };
    let mut total = 0i64;
    for ty in opening_types(doc, kind, locale) {
        let n = ty.instances.len() as i64;
        total += n;
        let storeys: Vec<String> = ty.storeys.iter().map(|s| storey_name(doc, Some(*s))).collect();
        table.rows.push(vec![
            Cell::text(ty.mark.clone()),
            Cell::text(style_name(&ty.style, locale)),
            Cell::num2(ty.width.as_m()),
            Cell::num2(ty.height.as_m()),
            Cell::num2(ty.sill.as_m()),
            Cell::text(storeys.join(", ")),
            Cell::int(n),
        ]);
    }
    table.footer.push(vec![
        Cell::text(t(locale, "schedules.total")),
        Cell::Empty,
        Cell::Empty,
        Cell::Empty,
        Cell::Empty,
        Cell::Empty,
        Cell::int(total),
    ]);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule;
    use crate::testutil::*;
    use kanavos_model::ScheduleKind;

    fn doc_with_openings() -> kanavos_model::Document {
        let (mut doc, g) = base();
        let w = add_wall(&mut doc, g, (0.0, 0.0), (10.0, 0.0));
        add(&mut doc, g, opening(w, OpeningKind::Window, "window.casement_2", 1.2, 1.4, 0.9));
        add(&mut doc, g, opening(w, OpeningKind::Window, "window.casement_2", 1.2, 1.4, 0.9));
        add(&mut doc, g, opening(w, OpeningKind::Window, "window.casement_2", 0.8, 1.4, 0.9));
        add(&mut doc, g, opening(w, OpeningKind::Door, "door.single_swing", 0.9, 2.1, 0.0));
        let u = upper(&doc);
        let w2 = add_wall(&mut doc, u, (0.0, 0.0), (10.0, 0.0));
        add(&mut doc, u, opening(w2, OpeningKind::Window, "window.casement_2", 1.2, 1.4, 0.9));
        doc
    }

    #[test]
    fn windows_are_grouped_by_size_and_counted() {
        let doc = doc_with_openings();
        let types = opening_types(&doc, OpeningKind::Window, Locale::El);
        assert_eq!(types.len(), 2);
        // Sorted by width: 0.80 first, then 1.20 with three instances on two storeys.
        let big = types.iter().find(|t| t.instances.len() == 3).map(|t| t.storeys.len());
        assert_eq!(big, Some(2));
        assert_eq!(types.first().map(|t| t.mark.as_str()), Some("Π1"));
        let table = schedule(&doc, ScheduleKind::Windows, Locale::El);
        assert_eq!(table.rows.len(), 2);
        let total = table.footer.first().and_then(|r| r.last()).map(|c| c.display(Locale::El));
        assert_eq!(total.as_deref(), Some("4"));
        let csv = table.to_csv(Locale::El);
        assert!(csv.contains("1,20;1,40;0,90"), "{csv}");
    }

    #[test]
    fn shared_tags_become_the_type_mark() {
        let (mut doc, g) = base();
        let w = add_wall(&mut doc, g, (0.0, 0.0), (10.0, 0.0));
        for _ in 0..2 {
            let id = add(&mut doc, g, opening(w, OpeningKind::Door, "door.single_swing", 0.9, 2.1, 0.0));
            if let Some(e) = doc.element(id) {
                let mut e = e.clone();
                e.tag = "Θ7".into();
                doc = doc.with_element(e);
            }
        }
        let types = opening_types(&doc, OpeningKind::Door, Locale::El);
        assert_eq!(types.len(), 1);
        assert_eq!(types.first().map(|t| t.mark.as_str()), Some("Θ7"));
    }
}
