//! Every user-facing string of the tools is in the catalogue, in Greek and English.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kanavos_model::Category;
use kanavos_tools::{SnapHint, ToolId};

const SOURCES: &[&str] = &[
    include_str!("../src/annotate.rs"),
    include_str!("../src/common.rs"),
    include_str!("../src/lib.rs"),
    include_str!("../src/measure.rs"),
    include_str!("../src/opening.rs"),
    include_str!("../src/outline.rs"),
    include_str!("../src/path.rs"),
    include_str!("../src/place.rs"),
    include_str!("../src/preview.rs"),
    include_str!("../src/select.rs"),
    include_str!("../src/snap.rs"),
    include_str!("../src/wall.rs"),
];

/// Every complete `"tools.…"` string literal in the sources.
fn literal_keys() -> Vec<String> {
    let mut keys = Vec::new();
    for src in SOURCES {
        let mut rest = *src;
        while let Some(i) = rest.find("\"tools.") {
            let after = &rest[i + 1..];
            let end = after.find('"').unwrap_or(after.len());
            let key = &after[..end];
            if key.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_') && !key.ends_with('.') {
                keys.push(key.to_owned());
            }
            rest = &after[end.min(after.len())..];
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

#[test]
fn every_key_used_in_the_code_exists() {
    let keys = literal_keys();
    assert!(keys.len() > 80, "found {} keys", keys.len());
    let missing: Vec<&String> = keys.iter().filter(|k| !kanavos_i18n::has(k)).collect();
    assert!(missing.is_empty(), "missing catalogue entries: {missing:?}");
}

#[test]
fn every_tool_category_and_snap_has_a_name() {
    for id in ToolId::ALL {
        assert!(kanavos_i18n::has(&format!("tools.{}", id.key())), "{id:?}");
    }
    for c in Category::ALL {
        for form in ["of", "to"] {
            assert!(kanavos_i18n::has(&format!("tools.ref.{}.{form}", c.key())), "{c:?} {form}");
        }
    }
    let hints = [
        SnapHint::Free,
        SnapHint::Endpoint,
        SnapHint::Midpoint,
        SnapHint::Corner,
        SnapHint::Centre,
        SnapHint::Intersection,
        SnapHint::OnEdge,
        SnapHint::Perpendicular,
        SnapHint::Extension,
        SnapHint::Angle,
        SnapHint::Grid,
        SnapHint::Parallel,
        SnapHint::Alignment,
        SnapHint::Face,
        SnapHint::Centreline,
        SnapHint::LayerBoundary,
        SnapHint::Insertion,
        SnapHint::Locked,
    ];
    for h in hints {
        assert!(kanavos_i18n::has(h.key()), "{h:?}");
    }
    assert!(kanavos_i18n::check().is_empty(), "{:?}", kanavos_i18n::check());
}

#[test]
fn every_tool_handles_events_without_panicking() {
    use kanavos_tools::{Key, Modifiers, MouseButton, SnapSettings, ToolContext, ToolEvent, ToolSettings};
    let doc = kanavos_model::starter();
    let storey = doc.storeys()[0].id;
    let sel = Default::default();
    let snap = SnapSettings::default();
    let settings = ToolSettings::default();
    for locale in [kanavos_units::Locale::El, kanavos_units::Locale::En] {
        for id in ToolId::ALL {
            let ctx = ToolContext {
                doc: &doc,
                storey,
                selection: &sel,
                pixel_size: 0.01,
                snap: &snap,
                settings: &settings,
                locale,
            };
            let mut tool = kanavos_tools::make_tool(id);
            assert_eq!(tool.id(), id);
            let m = Modifiers::default();
            let at = kanavos_geom::Point2::new(1.0, 1.0);
            let events = [
                ToolEvent::Move { at, mods: m },
                ToolEvent::Press { at, button: MouseButton::Primary, mods: m },
                ToolEvent::Release { at, button: MouseButton::Primary, mods: m },
                ToolEvent::Text("2".into()),
                ToolEvent::Key { key: Key::Tab, mods: m },
                ToolEvent::Key { key: Key::Flip, mods: m },
                ToolEvent::Key { key: Key::Rotate, mods: m },
                ToolEvent::Key { key: Key::Backspace, mods: m },
                ToolEvent::Key { key: Key::Escape, mods: m },
                ToolEvent::Key { key: Key::Escape, mods: m },
            ];
            for ev in &events {
                let out = tool.handle(&ctx, ev);
                assert!(!out.prompt.is_empty(), "{id:?} has a prompt after {ev:?}");
                assert!(!out.prompt.contains("tools."), "{id:?}: untranslated prompt {}", out.prompt);
            }
            tool.cancel();
            assert!(tool.is_idle(), "{id:?} is idle after cancel");
        }
    }
}
