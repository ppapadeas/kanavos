//! Material take-off: area and volume per material, from build-up layers and solid elements.

use std::collections::BTreeMap;

use kanavos_drawing::table::{Align, Cell, Table};
use kanavos_elements::{quantities, solids};
use kanavos_i18n::t;
use kanavos_model::{Document, ElementKind, Id, Structure};
use kanavos_units::{Area, Locale, Volume};

use crate::col;

/// Quantity of one material.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialQuantity {
    /// Material.
    pub material: Id,
    /// Name.
    pub name: String,
    /// Layer area: for every wall, slab or roof layer, the element's net area (sides of walls with openings
    /// subtracted, plan area of slabs, roof area). Solid elements (columns, beams, stairs) add no area.
    pub area: Area,
    /// Volume: layer area × layer thickness, plus the volume of solid elements.
    pub volume: Volume,
    /// Mass from the material density, kilograms; `None` when the density is unknown.
    pub mass_kg: Option<f64>,
}

fn add_layers(doc: &Document, acc: &mut BTreeMap<Id, (Area, Volume)>, structure: &Structure, area: Area) {
    for l in doc.structure_layers(structure).unwrap_or_default() {
        let e = acc.entry(l.material).or_insert((Area::ZERO, Volume::ZERO));
        e.0 += area;
        e.1 += area * l.thickness;
    }
}

/// Quantities of every material used by walls, slabs, roofs, columns, beams, stairs and railings, ordered by
/// material id. Elements whose geometry fails are skipped.
pub fn material_quantities(doc: &Document) -> Vec<MaterialQuantity> {
    let mut acc: BTreeMap<Id, (Area, Volume)> = BTreeMap::new();
    for e in doc.elements() {
        match &e.kind {
            ElementKind::Wall(w) => {
                if let Ok(q) = quantities(doc, e.id) {
                    add_layers(doc, &mut acc, &w.structure, q.area.unwrap_or(Area::ZERO));
                }
            }
            ElementKind::Slab(s) => {
                if let Ok(q) = quantities(doc, e.id) {
                    add_layers(doc, &mut acc, &s.structure, q.area.unwrap_or(Area::ZERO));
                }
            }
            ElementKind::Roof(r) => {
                if let Ok(q) = quantities(doc, e.id) {
                    add_layers(doc, &mut acc, &r.structure, q.area.unwrap_or(Area::ZERO));
                }
            }
            ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Stair(_)
            | ElementKind::Railing(_) => {
                for s in solids(doc, e.id).unwrap_or_default() {
                    if let Some(m) = s.material {
                        let entry = acc.entry(m).or_insert((Area::ZERO, Volume::ZERO));
                        entry.1 += Volume::m3(s.mesh.volume().abs());
                    }
                }
            }
            _ => {}
        }
    }
    acc.into_iter()
        .map(|(id, (area, volume))| {
            let m = doc.material(id);
            MaterialQuantity {
                material: id,
                name: m.map(|m| m.name.clone()).unwrap_or_else(|| id.to_string()),
                area,
                volume,
                mass_kg: m.and_then(|m| m.density).map(|d| d * volume.as_m3()),
            }
        })
        .collect()
}

/// The material take-off table.
pub(crate) fn table(doc: &Document, locale: Locale) -> Table {
    let mut table = Table {
        title: t(locale, "schedules.materials").into_owned(),
        columns: vec![
            col(locale, "schedules.col.material", Align::Left, 55.0),
            col(locale, "schedules.col.area_m2", Align::Right, 22.0),
            col(locale, "schedules.col.volume_m3", Align::Right, 22.0),
            col(locale, "schedules.col.mass_kg", Align::Right, 24.0),
        ],
        ..Table::default()
    };
    let (mut v, mut kg) = (0.0, 0.0);
    for q in material_quantities(doc) {
        v += q.volume.as_m3();
        kg += q.mass_kg.unwrap_or(0.0);
        table.rows.push(vec![
            Cell::text(q.name),
            Cell::num2(q.area.as_m2()),
            Cell::num2(q.volume.as_m3()),
            q.mass_kg.map(|m| Cell::Number { value: m, decimals: 0 }).unwrap_or(Cell::Empty),
        ]);
    }
    table.footer.push(vec![
        Cell::text(t(locale, "schedules.total")),
        Cell::Empty,
        Cell::num2(v),
        Cell::Number { value: kg, decimals: 0 },
    ]);
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use kanavos_geom::Point2;
    use kanavos_model::{Column, Profile};
    use kanavos_units::Length;

    #[test]
    fn wall_layers_and_columns_are_taken_off() {
        let (mut doc, g) = base();
        add_wall(&mut doc, g, (0.0, 0.0), (10.0, 0.0));
        let concrete = doc.defaults().structural_material.unwrap_or_default();
        add(
            &mut doc,
            g,
            ElementKind::Column(Column {
                position: Point2::new(5.0, 5.0),
                rotation: Default::default(),
                profile: Profile::Rectangle { width: Length::m(0.4), depth: Length::m(0.5) },
                material: concrete,
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        );
        let q = material_quantities(&doc);
        // Exterior build-up: plaster 1 + EPS 8 + brick 20 + plaster 2 cm on a 10 × 3 m wall.
        let brick = q.iter().find(|m| m.name == "Οπτοπλινθοδομή").map(|m| (m.area.as_m2(), m.volume.as_m3()));
        let (a, v) = brick.unwrap_or_default();
        assert!((a - 30.0).abs() < 1e-9 && (v - 6.0).abs() < 1e-9, "{a} {v}");
        let plaster = q.iter().find(|m| m.name == "Επίχρισμα").map(|m| (m.area.as_m2(), m.volume.as_m3()));
        let (a, v) = plaster.unwrap_or_default();
        assert!((a - 60.0).abs() < 1e-9 && (v - 0.9).abs() < 1e-9, "{a} {v}");
        let column = q.iter().find(|m| m.material == concrete).map(|m| m.volume.as_m3());
        assert!(column.is_some_and(|v| (v - 0.6).abs() < 1e-6), "{column:?}");
        let mass = q.iter().find(|m| m.material == concrete).and_then(|m| m.mass_kg);
        assert!(mass.is_some_and(|kg| (kg - 1440.0).abs() < 1e-3));
    }
}
