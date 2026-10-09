//! Stairs, railings, spaces, footprints and outlines.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::{area, close, fx};
use kanavos_elements::{
    CheckLevel, OutlineFace, detect_all_spaces, detect_space, outline_from_walls, outline_with_overhang,
    plan_parts, quantities, railing_checks, solids, stair_checks, stair_layout, storey_footprint,
};
use kanavos_geom::{Length, Point2};
use kanavos_i18n::Locale;
use kanavos_model::{RailingInfill, StairShape};

#[test]
fn straight_stair_layout_counts_risers() {
    let mut f = fx();
    let s = f.stair(StairShape::Straight, 18, 3.06, 0.28, 1.0);
    let l = stair_layout(&f.doc, s).unwrap();
    assert_eq!(l.flights.len(), 1);
    assert_eq!(l.flights[0].treads.len(), 17);
    assert_eq!(l.tread_elevations.len(), 17);
    assert!(close(l.riser_height.as_m(), 0.17, 1e-12));
    assert!(close(l.walking_length().as_m(), 17.0 * 0.28, 1e-9));
    assert!(close(l.top, 3.06, 1e-12));
}

#[test]
fn quarter_and_half_turn_stairs_keep_the_tread_count() {
    for shape in [
        StairShape::QuarterTurnLeft,
        StairShape::QuarterTurnRight,
        StairShape::HalfTurnLeft,
        StairShape::HalfTurnRight,
    ] {
        let mut f = fx();
        let s = f.stair(shape, 18, 3.06, 0.28, 1.0);
        let l = stair_layout(&f.doc, s).unwrap();
        assert_eq!(l.flights.len(), 2);
        let treads: usize = l.flights.iter().map(|x| x.treads.len()).sum();
        // 17 treads in all, one of which is the landing.
        assert_eq!(treads + l.landings.len(), 17, "{shape:?}");
        assert_eq!(l.landings.len(), 1);
        assert!(close(l.landing_elevations[0], 9.0 * 0.17, 1e-12));
        assert_eq!(l.flights[1].first_riser, 9);
    }
    // The half-turn landing spans both flights and the gap.
    let mut f = fx();
    let s = f.stair(StairShape::HalfTurnLeft, 18, 3.06, 0.28, 1.0);
    let l = stair_layout(&f.doc, s).unwrap();
    assert!(close(l.landings[0].area().as_m2(), 1.0 * 2.1, 1e-9));
}

#[test]
fn stair_solids_have_a_sloped_waist_and_landings() {
    let mut f = fx();
    let s = f.stair(StairShape::QuarterTurnLeft, 18, 3.06, 0.28, 1.0);
    let sol = solids(&f.doc, s).unwrap();
    assert_eq!(sol.len(), 3);
    for x in &sol {
        assert!(x.mesh.is_closed(1e-6), "{:?}", x.part);
        assert!(x.mesh.volume() > 0.0);
    }
    // A flight's volume: steps (n triangles r·g/2 across the width) plus the waist slab.
    let flight = &sol[0];
    let (n, r, g, w, t): (f64, f64, f64, f64, f64) = (8.0, 0.17, 0.28, 1.0, 0.15);
    let hyp = (r * r + g * g).sqrt();
    let steps = n * r * g / 2.0 * w;
    let waist = t * hyp / g * n * g * w;
    let v = flight.mesh.volume();
    assert!(v > steps && v < steps + waist * 1.2, "{v}");
    let q = quantities(&f.doc, s).unwrap();
    assert!(close(q.height.unwrap().as_m(), 3.06, 1e-12));
}

#[test]
fn stair_plan_cuts_the_flight_at_the_cut_plane() {
    let mut f = fx();
    let s = f.stair(StairShape::Straight, 18, 3.06, 0.28, 1.0);
    let p = plan_parts(&f.doc, s, Length::m(1.1)).unwrap();
    assert!(!p.cut.is_empty());
    // Treads up to 1.1 m (6 of them) below, the rest above.
    assert_eq!(p.below.len(), 6 * 4);
    assert_eq!(p.above.len(), 11 * 4);
}

#[test]
fn stair_checks_follow_the_regulation() {
    let mut f = fx();
    let good = f.stair(StairShape::Straight, 18, 3.06, 0.28, 1.0);
    assert!(stair_checks(&f.doc, good).unwrap().is_empty());
    // Steep: 15 risers of 0.204 m and 0.22 m treads.
    let steep = f.stair(StairShape::Straight, 15, 3.06, 0.22, 0.8);
    let c = stair_checks(&f.doc, steep).unwrap();
    let codes: Vec<&str> = c.iter().map(|x| x.code).collect();
    assert!(codes.contains(&"stair.riser_high"));
    assert!(codes.contains(&"stair.going_short"));
    assert!(codes.contains(&"stair.width_dwelling_only"));
    assert!(c.iter().any(|x| x.level == CheckLevel::Warning));
    // Messages exist in both languages and carry the numbers.
    let riser = c.iter().find(|x| x.code == "stair.riser_high").unwrap();
    assert!(riser.message(Locale::El).contains("0,20"), "{}", riser.message(Locale::El));
    assert!(riser.message(Locale::En).contains("0.20"));
    // A straight flight rising more than 3.60 m.
    let tall = f.stair(StairShape::Straight, 22, 3.80, 0.28, 1.0);
    assert!(stair_checks(&f.doc, tall).unwrap().iter().any(|x| x.code == "stair.flight_rise"));
}

#[test]
fn railing_solids_and_checks() {
    let mut f = fx();
    for infill in [RailingInfill::Bars, RailingInfill::Glass, RailingInfill::Solid] {
        let r = f.railing(&[(0.0, 0.0), (3.0, 0.0), (3.0, 2.0)], 1.0, infill);
        let s = solids(&f.doc, r).unwrap();
        assert!(s.len() >= 2);
        for x in &s {
            assert!(x.mesh.volume() > 0.0, "{infill:?} {:?}", x.part);
        }
        assert!(railing_checks(&f.doc, r).unwrap().is_empty());
        let p = plan_parts(&f.doc, r, Length::m(0.5)).unwrap();
        assert_eq!(p.cut.len(), 1);
    }
    let low = f.railing(&[(0.0, 0.0), (3.0, 0.0)], 0.9, RailingInfill::Bars);
    assert_eq!(railing_checks(&f.doc, low).unwrap()[0].code, "railing.low");
}

/// A 10 × 8 m house with an interior wall at x = 4 and a column in the right room.
fn small_plan() -> common::Fx {
    let mut f = fx();
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for i in 0..4 {
        let w = f.ext_wall(pts[i], pts[(i + 1) % 4]);
        if i == 0 {
            f.opening(w, kanavos_model::OpeningKind::Door, 7.0, 1.0, 0.0, 2.2);
        }
    }
    let inner = f.int_wall((4.0, 0.0), (4.0, 8.0));
    f.opening(inner, kanavos_model::OpeningKind::Door, 4.0, 0.9, 0.0, 2.1);
    f.column((7.0, 4.0), 0.3);
    f
}

#[test]
fn detected_rooms_have_the_clear_areas() {
    let f = small_plan();
    let left = detect_space(&f.doc, f.ground, Point2::new(2.0, 4.0)).unwrap();
    // x from 0.155 to 3.935, y from 0.155 to 7.845.
    assert!(close(left.area().as_m2(), 3.78 * 7.69, 1e-6), "{}", left.area().as_m2());
    assert!(left.holes.is_empty());
    let right = detect_space(&f.doc, f.ground, Point2::new(8.0, 2.0)).unwrap();
    assert_eq!(right.holes.len(), 1, "the column is an island");
    assert!(close(right.area().as_m2(), 5.78 * 7.69 - 0.09, 1e-6), "{}", right.area().as_m2());
    let all = detect_all_spaces(&f.doc, f.ground).unwrap();
    assert_eq!(all.len(), 2);
    // Inside a wall and outside the building.
    assert!(detect_space(&f.doc, f.ground, Point2::new(4.0, 4.0)).is_err());
    assert!(detect_space(&f.doc, f.ground, Point2::new(20.0, 4.0)).is_err());
}

#[test]
fn footprint_and_outlines() {
    let f = small_plan();
    let fp = storey_footprint(&f.doc, f.ground).unwrap();
    assert_eq!(fp.len(), 1);
    assert!(close(area(&fp), 10.31 * 8.31, 1e-6));
    let outer = outline_from_walls(&f.doc, f.ground, OutlineFace::Outer).unwrap();
    assert_eq!(outer.len(), 1);
    assert!(close(outer[0].area().as_m2(), 10.31 * 8.31, 1e-6));
    // Core: the outer face of the 20 cm brick, 9 cm in from the outer face (1 + 8 cm).
    let core = outline_from_walls(&f.doc, f.ground, OutlineFace::Core).unwrap();
    assert_eq!(core.len(), 1);
    let w = 10.31 - 2.0 * 0.09;
    let h = 8.31 - 2.0 * 0.09;
    // The starter build-up puts the render on the walls' left: inside for a counter-clockwise plan. Either way
    // the core outline is between the outer and the inner face.
    let a = core[0].area().as_m2();
    assert!(a < 10.31 * 8.31 && a > 9.69 * 7.69, "{a} (outer-face core would be {})", w * h);
    let eaves = outline_with_overhang(&outer, Length::m(0.5)).unwrap();
    assert!(close(eaves[0].area().as_m2(), 11.31 * 9.31, 1e-6));
}

#[test]
fn footprint_with_a_slab_keeps_light_wells() {
    let mut f = small_plan();
    let s = f.slab(&[(-0.155, -0.155), (10.155, -0.155), (10.155, 8.155), (-0.155, 8.155)], 0.0);
    let mut doc = f.doc.clone();
    let mut e = doc.element(s).unwrap().clone();
    if let kanavos_model::ElementKind::Slab(slab) = &mut e.kind {
        slab.outline.holes.push(kanavos_geom::Polygon2::from_m(&[
            (1.0, 1.0),
            (2.0, 1.0),
            (2.0, 2.0),
            (1.0, 2.0),
        ]));
    }
    doc = doc.with_element(e);
    let fp = storey_footprint(&doc, f.ground).unwrap();
    assert!(close(area(&fp), 10.31 * 8.31 - 1.0, 1e-6), "{}", area(&fp));
}
