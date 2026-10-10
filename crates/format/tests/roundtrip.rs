//! Save, load and error behaviour of `.kanavos` files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::io::{Cursor, Write};
use std::path::PathBuf;

use kanavos_format::{
    CURRENT_SCHEMA_VERSION, FormatError, LoadOptions, LoadWarning, MANIFEST_ENTRY, MODEL_ENTRY, SaveOptions,
    THUMBNAIL_ENTRY, Validation, autosave, autosave_with, backup_path, discard_recovery, find_recovery,
    from_bytes, from_bytes_with, load, read_manifest, save, to_bytes,
};
use kanavos_geom::Length;
use kanavos_model::{Document, Element, ElementKind, Id, starter};
use serde_json::{Value, json};

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

fn options() -> SaveOptions {
    SaveOptions {
        app_version: "0.0.1-test".into(),
        created: Some("2026-10-01T08:00:00Z".into()),
        modified: Some("2026-10-09T12:30:00+03:00".into()),
        ..SaveOptions::default()
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("kanavos-format").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Builds an archive from raw entries, for hand-made test files.
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut out).unwrap();
    out
}

#[test]
fn starter_round_trip_is_byte_identical() {
    let doc = starter();
    let a = to_bytes(&doc, &options()).unwrap();
    let loaded = from_bytes(&a).unwrap();
    assert_eq!(loaded.document, doc);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let b = to_bytes(&loaded.document, &SaveOptions::resave(&loaded.manifest, "2026-10-09T12:30:00+03:00"))
        .unwrap();
    // `resave` uses the crate version as app version; use the same options for an exact comparison.
    let b_same = to_bytes(&loaded.document, &options()).unwrap();
    assert_eq!(a, b_same);
    assert_ne!(a.len(), 0);
    assert_eq!(from_bytes(&b).unwrap().manifest.created.as_deref(), Some("2026-10-01T08:00:00Z"));
}

#[test]
fn house_round_trip_is_byte_identical_with_thumbnail() {
    let doc = common::house();
    let opts = SaveOptions { thumbnail: Some(PNG.to_vec()), ..options() };
    let a = to_bytes(&doc, &opts).unwrap();
    let loaded = from_bytes(&a).unwrap();
    assert_eq!(loaded.document, doc);
    assert_eq!(loaded.thumbnail.as_deref(), Some(PNG));
    let b = to_bytes(&loaded.document, &SaveOptions { thumbnail: loaded.thumbnail.clone(), ..options() })
        .unwrap();
    assert_eq!(a, b);
    // Same document, same options: same bytes, every time.
    assert_eq!(a, to_bytes(&doc, &opts).unwrap());
}

#[test]
fn manifest_describes_the_project() {
    let doc = common::house();
    let bytes = to_bytes(&doc, &options()).unwrap();
    let manifest: Value = serde_json::from_slice(&entry(&bytes, MANIFEST_ENTRY)).unwrap();
    assert_eq!(manifest["format"], "kanavos");
    assert_eq!(manifest["schema_version"], CURRENT_SCHEMA_VERSION);
    assert_eq!(manifest["app_version"], "0.0.1-test");
    assert_eq!(manifest["created"], "2026-10-01T08:00:00Z");
    assert_eq!(manifest["counts"]["storeys"], 2);
    assert_eq!(manifest["counts"]["by_category"]["wall"], 4);
    assert_eq!(manifest["counts"]["by_category"]["door"], 1);
    assert_eq!(manifest["thumbnail"], false);
    // Entry order is fixed: manifest first, then the model.
    let zip = zip::ZipArchive::new(Cursor::new(&bytes[..])).unwrap();
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    assert_eq!(names, vec![MANIFEST_ENTRY, MODEL_ENTRY]);
    // The model is readable JSON text.
    let model = String::from_utf8(entry(&bytes, MODEL_ENTRY)).unwrap();
    assert!(model.contains("\"Καθιστικό\""));
}

#[test]
fn save_is_atomic_and_keeps_a_backup() {
    let dir = scratch("save");
    let path = dir.join("house.kanavos");
    let first = starter();
    save(&path, &first, &options()).unwrap();
    assert!(!backup_path(&path).exists(), "no backup when there was no previous file");
    let first_bytes = std::fs::read(&path).unwrap();

    let second = common::house();
    let manifest = save(&path, &second, &options()).unwrap();
    assert_eq!(manifest.counts.elements, second.element_count());
    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), first_bytes);
    assert_eq!(load(&path).unwrap().document, second);
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert_eq!(read_manifest(&path).unwrap().counts.elements, second.element_count());

    // Without backups the previous .bak is left as it was.
    save(&path, &first, &SaveOptions { backup: false, ..options() }).unwrap();
    assert_eq!(std::fs::read(backup_path(&path)).unwrap(), first_bytes);
}

#[test]
fn saving_into_a_missing_directory_fails_cleanly() {
    let dir = scratch("missing");
    let path = dir.join("no-such-dir").join("house.kanavos");
    let err = save(&path, &starter(), &options()).unwrap_err();
    assert!(matches!(err, FormatError::Io { .. }), "{err}");
}

#[test]
fn not_a_zip_is_reported() {
    assert!(matches!(from_bytes(b"{\"format\":\"kanavos\"}"), Err(FormatError::NotAnArchive)));
    assert!(matches!(from_bytes(b""), Err(FormatError::NotAnArchive)));
    let truncated = &to_bytes(&starter(), &options()).unwrap()[..100];
    assert!(matches!(from_bytes(truncated), Err(FormatError::CorruptArchive(_))));
}

#[test]
fn other_zip_files_are_the_wrong_format() {
    let other = archive(&[("word/document.xml", b"<w/>")]);
    assert!(matches!(from_bytes(&other), Err(FormatError::WrongFormat(_))));
    let other = archive(&[(MANIFEST_ENTRY, br#"{"format":"something-else","schema_version":1}"#)]);
    let err = from_bytes(&other).unwrap_err();
    assert!(matches!(err, FormatError::WrongFormat(_)));
    assert!(err.to_string().contains("something-else"));
}

#[test]
fn newer_schema_is_refused_before_reading_the_model() {
    let file = archive(&[(
        MANIFEST_ENTRY,
        br#"{"format":"kanavos","schema_version":99,"app_version":"9.0","counts":"not even an object"}"#,
    )]);
    match from_bytes(&file) {
        Err(FormatError::NewerVersion { found: 99, supported }) => {
            assert_eq!(supported, CURRENT_SCHEMA_VERSION)
        }
        other => panic!("expected NewerVersion, got {other:?}"),
    }
}

#[test]
fn missing_model_is_reported() {
    let file = archive(&[(MANIFEST_ENTRY, br#"{"format":"kanavos","schema_version":1}"#)]);
    assert!(matches!(from_bytes(&file), Err(FormatError::MissingEntry("model.json"))));
}

#[test]
fn corrupt_json_names_the_failing_field() {
    let doc = common::house();
    let bytes = to_bytes(&doc, &options()).unwrap();
    let mut model: Value = serde_json::from_slice(&entry(&bytes, MODEL_ENTRY)).unwrap();
    let wall_id = doc.elements().find(|e| e.as_wall().is_some()).unwrap().id;
    model["elements"][wall_id.0.to_string()]["kind"]["height"] = json!("tall");
    let model = serde_json::to_vec_pretty(&model).unwrap();
    let broken = archive(&[(MANIFEST_ENTRY, &entry(&bytes, MANIFEST_ENTRY)), (MODEL_ENTRY, &model)]);
    match from_bytes(&broken) {
        Err(FormatError::CorruptJson { entry, path, .. }) => {
            assert_eq!(entry, MODEL_ENTRY);
            assert!(path.starts_with(&format!("elements.{}", wall_id.0)), "path {path}");
        }
        other => panic!("expected CorruptJson, got {other:?}"),
    }

    let garbage =
        archive(&[(MANIFEST_ENTRY, &entry(&bytes, MANIFEST_ENTRY)), (MODEL_ENTRY, b"{\n  \"next_id\": ")]);
    match from_bytes(&garbage) {
        Err(FormatError::CorruptJson { line, .. }) => assert_eq!(line, 2),
        other => panic!("expected CorruptJson, got {other:?}"),
    }
}

#[test]
fn non_finite_numbers_are_refused_with_their_path() {
    let doc = starter();
    let storey = doc.storeys()[0].clone();
    let mut bad = storey.clone();
    bad.height = Length::m(f64::NAN);
    let doc = doc.with_storey(bad);
    match to_bytes(&doc, &options()) {
        Err(FormatError::NonFinite { path }) => assert_eq!(path, format!("storeys.{}.height", storey.id.0)),
        other => panic!("expected NonFinite, got {other:?}"),
    }
}

#[test]
fn options_are_checked() {
    let doc = starter();
    let bad_time = SaveOptions { modified: Some("yesterday".into()), ..options() };
    assert!(matches!(to_bytes(&doc, &bad_time), Err(FormatError::InvalidTimestamp(_))));
    let bad_png = SaveOptions { thumbnail: Some(b"GIF89a".to_vec()), ..options() };
    assert!(matches!(to_bytes(&doc, &bad_png), Err(FormatError::InvalidThumbnail)));
}

#[test]
fn inconsistent_documents_are_refused_unless_lenient() {
    let doc = starter();
    let storey = doc.storeys()[0].id;
    let (doc, id) = doc.with_new_id();
    let doc = doc.with_element(Element::on_storey(
        id,
        storey,
        ElementKind::Opening(kanavos_model::Opening {
            host: Id(9999),
            kind: Default::default(),
            style: "door.single_swing".into(),
            offset: Length::m(1.0),
            width: Length::m(0.9),
            height: Length::m(2.1),
            sill: Length::ZERO,
            flip_side: false,
            flip_hand: false,
            params: Default::default(),
        }),
    ));
    let bytes = to_bytes(&doc, &options()).unwrap();
    assert!(matches!(from_bytes(&bytes), Err(FormatError::Invalid(_))));
    let lenient = from_bytes_with(&bytes, &LoadOptions { validation: Validation::Lenient }).unwrap();
    assert!(matches!(lenient.warnings.as_slice(), [LoadWarning::Invalid(_)]));
    assert_eq!(lenient.document, doc);
}

#[test]
fn unknown_entries_and_mismatched_counts_are_warnings() {
    let bytes = to_bytes(&common::house(), &options()).unwrap();
    let mut manifest: Value = serde_json::from_slice(&entry(&bytes, MANIFEST_ENTRY)).unwrap();
    manifest["counts"]["elements"] = json!(1);
    manifest["future_field"] = json!({"anything": true});
    let file = archive(&[
        (MANIFEST_ENTRY, &serde_json::to_vec(&manifest).unwrap()),
        (MODEL_ENTRY, &entry(&bytes, MODEL_ENTRY)),
        ("assets/logo.svg", b"<svg/>"),
        ("notes.txt", b"hello"),
        (THUMBNAIL_ENTRY, b"not a png"),
    ]);
    let loaded = from_bytes(&file).unwrap();
    assert!(loaded.warnings.contains(&LoadWarning::UnknownEntry("notes.txt".into())));
    assert!(loaded.warnings.contains(&LoadWarning::BadThumbnail));
    assert!(loaded.warnings.iter().any(|w| matches!(w, LoadWarning::CountMismatch { manifest: 1, .. })));
    assert!(
        !loaded
            .warnings
            .iter()
            .any(|w| matches!(w, LoadWarning::UnknownEntry(n) if n.starts_with("assets/")))
    );
    assert!(loaded.thumbnail.is_none());
}

/// A file in the synthetic schema 0 layout (tables as arrays, `last_id`), as described in `docs/format.md`.
#[test]
fn schema_0_files_are_upgraded() {
    let v0_model = json!({
        "last_id": 4,
        "info": {"name": "Παλιό έργο"},
        "storeys": [
            {"id": 1, "name": "Ισόγειο", "elevation": 0.0, "height": 3.0},
        ],
        "materials": [
            {"id": 2, "name": "Σκυρόδεμα", "color": {"r": 200, "g": 200, "b": 200, "a": 255}},
        ],
        "elements": [
            {"id": 3, "storey": 1, "kind": {
                "type": "wall", "start": [0.0, 0.0], "end": [5.0, 0.0],
                "structure": {"type": "basic", "material": 2, "thickness": 0.25},
                "height": 3.0
            }},
        ],
        "views": [],
    });
    let manifest = json!({"format": "kanavos", "schema_version": 0, "app_version": "0.0.0"});
    let file = archive(&[
        (MANIFEST_ENTRY, &serde_json::to_vec(&manifest).unwrap()),
        (MODEL_ENTRY, &serde_json::to_vec(&v0_model).unwrap()),
    ]);
    let loaded = from_bytes_with(&file, &LoadOptions { validation: Validation::Strict }).unwrap();
    // The only warning is the upgrade: a manifest without counts is not a count mismatch.
    assert_eq!(loaded.warnings, vec![LoadWarning::Migrated { from: 0, to: CURRENT_SCHEMA_VERSION }]);
    let doc = &loaded.document;
    assert_eq!(doc.info().name, "Παλιό έργο");
    assert_eq!(doc.storeys().len(), 1);
    assert_eq!(doc.peek_next_id(), Id(5));
    let wall = doc.element(Id(3)).and_then(Element::as_wall).unwrap();
    assert!(doc.tolerance().equal(wall.length(), Length::m(5.0)));
    // Saving writes the current schema, and that file loads without migration.
    let resaved = from_bytes(&to_bytes(doc, &options()).unwrap()).unwrap();
    assert_eq!(resaved.manifest.schema_version, CURRENT_SCHEMA_VERSION);
    assert!(resaved.warnings.is_empty());
    assert_eq!(&resaved.document, doc);
}

#[test]
fn autosave_and_recovery() {
    let dir = scratch("recovery").join("session");
    assert!(find_recovery(&dir).unwrap().is_none());
    let doc = common::house();
    let path = autosave(&dir, &doc).unwrap();
    assert!(path.exists());
    let found = find_recovery(&dir).unwrap().unwrap();
    assert_eq!(found.path, path);
    assert_eq!(found.manifest.as_ref().unwrap().counts.elements, doc.element_count());
    assert_eq!(load(&found.path).unwrap().document, doc);

    // A second autosave replaces the first; recovery_of names the project.
    let opts = SaveOptions { recovery_of: Some("C:/projects/house.kanavos".into()), ..options() };
    autosave_with(&dir, &starter(), &opts).unwrap();
    let found = find_recovery(&dir).unwrap().unwrap();
    assert_eq!(found.manifest.unwrap().recovery_of.as_deref(), Some("C:/projects/house.kanavos"));
    assert!(!backup_path(&path).exists());

    discard_recovery(&dir).unwrap();
    assert!(find_recovery(&dir).unwrap().is_none());
    discard_recovery(&dir).unwrap();
}

#[test]
fn damaged_recovery_file_is_still_found() {
    let dir = scratch("damaged-recovery");
    std::fs::write(dir.join(kanavos_format::RECOVERY_FILE_NAME), b"half a file").unwrap();
    let found = find_recovery(&dir).unwrap().unwrap();
    assert!(found.manifest.is_none());
    assert!(load(&found.path).is_err());
    assert!(matches!(read_manifest(&found.path), Err(FormatError::NotAnArchive)));
    let truncated = to_bytes(&starter(), &options()).unwrap();
    std::fs::write(&found.path, &truncated[..truncated.len() / 2]).unwrap();
    assert!(matches!(read_manifest(&found.path), Err(FormatError::CorruptArchive(_))));
}

#[test]
fn empty_document_round_trips() {
    let doc = Document::new();
    let bytes = to_bytes(&doc, &SaveOptions::default()).unwrap();
    let loaded = from_bytes(&bytes).unwrap();
    assert_eq!(loaded.document, doc);
    assert!(loaded.manifest.created.is_none());
    assert_eq!(bytes, to_bytes(&loaded.document, &SaveOptions::default()).unwrap());
}

/// A schema 1 model as written before the fields added within schema 1 existed: no project id, and an imported
/// wall whose IFC GlobalId is a user property.
fn early_schema_1_model() -> Value {
    json!({
        "next_id": 4,
        "info": {"name": "Έργο του 2025"},
        "storeys": {"1": {"id": 1, "name": "Ισόγειο", "elevation": 0.0, "height": 3.0}},
        "materials": {"2": {"id": 2, "name": "Σκυρόδεμα", "color": {"r": 200, "g": 200, "b": 200, "a": 255}}},
        "elements": {
            "3": {"id": 3, "storey": 1, "tag": "Τ1",
                  "properties": {"ifc:GlobalId": "0aWallGuid000000000001", "Σχόλιο": "υπάρχον"},
                  "kind": {
                "type": "wall", "start": [0.0, 0.0], "end": [5.0, 0.0],
                "structure": {"type": "basic", "material": 2, "thickness": 0.25},
                "height": 3.0
            }}
        }
    })
}

fn schema_1_file(model: &Value) -> Vec<u8> {
    let manifest = json!({"format": "kanavos", "schema_version": 1, "app_version": "0.0.1"});
    archive(&[
        (MANIFEST_ENTRY, &serde_json::to_vec(&manifest).unwrap()),
        (MODEL_ENTRY, &serde_json::to_vec(model).unwrap()),
    ])
}

#[test]
fn early_schema_1_files_load_with_the_added_fields_at_their_defaults() {
    let loaded = from_bytes(&schema_1_file(&early_schema_1_model())).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let doc = &loaded.document;
    assert!(doc.project_id().is_nil(), "no id until the application saves the project");
    let wall = doc.element(Id(3)).unwrap();
    // The GlobalId kept as a user property moved into the external id; other properties stay.
    assert_eq!(wall.external_id.as_deref(), Some("0aWallGuid000000000001"));
    assert!(!wall.properties.contains_key("ifc:GlobalId"));
    assert_eq!(wall.properties.get("Σχόλιο").map(String::as_str), Some("υπάρχον"));
    // The next save writes the current layout, which reads back unchanged.
    let bytes = to_bytes(doc, &options()).unwrap();
    let model: Value = serde_json::from_slice(&entry(&bytes, MODEL_ENTRY)).unwrap();
    assert_eq!(model["elements"]["3"]["external_id"], "0aWallGuid000000000001");
    assert!(model.get("project_id").is_none());
    assert_eq!(&from_bytes(&bytes).unwrap().document, doc);
}
