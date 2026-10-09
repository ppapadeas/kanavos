//! Placing, moving and removing views on sheets.

use kanavos_geom::Point2;
use kanavos_model::{Document, Id, Sheet, ViewKind, Viewport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Command, CommandError, Outcome};

fn invalid(field: &'static str, reason: impl Into<String>) -> CommandError {
    CommandError::InvalidArgument { field, reason: reason.into() }
}

fn sheet(doc: &Document, id: Id) -> Result<Sheet, CommandError> {
    doc.sheet(id).cloned().ok_or(CommandError::NotFound(id))
}

fn check_centre(c: [f64; 2]) -> Result<(), CommandError> {
    if c.iter().all(|v| v.is_finite()) { Ok(()) } else { Err(invalid("centre", "must be finite")) }
}

/// Finds a viewport by index or by the view it shows (the first one showing it).
fn viewport_index(s: &Sheet, index: Option<usize>, view: Option<Id>) -> Result<usize, CommandError> {
    match (index, view) {
        (Some(i), _) if i < s.viewports.len() => Ok(i),
        (Some(_), _) => Err(invalid("index", "is beyond the sheet's viewports")),
        (None, Some(v)) => s
            .viewports
            .iter()
            .position(|vp| vp.view == v)
            .ok_or_else(|| invalid("view", "is not on the sheet")),
        (None, None) => Err(invalid("index", "give the viewport index or its view")),
    }
}

/// `sheet.place_view` — places a view on a sheet as a new viewport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlaceView {
    /// Sheet.
    pub sheet: Id,
    /// View to place (any 2D view; 3D views cannot be placed yet).
    pub view: Id,
    /// Centre on the sheet in millimetres from the lower-left corner; default: the middle of the drawing area.
    #[serde(default)]
    pub centre: Option<[f64; 2]>,
    /// Scale override; default: the view's scale.
    #[serde(default)]
    pub scale: Option<u32>,
    /// Title under the viewport; default: the view name.
    #[serde(default)]
    pub title: Option<String>,
    /// Model-space crop rectangle (two opposite corners).
    #[serde(default)]
    pub crop: Option<[Point2; 2]>,
}

impl Command for PlaceView {
    fn name(&self) -> &'static str {
        "sheet.place_view"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let mut s = sheet(doc, self.sheet)?;
        let v = doc.view(self.view).ok_or(CommandError::NotFound(self.view))?;
        if matches!(v.kind, ViewKind::ThreeD { .. }) {
            return Err(invalid("view", "3D views cannot be placed on sheets yet"));
        }
        if self.scale == Some(0) {
            return Err(invalid("scale", "must be at least 1"));
        }
        let (w, h) = s.paper.dimensions_mm();
        // Middle of the frame (20 mm binding margin on the left, 10 mm elsewhere), above the title block band.
        let centre = self.centre.unwrap_or([(20.0 + w - 10.0) / 2.0, (10.0 + h - 10.0) / 2.0 + 15.0]);
        check_centre(centre)?;
        s.viewports.push(Viewport {
            view: self.view,
            centre,
            scale: self.scale,
            title: self.title.clone(),
            crop: self.crop,
        });
        Ok((doc.with_sheet(s), Outcome { changed: vec![self.sheet], ..Outcome::default() }))
    }
}

/// `sheet.move_viewport` — moves a viewport on its sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoveViewport {
    /// Sheet.
    pub sheet: Id,
    /// Viewport index on the sheet (0-based).
    #[serde(default)]
    pub index: Option<usize>,
    /// Or: the view the viewport shows (the first viewport showing it).
    #[serde(default)]
    pub view: Option<Id>,
    /// New centre in millimetres from the sheet's lower-left corner.
    pub centre: [f64; 2],
}

impl Command for MoveViewport {
    fn name(&self) -> &'static str {
        "sheet.move_viewport"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let mut s = sheet(doc, self.sheet)?;
        check_centre(self.centre)?;
        let i = viewport_index(&s, self.index, self.view)?;
        if let Some(vp) = s.viewports.get_mut(i) {
            vp.centre = self.centre;
        }
        Ok((doc.with_sheet(s), Outcome { changed: vec![self.sheet], ..Outcome::default() }))
    }
}

/// `sheet.remove_viewport` — removes a viewport from its sheet (the view itself stays).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RemoveViewport {
    /// Sheet.
    pub sheet: Id,
    /// Viewport index on the sheet (0-based).
    #[serde(default)]
    pub index: Option<usize>,
    /// Or: the view the viewport shows (the first viewport showing it).
    #[serde(default)]
    pub view: Option<Id>,
}

impl Command for RemoveViewport {
    fn name(&self) -> &'static str {
        "sheet.remove_viewport"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let mut s = sheet(doc, self.sheet)?;
        let i = viewport_index(&s, self.index, self.view)?;
        if i < s.viewports.len() {
            s.viewports.remove(i);
        }
        Ok((doc.with_sheet(s), Outcome { changed: vec![self.sheet], ..Outcome::default() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::commands::CreateSheet;
    use kanavos_model::{Paper, PaperSize, starter};

    #[test]
    fn place_move_and_remove_viewports() {
        let mut s = Session::new(starter());
        let sh = s
            .run(CreateSheet {
                number: "Α-10".into(),
                name: "Κατόψεις".into(),
                paper: Paper { size: PaperSize::A3, landscape: true },
                viewports: Vec::new(),
            })
            .unwrap()
            .created[0];
        let plan = s.document().views().find(|v| matches!(v.kind, ViewKind::Plan { .. })).unwrap().id;
        let three_d = s.document().views().find(|v| matches!(v.kind, ViewKind::ThreeD { .. })).unwrap().id;
        s.run(PlaceView { sheet: sh, view: plan, centre: None, scale: Some(100), title: None, crop: None })
            .unwrap();
        let vp = &s.document().sheet(sh).unwrap().viewports[0];
        assert!((vp.centre[0] - 215.0).abs() < 1e-9 && (vp.centre[1] - 163.5).abs() < 1e-9);
        assert!(
            s.run(PlaceView { sheet: sh, view: three_d, centre: None, scale: None, title: None, crop: None })
                .is_err()
        );
        s.run(MoveViewport { sheet: sh, index: None, view: Some(plan), centre: [100.0, 120.0] }).unwrap();
        let c = s.document().sheet(sh).unwrap().viewports[0].centre;
        assert!((c[0] - 100.0).abs() < 1e-9 && (c[1] - 120.0).abs() < 1e-9);
        assert!(s.run(MoveViewport { sheet: sh, index: Some(3), view: None, centre: [1.0, 1.0] }).is_err());
        s.run(RemoveViewport { sheet: sh, index: Some(0), view: None }).unwrap();
        assert!(s.document().sheet(sh).unwrap().viewports.is_empty());
    }
}
