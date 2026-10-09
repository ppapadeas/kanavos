//! Scripted tests of the wall tool (`specs/tools/wall.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use kanavos_engine::AnyCommand;
use kanavos_geom::{Angle, Arc2};
use kanavos_model::{Georef, ReferenceLine, Site};
use kanavos_tools::{Key, ToolId, WallMode};

#[test]
fn chain_of_walls_closes_on_its_first_point() {
    let mut h = Harness::new(ToolId::Wall);
    h.click(0.0, 0.0);
    h.click(5.0, 0.0);
    h.click(5.0, 4.0);
    h.click(0.0, 4.0);
    let out = h.click(0.0, 0.0);
    assert!(out.finished, "closing the chain ends it");
    let walls = h.walls();
    assert_eq!(walls.len(), 4);
    assert!(close(walls[3].start, p(0.0, 4.0)) && close(walls[3].end, p(0.0, 0.0)));
    assert!(h.tool.is_idle());
    // Each wall was its own undo step.
    assert_eq!(h.applied.len(), 4);
}

#[test]
fn enter_double_click_and_escape_end_the_chain() {
    let mut h = Harness::new(ToolId::Wall);
    h.click(0.0, 0.0);
    h.click(3.0, 0.0);
    assert!(h.key(Key::Enter).finished);
    assert!(h.tool.is_idle());
    h.click(0.0, 2.0);
    h.click(3.0, 2.0);
    h.double_click(3.0, 5.0);
    assert!(h.tool.is_idle());
    assert_eq!(h.walls().len(), 3);
    h.click(10.0, 0.0);
    h.key(Key::Escape);
    assert!(h.tool.is_idle());
}

#[test]
fn typed_length_angle_and_offset() {
    let mut h = Harness::new(ToolId::Wall);
    h.click(1.0, 1.0);
    h.move_to(2.0, 1.2);
    let out = h.text("4,5");
    assert!(out.fields[0].locked && out.fields[0].value == "4,5");
    assert!(out.typing);
    h.key(Key::Tab);
    h.text("90");
    let out = h.key(Key::Enter);
    assert_eq!(out.commands.len(), 1);
    let w = &h.walls()[0];
    assert!(close(w.end, p(1.0, 5.5)), "{:?}", w.end);
    // Arithmetic with units from the new end, following the pointer direction.
    h.move_to(9.0, 5.5);
    h.text("3.20+25cm");
    h.key(Key::Enter);
    let w = &h.walls()[1];
    assert!(close(w.end, p(4.45, 5.5)), "{:?}", w.end);
    // Polar and relative input.
    h.text("@2<270");
    h.key(Key::Enter);
    h.text("@-1;0");
    h.key(Key::Enter);
    let walls = h.walls();
    assert!(close(walls[2].end, p(4.45, 3.5)));
    assert!(close(walls[3].end, p(3.45, 3.5)));
}

#[test]
fn unreadable_input_is_explained() {
    let mut h = Harness::new(ToolId::Wall);
    h.click(0.0, 0.0);
    let out = h.text("3+");
    assert!(out.warning.is_some());
    h.key(Key::Backspace);
    let out = h.key(Key::Backspace);
    assert!(out.warning.is_none());
}

#[test]
fn backspace_removes_the_last_wall_of_the_chain() {
    let mut h = Harness::new(ToolId::Wall);
    h.click(0.0, 0.0);
    h.click(4.0, 0.0);
    h.click(4.0, 3.0);
    assert_eq!(h.walls().len(), 2);
    let out = h.key(Key::Backspace);
    assert!(matches!(out.commands.as_slice(), [AnyCommand::DeleteElements(_)]));
    assert_eq!(h.walls().len(), 1);
    // The chain continues from the end of the remaining wall.
    h.click(4.0, -2.0);
    let walls = h.walls();
    assert_eq!(walls.len(), 2);
    assert!(walls.iter().any(|w| close(w.start, p(4.0, 0.0)) && close(w.end, p(4.0, -2.0))));
    // Backspace back to the start, then once more ends the chain.
    h.key(Key::Backspace);
    h.key(Key::Backspace);
    assert_eq!(h.walls().len(), 0);
    h.key(Key::Backspace);
    assert!(h.tool.is_idle());
}

#[test]
fn rectangle_mode_makes_four_walls_in_one_step_clockwise() {
    let mut h = Harness::new(ToolId::Wall);
    h.settings.wall.mode = WallMode::Rectangle;
    h.settings.wall.reference = ReferenceLine::Left;
    h.click(0.0, 0.0);
    h.move_to(6.0, 4.0);
    assert!(!h.last.preview.items.is_empty());
    let out = h.click(6.0, 4.0);
    assert_eq!(out.commands.len(), 1, "one batch, one undo step");
    assert!(matches!(&out.commands[0], AnyCommand::Batch(b) if b.commands.len() == 4));
    let walls = h.walls();
    assert_eq!(walls.len(), 4);
    assert!(
        close(walls[0].start, p(0.0, 0.0)) && close(walls[0].end, p(0.0, 4.0)),
        "west wall runs north first"
    );
    // Clockwise ring: the left face (reference line) is outside.
    let area: f64 =
        walls.iter().map(|w| w.start.x.as_m() * w.end.y.as_m() - w.end.x.as_m() * w.start.y.as_m()).sum();
    assert!(area < 0.0);
    assert!(walls.iter().all(|w| w.reference == ReferenceLine::Left));
}

#[test]
fn rectangle_with_typed_extents() {
    let mut h = Harness::new(ToolId::Wall);
    h.settings.wall.mode = WallMode::Rectangle;
    h.click(1.0, 1.0);
    h.move_to(0.0, 5.0);
    h.text("8");
    h.key(Key::Tab);
    h.text("6");
    h.key(Key::Enter);
    let walls = h.walls();
    assert_eq!(walls.len(), 4);
    let xs: Vec<f64> = walls.iter().map(|w| w.start.x.as_m()).collect();
    assert!(xs.iter().any(|x| near_m(*x, -7.0)));
    assert!(walls.iter().any(|w| near_m(w.start.y.as_m(), 7.0)));
}

#[test]
fn arc_mode_bends_through_the_third_point() {
    let mut h = Harness::new(ToolId::Wall);
    h.settings.wall.mode = WallMode::Arc;
    h.click(0.0, 0.0);
    h.click(4.0, 0.0);
    h.move_to(2.0, -1.0);
    assert!(!h.last.preview.items.is_empty());
    h.click(2.0, -1.0);
    let w = &h.walls()[0];
    assert!(w.bulge > 0.0, "bends to the right of start → end");
    let arc = Arc2::from_bulge(w.start, w.end, w.bulge).unwrap();
    assert!(close(arc.at(0.5), p(2.0, -1.0)));
    // The chain continues from the end; a typed rise sets the next arc.
    h.click(8.0, 0.0);
    h.move_to(6.0, 1.0);
    h.text("0,5");
    h.key(Key::Enter);
    let walls = h.walls();
    assert_eq!(walls.len(), 2);
    let arc = Arc2::from_bulge(walls[1].start, walls[1].end, walls[1].bulge).unwrap();
    assert!(close(arc.at(0.5), p(6.0, 0.5)), "{:?}", arc.at(0.5));
}

#[test]
fn arcs_beyond_a_half_circle_are_refused() {
    let mut h = Harness::new(ToolId::Wall);
    h.settings.wall.mode = WallMode::Arc;
    h.click(0.0, 0.0);
    h.click(2.0, 0.0);
    let out = h.click(1.0, -3.0);
    assert!(out.commands.is_empty());
    assert!(out.warning.is_some());
}

#[test]
fn flip_swaps_the_reference_side() {
    let mut h = Harness::new(ToolId::Wall);
    h.settings.wall.reference = ReferenceLine::Left;
    h.click(0.0, 0.0);
    h.key(Key::Flip);
    h.click(3.0, 0.0);
    assert_eq!(h.walls()[0].reference, ReferenceLine::Right);
}

#[test]
fn absolute_and_national_grid_start_points() {
    let mut h = Harness::new(ToolId::Wall);
    let site = Site {
        georef: Georef {
            epsg: 2100,
            easting: 476_500.0,
            northing: 4_097_300.0,
            rotation: Angle::ZERO,
            altitude: 0.0,
        },
        ..h.doc.site().clone()
    };
    h.doc = h.doc.with_site(site);
    h.text("#476510;4097305");
    h.key(Key::Enter);
    h.text("@4<0");
    h.key(Key::Enter);
    let w = &h.walls()[0];
    assert!(close(w.start, p(10.0, 5.0)), "{:?}", w.start);
    assert!(close(w.end, p(14.0, 5.0)));
    h.key(Key::Escape);
    h.key(Key::Escape);
    h.text("2.5,1.0");
    h.key(Key::Enter);
    h.text("2");
    h.move_to(2.5, 9.0);
    h.key(Key::Enter);
    let walls = h.walls();
    assert!(walls.iter().any(|w| close(w.start, p(2.5, 1.0)) && close(w.end, p(2.5, 3.0))));
}

#[test]
fn walls_snap_to_existing_wall_ends() {
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.add_wall((0.0, 0.0), (5.0, 0.0));
    h.click(5.04, 0.03);
    let out = h.move_to(5.0, 3.0);
    let snap = out.snap.unwrap();
    assert!(snap.label.starts_with("angle 90"), "{}", snap.label);
    h.click(5.0, 3.0);
    let walls = h.walls();
    assert!(walls.iter().any(|w| close(w.start, p(5.0, 0.0)) && close(w.end, p(5.0, 3.0))));
}

#[test]
fn preview_shows_the_wall_and_its_length() {
    let mut h = Harness::new(ToolId::Wall);
    h.click(0.0, 0.0);
    let out = h.move_to(3.25, 0.0);
    assert!(out.preview.items.iter().any(|i| i.layer.starts_with("tool.ghost.wall")));
    assert!(
        out.preview
            .items
            .iter()
            .any(|i| matches!(&i.shape, kanavos_drawing::Shape::Text(t) if t.content == "3.25 m"))
    );
    assert_eq!(out.fields[0].value, "3.25 m");
    assert!(!out.fields[0].locked);
    assert!(out.prompt.contains("next point"));
}
