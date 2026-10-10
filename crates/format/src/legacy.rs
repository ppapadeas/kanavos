//! Upgrades inside schema 1 that need no new schema version.
//!
//! Optional fields added to schema 1 read from older files as their defaults (see `docs/format.md` §4). Some
//! information, however, was kept elsewhere before its field existed; it is moved into place when a file is read,
//! so the rest of the program only knows the current layout. The next save writes the current layout.
//!
//! - Elements imported from IFC kept their `GlobalId` in the user property [`LEGACY_GLOBAL_ID_PROPERTY`]. It now
//!   lives in `Element::external_id`.

use kanavos_model::{Document, Element};

/// User property in which IFC imports kept an element's `GlobalId` before elements had `external_id`.
pub const LEGACY_GLOBAL_ID_PROPERTY: &str = "ifc:GlobalId";

/// Moves layouts of older files into the current fields. Returns the document and the number of elements
/// changed.
///
/// An element with the legacy `GlobalId` property and no external id gets the property's value as its external
/// id, and the property is removed. When the element already has an external id, the property is removed and the
/// field wins.
pub fn upgrade(doc: Document) -> (Document, usize) {
    let changed: Vec<Element> = doc
        .elements()
        .filter(|e| e.properties.contains_key(LEGACY_GLOBAL_ID_PROPERTY))
        .map(|e| {
            let mut e = e.clone();
            let legacy = e.properties.remove(LEGACY_GLOBAL_ID_PROPERTY);
            if e.external_id.is_none() {
                e.external_id = legacy.filter(|v| !v.trim().is_empty());
            }
            e
        })
        .collect();
    let count = changed.len();
    let mut doc = doc;
    for e in changed {
        doc = doc.with_element(e);
    }
    (doc, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::{ElementKind, Id, TextNote};

    fn note(id: u64) -> Element {
        let mut e = Element::on_storey(
            Id(id),
            Id(1),
            ElementKind::Text(TextNote {
                position: kanavos_geom::Point2::new(0.0, 0.0),
                content: "Σημείωση".into(),
                size_mm: 2.5,
                rotation: Default::default(),
                align: Default::default(),
                bold: false,
            }),
        );
        e.properties.insert("Χρήστης".into(), "τιμή".into());
        e
    }

    #[test]
    fn the_legacy_property_moves_into_the_field() {
        let mut a = note(2);
        a.properties.insert(LEGACY_GLOBAL_ID_PROPERTY.into(), "0K7w7JNSD0rP0V1GZMBlQa".into());
        let mut b = note(3);
        b.properties.insert(LEGACY_GLOBAL_ID_PROPERTY.into(), "1aaaaaaaaaaaaaaaaaaaaa".into());
        b.external_id = Some("2bbbbbbbbbbbbbbbbbbbbb".into());
        let c = note(4);
        let doc = Document::new().with_element(a).with_element(b).with_element(c.clone());
        let (doc, n) = upgrade(doc);
        assert_eq!(n, 2);
        let a = doc.element(Id(2)).unwrap();
        assert_eq!(a.external_id.as_deref(), Some("0K7w7JNSD0rP0V1GZMBlQa"));
        assert!(!a.properties.contains_key(LEGACY_GLOBAL_ID_PROPERTY));
        assert_eq!(a.properties.get("Χρήστης").map(String::as_str), Some("τιμή"));
        assert_eq!(doc.element(Id(3)).unwrap().external_id.as_deref(), Some("2bbbbbbbbbbbbbbbbbbbbb"));
        assert_eq!(doc.element(Id(4)), Some(&c));
        // A current document is left alone.
        let (again, n) = upgrade(doc.clone());
        assert_eq!(n, 0);
        assert_eq!(again, doc);
    }
}
