//! Scripted tests of the select tool: selection, handles, move, copy, rotate, mirror, delete
//! (`specs/tools/selection.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use kanavos_engine::AnyCommand;
use kanavos_model::{ElementKind, Opening, OpeningKind, Profile};
use kanavos_tools::{HandleRole, Key, ToolId};
use kanavos_units::Length;

fn wall(h: &Harness, id: kanavos_model::Id) -> kanavos_model::Wall {
    h.doc.element(id).unwrap().as_wall().unwrap().clone()
}

#[test]
fn click_shift_ctrl_and_empty_space() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    let b = h.add_wall((0.0, 3.0), (4.0, 3.0));
    let out = h.move_to(2.0, 0.05);
    assert_eq!(out.highlight, vec![a], "pre-highlight under the pointer");
    h.click(2.0, 0.05);
    assert_eq!(h.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
    h.click_mods(2.0, 3.0, SHIFT);
    assert_eq!(h.selection.len(), 2);
    // Ctrl on a handle (the wall's midpoint) still toggles the selection.
    h.click_mods(2.0, 0.0, CTRL);
    assert_eq!(h.selection.iter().copied().collect::<Vec<_>>(), vec![b]);
    h.click(10.0, 10.0);
    assert!(h.selection.is_empty());
}

#[test]
fn window_and_crossing_boxes() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    let b = h.add_wall((6.0, 0.0), (10.0, 0.0));
    // Left to right: only what is fully inside.
    let out = h.drag((-1.0, -1.0), (7.0, 1.0), NONE);
    assert!(out.commands.is_empty());
    assert_eq!(h.selection.iter().copied().collect::<Vec<_>>(), vec![a]);
    // Right to left: what the box touches.
    h.drag((7.0, 1.0), (-1.0, -1.0), NONE);
    assert_eq!(h.selection.len(), 2);
    assert!(h.selection.contains(&b));
}

#[test]
fn drag_moves_from_the_grabbed_point_with_snapping() {
    let mut h = Harness::new(ToolId::Select).with_snapping();
    h.add_wall((0.0, 0.0), (4.0, 0.0));
    let b = h.add_wall((10.0, 0.0), (10.0, 4.0));
    h.click(10.0, 2.0);
    // Pressing just outside the end handle (8 px away) grabs the selection at the wall end.
    let out = h.drag((10.0, 0.08), (4.03, 0.02), NONE);
    assert!(matches!(out.commands.as_slice(), [AnyCommand::TransformElements(_)]));
    let w = wall(&h, b);
    assert!(close(w.start, p(4.0, 0.0)), "{:?}", w.start);
    assert!(close(w.end, p(4.0, 4.0)));
}

#[test]
fn typed_move_distance_and_ctrl_copy() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    h.click(2.0, 0.0);
    // Drag upwards, type 2.5 while dragging, release, Enter.
    h.send(kanavos_tools::ToolEvent::Press {
        at: p(2.0, 0.0),
        button: kanavos_tools::MouseButton::Primary,
        mods: NONE,
    });
    h.move_to(2.0, 0.5);
    h.move_to(2.0, 1.0);
    let out = h.text("2.5");
    assert!(out.typing && out.fields[0].locked);
    h.send(kanavos_tools::ToolEvent::Release {
        at: p(2.0, 1.0),
        button: kanavos_tools::MouseButton::Primary,
        mods: NONE,
    });
    assert!(h.applied.is_empty(), "waits for Enter after typing");
    h.key(Key::Enter);
    assert!(close(wall(&h, a).start, p(0.0, 2.5)));
    // Ctrl-drag copies and selects the copy.
    let out = h.drag((2.0, 2.5), (2.0, 5.5), CTRL);
    assert!(matches!(&out.commands[..], [AnyCommand::TransformElements(t)] if t.copy));
    assert_eq!(h.walls().len(), 2);
    assert!(!h.selection.contains(&a));
}

#[test]
fn rotate_by_clicks_and_by_typed_degrees() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    h.click(2.0, 0.0);
    h.key(Key::Rotate);
    h.click(0.0, 0.0);
    h.click(4.0, 0.0);
    let out = h.move_to(0.0, 4.0);
    assert!(out.preview.items.iter().any(|i| i.layer.starts_with("tool.ghost.wall")));
    h.click(0.0, 4.0);
    let w = wall(&h, a);
    assert!(close(w.end, p(0.0, 4.0)), "{:?}", w.end);
    h.key(Key::Rotate);
    h.click(0.0, 0.0);
    h.text("-90");
    h.key(Key::Enter);
    assert!(close(wall(&h, a).end, p(4.0, 0.0)));
}

#[test]
fn mirror_about_an_axis_and_keep_the_original_with_ctrl() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    h.click(2.0, 0.0);
    h.key(Key::Mirror);
    h.click(5.0, -1.0);
    h.click_mods(5.0, 1.0, CTRL);
    assert_eq!(h.walls().len(), 2);
    assert!(close(wall(&h, a).start, p(0.0, 0.0)), "original kept");
    assert!(h.walls().iter().any(|w| close(w.start, p(6.0, 0.0)) && close(w.end, p(10.0, 0.0))));
}

#[test]
fn delete_and_escape() {
    let mut h = Harness::new(ToolId::Select);
    h.add_wall((0.0, 0.0), (4.0, 0.0));
    h.click(2.0, 0.0);
    h.key(Key::Escape);
    assert!(h.selection.is_empty());
    h.click(2.0, 0.0);
    let out = h.key(Key::Delete);
    assert!(matches!(out.commands.as_slice(), [AnyCommand::DeleteElements(_)]));
    assert!(h.walls().is_empty());
    assert!(h.selection.is_empty());
}

#[test]
fn wall_end_handle_stretches_and_connected_ends_follow() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    let b = h.add_wall((4.0, 0.0), (4.0, 3.0));
    h.click(2.0, 0.0);
    let out = h.move_to(4.0, 0.0);
    assert!(out.handles.iter().any(|x| x.hot && x.role == HandleRole::End && x.element == a));
    let out = h.drag((4.0, 0.0), (5.0, 0.0), NONE);
    assert_eq!(out.commands.len(), 1, "one undo step");
    assert!(close(wall(&h, a).end, p(5.0, 0.0)));
    assert!(close(wall(&h, b).start, p(5.0, 0.0)), "connected wall follows");
    // A typed length sets the wall's new length.
    h.send(kanavos_tools::ToolEvent::Press {
        at: p(5.0, 0.0),
        button: kanavos_tools::MouseButton::Primary,
        mods: NONE,
    });
    h.move_to(5.5, 0.0);
    h.text("3");
    h.key(Key::Enter);
    assert!(close(wall(&h, a).end, p(3.0, 0.0)), "{:?}", wall(&h, a).end);
}

#[test]
fn wall_midpoint_handle_moves_the_wall_sideways() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (4.0, 0.0));
    let b = h.add_wall((4.0, 0.0), (4.0, 3.0));
    h.click(1.0, 0.0);
    h.drag((2.0, 0.0), (2.5, 1.0), NONE);
    let w = wall(&h, a);
    assert!(close(w.start, p(0.0, 1.0)) && close(w.end, p(4.0, 1.0)), "{w:?}");
    assert!(close(wall(&h, b).start, p(4.0, 1.0)));
}

#[test]
fn stretching_keeps_openings_and_refuses_cutting_them_off() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (6.0, 0.0));
    let d = h.add(ElementKind::Opening(Opening {
        host: a,
        kind: OpeningKind::Door,
        style: "door.single_swing".into(),
        offset: Length::m(4.0),
        width: Length::m(0.9),
        height: Length::m(2.1),
        sill: Length::ZERO,
        flip_side: false,
        flip_hand: false,
        params: Default::default(),
    }));
    h.click(1.0, 0.0);
    // Moving the start keeps the door where it is in the world.
    h.drag((0.0, 0.0), (-1.0, 0.0), NONE);
    let o = h.doc.element(d).unwrap().as_opening().unwrap().clone();
    assert!(near_m(o.offset.as_m(), 5.0));
    // Shortening past the door is refused with an explanation.
    let out = h.drag((6.0, 0.0), (3.0, 0.0), NONE);
    assert!(out.commands.is_empty());
    assert!(out.warning.is_some());
    assert!(close(wall(&h, a).end, p(6.0, 0.0)));
}

#[test]
fn opening_handles_move_and_widen() {
    let mut h = Harness::new(ToolId::Select);
    let a = h.add_wall((0.0, 0.0), (6.0, 0.0));
    let d = h.add(ElementKind::Opening(Opening {
        host: a,
        kind: OpeningKind::Window,
        style: "window.casement_2".into(),
        offset: Length::m(2.0),
        width: Length::m(0.9),
        height: Length::m(1.2),
        sill: Length::m(0.9),
        flip_side: false,
        flip_hand: false,
        params: Default::default(),
    }));
    h.click(2.0, 0.0);
    assert_eq!(h.selection.iter().copied().collect::<Vec<_>>(), vec![d], "openings win over their walls");
    h.drag((2.45, 0.0), (2.75, 0.0), NONE);
    let o = h.doc.element(d).unwrap().as_opening().unwrap().clone();
    assert!(near_m(o.width.as_m(), 1.2) && near_m(o.offset.as_m(), 2.15), "{o:?}");
    h.drag((2.15, 0.0), (3.15, 0.0), NONE);
    let o = h.doc.element(d).unwrap().as_opening().unwrap().clone();
    assert!(near_m(o.offset.as_m(), 3.15));
    // Flip swaps the side; with Shift the hand.
    h.key(Key::Flip);
    h.key_mods(Key::Flip, SHIFT);
    let o = h.doc.element(d).unwrap().as_opening().unwrap().clone();
    assert!(o.flip_side && o.flip_hand);
}

#[test]
fn outline_vertices_drag_and_edge_midpoints_insert() {
    let mut h = Harness::new(ToolId::Select);
    let s = h.add(ElementKind::Slab(kanavos_model::Slab {
        outline: kanavos_geom::Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]).into(),
        structure: kanavos_model::Structure::Layered { build_up: h.doc.defaults().floor_slab.unwrap() },
        top_offset: Length::ZERO,
    }));
    h.click(2.0, 2.0);
    let out = h.move_to(2.0, 2.0);
    assert_eq!(out.handles.iter().filter(|x| x.role == HandleRole::Vertex).count(), 4);
    assert_eq!(out.handles.iter().filter(|x| x.role == HandleRole::InsertVertex).count(), 4);
    h.drag((4.0, 4.0), (5.0, 5.0), NONE);
    h.drag((2.0, 0.0), (2.0, -1.0), NONE);
    let ElementKind::Slab(slab) = &h.doc.element(s).unwrap().kind else { unreachable!() };
    assert_eq!(slab.outline.outer.len(), 5);
    assert!(slab.outline.outer.vertices.iter().any(|v| close(*v, p(5.0, 5.0))));
    assert!(slab.outline.outer.vertices.iter().any(|v| close(*v, p(2.0, -1.0))));
}

#[test]
fn roof_hole_vertices_have_handles_too() {
    let mut h = Harness::new(ToolId::Select);
    let r = h.add(ElementKind::Roof(kanavos_model::Roof {
        outline: kanavos_geom::Polygon2::from_m(&[(0.0, 0.0), (8.0, 0.0), (8.0, 6.0), (0.0, 6.0)]),
        holes: vec![kanavos_geom::Polygon2::from_m(&[(2.0, 2.0), (3.0, 2.0), (3.0, 3.0), (2.0, 3.0)])],
        base: Length::m(3.0),
        structure: kanavos_model::Structure::Layered { build_up: h.doc.defaults().pitched_roof.unwrap() },
        shape: kanavos_model::RoofShape::Pitched { slopes: vec![Some(kanavos_geom::Angle::deg(30.0)); 4] },
    }));
    h.selection = [r].into_iter().collect();
    let out = h.move_to(6.0, 4.0);
    assert_eq!(out.handles.iter().filter(|x| x.role == HandleRole::Vertex).count(), 8);
    assert_eq!(out.handles.iter().filter(|x| x.role == HandleRole::InsertVertex).count(), 8);
    // Drag a hole corner, then insert a vertex in a hole edge: the outline's slopes stay one per edge.
    h.drag((3.0, 3.0), (3.5, 3.5), NONE);
    h.drag((2.5, 2.0), (2.5, 1.5), NONE);
    let ElementKind::Roof(roof) = &h.doc.element(r).unwrap().kind else { unreachable!() };
    assert_eq!(roof.outline.len(), 4);
    assert_eq!(roof.holes[0].len(), 5);
    assert!(roof.holes[0].vertices.iter().any(|v| close(*v, p(3.5, 3.5))));
    assert!(roof.holes[0].vertices.iter().any(|v| close(*v, p(2.5, 1.5))));
    let kanavos_model::RoofShape::Pitched { slopes } = &roof.shape else { unreachable!() };
    assert_eq!(slopes.len(), 4);
}

#[test]
fn inserting_a_point_in_a_sloped_railing_interpolates_its_base() {
    let mut h = Harness::new(ToolId::Select);
    let steel = h.doc.defaults().railing_material.unwrap();
    let r = h.add(ElementKind::Railing(kanavos_model::Railing {
        path: vec![p(0.0, 0.0), p(4.0, 0.0)],
        height: Length::m(1.0),
        base_offset: Length::ZERO,
        elevations: vec![Length::ZERO, Length::m(2.0)],
        post_spacing: Length::m(1.2),
        infill: kanavos_model::RailingInfill::Bars,
        material: steel,
        infill_material: None,
    }));
    h.selection = [r].into_iter().collect();
    h.move_to(1.0, 0.0);
    h.drag((2.0, 0.0), (1.0, 1.0), NONE);
    let ElementKind::Railing(rl) = &h.doc.element(r).unwrap().kind else { unreachable!() };
    assert_eq!(rl.path.len(), 3);
    assert_eq!(rl.elevations.len(), 3, "one base per point");
    // (1, 1) projects a quarter of the way along the edge.
    assert!((rl.elevations[1].as_m() - 0.5).abs() < 1e-9, "{:?}", rl.elevations);
}

#[test]
fn objects_are_picked_inside_their_symbol() {
    let mut h = Harness::new(ToolId::Select);
    let obj = kanavos_model::Object {
        definition: "furniture.table".into(),
        position: p(5.0, 5.0),
        rotation: kanavos_geom::Angle::deg(90.0),
        mirrored: false,
        elevation: Length::ZERO,
        params: Default::default(),
    };
    let d = kanavos_drawing::Drawing {
        items: kanavos_objects::object_symbol(&obj),
        ..kanavos_drawing::Drawing::model(50.0)
    };
    let r = d.bounds().unwrap();
    let id = h.add(ElementKind::Object(obj));
    // The middle of the symbol's box, turned by 90° about the insertion point.
    let (cx, cy) = ((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0);
    h.click(5.0 - cy, 5.0 + cx);
    assert!(h.selection.contains(&id));
}

#[test]
fn point_handles_move_columns() {
    let mut h = Harness::new(ToolId::Select);
    let c = h.add(ElementKind::Column(kanavos_model::Column {
        position: p(1.0, 1.0),
        rotation: kanavos_geom::Angle::ZERO,
        profile: Profile::Rectangle { width: Length::cm(40.0), depth: Length::cm(40.0) },
        material: h.doc.defaults().structural_material.unwrap(),
        height: Length::m(3.0),
        base_offset: Length::ZERO,
    }));
    h.click(1.1, 1.1);
    assert!(h.selection.contains(&c));
    h.drag((1.0, 1.0), (3.0, 2.0), NONE);
    let ElementKind::Column(col) = &h.doc.element(c).unwrap().kind else { unreachable!() };
    assert!(close(col.position, p(3.0, 2.0)));
}
