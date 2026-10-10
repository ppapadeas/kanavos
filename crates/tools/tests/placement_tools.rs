//! Scripted tests of the column, object, beam, stair, railing and opening tools (`specs/tools/placement.md`,
//! `specs/tools/openings.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use kanavos_engine::AnyCommand;
use kanavos_model::{ElementKind, OpeningKind, StairShape, StairUse};
use kanavos_tools::{Key, ToolId};

#[test]
fn column_click_drag_and_rotate_key() {
    let mut h = Harness::new(ToolId::Column);
    let out = h.move_to(1.0, 1.0);
    assert!(out.preview.items.iter().any(|i| i.layer.starts_with("tool.ghost.column")));
    h.click(1.0, 1.0);
    // Drag sets the rotation from the drag direction.
    h.drag((4.0, 1.0), (4.0, 3.0), NONE);
    // The rotate key turns the next column by 90°.
    h.key(Key::Rotate);
    h.click(7.0, 1.0);
    let cols: Vec<_> = h
        .of("column")
        .into_iter()
        .filter_map(|e| match e.kind {
            ElementKind::Column(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(cols.len(), 3);
    assert!(close(cols[0].position, p(1.0, 1.0)) && near_m(cols[0].rotation.as_deg(), 0.0));
    assert!(close(cols[1].position, p(4.0, 1.0)) && near_m(cols[1].rotation.as_deg(), 90.0));
    assert!(near_m(cols[2].rotation.as_deg(), 90.0));
    assert!(near_m(cols[0].height.as_m(), 3.2));
}

#[test]
fn column_with_pick_rotation_and_typed_degrees() {
    let mut h = Harness::new(ToolId::Column);
    h.settings.column.pick_rotation = true;
    h.click(0.0, 0.0);
    assert!(!h.tool.is_idle());
    h.click(-1.0, 1.0);
    h.click(5.0, 0.0);
    h.text("30");
    h.key(Key::Enter);
    let cols: Vec<_> = h
        .of("column")
        .into_iter()
        .filter_map(|e| match e.kind {
            ElementKind::Column(c) => Some(c),
            _ => None,
        })
        .collect();
    assert!(near_m(cols[0].rotation.as_deg(), 135.0));
    assert!(near_m(cols[1].rotation.as_deg(), 30.0));
}

#[test]
fn object_from_the_catalogue_with_rotation_and_mirror() {
    let mut h = Harness::new(ToolId::Object);
    h.settings.object = "sanitary.wc".into();
    h.key(Key::Rotate);
    h.key(Key::Flip);
    h.click(2.0, 2.0);
    let ElementKind::Object(o) = &h.of("object")[0].kind else { unreachable!() };
    assert_eq!(o.definition, "sanitary.wc");
    assert!(near_m(o.rotation.as_deg(), 90.0));
    assert!(o.mirrored);
}

#[test]
fn beams_chain_and_backspace_removes_the_last() {
    let mut h = Harness::new(ToolId::Beam);
    h.click(0.0, 0.0);
    h.click(5.0, 0.0);
    h.click(5.0, 5.0);
    assert_eq!(h.of("beam").len(), 2);
    let out = h.key(Key::Backspace);
    assert!(matches!(out.commands.as_slice(), [AnyCommand::DeleteElements(_)]));
    assert_eq!(h.of("beam").len(), 1);
    let ElementKind::Beam(b) = &h.of("beam")[0].kind else { unreachable!() };
    assert!(near_m(b.top_offset.as_m(), 3.2));
}

#[test]
fn stair_from_origin_and_direction() {
    let mut h = Harness::new(ToolId::Stair);
    let out = h.move_to(1.0, 1.0);
    assert!(out.prompt.contains("risers"), "{}", out.prompt);
    h.click(1.0, 1.0);
    h.move_to(1.0, 5.0);
    h.click(1.0, 5.0);
    let ElementKind::Stair(s) = &h.of("stair")[0].kind else { unreachable!() };
    assert!(close(s.origin, p(1.0, 1.0)));
    assert!(near_m(s.direction.as_deg(), 90.0));
    assert_eq!(s.risers, 18, "3.20 m at most 18 cm per riser");
    assert!(s.riser_height().as_m() <= 0.18 + 1e-9);
    assert_eq!(s.shape, StairShape::Straight);
    assert_eq!(s.usage, StairUse::DwellingInternal, "the default use");
}

#[test]
fn half_turn_stair_with_typed_direction() {
    let mut h = Harness::new(ToolId::Stair);
    h.settings.stair.shape = StairShape::HalfTurnLeft;
    h.settings.stair.usage = StairUse::Residential;
    h.click(0.0, 0.0);
    h.text("0");
    h.key(Key::Enter);
    let ElementKind::Stair(s) = &h.of("stair")[0].kind else { unreachable!() };
    assert!(near_m(s.direction.as_deg(), 0.0));
    assert_eq!(s.shape, StairShape::HalfTurnLeft);
    assert_eq!(s.usage, StairUse::Residential, "the use comes from the tool settings");
}

#[test]
fn railing_along_a_path() {
    let mut h = Harness::new(ToolId::Railing);
    h.click(0.0, 0.0);
    h.click(3.0, 0.0);
    h.click(3.0, 2.0);
    h.key(Key::Enter);
    let ElementKind::Railing(r) = &h.of("railing")[0].kind else { unreachable!() };
    assert_eq!(r.path.len(), 3);
    assert!(near_m(r.height.as_m(), 1.0));
    assert!(r.elevations.is_empty(), "a drawn railing is flat");
    assert_eq!(r.infill_material, None, "bars use the frame's material");
}

#[test]
fn glass_railings_get_the_glass_material() {
    let mut h = Harness::new(ToolId::Railing);
    h.settings.railing.infill = kanavos_model::RailingInfill::Glass;
    h.click(0.0, 0.0);
    h.click(3.0, 0.0);
    h.key(Key::Enter);
    let ElementKind::Railing(r) = &h.of("railing")[0].kind else { unreachable!() };
    let glass = h.doc.materials().find(|m| m.category == kanavos_model::MaterialCategory::Glass).unwrap();
    assert_eq!(r.infill_material, Some(glass.id));
}

fn opening(h: &Harness, i: usize) -> kanavos_model::Opening {
    h.of("opening")[i].as_opening().cloned().unwrap()
}

/// Whether a placed opening faces its wall's left side (looking from start to end), judged from its drawn symbol:
/// for a door, the side its leaf opens to; for a window, the side of its frame, which stands near the outer face.
fn faces_left(h: &Harness, i: usize) -> bool {
    let e = &h.of("opening")[i];
    let o = e.as_opening().unwrap();
    let pl = kanavos_elements::opening_placement(&h.doc, e.id).unwrap();
    let ctx =
        kanavos_objects::OpeningContext { opening: o, left_face: pl.left_face, right_face: pl.right_face };
    let layer = if o.kind == OpeningKind::Door { "door.symbol" } else { "window.symbol" };
    let ys: Vec<f64> = kanavos_objects::opening_symbol(&ctx)
        .iter()
        .filter(|i| i.layer == layer)
        .flat_map(|i| i.shape.extent_points())
        .map(|q| q.y)
        .collect();
    let lo = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(lo.is_finite() && hi.is_finite(), "opening {i} draws nothing on {layer}");
    let mid = (lo + hi) / 2.0;
    assert!(mid.abs() > 0.01, "opening {i} is centred in its wall");
    mid > 0.0
}

#[test]
fn door_snaps_to_steps_centre_and_ends() {
    let mut h = Harness::new(ToolId::Door).with_snapping();
    let w = h.add_wall((0.0, 0.0), (6.0, 0.0));
    // Near the start: the jamb distance rounds to 5 cm (pointer at 1.03 → jamb 0.58 → 0.60).
    let out = h.move_to(1.03, 0.05);
    assert_eq!(out.highlight, vec![w]);
    assert!(out.snap.as_ref().unwrap().label.contains("5 cm step"), "{:?}", out.snap);
    h.click(1.03, 0.05);
    let o = opening(&h, 0);
    assert_eq!(o.host, w);
    assert_eq!(o.kind, OpeningKind::Door);
    assert!(near_m(o.offset.as_m(), 0.60 + 0.45), "{}", o.offset.as_m());
    // The pointer is on the wall's left side: the door opens to that side, which needs the side flipped.
    assert!(o.flip_side, "pointer on the left side");
    assert!(faces_left(&h, 0), "opens to the left side");
    // Centre of the wall.
    h.click(3.04, -0.05);
    let o = opening(&h, 1);
    assert!(near_m(o.offset.as_m(), 3.0));
    assert!(!o.flip_side, "pointer on the right side: the default");
    assert!(!faces_left(&h, 1), "opens to the right side");
    // Flush with the end.
    h.click(5.52, 0.1);
    let o = opening(&h, 2);
    assert!(near_m(o.offset.as_m(), 6.0 - 0.45));
}

#[test]
fn typed_distance_from_the_nearest_end_and_width() {
    let mut h = Harness::new(ToolId::Window).with_snapping();
    h.add_wall((0.0, 0.0), (6.0, 0.0));
    h.move_to(4.8, 0.05);
    h.text("0,5");
    h.key(Key::Tab);
    h.text("1.6");
    h.key(Key::Enter);
    let o = opening(&h, 0);
    assert_eq!(o.kind, OpeningKind::Window);
    assert!(near_m(o.width.as_m(), 1.6));
    assert!(near_m(o.offset.as_m(), 6.0 - 0.5 - 0.8), "{}", o.offset.as_m());
    assert!(near_m(o.sill.as_m(), 0.9));
    // The pointer was on the wall's left side: that side becomes the outer face, which for windows is the left face
    // unless flipped.
    assert!(!o.flip_side);
    assert!(faces_left(&h, 0));
}

#[test]
fn the_pointer_side_sets_a_windows_outer_face() {
    let mut h = Harness::new(ToolId::Window).with_snapping();
    h.add_wall((0.0, 0.0), (6.0, 0.0));
    h.click(2.0, -0.1);
    assert!(opening(&h, 0).flip_side, "outer face on the right face: flipped");
    assert!(!faces_left(&h, 0));
    h.click(4.5, 0.1);
    assert!(!opening(&h, 1).flip_side, "outer face on the left face: the default");
    assert!(faces_left(&h, 1));
}

#[test]
fn usual_clicks_on_a_clockwise_ring_give_unflipped_doors_and_windows() {
    // Exterior walls drawn clockwise have their left face, and the build-up's first layer, outside. Clicking
    // outside for a window and inside for a door places both unflipped: windows face out, doors open in.
    let mut h = Harness::new(ToolId::Window).with_snapping();
    let south = h.add_wall((6.0, 0.0), (0.0, 0.0));
    let east = h.add_wall((6.0, 5.0), (6.0, 0.0));
    h.click(3.0, -0.2);
    h.click(6.2, 2.5);
    h.tool = kanavos_tools::make_tool(ToolId::Door);
    h.click(1.5, 0.2);
    h.click(5.8, 4.2);
    let all = h.of("opening");
    assert_eq!(all.len(), 4);
    for (i, e) in all.iter().enumerate() {
        let o = e.as_opening().unwrap();
        assert!(!o.flip_side, "{:?} {i}", o.kind);
        assert!(o.host == south || o.host == east);
        // The left side of these walls is outside.
        assert_eq!(faces_left(&h, i), o.kind == OpeningKind::Window, "{:?} {i}", o.kind);
    }
}

#[test]
fn voids_are_never_flipped() {
    let mut h = Harness::new(ToolId::Void).with_snapping();
    h.add_wall((0.0, 0.0), (6.0, 0.0));
    h.click(1.5, 0.1);
    h.click(4.5, -0.1);
    assert!(!opening(&h, 0).flip_side && !opening(&h, 1).flip_side);
}

#[test]
fn overlapping_openings_are_refused_and_flip_swaps_the_hinge() {
    let mut h = Harness::new(ToolId::Door).with_snapping();
    h.add_wall((0.0, 0.0), (6.0, 0.0));
    h.key(Key::Flip);
    h.click(3.0, 0.05);
    assert!(opening(&h, 0).flip_hand);
    let out = h.click(3.3, 0.05);
    assert!(out.commands.is_empty());
    assert!(out.warning.unwrap().contains("overlaps"));
    // Too wide for the wall.
    let mut h = Harness::new(ToolId::Void).with_snapping();
    h.add_wall((0.0, 0.0), (0.8, 0.0));
    let out = h.click(0.4, 0.0);
    assert!(out.commands.is_empty());
    assert!(out.warning.is_some());
}

#[test]
fn openings_need_a_wall() {
    let mut h = Harness::new(ToolId::Door).with_snapping();
    let out = h.click(3.0, 3.0);
    assert!(out.commands.is_empty());
    assert!(out.prompt.contains("hover a wall"));
}

#[test]
fn doors_in_curved_walls() {
    let mut h = Harness::new(ToolId::Door).with_snapping();
    let id = h.add_wall((0.0, 0.0), (4.0, 0.0));
    let mut w = h.doc.element(id).unwrap().as_wall().unwrap().clone();
    w.bulge = 1.0;
    h.doc = h.doc.with_element(kanavos_model::Element {
        kind: ElementKind::Wall(w),
        ..h.doc.element(id).unwrap().clone()
    });
    h.click(2.0, -2.0);
    let o = opening(&h, 0);
    assert_eq!(o.host, id);
    assert!(near_m(o.offset.as_m(), std::f64::consts::PI), "centre of the half circle: {}", o.offset.as_m());
}
