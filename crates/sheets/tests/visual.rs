//! Visual checks: rasterise the coverage diagram, the site plan, an area-table sheet and the permit sheets of a
//! sample house to PNG under `target/visual/` with `kanavos-raster`, and check basic layout facts about them.
//!
//! The PNGs are for people to look at; the assertions only guard the layout (everything on the paper).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, missing_docs)]

mod support;

use std::path::PathBuf;

use kanavos_drawing::page::PageFrame;
use kanavos_drawing::{Drawing, DrawingUnits};
use kanavos_model::{Document, Id, ViewKind};
use kanavos_raster::{RasterOptions, RasterSize, render_png};
use kanavos_sheets::{auto_layout, permit_set, render_view, sheet_drawing};
use kanavos_units::Locale;
use support::house::house;

fn save(d: &Drawing, frame: Option<PageFrame>, dpi: f64, name: &str) -> PathBuf {
    let opts =
        RasterOptions { size: RasterSize::Dpi(dpi), frame, margin_mm: 10.0, ..RasterOptions::default() };
    let png = render_png(d, &opts).unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/visual");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.png"));
    std::fs::write(&path, png).unwrap();
    println!("wrote {}", path.display());
    path
}

fn render_sheet(doc: &Document, sheet: Id, name: &str) {
    let s = doc.sheet(sheet).unwrap();
    let (w, h) = s.paper.dimensions_mm();
    let d = sheet_drawing(doc, sheet, Locale::El).unwrap();
    // Everything stays on the paper.
    let b = d.bounds().unwrap();
    assert!(b.min.x >= 0.0 && b.min.y >= 0.0 && b.max.x <= w && b.max.y <= h, "{name}: {b:?} on {w}×{h}");
    save(&d, Some(PageFrame::paper(w, h)), 110.0, name);
}

fn view_of(doc: &Document, f: impl Fn(&ViewKind) -> bool) -> Id {
    doc.views().find(|v| f(&v.kind)).unwrap().id
}

#[test]
fn coverage_diagram_png() {
    let doc = house();
    let view = view_of(&doc, |k| matches!(k, ViewKind::CoverageDiagram));
    let d = render_view(&doc, view, Locale::El).unwrap();
    assert_eq!(d.units, DrawingUnits::Metres);
    save(&d, None, 150.0, "coverage_diagram");
}

#[test]
fn site_plan_png() {
    let doc = house();
    let view = view_of(&doc, |k| matches!(k, ViewKind::SitePlan));
    let d = render_view(&doc, view, Locale::El).unwrap();
    save(&d, None, 150.0, "site_plan");
}

#[test]
fn area_table_sheet_png() {
    let doc = house();
    let table = view_of(&doc, |k| matches!(k, ViewKind::AreaTable));
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
        let file = format!("permit_{}", s.number.replace('Α', "A"));
        render_sheet(&doc, s.id, &file);
    }
}
