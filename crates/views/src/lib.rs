//! Derivation of plans, sections, elevations and 3D views.
//!
//! Layer L5. See `docs/architecture.md` and the specs in `specs/views/`.
//!
//! Every function here is a pure function of the document: drawings are derived, never drawn twice.
//!
//! Drawing layer names are `<category>.<part>` (e.g. `wall.cut`, `door.symbol`, `space.stamp`) so that views can
//! hide parts with [`kanavos_model::View::hidden_layers`] prefixes and DXF exports get meaningful layers. Parts
//! used: `cut` (heavy outline of cut areas), `layer` (cut fills and material boundaries), `below`, `above`
//! (dashed), `beyond` (section and elevation lines), `edge` (3D lines), `symbol`, `stamp`, `line`, `label`,
//! `break`, `marker`, `text`.
//!
//! Geometry this crate needs before the shared crates offer it lives behind small local modules so it can be
//! switched over later: polygon booleans (`poly`, over `i_overlay`), the mesh–plane section (`meshops`) and door
//! and window holes in wall solids (`holes`).

mod annot;
mod autodim;
mod cut;
mod hlr;
mod meshops;
mod plan;
mod poly;
mod project;
mod scene;
mod section;
mod style;
mod threed;

use std::collections::BTreeSet;

use kanavos_elements::GeometryError;
use kanavos_i18n::{t, tf};
use kanavos_model::{Document, Id, ViewKind};
use kanavos_units::Locale;

pub use autodim::auto_dimensions;
pub use plan::{PlanOptions, plan};
pub use scene::{
    CacheStats, Scene, SceneCache, SceneItem, SceneOptions, box_planes, clip_mesh, clip_scene_to_box,
    element_scene_items, scene,
};
pub use section::{SectionFrame, SectionOptions, section, section_frame, section_with};
pub use style::Pens;
pub use threed::{ThreeDOptions, camera_drawing, three_d};

/// Why a view could not be derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    /// The view or storey does not exist.
    NotFound(Id),
    /// The view is of another kind than the function handles.
    WrongKind(Id),
    /// An element's geometry failed (the view is still produced; this is for callers that need strictness).
    Geometry(GeometryError),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::NotFound(id) => write!(f, "view or storey {id} not found"),
            ViewError::WrongKind(id) => write!(f, "view {id} cannot be drawn by this function"),
            ViewError::Geometry(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ViewError {}

/// Model-space drawing of a plan, section, elevation or 3D view, honouring the view's scale and hidden layers.
/// 3D views come back as hidden-line drawings in projection-plane metres. Other view kinds (schedules, site and
/// regulation views) are drawn elsewhere and return [`ViewError::WrongKind`].
pub fn view_drawing(doc: &Document, view: Id, locale: Locale) -> Result<kanavos_drawing::Drawing, ViewError> {
    let v = doc.view(view).ok_or(ViewError::NotFound(view))?;
    let mut d = match &v.kind {
        ViewKind::Plan { storey } => plan(
            doc,
            *storey,
            &PlanOptions {
                scale: f64::from(v.scale),
                locale,
                hidden_layers: v.hidden_layers.clone(),
                ..PlanOptions::default()
            },
        )?,
        ViewKind::Section { .. } | ViewKind::Elevation { .. } => section(doc, view, locale)?,
        ViewKind::ThreeD { .. } => {
            three_d(doc, view, &ThreeDOptions { locale, ..ThreeDOptions::default() }, None)?
        }
        _ => return Err(ViewError::WrongKind(view)),
    };
    filter_layers(&mut d, &v.hidden_layers);
    Ok(d)
}

/// Removes items whose layer starts with any of the hidden prefixes.
pub fn filter_layers(d: &mut kanavos_drawing::Drawing, hidden: &BTreeSet<String>) {
    if hidden.is_empty() {
        return;
    }
    d.items.retain(|i| !hidden.iter().any(|h| i.layer.starts_with(h.as_str())));
}

/// A generated title for a view, e.g. "Τομή Α-Α", "Όψη Β" or "Κάτοψη Ισογείου"; `None` for kinds without one.
pub fn view_title(doc: &Document, view: Id, locale: Locale) -> Option<String> {
    let v = doc.view(view)?;
    Some(match &v.kind {
        ViewKind::Section { marker, .. } => tf(locale, "views.title.section", &[("m", marker)]),
        ViewKind::Elevation { marker, .. } => tf(locale, "views.title.elevation", &[("m", marker)]),
        ViewKind::Plan { storey } => tf(
            locale,
            "views.title.plan",
            &[("storey", &doc.storey(*storey).map(|s| s.name.clone()).unwrap_or_default())],
        ),
        ViewKind::ThreeD { camera } => {
            t(locale, if camera.parallel { "views.title.axonometric" } else { "views.title.perspective" })
                .into_owned()
        }
        _ => return None,
    })
}
