//! Shared fixtures: documents built with the model API.

#![allow(dead_code, clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;

use kanavos_drawing::Rgba;
use kanavos_geom::{Angle, Length, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Column, Document, Element, ElementKind, Id, Material, MaterialCategory, Object, Opening, OpeningKind,
    Profile, ReferenceLine, Roof, RoofShape, Slab, Space, SpaceCategory, Stair, StairShape, Structure,
    TextNote, Wall, starter,
};

/// Adds an element on a storey and returns the new document and the element id.
pub fn add(doc: &Document, storey: Id, kind: ElementKind) -> (Document, Id) {
    let (doc, id) = doc.with_new_id();
    (doc.with_element(Element::on_storey(id, storey, kind)), id)
}

pub fn wall(doc: &Document, a: (f64, f64), b: (f64, f64), thickness_cm: f64) -> Wall {
    let material = doc.materials().next().map(|m| m.id).unwrap_or(Id(1));
    Wall {
        start: Point2::new(a.0, a.1),
        end: Point2::new(b.0, b.1),
        bulge: 0.0,
        reference: ReferenceLine::Centre,
        structure: Structure::Basic { material, thickness: Length::cm(thickness_cm) },
        height: Length::m(3.0),
        base_offset: Length::ZERO,
    }
}

/// The starter template with a small furnished ground floor: four walls with a door and a window, a slab, a
/// space, a column, a stair, a roof, an object and a note with Greek text.
pub fn house() -> Document {
    let doc = starter();
    let ground = doc.storeys().first().map(|s| s.id).unwrap_or(Id(0));
    let ext = doc.defaults().exterior_wall.unwrap_or(Id(0));
    let concrete = doc.defaults().structural_material.unwrap_or(Id(0));
    let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let mut doc = doc;
    let mut walls = Vec::new();
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let mut w = wall(&doc, a, b, 31.0);
        w.structure = Structure::Layered { build_up: ext };
        let (d, id) = add(&doc, ground, ElementKind::Wall(w));
        doc = d;
        walls.push(id);
    }
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Opening(Opening {
            host: walls[0],
            kind: OpeningKind::Door,
            style: "door.single_swing".into(),
            offset: Length::m(2.0),
            width: Length::m(0.9),
            height: Length::m(2.1),
            sill: Length::ZERO,
            flip_side: false,
            flip_hand: true,
            params: BTreeMap::from([("frame".to_owned(), 0.05)]),
        }),
    );
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Opening(Opening {
            host: walls[1],
            kind: OpeningKind::Window,
            style: "window.casement".into(),
            offset: Length::m(4.0),
            width: Length::m(1.2),
            height: Length::m(1.4),
            sill: Length::m(0.9),
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
        }),
    );
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Slab(Slab {
            outline: PolygonWithHoles {
                outer: Polygon2::from_m(&corners),
                holes: vec![Polygon2::from_m(&[(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)])],
            },
            structure: Structure::Layered { build_up: doc.defaults().ground_slab.unwrap_or(Id(0)) },
            top_offset: Length::ZERO,
        }),
    );
    doc = d;
    let (d, _) = add(
        &doc,
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
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Column(Column {
            position: Point2::new(5.0, 4.0),
            rotation: Angle::deg(15.0),
            profile: Profile::Rectangle { width: Length::cm(30.0), depth: Length::cm(40.0) },
            material: concrete,
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        }),
    );
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Stair(Stair {
            origin: Point2::new(7.0, 1.0),
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
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Roof(Roof {
            outline: Polygon2::from_m(&[(-0.5, -0.5), (10.5, -0.5), (10.5, 8.5), (-0.5, 8.5)]),
            base: Length::m(3.2),
            structure: Structure::Basic { material: concrete, thickness: Length::cm(20.0) },
            holes: Vec::new(),
            shape: RoofShape::Pitched {
                slopes: vec![Some(Angle::deg(30.0)), None, Some(Angle::deg(30.0)), None],
            },
        }),
    );
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Object(Object {
            definition: "furniture.bed_double".into(),
            position: Point2::new(3.0, 5.0),
            rotation: Angle::deg(90.0),
            mirrored: false,
            elevation: Length::ZERO,
            params: BTreeMap::from([("width".to_owned(), 1.6)]),
        }),
    );
    doc = d;
    let (d, _) = add(
        &doc,
        ground,
        ElementKind::Text(TextNote {
            position: Point2::new(1.0, -2.0),
            content: "Νέα κατοικία\n«Δοκιμή» — ½".into(),
            size_mm: 3.5,
            rotation: Angle::ZERO,
            align: Default::default(),
            bold: true,
        }),
    );
    doc = d;
    let (d, id) = doc.with_new_id();
    doc = d.with_material(Material {
        id,
        name: "Ειδικό υλικό".into(),
        category: MaterialCategory::Other,
        color: Rgba::new(1, 2, 3, 4),
        cut_pattern: Default::default(),
        cut_background: None,
        priority: 7,
        conductivity: None,
        density: Some(1234.5),
    });
    doc
}
