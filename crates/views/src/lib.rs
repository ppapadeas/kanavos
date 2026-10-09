//! Derivation of plans, sections, elevations and 3D views.
//!
//! Layer L5. See `docs/architecture.md`.
//!
//! Every function here is a pure function of the document: drawings are derived, never drawn twice.
//!
//! Drawing layer names are `<category>.<part>` (e.g. `wall.cut`, `door.symbol`, `space.stamp`) so that views can
//! hide parts with [`kanavos_model::View::hidden_layers`] prefixes and DXF exports get meaningful layers.

mod plan;
mod scene;
mod section;

use std::collections::BTreeSet;

use kanavos_elements::GeometryError;
use kanavos_model::{Document, Id, ViewKind};
use kanavos_units::Locale;

pub use plan::{PlanOptions, plan};
pub use scene::{Scene, SceneItem, SceneOptions, element_scene_items, scene};
pub use section::{SectionFrame, section, section_frame};

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

/// Model-space drawing of a plan, section or elevation view, honouring the view's scale and hidden layers.
/// Other view kinds (3D, schedules, site and regulation views) are drawn elsewhere and return
/// [`ViewError::WrongKind`].
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
