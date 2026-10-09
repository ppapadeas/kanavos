//! Edge cases of wall junctions: offset reference lines, arcs meeting arcs, overlapping walls, walls at different
//! heights, near-collinear and acute corners.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::{area, close, fx};
use kanavos_elements::{JunctionKind, StoreyGeometry, wall_layout};
use kanavos_geom::{Length, PolygonWithHoles, union_all};
use kanavos_model::{ElementKind, ReferenceLine};

fn set_reference(f: &mut common::Fx, id: kanavos_model::Id, r: ReferenceLine) {
    let mut e = f.doc.element(id).unwrap().clone();
    if let ElementKind::Wall(w) = &mut e.kind {
        w.reference = r;
    }
    f.doc = f.doc.with_element(e);
}

fn outlines(f: &common::Fx) -> Vec<PolygonWithHoles> {
    let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
    geo.walls().map(|(_, l)| l.outline.clone().into()).collect()
}

#[test]
fn walls_drawn_on_their_outer_face_close_on_the_reference_polygon() {
    let mut f = fx();
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for i in 0..4 {
        let w = f.ext_wall(pts[i], pts[(i + 1) % 4]);
        // Counter-clockwise plan: the right side is outside; the reference line is the outer face.
        set_reference(&mut f, w, ReferenceLine::Right);
    }
    let u = union_all(&outlines(&f)).unwrap();
    assert_eq!(u.len(), 1);
    assert!(close(u[0].outer.area().as_m2(), 80.0, 1e-6), "{}", u[0].outer.area().as_m2());
    assert!(close(u[0].holes[0].area().as_m2(), (10.0 - 0.62) * (8.0 - 0.62), 1e-6));
}

#[test]
fn mixed_reference_lines_still_meet() {
    let mut f = fx();
    let a = f.ext_wall((0.0, 0.0), (6.0, 0.0));
    let b = f.ext_wall((6.0, 0.0), (6.0, 4.0));
    set_reference(&mut f, a, ReferenceLine::Right);
    // b keeps its centred reference: the faces still meet at the joint line.
    let la = wall_layout(&f.doc, a).unwrap();
    let lb = wall_layout(&f.doc, b).unwrap();
    let u = union_all(&[la.outline.clone().into(), lb.outline.clone().into()]).unwrap();
    assert_eq!(u.len(), 1);
    let sum = la.outline.area().as_m2() + lb.outline.area().as_m2();
    assert!(close(sum, area(&u), 1e-6), "overlap {sum} vs {}", area(&u));
}

#[test]
fn two_half_circles_make_a_ring() {
    let mut f = fx();
    // Two arcs with bulge 1 (half circles) between (0,0) and (6,0): a full circle of radius 3.
    f.arc_wall((0.0, 0.0), (6.0, 0.0), 1.0);
    f.arc_wall((6.0, 0.0), (0.0, 0.0), 1.0);
    let o = outlines(&f);
    let u = union_all(&o).unwrap();
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].holes.len(), 1);
    let ring = std::f64::consts::PI * (3.155f64.powi(2) - 2.845f64.powi(2));
    assert!(close(area(&u), ring, 0.01), "{} vs {ring}", area(&u));
    let sum: f64 = o.iter().map(|p| p.area().as_m2()).sum();
    assert!(close(sum, area(&u), 1e-3), "overlap {sum} vs {}", area(&u));
}

#[test]
fn overlapping_parallel_walls_are_not_joined() {
    let mut f = fx();
    let a = f.int_wall((0.0, 0.0), (5.0, 0.0));
    let b = f.int_wall((5.0, 0.0), (3.0, 0.0)); // doubles back over a
    let la = wall_layout(&f.doc, a).unwrap();
    let lb = wall_layout(&f.doc, b).unwrap();
    assert!(la.joins.is_empty() && lb.joins.is_empty());
    assert!(close(la.outline.area().as_m2(), 0.65, 1e-9));
    assert!(close(lb.outline.area().as_m2(), 0.26, 1e-9));
}

#[test]
fn walls_at_different_heights_do_not_join() {
    let mut f = fx();
    let a = f.int_wall((0.0, 0.0), (5.0, 0.0));
    let b = f.int_wall((5.0, 0.0), (5.0, 4.0));
    let mut e = f.doc.element(b).unwrap().clone();
    if let ElementKind::Wall(w) = &mut e.kind {
        w.base_offset = Length::m(3.5); // a wall on top, above a
    }
    f.doc = f.doc.with_element(e);
    assert!(wall_layout(&f.doc, a).unwrap().joins.is_empty());
}

#[test]
fn nearly_collinear_walls_continue_and_acute_corners_stay_bounded() {
    let mut f = fx();
    let a = f.int_wall((0.0, 0.0), (5.0, 0.0));
    let b = f.int_wall((5.0, 0.0), (10.0, 0.01)); // 0.11° off straight
    let la = wall_layout(&f.doc, a).unwrap();
    assert_eq!(la.joins[0].kind, JunctionKind::Continuation);
    let _ = b;
    // A 15° corner: the mitre would reach far; it is bounded by the bisector fallback.
    let mut g = fx();
    let c = g.int_wall((0.0, 0.0), (5.0, 0.0));
    let d = g.int_wall((5.0, 0.0), (0.2, 1.286));
    for id in [c, d] {
        let l = wall_layout(&g.doc, id).unwrap();
        let b = l.outline.bbox().unwrap();
        assert!(b.max.x.as_m() < 5.0 + 1.0, "{:?}", l.outline);
    }
}

#[test]
fn arc_wall_tee_onto_a_straight_wall() {
    let mut f = fx();
    let a = f.ext_wall((0.0, 0.0), (10.0, 0.0));
    let c = f.arc_wall((5.0, 0.0), (5.0, 5.0), 0.3);
    let la = wall_layout(&f.doc, a).unwrap();
    let lc = wall_layout(&f.doc, c).unwrap();
    assert_eq!(lc.joins[0].kind, JunctionKind::Tee);
    let u = union_all(&[la.outline.clone().into(), lc.outline.clone().into()]).unwrap();
    assert_eq!(u.len(), 1);
    let sum = la.outline.area().as_m2() + lc.outline.area().as_m2();
    assert!(close(sum, area(&u), 1e-4), "overlap {sum} vs {}", area(&u));
}

#[test]
fn arc_wall_with_openings_is_a_closed_solid() {
    let mut f = fx();
    let a = f.arc_wall((0.0, 0.0), (6.0, 0.0), 0.6);
    f.opening(a, kanavos_model::OpeningKind::Window, 2.0, 1.0, 0.9, 1.2);
    f.opening(a, kanavos_model::OpeningKind::Door, 5.0, 0.9, 0.0, 2.1);
    let s = kanavos_elements::solids(&f.doc, a).unwrap();
    assert_eq!(s.len(), 4);
    let mut v = 0.0;
    for x in &s {
        assert!(x.mesh.is_closed(1e-6), "{:?}: {} open", x.part, x.mesh.welded(1e-7).open_edge_count());
        v += x.mesh.volume();
    }
    let l = wall_layout(&f.doc, a).unwrap();
    let plan: f64 = l.layers.iter().map(|b| b.region.area().as_m2()).sum();
    // Openings remove width (on the reference line) × height × thickness, within the arc's chord error.
    let expected = plan * 3.0 - 0.31 * (1.0 * 1.2 + 0.9 * 2.1);
    assert!(close(v, expected, 0.01), "{v} vs {expected}");
}

#[test]
fn storey_cut_outline_joins_walls() {
    let mut f = fx();
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for i in 0..4 {
        f.ext_wall(pts[i], pts[(i + 1) % 4]);
    }
    f.int_wall((4.0, 0.0), (4.0, 8.0));
    let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
    let o = geo.cut_outline(&f.doc, Length::m(1.1));
    assert_eq!(o.len(), 1);
    assert_eq!(o[0].holes.len(), 2);
}

#[test]
fn arc_tighter_than_the_wall_is_refused() {
    let mut f = fx();
    let a = f.arc_wall((0.0, 0.0), (0.2, 0.0), 1.0); // radius 0.1 m, wall 0.31 m thick
    let e = f.doc.element(a).unwrap().clone();
    assert!(kanavos_elements::validate(&f.doc, &e).is_err());
}
