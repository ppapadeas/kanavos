//! Property tests: random documents survive save → load → save byte for byte.
//!
//! Documents start from the starter template and receive a random sequence of edits made with the model API
//! (new storeys, walls, slabs, spaces, columns, openings, materials, properties, deletions). Numbers cover the
//! awkward cases for JSON: negative zero, subnormals, extremes and arbitrary fractions.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;

use kanavos_drawing::Rgba;
use kanavos_format::{SaveOptions, from_bytes, to_bytes};
use kanavos_geom::{Angle, Length, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Column, Document, Element, ElementKind, Id, Material, Opening, OpeningKind, Phase, Profile,
    ReferenceLine, Slab, Space, SpaceCategory, Storey, Structure, Wall, starter,
};
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    Storey {
        name: String,
        elevation: f64,
        height: f64,
    },
    Wall {
        storey: usize,
        a: (f64, f64),
        b: (f64, f64),
        thickness: f64,
        height: f64,
        reference: u8,
        bulge: f64,
    },
    Slab {
        storey: usize,
        x: f64,
        y: f64,
        w: f64,
        d: f64,
        hole: bool,
        top: f64,
    },
    Space {
        storey: usize,
        x: f64,
        y: f64,
        w: f64,
        d: f64,
        name: String,
        number: String,
        category: u8,
    },
    Column {
        storey: usize,
        x: f64,
        y: f64,
        rotation: f64,
        round: bool,
        size: f64,
    },
    Opening {
        wall: usize,
        kind: u8,
        offset: f64,
        width: f64,
        height: f64,
        sill: f64,
        param: Option<(String, f64)>,
    },
    Material {
        name: String,
        rgba: (u8, u8, u8, u8),
        conductivity: Option<f64>,
    },
    Property {
        element: usize,
        key: String,
        value: String,
        tag: String,
        phase: u8,
    },
    Delete {
        element: usize,
    },
    Info {
        name: String,
        notes: String,
    },
}

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..12).prop_map(String::from_iter)
}

/// Any finite f64, biased towards the values that trip up text formats.
fn number() -> impl Strategy<Value = f64> {
    prop_oneof![
        6 => -1.0e3..1.0e3f64,
        1 => Just(-0.0),
        1 => Just(5e-324),
        1 => Just(f64::MIN_POSITIVE),
        1 => Just(1.0e300),
        1 => Just(-f64::MAX),
        1 => Just(0.1 + 0.2),
        1 => any::<f64>().prop_filter("finite", |v| v.is_finite()),
    ]
}

/// A finite f64 small enough that the arithmetic in [`apply`] (sums, scaling by 10) stays finite.
fn small() -> impl Strategy<Value = f64> {
    prop_oneof![8 => -1.0e3..1.0e3f64, 1 => Just(-0.0), 1 => Just(5e-324), 1 => Just(1.0e-9)]
}

fn point() -> impl Strategy<Value = (f64, f64)> {
    (number(), number())
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (text(), number(), number()).prop_map(|(name, elevation, height)| Op::Storey {
            name,
            elevation,
            height
        }),
        (any::<usize>(), point(), point(), number(), number(), 0u8..3, number()).prop_map(
            |(storey, a, b, thickness, height, reference, bulge)| Op::Wall {
                storey,
                a,
                b,
                thickness,
                height,
                reference,
                bulge
            }
        ),
        (any::<usize>(), small(), small(), small(), small(), any::<bool>(), number())
            .prop_map(|(storey, x, y, w, d, hole, top)| Op::Slab { storey, x, y, w, d, hole, top }),
        (any::<usize>(), small(), small(), small(), small(), text(), text(), 0u8..12).prop_map(
            |(storey, x, y, w, d, name, number, category)| Op::Space {
                storey,
                x,
                y,
                w,
                d,
                name,
                number,
                category
            }
        ),
        (any::<usize>(), small(), small(), number(), any::<bool>(), small()).prop_map(
            |(storey, x, y, rotation, round, size)| Op::Column { storey, x, y, rotation, round, size }
        ),
        (
            any::<usize>(),
            0u8..3,
            number(),
            number(),
            number(),
            number(),
            prop::option::of((text(), number()))
        )
            .prop_map(|(wall, kind, offset, width, height, sill, param)| Op::Opening {
                wall,
                kind,
                offset,
                width,
                height,
                sill,
                param
            }),
        (text(), any::<(u8, u8, u8, u8)>(), prop::option::of(small()))
            .prop_map(|(name, rgba, conductivity)| Op::Material { name, rgba, conductivity }),
        (any::<usize>(), text(), text(), text(), 0u8..3)
            .prop_map(|(element, key, value, tag, phase)| Op::Property { element, key, value, tag, phase }),
        any::<usize>().prop_map(|element| Op::Delete { element }),
        (text(), text()).prop_map(|(name, notes)| Op::Info { name, notes }),
    ]
}

fn pick<T: Copy>(items: &[T], i: usize) -> Option<T> {
    if items.is_empty() { None } else { items.get(i % items.len()).copied() }
}

fn rect(x: f64, y: f64, w: f64, d: f64) -> Polygon2 {
    Polygon2::from_m(&[(x, y), (x + w, y), (x + w, y + d), (x, y + d)])
}

fn apply(doc: Document, op: &Op) -> Document {
    let storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
    let elements: Vec<Id> = doc.elements().map(|e| e.id).collect();
    let walls: Vec<Id> = doc.elements().filter(|e| e.as_wall().is_some()).map(|e| e.id).collect();
    let material = doc.materials().next().map(|m| m.id).unwrap_or(Id(0));
    match op {
        Op::Storey { name, elevation, height } => {
            let (doc, id) = doc.with_new_id();
            doc.with_storey(Storey::new(id, name.clone(), Length::m(*elevation), Length::m(*height)))
        }
        Op::Wall { storey, a, b, thickness, height, reference, bulge } => {
            let Some(s) = pick(&storeys, *storey) else { return doc };
            let reference = match reference {
                0 => ReferenceLine::Left,
                1 => ReferenceLine::Centre,
                _ => ReferenceLine::Right,
            };
            let structure = match doc.defaults().interior_wall {
                Some(b) if thickness.is_sign_negative() => Structure::Layered { build_up: b },
                _ => Structure::Basic { material, thickness: Length::m(*thickness) },
            };
            let wall = Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: *bulge,
                reference,
                structure,
                height: Length::m(*height),
                base_offset: Length::m(*bulge / 7.0),
            };
            common::add(&doc, s, ElementKind::Wall(wall)).0
        }
        Op::Slab { storey, x, y, w, d, hole, top } => {
            let Some(s) = pick(&storeys, *storey) else { return doc };
            let holes = if *hole { vec![rect(x + w / 4.0, y + d / 4.0, w / 4.0, d / 4.0)] } else { vec![] };
            let slab = Slab {
                outline: PolygonWithHoles { outer: rect(*x, *y, *w, *d), holes },
                structure: Structure::Basic { material, thickness: Length::m(top.abs()) },
                top_offset: Length::m(*top),
            };
            common::add(&doc, s, ElementKind::Slab(slab)).0
        }
        Op::Space { storey, x, y, w, d, name, number, category } => {
            let Some(s) = pick(&storeys, *storey) else { return doc };
            let category = [
                SpaceCategory::MainUse,
                SpaceCategory::Auxiliary,
                SpaceCategory::Stairwell,
                SpaceCategory::LiftShaft,
                SpaceCategory::SemiOpen,
                SpaceCategory::Balcony,
                SpaceCategory::Pilotis,
                SpaceCategory::Parking,
                SpaceCategory::Attic,
                SpaceCategory::Technical,
                SpaceCategory::Shaft,
                SpaceCategory::Other,
            ]
            .get(usize::from(*category))
            .copied()
            .unwrap_or_default();
            let space = Space {
                boundary: rect(*x, *y, *w, *d).into(),
                name: name.clone(),
                number: number.clone(),
                category,
                height: if *w > 0.0 { Some(Length::m(*d)) } else { None },
                stamp: if *d > 0.0 { Some(Point2::new(*x, *y)) } else { None },
                finish: name.chars().rev().collect(),
            };
            common::add(&doc, s, ElementKind::Space(space)).0
        }
        Op::Column { storey, x, y, rotation, round, size } => {
            let Some(s) = pick(&storeys, *storey) else { return doc };
            let profile = if *round {
                Profile::Circle { diameter: Length::m(*size) }
            } else {
                Profile::Rectangle { width: Length::m(*size), depth: Length::m(size * 1.5) }
            };
            let column = Column {
                position: Point2::new(*x, *y),
                rotation: Angle::rad(*rotation),
                profile,
                material,
                height: Length::m(*size * 10.0),
                base_offset: Length::m(*x - *y),
            };
            common::add(&doc, s, ElementKind::Column(column)).0
        }
        Op::Opening { wall, kind, offset, width, height, sill, param } => {
            let Some(w) = pick(&walls, *wall) else { return doc };
            let storey = doc.element(w).and_then(|e| e.storey);
            let Some(s) = storey else { return doc };
            let kind = match kind {
                0 => OpeningKind::Door,
                1 => OpeningKind::Window,
                _ => OpeningKind::Void,
            };
            let opening = Opening {
                host: w,
                kind,
                style: format!("{kind:?}").to_lowercase(),
                offset: Length::m(*offset),
                width: Length::m(*width),
                height: Length::m(*height),
                sill: Length::m(*sill),
                flip_side: *offset > 0.0,
                flip_hand: *width > 0.0,
                params: param.iter().cloned().collect::<BTreeMap<_, _>>(),
            };
            common::add(&doc, s, ElementKind::Opening(opening)).0
        }
        Op::Material { name, rgba, conductivity } => {
            let (doc, id) = doc.with_new_id();
            doc.with_material(Material {
                id,
                name: name.clone(),
                category: Default::default(),
                color: Rgba::new(rgba.0, rgba.1, rgba.2, rgba.3),
                cut_pattern: Default::default(),
                cut_background: if rgba.0 > 128 { Some(Rgba::rgb(rgba.1, rgba.2, rgba.3)) } else { None },
                priority: rgba.3,
                conductivity: *conductivity,
                density: conductivity.map(|c| c * 1000.0),
            })
        }
        Op::Property { element, key, value, tag, phase } => {
            let Some(id) = pick(&elements, *element) else { return doc };
            let Some(e) = doc.element(id) else { return doc };
            let mut e: Element = e.clone();
            e.properties.insert(key.clone(), value.clone());
            e.tag = tag.clone();
            e.phase = match phase {
                0 => Phase::New,
                1 => Phase::Existing,
                _ => Phase::Demolished,
            };
            doc.with_element(e)
        }
        Op::Delete { element } => {
            let Some(id) = pick(&elements, *element) else { return doc };
            doc.without_element(id).unwrap_or(doc)
        }
        Op::Info { name, notes } => {
            let mut info = doc.info().clone();
            info.name = name.clone();
            info.notes = notes.clone();
            doc.with_info(info)
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn random_documents_round_trip_byte_identically(ops in prop::collection::vec(op(), 0..40)) {
        let doc = ops.iter().fold(starter(), apply);
        prop_assert!(doc.validate().is_ok());
        let options = SaveOptions {
            created: Some("2026-01-02T03:04:05Z".into()),
            modified: Some("2026-10-09T12:00:00.5+03:00".into()),
            ..SaveOptions::default()
        };
        let first = to_bytes(&doc, &options).unwrap();
        let loaded = from_bytes(&first).unwrap();
        prop_assert_eq!(&loaded.document, &doc);
        prop_assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let second = to_bytes(&loaded.document, &options).unwrap();
        prop_assert_eq!(first, second);
    }
}
