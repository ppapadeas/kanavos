//! The project document: storeys, elements, attributes and relations.
//!
//! Layer L3. See `docs/architecture.md`.
//!
//! A [`Document`] is an immutable value. Changes produce a new document (see `kanavos-engine`), and unchanged parts
//! are shared between versions through reference counting, so keeping every version for undo is cheap.

use std::collections::BTreeMap;
use std::sync::Arc;

use kanavos_geom::Point2;
use kanavos_units::{Length, Tolerance};

/// Stable identifier of anything in a document. Never reused within a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub u64);

/// A horizontal level of the building.
#[derive(Debug, Clone, PartialEq)]
pub struct Storey {
    /// Identifier.
    pub id: Id,
    /// Display name, e.g. "Ισόγειο".
    pub name: String,
    /// Elevation of the finished floor above project zero.
    pub elevation: Length,
    /// Floor-to-floor height.
    pub height: Length,
}

/// Where a wall's reference line sits relative to its thickness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReferenceLine {
    /// Thickness extends to the left of the start→end direction.
    Left,
    /// Thickness is centred on the line.
    #[default]
    Centre,
    /// Thickness extends to the right of the start→end direction.
    Right,
}

/// A straight wall.
#[derive(Debug, Clone, PartialEq)]
pub struct Wall {
    /// Identifier.
    pub id: Id,
    /// Storey the wall is placed on.
    pub storey: Id,
    /// Start of the reference line.
    pub start: Point2,
    /// End of the reference line.
    pub end: Point2,
    /// Position of the reference line.
    pub reference: ReferenceLine,
    /// Total thickness.
    pub thickness: Length,
    /// Height from the storey's floor.
    pub height: Length,
}

impl Wall {
    /// Length of the reference line.
    pub fn length(&self) -> Length {
        self.start.distance(self.end)
    }
}

/// Any element in the model.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// A wall.
    Wall(Wall),
}

impl Element {
    /// The element's identifier.
    pub fn id(&self) -> Id {
        match self {
            Element::Wall(w) => w.id,
        }
    }
    /// The storey the element belongs to.
    pub fn storey(&self) -> Id {
        match self {
            Element::Wall(w) => w.storey,
        }
    }
}

/// The project document.
#[derive(Debug, Clone, Default)]
pub struct Document {
    next_id: u64,
    tolerance: Tolerance,
    storeys: Arc<BTreeMap<Id, Storey>>,
    elements: Arc<BTreeMap<Id, Element>>,
}

impl Document {
    /// An empty document.
    pub fn new() -> Self {
        Document::default()
    }

    /// Project tolerance.
    pub fn tolerance(&self) -> Tolerance {
        self.tolerance
    }

    /// Reserves a fresh identifier, returning it with the updated document.
    pub fn with_new_id(&self) -> (Document, Id) {
        let mut next = self.clone();
        next.next_id += 1;
        (next, Id(self.next_id + 1))
    }

    /// All storeys ordered by elevation.
    pub fn storeys(&self) -> Vec<&Storey> {
        let mut v: Vec<&Storey> = self.storeys.values().collect();
        v.sort_by(|a, b| a.elevation.as_m().total_cmp(&b.elevation.as_m()));
        v
    }

    /// Looks up a storey.
    pub fn storey(&self, id: Id) -> Option<&Storey> {
        self.storeys.get(&id)
    }

    /// Looks up an element.
    pub fn element(&self, id: Id) -> Option<&Element> {
        self.elements.get(&id)
    }

    /// All elements in id order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.elements.values()
    }

    /// All walls on a storey.
    pub fn walls_on(&self, storey: Id) -> impl Iterator<Item = &Wall> {
        self.elements.values().filter_map(move |e| match e {
            Element::Wall(w) if w.storey == storey => Some(w),
            _ => None,
        })
    }

    /// Returns a document with the storey inserted or replaced.
    pub fn with_storey(&self, storey: Storey) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.storeys).insert(storey.id, storey);
        next
    }

    /// Returns a document with the element inserted or replaced.
    pub fn with_element(&self, element: Element) -> Document {
        let mut next = self.clone();
        Arc::make_mut(&mut next.elements).insert(element.id(), element);
        next
    }

    /// Returns a document without the element, or `None` if it does not exist.
    pub fn without_element(&self, id: Id) -> Option<Document> {
        if !self.elements.contains_key(&id) {
            return None;
        }
        let mut next = self.clone();
        Arc::make_mut(&mut next.elements).remove(&id);
        Some(next)
    }

    /// Whether two documents share the same element storage (no element changed between them).
    pub fn shares_elements_with(&self, other: &Document) -> bool {
        Arc::ptr_eq(&self.elements, &other.elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(doc: &Document) -> (Document, Id) {
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_storey(Storey {
            id,
            name: "Ισόγειο".into(),
            elevation: Length::ZERO,
            height: Length::m(3.2),
        });
        (doc, id)
    }

    #[test]
    fn ids_are_unique_and_increasing() {
        let d = Document::new();
        let (d, a) = d.with_new_id();
        let (_, b) = d.with_new_id();
        assert!(b > a);
    }

    #[test]
    fn versions_share_unchanged_data() {
        let (d1, s) = ground(&Document::new());
        let d2 = d1.with_storey(Storey {
            id: s,
            name: "Ισόγειο".into(),
            elevation: Length::ZERO,
            height: Length::m(3.0),
        });
        assert!(d1.shares_elements_with(&d2));
        assert!(d1.storey(s).map(|x| x.height) != d2.storey(s).map(|x| x.height));
    }

    #[test]
    fn walls_are_filtered_by_storey() {
        let (d, s) = ground(&Document::new());
        let (d, w) = d.with_new_id();
        let d = d.with_element(Element::Wall(Wall {
            id: w,
            storey: s,
            start: Point2::new(0.0, 0.0),
            end: Point2::new(5.0, 0.0),
            reference: ReferenceLine::Centre,
            thickness: Length::cm(25.0),
            height: Length::m(3.0),
        }));
        assert_eq!(d.walls_on(s).count(), 1);
        assert_eq!(d.walls_on(Id(999)).count(), 0);
    }
}
