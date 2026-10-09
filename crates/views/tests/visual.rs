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

    let t = Instant::now();
    let ground =
        plan(&h.doc, h.ground, &PlanOptions { space_perimeter: true, ..PlanOptions::default() }).unwrap();
    eprintln!("ground plan: {} items in {:?}", ground.items.len(), t.elapsed());
    common::save_png(&ground, "plan-ground-50.png");
    assert!(layers(&ground, "wall.cut") > 0);
    assert!(layers(&ground, "column.cut") > 0);
    assert!(layers(&ground, "window.symbol.above") > 0, "the high kitchen window is dashed");
    assert!(layers(&ground, "stair.break") > 0);
    assert!(layers(&ground, "slab.above") > 0, "the balcony above is dashed");
    assert!(layers(&ground, "beam.above") > 0, "the beam above the cut is dashed");
    let tx = texts(&ground);
    assert!(tx.iter().any(|s| s == "ΑΝ"));
    assert!(tx.iter().any(|s| s == "18×17,8/28"));
    assert!(tx.iter().any(|s| s.contains("m²")));
    assert!(tx.iter().any(|s| s == "±0,00"));
    assert!(tx.iter().any(|s| s == "Β"), "north arrow");

    let first = plan(&h.doc, h.first, &PlanOptions::default()).unwrap();
    common::save_png(&first, "plan-first-50.png");
    assert!(texts(&first).iter().any(|s| s == "ΚΑΤ"));
    assert!(layers(&first, "roof.above") > 0, "the roof and parapet are dashed");

    let small =
        plan(&h.doc, h.ground, &PlanOptions { scale: 100.0, locale: Locale::En, ..PlanOptions::default() })
            .unwrap();
    common::save_png(&small, "plan-ground-100.png");
    assert!(texts(&small).iter().any(|s| s == "UP"));

    let t = Instant::now();
    let section = view_drawing(&h.doc, h.section, Locale::El).unwrap();
    eprintln!("section: {} items in {:?}", section.items.len(), t.elapsed());
    common::save_png(&section, "section-a.png");
    assert!(layers(&section, "wall.cut") > 0);
    assert!(layers(&section, "slab.cut") > 0);
    assert!(layers(&section, "stair.cut") > 0);
    assert!(layers(&section, "wall.beyond") > 0);
    assert!(layers(&section, "ground.line") > 0);
    assert!(texts(&section).iter().any(|s| s == "+3,20"));

    let t = Instant::now();
    let elevation = view_drawing(&h.doc, h.elevation, Locale::El).unwrap();
    eprintln!("elevation: {} items in {:?}", elevation.items.len(), t.elapsed());
    common::save_png(&elevation, "elevation-south.png");
    assert!(layers(&elevation, "window.beyond") > 0, "window frames show in the elevation");
    assert_eq!(layers(&elevation, "wall.cut"), 0);

    let t = Instant::now();
    let axo = view_drawing(&h.doc, h.axo, Locale::El).unwrap();
    eprintln!("axonometric: {} items in {:?}", axo.items.len(), t.elapsed());
    common::save_png(&axo, "axonometric.png");
    assert!(layers(&axo, "wall.edge") > 0);

    let persp = view_drawing(&h.doc, h.perspective, Locale::El).unwrap();
    common::save_png(&persp, "perspective.png");
    assert!(layers(&persp, "wall.edge") > 0);
}

#[test]
fn grid_lines_and_notes_render() {
    use kanavos_geom::{Angle, Point2};
    use kanavos_model::{Element, ElementKind, GridLine, TextAlign, TextNote};
    let h = common::house();
    let mut doc = h.doc.clone();
    let grids = [
        ("Α", (0.0, -2.5), (0.0, 11.5)),
        ("Β", (6.0, -2.5), (6.0, 11.5)),
        ("Γ", (12.0, -2.5), (12.0, 11.5)),
        ("1", (-2.5, 0.0), (14.5, 0.0)),
        ("2", (-2.5, 9.0), (14.5, 9.0)),
    ];
    for (label, a, b) in grids {
        let (d, id) = doc.with_new_id();
        let mut e = Element::on_storey(
            id,
            h.ground,
            ElementKind::Grid(GridLine {
                label: label.into(),
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
            }),
        );
        e.storey = None;
        doc = d.with_element(e);
    }
    let (d, id) = doc.with_new_id();
    doc = d.with_element(Element::on_storey(
        id,
        h.ground,
        ElementKind::Text(TextNote {
            position: Point2::new(1.0, -4.6),
            content: "Σημείωση: οι διαστάσεις σε μέτρα\nΤα υλικά κατά τη μελέτη".into(),
            size_mm: 2.5,
            rotation: Angle::ZERO,
            align: TextAlign::Left,
            bold: false,
        }),
    ));
    let d = plan(&doc, h.ground, &PlanOptions { scale: 100.0, ..PlanOptions::default() }).unwrap();
    common::save_png(&d, "plan-grid-100.png");
    assert_eq!(layers(&d, "grid.bubble"), 10);
    assert!(texts(&d).iter().any(|t| t == "Τα υλικά κατά τη μελέτη"));
}
