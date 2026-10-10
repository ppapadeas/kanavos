//! Visual check: a small house rasterised to PNG (plan of the ground floor, roof planes, a vertical section).
//!
//! Run with `KANAVOS_PNG_DIR=<dir> cargo test -p kanavos-elements --test visual` to write the images; without the
//! variable the test still computes everything and checks basic properties.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::raster::{Canvas, material_colour};
use common::{Fx, fx};
use kanavos_elements::{RoofLineKind, StoreyGeometry, plan_parts, roof_lines, roof_planes, solids};
use kanavos_geom::{Bbox2, Length, Point2, PolygonWithHoles, Segment2, union_all};
use kanavos_model::{ElementKind, OpeningKind, RailingInfill, StairShape};
use kanavos_solid::{PlaneFrame, section};

/// Ground floor of a house: exterior walls with a curved bay, interior walls meeting in tees and a cross, a
/// column, doors and windows (one high), a quarter-turn stair with a railing, and an L-shaped hip roof.
fn house() -> (Fx, kanavos_model::Id) {
    let mut f = fx();
    let pts = [(0.0, 0.0), (12.0, 0.0), (12.0, 6.0), (7.0, 6.0), (7.0, 10.0), (0.0, 10.0)];
    let mut ext = Vec::new();
    for i in 0..pts.len() {
        if i == 1 {
            // A curved bay on the east wall.
            ext.push(f.arc_wall(pts[1], pts[2], 0.25));
        } else {
            ext.push(f.ext_wall(pts[i], pts[(i + 1) % pts.len()]));
        }
    }
    f.opening(ext[0], OpeningKind::Door, 3.0, 1.0, 0.0, 2.2);
    f.opening(ext[0], OpeningKind::Window, 8.0, 1.6, 0.9, 1.4);
    f.opening(ext[4], OpeningKind::Window, 3.0, 1.2, 0.9, 1.4);
    f.opening(ext[5], OpeningKind::Window, 2.0, 0.6, 1.7, 0.5); // high window above the cut
    let a = f.int_wall((5.0, 0.0), (5.0, 10.0));
    f.opening(a, OpeningKind::Door, 4.5, 0.9, 0.0, 2.1);
    f.int_wall((5.0, 6.0), (7.0, 6.0));
    f.int_wall((0.0, 4.0), (5.0, 4.0));
    // A short wall crossing the interior wall.
    f.int_wall((4.0, 7.5), (6.5, 7.5));
    f.column((9.0, 3.0), 0.35);
    let stair = f.stair(StairShape::QuarterTurnLeft, 18, 3.06, 0.28, 1.0);
    let mut doc = f.doc.clone();
    let mut e = doc.element(stair).unwrap().clone();
    if let ElementKind::Stair(s) = &mut e.kind {
        s.origin = Point2::new(1.0, 5.0);
    }
    doc = doc.with_element(e);
    f.doc = doc;
    f.railing(&[(2.0, 4.5), (2.0, 9.4)], 1.0, RailingInfill::Bars);
    // L-shaped hip roof with 0.5 m eaves, on the same storey at +3.00.
    let roof = f.roof(
        &[(-0.65, -0.65), (12.65, -0.65), (12.65, 6.65), (7.65, 6.65), (7.65, 10.65), (-0.65, 10.65)],
        vec![Some(30.0); 6],
        3.0,
    );
    (f, roof)
}

fn png_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("KANAVOS_PNG_DIR").map(std::path::PathBuf::from)
}

#[test]
fn house_plan_roof_and_section() {
    let (f, roof) = house();
    let doc = &f.doc;
    let ground = f.ground;
    let geo = StoreyGeometry::compute(doc, ground).unwrap();
    let cut = doc.storey(ground).unwrap().cut_elevation();
    let bounds = Bbox2 { min: Point2::new(-1.0, -1.0), max: Point2::new(13.5, 11.0) };

    // Plan.
    let mut plan = Canvas::new(bounds, 80.0);
    let mut cut_regions: Vec<PolygonWithHoles> = Vec::new();
    let mut belows = Vec::new();
    let mut aboves = Vec::new();
    for e in doc.elements_on(ground) {
        if !e.category().is_building_part() || matches!(e.kind, ElementKind::Roof(_)) {
            continue;
        }
        let parts = geo.plan_parts(doc, e.id, cut).unwrap_or_default();
        for c in &parts.cut {
            plan.fill(std::slice::from_ref(&c.region), material_colour(c.material));
            cut_regions.push(c.region.clone());
        }
        for c in &parts.cut {
            plan.outline(std::slice::from_ref(&c.region), [120, 120, 120]);
        }
        belows.extend(parts.below);
        aboves.extend(parts.above);
    }
    for s in &belows {
        plan.line(s, [60, 160, 60]);
    }
    for s in &aboves {
        plan.dashed(s, [40, 80, 220]);
    }
    // Heavy outline of all walls together.
    let all = union_all(&cut_regions).unwrap();
    plan.outline(&all, [0, 0, 0]);
    // Junction markers.
    for j in geo.junctions() {
        let p = j.at;
        let d = 0.08;
        plan.line(
            &Segment2::new(p - kanavos_geom::Vec2::new(d, 0.0), p + kanavos_geom::Vec2::new(d, 0.0)),
            [220, 0, 0],
        );
        plan.line(
            &Segment2::new(p - kanavos_geom::Vec2::new(0.0, d), p + kanavos_geom::Vec2::new(0.0, d)),
            [220, 0, 0],
        );
    }
    assert!(!cut_regions.is_empty());
    assert!(!aboves.is_empty(), "the high window shows above the cut");

    // Roof planes.
    let mut roof_img = Canvas::new(bounds, 60.0);
    let planes = roof_planes(doc, roof).unwrap();
    assert_eq!(planes.len(), 6);
    let colours = [
        [230, 160, 140],
        [160, 200, 230],
        [190, 230, 160],
        [240, 220, 140],
        [210, 170, 230],
        [170, 220, 210],
    ];
    for (i, p) in planes.iter().enumerate() {
        roof_img.fill(&p.region, colours[i % colours.len()]);
    }
    for l in roof_lines(doc, roof).unwrap() {
        let c = match l.kind {
            RoofLineKind::Ridge => [200, 0, 0],
            RoofLineKind::Hip => [0, 0, 200],
            RoofLineKind::Valley => [0, 150, 0],
            RoofLineKind::Eave => [0, 0, 0],
            RoofLineKind::Verge => [120, 0, 120],
            RoofLineKind::Step => [255, 120, 0],
            RoofLineKind::Opening => [0, 160, 160],
        };
        roof_img.line(&l.segment, c);
    }

    // Vertical section through the south windows and the stair (x–z), looking north.
    let frame = PlaneFrame::vertical(Point2::new(-1.0, 0.0), Point2::new(13.0, 0.0), 0.0).unwrap();
    let frame = PlaneFrame { origin: kanavos_geom::Point3::new(-1.0, 0.0, 0.0), ..frame };
    let mut sec_polys: Vec<(PolygonWithHoles, Option<kanavos_model::Id>)> = Vec::new();
    for e in doc.elements_on(ground) {
        let Ok(sol) = geo.solids(doc, e.id) else { continue };
        for s in sol {
            for p in section(&s.mesh, &frame, 1e-9) {
                sec_polys.push((p, s.material));
            }
        }
    }
    assert!(!sec_polys.is_empty());
    let sec_bounds = Bbox2 { min: Point2::new(0.0, -0.5), max: Point2::new(14.0, 7.0) };
    let mut sec = Canvas::new(sec_bounds, 60.0);
    for (p, m) in &sec_polys {
        sec.fill(std::slice::from_ref(p), material_colour(*m));
        sec.outline(std::slice::from_ref(p), [0, 0, 0]);
    }

    // 3D solids of every element are closed.
    for e in doc.elements_on(ground) {
        if let Ok(sol) = geo.solids(doc, e.id) {
            for s in sol {
                if matches!(e.kind, ElementKind::Railing(_)) {
                    continue; // posts and bars are separate closed boxes; checked elsewhere
                }
                assert!(
                    s.mesh.is_closed(1e-6),
                    "{} {:?} open edges {}",
                    e.id,
                    s.part,
                    s.mesh.welded(1e-7).open_edge_count()
                );
            }
        }
    }

    if let Some(dir) = png_dir() {
        std::fs::create_dir_all(&dir).unwrap();
        plan.write_png(&dir.join("house_plan.png")).unwrap();
        roof_img.write_png(&dir.join("house_roof.png")).unwrap();
        sec.write_png(&dir.join("house_section.png")).unwrap();
        // The same plan through the single-element API, to compare visually with the storey path.
        let mut single = Canvas::new(bounds, 80.0);
        for e in doc.elements_on(ground) {
            if let Ok(p) = plan_parts(doc, e.id, cut) {
                for c in &p.cut {
                    single.fill(std::slice::from_ref(&c.region), material_colour(c.material));
                }
            }
        }
        single.write_png(&dir.join("house_plan_single.png")).unwrap();
        // Isometric flat-shaded views from the south-east, with and without the roof, and the stair alone from
        // below.
        for (name, with_roof) in
            [("house_3d.png", true), ("house_3d_open.png", false), ("stair_3d.png", false)]
        {
            let stair_only = name == "stair_3d.png";
            let view = if stair_only {
                kanavos_geom::Vec3::new(-0.3, 1.0, 0.25).normalized().unwrap()
            } else {
                kanavos_geom::Vec3::new(-1.0, 1.0, -0.9).normalized().unwrap()
            };
            let right = view.cross(kanavos_geom::Vec3::Z).normalized().unwrap();
            let up = right.cross(view);
            let light = kanavos_geom::Vec3::new(0.4, -0.7, 0.8).normalized().unwrap();
            let mut tris = Vec::new();
            for e in doc.elements_on(ground) {
                if !with_roof && matches!(e.kind, ElementKind::Roof(_)) {
                    continue;
                }
                if stair_only && !matches!(e.kind, ElementKind::Stair(_)) {
                    continue;
                }
                if let Ok(sol) = geo.solids(doc, e.id) {
                    for s in sol {
                        let base = material_colour(s.material);
                        for [a, b, c] in s.mesh.triangle_points() {
                            let n = (b - a).cross(c - a).normalized().unwrap_or(kanavos_geom::Vec3::Z);
                            if n.dot(view) > 0.0 {
                                continue;
                            }
                            let k = 0.35 + 0.65 * n.dot(light).max(0.0);
                            let col = [
                                (base[0] as f64 * k) as u8,
                                (base[1] as f64 * k) as u8,
                                (base[2] as f64 * k) as u8,
                            ];
                            let pr = |p: kanavos_geom::Point3| {
                                (Point2::new(p.to_vec().dot(right), p.to_vec().dot(up)), p.to_vec().dot(view))
                            };
                            tris.push(([pr(a), pr(b), pr(c)], col));
                        }
                    }
                }
            }
            let pts: Vec<Point2> = tris.iter().flat_map(|(t, _)| t.iter().map(|(p, _)| *p)).collect();
            let b = Bbox2::from_points(pts).unwrap();
            let mut img = Canvas::new(b, 60.0);
            let mut depth = Vec::new();
            for (t, col) in &tris {
                img.triangle(&mut depth, *t, *col);
            }
            img.write_png(&dir.join(name)).unwrap();
        }
        // Close-ups of junctions.
        let spots = [
            ("se", 12.0, 0.0),
            ("arc", 12.75, 3.0),
            ("tee", 5.0, 0.0),
            ("cross", 5.0, 7.5),
            ("inner_l", 7.0, 6.0),
            ("ne", 12.0, 6.0),
        ];
        for (name, x, y) in spots {
            let b = Bbox2 { min: Point2::new(x - 0.6, y - 0.6), max: Point2::new(x + 0.6, y + 0.6) };
            let mut c = Canvas::new(b, 500.0);
            for e in doc.elements_on(ground) {
                let parts = geo.plan_parts(doc, e.id, cut).unwrap_or_default();
                for r in &parts.cut {
                    c.fill(std::slice::from_ref(&r.region), material_colour(r.material));
                }
                for r in &parts.cut {
                    c.outline(std::slice::from_ref(&r.region), [90, 90, 90]);
                }
            }
            c.outline(&all, [0, 0, 0]);
            c.write_png(&dir.join(format!("zoom_{name}.png"))).unwrap();
        }
    }
    let _ = (Length::ZERO, solids(doc, roof).unwrap());
}
