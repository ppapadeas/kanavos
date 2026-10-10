//! Shared fixtures for the IFC tests: houses built with the model API.

#![allow(dead_code, clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use kanavos_geom::{Angle, Length, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Beam, Column, Document, Element, ElementKind, Georef, Id, Object, Opening, OpeningKind, Parapet, Profile,
    Railing, RailingInfill, ReferenceLine, Roof, RoofShape, Site, Slab, Space, SpaceCategory, Stair,
    StairShape, Structure, Wall, starter,
};

pub fn add(doc: &Document, storey: Id, kind: ElementKind) -> (Document, Id) {
    let (doc, id) = doc.with_new_id();
    (doc.with_element(Element::on_storey(id, storey, kind)), id)
}

pub struct House {
    pub doc: Document,
    pub ground: Id,
    pub first: Id,
    pub walls: Vec<Id>,
    pub door: Id,
    pub window: Id,
    pub slab: Id,
    pub space: Id,
    pub column: Id,
    pub roof: Id,
}

pub const CORNERS: [(f64, f64); 4] = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];

/// The small house of the acceptance tests: two storeys from the starter template, four exterior walls on the
/// ground floor with a door and a window, a floor slab with a shaft, a living room, a column and a flat roof
/// with a parapet on the first floor.
pub fn small_house() -> House {
    let doc = starter();
    let storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
    let (ground, first) = (storeys[0], storeys[1]);
    let ext = doc.defaults().exterior_wall.unwrap();
    let concrete = doc.defaults().structural_material.unwrap();
    let mut doc = doc;
    let mut walls = Vec::new();
    for i in 0..4 {
        let (a, b) = (CORNERS[i], CORNERS[(i + 1) % 4]);
        let (d, id) = add(
            &doc,
            ground,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Layered { build_up: ext },
                height: Length::m(3.2),
                base_offset: Length::ZERO,
            }),
        );
        doc = d;
        walls.push(id);
    }
    let (d, door) = add(
        &doc,
        ground,
        ElementKind::Opening(Opening {
            host: walls[0],
            kind: OpeningKind::Door,
            style: "door.single_swing".into(),
            offset: Length::m(2.5),
            width: Length::m(1.0),
            height: Length::m(2.2),
            sill: Length::ZERO,
            flip_side: false,
            flip_hand: true,
            params: BTreeMap::from([("frame".to_owned(), 0.06)]),
        }),
    );
    let (d, window) = add(
        &d,
        ground,
        ElementKind::Opening(Opening {
            host: walls[1],
            kind: OpeningKind::Window,
            style: "window.casement".into(),
            offset: Length::m(4.0),
            width: Length::m(1.5),
            height: Length::m(1.4),
            sill: Length::m(0.9),
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
        }),
    );
    let (d, slab) = add(
        &d,
        ground,
        ElementKind::Slab(Slab {
            outline: PolygonWithHoles {
                outer: Polygon2::from_m(&[
                    (-0.155, -0.155),
                    (10.155, -0.155),
                    (10.155, 8.155),
                    (-0.155, 8.155),
                ]),
                holes: vec![Polygon2::from_m(&[(7.0, 5.0), (8.0, 5.0), (8.0, 7.0), (7.0, 7.0)])],
            },
            structure: Structure::Layered { build_up: d.defaults().ground_slab.unwrap() },
            top_offset: Length::ZERO,
        }),
    );
    let (d, space) = add(
        &d,
        ground,
        ElementKind::Space(Space {
            boundary: Polygon2::from_m(&[(0.155, 0.155), (9.845, 0.155), (9.845, 7.845), (0.155, 7.845)])
                .into(),
            name: "Καθιστικό".into(),
            number: "0.01".into(),
            category: SpaceCategory::MainUse,
            height: None,
            stamp: None,
            finish: "Πλακίδια".into(),
        }),
    );
    let (d, column) = add(
        &d,
        ground,
        ElementKind::Column(Column {
            position: Point2::new(5.0, 4.0),
            rotation: Angle::deg(30.0),
            profile: Profile::Rectangle { width: Length::cm(30.0), depth: Length::cm(50.0) },
            material: concrete,
            height: Length::m(3.2),
            base_offset: Length::ZERO,
        }),
    );
    let (d, roof) = add(
        &d,
        first,
        ElementKind::Roof(Roof {
            outline: Polygon2::from_m(&[(-0.3, -0.3), (10.3, -0.3), (10.3, 8.3), (-0.3, 8.3)]),
            base: Length::ZERO,
            structure: Structure::Layered { build_up: d.defaults().flat_roof.unwrap() },
            holes: Vec::new(),
            shape: RoofShape::Flat {
                parapet: Some(Parapet { height: Length::m(0.6), thickness: Length::cm(20.0) }),
            },
        }),
    );
    let mut info = d.info().clone();
    info.name = "Διώροφη κατοικία".into();
    info.location = "Καλαμάτα".into();
    let doc = d.with_info(info);
    House { doc, ground, first, walls, door, window, slab, space, column, roof }
}

/// The small house plus everything else the exporter handles: a pitched roof, a stair, a beam, a railing, a
/// curved wall with a window, objects, a void, an annotation, and georeferencing.
pub fn full_house() -> House {
    let mut h = small_house();
    let concrete = h.doc.defaults().structural_material.unwrap();
    let steel = h.doc.defaults().railing_material.unwrap();
    let (d, _) = add(
        &h.doc,
        h.ground,
        ElementKind::Stair(Stair {
            origin: Point2::new(7.5, 1.0),
            direction: Angle::deg(90.0),
            shape: StairShape::HalfTurnLeft,
            width: Length::m(1.0),
            rise: Length::m(3.2),
            risers: 18,
            going: Length::cm(28.0),
            first_flight_risers: None,
            gap: Length::cm(10.0),
            thickness: Length::cm(15.0),
            material: concrete,
            usage: Default::default(),
            base_offset: Length::ZERO,
        }),
    );
    let (d, _) = add(
        &d,
        h.ground,
        ElementKind::Beam(Beam {
            start: Point2::new(0.0, 4.0),
            end: Point2::new(10.0, 4.0),
            profile: Profile::Rectangle { width: Length::cm(25.0), depth: Length::cm(50.0) },
            material: concrete,
            top_offset: Length::m(3.2),
        }),
    );
    let (d, _) = add(
        &d,
        h.first,
        ElementKind::Railing(Railing {
            path: vec![Point2::new(0.0, 0.0), Point2::new(5.0, 0.0), Point2::new(5.0, 3.0)],
            height: Length::m(1.0),
            base_offset: Length::ZERO,
            post_spacing: Length::m(1.2),
            infill: RailingInfill::Bars,
            elevations: Vec::new(),
            material: steel,
            infill_material: None,
        }),
    );
    let (d, curved) = add(
        &d,
        h.first,
        ElementKind::Wall(Wall {
            start: Point2::new(12.0, 0.0),
            end: Point2::new(16.0, 0.0),
            bulge: 0.5,
            reference: ReferenceLine::Left,
            structure: Structure::Basic { material: concrete, thickness: Length::cm(25.0) },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        }),
    );
    let (d, _) = add(
        &d,
        h.first,
        ElementKind::Opening(Opening {
            host: curved,
            kind: OpeningKind::Window,
            style: "window.fixed".into(),
            offset: Length::m(2.0),
            width: Length::m(1.0),
            height: Length::m(1.2),
            sill: Length::m(1.0),
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
        }),
    );
    let (d, _) = add(
        &d,
        h.ground,
        ElementKind::Opening(Opening {
            host: h.walls[2],
            kind: OpeningKind::Void,
            style: "void".into(),
            offset: Length::m(7.0),
            width: Length::m(0.8),
            height: Length::m(0.8),
            sill: Length::m(1.5),
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
        }),
    );
    let (d, _) = add(
        &d,
        h.first,
        ElementKind::Roof(Roof {
            outline: Polygon2::from_m(&[(11.5, -1.0), (17.0, -1.0), (17.0, 3.0), (11.5, 3.0)]),
            base: Length::m(3.0),
            structure: Structure::Layered { build_up: d.defaults().pitched_roof.unwrap() },
            holes: Vec::new(),
            shape: RoofShape::Pitched { slopes: vec![Some(Angle::deg(25.0)); 4] },
        }),
    );
    let mut doc = d;
    for (def, x) in [("furniture.bed_double", 2.0), ("sanitary.wc", 4.0), ("equipment.boiler", 6.0)] {
        let (d, _) = add(
            &doc,
            h.ground,
            ElementKind::Object(Object {
                definition: def.into(),
                position: Point2::new(x, 6.0),
                rotation: Angle::deg(45.0),
                mirrored: x > 5.0,
                elevation: Length::ZERO,
                params: BTreeMap::from([("width".to_owned(), 0.8)]),
            }),
        );
        doc = d;
    }
    let (d, _) = add(
        &doc,
        h.ground,
        ElementKind::Text(kanavos_model::TextNote {
            position: Point2::new(1.0, -2.0),
            content: "Σημείωση".into(),
            size_mm: 3.5,
            rotation: Angle::ZERO,
            align: Default::default(),
            bold: false,
        }),
    );
    let site = Site {
        georef: Georef {
            epsg: 2100,
            easting: 431_250.0,
            northing: 4_104_300.0,
            rotation: Angle::deg(12.0),
            altitude: 42.5,
        },
        plot: Some(Polygon2::from_m(&[(-5.0, -5.0), (25.0, -5.0), (25.0, 20.0), (-5.0, 20.0)])),
        ..Site::default()
    };
    h.doc = d.with_site(site);
    // A user property and a tag on a wall.
    let mut w = h.doc.element(h.walls[0]).unwrap().clone();
    w.tag = "Τ1".into();
    w.properties.insert("Σχόλιο".into(), "νότια όψη".into());
    h.doc = h.doc.with_element(w);
    h
}
