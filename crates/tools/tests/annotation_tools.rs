//! Scripted tests of the dimension, text, level, grid, section, elevation and drafting tools
//! (`specs/tools/annotation.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use kanavos_engine::AnyCommand;
use kanavos_geom::Arc2;
use kanavos_model::{DraftShape, ElementKind, Opening, OpeningKind, ViewKind};
use kanavos_tools::{Key, ToolId};
use kanavos_units::Length;

#[test]
fn dimension_chain_then_place_the_line() {
    let mut h = Harness::new(ToolId::Dimension);
    h.click(0.0, 0.0);
    h.click(3.0, 0.0);
    h.click(5.0, 0.0);
    h.key(Key::Enter);
    let out = h.move_to(2.0, 1.0);
    assert!(out.preview.items.iter().any(|i| i.layer.starts_with("tool.ghost.dimension")));
    assert!(out.typing);
    h.click(2.0, 1.0);
    let ElementKind::Dimension(d) = &h.of("dimension")[0].kind else { unreachable!() };
    assert_eq!(d.points.len(), 3);
    assert!(close(d.line_point, p(2.0, 1.0)));
    assert!(near_m(d.direction.as_deg(), 0.0));
}

#[test]
fn dimension_typed_offset_and_orientation() {
    let mut h = Harness::new(ToolId::Dimension);
    h.click(0.0, 0.0);
    h.click(4.0, 3.0);
    h.double_click(4.0, 3.0);
    h.key(Key::Flip); // aligned → horizontal
    h.move_to(2.0, 5.0);
    h.text("0,8");
    h.key(Key::Enter);
    let ElementKind::Dimension(d) = &h.of("dimension")[0].kind else { unreachable!() };
    assert!(near_m(d.direction.as_deg(), 0.0));
    assert!(near_m(d.line_point.y.as_m(), 3.8), "{:?}", d.line_point);
}

#[test]
fn clicking_a_wall_dimensions_it_with_its_openings() {
    let mut h = Harness::new(ToolId::Dimension).with_snapping();
    let w = h.add_wall((0.0, 0.0), (6.0, 0.0));
    h.add(ElementKind::Opening(Opening {
        host: w,
        kind: OpeningKind::Door,
        style: "door.single_swing".into(),
        offset: Length::m(2.0),
        width: Length::m(0.9),
        height: Length::m(2.1),
        sill: Length::ZERO,
        flip_side: false,
        flip_hand: false,
        params: Default::default(),
    }));
    // On the left face, away from snap points.
    h.click(4.3, 0.12);
    h.click(3.0, 1.5);
    let ElementKind::Dimension(d) = &h.of("dimension")[0].kind else { unreachable!() };
    assert_eq!(d.points.len(), 4, "{:?}", d.points);
    let xs: Vec<f64> = d.points.iter().map(|q| q.x.as_m()).collect();
    assert!(near_m(xs[1], 1.55) && near_m(xs[2], 2.45), "{xs:?}");
    assert!(d.points.iter().all(|q| near_m(q.y.as_m(), 0.155)));
}

#[test]
fn text_box_collects_text_and_commits_on_enter() {
    let mut h = Harness::new(ToolId::Text);
    let out = h.click(1.0, 2.0);
    let entry = out.text_entry.unwrap();
    assert!(close(entry.at, p(1.0, 2.0)) && entry.text.is_empty());
    h.text("Σαλόνι");
    h.key_mods(Key::Enter, SHIFT);
    h.text("25 m²");
    let out = h.key(Key::Backspace);
    assert_eq!(out.text_entry.unwrap().text, "Σαλόνι\n25 m");
    let out = h.key(Key::Enter);
    assert!(out.finished && out.text_entry.is_none());
    let ElementKind::Text(t) = &h.of("text")[0].kind else { unreachable!() };
    assert_eq!(t.content, "Σαλόνι\n25 m");
    // Escape discards; clicking elsewhere commits.
    h.click(5.0, 5.0);
    h.text("x");
    h.key(Key::Escape);
    h.click(6.0, 6.0);
    h.text("A");
    h.click(7.0, 7.0);
    assert_eq!(h.of("text").len(), 2);
}

#[test]
fn level_marks_use_the_storey_or_a_typed_value() {
    let mut h = Harness::new(ToolId::Level);
    h.click(1.0, 1.0);
    h.text("+3,20");
    h.click(2.0, 2.0);
    let levels: Vec<_> = h
        .of("level")
        .into_iter()
        .filter_map(|e| match e.kind {
            ElementKind::Level(l) => Some(l),
            _ => None,
        })
        .collect();
    assert_eq!(levels[0].value, None);
    assert!(near_m(levels[1].value.unwrap().as_m(), 3.2));
}

#[test]
fn grid_lines_are_labelled_in_sequence() {
    let mut h = Harness::new(ToolId::Grid);
    h.click(0.0, -1.0);
    h.click(0.0, 10.0);
    h.click(5.0, -1.0);
    h.click(5.0, 10.0);
    h.click(-1.0, 0.0);
    h.click(8.0, 0.0);
    let labels: Vec<String> = h
        .doc
        .elements()
        .filter_map(|e| match &e.kind {
            ElementKind::Grid(g) => Some((e.storey, g.label.clone())),
            _ => None,
        })
        .map(|(s, l)| {
            assert!(s.is_none(), "grid lines are project-wide");
            l
        })
        .collect();
    assert_eq!(labels, vec!["1", "2", "A"]);
}

#[test]
fn section_and_elevation_markers_take_the_next_letter() {
    let mut h = Harness::new(ToolId::Section);
    let out = h.move_to(0.0, 0.0);
    assert!(out.prompt.contains("Α"));
    h.click(-1.0, 2.0);
    h.click(11.0, 2.0);
    let out = h.move_to(5.0, 5.0);
    assert!(out.preview.items.iter().any(|i| i.layer.contains("section.marker")));
    let out = h.click(5.0, 5.0);
    assert!(matches!(out.commands.as_slice(), [AnyCommand::CreateView(_)]));
    h.click(-1.0, 4.0);
    h.click(11.0, 4.0);
    h.click(5.0, 1.0);
    h.switch(ToolId::Elevation);
    h.click(0.0, -5.0);
    h.click(10.0, -5.0);
    h.click(5.0, 0.0);
    let views: Vec<_> = h
        .doc
        .views()
        .filter_map(|v| match &v.kind {
            ViewKind::Section { marker, flip, .. } => Some((v.name.clone(), marker.clone(), *flip, true)),
            ViewKind::Elevation { marker, flip, .. } => Some((v.name.clone(), marker.clone(), *flip, false)),
            _ => None,
        })
        .collect();
    assert_eq!(views.len(), 3);
    assert_eq!(views[0], ("Section Α-Α".to_owned(), "Α".to_owned(), false, true));
    assert_eq!(views[1].1, "Β");
    assert!(views[1].2, "looks to the right of start → end");
    assert_eq!(views[2], ("Elevation Γ".to_owned(), "Γ".to_owned(), false, false));
}

#[test]
fn lines_polylines_arcs_and_circles() {
    let mut h = Harness::new(ToolId::Line);
    h.click(0.0, 0.0);
    h.click(2.0, 0.0);
    h.click(2.0, 2.0);
    h.key(Key::Escape);
    assert_eq!(h.of("drafting").len(), 2);

    h.switch(ToolId::Polyline);
    h.click(10.0, 0.0);
    h.click(12.0, 0.0);
    h.click(12.0, 2.0);
    h.click(10.0, 0.0);
    let closed = h.of("drafting").into_iter().find_map(|e| match e.kind {
        ElementKind::Drafting(d) => match d.shape {
            DraftShape::Polyline { points, closed } => Some((points.len(), closed)),
            _ => None,
        },
        _ => None,
    });
    assert_eq!(closed, Some((3, true)));

    h.switch(ToolId::Arc);
    h.click(0.0, 5.0);
    h.click(4.0, 5.0);
    h.click(2.0, 7.0);
    h.switch(ToolId::Circle);
    h.click(20.0, 20.0);
    h.text("1,5");
    h.key(Key::Enter);
    let mut found_arc = false;
    let mut found_circle = false;
    for e in h.of("drafting") {
        if let ElementKind::Drafting(d) = e.kind {
            match d.shape {
                DraftShape::Arc { arc } => {
                    found_arc = true;
                    assert!(close(arc.at(0.0), p(0.0, 5.0)) && close(arc.at(1.0), p(4.0, 5.0)));
                    assert!(close(arc.at(0.5), p(2.0, 7.0)));
                    let _: Arc2 = arc;
                }
                DraftShape::Circle { centre, radius } => {
                    found_circle = true;
                    assert!(close(centre, p(20.0, 20.0)) && near_m(radius.as_m(), 1.5));
                }
                _ => {}
            }
        }
    }
    assert!(found_arc && found_circle);
}
