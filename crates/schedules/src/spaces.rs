//! Space schedule: number, name, category, storey, area, perimeter and clear height.

use std::cmp::Ordering;

use kanavos_drawing::table::{Align, Cell, Table};
use kanavos_geom::Point2;
use kanavos_i18n::{t, tf};
use kanavos_model::{Document, Element, ElementKind, Id, SpaceCategory};
use kanavos_units::{Area, Length, Locale};

use crate::{col, storey_name, storey_rank};

/// One space in the schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceRow {
    /// Space element.
    pub id: Id,
    /// Storey, if any.
    pub storey: Option<Id>,
    /// Number, e.g. "0.01".
    pub number: String,
    /// Name.
    pub name: String,
    /// Category.
    pub category: SpaceCategory,
    /// Net area (boundary minus holes).
    pub area: Area,
    /// Perimeter of all rings.
    pub perimeter: Length,
    /// Clear height: the space's own value, or the storey height minus the thickest slab of the storey above
    /// that covers the space's centroid. `None` when neither is known.
    pub height: Option<Length>,
}

/// Catalogue key of a space category's name.
pub fn space_category_key(c: SpaceCategory) -> &'static str {
    match c {
        SpaceCategory::MainUse => "schedules.category.main_use",
        SpaceCategory::Auxiliary => "schedules.category.auxiliary",
        SpaceCategory::Stairwell => "schedules.category.stairwell",
        SpaceCategory::LiftShaft => "schedules.category.lift_shaft",
        SpaceCategory::SemiOpen => "schedules.category.semi_open",
        SpaceCategory::Balcony => "schedules.category.balcony",
        SpaceCategory::Pilotis => "schedules.category.pilotis",
        SpaceCategory::Parking => "schedules.category.parking",
        SpaceCategory::Attic => "schedules.category.attic",
        SpaceCategory::Technical => "schedules.category.technical",
        SpaceCategory::Shaft => "schedules.category.shaft",
        SpaceCategory::Other => "schedules.category.other",
    }
}

/// Compares space numbers naturally ("0.2" before "0.10"), falling back to text order.
fn natural(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<Result<u64, String>> {
        s.split(['.', '-', ' ', '/']).map(|p| p.parse::<u64>().map_err(|_| p.to_owned())).collect()
    };
    let (pa, pb) = (parts(a), parts(b));
    for (x, y) in pa.iter().zip(pb.iter()) {
        let o = match (x, y) {
            (Ok(x), Ok(y)) => x.cmp(y),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(x), Err(y)) => x.cmp(y),
        };
        if o != Ordering::Equal {
            return o;
        }
    }
    pa.len().cmp(&pb.len()).then_with(|| a.cmp(b))
}

fn clear_height(doc: &Document, e: &Element, centroid: Option<Point2>) -> Option<Length> {
    let ElementKind::Space(s) = &e.kind else { return None };
    if let Some(h) = s.height {
        return Some(h);
    }
    let storey = doc.storey(e.storey?)?;
    let tol = doc.tolerance();
    let slab = doc.storey_above(storey.id).and_then(|above| {
        doc.elements_on(above.id)
            .filter_map(|x| match &x.kind {
                ElementKind::Slab(sl)
                    if centroid.is_some_and(|c| sl.outline.contains(c, &tol))
                        && tol.is_zero(sl.top_offset) =>
                {
                    doc.structure_thickness(&sl.structure)
                }
                _ => None,
            })
            .reduce(Length::max)
    });
    let below_slab = storey.height - slab.unwrap_or(Length::ZERO);
    // A roof on the storey itself (its underside at the eaves) can be lower still.
    let under_roof = doc
        .elements_on(storey.id)
        .filter_map(|x| match &x.kind {
            ElementKind::Roof(r) if centroid.is_some_and(|c| r.outline.contains(c, &tol)) => {
                doc.structure_thickness(&r.structure).map(|t| r.base - t)
            }
            _ => None,
        })
        .reduce(Length::min);
    Some(under_roof.map_or(below_slab, |r| r.min(below_slab)))
}

/// All spaces, ordered by storey elevation, then number (natural order), then name.
pub fn space_rows(doc: &Document) -> Vec<SpaceRow> {
    let mut rows: Vec<SpaceRow> = doc
        .elements()
        .filter_map(|e| {
            let ElementKind::Space(s) = &e.kind else { return None };
            let centroid = s.stamp.or_else(|| s.boundary.outer.centroid());
            Some(SpaceRow {
                id: e.id,
                storey: e.storey,
                number: s.number.clone(),
                name: s.name.clone(),
                category: s.category,
                area: s.boundary.area(),
                perimeter: s.boundary.perimeter(),
                height: clear_height(doc, e, centroid),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        storey_rank(doc, a.storey)
            .cmp(&storey_rank(doc, b.storey))
            .then_with(|| natural(&a.number, &b.number))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

/// The space schedule, with a subtotal per storey and a grand total.
pub(crate) fn table(doc: &Document, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "schedules.spaces").into_owned(),
        columns: vec![
            col(locale, "schedules.col.number", Align::Left, 14.0),
            col(locale, "schedules.col.name", Align::Left, 40.0),
            col(locale, "schedules.col.category", Align::Left, 34.0),
            col(locale, "schedules.col.storey", Align::Left, 28.0),
            col(locale, "schedules.col.area_m2", Align::Right, 20.0),
            col(locale, "schedules.col.perimeter_m", Align::Right, 20.0),
            col(locale, "schedules.col.height_m", Align::Right, 16.0),
        ],
        ..Table::default()
    };
    let rows = space_rows(doc);
    let mut total = 0.0;
    let mut subtotals: Vec<(Option<Id>, f64)> = Vec::new();
    for r in &rows {
        let a = r.area.as_m2();
        total += a;
        match subtotals.last_mut() {
            Some((s, sum)) if *s == r.storey => *sum += a,
            _ => subtotals.push((r.storey, a)),
        }
        table.rows.push(vec![
            Cell::text(r.number.clone()),
            Cell::text(r.name.clone()),
            Cell::text(t(locale, space_category_key(r.category))),
            Cell::text(storey_name(doc, r.storey)),
            Cell::num2(a),
            Cell::num2(r.perimeter.as_m()),
            r.height.map(|h| Cell::num2(h.as_m())).unwrap_or(Cell::Empty),
        ]);
    }
    if subtotals.len() > 1 {
        for (s, sum) in &subtotals {
            let name = storey_name(doc, *s);
            table.footer.push(vec![
                Cell::Empty,
                Cell::text(tf(locale, "schedules.subtotal_of", &[("name", &name)])),
                Cell::Empty,
                Cell::Empty,
                Cell::num2(*sum),
            ]);
        }
    }
    table.footer.push(vec![
        Cell::Empty,
        Cell::text(t(locale, "schedules.total")),
        Cell::Empty,
        Cell::Empty,
        Cell::num2(total),
    ]);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn numbers_sort_naturally() {
        assert_eq!(natural("0.2", "0.10"), Ordering::Less);
        assert_eq!(natural("1.01", "0.15"), Ordering::Greater);
        assert_eq!(natural("Α", "Β"), Ordering::Less);
    }

    #[test]
    fn spaces_are_listed_by_storey_with_subtotals() {
        let (mut doc, g) = base();
        let u = upper(&doc);
        add(&mut doc, u, space("Υπνοδωμάτιο", "1.01", SpaceCategory::MainUse, 4.0, 3.5));
        add(&mut doc, g, space("Αποθήκη", "0.10", SpaceCategory::Auxiliary, 2.0, 1.5));
        add(&mut doc, g, space("Καθιστικό", "0.2", SpaceCategory::MainUse, 5.0, 4.0));
        let rows = space_rows(&doc);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Καθιστικό", "Αποθήκη", "Υπνοδωμάτιο"]);
        let first = rows.first().map(|r| (r.area.as_m2(), r.perimeter.as_m(), r.height.map(|h| h.as_m())));
        assert_eq!(first.map(|f| ((f.0 - 20.0).abs() < 1e-9, (f.1 - 18.0).abs() < 1e-9)), Some((true, true)));
        // No slab above: the clear height is the storey height.
        assert!(first.and_then(|f| f.2).is_some_and(|h| (h - 3.2).abs() < 1e-9));
        let table = table(&doc, Locale::El);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.footer.len(), 3);
        let total = table.footer.last().and_then(|r| r.get(4)).map(|c| c.display(Locale::El));
        assert_eq!(total.as_deref(), Some("37,00"));
        let category = table.rows.get(1).and_then(|r| r.get(2)).map(|c| c.display(Locale::El));
        assert_eq!(category.as_deref(), Some("Βοηθητικός χώρος"));
    }
}
