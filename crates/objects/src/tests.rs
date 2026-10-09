//! Unit tests over the whole catalogue.

use std::collections::BTreeMap;

use kanavos_drawing::{Drawing, Item, LinePattern, Shape};
use kanavos_geom::Point2;
use kanavos_model::{Id, Object, Opening, OpeningKind};
use kanavos_units::{Angle, Length};

use super::*;

fn opening(def: &ObjectDef, flip_side: bool, flip_hand: bool) -> Opening {
    Opening {
        host: Id(1),
        kind: def.category.opening_kind().unwrap_or(OpeningKind::Void),
        style: def.id.into(),
        offset: Length::m(2.0),
        width: Length::m(def.width),
        height: Length::m(def.height),
        sill: Length::m(def.sill),
        flip_side,
        flip_hand,
        params: BTreeMap::new(),
    }
}

fn object(def: &ObjectDef) -> Object {
    Object {
        definition: def.id.into(),
        position: Point2::new(0.0, 0.0),
        rotation: Angle::ZERO,
        mirrored: false,
        elevation: Length::ZERO,
        params: BTreeMap::new(),
    }
}

/// A 30 cm wall on its centre line.
fn ctx(o: &Opening) -> OpeningContext<'_> {
    OpeningContext { opening: o, left_face: 0.15, right_face: -0.15 }
}

fn bounds(items: &[Item]) -> Rect {
    Drawing { items: items.to_vec(), ..Drawing::model(50.0) }.bounds().unwrap()
}

fn all_finite(items: &[Item]) -> bool {
    items.iter().all(|i| i.shape.extent_points().iter().all(|p| p.x.is_finite() && p.y.is_finite()))
}

fn openings() -> impl Iterator<Item = &'static ObjectDef> {
    catalogue().iter().filter(|d| d.category.is_opening())
}

fn objects() -> impl Iterator<Item = &'static ObjectDef> {
    catalogue().iter().filter(|d| !d.category.is_opening())
}

#[test]
fn catalogue_ids_are_unique_and_named() {
    let mut ids: Vec<&str> = catalogue().iter().map(|d| d.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n);
    for kind in [OpeningKind::Door, OpeningKind::Window, OpeningKind::Void] {
        assert!(find(default_style(kind)).is_some());
    }
    for d in catalogue() {
        assert_eq!(d.name, format!("objects.{}", d.id));
        let prefix = d.id.split('.').next().unwrap();
        assert!(
            [
                "door",
                "window",
                "void",
                "furniture",
                "kitchen",
                "sanitary",
                "equipment",
                "vegetation",
                "vehicle"
            ]
            .contains(&prefix)
        );
        assert!(d.ifc_class.starts_with("Ifc") && !d.ifc_type.is_empty());
    }
}

#[test]
fn catalogue_keeps_the_ids_documents_already_use() {
    for id in [
        "door.single_swing",
        "door.double_swing",
        "door.sliding",
        "window.casement_1",
        "window.casement_2",
        "window.sliding",
        "window.fixed",
        "void.rectangular",
        "furniture.bed_double",
        "furniture.bed_single",
        "furniture.table",
        "furniture.chair",
        "furniture.sofa",
        "furniture.wardrobe",
        "sanitary.wc",
        "sanitary.basin",
        "sanitary.shower",
        "sanitary.bath",
        "kitchen.counter",
        "kitchen.sink",
        "kitchen.hob",
        "kitchen.fridge",
        "vegetation.tree",
        "vehicle.car",
    ] {
        assert!(find(id).is_some(), "{id} missing");
    }
}

#[test]
fn every_text_key_is_in_the_message_catalogue() {
    let mut missing = Vec::new();
    for d in catalogue() {
        if !kanavos_i18n::has(d.name) {
            missing.push(d.name.to_owned());
        }
        for p in &d.params {
            if !kanavos_i18n::has(p.label) {
                missing.push(p.label.to_owned());
            }
        }
    }
    for c in ObjectCategory::ALL {
        if !kanavos_i18n::has(c.name_key()) {
            missing.push(c.name_key().to_owned());
        }
    }
    for k in ["objects.param.width", "objects.param.depth", "objects.param.height", "objects.param.sill"] {
        if !kanavos_i18n::has(k) {
            missing.push(k.to_owned());
        }
    }
    missing.sort();
    missing.dedup();
    assert!(missing.is_empty(), "missing catalogue keys: {missing:#?}");
}

#[test]
fn parameter_defaults_lie_in_their_ranges() {
    for d in catalogue() {
        for p in &d.params {
            assert!(p.min <= p.default && p.default <= p.max, "{}.{}", d.id, p.key);
            assert!((p.clamp(p.default) - p.default).abs() < 1e-12, "{}.{}", d.id, p.key);
        }
        assert!(d.width >= d.min_size[0] && d.width <= d.max_size[0], "{}", d.id);
        assert!(d.height >= d.min_size[2] && d.height <= d.max_size[2], "{}", d.id);
    }
    let p = ParamDef { key: "k", label: "l", kind: ParamKind::Count, default: 2.0, min: 1.0, max: 4.0 };
    assert!((p.clamp(f64::NAN) - 2.0).abs() < 1e-12);
    assert!((p.clamp(9.0) - 4.0).abs() < 1e-12);
    assert!((p.clamp(2.6) - 3.0).abs() < 1e-12);
}

#[test]
fn opening_symbols_are_drawn_on_their_layers_within_reach() {
    for def in openings() {
        let o = opening(def, false, false);
        let items = opening_symbol(&ctx(&o));
        assert!(!items.is_empty(), "{} draws nothing", def.id);
        assert!(all_finite(&items), "{}", def.id);
        let prefix = match def.category {
            ObjectCategory::Door => "door.symbol",
            ObjectCategory::Window => "window.symbol",
            _ => "opening.symbol",
        };
        for i in &items {
            assert!(i.layer.starts_with(prefix), "{}: layer {}", def.id, i.layer);
        }
        // Nothing reaches further than the opening's width (or height, for overhead garage tracks) beyond it.
        let b = bounds(&items);
        let reach = def.width + 0.2;
        assert!(
            b.min.x >= -def.width / 2.0 - reach && b.max.x <= def.width / 2.0 + reach,
            "{}: {b:?}",
            def.id
        );
        assert!(
            b.min.y >= -0.15 - reach && b.max.y <= 0.15 + reach.max(def.height + 0.5),
            "{}: {b:?}",
            def.id
        );
    }
}

#[test]
fn flipping_mirrors_the_symbol() {
    for def in openings() {
        let base = bounds(&opening_symbol(&ctx(&opening(def, false, false))));
        let hand = bounds(&opening_symbol(&ctx(&opening(def, false, true))));
        let side = bounds(&opening_symbol(&ctx(&opening(def, true, false))));
        let both = bounds(&opening_symbol(&ctx(&opening(def, true, true))));
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(hand.min.x, -base.max.x) && close(hand.max.x, -base.min.x), "{}", def.id);
        assert!(close(hand.min.y, base.min.y) && close(hand.max.y, base.max.y), "{}", def.id);
        assert!(close(side.min.y, -base.max.y) && close(side.max.y, -base.min.y), "{}", def.id);
        assert!(close(side.min.x, base.min.x) && close(side.max.x, base.max.x), "{}", def.id);
        assert!(close(both.min.x, -base.max.x) && close(both.min.y, -base.max.y), "{}", def.id);
    }
}

#[test]
fn doors_open_towards_the_left_face_by_default() {
    let def = find("door.single_swing").unwrap();
    let b = bounds(&opening_symbol(&ctx(&opening(def, false, false))));
    // The leaf, open at 90°, reaches about its width beyond the left face.
    assert!(b.max.y > 0.15 + 0.7, "{b:?}");
    assert!(b.min.y >= -0.15 - 1e-9, "{b:?}");
    // The hinge is at −x: the open leaf lies near x = −w/2.
    let leaf = opening_symbol(&ctx(&opening(def, false, false)))
        .into_iter()
        .filter(|i| i.layer == "door.symbol" && matches!(i.shape, Shape::Polyline { .. }))
        .map(|i| bounds(&[i]))
        .find(|r| r.max.y > 0.5)
        .unwrap();
    assert!(leaf.max.x < 0.0, "{leaf:?}");
}

#[test]
fn opening_parts_are_closed_solids_inside_reach() {
    for def in openings() {
        for (fs, fh) in [(false, false), (true, false), (false, true), (true, true)] {
            let o = opening(def, fs, fh);
            let parts = opening_parts(&ctx(&o));
            if def.id == "void.rectangular" {
                assert!(parts.is_empty());
                continue;
            }
            assert!(!parts.is_empty(), "{} has no parts", def.id);
            for p in &parts {
                assert!(p.mesh.volume() > 0.0, "{} part {:?} is inside out", def.id, p.color);
                let bb = p.mesh.bbox().unwrap();
                assert!(bb.min.z >= -1e-9 && bb.max.z <= def.height + 0.15, "{}: {bb:?}", def.id);
                let reach = def.width / 2.0 + def.width + 0.2;
                assert!(bb.min.x >= -reach && bb.max.x <= reach, "{}: {bb:?}", def.id);
            }
        }
    }
}

#[test]
fn windows_carry_sills_shutters_and_roller_boxes_on_the_outer_face() {
    let def = find("window.casement_2").unwrap();
    let mut o = opening(def, false, false);
    o.params.insert("shutters".into(), 1.0);
    o.params.insert("roller".into(), 1.0);
    let items = opening_symbol(&ctx(&o));
    let layer = |l: &str| items.iter().filter(|i| i.layer == l).cloned().collect::<Vec<_>>();
    let shutters = bounds(&layer("window.symbol.shutter"));
    assert!(shutters.max.y < -0.15, "shutters on the outside: {shutters:?}");
    assert!(shutters.min.x < -0.6 - 0.5 && shutters.max.x > 0.6 + 0.5);
    let above = layer("window.symbol.above");
    assert!(!above.is_empty());
    assert!(above.iter().all(|i| i.stroke.as_ref().is_some_and(|s| s.pattern == LinePattern::Dashed)));
    let sills = bounds(&layer("window.symbol.sill"));
    assert!(sills.min.y < -0.15 && sills.max.y > 0.15);
    // Flipping the side puts the outer face on the left.
    o.flip_side = true;
    let items = opening_symbol(&ctx(&o));
    let shutters =
        bounds(&items.into_iter().filter(|i| i.layer == "window.symbol.shutter").collect::<Vec<_>>());
    assert!(shutters.min.y > 0.15);
}

#[test]
fn overhead_symbols_are_dashed() {
    for def in openings() {
        let o = opening(def, false, false);
        let items = opening_symbol_overhead(&ctx(&o), 50.0);
        assert!(!items.is_empty(), "{}", def.id);
        assert!(items.iter().all(|i| i.layer.ends_with(".above")));
    }
}

#[test]
fn degenerate_and_wild_inputs_do_not_panic() {
    for def in openings() {
        let mut o = opening(def, false, false);
        for (w, h) in [(0.0, 2.0), (-1.0, 2.0), (f64::NAN, 2.0), (0.4, 0.1), (8.0, 6.0), (0.31, 0.31)] {
            o.width = Length::m(w);
            o.height = Length::m(h);
            for key in ["frame", "frame_depth", "leaf", "panes", "panels", "leaves", "setback", "sidelight"] {
                o.params.insert(key.into(), f64::INFINITY);
            }
            let items = opening_symbol_at(&ctx(&o), 100.0);
            assert!(all_finite(&items));
            for p in opening_parts(&ctx(&o)) {
                assert!(
                    p.mesh.positions.iter().all(|q| q.x.is_finite() && q.y.is_finite() && q.z.is_finite())
                );
            }
            let thin = OpeningContext { opening: &o, left_face: 0.0, right_face: 0.0 };
            assert!(all_finite(&opening_symbol(&thin)));
            let _ = opening_parts(&thin);
        }
    }
    for def in objects() {
        let mut obj = object(def);
        for v in [0.0, -3.0, f64::NAN, 1e9] {
            for key in
                ["width", "depth", "height", "seats", "chairs", "burners", "bowls", "doors", "trunk", "mount"]
            {
                obj.params.insert(key.into(), v);
            }
            assert!(all_finite(&object_symbol_at(&obj, 100.0)), "{}", def.id);
            for p in object_parts(&obj) {
                assert!(
                    p.mesh.positions.iter().all(|q| q.x.is_finite() && q.y.is_finite() && q.z.is_finite())
                );
            }
        }
    }
}

#[test]
fn object_symbols_and_parts_fit_their_footprint() {
    for def in objects() {
        let obj = object(def);
        let items = object_symbol(&obj);
        assert!(!items.is_empty() && all_finite(&items), "{}", def.id);
        assert!(items.iter().all(|i| i.layer.starts_with("object.symbol")), "{}", def.id);
        let fp = object_footprint(&obj);
        let (w, d, h) = object_size(&obj);
        assert!(fp.width() >= w - 1e-9 && fp.height() >= d - 1e-9, "{}", def.id);
        let parts = object_parts(&obj);
        assert!(!parts.is_empty(), "{}", def.id);
        for p in &parts {
            assert!(p.mesh.volume() > 0.0, "{} part {:?} is inside out", def.id, p.color);
            let bb = p.mesh.bbox().unwrap();
            let m = 0.15;
            assert!(bb.min.x >= fp.min.x - m && bb.max.x <= fp.max.x + m, "{}: {bb:?} vs {fp:?}", def.id);
            assert!(bb.min.y >= fp.min.y - m && bb.max.y <= fp.max.y + m, "{}: {bb:?} vs {fp:?}", def.id);
            assert!(bb.min.z >= -1e-9, "{}: {bb:?}", def.id);
            let top = match def.id {
                "kitchen.wall_cabinets" | "equipment.water_heater" => 2.4 + h,
                "furniture.garden_table" => 2.5,
                "furniture.table" | "furniture.table_round" => 0.9,
                "furniture.bed_single" | "furniture.bed_double" => h + 0.5,
                "furniture.nightstand" | "furniture.tv_unit" | "furniture.desk" => h + 0.9,
                "kitchen.sink" | "kitchen.sink_double" => h + 0.35,
                "kitchen.counter" => h + 0.06,
                "sanitary.basin" | "sanitary.vanity" | "sanitary.bidet" | "sanitary.wc_wall" => h + 0.75,
                _ => h + 0.02,
            };
            assert!(bb.max.z <= top, "{}: {bb:?}", def.id);
        }
    }
}

#[test]
fn object_origin_matches_the_definition() {
    for def in objects() {
        let fp = object_footprint(&object(def));
        match def.origin {
            Origin::FrontCentre => {
                assert!(fp.max.y > def.depth * 0.9 && fp.max.y < def.depth + 0.5, "{}", def.id);
            }
            Origin::Centre => assert!((fp.min.y + fp.max.y).abs() < def.depth.max(1.0), "{}", def.id),
        }
    }
}

#[test]
fn small_scales_draw_simpler_symbols() {
    let mut simpler = 0;
    for def in catalogue() {
        let (fine, coarse) = if def.category.is_opening() {
            let o = opening(def, false, false);
            (opening_symbol_at(&ctx(&o), 50.0).len(), opening_symbol_at(&ctx(&o), 100.0).len())
        } else {
            let obj = object(def);
            (object_symbol_at(&obj, 50.0).len(), object_symbol_at(&obj, 100.0).len())
        };
        assert!(coarse <= fine, "{}", def.id);
        if coarse < fine {
            simpler += 1;
        }
    }
    assert!(simpler > openings().count() / 2);
}

#[test]
fn parameters_change_the_geometry() {
    let def = find("window.sliding").unwrap();
    let mut o = opening(def, false, false);
    let two = opening_symbol(&ctx(&o)).len();
    o.params.insert("panes".into(), 4.0);
    assert!(opening_symbol(&ctx(&o)).len() > two);
    // A kind/style mismatch falls back to the kind's default style.
    o.kind = OpeningKind::Door;
    assert_eq!(opening_def(&o).map(|d| d.id), Some("door.single_swing"));
}

#[test]
fn unknown_definitions_draw_a_box() {
    let mut obj = object(find("furniture.chair").unwrap());
    obj.definition = "no.such.thing".into();
    assert!(!object_symbol(&obj).is_empty());
    assert_eq!(object_parts(&obj).len(), 1);
    assert_eq!(object_size(&obj), (1.0, 1.0, 1.0));
}

#[test]
fn the_swing_angle_turns_leaf_and_arc() {
    let def = find("door.single_swing").unwrap();
    let mut o = opening(def, false, false);
    o.params.insert("swing".into(), 45.0);
    let items = opening_symbol(&ctx(&o));
    let arc = items
        .iter()
        .find_map(|i| match i.shape {
            Shape::Arc { start, sweep, .. } if i.layer == "door.symbol.swing" => Some((start, sweep)),
            _ => None,
        })
        .unwrap();
    assert!(arc.0.abs() < 1e-9 && (arc.1 - 45_f64.to_radians()).abs() < 1e-9, "{arc:?}");
    // The open leaf leans at 45°: it reaches as far along the wall as across it.
    let leaf = items
        .iter()
        .filter(|i| i.layer == "door.symbol" && matches!(i.shape, Shape::Polyline { .. }))
        .map(|i| bounds(std::slice::from_ref(i)))
        .find(|r| r.max.y > 0.3)
        .unwrap();
    assert!(((leaf.max.y - 0.15) - (leaf.max.x - leaf.min.x)).abs() < 0.05, "{leaf:?}");
}

#[test]
fn lined_openings_get_a_sill_board_when_raised() {
    let def = find("void.lined").unwrap();
    let mut o = opening(def, false, false);
    let boards =
        |o: &Opening| opening_parts(&ctx(o)).iter().map(|p| p.mesh.triangles.len()).sum::<usize>() / 12;
    assert_eq!(boards(&o), 3);
    o.sill = Length::m(0.9);
    assert_eq!(boards(&o), 4);
    let rect = find("void.rectangular").unwrap();
    let items = opening_symbol(&ctx(&opening(rect, false, false)));
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.layer == "opening.symbol.above"));
}
