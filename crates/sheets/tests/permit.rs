//! The permit set: views created, sheets in order, numbering, sizes and layout bounds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, missing_docs)]

mod support;

use kanavos_drawing::{DrawingUnits, Rect};
use kanavos_model::{Document, PaperSize, ViewKind, starter};
use kanavos_sheets::{
    BINDING_MARGIN, FRAME_MARGIN, LABEL_BAND, TITLE_BLOCK_HEIGHT, TITLE_BLOCK_WIDTH, ensure_permit_views,
    permit_set, sheet_drawing, viewport_drawing, viewport_map,
};
use kanavos_units::Locale;
use support::house::house;

fn view_kinds(doc: &Document, sheet: &kanavos_model::Sheet) -> Vec<ViewKind> {
    sheet.viewports.iter().map(|vp| doc.view(vp.view).unwrap().kind.clone()).collect()
}

#[test]
fn permit_views_are_created_once() {
    let doc = house();
    let (doc, views) = ensure_permit_views(&doc, Locale::El);
    assert_eq!(views.sections.len(), 2);
    assert_eq!(views.elevations.len(), 4);
    assert_eq!(views.plans.len(), 2);
    let names: Vec<String> = views.elevations.iter().map(|v| doc.view(*v).unwrap().name.clone()).collect();
    // Grid north is 12° counter-clockwise from +Y: the façades still face the four main directions.
    for n in ["Όψη βόρεια", "Όψη ανατολική", "Όψη νότια", "Όψη δυτική"]
    {
        assert!(names.iter().any(|x| x == n), "{n} missing from {names:?}");
    }
    let sections: Vec<(String, String)> = views
        .sections
        .iter()
        .map(|v| {
            let view = doc.view(*v).unwrap();
            match &view.kind {
                ViewKind::Section { marker, .. } => (view.name.clone(), marker.clone()),
                other => panic!("{other:?}"),
            }
        })
        .collect();
    assert_eq!(sections, [("Τομή Α-Α".into(), "Α".into()), ("Τομή Β-Β".into(), "Β".into())]);
    // Section Α-Α runs along the 12 m side and Β-Β across it, both through the building.
    let a = doc.view(views.sections[0]).unwrap();
    if let ViewKind::Section { start, end, .. } = &a.kind {
        let d = *end - *start;
        assert!(d.x.abs() > d.y.abs() && start.distance(*end).as_m() > 12.0);
    }
    // Running it again creates nothing new.
    let count = doc.views().count();
    let (again, views2) = ensure_permit_views(&doc, Locale::El);
    assert_eq!(again.views().count(), count);
    assert_eq!(views2.sections, views.sections);
    assert_eq!(views2.elevations, views.elevations);
}

#[test]
fn sheets_follow_the_permit_order_with_numbers() {
    let doc = permit_set(&house(), Locale::El).unwrap();
    let sheets = doc.sheets();
    let numbers: Vec<&str> = sheets.iter().map(|s| s.number.as_str()).collect();
    assert_eq!(numbers, ["Α-01", "Α-02", "Α-03", "Α-04", "Α-05", "Α-06"]);
    let kinds: Vec<Vec<ViewKind>> = sheets.iter().map(|s| view_kinds(&doc, s)).collect();
    assert!(matches!(kinds[0][..], [ViewKind::SitePlan, ViewKind::CoverageDiagram]));
    assert!(matches!(kinds[1][..], [ViewKind::Plan { .. }]));
    assert!(matches!(kinds[2][..], [ViewKind::Plan { .. }]));
    assert!(kinds[3].iter().all(|k| matches!(k, ViewKind::Section { .. })) && kinds[3].len() == 2);
    assert!(kinds[4].iter().all(|k| matches!(k, ViewKind::Elevation { .. })) && kinds[4].len() == 4);
    assert!(matches!(kinds[5][0], ViewKind::AreaTable));
    assert_eq!(sheets[1].name, "Κάτοψη ισογείου");
    // Sheets are A-series and at most A1 for a house.
    for s in &sheets {
        assert!(
            !matches!(s.paper.size, PaperSize::A0 | PaperSize::Custom { .. }),
            "{} on {:?}",
            s.number,
            s.paper
        );
    }
    // A second run adds a new set numbered after the first.
    let again = permit_set(&doc, Locale::El).unwrap();
    assert_eq!(again.sheets().last().unwrap().number, "Α-12");
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

#[test]
fn viewports_stay_in_the_frame_and_clear_of_the_title_block() {
    let doc = permit_set(&house(), Locale::El).unwrap();
    for s in doc.sheets() {
        let (w, h) = s.paper.dimensions_mm();
        let frame = Rect {
            min: kanavos_drawing::pt(BINDING_MARGIN, FRAME_MARGIN),
            max: kanavos_drawing::pt(w - FRAME_MARGIN, h - FRAME_MARGIN),
        };
        let tb = Rect {
            min: kanavos_drawing::pt(w - FRAME_MARGIN - TITLE_BLOCK_WIDTH, FRAME_MARGIN),
            max: kanavos_drawing::pt(w - FRAME_MARGIN, FRAME_MARGIN + TITLE_BLOCK_HEIGHT),
        };
        let mut placed: Vec<Rect> = Vec::new();
        for vp in &s.viewports {
            let view = doc.view(vp.view).unwrap();
            let d = viewport_drawing(&doc, vp, Locale::El).unwrap();
            let b = d.transformed(&viewport_map(&d, vp, view)).bounds().unwrap();
            // Label band under model views.
            let b = if d.units == DrawingUnits::Metres {
                Rect { min: kanavos_drawing::pt(b.min.x, b.min.y - LABEL_BAND + 4.0), max: b.max }
            } else {
                b
            };
            assert!(
                b.min.x >= frame.min.x
                    && b.max.x <= frame.max.x
                    && b.min.y >= frame.min.y
                    && b.max.y <= frame.max.y,
                "{}: {} at {b:?} outside {frame:?}",
                s.number,
                view.name
            );
            assert!(!overlaps(&b, &tb), "{}: {} over the title block", s.number, view.name);
            for p in &placed {
                assert!(!overlaps(&b, p), "{}: {} overlaps another viewport", s.number, view.name);
            }
            placed.push(b);
        }
        // The composed sheet has its border and title block.
        let sheet = sheet_drawing(&doc, s.id, Locale::El).unwrap();
        assert_eq!(sheet.layer_items("sheet.border").count(), 1);
        assert!(sheet.layer_items("sheet.title_block").count() > 10);
    }
}

#[test]
fn an_empty_project_still_gets_a_set() {
    let doc = permit_set(&starter(), Locale::En).unwrap();
    let sheets = doc.sheets();
    // No building: coverage diagram, the two plans and the tables, no sections or elevations.
    assert!(sheets.len() >= 3);
    assert!(
        sheets.iter().all(|s| view_kinds(&doc, s).iter().all(|k| !matches!(k, ViewKind::Section { .. })))
    );
    for s in sheets {
        sheet_drawing(&doc, s.id, Locale::En).unwrap();
    }
}
