//! IFC import of hand-written IFC 2x3 and IFC 4 files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kanavos_geom::{Length, Point2, Tolerance};
use kanavos_ifc::{GLOBAL_ID_PROPERTY, IfcError, ImportBase, ImportOptions, Imported, import, import_file};
use kanavos_model::{
    Category, Document, Element, ElementKind, MaterialCategory, OpeningKind, Phase, Profile, ReferenceLine,
    RoofShape, Structure,
};

fn data(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join(name)
}

fn load(name: &str) -> Imported {
    import_file(&data(name), &ImportOptions::default()).unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn near(p: Point2, x: f64, y: f64) -> bool {
    p.coincides(Point2::new(x, y), &Tolerance::default())
}

fn only(doc: &Document, category: Category) -> &Element {
    let all: Vec<&Element> = doc.elements_of(category).collect();
    assert_eq!(all.len(), 1, "expected one {category:?}");
    all[0]
}

fn material_name(doc: &Document, s: &Structure) -> String {
    match s {
        Structure::Basic { material, .. } => doc.material(*material).unwrap().name.clone(),
        Structure::Layered { build_up } => doc.build_up(*build_up).unwrap().name.clone(),
    }
}

#[test]
fn ifc2x3_house_in_millimetres() {
    let Imported { document: doc, report } = load("house-ifc2x3.ifc");
    assert_eq!(report.schema, "IFC2X3");
    assert!(doc.validate().is_ok());
    assert!(report.warnings.iter().any(|w| w.contains("0.001")), "{:?}", report.warnings);

    let storeys = doc.storeys();
    assert_eq!(storeys.len(), 2);
    assert_eq!(storeys[0].name, "Ground floor");
    assert!(close(storeys[0].height.as_m(), 3.0));
    assert!(close(storeys[1].elevation.as_m(), 3.0));
    assert!(doc.plan_view_of(storeys[1].id).is_some());

    // The wall: axis on its right face, layers reversed into left-to-right order.
    let w = only(&doc, Category::Wall);
    let wall = w.as_wall().unwrap();
    assert!(near(wall.start, 1.0, 0.0) && near(wall.end, 1.0, 5.0), "{wall:?}");
    assert_eq!(wall.reference, ReferenceLine::Right);
    assert!(close(wall.height.as_m(), 2.8));
    assert!(close(doc.structure_thickness(&wall.structure).unwrap().as_m(), 0.25));
    let Structure::Layered { build_up } = wall.structure else { panic!("expected a build-up") };
    let b = doc.build_up(build_up).unwrap();
    assert_eq!(b.name, "Wall 250");
    let names: Vec<&str> = b.layers.iter().map(|l| doc.material(l.material).unwrap().name.as_str()).collect();
    assert_eq!(names, ["Gypsum plaster", "Brick", "Render"]);
    assert_eq!(doc.material(b.layers[1].material).unwrap().category, MaterialCategory::Masonry);
    assert_eq!(w.tag, "W1");
    assert_eq!(w.external_id.as_deref(), Some("0aWallGuid000000000001"));
    assert!(!w.properties.contains_key(GLOBAL_ID_PROPERTY), "the identifier is not a user property");

    // The door, from its opening (extruded across the wall) and its overall size.
    let d = only(&doc, Category::Door);
    let door = d.as_opening().unwrap();
    assert_eq!(door.host, w.id);
    assert_eq!(door.kind, OpeningKind::Door);
    assert!(close(door.offset.as_m(), 1.45) && close(door.width.as_m(), 0.9));
    assert!(close(door.height.as_m(), 2.1) && close(door.sill.as_m(), 0.0));
    assert_eq!(door.style, "door.single_swing");
    assert_eq!(d.tag, "D1");

    let s = only(&doc, Category::Slab);
    let ElementKind::Slab(slab) = &s.kind else { unreachable!() };
    assert!(close(slab.outline.area().as_m2(), 29.0));
    assert_eq!(slab.outline.holes.len(), 1);
    assert!(close(slab.top_offset.as_m(), 0.0));
    assert!(close(doc.structure_thickness(&slab.structure).unwrap().as_m(), 0.2));
    assert_eq!(material_name(&doc, &slab.structure), "Concrete C20/25");

    let ElementKind::Column(column) = &only(&doc, Category::Column).kind else { unreachable!() };
    assert!(near(column.position, 3.0, 2.5));
    assert!(matches!(column.profile, Profile::Circle { diameter } if close(diameter.as_m(), 0.4)));
    assert!(close(column.height.as_m(), 2.8));
    assert_eq!(doc.material(column.material).unwrap().name, "Concrete C20/25");

    let ElementKind::Beam(beam) = &only(&doc, Category::Beam).kind else { unreachable!() };
    assert!(near(beam.start, 0.0, 2.5) && near(beam.end, 6.0, 2.5), "{beam:?}");
    assert!(
        matches!(beam.profile, Profile::Rectangle { width, depth } if close(width.as_m(), 0.3) && close(depth.as_m(), 0.5))
    );
    assert!(close(beam.top_offset.as_m(), 2.8));

    let sp = only(&doc, Category::Space).as_space().unwrap();
    assert_eq!(sp.name, "Living room");
    assert_eq!(sp.number, "101");
    assert!(close(sp.boundary.area().as_m2(), 12.0));
    assert!(close(sp.height.unwrap().as_m(), 2.6));

    // Report: what was mapped, and the sofa that was not.
    for (t, n) in [
        ("IFCWALLSTANDARDCASE", 1),
        ("IFCDOOR", 1),
        ("IFCOPENINGELEMENT", 1),
        ("IFCSLAB", 1),
        ("IFCSPACE", 1),
    ] {
        assert_eq!(report.mapped.get(t), Some(&n), "{t}");
    }
    assert_eq!(report.skipped["IFCFURNISHINGELEMENT"]["no Kanavos equivalent yet"], 1);
    assert_eq!(report.skipped_total(), 1);
    assert_eq!(report.issues[0].global_id, "0aFurniture00000000001");
    assert_eq!(report.entity_counts.get("IFCCARTESIANPOINT"), Some(&20));
}

#[test]
fn ifc4_house_in_metres() {
    let Imported { document: doc, report } = load("house-ifc4.ifc");
    assert_eq!(report.schema, "IFC4");
    assert!(doc.validate().is_ok());
    assert_eq!(doc.storeys()[0].name, "Ισόγειο");
    assert_eq!(doc.info().name, "Test house IFC4");
    assert_eq!(doc.info().location, "Kalamata");

    // A wall without an axis: the reference line is the centre line of its body.
    let w = only(&doc, Category::Wall);
    let wall = w.as_wall().unwrap();
    assert!(near(wall.start, 0.0, 0.1) && near(wall.end, 4.0, 0.1), "{wall:?}");
    assert_eq!(wall.reference, ReferenceLine::Centre);
    assert!(close(doc.structure_thickness(&wall.structure).unwrap().as_m(), 0.2));
    assert_eq!(w.phase, Phase::Existing);
    let Structure::Basic { material, .. } = wall.structure else { panic!("one material") };
    let m = doc.material(material).unwrap();
    assert_eq!(m.name, "Σκυρόδεμα");
    assert_eq!(m.category, MaterialCategory::Concrete);
    assert!(report.simplifications.keys().any(|k| k.contains("without an axis")));

    let win = only(&doc, Category::Window).as_opening().unwrap();
    assert!(close(win.offset.as_m(), 2.0) && close(win.width.as_m(), 1.2));
    assert!(close(win.height.as_m(), 1.2) && close(win.sill.as_m(), 1.0));
    assert_eq!(win.style, "window.casement");

    // Mapped representation: the shaft is moved by the mapping target.
    let ElementKind::Column(column) = &only(&doc, Category::Column).kind else { unreachable!() };
    assert!(near(column.position, 6.0, 6.0), "{column:?}");
    assert!(
        matches!(column.profile, Profile::Rectangle { width, depth } if close(width.as_m(), 0.3) && close(depth.as_m(), 0.3))
    );

    // Two slabs: the L-shaped floor (composite curve) and the roof slab, whose roof has no body of its own.
    let slabs: Vec<_> = doc
        .elements_of(Category::Slab)
        .filter_map(|e| match &e.kind {
            ElementKind::Slab(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(slabs.len(), 2);
    // The L-shaped floor has 33 m², less the 1 m² shaft cut by an opening element.
    let floor = slabs.iter().find(|s| close(s.outline.area().as_m2(), 32.0)).expect("L-shaped floor");
    assert_eq!(floor.outline.holes.len(), 1);
    assert_eq!(report.mapped.get("IFCOPENINGELEMENT"), Some(&2));
    assert!(close(floor.top_offset.as_m(), 0.0));
    assert!(close(doc.structure_thickness(&floor.structure).unwrap().as_m(), 0.25));
    let roof_slab = slabs.iter().find(|s| close(s.outline.area().as_m2(), 48.0)).expect("roof slab");
    assert!(close(roof_slab.top_offset.as_m(), 3.0));
    assert_eq!(report.skipped["IFCROOF"]["roof without its own body; its parts are imported separately"], 1);
    assert_eq!(report.skipped["IFCSTAIR"]["no Kanavos equivalent yet"], 1);
    assert!(report.simplifications.contains_key("roof slab imported as a slab"));

    let g = &doc.site().georef;
    assert_eq!(g.epsg, 2100);
    assert!(close(g.easting, 431_000.0) && close(g.northing, 4_100_000.0) && close(g.altitude, 10.0));
    assert!(close(g.rotation.as_rad(), -std::f64::consts::FRAC_PI_2));
}

#[test]
fn conversion_based_units() {
    let Imported { document: doc, .. } = load("wall-inches.ifc");
    let wall = only(&doc, Category::Wall).as_wall().unwrap().clone();
    assert!(close(wall.length().as_m(), 120.0 * 0.0254));
    assert!(close(doc.structure_thickness(&wall.structure).unwrap().as_m(), 8.0 * 0.0254));
    assert!(close(wall.height.as_m(), 96.0 * 0.0254));
    assert_eq!(wall.reference, ReferenceLine::Centre);
}

#[test]
fn starter_base_keeps_materials_and_views() {
    let starter = kanavos_model::starter();
    let with_starter = load("house-ifc4.ifc").document;
    assert!(with_starter.materials().count() > starter.materials().count());
    assert!(with_starter.defaults().exterior_wall.is_some());
    assert!(with_starter.views().any(|v| matches!(v.kind, kanavos_model::ViewKind::AreaTable)));
    // The starter's own storeys are replaced by the file's.
    assert_eq!(with_starter.storeys().len(), 1);

    let empty = import_file(
        &data("house-ifc4.ifc"),
        &ImportOptions { base: ImportBase::Empty, ..Default::default() },
    )
    .unwrap()
    .document;
    assert!(empty.defaults().exterior_wall.is_none());
    assert_eq!(empty.views().count(), 1, "only the plan view of the imported storey");
    assert_eq!(empty.elements().count(), with_starter.elements().count());
}

#[test]
fn global_ids_can_be_left_out() {
    let opts = ImportOptions { keep_global_ids: false, ..Default::default() };
    let doc = import_file(&data("house-ifc2x3.ifc"), &opts).unwrap().document;
    assert!(
        doc.elements().all(|e| e.external_id.is_none() && !e.properties.contains_key(GLOBAL_ID_PROPERTY))
    );
}

#[test]
fn files_that_cannot_be_imported() {
    let other =
        b"ISO-10303-21;HEADER;FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;ENDSEC;END-ISO-10303-21;";
    assert!(
        matches!(import(other, &ImportOptions::default()), Err(IfcError::UnsupportedSchema(s)) if s == "AUTOMOTIVE_DESIGN")
    );
    let no_project =
        b"ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;#1=IFCWALL();ENDSEC;END-ISO-10303-21;";
    assert!(matches!(import(no_project, &ImportOptions::default()), Err(IfcError::NoProject)));
    match import(
        b"ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n#1=IFCX(;\n",
        &ImportOptions::default(),
    ) {
        Err(IfcError::Syntax(e)) => assert_eq!(e.line, 6),
        other => panic!("expected a syntax error, got {other:?}"),
    }
    assert!(matches!(
        import_file(&data("does-not-exist.ifc"), &ImportOptions::default()),
        Err(IfcError::Io { .. })
    ));
}

#[test]
fn files_without_storeys_get_one() {
    let text = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\n\
        #1=IFCPROJECT('3aProject0000000000001',$,'P',$,$,$,$,$,$);\nENDSEC;END-ISO-10303-21;";
    let Imported { document, report } = import(text.as_bytes(), &ImportOptions::default()).unwrap();
    assert_eq!(document.storeys().len(), 1);
    assert!(report.warnings.iter().any(|w| w.contains("no storeys")));
    assert!(close(document.storeys()[0].height.as_m(), 3.0));
    let _ = Length::ZERO;
    let _ = RoofShape::Flat { parapet: None };
}
