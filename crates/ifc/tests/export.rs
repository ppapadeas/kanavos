//! IFC 4.3 export: structure, identifiers, quantities and determinism, checked by reading the output back with
//! the crate's own Part 21 reader.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::collections::{BTreeMap, HashSet};

use kanavos_geom::Point3;
use kanavos_ifc::step::{Entity, StepFile, Value, parse};
use kanavos_ifc::{ExportOptions, GLOBAL_ID_PROPERTY, Guid, export};
use kanavos_model::{Document, Id};
use kanavos_solid::cuboid;

fn options() -> ExportOptions<'static> {
    ExportOptions {
        timestamp: "2026-10-09T12:00:00".into(),
        author: "Test".into(),
        organization: "Kanavos".into(),
        file_name: "house.ifc".into(),
        ..ExportOptions::default()
    }
}

fn exported(doc: &Document) -> (String, StepFile) {
    let out = export(doc, &options());
    let file = parse(out.text.as_bytes()).expect("the export parses as ISO 10303-21");
    (out.text, file)
}

fn count(f: &StepFile, t: &str) -> usize {
    f.of_type(t).len()
}

fn entities<'a>(f: &'a StepFile, t: &str) -> Vec<&'a Entity> {
    f.of_type(t).iter().filter_map(|&id| f.get(id)).collect()
}

/// `Qto_*` values of a product: quantity name → value.
fn quantities(f: &StepFile, product: u64) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for rel in entities(f, "IFCRELDEFINESBYPROPERTIES") {
        if !rel.refs_at(4).contains(&product) {
            continue;
        }
        let Some(set) = rel.ref_at(5).and_then(|s| f.get(s)) else { continue };
        if set.name.as_ref() != "IFCELEMENTQUANTITY" {
            continue;
        }
        for q in set.refs_at(5) {
            let q = f.get(q).unwrap();
            out.insert(q.str_at(0).unwrap().to_owned(), q.f64_at(3).unwrap());
        }
    }
    out
}

/// Single-value properties of a product in a named set.
fn properties<'a>(f: &'a StepFile, product: u64, set_name: &str) -> BTreeMap<String, &'a Value> {
    let mut out = BTreeMap::new();
    for rel in entities(f, "IFCRELDEFINESBYPROPERTIES") {
        if !rel.refs_at(4).contains(&product) {
            continue;
        }
        let set = f.get(rel.ref_at(5).unwrap()).unwrap();
        if set.str_at(2) != Some(set_name) {
            continue;
        }
        for p in set.refs_at(4) {
            let p = f.get(p).unwrap();
            let v = match p.name.as_ref() {
                "IFCPROPERTYSINGLEVALUE" => p.attr(2).unwrap(),
                _ => &p.list_at(2).unwrap()[0],
            };
            out.insert(p.str_at(0).unwrap().to_owned(), v);
        }
    }
    out
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn header_names_the_schema_and_file() {
    let h = common::small_house();
    let (text, f) = exported(&h.doc);
    assert!(text.starts_with("ISO-10303-21;\nHEADER;\n"));
    assert!(text.ends_with("END-ISO-10303-21;\n"));
    assert!(text.is_ascii(), "Part 21 output must be 7-bit");
    assert_eq!(f.schemas(), vec!["IFC4X3_ADD2".to_owned()]);
    let name = f.header_entity("FILE_NAME").unwrap();
    assert_eq!(name.str_at(0), Some("house.ifc"));
    assert_eq!(name.str_at(1), Some("2026-10-09T12:00:00"));
    let description = f.header_entity("FILE_DESCRIPTION").unwrap();
    assert!(description.list_at(0).unwrap()[0].as_str().unwrap().starts_with("ViewDefinition ["));
}

#[test]
fn small_house_has_the_expected_entities() {
    let h = common::small_house();
    let out = export(&h.doc, &options());
    let f = parse(out.text.as_bytes()).unwrap();
    let expected = [
        ("IFCPROJECT", 1),
        ("IFCSITE", 1),
        ("IFCBUILDING", 1),
        ("IFCBUILDINGSTOREY", 2),
        ("IFCWALL", 4),
        ("IFCOPENINGELEMENT", 2),
        ("IFCDOOR", 1),
        ("IFCWINDOW", 1),
        ("IFCRELVOIDSELEMENT", 2),
        ("IFCRELFILLSELEMENT", 2),
        ("IFCSLAB", 1),
        ("IFCSPACE", 1),
        ("IFCCOLUMN", 1),
        ("IFCROOF", 1),
        ("IFCUNITASSIGNMENT", 1),
        ("IFCGEOMETRICREPRESENTATIONCONTEXT", 2),
        ("IFCGEOMETRICREPRESENTATIONSUBCONTEXT", 3),
    ];
    for (t, n) in expected {
        assert_eq!(count(&f, t), n, "{t}");
    }
    // No georeferencing unless the site has it.
    assert_eq!(count(&f, "IFCMAPCONVERSION"), 0);
    assert_eq!(out.report.products.get("IfcWall"), Some(&4));
    assert!(out.report.warnings.is_empty(), "{:?}", out.report.warnings);
    assert_eq!(out.report.instances as usize, f.data.len());
    // Every reference resolves.
    assert!(f.dangling_references().is_empty(), "{:?}", f.dangling_references());
}

#[test]
fn units_and_contexts() {
    let (_, f) = exported(&common::small_house().doc);
    let units: Vec<(String, String)> = entities(&f, "IFCSIUNIT")
        .iter()
        .map(|u| (u.enum_at(1).unwrap().to_owned(), u.enum_at(3).unwrap().to_owned()))
        .collect();
    for pair in [
        ("LENGTHUNIT", "METRE"),
        ("AREAUNIT", "SQUARE_METRE"),
        ("VOLUMEUNIT", "CUBIC_METRE"),
        ("PLANEANGLEUNIT", "RADIAN"),
    ] {
        assert!(units.contains(&(pair.0.to_owned(), pair.1.to_owned())), "{pair:?}");
    }
    let subs: HashSet<String> = entities(&f, "IFCGEOMETRICREPRESENTATIONSUBCONTEXT")
        .iter()
        .map(|c| c.str_at(0).unwrap().to_owned())
        .collect();
    assert_eq!(subs, ["Body", "Axis", "FootPrint"].iter().map(|s| s.to_string()).collect());
    let types: HashSet<&str> =
        entities(&f, "IFCGEOMETRICREPRESENTATIONCONTEXT").iter().map(|c| c.str_at(1).unwrap()).collect();
    assert_eq!(types, ["Model", "Plan"].into_iter().collect());
}

#[test]
fn global_ids_are_valid_and_unique() {
    let (_, f) = exported(&common::full_house().doc);
    let mut seen = HashSet::new();
    let mut rooted = 0;
    for record in f.data.values() {
        let Some(e) = record.simple() else { continue };
        let Some(g) = e.str_at(0) else { continue };
        let rooted_type = e.name.starts_with("IFCREL")
            || e.name.starts_with("IFCPROPERTYSET")
            || e.name.as_ref() == "IFCELEMENTQUANTITY"
            || e.ref_at(5).and_then(|p| f.type_of(p)) == Some("IFCLOCALPLACEMENT")
            || e.name.as_ref() == "IFCPROJECT";
        if !rooted_type {
            continue;
        }
        rooted += 1;
        assert!(Guid::from_ifc(g).is_some(), "{} has an invalid GlobalId {g}", e.name);
        assert!(seen.insert(g.to_owned()), "duplicate GlobalId {g} on {}", e.name);
    }
    assert!(rooted > 100, "only {rooted} rooted entities checked");
}

#[test]
fn spatial_structure_and_containment() {
    let h = common::small_house();
    let (_, f) = exported(&h.doc);
    let project = f.of_type("IFCPROJECT")[0];
    let site = f.of_type("IFCSITE")[0];
    let building = f.of_type("IFCBUILDING")[0];
    let storeys = f.of_type("IFCBUILDINGSTOREY").to_vec();
    let aggregates: Vec<(u64, Vec<u64>)> =
        entities(&f, "IFCRELAGGREGATES").iter().map(|r| (r.ref_at(4).unwrap(), r.refs_at(5))).collect();
    assert!(aggregates.contains(&(project, vec![site])));
    assert!(aggregates.contains(&(site, vec![building])));
    assert!(aggregates.contains(&(building, storeys.clone())));
    // Spaces are aggregated by their storey; building elements contained in it; openings in neither.
    let ground = storeys[0];
    assert!(aggregates.contains(&(ground, f.of_type("IFCSPACE").to_vec())));
    let contained: BTreeMap<u64, Vec<u64>> = entities(&f, "IFCRELCONTAINEDINSPATIALSTRUCTURE")
        .iter()
        .map(|r| (r.ref_at(5).unwrap(), r.refs_at(4)))
        .collect();
    let on_ground = &contained[&ground];
    for t in ["IFCWALL", "IFCDOOR", "IFCWINDOW", "IFCSLAB", "IFCCOLUMN"] {
        for id in f.of_type(t) {
            assert!(on_ground.contains(id), "{t} #{id} not contained in the ground storey");
        }
    }
    assert_eq!(contained[&storeys[1]], f.of_type("IFCROOF").to_vec());
    for id in f.of_type("IFCOPENINGELEMENT") {
        assert!(contained.values().all(|v| !v.contains(id)));
    }
    // Storey elevations.
    let elevations: Vec<f64> = storeys.iter().map(|&s| f.get(s).unwrap().f64_at(9).unwrap()).collect();
    assert!(close(elevations[0], 0.0) && close(elevations[1], 3.2));
}

#[test]
fn openings_void_walls_and_are_filled() {
    let h = common::small_house();
    let (_, f) = exported(&h.doc);
    let walls = f.of_type("IFCWALL");
    for v in entities(&f, "IFCRELVOIDSELEMENT") {
        assert!(walls.contains(&v.ref_at(4).unwrap()));
        assert_eq!(f.type_of(v.ref_at(5).unwrap()), Some("IFCOPENINGELEMENT"));
    }
    let door = f.get(f.of_type("IFCDOOR")[0]).unwrap();
    assert!(close(door.f64_at(8).unwrap(), 2.2) && close(door.f64_at(9).unwrap(), 1.0));
    assert_eq!(door.str_at(4), Some("door.single_swing"));
    assert_eq!(door.enum_at(10), Some("DOOR"));
    let window = f.get(f.of_type("IFCWINDOW")[0]).unwrap();
    assert!(close(window.f64_at(8).unwrap(), 1.4) && close(window.f64_at(9).unwrap(), 1.5));
    // The door is placed relative to its opening, the opening relative to its wall.
    let fills = entities(&f, "IFCRELFILLSELEMENT");
    let fill = fills.iter().find(|r| r.ref_at(5) == Some(f.of_type("IFCDOOR")[0])).unwrap();
    let opening = f.get(fill.ref_at(4).unwrap()).unwrap();
    let door_placement = f.get(door.ref_at(5).unwrap()).unwrap();
    assert_eq!(door_placement.ref_at(0), opening.ref_at(5));
    let kanavos = properties(&f, f.of_type("IFCDOOR")[0], "Kanavos_Opening");
    assert_eq!(kanavos["Style"].as_str(), Some("door.single_swing"));
    assert_eq!(kanavos["FlipHand"].as_bool(), Some(true));
    assert!(close(kanavos["frame"].as_f64().unwrap(), 0.06));
}

#[test]
fn quantities_follow_the_model() {
    let h = common::small_house();
    let (_, f) = exported(&h.doc);
    let south = f.of_type("IFCWALL")[0];
    let q = quantities(&f, south);
    assert!(close(q["Length"], 10.0));
    assert!(close(q["Width"], 0.31));
    assert!(close(q["Height"], 3.2));
    assert!(close(q["GrossSideArea"], 32.0));
    assert!(close(q["NetSideArea"], 32.0 - 1.0 * 2.2), "{}", q["NetSideArea"]);
    // Mitred corners: the footprint runs from outer corner to outer corner on the outside face.
    assert!(close(q["GrossFootprintArea"], 0.31 * 10.0));
    let east = quantities(&f, f.of_type("IFCWALL")[1]);
    assert!(close(east["NetSideArea"], 8.0 * 3.2 - 1.5 * 1.4));

    let space = quantities(&f, f.of_type("IFCSPACE")[0]);
    assert!(close(space["NetFloorArea"], 9.69 * 7.69));
    assert!(close(space["Height"], 3.2));
    let slab = quantities(&f, f.of_type("IFCSLAB")[0]);
    assert!(close(slab["GrossArea"], 10.31 * 8.31));
    assert!(close(slab["NetArea"], 10.31 * 8.31 - 2.0));
    let thickness = h
        .doc
        .structure_thickness(
            &h.doc.defaults().ground_slab.map(|b| kanavos_model::Structure::Layered { build_up: b }).unwrap(),
        )
        .unwrap()
        .as_m();
    assert!(close(slab["Width"], thickness));
    let column = quantities(&f, f.of_type("IFCCOLUMN")[0]);
    assert!(close(column["CrossSectionArea"], 0.15));
    assert!(close(column["GrossVolume"], 0.15 * 3.2));
}

#[test]
fn property_sets_carry_kanavos_data() {
    let h = common::full_house();
    let (_, f) = exported(&h.doc);
    let space = f.of_type("IFCSPACE")[0];
    let e = f.get(space).unwrap();
    assert_eq!(e.str_at(2), Some("0.01"));
    assert_eq!(e.str_at(7), Some("Καθιστικό"));
    let own = properties(&f, space, "Kanavos_Space");
    assert_eq!(own["Category"].as_str(), Some("main_use"));
    assert_eq!(own["Finish"].as_str(), Some("Πλακίδια"));
    let common = properties(&f, space, "Pset_SpaceCommon");
    assert_eq!(common["IsExternal"].as_bool(), Some(false));
    let wall = f.of_type("IFCWALL")[0];
    let pset = properties(&f, wall, "Pset_WallCommon");
    assert_eq!(pset["IsExternal"].as_bool(), Some(true), "walls with the exterior build-up are external");
    assert_eq!(pset["Status"].as_str(), Some("NEW"));
    assert_eq!(properties(&f, wall, "Kanavos_Properties")["Σχόλιο"].as_str(), Some("νότια όψη"));
    assert_eq!(f.get(wall).unwrap().str_at(7), Some("Τ1"));
    let stair = properties(&f, f.of_type("IFCSTAIR")[0], "Pset_StairCommon");
    assert_eq!(stair["NumberOfRiser"].as_i64(), Some(18));
}

#[test]
fn walls_carry_layer_set_usages() {
    let h = common::small_house();
    let (_, f) = exported(&h.doc);
    let ext = h.doc.build_up(h.doc.defaults().exterior_wall.unwrap()).unwrap();
    for rel in entities(&f, "IFCRELASSOCIATESMATERIAL") {
        let related = rel.refs_at(4);
        if !related.iter().any(|r| f.type_of(*r) == Some("IFCWALL")) {
            continue;
        }
        assert_eq!(related.len(), 4, "the four walls share one usage");
        let usage = f.get(rel.ref_at(5).unwrap()).unwrap();
        assert_eq!(usage.name.as_ref(), "IFCMATERIALLAYERSETUSAGE");
        assert_eq!(usage.enum_at(1), Some("AXIS2"));
        assert_eq!(usage.enum_at(2), Some("NEGATIVE"));
        // Layers run from the left face, half the thickness left of a centred reference line.
        assert!(close(usage.f64_at(3).unwrap(), ext.thickness().as_m() / 2.0));
        let set = f.get(usage.ref_at(0).unwrap()).unwrap();
        assert_eq!(set.str_at(1), Some(ext.name.as_str()));
        let layers: Vec<(String, f64)> = set
            .refs_at(0)
            .iter()
            .map(|&l| {
                let l = f.get(l).unwrap();
                (f.get(l.ref_at(0).unwrap()).unwrap().str_at(0).unwrap().to_owned(), l.f64_at(1).unwrap())
            })
            .collect();
        let expected: Vec<(String, f64)> = ext
            .layers
            .iter()
            .map(|l| (h.doc.material(l.material).unwrap().name.clone(), l.thickness.as_m()))
            .collect();
        assert_eq!(layers, expected);
        return;
    }
    panic!("no material association for the walls");
}

#[test]
fn greek_text_is_escaped() {
    let (text, f) = exported(&common::small_house().doc);
    assert!(text.contains("\\X2\\"), "non-ASCII text must use \\X2\\ escapes");
    let project = f.get(f.of_type("IFCPROJECT")[0]).unwrap();
    assert_eq!(project.str_at(2), Some("Διώροφη κατοικία"));
    assert_eq!(project.str_at(5), Some("Καλαμάτα"));
}

#[test]
fn georeferencing_is_written_when_set() {
    let (_, f) = exported(&common::full_house().doc);
    let map = f.get(f.of_type("IFCMAPCONVERSION")[0]).unwrap();
    assert!(close(map.f64_at(2).unwrap(), 431_250.0));
    assert!(close(map.f64_at(3).unwrap(), 4_104_300.0));
    assert!(close(map.f64_at(4).unwrap(), 42.5));
    let r = 12f64.to_radians();
    assert!(close(map.f64_at(5).unwrap(), r.cos()) && close(map.f64_at(6).unwrap(), -r.sin()));
    let crs = f.get(map.ref_at(1).unwrap()).unwrap();
    assert_eq!(crs.name.as_ref(), "IFCPROJECTEDCRS");
    assert_eq!(crs.str_at(0), Some("EPSG:2100"));
    assert_eq!(f.type_of(map.ref_at(0).unwrap()), Some("IFCGEOMETRICREPRESENTATIONCONTEXT"));
}

#[test]
fn export_is_deterministic_and_ids_follow_the_project() {
    let h = common::full_house();
    let a = export(&h.doc, &options()).text;
    let b = export(&h.doc, &options()).text;
    assert_eq!(a, b);
    let other = ExportOptions { project_guid: Some(Guid([7; 16])), ..options() };
    let c = export(&h.doc, &other).text;
    assert_ne!(a, c);
    let ids = |t: &str| {
        let f = parse(t.as_bytes()).unwrap();
        f.of_type("IFCWALL")
            .iter()
            .map(|&w| f.get(w).unwrap().str_at(0).unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert!(ids(&a).iter().zip(ids(&c)).all(|(x, y)| *x != y));
    assert_eq!(a.len(), c.len(), "only the identifiers differ");
}

/// Every `GlobalId` in a file (attribute 0 of every rooted entity).
fn global_ids(text: &str) -> Vec<String> {
    let f = parse(text.as_bytes()).unwrap();
    f.data
        .keys()
        .filter_map(|id| f.get(*id))
        .filter_map(|e| e.str_at(0).filter(|s| s.len() == 22 && Guid::from_ifc(s).is_some()))
        .map(str::to_owned)
        .collect()
}

#[test]
fn global_ids_follow_the_project_id_and_survive_renames() {
    let h = common::small_house();
    let id = kanavos_model::ProjectId::from_u128(0x5eed_0000_1111_4222_8333_4444_5555_6666);
    let doc = h.doc.with_project_id(id);
    let before = global_ids(&export(&doc, &options()).text);
    assert!(before.len() > 20);
    // The project namespace is the project id itself.
    let project = Guid::v5(&Guid(id.to_bytes()), b"project").to_ifc();
    assert!(before.contains(&project));
    let mut info = doc.info().clone();
    info.name = "Μετονομασμένο έργο".into();
    info.location = "Αλλού".into();
    info.owner = "Νέος ιδιοκτήτης".into();
    let renamed = doc.with_info(info);
    assert_eq!(global_ids(&export(&renamed, &options()).text), before, "a rename keeps every GlobalId");
    // Another project id gives other identifiers.
    let other = doc.with_project_id(kanavos_model::ProjectId::from_u128(7));
    assert_ne!(global_ids(&export(&other, &options()).text), before);
    // A document without an id falls back to its descriptive fields, which a rename changes.
    let nil = doc.with_project_id(kanavos_model::ProjectId::NIL);
    let renamed_nil = renamed.with_project_id(kanavos_model::ProjectId::NIL);
    assert_ne!(
        global_ids(&export(&nil, &options()).text),
        global_ids(&export(&renamed_nil, &options()).text)
    );
}

#[test]
fn external_ids_are_written_and_copies_get_their_own() {
    let h = common::small_house();
    let stored = Guid::v5(&Guid([2; 16]), b"imported wall").to_ifc();
    let mut doc = h.doc.clone();
    let mut e = doc.element(h.walls[0]).unwrap().clone();
    e.external_id = Some(stored.clone());
    doc = doc.with_element(e.clone());
    // A copy made the way the engine copies (Element::copied_as) carries no external id.
    let (d, copy) = doc.with_new_id();
    let mut c = e.copied_as(copy);
    if let kanavos_model::ElementKind::Wall(w) = &mut c.kind {
        w.start = kanavos_geom::Point2::new(0.0, 20.0);
        w.end = kanavos_geom::Point2::new(10.0, 20.0);
    }
    doc = d.with_element(c);
    let (_, f) = exported(&doc);
    let walls: Vec<String> =
        f.of_type("IFCWALL").iter().map(|&w| f.get(w).unwrap().str_at(0).unwrap().to_owned()).collect();
    assert_eq!(walls.len(), 5);
    assert_eq!(walls.iter().filter(|g| **g == stored).count(), 1);
    assert_eq!(walls[0], stored);
    // An external id that is not an IFC GlobalId (e.g. from another system) is not written.
    let mut e2 = doc.element(h.walls[1]).unwrap().clone();
    e2.external_id = Some("survey-17".into());
    let (_, f) = exported(&doc.with_element(e2));
    let g = f.get(f.of_type("IFCWALL")[1]).unwrap().str_at(0).unwrap().to_owned();
    assert!(Guid::from_ifc(&g).is_some() && g != "survey-17");
}

#[test]
fn stored_global_ids_are_reused_once() {
    let h = common::small_house();
    let stored = Guid::v5(&Guid([1; 16]), b"from another tool").to_ifc();
    let mut doc = h.doc.clone();
    for w in &h.walls[..2] {
        let mut e = doc.element(*w).unwrap().clone();
        e.properties.insert(GLOBAL_ID_PROPERTY.to_owned(), stored.clone());
        doc = doc.with_element(e);
    }
    let (_, f) = exported(&doc);
    let walls: Vec<String> =
        f.of_type("IFCWALL").iter().map(|&w| f.get(w).unwrap().str_at(0).unwrap().to_owned()).collect();
    assert_eq!(walls[0], stored);
    assert_ne!(walls[1], stored, "a copied identifier must not be written twice");
    // The identifier is not repeated as a user property.
    assert!(!properties(&f, f.of_type("IFCWALL")[0], "Kanavos_Properties").contains_key(GLOBAL_ID_PROPERTY));
}

#[test]
fn geometry_callback_replaces_bodies() {
    let h = common::small_house();
    let column = h.column;
    let callback =
        move |id: Id| (id == column).then(|| cuboid(Point3::new(4.8, 3.8, 0.0), Point3::new(5.2, 4.2, 3.2)));
    let opts = ExportOptions { geometry: Some(&callback), ..options() };
    let out = export(&h.doc, &opts);
    let f = parse(out.text.as_bytes()).unwrap();
    let col = f.get(f.of_type("IFCCOLUMN")[0]).unwrap();
    let shape = f.get(col.ref_at(6).unwrap()).unwrap();
    let rep = f.get(shape.refs_at(2)[0]).unwrap();
    assert_eq!(rep.str_at(2), Some("Brep"));
    let item = rep.refs_at(3)[0];
    assert_eq!(f.type_of(item), Some("IFCFACETEDBREP"));
    // The mesh is stored in the column's own frame: its centre (5, 4) maps to the local origin.
    let shell = f.get(f.get(item).unwrap().ref_at(0).unwrap()).unwrap();
    assert_eq!(shell.refs_at(0).len(), 12, "a cuboid mesh has 12 triangles");
    let walls_with_brep = f
        .of_type("IFCSHAPEREPRESENTATION")
        .iter()
        .filter(|&&r| f.get(r).unwrap().str_at(2) == Some("Brep"))
        .count();
    assert_eq!(walls_with_brep, 1);
}

#[test]
fn full_house_exports_every_kind() {
    let h = common::full_house();
    let out = export(&h.doc, &options());
    let f = parse(out.text.as_bytes()).unwrap();
    for (t, n) in [
        ("IFCSTAIR", 1),
        ("IFCBEAM", 1),
        ("IFCRAILING", 1),
        ("IFCROOF", 2),
        ("IFCFURNITURE", 1),
        ("IFCSANITARYTERMINAL", 1),
        ("IFCBUILDINGELEMENTPROXY", 1),
        ("IFCWALL", 5),
        ("IFCOPENINGELEMENT", 4),
        ("IFCTRIMMEDCURVE", 1),
        ("IFCFACETEDBREP", 1),
    ] {
        assert_eq!(count(&f, t), n, "{t}");
    }
    assert_eq!(out.report.skipped.get("text"), Some(&1));
    assert!(out.report.warnings.is_empty(), "{:?}", out.report.warnings);
    assert!(f.dangling_references().is_empty());
    let hip = entities(&f, "IFCROOF").into_iter().find(|r| r.enum_at(8) == Some("HIP_ROOF"));
    assert!(hip.is_some());
    let sanitary = f.get(f.of_type("IFCSANITARYTERMINAL")[0]).unwrap();
    assert_eq!(sanitary.enum_at(8), Some("TOILETPAN"));
}

#[test]
fn writes_a_sample_file() {
    let h = common::full_house();
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("kanavos-sample-house.ifc");
    let report = kanavos_ifc::export_to_file(&path, &h.doc, &options()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let f = parse(text.as_bytes()).unwrap();
    assert_eq!(report.instances as usize, f.data.len());
    assert!(!path.with_extension("ifc.tmp").exists());
}

#[test]
fn pitched_roofs_with_holes_get_openings_and_net_areas() {
    let h = common::full_house();
    let hip = h
        .doc
        .elements()
        .find(|e| matches!(&e.kind, kanavos_model::ElementKind::Roof(r) if matches!(r.shape, kanavos_model::RoofShape::Pitched { .. })))
        .unwrap()
        .clone();
    let mut e = hip.clone();
    let kanavos_model::ElementKind::Roof(r) = &mut e.kind else { panic!() };
    let b = r.outline.bbox().unwrap();
    let c = b.centre();
    let (cx, cy) = (c.x.as_m(), c.y.as_m());
    r.holes = vec![kanavos_geom::Polygon2::from_m(&[
        (cx - 0.5, cy - 0.4),
        (cx + 0.5, cy - 0.4),
        (cx + 0.5, cy + 0.4),
        (cx - 0.5, cy + 0.4),
    ])];
    let doc = h.doc.with_element(e);
    let (_, f) = exported(&doc);
    let before = exported(&h.doc).1;
    assert_eq!(count(&f, "IFCOPENINGELEMENT"), count(&before, "IFCOPENINGELEMENT") + 1);
    assert_eq!(count(&f, "IFCRELVOIDSELEMENT"), count(&before, "IFCRELVOIDSELEMENT") + 1);
    assert!(f.dangling_references().is_empty());
    let roof_of = |f: &StepFile| {
        f.of_type("IFCROOF")
            .iter()
            .copied()
            .find(|&r| f.get(r).unwrap().enum_at(8) == Some("HIP_ROOF"))
            .unwrap()
    };
    let q = quantities(&f, roof_of(&f));
    let q0 = quantities(&before, roof_of(&before));
    assert!(close(q["ProjectedArea"], q0["ProjectedArea"] - 0.8));
    assert!(close(q["GrossArea"], q0["GrossArea"]));
    assert!(q["NetArea"] < q["GrossArea"] - 0.8, "{q:?}");
    // The opening reaches through the roof: its depth covers the roof's height range.
    let opening = f
        .of_type("IFCOPENINGELEMENT")
        .iter()
        .copied()
        .find(|&o| f.get(o).unwrap().str_at(2).is_some_and(|n| n.contains("opening 1")))
        .unwrap();
    let depth = quantities(&f, opening)["Depth"];
    assert!(depth > 0.5, "{depth}");
}
