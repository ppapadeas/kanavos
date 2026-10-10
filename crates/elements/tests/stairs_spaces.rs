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
use kanavos_model::{ElementKind, RailingInfill, StairShape, StairUse};

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
    // 18 risers up to the first floor at +3.20: 0.178 m risers, 0.28 m goings.
    let good = f.stair(StairShape::Straight, 18, 3.2, 0.28, 1.0);
    assert!(stair_checks(&f.doc, good).unwrap().is_empty(), "{:?}", stair_checks(&f.doc, good));
    // A stair that stops short of the next floor gets a note.
    let short = f.stair(StairShape::Straight, 18, 3.06, 0.28, 1.0);
    let c = stair_checks(&f.doc, short).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].code, "stair.rise_mismatch");
    assert!(c[0].message(Locale::En).contains("3.20"));
    // Steep: 15 risers of 0.204 m and 0.22 m treads.
    let steep = f.stair(StairShape::Straight, 15, 3.06, 0.22, 0.8);
    let c = stair_checks(&f.doc, steep).unwrap();
    let codes: Vec<&str> = c.iter().map(|x| x.code).collect();
    assert!(codes.contains(&"stair.riser_high"));
    assert!(codes.contains(&"stair.going_short"));
    // 0.80 m is wide enough for the internal stair of a dwelling (the default use).
    assert!(!codes.iter().any(|c| c.starts_with("stair.width")), "{codes:?}");
    assert!(c.iter().any(|x| x.level == CheckLevel::Warning));
    // Messages exist in both languages and carry the numbers.
    let riser = c.iter().find(|x| x.code == "stair.riser_high").unwrap();
    assert!(riser.message(Locale::El).contains("0,20"), "{}", riser.message(Locale::El));
    assert!(riser.message(Locale::En).contains("0.20"));
    // A straight flight rising more than 3.60 m.
    let tall = f.stair(StairShape::Straight, 22, 3.80, 0.28, 1.0);
    assert!(stair_checks(&f.doc, tall).unwrap().iter().any(|x| x.code == "stair.flight_rise"));
}

/// Sets a stair's use.
fn with_use(f: &mut common::Fx, s: kanavos_model::Id, usage: StairUse) {
    let mut e = f.doc.element(s).unwrap().clone();
    if let ElementKind::Stair(st) = &mut e.kind {
        st.usage = usage;
    }
    f.doc = f.doc.with_element(e);
}

/// (code, limit) of every remark.
fn remarks(f: &common::Fx, s: kanavos_model::Id) -> Vec<(&'static str, f64)> {
    stair_checks(&f.doc, s).unwrap().into_iter().map(|c| (c.code, c.limit)).collect()
}

#[test]
fn the_use_of_a_stair_decides_its_limits() {
    let mut f = fx();
    // 0.178 m risers, 0.28 m going, 1.00 m wide: a good dwelling stair.
    let s = f.stair(StairShape::Straight, 18, 3.2, 0.28, 1.0);
    assert!(remarks(&f, s).is_empty());
    // Shared stair of a two-storey residential building: 0.90 m is enough.
    with_use(&mut f, s, StairUse::Residential);
    assert!(remarks(&f, s).is_empty());
    // Other uses may have type II stairs, but the building also needs one 1.20 m wide.
    with_use(&mut f, s, StairUse::OtherUses);
    assert_eq!(remarks(&f, s), vec![("stair.width_one_main", 1.20)]);
    // Assembly, education and health care need type I: 1.20 m wide.
    for u in [StairUse::AssemblyEducation, StairUse::HealthCare] {
        with_use(&mut f, s, u);
        assert_eq!(remarks(&f, s), vec![("stair.width_narrow", 1.20)], "{u:?}");
    }
    // 0.19 m risers: a note for a dwelling (type II only), a warning where type I is required.
    let steep = f.stair(StairShape::Straight, 17, 3.23, 0.27, 1.2);
    assert!(remarks(&f, steep).contains(&("stair.riser_type_ii", 0.18)));
    assert!(remarks(&f, steep).contains(&("stair.going_type_ii", 0.28)));
    with_use(&mut f, steep, StairUse::HealthCare);
    let r = remarks(&f, steep);
    assert!(r.contains(&("stair.riser_high", 0.18)) && r.contains(&("stair.going_short", 0.28)), "{r:?}");
    // Stairs to auxiliary rooms: 0.204 m risers and 0.24 m goings are within §4.5, and no step rule applies.
    let aux = f.stair(StairShape::Straight, 15, 3.06, 0.24, 0.7);
    assert!(remarks(&f, aux).iter().any(|(c, _)| *c == "stair.riser_high"));
    with_use(&mut f, aux, StairUse::Auxiliary);
    let r = remarks(&f, aux);
    assert!(r.iter().all(|(c, _)| *c == "stair.rise_mismatch"), "{r:?}");
    // Shared stairs of residential buildings over three storeys need 1.20 m (§2).
    let (d, id) = f.doc.with_new_id();
    f.doc = d.with_storey(kanavos_model::Storey::new(id, "2", Length::m(6.4), Length::m(3.2)));
    let (d, id) = f.doc.with_new_id();
    f.doc = d.with_storey(kanavos_model::Storey::new(id, "3", Length::m(9.6), Length::m(3.2)));
    with_use(&mut f, s, StairUse::Residential);
    assert_eq!(remarks(&f, s), vec![("stair.width_narrow", 1.20)]);
    // The limits as a table.
    let l = kanavos_elements::stair_limits(StairUse::Auxiliary, 2);
    assert!(l.step_rule.is_none() && l.headroom_min.is_none());
    assert!((l.riser_max - 0.23).abs() < 1e-12 && (l.going_min - 0.23).abs() < 1e-12);
}

#[test]
fn headroom_under_a_slab_above_the_stair() {
    let mut f = fx();
    // A straight stair along +X from the origin: treads 0.28 m deep, 1.0 m wide, risers 3.2 / 18.
    let s = f.stair(StairShape::Straight, 18, 3.2, 0.28, 1.0);
    let floor = f.doc.defaults().floor_slab.unwrap();
    let slab = |hole_start: f64| {
        ElementKind::Slab(kanavos_model::Slab {
            outline: kanavos_geom::PolygonWithHoles {
                outer: kanavos_geom::Polygon2::from_m(&[
                    (-2.0, -3.0),
                    (10.0, -3.0),
                    (10.0, 3.0),
                    (-2.0, 3.0),
                ]),
                holes: vec![kanavos_geom::Polygon2::from_m(&[
                    (hole_start, -0.6),
                    (4.8, -0.6),
                    (4.8, 0.6),
                    (hole_start, 0.6),
                ])],
            },
            structure: kanavos_model::Structure::Layered { build_up: floor },
            top_offset: Length::ZERO,
        })
    };
    let first = f.first;
    let id = f.add(first, slab(-0.2));
    // The stairwell spans the whole flight: no remark.
    assert!(remarks(&f, s).is_empty(), "{:?}", remarks(&f, s));
    // A stairwell that starts at x = 2.0: the slab's edge is over the tread from 1.96 m (after 8 risers).
    let set = |f: &mut common::Fx, start: f64| {
        let mut e = f.doc.element(id).unwrap().clone();
        e.kind = slab(start);
        f.doc = f.doc.with_element(e);
    };
    set(&mut f, 2.0);
    let c = stair_checks(&f.doc, s).unwrap();
    let h = c.iter().find(|x| x.code == "stair.headroom").unwrap();
    let underside = 3.2 - (0.01 + 0.05 + 0.20 + 0.015);
    assert!(close(h.value, underside - 8.0 * 3.2 / 18.0, 1e-9), "{}", h.value);
    assert!(close(h.limit, 2.20, 1e-12));
    assert!(h.message(Locale::El).contains("2,20"));
    // Auxiliary stairs are not checked for headroom (§4.5).
    with_use(&mut f, s, StairUse::Auxiliary);
    assert!(stair_checks(&f.doc, s).unwrap().iter().all(|x| x.code != "stair.headroom"));
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
        if infill == RailingInfill::Solid {
            assert_eq!(p.cut.len(), 1);
        } else {
            assert!(p.cut.is_empty());
            assert_eq!(p.below.len(), 2 * 2 + 2);
        }
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

#[test]
fn core_outline_stays_closed_with_columns_in_the_walls() {
    // Concrete columns cut the brick core of the walls they stand in; the core outline must still be one ring
    // around the whole building, not the pieces of core between columns.
    let mut f = fx();
    let ring = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for i in 0..4 {
        f.ext_wall(ring[i], ring[(i + 1) % 4]);
    }
    for at in [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0), (5.0, 0.0), (5.0, 8.0)] {
        f.column(at, 0.3);
    }
    let core = outline_from_walls(&f.doc, f.ground, OutlineFace::Core).unwrap();
    assert_eq!(core.len(), 1, "one closed core outline");
    let a = core[0].area().as_m2();
    assert!(a > 9.69 * 7.69 && a < 10.31 * 8.31, "core outline area {a}");
}
