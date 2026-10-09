//! The ΝΟΚ report computed from a document: coverage, floor area, height, per-storey areas.

use kanavos_drawing::table::{Align, Cell, Column, Table};
use kanavos_drawing::{Drawing, Fill, HatchPattern, Item, LinePattern, Rgba, Stroke, pt, weights};
use kanavos_i18n::t;
use kanavos_model::{Document, ElementKind, Id, SpaceCategory};
use kanavos_units::{Area, Length, Locale};

use crate::{BuildingTerms, Meters, meters};

/// Areas of one storey.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreyAreas {
    /// Storey.
    pub storey: Id,
    /// Storey name.
    pub name: String,
    /// Gross area of all spaces.
    pub gross: Area,
    /// Area counted towards the building coefficient (δόμηση).
    pub counted: Area,
    /// Area exempt from δόμηση.
    pub exempt: Area,
}

/// The regulation figures of a project.
#[derive(Debug, Clone, PartialEq)]
pub struct NokReport {
    /// Plot area, if a plot is drawn.
    pub plot_area: Option<Area>,
    /// Coverage and floor-area meters.
    pub meters: Meters,
    /// Building height used and allowed.
    pub height: (Length, Length),
    /// Per-storey areas, lowest first.
    pub storeys: Vec<StoreyAreas>,
    /// Notes for the user (assumptions, missing data).
    pub notes: Vec<String>,
}

fn counts(category: SpaceCategory, below_ground: bool) -> bool {
    match category {
        SpaceCategory::MainUse => true,
        SpaceCategory::Auxiliary | SpaceCategory::Parking | SpaceCategory::Technical => !below_ground,
        _ => false,
    }
}

/// Computes the report. Simplified first version: coverage is the area of the spaces and slabs of the lowest
/// above-ground storey; δόμηση counts main-use spaces and above-ground auxiliary spaces.
pub fn nok_report(doc: &Document) -> NokReport {
    let site = doc.site();
    let plot_area = site.plot.as_ref().map(|p| p.area());
    let terms = BuildingTerms {
        plot_area: plot_area.unwrap_or(Area::ZERO),
        building_coefficient: site.terms.building_coefficient,
        max_coverage_ratio: site.terms.coverage_ratio,
    };
    let mut storeys = Vec::new();
    for st in doc.storeys() {
        let mut a = StoreyAreas {
            storey: st.id,
            name: st.name.clone(),
            gross: Area::ZERO,
            counted: Area::ZERO,
            exempt: Area::ZERO,
        };
        for e in doc.elements_on(st.id) {
            if let ElementKind::Space(s) = &e.kind {
                let area = s.boundary.area();
                a.gross += area;
                if counts(s.category, st.below_ground) {
                    a.counted += area;
                } else {
                    a.exempt += area;
                }
            }
        }
        storeys.push(a);
    }
    let ground = doc.storeys().into_iter().find(|s| !s.below_ground);
    let covered = ground
        .map(|g| {
            let slabs: Area = doc
                .elements_on(g.id)
                .filter_map(|e| match &e.kind {
                    ElementKind::Slab(s) => Some(s.outline.area()),
                    _ => None,
                })
                .sum();
            let spaces = storeys.iter().find(|s| s.storey == g.id).map(|s| s.gross).unwrap_or(Area::ZERO);
            slabs.max(spaces)
        })
        .unwrap_or(Area::ZERO);
    let counted: Vec<Area> = storeys.iter().map(|s| s.counted).collect();
    let top = doc.storeys().last().map(|s| s.top()).unwrap_or(Length::ZERO);
    let mut notes = Vec::new();
    if plot_area.is_none() {
        notes.push("rules.note.no_plot".to_owned());
    }
    NokReport {
        plot_area,
        meters: meters(&terms, covered, &counted),
        height: (top - site.ground_level, site.terms.max_height),
        storeys,
        notes,
    }
}

/// Area table (πίνακας εμβαδών) per storey.
pub fn area_table(doc: &Document, locale: Locale) -> Table {
    let r = nok_report(doc);
    let mut table = Table {
        title: t(locale, "rules.area_table").into_owned(),
        columns: vec![
            Column::new(t(locale, "rules.col.storey"), Align::Left, 40.0),
            Column::new(t(locale, "rules.col.gross"), Align::Right, 25.0),
            Column::new(t(locale, "rules.col.counted"), Align::Right, 25.0),
            Column::new(t(locale, "rules.col.exempt"), Align::Right, 25.0),
        ],
        ..Table::default()
    };
    let (mut g, mut c, mut x) = (0.0, 0.0, 0.0);
    for s in &r.storeys {
        g += s.gross.as_m2();
        c += s.counted.as_m2();
        x += s.exempt.as_m2();
        table.rows.push(vec![
            Cell::text(s.name.clone()),
            Cell::num2(s.gross.as_m2()),
            Cell::num2(s.counted.as_m2()),
            Cell::num2(s.exempt.as_m2()),
        ]);
    }
    table.footer.push(vec![
        Cell::text(t(locale, "rules.total")),
        Cell::num2(g),
        Cell::num2(c),
        Cell::num2(x),
    ]);
    table
}

/// Coverage diagram (διάγραμμα κάλυψης) in model space: plot, setback line placeholder, covered outlines.
pub fn coverage_diagram(doc: &Document, locale: Locale, scale: f64) -> Drawing {
    let _ = locale;
    let mut d = Drawing::model(scale);
    let p = |q: kanavos_geom::Point2| {
        let [x, y] = q.xy();
        pt(x, y)
    };
    if let Some(plot) = &doc.site().plot {
        d.push(Item::polyline(
            plot.vertices.iter().map(|q| p(*q)).collect(),
            true,
            Stroke::patterned(weights::MEDIUM, LinePattern::DashDotDot),
            "site.plot",
        ));
    }
    if let Some(ground) = doc.storeys().into_iter().find(|s| !s.below_ground) {
        for e in doc.elements_on(ground.id) {
            if let ElementKind::Slab(s) = &e.kind {
                d.push(Item::polygon(
                    s.outline.rings().map(|r| r.vertices.iter().map(|q| p(*q)).collect()).collect(),
                    Some(Fill::Hatch {
                        pattern: HatchPattern::Diagonal,
                        color: Rgba::BLACK,
                        background: None,
                        scale: 2.0,
                        angle: 0.0,
                    }),
                    Some(Stroke::solid(weights::THICK)),
                    "coverage.area",
                ));
            }
        }
    }
    d
}
