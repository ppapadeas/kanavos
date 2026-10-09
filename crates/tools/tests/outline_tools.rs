//! Scripted tests of the slab, roof, space, fill, plot and measure tools (`specs/tools/outlines.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use kanavos_engine::AnyCommand;
use kanavos_geom::Angle;
use kanavos_model::{DraftShape, ElementKind, RoofShape, SpaceCategory};
use kanavos_tools::{Key, OutlineMode, RoofKind, ToolId};

fn draw_rect_polygon(h: &mut Harness, x0: f64, y0: f64, x1: f64, y1: f64) {
    h.click(x0, y0);
    h.click(x1, y0);
    h.click(x1, y1);
    h.click(x0, y1);
    h.click(x0, y0);
}

#[test]
fn slab_by_polygon_closes_on_the_first_vertex() {
    let mut h = Harness::new(ToolId::Slab);
    h.click(0.0, 0.0);
    h.click(5.0, 0.0);
    h.click(5.0, 4.0);
    let out = h.move_to(0.0, 4.0);
    assert!(out.preview.items.iter().any(|i| i.layer.starts_with("tool.ghost.slab")));
    h.click(0.0, 4.0);
    let out = h.click(0.0, 0.0);
    assert!(out.finished);
    let slabs = h.of("slab");
    assert_eq!(slabs.len(), 1);
    let ElementKind::Slab(s) = &slabs[0].kind else { unreachable!() };
    assert!(near_m(s.outline.area().as_m2(), 20.0));
    assert!(s.outline.outer.is_ccw());
}

#[test]
fn slab_by_rectangle_and_by_enter() {
    let mut h = Harness::new(ToolId::Slab);
    h.settings.slab.mode = OutlineMode::Rectangle;
    h.click(0.0, 0.0);
    h.click(6.0, 3.0);
    assert_eq!(h.of("slab").len(), 1);
    h.settings.slab.mode = OutlineMode::Polygon;
    h.click(10.0, 0.0);
    h.click(12.0, 0.0);
    h.click(12.0, 2.0);
    h.key(Key::Enter);
    assert_eq!(h.of("slab").len(), 2);
}

#[test]
fn slab_on_the_lowest_storey_uses_the_ground_slab() {
    let mut h = Harness::new(ToolId::Slab);
    h.settings.slab.mode = OutlineMode::Rectangle;
    h.click(0.0, 0.0);
    h.click(4.0, 4.0);
    let ElementKind::Slab(s) = &h.of("slab")[0].kind else { unreachable!() };
    let ground = h.doc.defaults().ground_slab.unwrap();
    assert!(matches!(s.structure, kanavos_model::Structure::Layered { build_up } if build_up == ground));
}

#[test]
fn pick_inside_falls_back_to_drawing_with_an_explanation() {
    let mut h = Harness::new(ToolId::Slab);
    h.settings.slab.mode = OutlineMode::PickInside;
    for (a, b) in [
        ((0.0, 0.0), (6.0, 0.0)),
        ((6.0, 0.0), (6.0, 4.0)),
        ((6.0, 4.0), (0.0, 4.0)),
        ((0.0, 4.0), (0.0, 0.0)),
    ] {
        h.add_wall(a, b);
    }
    let out = h.move_to(3.0, 2.0);
    assert!(!out.prompt.is_empty());
    // Detection is not available yet: the click starts a polygon and the prompt says why.
    let out = h.click(0.0, 0.0);
    assert!(out.commands.is_empty());
    assert!(!h.tool.is_idle());
    assert!(h.last.prompt.contains("draw the outline"), "{}", h.last.prompt);
    h.click(6.0, 0.0);
    h.click(6.0, 4.0);
    h.key(Key::Enter);
    assert_eq!(h.of("slab").len(), 1);
}

#[test]
fn backspace_and_escape_in_outlines() {
    let mut h = Harness::new(ToolId::Slab);
    h.click(0.0, 0.0);
    h.click(3.0, 0.0);
    h.click(9.0, 9.0);
    h.key(Key::Backspace);
    h.click(3.0, 3.0);
    h.key(Key::Enter);
    let ElementKind::Slab(s) = &h.of("slab")[0].kind else { unreachable!() };
    assert!(near_m(s.outline.area().as_m2(), 4.5));
    h.click(20.0, 0.0);
    assert!(h.key(Key::Escape).finished);
    assert!(h.tool.is_idle());
}

fn roof(h: &Harness) -> kanavos_model::Roof {
    match &h.of("roof")[0].kind {
        ElementKind::Roof(r) => r.clone(),
        _ => unreachable!(),
    }
}

#[test]
fn hip_roof_slopes_every_edge_and_adds_the_overhang() {
    let mut h = Harness::new(ToolId::Roof);
    h.settings.roof.kind = RoofKind::Hip;
    draw_rect_polygon(&mut h, 0.0, 0.0, 10.0, 6.0);
    let r = roof(&h);
    assert!(near_m(r.outline.area().as_m2(), 11.0 * 7.0));
    let RoofShape::Pitched { slopes } = &r.shape else { panic!("pitched") };
    assert_eq!(slopes.len(), 4);
    assert!(slopes.iter().all(|s| s.is_some_and(|a| (a.as_deg() - 22.0).abs() < 1e-9)));
    assert!(near_m(r.base.as_m(), 3.2), "eaves at the storey height");
}

#[test]
fn gable_roof_slopes_the_long_edges_and_flip_swaps_them() {
    let mut h = Harness::new(ToolId::Roof);
    h.settings.roof.kind = RoofKind::Gable;
    h.settings.roof.mode = OutlineMode::Rectangle;
    h.click(0.0, 0.0);
    h.click(10.0, 6.0);
    let r = roof(&h);
    let RoofShape::Pitched { slopes } = &r.shape else { panic!("pitched") };
    for (i, s) in slopes.iter().enumerate() {
        let e = r.outline.edge(i).unwrap();
        assert_eq!(s.is_some(), e.length().as_m() > 8.0, "edge {i}");
    }
    let mut h = Harness::new(ToolId::Roof);
    h.settings.roof.kind = RoofKind::Gable;
    h.settings.roof.mode = OutlineMode::Rectangle;
    h.click(0.0, 0.0);
    h.key(Key::Flip);
    h.click(10.0, 6.0);
    let r = roof(&h);
    let RoofShape::Pitched { slopes } = &r.shape else { panic!("pitched") };
    for (i, s) in slopes.iter().enumerate() {
        let e = r.outline.edge(i).unwrap();
        assert_eq!(s.is_some(), e.length().as_m() < 8.0, "edge {i}");
    }
}

#[test]
fn mono_pitch_roof_slopes_the_edge_nearest_the_first_click() {
    let mut h = Harness::new(ToolId::Roof);
    h.settings.roof.kind = RoofKind::MonoPitch;
    h.settings.roof.pitch = Angle::deg(15.0);
    draw_rect_polygon(&mut h, 0.0, 0.0, 10.0, 6.0);
    let r = roof(&h);
    let RoofShape::Pitched { slopes } = &r.shape else { panic!("pitched") };
    assert_eq!(slopes.iter().filter(|s| s.is_some()).count(), 1);
    let i = slopes.iter().position(Option::is_some).unwrap();
    let e = r.outline.edge(i).unwrap();
    // The longer of the two edges at the first click: the south edge (y = -0.5).
    assert!(near_m(e.start.y.as_m(), -0.5) && near_m(e.end.y.as_m(), -0.5), "{e:?}");
}

#[test]
fn flat_roof_has_the_parapet() {
    let mut h = Harness::new(ToolId::Roof);
    h.settings.roof.kind = RoofKind::Flat;
    h.settings.roof.overhang = kanavos_units::Length::ZERO;
    draw_rect_polygon(&mut h, 0.0, 0.0, 8.0, 8.0);
    let r = roof(&h);
    assert!(matches!(r.shape, RoofShape::Flat { parapet: Some(_) }));
    assert!(near_m(r.outline.area().as_m2(), 64.0));
    assert_eq!(
        r.structure,
        kanavos_model::Structure::Layered { build_up: h.doc.defaults().flat_roof.unwrap() }
    );
}

#[test]
fn spaces_are_named_categorised_and_numbered() {
    let mut h = Harness::new(ToolId::Space);
    h.settings.space.mode = OutlineMode::Rectangle;
    h.settings.space.name = "Κουζίνα".into();
    h.settings.space.category = SpaceCategory::MainUse;
    h.click(0.0, 0.0);
    h.click(4.0, 3.0);
    h.settings.space.name = "Αποθήκη".into();
    h.settings.space.category = SpaceCategory::Auxiliary;
    h.click(4.0, 0.0);
    h.click(6.0, 3.0);
    let spaces: Vec<_> = h.of("space").into_iter().filter_map(|e| e.as_space().cloned()).collect();
    assert_eq!(spaces.len(), 2);
    assert_eq!(spaces[0].name, "Κουζίνα");
    assert_eq!(spaces[0].number, "0.01");
    assert_eq!(spaces[1].number, "0.02");
    assert_eq!(spaces[1].category, SpaceCategory::Auxiliary);
    // Upper storey numbers start with 1.
    h.storey = h.doc.storeys()[1].id;
    h.click(0.0, 0.0);
    h.click(4.0, 3.0);
    let up: Vec<_> = h.of("space").into_iter().filter_map(|e| e.as_space().cloned()).collect();
    assert_eq!(up[0].number, "1.01");
}

#[test]
fn space_pick_inside_previews_and_falls_back() {
    let mut h = Harness::new(ToolId::Space);
    let out = h.move_to(2.0, 2.0);
    assert!(out.prompt.contains("not available") || out.prompt.contains("enclose"), "{}", out.prompt);
    h.click(0.0, 0.0);
    h.click(3.0, 0.0);
    h.click(3.0, 3.0);
    h.double_click(0.0, 3.0);
    assert_eq!(h.of("space").len(), 1);
}

#[test]
fn fill_uses_the_drafting_style() {
    let mut h = Harness::new(ToolId::Fill);
    h.settings.draft.hatch = kanavos_drawing::HatchPattern::Diagonal;
    h.settings.fill_mode = OutlineMode::Rectangle;
    h.click(0.0, 0.0);
    h.click(2.0, 1.0);
    let fills = h.of("drafting");
    let ElementKind::Drafting(d) = &fills[0].kind else { unreachable!() };
    assert!(matches!(d.shape, DraftShape::Fill { .. }));
    assert_eq!(d.style.hatch, kanavos_drawing::HatchPattern::Diagonal);
}

#[test]
fn plot_sets_the_site_and_keeps_its_other_fields() {
    let mut h = Harness::new(ToolId::Plot);
    let mut site = h.doc.site().clone();
    site.terms.building_coefficient = 1.2;
    h.doc = h.doc.with_site(site);
    h.click(0.0, 0.0);
    h.click(20.0, 0.0);
    h.click(20.0, 25.0);
    let out = h.click(0.0, 0.0);
    assert!(matches!(out.commands.as_slice(), [AnyCommand::SetSite(_)]));
    let s = h.doc.site();
    assert!(near_m(s.plot.as_ref().unwrap().area().as_m2(), 250.0));
    assert!(near_m(s.terms.building_coefficient, 1.2));
}

#[test]
fn measure_reports_lengths_and_area_and_creates_nothing() {
    let mut h = Harness::new(ToolId::Measure);
    h.click(0.0, 0.0);
    let out = h.move_to(3.0, 0.0);
    assert!(out.fields.iter().any(|f| f.label == "Segment" && f.value == "3.00 m"));
    h.click(3.0, 0.0);
    h.click(3.0, 4.0);
    let out = h.click(0.0, 0.0);
    assert!(out.commands.is_empty());
    assert!(out.fields.iter().any(|f| f.label == "Area" && f.value == "6.00 m²"));
    assert!(out.prompt.contains("perimeter 12.00 m"), "{}", out.prompt);
    assert!(h.applied.is_empty());
    h.key(Key::Escape);
    assert!(h.tool.is_idle());
}
