//! Visual checks: the fixture house's plans, section, elevation and 3D views are rendered to PNG files under
//! `target/views-png/` for people to look at, and checked for the parts every drawing must have.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::time::Instant;

use kanavos_drawing::{Drawing, Shape};
use kanavos_units::Locale;
use kanavos_views::{PlanOptions, plan, view_drawing};

fn layers(d: &Drawing, prefix: &str) -> usize {
    d.layer_items(prefix).count()
}

fn texts(d: &Drawing) -> Vec<String> {
    d.items
        .iter()
        .filter_map(|i| match &i.shape {
            Shape::Text(t) => Some(t.content.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn house_views_render() {
    let h = common::house();
    let r = common::raster::Raster::new();
    let out = common::out_dir();

    let t = Instant::now();
    let ground =
        plan(&h.doc, h.ground, &PlanOptions { space_perimeter: true, ..PlanOptions::default() }).unwrap();
    eprintln!("ground plan: {} items in {:?}", ground.items.len(), t.elapsed());
    r.render(&ground, 6.0, &out.join("plan-ground-50.png"));
    assert!(layers(&ground, "wall.cut") > 0);
    assert!(layers(&ground, "column.cut") > 0);
    assert!(layers(&ground, "window.above") > 0, "the high kitchen window is dashed");
    assert!(layers(&ground, "stair.break") > 0);
    assert!(layers(&ground, "slab.above") > 0, "the balcony above is dashed");
    let tx = texts(&ground);
    assert!(tx.iter().any(|s| s == "ΑΝ"));
    assert!(tx.iter().any(|s| s == "18×17,8/28"));
    assert!(tx.iter().any(|s| s.contains("m²")));
    assert!(tx.iter().any(|s| s == "±0,00"));
    assert!(tx.iter().any(|s| s == "Β"), "north arrow");

    let first = plan(&h.doc, h.first, &PlanOptions::default()).unwrap();
    r.render(&first, 6.0, &out.join("plan-first-50.png"));
    assert!(texts(&first).iter().any(|s| s == "ΚΑΤ"));
    assert!(layers(&first, "roof.above") > 0, "the roof and parapet are dashed");

    let small =
        plan(&h.doc, h.ground, &PlanOptions { scale: 100.0, locale: Locale::En, ..PlanOptions::default() })
            .unwrap();
    r.render(&small, 8.0, &out.join("plan-ground-100.png"));
    assert!(texts(&small).iter().any(|s| s == "UP"));

    let t = Instant::now();
    let section = view_drawing(&h.doc, h.section, Locale::El).unwrap();
    eprintln!("section: {} items in {:?}", section.items.len(), t.elapsed());
    r.render(&section, 6.0, &out.join("section-a.png"));
    assert!(layers(&section, "wall.cut") > 0);
    assert!(layers(&section, "slab.cut") > 0);
    assert!(layers(&section, "stair.cut") > 0);
    assert!(layers(&section, "wall.beyond") > 0);
    assert!(layers(&section, "ground.line") > 0);
    assert!(texts(&section).iter().any(|s| s == "+3,20"));

    let t = Instant::now();
    let elevation = view_drawing(&h.doc, h.elevation, Locale::El).unwrap();
    eprintln!("elevation: {} items in {:?}", elevation.items.len(), t.elapsed());
    r.render(&elevation, 6.0, &out.join("elevation-south.png"));
    assert!(layers(&elevation, "window.beyond") > 0, "window frames show in the elevation");
    assert_eq!(layers(&elevation, "wall.cut"), 0);

    let t = Instant::now();
    let axo = view_drawing(&h.doc, h.axo, Locale::El).unwrap();
    eprintln!("axonometric: {} items in {:?}", axo.items.len(), t.elapsed());
    r.render(&axo, 6.0, &out.join("axonometric.png"));
    assert!(layers(&axo, "wall.edge") > 0);

    let persp = view_drawing(&h.doc, h.perspective, Locale::El).unwrap();
    r.render(&persp, 6.0, &out.join("perspective.png"));
    assert!(layers(&persp, "wall.edge") > 0);
}
