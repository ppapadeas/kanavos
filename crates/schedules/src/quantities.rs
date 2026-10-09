//! Wall and slab schedules grouped by build-up.

use std::collections::BTreeMap;

use kanavos_drawing::table::{Align, Cell, Table};
use kanavos_elements::quantities;
use kanavos_i18n::t;
use kanavos_model::{Document, ElementKind, Id, Structure};
use kanavos_units::{Area, Length, Locale, Volume, format_decimal};

use crate::{col, storey_name, storey_rank};

/// Walls sharing one build-up (or one basic material and thickness).
#[derive(Debug, Clone, PartialEq)]
pub struct WallGroup {
    /// Build-up name, or "material thickness" for basic walls.
    pub name: String,
    /// Total thickness.
    pub thickness: Length,
    /// Number of walls.
    pub count: usize,
    /// Total reference-line length.
    pub length: Length,
    /// Total net side area (openings subtracted).
    pub area: Area,
    /// Total net volume.
    pub volume: Volume,
}

/// Slabs of one storey sharing one build-up.
#[derive(Debug, Clone, PartialEq)]
pub struct SlabGroup {
    /// Storey.
    pub storey: Option<Id>,
    /// Build-up name, or "material thickness" for basic slabs.
    pub name: String,
    /// Total thickness.
    pub thickness: Length,
    /// Number of slabs.
    pub count: usize,
    /// Total net plan area.
    pub area: Area,
    /// Total perimeter (outer and hole edges).
    pub perimeter: Length,
    /// Total volume.
    pub volume: Volume,
}

/// Display name of a structure: the build-up name, or the material name and thickness in centimetres.
pub(crate) fn structure_name(doc: &Document, s: &Structure, locale: Locale) -> String {
    match s {
        Structure::Layered { build_up } => {
            doc.build_up(*build_up).map(|b| b.name.clone()).unwrap_or_else(|| build_up.to_string())
        }
        Structure::Basic { material, thickness } => {
            let m = doc.material(*material).map(|m| m.name.clone()).unwrap_or_else(|| material.to_string());
            format!("{m} {} cm", format_decimal(thickness.as_cm(), 1, locale))
        }
    }
}

/// Walls grouped by structure, in order of first appearance by name.
pub fn wall_groups(doc: &Document, locale: Locale) -> Vec<WallGroup> {
    let mut groups: BTreeMap<String, WallGroup> = BTreeMap::new();
    for e in doc.elements() {
        let ElementKind::Wall(w) = &e.kind else { continue };
        let Ok(q) = quantities(doc, e.id) else { continue };
        let name = structure_name(doc, &w.structure, locale);
        let g = groups.entry(name.clone()).or_insert_with(|| WallGroup {
            name,
            thickness: q.thickness.unwrap_or(Length::ZERO),
            count: 0,
            length: Length::ZERO,
            area: Area::ZERO,
            volume: Volume::ZERO,
        });
        g.count += 1;
        g.length += q.length.unwrap_or(Length::ZERO);
        g.area += q.area.unwrap_or(Area::ZERO);
        g.volume += q.volume.unwrap_or(Volume::ZERO);
    }
    groups.into_values().collect()
}

/// Slabs grouped by storey and structure, lowest storey first.
pub fn slab_groups(doc: &Document, locale: Locale) -> Vec<SlabGroup> {
    let mut groups: BTreeMap<(usize, String), SlabGroup> = BTreeMap::new();
    for e in doc.elements() {
        let ElementKind::Slab(s) = &e.kind else { continue };
        let Ok(q) = quantities(doc, e.id) else { continue };
        let name = structure_name(doc, &s.structure, locale);
        let g = groups.entry((storey_rank(doc, e.storey), name.clone())).or_insert_with(|| SlabGroup {
            storey: e.storey,
            name,
            thickness: q.thickness.unwrap_or(Length::ZERO),
            count: 0,
            area: Area::ZERO,
            perimeter: Length::ZERO,
            volume: Volume::ZERO,
        });
        g.count += 1;
        g.area += q.area.unwrap_or(Area::ZERO);
        g.perimeter += q.perimeter.unwrap_or(Length::ZERO);
        g.volume += q.volume.unwrap_or(Volume::ZERO);
    }
    groups.into_values().collect()
}

/// Wall schedule by build-up.
pub(crate) fn walls_table(doc: &Document, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "schedules.walls").into_owned(),
        columns: vec![
            col(locale, "schedules.col.build_up", Align::Left, 60.0),
            col(locale, "schedules.col.thickness_m", Align::Right, 18.0),
            col(locale, "schedules.col.count", Align::Right, 12.0),
            col(locale, "schedules.col.length_m", Align::Right, 20.0),
            col(locale, "schedules.col.area_m2", Align::Right, 20.0),
            col(locale, "schedules.col.volume_m3", Align::Right, 20.0),
        ],
        ..Table::default()
    };
    let (mut n, mut l, mut a, mut v) = (0usize, 0.0, 0.0, 0.0);
    for g in wall_groups(doc, locale) {
        n += g.count;
        l += g.length.as_m();
        a += g.area.as_m2();
        v += g.volume.as_m3();
        table.rows.push(vec![
            Cell::text(g.name),
            Cell::num2(g.thickness.as_m()),
            Cell::int(g.count as i64),
            Cell::num2(g.length.as_m()),
            Cell::num2(g.area.as_m2()),
            Cell::num2(g.volume.as_m3()),
        ]);
    }
    table.footer.push(vec![
        Cell::text(t(locale, "schedules.total")),
        Cell::Empty,
        Cell::int(n as i64),
        Cell::num2(l),
        Cell::num2(a),
        Cell::num2(v),
    ]);
    table
}

/// Slab schedule by storey and build-up.
pub(crate) fn slabs_table(doc: &Document, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "schedules.slabs").into_owned(),
        columns: vec![
            col(locale, "schedules.col.storey", Align::Left, 28.0),
            col(locale, "schedules.col.build_up", Align::Left, 55.0),
            col(locale, "schedules.col.thickness_m", Align::Right, 18.0),
            col(locale, "schedules.col.count", Align::Right, 12.0),
            col(locale, "schedules.col.area_m2", Align::Right, 20.0),
            col(locale, "schedules.col.perimeter_m", Align::Right, 20.0),
            col(locale, "schedules.col.volume_m3", Align::Right, 20.0),
        ],
        ..Table::default()
    };
    let (mut n, mut a, mut p, mut v) = (0usize, 0.0, 0.0, 0.0);
    for g in slab_groups(doc, locale) {
        n += g.count;
        a += g.area.as_m2();
        p += g.perimeter.as_m();
        v += g.volume.as_m3();
        table.rows.push(vec![
            Cell::text(storey_name(doc, g.storey)),
            Cell::text(g.name),
            Cell::num2(g.thickness.as_m()),
            Cell::int(g.count as i64),
            Cell::num2(g.area.as_m2()),
            Cell::num2(g.perimeter.as_m()),
            Cell::num2(g.volume.as_m3()),
        ]);
    }
    table.footer.push(vec![
        Cell::text(t(locale, "schedules.total")),
        Cell::Empty,
        Cell::Empty,
        Cell::int(n as i64),
        Cell::num2(a),
        Cell::num2(p),
        Cell::num2(v),
    ]);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use kanavos_geom::Polygon2;
    use kanavos_model::{OpeningKind, Slab};

    #[test]
    fn walls_are_grouped_by_build_up_with_openings_subtracted() {
        let (mut doc, g) = base();
        let w = add_wall(&mut doc, g, (0.0, 0.0), (10.0, 0.0));
        add_wall(&mut doc, g, (10.0, 0.0), (10.0, 6.0));
        add(&mut doc, g, opening(w, OpeningKind::Window, "window.casement_2", 1.2, 1.5, 0.9));
        let groups = wall_groups(&doc, Locale::El);
        assert_eq!(groups.len(), 1);
        let gr = groups.first().map(|g| (g.count, g.length.as_m(), g.area.as_m2(), g.thickness.as_m()));
        let (count, length, area, thickness) = gr.unwrap_or_default();
        assert_eq!(count, 2);
        assert!((length - 16.0).abs() < 1e-9);
        // 16 m × 3 m minus a 1.2 × 1.5 window.
        assert!((area - (48.0 - 1.8)).abs() < 1e-9, "{area}");
        assert!((thickness - 0.31).abs() < 1e-9);
        let table = walls_table(&doc, Locale::El);
        let vol = table.footer.first().and_then(|r| r.get(5)).map(|c| c.display(Locale::El));
        assert_eq!(vol.as_deref(), Some("14,32"));
    }

    #[test]
    fn slabs_are_grouped_per_storey() {
        let (mut doc, g) = base();
        let build_up = doc.defaults().floor_slab.unwrap_or_default();
        for storey in [g, upper(&doc)] {
            add(
                &mut doc,
                storey,
                ElementKind::Slab(Slab {
                    outline: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]).into(),
                    structure: Structure::Layered { build_up },
                    top_offset: Length::ZERO,
                }),
            );
        }
        let groups = slab_groups(&doc, Locale::El);
        assert_eq!(groups.len(), 2);
        let table = slabs_table(&doc, Locale::En);
        let area = table.footer.first().and_then(|r| r.get(4)).map(|c| c.display(Locale::En));
        assert_eq!(area.as_deref(), Some("160.00"));
        // 0.275 m thick build-up: 2 × 80 m² × 0.275 m.
        let vol = table.footer.first().and_then(|r| r.get(6)).map(|c| c.display(Locale::En));
        assert_eq!(vol.as_deref(), Some("44.00"));
    }
}
