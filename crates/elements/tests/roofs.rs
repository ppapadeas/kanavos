//! Roof planes, lines, solids, plan parts and walls clipped by roofs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::{Fx, close, fx};
use kanavos_elements::{
    GeometryError, RoofLineKind, footprint, plan_parts, quantities, roof_lines, roof_planes, solids, validate,
};
use kanavos_geom::{Length, Polygon2};
use kanavos_model::{ElementKind, Id, Parapet, RoofShape};

const RECT: [(f64, f64); 4] = [(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)];
const L: [(f64, f64); 6] = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 5.0), (5.0, 10.0), (0.0, 10.0)];

fn count(lines: &[kanavos_elements::RoofLine], kind: RoofLineKind) -> usize {
    lines.iter().filter(|l| l.kind == kind).count()
}

#[test]
fn mono_pitch_has_one_plane() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(20.0), None, None, None], 3.0);
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 1);
    let p = &planes[0];
    assert!(close(p.plan_area(), 60.0, 1e-9));
    assert!(close(p.pitch.as_deg(), 20.0, 1e-9));
    // Rises northwards from the south eave at +3.00.
    assert!(close(p.height.at(kanavos_geom::Point2::new(5.0, 0.0)), 3.0, 1e-9));
    assert!(close(
        p.height.at(kanavos_geom::Point2::new(5.0, 6.0)),
        3.0 + 6.0 * 20f64.to_radians().tan(),
        1e-9
    ));
    assert!(p.downhill.y < -0.99);
    let lines = roof_lines(&f.doc, r).unwrap();
    assert_eq!(count(&lines, RoofLineKind::Eave), 1);
    assert_eq!(count(&lines, RoofLineKind::Ridge), 0);
}

#[test]
fn gable_roof_has_two_planes_and_a_ridge() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(30.0), None, Some(30.0), None], 3.0);
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 2);
    for p in &planes {
        assert!(close(p.plan_area(), 30.0, 1e-9));
        assert!(close(p.true_area(), 30.0 / 30f64.to_radians().cos(), 1e-9));
    }
    let lines = roof_lines(&f.doc, r).unwrap();
    assert_eq!(count(&lines, RoofLineKind::Ridge), 1);
    let ridge = lines.iter().find(|l| l.kind == RoofLineKind::Ridge).unwrap();
    assert!(close(ridge.segment.length().as_m(), 10.0, 1e-9));
    assert!(close(ridge.z.0, 3.0 + 3.0 * 30f64.to_radians().tan(), 1e-9));
    assert_eq!(count(&lines, RoofLineKind::Eave), 2);
    assert_eq!(count(&lines, RoofLineKind::Verge), 4);
    assert_eq!(count(&lines, RoofLineKind::Hip), 0);
}

#[test]
fn hip_roof_has_four_planes_four_hips_and_a_ridge() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(35.0); 4], 3.0);
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 4);
    let total: f64 = planes.iter().map(|p| p.plan_area()).sum();
    assert!(close(total, 60.0, 1e-9));
    let lines = roof_lines(&f.doc, r).unwrap();
    assert_eq!(count(&lines, RoofLineKind::Hip), 4);
    assert_eq!(count(&lines, RoofLineKind::Ridge), 1);
    assert_eq!(count(&lines, RoofLineKind::Eave), 4);
    assert_eq!(count(&lines, RoofLineKind::Valley), 0);
    let q = quantities(&f.doc, r).unwrap();
    assert!(close(q.area.unwrap().as_m2(), 60.0 / 35f64.to_radians().cos(), 1e-9));
    assert!(close(q.gross_area.unwrap().as_m2(), 60.0, 1e-9));
}

#[test]
fn l_shaped_hip_roof_has_six_planes_and_a_valley() {
    let mut f = fx();
    let r = f.roof(&L, vec![Some(30.0); 6], 3.0);
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 6);
    let total: f64 = planes.iter().map(|p| p.plan_area()).sum();
    assert!(close(total, 75.0, 1e-6));
    let lines = roof_lines(&f.doc, r).unwrap();
    assert_eq!(count(&lines, RoofLineKind::Valley), 1, "{lines:#?}");
    assert_eq!(count(&lines, RoofLineKind::Ridge), 2);
    // One hip at each of the five outside corners.
    assert_eq!(count(&lines, RoofLineKind::Hip), 5);
    // Ridges at the height of half the arm width.
    for l in lines.iter().filter(|l| l.kind == RoofLineKind::Ridge) {
        assert!(close(l.z.0, 3.0 + 2.5 * 30f64.to_radians().tan(), 1e-6));
    }
}

#[test]
fn l_shaped_roof_with_gable_ends() {
    let mut f = fx();
    // Gables at the two arm ends (edges 1 and 4).
    let r = f.roof(&L, vec![Some(30.0), None, Some(30.0), Some(30.0), None, Some(30.0)], 3.0);
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 4);
    let total: f64 = planes.iter().map(|p| p.plan_area()).sum();
    assert!(close(total, 75.0, 1e-6));
    let lines = roof_lines(&f.doc, r).unwrap();
    assert_eq!(count(&lines, RoofLineKind::Valley), 1);
    assert_eq!(count(&lines, RoofLineKind::Hip), 1);
}

#[test]
fn different_pitches_shift_the_ridge() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(45.0), None, Some(26.565_051_177_077_99), None], 3.0);
    let planes = roof_planes(&f.doc, r).unwrap();
    // tan 45 = 1, tan 26.57 = 0.5: the ridge is 2 m from the south eave.
    let south = planes.iter().find(|p| p.edges.contains(&0)).unwrap();
    assert!(close(south.plan_area(), 20.0, 1e-6));
}

#[test]
fn roof_solids_are_closed_with_perpendicular_thickness() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(30.0); 4], 3.0);
    let s = solids(&f.doc, r).unwrap();
    // Five layers in the pitched build-up: tiles, membrane, XPS, concrete, plaster.
    assert_eq!(s.len(), 5);
    let mut volume = 0.0;
    for solid in &s {
        assert!(
            solid.mesh.is_closed(1e-6),
            "{:?}: {} open",
            solid.part,
            solid.mesh.welded(1e-7).open_edge_count()
        );
        volume += solid.mesh.volume();
    }
    // Plan area × vertical thickness (thickness / cos pitch) for a roof with equal pitches.
    let t = 0.03 + 0.005 + 0.08 + 0.15 + 0.015;
    assert!(close(volume, 60.0 * t / 30f64.to_radians().cos(), 1e-6), "{volume}");
}

#[test]
fn flat_roof_with_parapet() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![None; 4], 3.0);
    let mut doc = f.doc.clone();
    let mut e = doc.element(r).unwrap().clone();
    if let ElementKind::Roof(roof) = &mut e.kind {
        roof.shape =
            RoofShape::Flat { parapet: Some(Parapet { height: Length::m(0.6), thickness: Length::m(0.2) }) };
    }
    doc = doc.with_element(e);
    let planes = roof_planes(&doc, r).unwrap();
    assert_eq!(planes.len(), 1);
    assert!(close(planes[0].pitch.as_deg(), 0.0, 1e-12));
    let s = solids(&doc, r).unwrap();
    let parapet = s.iter().find(|x| x.part == kanavos_elements::SolidPart::Parapet).unwrap();
    assert!(parapet.mesh.is_closed(1e-6));
    assert!(close(parapet.mesh.volume(), (60.0 - 9.6 * 5.6) * 0.6, 1e-6));
}

#[test]
fn roof_plan_parts_show_lines_above_and_a_cut_band() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(30.0); 4], 1.0);
    // Cut at 1.5 m: the roof surface crosses the cut plane; the low eaves are below, the ridge above.
    let p = plan_parts(&f.doc, r, Length::m(1.5)).unwrap();
    assert!(!p.cut.is_empty());
    assert!(!p.below.is_empty());
    assert!(!p.above.is_empty());
    // A cut above everything: all lines below, nothing cut.
    let p = plan_parts(&f.doc, r, Length::m(20.0)).unwrap();
    assert!(p.cut.is_empty() && p.above.is_empty());
    assert_eq!(p.below.len(), 4 + 4 + 1);
}

#[test]
fn gable_wall_is_clipped_to_the_roof_underside() {
    let mut f = fx();
    // Gable roof over a 10 × 6 m box, eaves at 3.0 m, pitch 30°, gables at x = 0 and x = 10.
    let roof = f.roof(&RECT, vec![Some(30.0), None, Some(30.0), None], 3.0);
    let _ = roof;
    // A tall basic wall on the east gable line, inside the roof outline.
    let w = f.basic_wall((10.0 - 0.15, 0.3), (10.0 - 0.15, 5.7), 0.3, 8.0);
    let s = solids(&f.doc, w).unwrap();
    assert_eq!(s.len(), 1);
    assert!(s[0].mesh.is_closed(1e-6), "open edges {}", s[0].mesh.welded(1e-7).open_edge_count());
    let bbox = s[0].mesh.bbox().unwrap();
    let t = 0.03 + 0.005 + 0.08 + 0.15 + 0.015;
    let tan = 30f64.to_radians().tan();
    let peak = 3.0 + 3.0 * tan - t / 30f64.to_radians().cos();
    assert!(close(bbox.max.z, peak, 1e-6), "{} vs {peak}", bbox.max.z);
    // Volume: the gable pentagon (top following the underside) times the wall thickness.
    let under = |y: f64| 3.0 + (3.0 - (y - 3.0).abs()) * tan - t / 30f64.to_radians().cos();
    let n = 2000;
    let mut a = 0.0;
    for i in 0..n {
        let y = 0.3 + 5.4 * (i as f64 + 0.5) / n as f64;
        a += under(y) * 5.4 / n as f64;
    }
    assert!(close(s[0].mesh.volume(), a * 0.3, 1e-4), "{} vs {}", s[0].mesh.volume(), a * 0.3);
}

/// Gives roof `r` these holes (and optionally a flat shape with a parapet).
fn with_holes(f: &mut Fx, r: Id, holes: &[&[(f64, f64)]], flat: Option<Option<Parapet>>) {
    let mut e = f.doc.element(r).unwrap().clone();
    if let ElementKind::Roof(roof) = &mut e.kind {
        roof.holes = holes.iter().map(|h| Polygon2::from_m(h)).collect();
        if let Some(parapet) = flat {
            roof.shape = RoofShape::Flat { parapet };
        }
    }
    f.doc = f.doc.with_element(e);
}

/// A ring of plan points.
type Ring = &'static [(f64, f64)];

/// Total thickness of the pitched-roof build-up of the starter template.
const T: f64 = 0.03 + 0.005 + 0.08 + 0.15 + 0.015;

#[test]
fn a_skylight_is_cut_through_every_layer_of_a_hip_roof() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(30.0); 4], 3.0);
    // 2 × 1 m inside the south plane.
    with_holes(&mut f, r, &[&[(4.0, 0.5), (6.0, 0.5), (6.0, 1.5), (4.0, 1.5)]], None);
    validate(&f.doc, f.doc.element(r).unwrap()).unwrap();
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 4);
    let total: f64 = planes.iter().map(|p| p.plan_area()).sum();
    assert!(close(total, 58.0, 1e-6), "{total}");
    let lines = roof_lines(&f.doc, r).unwrap();
    assert_eq!(count(&lines, RoofLineKind::Opening), 4);
    let cos = 30f64.to_radians().cos();
    let tan = 30f64.to_radians().tan();
    // The lower edge of the hole lies 0.5 m up the south plane.
    assert!(lines.iter().any(|l| l.kind == RoofLineKind::Opening && close(l.z.0, 3.0 + 0.5 * tan, 1e-6)));
    assert_eq!(count(&lines, RoofLineKind::Hip), 4);
    assert_eq!(count(&lines, RoofLineKind::Eave), 4);
    // Quantities: plan and true area without the hole; the hole edges add to the perimeter.
    let q = quantities(&f.doc, r).unwrap();
    assert!(close(q.gross_area.unwrap().as_m2(), 58.0, 1e-6));
    assert!(close(q.area.unwrap().as_m2(), 58.0 / cos, 1e-6));
    assert!(close(q.perimeter.unwrap().as_m(), 32.0 + 6.0, 1e-9));
    // Every layer is a closed solid with the hole cut vertically through it.
    let s = solids(&f.doc, r).unwrap();
    assert_eq!(s.len(), 5);
    let mut volume = 0.0;
    for solid in &s {
        assert!(
            solid.mesh.is_closed(1e-6),
            "{:?}: {} open",
            solid.part,
            solid.mesh.welded(1e-7).open_edge_count()
        );
        volume += solid.mesh.volume();
    }
    assert!(close(volume, 58.0 * T / cos, 1e-6), "{volume}");
    // The footprint keeps the hole; a cut above everything shows the hole edges with the other lines.
    let fp = footprint(&f.doc, r).unwrap().unwrap();
    assert!(close(fp.area().as_m2(), 58.0, 1e-9));
    let p = plan_parts(&f.doc, r, Length::m(20.0)).unwrap();
    assert_eq!(p.below.len(), 4 + 4 + 1 + 4);
    // A cut through the roof slab at the hole: the cut band has a gap where the hole is.
    let p = plan_parts(&f.doc, r, Length::m(3.0 + 1.0 * tan - 0.05)).unwrap();
    let cut: f64 = p.cut.iter().map(|c| c.region.area().as_m2()).sum();
    assert!(cut > 0.0);
    assert!(
        p.cut.iter().all(|c| !c.region.contains(kanavos_geom::Point2::new(5.0, 1.0), &Default::default()))
    );
}

#[test]
fn a_hole_across_the_ridge_splits_it() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(30.0), None, Some(30.0), None], 3.0);
    with_holes(&mut f, r, &[&[(4.0, 2.5), (6.0, 2.5), (6.0, 3.5), (4.0, 3.5)]], None);
    let planes = roof_planes(&f.doc, r).unwrap();
    assert_eq!(planes.len(), 2);
    for p in &planes {
        assert!(close(p.plan_area(), 29.0, 1e-6));
    }
    let lines = roof_lines(&f.doc, r).unwrap();
    // The ridge stops at the hole: two pieces of 4 m.
    let ridges: Vec<_> = lines.iter().filter(|l| l.kind == RoofLineKind::Ridge).collect();
    assert_eq!(ridges.len(), 2);
    assert!(ridges.iter().all(|l| close(l.segment.length().as_m(), 4.0, 1e-6)));
    // The hole's sides each run over both planes.
    assert_eq!(count(&lines, RoofLineKind::Opening), 6);
    for solid in solids(&f.doc, r).unwrap() {
        assert!(solid.mesh.is_closed(1e-6));
    }
}

#[test]
fn a_courtyard_in_a_flat_roof_keeps_the_parapet() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![None; 4], 3.0);
    let parapet = Parapet { height: Length::m(0.6), thickness: Length::m(0.2) };
    with_holes(&mut f, r, &[&[(3.0, 2.0), (7.0, 2.0), (7.0, 4.0), (3.0, 4.0)]], Some(Some(parapet)));
    validate(&f.doc, f.doc.element(r).unwrap()).unwrap();
    let s = solids(&f.doc, r).unwrap();
    let layers: f64 =
        s.iter().filter(|x| x.part != kanavos_elements::SolidPart::Parapet).map(|x| x.mesh.volume()).sum();
    assert!(close(layers, 52.0 * T, 1e-6), "{layers}");
    let par = s.iter().find(|x| x.part == kanavos_elements::SolidPart::Parapet).unwrap();
    assert!(close(par.mesh.volume(), (60.0 - 9.6 * 5.6) * 0.6, 1e-6));
    assert_eq!(count(&roof_lines(&f.doc, r).unwrap(), RoofLineKind::Opening), 4);
}

#[test]
fn walls_under_a_hole_are_not_capped_there() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![None; 4], 3.0);
    with_holes(&mut f, r, &[&[(4.0, 2.0), (6.0, 2.0), (6.0, 4.0), (4.0, 4.0)]], Some(None));
    let w = f.basic_wall((3.0, 3.0), (7.0, 3.0), 0.3, 8.0);
    let s = solids(&f.doc, w).unwrap();
    assert!(s[0].mesh.is_closed(1e-6), "open edges {}", s[0].mesh.welded(1e-7).open_edge_count());
    assert!(close(s[0].mesh.bbox().unwrap().max.z, 8.0, 1e-9), "the wall rises through the hole");
    // 2 m under the roof (to its underside) and 2 m through the hole.
    let expected = 0.3 * (2.0 * (3.0 - T) + 2.0 * 8.0);
    assert!(close(s[0].mesh.volume(), expected, 1e-6), "{} vs {expected}", s[0].mesh.volume());
}

#[test]
fn bad_holes_are_refused() {
    let mut f = fx();
    let r = f.roof(&RECT, vec![Some(30.0); 4], 3.0);
    let check = |f: &Fx| validate(&f.doc, f.doc.element(r).unwrap());
    let field_of = |e: GeometryError| match e {
        GeometryError::Invalid { field, reason, .. } => (field, reason),
        other => panic!("{other:?}"),
    };
    let cases: [(&[Ring], &str); 5] = [
        (&[&[(9.0, 1.0), (11.0, 1.0), (11.0, 2.0), (9.0, 2.0)]], "inside"),
        (
            &[
                &[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)],
                &[(2.0, 2.0), (4.0, 2.0), (4.0, 4.0), (2.0, 4.0)],
            ],
            "overlap",
        ),
        (&[&[(1.0, 1.0), (1.0, 1.0), (2.0, 1.0)]], "three points"),
        (&[&[(1.0, 1.0), (3.0, 3.0), (3.0, 1.0), (1.0, 2.0)]], "cross"),
        (&[&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]], "no roof"),
    ];
    for (holes, why) in cases {
        with_holes(&mut f, r, holes, None);
        let (field, reason) = field_of(check(&f).unwrap_err());
        assert_eq!(field, "holes");
        assert!(reason.contains(why), "{why}: {reason}");
    }
    // Two holes that only touch, and a hole on the outline's edge, are fine.
    with_holes(
        &mut f,
        r,
        &[
            &[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)],
            &[(3.0, 1.0), (5.0, 1.0), (5.0, 3.0), (3.0, 3.0)],
        ],
        None,
    );
    check(&f).unwrap();
    // On a flat roof with a parapet, holes keep clear of the parapet.
    let parapet = Parapet { height: Length::m(1.0), thickness: Length::m(0.3) };
    with_holes(&mut f, r, &[&[(0.1, 1.0), (2.0, 1.0), (2.0, 2.0), (0.1, 2.0)]], Some(Some(parapet)));
    assert!(field_of(check(&f).unwrap_err()).1.contains("parapet"));
}
