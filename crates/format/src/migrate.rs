//! Schema migrations for `model.json`.
//!
//! Each step upgrades the JSON of one schema version to the next, working on untyped [`serde_json::Value`]s so
//! that old layouts never need Rust types of their own. Loading an older file runs every step from the file's
//! version up to [`CURRENT_SCHEMA_VERSION`], then deserialises the result as the current [`Document`].
//!
//! To change the schema:
//!
//! 1. bump [`CURRENT_SCHEMA_VERSION`];
//! 2. add a step `{ from: old, … }` to [`STEPS`] that rewrites the old JSON into the new shape;
//! 3. add a test with a hand-written file in the old layout;
//! 4. describe the change in `docs/format.md`.
//!
//! [`Document`]: kanavos_model::Document

use serde_json::{Map, Value};

use crate::FormatError;

/// Schema version written by this build.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// Oldest schema version this build can upgrade from.
pub const OLDEST_SCHEMA_VERSION: u32 = 0;

/// One migration step: rewrites the JSON of schema `from` into schema `from + 1`.
struct Step {
    from: u32,
    apply: fn(Value) -> Result<Value, String>,
}

/// All steps, in order.
const STEPS: &[Step] = &[Step { from: 0, apply: v0_to_v1 }, Step { from: 1, apply: v1_to_v2 }];

/// Fails unless this build can read schema `version`, directly or through migrations.
// While the oldest readable version is 0 the lower bound check is trivially true; it stays so that raising
// `OLDEST_SCHEMA_VERSION` later needs no other change.
#[allow(clippy::absurd_extreme_comparisons)]
pub(crate) fn check_readable(version: u32) -> Result<(), FormatError> {
    if version > CURRENT_SCHEMA_VERSION {
        return Err(FormatError::NewerVersion { found: version, supported: CURRENT_SCHEMA_VERSION });
    }
    if version < OLDEST_SCHEMA_VERSION {
        return Err(FormatError::Migration {
            from: version,
            message: "this schema version is too old".into(),
        });
    }
    Ok(())
}

/// Upgrades `model` from schema `from` to [`CURRENT_SCHEMA_VERSION`].
///
/// Fails if `from` is newer than this build, older than [`OLDEST_SCHEMA_VERSION`], or a step rejects the data.
pub fn migrate(mut model: Value, from: u32) -> Result<Value, FormatError> {
    check_readable(from)?;
    for version in from..CURRENT_SCHEMA_VERSION {
        let step = STEPS.iter().find(|s| s.from == version).ok_or_else(|| FormatError::Migration {
            from: version,
            message: "no upgrade step is defined".into(),
        })?;
        model = (step.apply)(model).map_err(|message| FormatError::Migration { from: version, message })?;
    }
    Ok(model)
}

/// Schema 0 → 1.
///
/// Schema 0 is the draft layout used while the format was designed; no released build wrote it. It is kept as the
/// worked example of a migration. It differs from schema 1 in two ways:
///
/// - the id counter is called `last_id` (schema 1: `next_id`);
/// - `storeys`, `materials`, `build_ups`, `elements`, `views` and `sheets` are arrays of objects carrying an `id`
///   (schema 1: objects keyed by id).
fn v0_to_v1(model: Value) -> Result<Value, String> {
    let Value::Object(mut root) = model else {
        return Err("the model is not a JSON object".into());
    };
    if let Some(counter) = root.remove("last_id") {
        if root.contains_key("next_id") {
            return Err("both last_id and next_id are present".into());
        }
        root.insert("next_id".into(), counter);
    }
    for table in ["storeys", "materials", "build_ups", "elements", "views", "sheets"] {
        let Some(value) = root.remove(table) else { continue };
        let keyed = match value {
            Value::Array(items) => {
                let mut map = Map::new();
                for (i, item) in items.into_iter().enumerate() {
                    let id = item
                        .get("id")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| format!("{table}[{i}] has no numeric id"))?;
                    if map.insert(id.to_string(), item).is_some() {
                        return Err(format!("{table} lists id {id} twice"));
                    }
                }
                Value::Object(map)
            }
            // Already keyed (a file written half-way through the draft): keep as is.
            other @ Value::Object(_) => other,
            _ => return Err(format!("{table} is neither a list nor an object")),
        };
        root.insert(table.into(), keyed);
    }
    Ok(Value::Object(root))
}

/// Schema 1 → 2.
///
/// Schema 2 gives an opening's `flip_side` the orientation of the wall build-ups (`specs/objects/orientation.md`):
/// unflipped, a window's outer face is its wall's left face and a door opens towards the right face. In schema 1 it
/// was the other way round, so every opening's `flip_side` is inverted (a missing one counts as `false`) and doors
/// and windows stay where they were.
fn v1_to_v2(model: Value) -> Result<Value, String> {
    let Value::Object(mut root) = model else {
        return Err("the model is not a JSON object".into());
    };
    match root.get_mut("elements") {
        None => {}
        Some(Value::Object(elements)) => {
            for (id, element) in elements.iter_mut() {
                let Some(kind) = element.get_mut("kind").and_then(Value::as_object_mut) else { continue };
                if kind.get("type").and_then(Value::as_str) != Some("opening") {
                    continue;
                }
                let flipped = match kind.get("flip_side") {
                    None | Some(Value::Null) => false,
                    Some(Value::Bool(b)) => *b,
                    Some(_) => return Err(format!("elements.{id}.kind.flip_side is not a boolean")),
                };
                kind.insert("flip_side".into(), Value::Bool(!flipped));
            }
        }
        Some(_) => return Err("elements is not an object".into()),
    }
    Ok(Value::Object(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v0_lists_become_keyed_maps() {
        let v0 = json!({
            "last_id": 3,
            "storeys": [{"id": 1, "name": "Ισόγειο", "elevation": 0.0, "height": 3.0}],
            "materials": [],
        });
        let v1 = migrate(v0, 0).unwrap();
        assert_eq!(v1["next_id"], 3);
        assert!(v1.get("last_id").is_none());
        assert_eq!(v1["storeys"]["1"]["name"], "Ισόγειο");
        assert!(v1["materials"].as_object().unwrap().is_empty());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let v0 = json!({"last_id": 2, "storeys": [{"id": 1}, {"id": 1}]});
        let err = migrate(v0, 0).unwrap_err();
        assert!(matches!(err, FormatError::Migration { from: 0, .. }), "{err}");
    }

    #[test]
    fn v1_openings_swap_their_side_flag() {
        let v1 = json!({
            "next_id": 9,
            "elements": {
                "3": {"id": 3, "storey": 1, "kind": {"type": "wall", "start": [0.0, 0.0], "end": [5.0, 0.0]}},
                "4": {"id": 4, "storey": 1, "kind": {"type": "opening", "host": 3, "flip_side": false}},
                "5": {"id": 5, "storey": 1, "kind": {"type": "opening", "host": 3, "flip_side": true}},
                "6": {"id": 6, "storey": 1, "kind": {"type": "opening", "host": 3, "flip_hand": true}},
            },
        });
        let v2 = migrate(v1, 1).unwrap();
        let flip = |id: &str| v2["elements"][id]["kind"]["flip_side"].clone();
        assert_eq!(flip("4"), json!(true));
        assert_eq!(flip("5"), json!(false));
        assert_eq!(flip("6"), json!(true), "a missing flag was false");
        assert_eq!(v2["elements"]["6"]["kind"]["flip_hand"], json!(true), "the hand is unchanged");
        assert!(v2["elements"]["3"]["kind"].get("flip_side").is_none(), "walls are untouched");
        // Schema 0 files pass through both steps.
        let v0 = json!({"last_id": 2, "elements": [{"id": 1, "kind": {"type": "opening", "host": 9}}]});
        assert_eq!(migrate(v0, 0).unwrap()["elements"]["1"]["kind"]["flip_side"], json!(true));
    }

    #[test]
    fn a_malformed_side_flag_is_reported() {
        let v1 = json!({"elements": {"4": {"id": 4, "kind": {"type": "opening", "flip_side": "yes"}}}});
        let err = migrate(v1, 1).unwrap_err();
        assert!(matches!(err, FormatError::Migration { from: 1, .. }), "{err}");
        assert!(err.to_string().contains("elements.4.kind.flip_side"), "{err}");
    }

    #[test]
    fn current_version_is_untouched_and_newer_is_refused() {
        let v = json!({"next_id": 1});
        assert_eq!(migrate(v.clone(), CURRENT_SCHEMA_VERSION).unwrap(), v);
        assert!(matches!(
            migrate(v, CURRENT_SCHEMA_VERSION + 1),
            Err(FormatError::NewerVersion { found: 3, supported: 2 })
        ));
    }

    #[test]
    fn every_version_has_a_step() {
        for v in OLDEST_SCHEMA_VERSION..CURRENT_SCHEMA_VERSION {
            assert!(STEPS.iter().any(|s| s.from == v), "no step from {v}");
        }
    }
}
