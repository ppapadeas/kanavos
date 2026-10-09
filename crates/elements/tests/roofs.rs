//! Roof planes, lines, solids, plan parts and walls clipped by roofs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::{close, fx};
use kanavos_elements::{RoofLineKind, plan_parts, quantities, roof_lines, roof_planes, solids};
use kanavos_geom::Length;
use kanavos_model::{ElementKind, Parapet, RoofShape};

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
