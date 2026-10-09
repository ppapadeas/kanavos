//! Duplicating storeys, materials, build-ups and views.

use kanavos_i18n::tf;
use kanavos_model::{BuildUp, Document, Element, Id, Material, Storey, View, ViewKind};
use kanavos_units::{Length, Locale};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CopyToStorey, non_empty};
use crate::{Command, CommandError, Outcome};

fn yes() -> bool {
    true
}

/// The given name, or "<name> - αντίγραφο" / "<name> - copy".
fn copy_name(given: &Option<String>, original: &str, locale: Locale) -> Result<String, CommandError> {
    match given {
        Some(n) => {
            non_empty(n, "name")?;
            Ok(n.trim().to_owned())
        }
        None => Ok(tf(locale, "engine.copy_name", &[("name", original)])),
    }
}

// ----- storey.duplicate ----------------------------------------------------------------------------------------

/// `storey.duplicate` — copies a storey with all its elements and its plan view. By default the copy goes
/// directly above the original and the storeys above move up by the original's height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuplicateStorey {
    /// Storey to copy.
    pub id: Id,
    /// Name of the copy; default "<name> - αντίγραφο".
    #[serde(default)]
    pub name: Option<String>,
    /// Floor elevation of the copy; default: the top of the original.
    #[serde(default)]
    pub elevation: Option<Length>,
    /// Move the storeys at or above the copy's elevation up by its height (default true).
    #[serde(default = "yes")]
    pub shift_above: bool,
    /// Language of default names.
    #[serde(default)]
    pub locale: Locale,
}

impl Command for DuplicateStorey {
    fn name(&self) -> &'static str {
        "storey.duplicate"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let src = doc.storey(self.id).ok_or(CommandError::NotFound(self.id))?.clone();
        let elevation = self.elevation.unwrap_or(src.top());
        if !elevation.is_finite() {
            return Err(CommandError::InvalidArgument {
                field: "elevation",
                reason: "must be finite".into(),
            });
        }
        let name = copy_name(&self.name, &src.name, self.locale)?;
        let tol = doc.tolerance();
        let mut next = doc.clone();
        let mut out = Outcome::default();
        if self.shift_above {
            for s in doc.storeys() {
                if s.id != src.id && s.elevation.as_m() >= elevation.as_m() - tol.length.as_m() {
                    let mut moved = s.clone();
                    moved.elevation += src.height;
                    next = next.with_storey(moved);
                    out.changed.push(s.id);
                }
            }
        }
        let (d, sid) = next.with_new_id();
        next = d.with_storey(Storey { id: sid, name: name.clone(), elevation, ..src.clone() });
        let (d, vid) = next.with_new_id();
        let plan = doc.plan_view_of(src.id);
        next = d.with_view(View {
            id: vid,
            name: tf(self.locale, "engine.plan_name", &[("name", &name)]),
            kind: ViewKind::Plan { storey: sid },
            scale: plan.map(|v| v.scale).unwrap_or(50),
            hidden_layers: plan.map(|v| v.hidden_layers.clone()).unwrap_or_default(),
        });
        out.created.push(sid);
        out.created.push(vid);
        let ids: Vec<Id> = doc.elements_on(src.id).map(|e| e.id).collect();
        if !ids.is_empty() {
            let (d, o) = CopyToStorey { ids, storey: sid }.apply(&next)?;
            next = d;
            out.created.extend(o.created);
        }
        Ok((next, out))
    }
}

// ----- material.duplicate / build_up.duplicate -----------------------------------------------------------------

/// `material.duplicate` — copies a material under a new name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuplicateMaterial {
    /// Material to copy.
    pub id: Id,
    /// Name of the copy; default "<name> - αντίγραφο".
    #[serde(default)]
    pub name: Option<String>,
    /// Language of the default name.
    #[serde(default)]
    pub locale: Locale,
}

impl Command for DuplicateMaterial {
    fn name(&self) -> &'static str {
        "material.duplicate"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let src = doc.material(self.id).ok_or(CommandError::NotFound(self.id))?;
        let name = copy_name(&self.name, &src.name, self.locale)?;
        let (next, id) = doc.with_new_id();
        let next = next.with_material(Material { id, name, ..src.clone() });
        Ok((next, Outcome { created: vec![id], ..Outcome::default() }))
    }
}

/// `build_up.duplicate` — copies a build-up under a new name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuplicateBuildUp {
    /// Build-up to copy.
    pub id: Id,
    /// Name of the copy; default "<name> - αντίγραφο".
    #[serde(default)]
    pub name: Option<String>,
    /// Language of the default name.
    #[serde(default)]
    pub locale: Locale,
}

impl Command for DuplicateBuildUp {
    fn name(&self) -> &'static str {
        "build_up.duplicate"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let src = doc.build_up(self.id).ok_or(CommandError::NotFound(self.id))?;
        let name = copy_name(&self.name, &src.name, self.locale)?;
        let (next, id) = doc.with_new_id();
        let next = next.with_build_up(BuildUp { id, name, ..src.clone() });
        Ok((next, Outcome { created: vec![id], ..Outcome::default() }))
    }
}

// ----- view.duplicate ------------------------------------------------------------------------------------------

/// `view.duplicate` — copies a view (with its hidden layers and scale) and, by default, the annotation drawn in
/// it. A copied plan view shows the same storey.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuplicateView {
    /// View to copy.
    pub id: Id,
    /// Name of the copy; default "<name> - αντίγραφο".
    #[serde(default)]
    pub name: Option<String>,
    /// Copy the dimensions, text and level marks owned by a section or elevation view (default true).
    #[serde(default = "yes")]
    pub annotations: bool,
    /// Language of the default name.
    #[serde(default)]
    pub locale: Locale,
}

impl Command for DuplicateView {
    fn name(&self) -> &'static str {
        "view.duplicate"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let src = doc.view(self.id).ok_or(CommandError::NotFound(self.id))?;
        let name = copy_name(&self.name, &src.name, self.locale)?;
        let (mut next, vid) = doc.with_new_id();
        next = next.with_view(View { id: vid, name, ..src.clone() });
        let mut out = Outcome { created: vec![vid], ..Outcome::default() };
        if self.annotations {
            let owned: Vec<Element> = doc.elements_in_view(self.id).cloned().collect();
            for e in owned {
                let (d, nid) = next.with_new_id();
                next = d.with_element(Element { id: nid, view: Some(vid), ..e });
                out.created.push(nid);
            }
        }
        Ok((next, out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use crate::commands::{CreateElement, CreateView, DeleteView};
    use kanavos_geom::Point2;
    use kanavos_model::{
        ElementKind, LevelMark, Opening, OpeningKind, ReferenceLine, Structure, Wall, starter,
    };

    #[test]
    fn duplicating_a_storey_copies_elements_and_shifts_storeys_above() {
        let mut s = Session::new(starter());
        let storeys: Vec<Id> = s.document().storeys().iter().map(|s| s.id).collect();
        let (ground, first) = (storeys[0], storeys[1]);
        let bu = s.document().defaults().exterior_wall.unwrap();
        let w = s
            .run(CreateElement::on(
                ground,
                ElementKind::Wall(Wall {
                    start: Point2::new(0.0, 0.0),
                    end: Point2::new(5.0, 0.0),
                    bulge: 0.0,
                    reference: ReferenceLine::Centre,
                    structure: Structure::Layered { build_up: bu },
                    height: Length::m(3.0),
                    base_offset: Length::ZERO,
                }),
            ))
            .unwrap()
            .created[0];
        s.run(CreateElement::on(
            ground,
            ElementKind::Opening(Opening {
                host: w,
                kind: OpeningKind::Window,
                style: "window.casement_2".into(),
                offset: Length::m(2.5),
                width: Length::m(1.2),
                height: Length::m(1.4),
                sill: Length::m(0.9),
                flip_side: false,
                flip_hand: false,
                params: Default::default(),
            }),
        ))
        .unwrap();
        let out = s
            .run(DuplicateStorey {
                id: ground,
                name: None,
                elevation: None,
                shift_above: true,
                locale: Locale::El,
            })
            .unwrap();
        let doc = s.document();
        let copy = out.created[0];
        assert_eq!(doc.storey(copy).unwrap().name, "Ισόγειο - αντίγραφο");
        assert!(doc.tolerance().equal(doc.storey(copy).unwrap().elevation, Length::m(3.2)));
        assert!(doc.tolerance().equal(doc.storey(first).unwrap().elevation, Length::m(6.4)));
        assert_eq!(doc.elements_on(copy).count(), 2);
        assert!(doc.plan_view_of(copy).is_some());
        doc.validate().unwrap();
    }

    #[test]
    fn duplicated_views_bring_their_annotation_and_can_be_deleted() {
        let mut s = Session::new(starter());
        let v = s
            .run(CreateView {
                name: "Τομή Α-Α".into(),
                kind: ViewKind::Section {
                    start: Point2::new(0.0, 0.0),
                    end: Point2::new(10.0, 0.0),
                    depth: Length::m(20.0),
                    flip: false,
                    marker: "Α".into(),
                },
                scale: 100,
            })
            .unwrap()
            .created[0];
        s.run(CreateElement {
            storey: None,
            view: Some(v),
            kind: ElementKind::Level(LevelMark {
                position: Point2::new(-1.0, 3.2),
                value: None,
                label: String::new(),
            }),
            tag: String::new(),
            phase: Default::default(),
            properties: Default::default(),
        })
        .unwrap();
        let out = s.run(DuplicateView { id: v, name: None, annotations: true, locale: Locale::En }).unwrap();
        assert_eq!(out.created.len(), 2);
        assert_eq!(s.document().view(out.created[0]).unwrap().name, "Τομή Α-Α - copy");
        // A duplicated plan view can be deleted again; the original cannot.
        let plan = s.document().views().find(|v| matches!(v.kind, ViewKind::Plan { .. })).unwrap().id;
        let dup =
            s.run(DuplicateView { id: plan, name: None, annotations: true, locale: Locale::El }).unwrap();
        s.run(DeleteView { id: dup.created[0] }).unwrap();
        assert!(s.run(DeleteView { id: plan }).is_err());
    }

    #[test]
    fn duplicating_materials_and_build_ups_renames_them() {
        let mut s = Session::new(starter());
        let m = s.document().materials().next().unwrap().id;
        let out = s.run(DuplicateMaterial {
            id: m, name: Some("Σκυρόδεμα C30".into()), locale: Locale::El
        });
        let id = out.unwrap().created[0];
        assert_eq!(s.document().material(id).unwrap().name, "Σκυρόδεμα C30");
        let b = s.document().build_ups().next().unwrap().id;
        let id = s.run(DuplicateBuildUp { id: b, name: None, locale: Locale::En }).unwrap().created[0];
        assert!(s.document().build_up(id).unwrap().name.ends_with(" - copy"));
        assert!(s.run(DuplicateMaterial { id: m, name: Some("  ".into()), locale: Locale::El }).is_err());
    }
}
