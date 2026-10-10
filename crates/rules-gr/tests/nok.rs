//! ΝΟΚ figures on small, hand-computed documents built with the model API.
//!
//! Every expected number is worked out in the comment above its assertion.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, missing_docs)]

use kanavos_geom::{Point2, Polygon2};
use kanavos_model::{
    BuildingTerms, Document, Element, ElementKind, Id, ReferenceLine, Roof, RoofShape, Site, Space,
    SpaceCategory, Storey, Structure, Wall, starter,
};
use kanavos_rules_gr::{EdgeKind, NokRules, Severity, area_table, nok_report, nok_report_with, terms_table};
use kanavos_units::{Angle, Length, Locale};

const TOL: f64 = 0.005;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < TOL
}

/// The starter (ground at ±0.00 and first floor at +3.20, both 3.20 m high) with a 20 × 25 m plot, Σ.Δ. 0.8,
/// coverage 60 %, max height 11 m, street along the south edge and no building line.
fn base() -> Document {
    let doc = starter();
    doc.with_site(Site {
        plot: Some(Polygon2::from_m(&[(0.0, 0.0), (20.0, 0.0), (20.0, 25.0), (0.0, 25.0)])),
        street_lines: vec![vec![Point2::new(-5.0, 0.0), Point2::new(25.0, 0.0)]],
        terms: BuildingTerms {
            building_coefficient: 0.8,
            coverage_ratio: 0.6,
            max_height: Length::m(11.0),
            max_floors: Some(2),
            volume_coefficient: None,
            setback: None,
        },
        ..Site::default()
    })
}

fn storeys(doc: &Document) -> Vec<Id> {
    doc.storeys().iter().map(|s| s.id).collect()
}

fn add(doc: &mut Document, storey: Id, kind: ElementKind) -> Id {
    let (d, id) = doc.with_new_id();
    *doc = d.with_element(Element::on_storey(id, storey, kind));
    id
}

fn material(doc: &Document, name: &str) -> Id {
    doc.materials().find(|m| m.name == name).map(|m| m.id).unwrap()
}

/// Four walls with their outer faces on `[x0, x1] × [y0, y1]` (clockwise, reference on the left face).
fn walls(doc: &mut Document, storey: Id, r: (f64, f64, f64, f64), structure: Structure, height: f64) {
    let (x0, y0, x1, y1) = r;
    let c = [(x0, y0), (x0, y1), (x1, y1), (x1, y0)];
    for i in 0..4 {
        let (a, b) = (c[i], c[(i + 1) % 4]);
        add(
            doc,
            storey,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference: ReferenceLine::Left,
                structure: structure.clone(),
                height: Length::m(height),
                base_offset: Length::ZERO,
            }),
        );
    }
}

/// [`walls`] of 0.25 m brick.
fn brick_walls(doc: &mut Document, storey: Id, r: (f64, f64, f64, f64), height: f64) {
    let brick = Structure::Basic {
        material: material(doc, "Οπτοπλινθοδομή"), thickness: Length::m(0.25)
    };
    walls(doc, storey, r, brick, height);
}

fn space(category: SpaceCategory, r: (f64, f64, f64, f64)) -> ElementKind {
    let (x0, y0, x1, y1) = r;
    ElementKind::Space(Space {
        boundary: Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]).into(),
        name: "Χώρος".into(),
        number: String::new(),
        category,
        height: None,
        stamp: None,
        finish: String::new(),
    })
}

/// A 10 × 8 m box (outer faces) on the ground floor at (5, 6), 0.25 m brick walls, one main-use space.
fn box_house() -> Document {
    let mut doc = base();
    let g = storeys(&doc)[0];
    brick_walls(&mut doc, g, (5.0, 6.0, 15.0, 14.0), 3.2);
    add(&mut doc, g, space(SpaceCategory::MainUse, (5.25, 6.25, 14.75, 13.75)));
    doc
}

#[test]
fn box_house_figures() {
    let r = nok_report(&box_house());
    // Plot 20 × 25 = 500 m²; allowed coverage 60 % = 300 m², floor area 0.8 × 500 = 400 m².
    assert!(close(r.plot_area.unwrap().as_m2(), 500.0));
    assert!(close(r.meters.coverage.allowed.as_m2(), 300.0));
    assert!(close(r.meters.floor_area.allowed.as_m2(), 400.0));
    // Coverage and δόμηση: the 10 × 8 outline (walls included) = 80 m².
    assert!(close(r.meters.coverage.used.as_m2(), 80.0), "{:?}", r.meters.coverage);
    assert!(close(r.meters.floor_area.used.as_m2(), 80.0));
    let g = &r.storeys[0];
    assert!(close(g.gross.as_m2(), 80.0) && close(g.main.as_m2(), 80.0) && close(g.exempt.as_m2(), 0.0));
    // No roof or slab: the height is the wall top, 3.20 m above the ground at ±0.00.
    assert!(close(r.height.0.as_m(), 3.2));
    assert_eq!(r.floors.used, 1);
    // Volume: 80 m² × 3.20 m = 256 m³; allowed 5 × Σ.Δ. × plot = 5 × 0.8 × 500 = 2000 m³.
    let v = r.volume.unwrap();
    assert!(close(v.used.as_m3(), 256.0) && close(v.allowed.as_m3(), 2000.0));
    // Uncovered area 500 − 80.
    assert!(close(r.uncovered.unwrap().as_m2(), 420.0));
    // Σ.Δ. is not used up, so H for the setbacks is the maximum height, 11 m:
    // Δ = 3 + 0.10 × 11 = 4.10 m, δ = 2.50 + 0.05 × 11 = 3.05 m.
    assert!(close(r.setbacks.delta.as_m(), 4.1) && close(r.setbacks.small_delta.as_m(), 3.05));
    // The box is 5 m from the sides, 6 m from the street, 11 m from the rear: it complies.
    assert!(r.complies(), "{:#?}", r.findings);
    let kinds: Vec<EdgeKind> = r.setbacks.edges.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [EdgeKind::Street, EdgeKind::Side, EdgeKind::Rear, EdgeKind::Side]);
    assert!(close(r.setbacks.edges[1].actual.unwrap().as_m(), 5.0));
}

#[test]
fn exempt_spaces_share_the_walls_in_proportion() {
    let mut doc = base();
    let g = storeys(&doc)[0];
    // Outline 10 × 6 = 60 m² (outer faces); net spaces 7.5 × 5 = 37.5 m² and 2 × 5 = 10 m², 47.5 m² in all,
    // so each m² of space carries 60 / 47.5 m² of floor area.
    brick_walls(&mut doc, g, (0.0, 0.0, 10.0, 6.0), 3.2);
    add(&mut doc, g, space(SpaceCategory::MainUse, (0.25, 0.25, 7.75, 5.25)));
    add(&mut doc, g, space(SpaceCategory::Stairwell, (7.75, 0.25, 9.75, 5.25)));
    let r = nok_report(&doc);
    let s = &r.storeys[0];
    assert!(close(s.gross.as_m2(), 60.0));
    // Stairwell share 10 × 60 / 47.5 = 12.6316 m² ≤ 25 m²: exempt. Main use 37.5 × 60 / 47.5 = 47.3684 m².
    assert!(close(s.circulation.as_m2(), 12.6316), "{}", s.circulation.as_m2());
    assert!(close(s.counted.as_m2(), 47.3684) && close(s.exempt.as_m2(), 12.6316));
}

#[test]
fn stairwells_beyond_the_limit_count() {
    let mut doc = base();
    let g = storeys(&doc)[0];
    brick_walls(&mut doc, g, (0.0, 0.0, 10.0, 8.0), 3.2);
    add(&mut doc, g, space(SpaceCategory::Stairwell, (0.25, 0.25, 9.75, 7.75)));
    let r = nok_report(&doc);
    // The whole 80 m² is stairwell; 25 m² is exempt, 55 m² counts.
    assert!(close(r.storeys[0].counted.as_m2(), 55.0));
    assert!(r.findings.iter().any(|f| f.key == "rules.note.stair_excess"));
}

#[test]
fn basement_auxiliary_is_exempt_and_main_use_counts_half() {
    let mut doc = base();
    let (d, b) = doc.with_new_id();
    let mut basement = Storey::new(b, "Υπόγειο", Length::m(-3.0), Length::m(3.0));
    basement.below_ground = true;
    doc = d.with_storey(basement);
    brick_walls(&mut doc, b, (5.0, 6.0, 15.0, 14.0), 3.0);
    // Two equal halves of 4.75 × 7.5 m: each carries 40 m² of the 80 m² outline.
    add(&mut doc, b, space(SpaceCategory::Auxiliary, (5.25, 6.25, 10.0, 13.75)));
    add(&mut doc, b, space(SpaceCategory::MainUse, (10.0, 6.25, 14.75, 13.75)));
    let r = nok_report(&doc);
    let s = r.storeys.iter().find(|s| s.below_ground).unwrap();
    // Storage exempt; main use in a basement counts half: 40 × 0.5 = 20 m².
    assert!(close(s.counted.as_m2(), 20.0) && close(s.exempt.as_m2(), 60.0));
    // A basement adds nothing to coverage, and its ceiling is at ±0.00, on the ground.
    assert!(close(s.covered.as_m2(), 0.0) && close(r.meters.coverage.used.as_m2(), 0.0));
    assert!(!r.findings.iter().any(|f| f.key == "rules.warn.basement_too_high"));
    // With the ground 1.50 m lower, the ceiling stands 1.50 m > 1.20 m above it: not a basement.
    let mut site = doc.site().clone();
    site.ground_level = Length::m(-1.5);
    let r = nok_report(&doc.with_site(site));
    assert!(r.findings.iter().any(|f| f.key == "rules.warn.basement_too_high"));
}

#[test]
fn open_spaces_within_and_beyond_their_allowance() {
    let make = |semi: f64, balcony: f64| {
        let mut doc = box_house();
        let ids = storeys(&doc);
        // A semi-open space south of the box and a balcony on the first floor, both 1 m deep.
        add(&mut doc, ids[0], space(SpaceCategory::SemiOpen, (5.0, 6.0 - semi / 10.0, 15.0, 6.0)));
        add(&mut doc, ids[1], space(SpaceCategory::Balcony, (5.0, 14.0, 15.0, 14.0 + balcony / 10.0)));
        nok_report(&doc)
    };
    // Allowed floor area 400 m²: open spaces exempt up to 40 % = 160 m², semi-open alone capped at 20 % = 80 m².
    let r = make(30.0, 20.0);
    assert!(
        close(r.open_spaces.allowance.as_m2(), 160.0) && close(r.open_spaces.semi_open_cap.as_m2(), 80.0)
    );
    assert!(close(r.open_spaces.counted.as_m2(), 0.0));
    // Coverage takes the semi-open space (80 + 30) but not the balcony.
    assert!(close(r.meters.coverage.used.as_m2(), 110.0), "{}", r.meters.coverage.used.as_m2());
    assert!(close(r.meters.floor_area.used.as_m2(), 80.0));
    // 70 + 100 = 170 m² > 160 m²: the whole 170 m² counts.
    let r = make(70.0, 100.0);
    assert!(close(r.open_spaces.counted.as_m2(), 170.0));
    assert!(close(r.meters.floor_area.used.as_m2(), 250.0));
    // 90 m² of semi-open space breaks the 80 m² cap.
    let r = make(90.0, 0.0);
    assert!(
        r.findings.iter().any(|f| f.key == "rules.warn.semi_open_cap" && f.severity == Severity::Violation)
    );
}

#[test]
fn external_insulation_is_left_out_of_coverage() {
    let mut doc = base();
    let g = storeys(&doc)[0];
    // Exterior build-up of the starter: render 1 + EPS 8 + brick 20 + plaster 2 cm, render and EPS outside.
    let bu = doc.defaults().exterior_wall.unwrap();
    walls(&mut doc, g, (5.0, 6.0, 15.0, 14.0), Structure::Layered { build_up: bu }, 3.2);
    // Without the outer 9 cm on each side: 9.82 × 7.82 = 76.7924 m².
    let r = nok_report(&doc);
    assert!(close(r.meters.coverage.used.as_m2(), 76.7924), "{}", r.meters.coverage.used.as_m2());
    let off = NokRules { exclude_external_insulation: false, ..NokRules::default() };
    let r = nok_report_with(&doc, &off);
    assert!(close(r.meters.coverage.used.as_m2(), 80.0));
}

#[test]
fn height_is_measured_to_the_structural_top_of_the_roof_slab() {
    let mut doc = box_house();
    let ids = storeys(&doc);
    brick_walls(&mut doc, ids[1], (5.0, 6.0, 15.0, 14.0), 3.2);
    let bu = doc.defaults().flat_roof.unwrap();
    add(
        &mut doc,
        ids[1],
        ElementKind::Roof(Roof {
            outline: Polygon2::from_m(&[(5.0, 6.0), (15.0, 6.0), (15.0, 14.0), (5.0, 14.0)]),
            base: Length::m(3.2),
            structure: Structure::Layered { build_up: bu },
            holes: Vec::new(),
            shape: RoofShape::Flat { parapet: None },
        }),
    );
    let mut site = doc.site().clone();
    site.ground_level = Length::m(-0.3);
    let doc = doc.with_site(site);
    let r = nok_report(&doc);
    // Roof top at 3.20 + 3.20 = 6.40; above the concrete: gravel 5 + XPS 8 + membrane 0.5 + screed 6 = 19.5 cm.
    // H = 6.40 − 0.195 + 0.30 = 6.505 m.
    assert!(close(r.height.0.as_m(), 6.505), "{}", r.height.0.as_m());
    assert_eq!(r.floors.used, 2);
    // Volume: 80 m² × (3.20 + 3.20 + 0.30 plinth) = 536 m³.
    assert!(close(r.volume_used.as_m3(), 536.0), "{}", r.volume_used.as_m3());
}

#[test]
fn pitched_roof_ridge_is_checked_against_the_roof_allowance() {
    let mut doc = box_house();
    let ids = storeys(&doc);
    let concrete = material(&doc, "Οπλισμένο σκυρόδεμα");
    let gable = |pitch: f64| {
        ElementKind::Roof(Roof {
            outline: Polygon2::from_m(&[(5.0, 6.0), (15.0, 6.0), (15.0, 14.0), (5.0, 14.0)]),
            base: Length::m(3.0),
            structure: Structure::Basic { material: concrete, thickness: Length::m(0.15) },
            // Slopes on the two 10 m edges, gables on the 8 m edges.
            holes: Vec::new(),
            shape: RoofShape::Pitched {
                slopes: vec![Some(Angle::deg(pitch)), None, Some(Angle::deg(pitch)), None],
            },
        })
    };
    let mut low = doc.clone();
    add(&mut low, ids[1], gable(30.0));
    let r = nok_report(&low);
    // Eaves at 3.20 + 3.00 = 6.20; rise 4 × tan 30° = 2.3094; ridge 8.5094 m ≤ 11 + 2 m.
    assert!(close(r.ridge.unwrap().as_m(), 8.5094), "{:?}", r.ridge);
    assert!(!r.findings.iter().any(|f| f.key == "rules.warn.roof_too_high"));
    // Lowest bearing point: eaves − thickness = 6.05 m.
    assert!(close(r.height.0.as_m(), 6.05));
    // At 70° the ridge reaches 6.20 + 4 × tan 70° = 17.19 m > 13 m.
    add(&mut doc, ids[1], gable(70.0));
    let r = nok_report(&doc);
    assert!(r.findings.iter().any(|f| f.key == "rules.warn.roof_too_high"));
}

#[test]
fn setback_violations_carry_their_place() {
    let mut doc = base();
    let g = storeys(&doc)[0];
    // 1.50 m from the west boundary where δ = 3.05 m is required.
    brick_walls(&mut doc, g, (1.5, 6.0, 11.5, 14.0), 3.2);
    let r = nok_report(&doc);
    let f = r.findings.iter().find(|f| f.key == "rules.warn.setback").unwrap();
    assert_eq!(f.severity, Severity::Violation);
    let at = f.location.unwrap();
    assert!(close(at.x.as_m(), 1.5), "{at:?}");
    let msg = f.message(Locale::El);
    assert!(msg.contains("1,50 m") && msg.contains("3,05 m") && msg.contains("Δ-Α"), "{msg}");
    let msg = f.message(Locale::En);
    assert!(msg.contains("1.50 m") && msg.contains("3.05 m"), "{msg}");
    assert!(!r.complies());
}

#[test]
fn fixed_setback_from_the_terms_overrides_the_formula() {
    let mut doc = box_house();
    let mut site = doc.site().clone();
    site.terms.setback = Some(Length::m(15.0));
    doc = doc.with_site(site);
    let r = nok_report(&doc);
    assert!(close(r.setbacks.delta.as_m(), 15.0) && close(r.setbacks.small_delta.as_m(), 15.0));
    assert!(r.findings.iter().filter(|f| f.key == "rules.warn.setback").count() >= 2);
}

#[test]
fn small_plots_may_cover_120_square_metres() {
    let mut doc = base();
    let mut site = doc.site().clone();
    site.plot = Some(Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 18.0), (0.0, 18.0)]));
    doc = doc.with_site(site);
    let r = nok_report(&doc);
    // 60 % of 180 m² = 108 m² < 120 m²: raised to 120 m² (70 % = 126 m² is not reached).
    assert!(close(r.meters.coverage.allowed.as_m2(), 120.0));
}

#[test]
fn attic_storeys_are_exempt_up_to_half_the_storey_below_and_are_not_floors() {
    let mut doc = box_house();
    let ids = storeys(&doc);
    brick_walls(&mut doc, ids[1], (5.0, 6.0, 15.0, 14.0), 2.0);
    add(&mut doc, ids[1], space(SpaceCategory::Attic, (5.25, 6.25, 14.75, 13.75)));
    let r = nok_report(&doc);
    // The attic carries the whole 80 m² outline; half the storey below (40 m²) is exempt, 40 m² counts.
    assert!(close(r.storeys[1].counted.as_m2(), 40.0), "{}", r.storeys[1].counted.as_m2());
    assert_eq!(r.floors.used, 1);
}

#[test]
fn small_pilotis_is_flagged() {
    let mut doc = box_house();
    let g = storeys(&doc)[0];
    add(&mut doc, g, space(SpaceCategory::Pilotis, (5.0, 2.0, 9.0, 6.0)));
    let r = nok_report(&doc);
    // 16 m² of pilotis against 96 m² of coverage: less than half.
    assert!(r.findings.iter().any(|f| f.key == "rules.warn.pilotis"));
}

#[test]
fn no_plot_gives_a_note_and_no_limits() {
    let doc = starter();
    let r = nok_report(&doc);
    assert!(r.plot_area.is_none() && r.volume.is_none());
    assert!(r.notes.iter().any(|n| n == "rules.note.no_plot"));
}

#[test]
fn tables_have_text_in_both_languages() {
    let doc = box_house();
    for locale in [Locale::El, Locale::En] {
        for table in [area_table(&doc, locale), terms_table(&doc, locale)] {
            assert!(!table.title.starts_with("rules."));
            assert!(table.columns.iter().all(|c| !c.header.starts_with("rules.")));
            for row in table.rows.iter().chain(table.footer.iter()) {
                for c in row {
                    assert!(!c.display(locale).starts_with("rules."), "untranslated {:?}", c);
                }
            }
        }
    }
    let t = area_table(&doc, Locale::El);
    // Ground floor row: main 80,00, δόμηση 80,00, κάλυψη 80,00.
    let ground: Vec<String> = t.rows[0].iter().map(|c| c.display(Locale::El)).collect();
    assert_eq!(ground[2], "80,00");
    assert_eq!(ground[7], "80,00");
    assert_eq!(ground[8], "80,00");
}
