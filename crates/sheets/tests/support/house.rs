//! A small two-storey house on a plot, built with the model API, for sheet and permit-set tests.

#![allow(dead_code)]

use kanavos_geom::{Point2, Point3, Polygon2};
use kanavos_model::{
    BuildingTerms, Column, Document, Element, ElementKind, Georef, Id, Opening, OpeningKind, Parapet,
    Profile, ProjectInfo, ReferenceLine, Roof, RoofShape, Site, Slab, Space, SpaceCategory, Stair,
    StairShape, Structure, Wall, starter,
};
use kanavos_units::{Angle, Length};

fn add(doc: &mut Document, storey: Id, kind: ElementKind) -> Id {
    let (d, id) = doc.with_new_id();
    *doc = d.with_element(Element::on_storey(id, storey, kind));
    id
}

fn tag(doc: &mut Document, id: Id, tag: &str) {
    if let Some(e) = doc.element(id) {
        let mut e = e.clone();
        e.tag = tag.into();
        *doc = doc.with_element(e);
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon2 {
    Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

fn space(name: &str, number: &str, category: SpaceCategory, r: Polygon2) -> ElementKind {
    ElementKind::Space(Space {
        boundary: r.into(),
        name: name.into(),
        number: number.into(),
        category,
        height: None,
        stamp: None,
        finish: String::new(),
    })
}

fn opening(host: Id, kind: OpeningKind, style: &str, offset: f64, w: f64, h: f64, sill: f64) -> ElementKind {
    ElementKind::Opening(Opening {
        host,
        kind,
        style: style.into(),
        offset: Length::m(offset),
        width: Length::m(w),
        height: Length::m(h),
        sill: Length::m(sill),
        flip_side: false,
        flip_hand: false,
        params: Default::default(),
    })
}

/// Exterior walls around `[x0, x1] × [y0, y1]`, clockwise (west, north, east, south) with the reference line on
/// the left face, so the build-up's first layers (render and insulation) face outwards and the outer faces lie
/// on the rectangle.
fn ring_of_walls(doc: &mut Document, storey: Id, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Id> {
    let build_up = doc.defaults().exterior_wall.unwrap_or_default();
    let corners = [(x0, y0), (x0, y1), (x1, y1), (x1, y0)];
    (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            add(
                doc,
                storey,
                ElementKind::Wall(Wall {
                    start: Point2::new(a.0, a.1),
                    end: Point2::new(b.0, b.1),
                    bulge: 0.0,
                    reference: ReferenceLine::Left,
                    structure: Structure::Layered { build_up },
                    height: Length::m(3.2),
                    base_offset: Length::ZERO,
                }),
            )
        })
        .collect()
}

/// The sample house: a 22 × 27 m plot facing a street to the south, a 12 × 9 m two-storey house with a
/// semi-open porch, a balcony, a stair and a flat roof with a parapet.
pub fn house() -> Document {
    let mut doc = starter();
    doc = doc.with_info(ProjectInfo {
        name: "Νέα διώροφη κατοικία".into(),
        location: "Οδός Αριστομένους 12".into(),
        municipality: "Δήμος Καλαμάτας".into(),
        owner: "Μαρία Παπαδοπούλου".into(),
        designer: "Γιώργος Νικολάου, αρχιτέκτων μηχανικός".into(),
        designer_details: "Α.Μ. ΤΕΕ 123456\nΦαρών 45, Καλαμάτα".into(),
        date: "Οκτώβριος 2026".into(),
        revision: "0".into(),
        notes: String::new(),
    });
    doc = doc.with_site(Site {
        plot: Some(Polygon2::from_m(&[(0.0, 0.0), (22.0, 0.0), (23.0, 27.0), (-1.0, 26.0)])),
        street_lines: vec![vec![Point2::new(-6.0, 0.0), Point2::new(28.0, 0.0)]],
        building_lines: vec![vec![Point2::new(-6.0, 4.0), Point2::new(28.0, 4.0)]],
        terms: BuildingTerms {
            building_coefficient: 0.8,
            coverage_ratio: 0.6,
            max_height: Length::m(11.0),
            max_floors: Some(3),
            volume_coefficient: None,
            setback: None,
        },
        georef: Georef {
            epsg: 2100,
            easting: 372_450.0,
            northing: 4_100_820.0,
            rotation: Angle::deg(12.0),
            altitude: 18.40,
        },
        terrain: vec![
            Point3::new(0.0, 0.0, -0.55),
            Point3::new(22.0, 0.0, -0.45),
            Point3::new(23.0, 27.0, 0.35),
            Point3::new(-1.0, 26.0, 0.25),
            Point3::new(11.0, 20.0, -0.10),
        ],
        ground_level: Length::m(-0.30),
    });
    let storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
    let (g, u) = (storeys[0], storeys[1]);
    let concrete = doc.defaults().structural_material.unwrap_or_default();
    let slab_bu = doc.defaults().floor_slab.unwrap_or_default();
    let roof_bu = doc.defaults().flat_roof.unwrap_or_default();
    let inner_bu = doc.defaults().interior_wall.unwrap_or_default();
    let ground_bu = doc.defaults().ground_slab.unwrap_or_default();

    // Ground floor.
    // walls: [west, north, east, south]; offsets run from each wall's start (clockwise).
    let walls = ring_of_walls(&mut doc, g, 5.0, 8.0, 17.0, 17.0);
    let d = add(&mut doc, g, opening(walls[3], OpeningKind::Door, "door.single_swing", 7.5, 1.0, 2.2, 0.0));
    tag(&mut doc, d, "Θ1");
    add(&mut doc, g, opening(walls[3], OpeningKind::Window, "window.casement_2", 3.5, 1.6, 1.4, 0.9));
    add(&mut doc, g, opening(walls[2], OpeningKind::Window, "window.casement_2", 4.5, 1.2, 1.4, 0.9));
    add(&mut doc, g, opening(walls[1], OpeningKind::Window, "window.casement_2", 3.0, 1.2, 1.4, 0.9));
    add(&mut doc, g, opening(walls[1], OpeningKind::Window, "window.casement_1", 9.0, 0.8, 1.2, 1.0));
    add(&mut doc, g, opening(walls[0], OpeningKind::Window, "window.casement_2", 4.5, 1.2, 1.4, 0.9));
    add(
        &mut doc,
        g,
        ElementKind::Wall(Wall {
            start: Point2::new(11.0, 8.31),
            end: Point2::new(11.0, 16.69),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Layered { build_up: inner_bu },
            height: Length::m(3.2),
            base_offset: Length::ZERO,
        }),
    );
    add(&mut doc, g, space("Καθιστικό", "0.01", SpaceCategory::MainUse, rect(5.31, 8.31, 10.935, 16.69)));
    add(&mut doc, g, space("Κουζίνα", "0.02", SpaceCategory::MainUse, rect(11.065, 11.5, 16.69, 16.69)));
    add(
        &mut doc,
        g,
        space("Κλιμακοστάσιο", "0.03", SpaceCategory::Stairwell, rect(11.065, 8.31, 16.69, 11.5)),
    );
    add(&mut doc, g, space("Στέγαστρο εισόδου", "0.04", SpaceCategory::SemiOpen, rect(7.0, 5.5, 11.0, 8.0)));
    for x in [7.15, 10.85] {
        add(
            &mut doc,
            g,
            ElementKind::Column(Column {
                position: Point2::new(x, 5.65),
                rotation: Angle::ZERO,
                profile: Profile::Rectangle { width: Length::m(0.3), depth: Length::m(0.3) },
                material: concrete,
                height: Length::m(3.2),
                base_offset: Length::ZERO,
            }),
        );
    }
    add(
        &mut doc,
        g,
        ElementKind::Stair(Stair {
            origin: Point2::new(11.5, 9.0),
            direction: Angle::ZERO,
            shape: StairShape::Straight,
            width: Length::m(1.0),
            rise: Length::m(3.2),
            risers: 18,
            going: Length::m(0.28),
            first_flight_risers: None,
            gap: Length::ZERO,
            thickness: Length::m(0.15),
            material: concrete,
            base_offset: Length::ZERO,
        }),
    );
    add(
        &mut doc,
        g,
        ElementKind::Slab(Slab {
            outline: rect(5.0, 8.0, 17.0, 17.0).into(),
            structure: Structure::Layered { build_up: ground_bu },
            top_offset: Length::ZERO,
        }),
    );

    // First floor.
    let walls = ring_of_walls(&mut doc, u, 5.0, 8.0, 17.0, 17.0);
    let bd = add(&mut doc, u, opening(walls[3], OpeningKind::Door, "door.single_swing", 3.0, 0.9, 2.2, 0.0));
    tag(&mut doc, bd, "Θ2");
    add(&mut doc, u, opening(walls[3], OpeningKind::Window, "window.casement_2", 9.0, 1.2, 1.4, 0.9));
    add(&mut doc, u, opening(walls[2], OpeningKind::Window, "window.casement_2", 4.5, 1.2, 1.4, 0.9));
    add(&mut doc, u, opening(walls[1], OpeningKind::Window, "window.casement_2", 3.0, 1.2, 1.4, 0.9));
    add(&mut doc, u, opening(walls[0], OpeningKind::Window, "window.casement_2", 4.5, 1.2, 1.4, 0.9));
    add(&mut doc, u, space("Υπνοδωμάτιο 1", "1.01", SpaceCategory::MainUse, rect(5.31, 8.31, 10.935, 16.69)));
    add(
        &mut doc,
        u,
        space("Υπνοδωμάτιο 2", "1.02", SpaceCategory::MainUse, rect(11.065, 11.5, 16.69, 16.69)),
    );
    add(
        &mut doc,
        u,
        space("Κλιμακοστάσιο", "1.03", SpaceCategory::Stairwell, rect(11.065, 8.31, 16.69, 11.5)),
    );
    add(&mut doc, u, space("Εξώστης", "1.04", SpaceCategory::Balcony, rect(12.0, 6.5, 16.0, 8.0)));
    add(
        &mut doc,
        u,
        ElementKind::Slab(Slab {
            outline: Polygon2::from_m(&[
                (5.0, 8.0),
                (7.0, 8.0),
                (7.0, 5.5),
                (11.0, 5.5),
                (11.0, 8.0),
                (12.0, 8.0),
                (12.0, 6.5),
                (16.0, 6.5),
                (16.0, 8.0),
                (17.0, 8.0),
                (17.0, 17.0),
                (5.0, 17.0),
            ])
            .into(),
            structure: Structure::Layered { build_up: slab_bu },
            top_offset: Length::ZERO,
        }),
    );
    add(
        &mut doc,
        u,
        ElementKind::Roof(Roof {
            outline: rect(5.0, 8.0, 17.0, 17.0),
            base: Length::m(3.2),
            structure: Structure::Layered { build_up: roof_bu },
            shape: RoofShape::Flat {
                parapet: Some(Parapet { height: Length::m(0.6), thickness: Length::m(0.2) }),
            },
        }),
    );
    doc
}
