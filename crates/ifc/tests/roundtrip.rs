//! Export → import round trips: what Kanavos writes, Kanavos reads back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeSet;

use kanavos_ifc::step::parse;
use kanavos_ifc::{ExportOptions, GLOBAL_ID_PROPERTY, ImportOptions, export, import};
use kanavos_model::{Category, Document, Element, ElementKind, Opening, RoofShape, Wall};

fn round_trip(doc: &Document) -> (Document, kanavos_ifc::ImportReport) {
    let text = export(doc, &ExportOptions::default()).text;
    let imported = import(text.as_bytes(), &ImportOptions::default()).unwrap();
    (imported.document, imported.report)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn walls(doc: &Document) -> Vec<&Wall> {
    doc.elements().filter_map(Element::as_wall).collect()
}

fn find_wall<'a>(doc: &'a Document, w: &Wall) -> Option<&'a Element> {
    let tol = doc.tolerance();
    doc.elements().find(|e| {
        e.as_wall().is_some_and(|x| x.start.coincides(w.start, &tol) && x.end.coincides(w.end, &tol))
    })
}

fn openings(doc: &Document) -> Vec<(&Element, &Opening)> {
    doc.elements().filter_map(|e| e.as_opening().map(|o| (e, o))).collect()
}

#[test]
fn small_house_survives_export_and_import() {
    let h = common::small_house();
    let (back, report) = round_trip(&h.doc);
    assert!(back.validate().is_ok());
    assert_eq!(report.skipped_total(), 0, "{:?}", report.skipped);

    // Storeys: names, elevations, heights.
    let (a, b) = (h.doc.storeys(), back.storeys());
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.name, y.name);
        assert!(close(x.elevation.as_m(), y.elevation.as_m()) && close(x.height.as_m(), y.height.as_m()));
    }

    // Walls: count, reference lines, lengths, thicknesses, heights, build-ups.
    assert_eq!(walls(&back).len(), 4);
    for w in walls(&h.doc) {
        let e = find_wall(&back, w).expect("wall with the same reference line");
        let x = e.as_wall().unwrap();
        assert!(close(x.length().as_m(), w.length().as_m()));
        assert_eq!(x.reference, w.reference);
        assert!(close(x.height.as_m(), w.height.as_m()));
        assert!(close(
            back.structure_thickness(&x.structure).unwrap().as_m(),
            h.doc.structure_thickness(&w.structure).unwrap().as_m()
        ));
        // The import starts from the same template, so the exterior build-up is recognised, not copied.
        assert_eq!(x.structure, w.structure);
    }
    assert_eq!(back.build_ups().count(), h.doc.build_ups().count());
    assert_eq!(back.materials().count(), h.doc.materials().count());

    // Openings: kind, host, position and size.
    let before = openings(&h.doc);
    let after = openings(&back);
    assert_eq!(before.len(), after.len());
    for (e, o) in &before {
        let host = h.doc.element(o.host).unwrap().as_wall().unwrap();
        let new_host = find_wall(&back, host).unwrap().id;
        let (_, x) =
            after.iter().find(|(_, x)| x.host == new_host && x.kind == o.kind).expect("same opening");
        assert!(close(x.offset.as_m(), o.offset.as_m()), "{x:?}");
        assert!(close(x.width.as_m(), o.width.as_m()) && close(x.height.as_m(), o.height.as_m()));
        assert!(close(x.sill.as_m(), o.sill.as_m()));
        assert_eq!(
            (x.style.as_str(), x.flip_side, x.flip_hand),
            (o.style.as_str(), o.flip_side, o.flip_hand)
        );
        assert_eq!(x.params, o.params);
        let _ = e;
    }

    // Spaces: boundary area, name, number, category, finish.
    let s0 = h.doc.element(h.space).unwrap().as_space().unwrap();
    let s1 = back.elements_of(Category::Space).next().unwrap().as_space().unwrap();
    assert!(close(s0.boundary.area().as_m2(), s1.boundary.area().as_m2()));
    assert_eq!(
        (&s0.name, &s0.number, s0.category, &s0.finish),
        (&s1.name, &s1.number, s1.category, &s1.finish)
    );

    // Slab with its hole, column, flat roof with its parapet.
    let ElementKind::Slab(slab0) = &h.doc.element(h.slab).unwrap().kind else { unreachable!() };
    let ElementKind::Slab(slab1) = &back.elements_of(Category::Slab).next().unwrap().kind else {
        unreachable!()
    };
    assert!(close(slab0.outline.area().as_m2(), slab1.outline.area().as_m2()));
    assert_eq!(slab1.outline.holes.len(), 1);
    assert_eq!(slab0.structure, slab1.structure);
    let ElementKind::Column(c0) = &h.doc.element(h.column).unwrap().kind else { unreachable!() };
    let ElementKind::Column(c1) = &back.elements_of(Category::Column).next().unwrap().kind else {
        unreachable!()
    };
    assert!(c0.position.coincides(c1.position, &back.tolerance()));
    assert!(close(c0.rotation.as_rad(), c1.rotation.as_rad()));
    assert_eq!(c0.profile, c1.profile);
    let ElementKind::Roof(r0) = &h.doc.element(h.roof).unwrap().kind else { unreachable!() };
    let ElementKind::Roof(r1) = &back.elements_of(Category::Roof).next().unwrap().kind else {
        unreachable!()
    };
    assert!(close(r0.outline.area().as_m2(), r1.outline.area().as_m2()));
    assert!(close(r0.base.as_m(), r1.base.as_m()));
    assert_eq!(r0.structure, r1.structure);
    match (&r0.shape, &r1.shape) {
        (RoofShape::Flat { parapet: Some(p0) }, RoofShape::Flat { parapet: Some(p1) }) => {
            assert!(
                close(p0.height.as_m(), p1.height.as_m()) && close(p0.thickness.as_m(), p1.thickness.as_m())
            );
        }
        other => panic!("parapet lost: {other:?}"),
    }
    assert_eq!(back.info().name, h.doc.info().name);
}

#[test]
fn full_house_keeps_curved_walls_objects_and_georeferencing() {
    let h = common::full_house();
    let (back, report) = round_trip(&h.doc);
    let curved = walls(&h.doc).into_iter().find(|w| w.bulge.abs() > 0.0).unwrap();
    let e = find_wall(&back, curved).expect("curved wall");
    let x = e.as_wall().unwrap();
    assert!(close(x.bulge, curved.bulge));
    assert_eq!(x.reference, curved.reference);
    assert!(close(x.length().as_m(), curved.length().as_m()));
    let window = openings(&back).into_iter().find(|(_, o)| o.host == e.id).unwrap().1;
    assert!(close(window.offset.as_m(), 2.0), "{window:?}");
    assert_eq!(back.elements_of(Category::Object).count(), 3);
    let g0 = &h.doc.site().georef;
    let g1 = &back.site().georef;
    assert!(
        close(g0.easting, g1.easting) && close(g0.northing, g1.northing) && close(g0.altitude, g1.altitude)
    );
    assert!(close(g0.rotation.as_rad(), g1.rotation.as_rad()));
    // Tag, phase and user properties survive.
    let w0 = h.doc.element(h.walls[0]).unwrap();
    let w1 = find_wall(&back, w0.as_wall().unwrap()).unwrap();
    assert_eq!(w1.tag, w0.tag);
    assert_eq!(w1.properties.get("Σχόλιο"), w0.properties.get("Σχόλιο"));
    // What is not imported yet is reported, not lost silently.
    let skipped: BTreeSet<&str> = report.skipped.keys().map(String::as_str).collect();
    assert_eq!(skipped, ["IFCRAILING", "IFCROOF", "IFCSTAIR"].into_iter().collect());
    assert_eq!(report.skipped["IFCROOF"]["pitched roof geometry is not imported yet"], 1);
}

#[test]
fn global_ids_survive_import_and_export() {
    let h = common::small_house();
    let first = export(&h.doc, &ExportOptions::default()).text;
    let back = import(first.as_bytes(), &ImportOptions::default()).unwrap().document;
    assert!(
        back.elements().all(|e| e.external_id.is_some() && !e.properties.contains_key(GLOBAL_ID_PROPERTY))
    );
    // Each imported element's external id is the GlobalId it had in the file.
    let written: BTreeSet<String> = {
        let f = parse(first.as_bytes()).unwrap();
        f.data.keys().filter_map(|id| f.get(*id)).filter_map(|e| e.str_at(0).map(str::to_owned)).collect()
    };
    assert!(back.elements().all(|e| e.external_id.as_ref().is_some_and(|g| written.contains(g))));
    // Export the imported document under a different project namespace: the elements keep their identifiers.
    let second = export(
        &back,
        &ExportOptions { project_guid: Some(kanavos_ifc::Guid([9; 16])), ..ExportOptions::default() },
    )
    .text;
    let ids = |text: &str, t: &str| -> BTreeSet<String> {
        let f = parse(text.as_bytes()).unwrap();
        f.of_type(t).iter().map(|&id| f.get(id).unwrap().str_at(0).unwrap().to_owned()).collect()
    };
    for t in ["IFCWALL", "IFCDOOR", "IFCWINDOW", "IFCSLAB", "IFCSPACE", "IFCCOLUMN", "IFCROOF"] {
        assert_eq!(ids(&first, t), ids(&second, t), "{t}");
    }
}

#[test]
fn a_second_round_trip_changes_nothing() {
    let h = common::small_house();
    let (once, _) = round_trip(&h.doc);
    let (twice, _) = round_trip(&once);
    assert_eq!(once.element_count(), twice.element_count());
    let a = export(&once, &ExportOptions::default()).text;
    let b = export(&twice, &ExportOptions::default()).text;
    assert_eq!(parse(a.as_bytes()).unwrap().type_counts(), parse(b.as_bytes()).unwrap().type_counts());
}
