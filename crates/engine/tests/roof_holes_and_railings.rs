//! Roof holes and railings along a stair, on the sample house: built through commands, checked, and drawn as a
//! plan, two sections and two 3D hidden-line views. The drawings are rendered to PNG in
//! `<target>/tmp/feature-png/` for people to look at.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use kanavos_drawing::Drawing;
use kanavos_elements::{RoofLineKind, roof_lines, solids};
use kanavos_engine::Session;
use kanavos_engine::commands::{CreateView, RailingAlongStair, StairSide, UpdateElement};
use kanavos_engine::sample::sample_session;
use kanavos_geom::{Length, Point2, Point3, Polygon2};
use kanavos_model::{Camera, Category, ElementKind, Id, MaterialCategory, RailingInfill, ViewKind};
use kanavos_units::Locale;
use kanavos_views::{PlanOptions, SceneOptions, ThreeDOptions, camera_drawing, plan, scene, view_drawing};

fn save_png(d: &Drawing, name: &str) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("feature-png");
    std::fs::create_dir_all(&dir).unwrap();
    let opts = kanavos_raster::RasterOptions {
        size: kanavos_raster::RasterSize::Dpi(150.0),
        margin_mm: 12.0,
        background: Some(kanavos_drawing::Rgba::WHITE),
        ..kanavos_raster::RasterOptions::default()
    };
    std::fs::write(dir.join(name), kanavos_raster::render_png(d, &opts).unwrap()).unwrap();
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon2 {
    Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

fn section(s: &mut Session, name: &str, start: (f64, f64), end: (f64, f64), depth: f64) -> Id {
    let kind = ViewKind::Section {
        start: Point2::new(start.0, start.1),
        end: Point2::new(end.0, end.1),
        depth: Length::m(depth),
        flip: false,
        marker: name.into(),
    };
    s.run(CreateView { name: format!("Τομή {name}-{name}"), kind, scale: 50 }).unwrap().created[0]
}

/// Line items of a drawing on a layer, as segments.
fn lines(d: &Drawing, layer: &str) -> Vec<(Point2, Point2)> {
    d.items
        .iter()
        .filter(|i| i.layer == layer)
        .filter_map(|i| match &i.shape {
            kanavos_drawing::Shape::Line { a, b } => Some((Point2::new(a.x, a.y), Point2::new(b.x, b.y))),
            _ => None,
        })
        .collect()
}

#[test]
fn roof_holes_and_stair_railings_on_the_sample_house() {
    let mut s = sample_session().unwrap();
    let doc = s.document().clone();

    // Two holes in the hip roof: a skylight in the south plane and a hole across the ridge.
    let roof = doc.elements().find(|e| e.category() == Category::Roof).unwrap().clone();
    let ElementKind::Roof(mut r) = roof.kind.clone() else { panic!() };
    let b = r.outline.bbox().unwrap();
    let (x0, y0) = (b.min.x.as_m(), b.min.y.as_m());
    let c = b.centre();
    let (cx, cy) = (c.x.as_m(), c.y.as_m());
    let skylight = rect(x0 + 2.4, y0 + 1.8, x0 + 3.6, y0 + 2.8);
    let ridge = rect(cx - 0.5, cy - 0.5, cx + 0.5, cy + 0.5);
    r.holes = vec![skylight.clone(), ridge];
    s.run(UpdateElement { id: roof.id, kind: Some(ElementKind::Roof(r)), ..UpdateElement::default() })
        .unwrap();

    // Railings on both sides of the stair's flights: glass on the inner side, bars along the walls.
    let stair = doc.elements().find(|e| e.category() == Category::Stair).unwrap().id;
    let glass = doc.materials().find(|m| m.category == MaterialCategory::Glass).unwrap().id;
    let inner = RailingAlongStair {
        stair,
        side: StairSide::Right,
        height: Length::m(1.0),
        inset: Length::cm(5.0),
        infill: RailingInfill::Glass,
        material: None,
        infill_material: Some(glass),
        post_spacing: Length::m(1.2),
        tag: String::new(),
    };
    let created = s.run(inner.clone()).unwrap().created;
    assert_eq!(created.len(), 2, "one railing per flight");
    s.run(RailingAlongStair {
        side: StairSide::Left,
        infill: RailingInfill::Bars,
        infill_material: None,
        ..inner
    })
    .unwrap();

    let skylight_x = (x0 + 3.0, y0 - 2.0, y0 + 13.0);
    let sec_roof = section(&mut s, "Γ", (skylight_x.0, skylight_x.2), (skylight_x.0, skylight_x.1), 15.0);
    let sec_stair = section(&mut s, "Δ", (8.75, 12.0), (8.75, 5.0), 3.0);
    let doc = s.document().clone();

    // Geometry: the roof's layers stay closed, and the holes are outlined on its top surface.
    for solid in solids(&doc, roof.id).unwrap() {
        assert!(solid.mesh.is_closed(1e-6), "{:?}", solid.part);
    }
    let roof_edges = roof_lines(&doc, roof.id).unwrap();
    assert!(roof_edges.iter().filter(|l| l.kind == RoofLineKind::Opening).count() >= 8);
    for id in &created {
        let ElementKind::Railing(rl) = &doc.element(*id).unwrap().kind else { panic!() };
        assert!(rl.is_sloped());
        assert!(kanavos_elements::railing_checks(&doc, *id).unwrap().is_empty());
    }

    // Plan of the first floor: the roof above the cut, dashed, with the skylight's edges.
    let first = doc.storeys()[1].id;
    let d = plan(&doc, first, &PlanOptions::default()).unwrap();
    save_png(&d, "first-floor-plan.png");
    let on_skylight = |p: Point2| {
        let tol = kanavos_geom::Tolerance { length: Length::m(1e-3), ..Default::default() };
        skylight.edges().any(|e| tol.is_zero(e.distance_to(p)))
    };
    let roof_above = lines(&d, "roof.above");
    assert!(
        roof_above.iter().filter(|(a, b)| on_skylight(*a) && on_skylight(*b)).count() >= 4,
        "the skylight shows in the plan"
    );
    // Plan of the ground floor: the stair and its railings.
    let ground = doc.storeys()[0].id;
    let d = plan(&doc, ground, &PlanOptions::default()).unwrap();
    save_png(&d, "ground-floor-plan.png");
    // The stair and its railings alone, larger.
    let railings: BTreeSet<u64> = doc
        .elements()
        .filter(|e| {
            matches!(e.kind, ElementKind::Railing(_) | ElementKind::Stair(_)) && e.storey == Some(ground)
        })
        .map(|e| e.id.0)
        .collect();
    let mut only = d.clone();
    only.items.retain(|i| i.source.is_some_and(|s| railings.contains(&s)));
    assert!(only.items.iter().any(|i| i.layer.starts_with("railing.")), "railings in the plan");
    save_png(&only, "ground-floor-stair.png");

    // Sections through the skylight and along the stair.
    let d = view_drawing(&doc, sec_roof, Locale::El).unwrap();
    assert!(!d.items.is_empty());
    save_png(&d, "section-through-skylight.png");
    let d = view_drawing(&doc, sec_stair, Locale::El).unwrap();
    assert!(
        d.items.iter().any(|i| i.source.is_some_and(|s| created.contains(&Id(s)))),
        "railings in the section"
    );
    save_png(&d, "section-along-stair.png");

    // 3D: the whole house from the south-west, and the stair with its railings alone.
    let opts = ThreeDOptions::default();
    let all = scene(&doc, &SceneOptions::default());
    let house = Camera {
        eye: Point3::new(-14.0, -16.0, 18.0),
        target: Point3::new(6.0, 5.0, 4.0),
        fov: 22.0,
        parallel: true,
    };
    save_png(&camera_drawing(&doc, &all, &house, 100.0, &opts), "house-3d.png");
    let hidden: BTreeSet<Category> =
        Category::ALL.into_iter().filter(|c| !matches!(c, Category::Stair | Category::Railing)).collect();
    let stairs = scene(&doc, &SceneOptions { storeys: Some([ground].into_iter().collect()), hidden });
    let view = Camera {
        eye: Point3::new(3.5, 2.0, 6.5),
        target: Point3::new(8.75, 7.7, 1.6),
        fov: 5.5,
        parallel: true,
    };
    let d = camera_drawing(&doc, &stairs, &view, 25.0, &opts);
    assert!(!d.items.is_empty());
    save_png(&d, "stair-railings-3d.png");

    // IFC: one opening per hole voids the roof.
    let ifc = String::from_utf8(kanavos_engine::export::ifc(&doc).unwrap()).unwrap();
    assert_eq!(ifc.matches("IFCOPENINGELEMENT(").count() - ifc_openings_without_roof(&doc), 2);
}

/// Openings the house had before its roof got holes (doors, windows and voids in walls).
fn ifc_openings_without_roof(doc: &kanavos_model::Document) -> usize {
    doc.elements().filter(|e| matches!(e.kind, ElementKind::Opening(_))).count()
}

#[test]
fn railing_infill_colours_follow_their_material() {
    let mut s = sample_session().unwrap();
    let doc = s.document().clone();
    let stair = doc.elements().find(|e| e.category() == Category::Stair).unwrap().id;
    let glass = doc.materials().find(|m| m.category == MaterialCategory::Glass).unwrap().clone();
    let created = s
        .run(RailingAlongStair {
            stair,
            side: StairSide::Right,
            height: Length::m(1.0),
            inset: Length::cm(5.0),
            infill: RailingInfill::Glass,
            material: None,
            infill_material: Some(glass.id),
            post_spacing: Length::m(1.2),
            tag: String::new(),
        })
        .unwrap()
        .created;
    let doc = s.document().clone();
    let items = kanavos_views::element_scene_items(&doc, created[0]);
    assert!(items.iter().any(|i| i.material == Some(glass.id) && i.color == glass.color));
    // Changing the infill material's colour refreshes the cached meshes of the railing.
    let mut cache = kanavos_views::SceneCache::new();
    let opts = SceneOptions::default();
    let _ = cache.scene(&doc, &opts);
    let tinted = doc.with_material(kanavos_model::Material {
        color: kanavos_drawing::Rgba::new(40, 160, 90, 120),
        ..glass.clone()
    });
    assert_eq!(cache.scene(&tinted, &opts), scene(&tinted, &opts));
}
