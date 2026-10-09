//! Wall junctions, openings and wall solids: numeric golden tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::{area, close, fx};
use kanavos_elements::{JunctionKind, StoreyGeometry, plan_parts, quantities, solids, wall_layout};
use kanavos_geom::{Length, Point2, PolygonWithHoles, Tolerance, union_all};
use kanavos_model::OpeningKind;

fn has_vertex(p: &kanavos_geom::Polygon2, x: f64, y: f64) -> bool {
    p.vertices.iter().any(|q| {
        q.coincides(Point2::new(x, y), &Tolerance { length: Length::mm(0.01), ..Tolerance::default() })
    })
}

#[test]
fn single_wall_is_a_rectangle() {
    let mut f = fx();
    let w = f.basic_wall((0.0, 0.0), (5.0, 0.0), 0.25, 3.0);
    let l = wall_layout(&f.doc, w).unwrap();
    assert!(close(l.outline.area().as_m2(), 1.25, 1e-9));
    assert_eq!(l.outline.len(), 4);
    assert_eq!(l.layers.len(), 1);
    assert!(l.joins.is_empty());
    assert_eq!(l.left_face.len(), 2);
    assert!(close(l.left_face[0].distance(l.left_face[1]).as_m(), 5.0, 1e-9));
}

#[test]
fn l_corner_mitres_equal_build_ups() {
    let mut f = fx();
    let a = f.ext_wall((0.0, 0.0), (10.0, 0.0));
    let b = f.ext_wall((10.0, 0.0), (10.0, 8.0));
    let la = wall_layout(&f.doc, a).unwrap();
    let lb = wall_layout(&f.doc, b).unwrap();
    // Outer corner (10.155, -0.155) and inner corner (9.845, 0.155): a mitre.
    assert!(has_vertex(&la.outline, 10.155, -0.155), "{:?}", la.outline);
    assert!(has_vertex(&la.outline, 9.845, 0.155));
    assert!(has_vertex(&lb.outline, 10.155, -0.155));
    assert!(has_vertex(&lb.outline, 9.845, 0.155));
    // The mitre keeps each wall's area equal to length × thickness.
    assert!(close(la.outline.area().as_m2(), 10.0 * 0.31, 1e-9));
    assert!(close(lb.outline.area().as_m2(), 8.0 * 0.31, 1e-9));
    // Layers tile the outline and each layer wraps the corner continuously.
    let layers_a: f64 = la.layers.iter().map(|b| b.region.area().as_m2()).sum();
    assert!(close(layers_a, la.outline.area().as_m2(), 1e-9));
    for k in 0..4 {
        let both: Vec<PolygonWithHoles> = la
            .layers
            .iter()
            .chain(lb.layers.iter())
            .filter(|b| b.layer == k)
            .map(|b| b.region.clone().into())
            .collect();
        let u = union_all(&both).unwrap();
        assert_eq!(u.len(), 1, "layer {k} should wrap the corner");
    }
    assert_eq!(la.joins.len(), 1);
    assert_eq!(la.joins[0].kind, JunctionKind::Corner);
}

#[test]
fn l_corner_with_different_thicknesses_meets_at_the_faces() {
    let mut f = fx();
    let a = f.basic_wall((0.0, 0.0), (6.0, 0.0), 0.30, 3.0);
    let b = f.basic_wall((6.0, 0.0), (6.0, 4.0), 0.10, 3.0);
    let la = wall_layout(&f.doc, a).unwrap();
    let lb = wall_layout(&f.doc, b).unwrap();
    // Faces extended/trimmed to meet: outer corner at (6.05, -0.15), inner corner at (5.95, 0.15).
    assert!(has_vertex(&la.outline, 6.05, -0.15), "{:?}", la.outline);
    assert!(has_vertex(&la.outline, 5.95, 0.15));
    assert!(has_vertex(&lb.outline, 6.05, -0.15));
    assert!(has_vertex(&lb.outline, 5.95, 0.15));
    let u = union_all(&[la.outline.clone().into(), lb.outline.clone().into()]).unwrap();
    assert_eq!(u.len(), 1);
    assert!(u[0].outer.len() == 6, "{:?}", u[0].outer);
}

#[test]
fn l_corner_with_a_stronger_core_runs_through() {
    let mut f = fx();
    let cw = kanavos_model::Structure::Layered { build_up: f.concrete_wall };
    let a = f.wall_with((0.0, 0.0), (6.0, 0.0), cw, 3.0);
    let b = f.int_wall((6.0, 0.0), (6.0, 4.0));
    let la = wall_layout(&f.doc, a).unwrap();
    let lb = wall_layout(&f.doc, b).unwrap();
    // The concrete wall reaches the brick wall's far (east) face x = 6.065.
    let max_x = la.outline.vertices.iter().map(|p| p.x.as_m()).fold(f64::MIN, f64::max);
    assert!(close(max_x, 6.065, 1e-6), "{max_x}");
    // The brick wall stops at the concrete wall's north face y = 0.145.
    let min_y = lb.outline.vertices.iter().map(|p| p.y.as_m()).fold(f64::MAX, f64::min);
    assert!(close(min_y, 0.145, 1e-6), "{min_y}");
    assert!(close(
        area(&union_all(&[la.outline.into(), lb.outline.into()]).unwrap()),
        6.065 * 0.29 + 0.13 * (4.0 - 0.145),
        1e-6
    ));
}

#[test]
fn tee_trims_the_abutting_wall_and_lets_its_core_through() {
    let mut f = fx();
    let a = f.ext_wall((0.0, 0.0), (10.0, 0.0));
    let c = f.int_wall((5.0, 0.0), (5.0, 5.0));
    let la = wall_layout(&f.doc, a).unwrap();
    let lc = wall_layout(&f.doc, c).unwrap();
    // The abutting outline stops at the through wall's near face (north face y = 0.155).
    let min_y = lc.outline.vertices.iter().map(|p| p.y.as_m()).fold(f64::MAX, f64::min);
    assert!(close(min_y, 0.155, 1e-6));
    assert!(close(lc.outline.area().as_m2(), 0.13 * (5.0 - 0.155), 1e-9));
    // Its brick core (priority 60) runs through the plaster (10) and the insulation (20) of the through wall and
    // stops at the through wall's brick (60): depth 1 + 8 cm.
    let core = lc.layers.iter().find(|b| b.layer == 1).unwrap();
    let core_min = core.region.vertices.iter().map(|p| p.y.as_m()).fold(f64::MAX, f64::min);
    assert!(close(core_min, 0.155 - 0.09, 1e-6), "{core_min}");
    assert!(close(core.region.area().as_m2(), 0.09 * (5.0 - 0.065), 1e-9));
    // Its plasters (10) stop at the face.
    for k in [0, 2] {
        let b = lc.layers.iter().find(|b| b.layer == k).unwrap();
        let m = b.region.vertices.iter().map(|p| p.y.as_m()).fold(f64::MAX, f64::min);
        assert!(close(m, 0.155, 1e-6));
    }
    // The through wall's plaster and insulation are cut where the core passes; its brick and inner plaster not.
    assert_eq!(la.layers.iter().filter(|b| b.layer == 0).count(), 2);
    assert_eq!(la.layers.iter().filter(|b| b.layer == 1).count(), 2);
    assert_eq!(la.layers.iter().filter(|b| b.layer == 2).count(), 1);
    assert_eq!(la.layers.iter().filter(|b| b.layer == 3).count(), 1);
    // Areas still balance: what the through wall lost, the abutting core gained.
    let lost = 10.0 * 0.31 - la.layers.iter().map(|b| b.region.area().as_m2()).sum::<f64>();
    assert!(close(lost, 0.09 * 0.09, 1e-9), "{lost}");
    assert_eq!(lc.joins[0].kind, JunctionKind::Tee);
}

#[test]
fn crossing_walls_interrupt_the_weaker_one() {
    let mut f = fx();
    let a = f.int_wall((0.0, 2.0), (6.0, 2.0));
    let b = f.int_wall((3.0, 0.0), (3.0, 5.0));
    let la = wall_layout(&f.doc, a).unwrap();
    let lb = wall_layout(&f.doc, b).unwrap();
    // Equal build-ups: the older wall (a) runs through; b's plasters are interrupted, its brick (equal priority)
    // too, so every layer of b has two pieces.
    for k in 0..3 {
        assert_eq!(lb.layers.iter().filter(|x| x.layer == k).count(), 2, "layer {k}");
        // a's plasters are cut where b's brick runs into them; a's brick is not.
        assert_eq!(la.layers.iter().filter(|x| x.layer == k).count(), if k == 1 { 1 } else { 2 });
    }
    // b's plasters stop at a's faces; its brick runs through a's plasters to a's brick.
    let lbt: f64 = lb.layers.iter().map(|x| x.region.area().as_m2()).sum();
    let expected = 0.13 * 5.0 - 0.13 * 0.13 + 2.0 * 0.09 * 0.02;
    assert!(close(lbt, expected, 1e-9), "{lbt} vs {expected}");
    // a's plasters are cut by b's brick.
    let lat: f64 = la.layers.iter().map(|x| x.region.area().as_m2()).sum();
    assert!(close(lat, 0.13 * 6.0 - 2.0 * 0.09 * 0.02, 1e-9));
    // The outline of the crossing wall is not split.
    assert!(close(lb.outline.area().as_m2(), 0.13 * 5.0, 1e-9));
    let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
    assert!(geo.junctions().iter().any(|j| j.kind == JunctionKind::Cross));
}

#[test]
fn four_walls_meeting_at_a_point_form_a_cross() {
    let mut f = fx();
    let w1 = f.int_wall((0.0, 0.0), (3.0, 0.0));
    let w2 = f.int_wall((3.0, 0.0), (6.0, 0.0));
    let w3 = f.int_wall((3.0, 0.0), (3.0, 3.0));
    let w4 = f.int_wall((3.0, -3.0), (3.0, 0.0));
    let total: Vec<PolygonWithHoles> =
        [w1, w2, w3, w4].iter().map(|&w| wall_layout(&f.doc, w).unwrap().outline.into()).collect();
    let u = union_all(&total).unwrap();
    assert_eq!(u.len(), 1);
    assert!(close(area(&u), 0.13 * 6.0 + 0.13 * 6.0 - 0.13 * 0.13, 1e-6), "{}", area(&u));
    // No overlaps between the outlines.
    let sum: f64 = total.iter().map(|p| p.area().as_m2()).sum();
    assert!(close(sum, area(&u), 1e-6), "{sum} vs {}", area(&u));
}

#[test]
fn three_walls_at_a_point_without_a_through_pair_make_a_pinwheel() {
    let mut f = fx();
    let ws = [
        f.int_wall((0.0, 0.0), (3.0, 0.0)),
        f.int_wall((0.0, 0.0), (-1.5, 2.598)),
        f.int_wall((0.0, 0.0), (-1.5, -2.598)),
    ];
    let outlines: Vec<PolygonWithHoles> =
        ws.iter().map(|&w| wall_layout(&f.doc, w).unwrap().outline.into()).collect();
    let u = union_all(&outlines).unwrap();
    let sum: f64 = outlines.iter().map(|p| p.area().as_m2()).sum();
    assert_eq!(u.len(), 1);
    assert!(close(sum, area(&u), 1e-6), "overlap: {sum} vs {}", area(&u));
}

#[test]
fn chain_of_walls_closes_a_room() {
    let mut f = fx();
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let ids: Vec<_> = (0..4).map(|i| f.ext_wall(pts[i], pts[(i + 1) % 4])).collect();
    let outlines: Vec<PolygonWithHoles> =
        ids.iter().map(|&w| wall_layout(&f.doc, w).unwrap().outline.into()).collect();
    let u = union_all(&outlines).unwrap();
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].outer.len(), 4);
    assert_eq!(u[0].holes.len(), 1);
    assert!(close(u[0].holes[0].area().as_m2(), 9.69 * 7.69, 1e-6));
    assert!(close(u[0].outer.area().as_m2(), 10.31 * 8.31, 1e-6));
}

#[test]
fn storey_geometry_matches_single_wall_layouts() {
    let mut f = fx();
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for i in 0..4 {
        f.ext_wall(pts[i], pts[(i + 1) % 4]);
    }
    f.int_wall((4.0, 0.0), (4.0, 8.0));
    f.int_wall((4.0, 4.0), (10.0, 4.0));
    f.int_wall((2.0, 2.0), (2.0, 6.0));
    let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
    assert_eq!(geo.walls().count(), 7);
    for (id, l) in geo.walls() {
        assert_eq!(&wall_layout(&f.doc, id).unwrap(), l, "wall {id}");
    }
}

#[test]
fn arc_wall_joins_a_straight_wall() {
    let mut f = fx();
    let s = f.ext_wall((0.0, 0.0), (6.0, 0.0));
    let a = f.arc_wall((6.0, 0.0), (6.0, 6.0), 0.4);
    let ls = wall_layout(&f.doc, s).unwrap();
    let la = wall_layout(&f.doc, a).unwrap();
    let u = union_all(&[ls.outline.clone().into(), la.outline.clone().into()]).unwrap();
    assert_eq!(u.len(), 1);
    let sum = ls.outline.area().as_m2() + la.outline.area().as_m2();
    assert!(close(sum, area(&u), 1e-4), "overlap {sum} vs {}", area(&u));
    // The arc's layers stay within its outline.
    let layers: f64 = la.layers.iter().map(|b| b.region.area().as_m2()).sum();
    assert!(close(layers, la.outline.area().as_m2(), 1e-4));
}

#[test]
fn openings_make_gaps_and_sill_lines_in_plan() {
    let mut f = fx();
    let w = f.basic_wall((0.0, 0.0), (6.0, 0.0), 0.25, 3.0);
    f.opening(w, OpeningKind::Door, 1.0, 0.9, 0.0, 2.1);
    f.opening(w, OpeningKind::Window, 3.5, 1.2, 0.9, 1.2);
    f.opening(w, OpeningKind::Window, 5.2, 0.6, 1.8, 0.5);
    let cut = Length::m(1.1);
    let p = plan_parts(&f.doc, w, cut).unwrap();
    // Three pieces between and beside the door and the low window.
    assert_eq!(p.cut.len(), 3);
    let a: f64 = p.cut.iter().map(|c| c.region.area().as_m2()).sum();
    assert!(close(a, 0.25 * (6.0 - 0.9 - 1.2), 1e-9));
    // Through the window the wall below the sill shows on both faces.
    assert_eq!(p.below.len(), 2);
    // The high window is above the cut: its outline is dashed.
    assert_eq!(p.above.len(), 4);
    assert_eq!(p.outline.len(), 3);
}

#[test]
fn arc_wall_with_an_opening_has_a_gap() {
    let mut f = fx();
    let a = f.arc_wall((0.0, 0.0), (6.0, 0.0), 0.5);
    let len = kanavos_model::Wall {
        start: Point2::new(0.0, 0.0),
        end: Point2::new(6.0, 0.0),
        bulge: 0.5,
        reference: kanavos_model::ReferenceLine::Centre,
        structure: kanavos_model::Structure::Basic { material: f.brick, thickness: Length::m(0.3) },
        height: Length::m(3.0),
        base_offset: Length::ZERO,
    }
    .length()
    .as_m();
    f.opening(a, OpeningKind::Door, len / 2.0, 1.0, 0.0, 2.1);
    let p = plan_parts(&f.doc, a, Length::m(1.1)).unwrap();
    // Each of the four layers is split in two.
    assert_eq!(p.cut.len(), 8);
    let outline = wall_layout(&f.doc, a).unwrap().outline.area().as_m2();
    let cut: f64 = p.cut.iter().map(|c| c.region.area().as_m2()).sum();
    assert!(close(outline - cut, 1.0 * 0.31, 0.01), "gap {}", outline - cut);
}

#[test]
fn wall_volume_with_openings() {
    let mut f = fx();
    let w = f.basic_wall((0.0, 0.0), (5.0, 0.0), 0.25, 3.0);
    f.opening(w, OpeningKind::Window, 2.5, 1.2, 0.9, 1.2);
    let s = solids(&f.doc, w).unwrap();
    assert_eq!(s.len(), 1);
    assert!(s[0].mesh.is_closed(1e-6));
    let v = s[0].mesh.volume();
    assert!(close(v, 3.75 - 0.36, 1e-9), "{v}");
    let q = quantities(&f.doc, w).unwrap();
    assert!(close(q.volume.unwrap().as_m3(), 3.39, 1e-9));
    assert!(close(q.area.unwrap().as_m2(), 15.0 - 1.44, 1e-9));
    assert!(close(q.face_area_left.unwrap().as_m2(), 15.0 - 1.44, 1e-9));
}

#[test]
fn layered_wall_solids_are_closed_per_layer_with_holes() {
    let mut f = fx();
    let a = f.ext_wall((0.0, 0.0), (10.0, 0.0));
    f.ext_wall((10.0, 0.0), (10.0, 8.0));
    f.int_wall((5.0, 0.0), (5.0, 5.0));
    f.opening(a, OpeningKind::Window, 2.5, 1.5, 0.9, 1.4);
    f.opening(a, OpeningKind::Door, 7.5, 1.0, 0.0, 2.2);
    let s = solids(&f.doc, a).unwrap();
    assert_eq!(s.len(), 4);
    let layout = wall_layout(&f.doc, a).unwrap();
    let plan_area: f64 = layout.layers.iter().map(|b| b.region.area().as_m2()).sum();
    let mut total = 0.0;
    for solid in &s {
        assert!(
            solid.mesh.is_closed(1e-6),
            "{:?} open edges {}",
            solid.part,
            solid.mesh.welded(1e-7).open_edge_count()
        );
        total += solid.mesh.volume();
    }
    let expected = plan_area * 3.0 - 0.31 * (1.5 * 1.4 + 1.0 * 2.2);
    assert!(close(total, expected, 1e-6), "{total} vs {expected}");
}
