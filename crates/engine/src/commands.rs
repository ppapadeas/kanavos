//! Built-in commands. Each command's `name()` is part of the public control-protocol API.

use kanavos_geom::Point2;
use kanavos_model::{Document, Element, Id, ReferenceLine, Storey, Wall};
use kanavos_units::Length;

use crate::{Command, CommandError, Outcome};

/// `storey.create` — adds a storey.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateStorey {
    /// Display name.
    pub name: String,
    /// Finished floor elevation above project zero.
    pub elevation: Length,
    /// Floor-to-floor height.
    pub height: Length,
}

impl Command for CreateStorey {
    fn name(&self) -> &'static str {
        "storey.create"
    }

    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        if self.name.trim().is_empty() {
            return Err(CommandError::InvalidArgument { field: "name", reason: "must not be empty".into() });
        }
        if !(self.height.is_finite() && self.height.as_m() > 0.0) {
            return Err(CommandError::InvalidArgument {
                field: "height",
                reason: "must be a positive length".into(),
            });
        }
        if !self.elevation.is_finite() {
            return Err(CommandError::InvalidArgument {
                field: "elevation",
                reason: "must be finite".into(),
            });
        }
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_storey(Storey {
            id,
            name: self.name.trim().to_owned(),
            elevation: self.elevation,
            height: self.height,
        });
        Ok((doc, Outcome { created: vec![id] }))
    }
}

/// `wall.create` — adds a straight wall on a storey.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateWall {
    /// Storey to place the wall on.
    pub storey: Id,
    /// Start of the reference line.
    pub start: Point2,
    /// End of the reference line.
    pub end: Point2,
    /// Reference line position.
    pub reference: ReferenceLine,
    /// Total thickness.
    pub thickness: Length,
    /// Height from the storey floor; `None` uses the storey height.
    pub height: Option<Length>,
}

impl CreateWall {
    /// A centred wall at the storey's full height.
    pub fn centred(storey: Id, start: Point2, end: Point2, thickness: Length) -> Self {
        CreateWall { storey, start, end, reference: ReferenceLine::Centre, thickness, height: None }
    }
}

impl Command for CreateWall {
    fn name(&self) -> &'static str {
        "wall.create"
    }

    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let storey = doc.storey(self.storey).ok_or(CommandError::NotFound(self.storey))?;
        let tol = doc.tolerance();
        if self.start.coincides(self.end, &tol) {
            return Err(CommandError::InvalidArgument {
                field: "end",
                reason: "start and end are the same point".into(),
            });
        }
        if tol.is_zero(self.thickness) || self.thickness.as_m() < 0.0 || !self.thickness.is_finite() {
            return Err(CommandError::InvalidArgument {
                field: "thickness",
                reason: "must be a positive length".into(),
            });
        }
        let height = self.height.unwrap_or(storey.height);
        if tol.is_zero(height) || height.as_m() < 0.0 || !height.is_finite() {
            return Err(CommandError::InvalidArgument {
                field: "height",
                reason: "must be a positive length".into(),
            });
        }
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_element(Element::Wall(Wall {
            id,
            storey: self.storey,
            start: self.start,
            end: self.end,
            reference: self.reference,
            thickness: self.thickness,
            height,
        }));
        Ok((doc, Outcome { created: vec![id] }))
    }
}

/// `element.delete` — removes an element.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteElement {
    /// Element to remove.
    pub id: Id,
}

impl Command for DeleteElement {
    fn name(&self) -> &'static str {
        "element.delete"
    }

    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let doc = doc.without_element(self.id).ok_or(CommandError::NotFound(self.id))?;
        Ok((doc, Outcome::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_length_walls() {
        let (doc, out) = CreateStorey { name: "0".into(), elevation: Length::ZERO, height: Length::m(3.0) }
            .apply(&Document::new())
            .unwrap();
        let p = Point2::new(1.0, 1.0);
        let err = CreateWall::centred(out.created[0], p, p, Length::cm(20.0)).apply(&doc).unwrap_err();
        assert!(matches!(err, CommandError::InvalidArgument { field: "end", .. }));
    }

    #[test]
    fn wall_defaults_to_storey_height() {
        let (doc, out) = CreateStorey { name: "0".into(), elevation: Length::ZERO, height: Length::m(3.2) }
            .apply(&Document::new())
            .unwrap();
        let st = out.created[0];
        let (doc, out) =
            CreateWall::centred(st, Point2::new(0.0, 0.0), Point2::new(4.0, 0.0), Length::cm(25.0))
                .apply(&doc)
                .unwrap();
        match doc.element(out.created[0]) {
            Some(Element::Wall(w)) => assert!(doc.tolerance().equal(w.height, Length::m(3.2))),
            other => panic!("expected a wall, got {other:?}"),
        }
    }

    #[test]
    fn delete_missing_element_is_an_error() {
        assert_eq!(
            DeleteElement { id: Id(7) }.apply(&Document::new()).unwrap_err(),
            CommandError::NotFound(Id(7))
        );
    }
}
