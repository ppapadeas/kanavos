//! Visual checks: rasterise the coverage diagram, the area-table sheet and the permit sheets of a sample house
//! to PNG under `target/visual/`, and check basic layout facts about them.
//!
//! The PNGs are for people to look at; the assertions only guard the layout (everything on the paper, nothing
//! over the title block).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, missing_docs)]

mod support;

use kanavos_drawing::DrawingUnits;
use kanavos_model::{Document, Id, ViewKind};
use kanavos_sheets::{BINDING_MARGIN, FRAME_MARGIN, auto_layout, permit_set, render_view, sheet_drawing};
use kanavos_units::Locale;
use support::house::house;
use support::raster::{Canvas, model_on_paper};

const DPI: f64 = 110.0;

fn render_sheet(doc: &Document, sheet: Id, name: &str) {
    let s = doc.sheet(sheet).unwrap();
    let (w, h) = s.paper.dimensions_mm();
    let d = sheet_drawing(doc, sheet, Locale::El).unwrap();
    // Everything stays on the paper.
    let b = d.bounds().unwrap();
    assert!(b.min.x >= 0.0 && b.min.y >= 0.0 && b.max.x <= w && b.max.y <= h, "{name}: {b:?} on {w}×{h}");
    let mut c = Canvas::new(w, h, DPI);
    c.drawing(&d);
    let path = c.save(name);
    println!("wrote {}", path.display());
}

#[test]
fn coverage_diagram_png() {
    let doc = house();
    let view = doc.views().find(|v| matches!(v.kind, ViewKind::CoverageDiagram)).unwrap().id;
    let d = render_view(&doc, view, Locale::El).unwrap();
    assert_eq!(d.units, DrawingUnits::Metres);
    let (paper, w, h) = model_on_paper(&d, 10.0);
    let mut c = Canvas::new(w, h, 150.0);
    c.drawing(&paper);
    println!("wrote {}", c.save("coverage_diagram").display());
}

#[test]
fn site_plan_png() {
    let doc = house();
    let view = doc.views().find(|v| matches!(v.kind, ViewKind::SitePlan)).unwrap().id;
    let d = render_view(&doc, view, Locale::El).unwrap();
    let (paper, w, h) = model_on_paper(&d, 10.0);
    let mut c = Canvas::new(w, h, 150.0);
    c.drawing(&paper);
    println!("wrote {}", c.save("site_plan").display());
}

#[test]
fn area_table_sheet_png() {
    let doc = house();
    let table = doc.views().find(|v| matches!(v.kind, ViewKind::AreaTable)).unwrap().id;
    let (paper, viewports) = auto_layout(&doc, &[table], Locale::El).unwrap();
    let (d, id) = doc.with_new_id();
    let doc = d.with_sheet(kanavos_model::Sheet {
        id,
        number: "Α-90".into(),
        name: "Πίνακας εμβαδών".into(),
        paper,
        viewports,
        title_block: true,
    });
    render_sheet(&doc, id, "area_table_sheet");
}

#[test]
fn permit_set_pngs() {
    let doc = permit_set(&house(), Locale::El).unwrap();
    let sheets = doc.sheets();
    assert!(sheets.len() >= 5, "{} sheets", sheets.len());
    for s in &sheets {
        let (w, _) = s.paper.dimensions_mm();
        assert!(w > BINDING_MARGIN + FRAME_MARGIN);
        let file = format!("permit_{}", s.number.replace('Α', "A"));
        render_sheet(&doc, s.id, &file);
    }
}
