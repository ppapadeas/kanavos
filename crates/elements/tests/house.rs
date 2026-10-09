//! A two-storey house end to end: ground and first floor walls, floor slab, stair, gable roof on the first floor
//! with gable walls clipped to it, spaces, footprint and quantities.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::{close, fx};
use kanavos_elements::{
    RoofLineKind, StoreyGeometry, detect_all_spaces, plan_parts, quantities, roof_lines, solids,
    storey_footprint,
};
use kanavos_geom::{Angle, Length, Point2, Polygon2};
use kanavos_model::{ElementKind, ReferenceLine, Roof, RoofShape, Structure, Wall};

fn wall(
    f: &mut common::Fx,
    storey: kanavos_model::Id,
    a: (f64, f64),
    b: (f64, f64),
    build_up: kanavos_model::Id,
    h: f64,
) -> kanavos_model::Id {
    f.add(
        storey,
        ElementKind::Wall(Wall {
            start: Point2::new(a.0, a.1),
            end: Point2::new(b.0, b.1),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Layered { build_up },
            height: Length::m(h),
            base_offset: Length::ZERO,
        }),
    )
}

#[test]
fn two_storey_house_with_a_gable_roof() {
    let mut f = fx();
    let (g, first) = (f.ground, f.first);
    let ext = f.ext;
    let int = f.int;
    let box_pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 7.0), (0.0, 7.0)];
    // Ground floor: four exterior walls and an interior wall.
    for i in 0..4 {
        wall(&mut f, g, box_pts[i], box_pts[(i + 1) % 4], ext, 3.2);
    }
    wall(&mut f, g, (6.0, 0.0), (6.0, 7.0), int, 3.2);
    // Floor slab of the first floor, flush with the outer faces.
    let slab_build_up = f.slab_build_up;
    f.add(
        first,
        ElementKind::Slab(kanavos_model::Slab {
            outline: Polygon2::from_m(&[
                (-0.155, -0.155),
                (10.155, -0.155),
                (10.155, 7.155),
                (-0.155, 7.155),
            ])
            .into(),
            structure: Structure::Layered { build_up: slab_build_up },
            top_offset: Length::ZERO,
        }),
    );
    // First floor: long walls 2.8 m, gable walls tall enough to reach the ridge (they are clipped).
    wall(&mut f, first, box_pts[0], box_pts[1], ext, 2.8);
    let east = wall(&mut f, first, box_pts[1], box_pts[2], ext, 6.0);
    wall(&mut f, first, box_pts[2], box_pts[3], ext, 2.8);
    wall(&mut f, first, box_pts[3], box_pts[0], ext, 6.0);
    // Gable roof over the first floor, eaves 0.5 m out, top surface at the eaves +2.80 above the first floor.
    let roof_build_up = f.roof_build_up;
    let roof = f.add(
        first,
        ElementKind::Roof(Roof {
            outline: Polygon2::from_m(&[(-0.65, -0.65), (10.65, -0.65), (10.65, 7.65), (-0.65, 7.65)]),
            base: Length::m(2.8),
            structure: Structure::Layered { build_up: roof_build_up },
            shape: RoofShape::Pitched {
                slopes: vec![Some(Angle::deg(30.0)), None, Some(Angle::deg(30.0)), None],
            },
        }),
    );
    // Stair from the ground floor to the first floor.
    let stair = f.stair(kanavos_model::StairShape::Straight, 18, 3.2, 0.28, 1.0);

    // Every element validates and has solids or plan parts.
    for e in f.doc.elements() {
        kanavos_elements::validate(&f.doc, e).unwrap_or_else(|err| panic!("{}: {err}", e.id));
    }
    // Roof: one ridge over the middle of the house.
    let lines = roof_lines(&f.doc, roof).unwrap();
    let ridge = lines.iter().find(|l| l.kind == RoofLineKind::Ridge).unwrap();
    let tan = 30f64.to_radians().tan();
    let ridge_z = 3.2 + 2.8 + 4.15 * tan;
    assert!(close(ridge.z.0, ridge_z, 1e-6));
    // The east gable wall follows the roof underside: its peak is the ridge minus the vertical roof thickness.
    let s = solids(&f.doc, east).unwrap();
    let top = s.iter().map(|x| x.mesh.bbox().unwrap().max.z).fold(f64::MIN, f64::max);
    let t = 0.03 + 0.005 + 0.08 + 0.15 + 0.015;
    assert!(close(top, ridge_z - t / 30f64.to_radians().cos(), 1e-6), "{top}");
    for x in &s {
        assert!(x.mesh.is_closed(1e-6));
    }
    // Plans: the ground floor shows the stair cut and treads; the first floor is cut through its walls.
    let ground_cut = f.doc.storey(g).unwrap().cut_elevation();
    let p = plan_parts(&f.doc, stair, ground_cut).unwrap();
    assert!(!p.cut.is_empty() && !p.above.is_empty());
    let first_geo = StoreyGeometry::compute(&f.doc, first).unwrap();
    let first_cut = f.doc.storey(first).unwrap().cut_elevation();
    assert_eq!(first_geo.cut_outline(&f.doc, first_cut).len(), 1);
    // The roof seen from the first floor plan is above the cut: dashed lines only.
    let rp = plan_parts(&f.doc, roof, first_cut).unwrap();
    assert!(rp.cut.is_empty() && rp.below.is_empty() && !rp.above.is_empty());
    // Spaces: two rooms downstairs, one upstairs.
    assert_eq!(detect_all_spaces(&f.doc, g).unwrap().len(), 2);
    assert_eq!(detect_all_spaces(&f.doc, first).unwrap().len(), 1);
    // Coverage footprint of the ground floor: the outer faces.
    let fp = storey_footprint(&f.doc, g).unwrap();
    assert!(close(fp.iter().map(|p| p.area().as_m2()).sum::<f64>(), 10.31 * 7.31, 1e-6));
    // Roof area: plan 11.3 × 8.3, true area / cos 30°.
    let q = quantities(&f.doc, roof).unwrap();
    assert!(close(q.gross_area.unwrap().as_m2(), 11.3 * 8.3, 1e-6));
    assert!(close(q.area.unwrap().as_m2(), 11.3 * 8.3 / 30f64.to_radians().cos(), 1e-6));
}
