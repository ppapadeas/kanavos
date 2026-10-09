//! 3D views as line drawings for sheets (see `specs/views/three-d-line-views.md`).
//!
//! The model is projected through the view's camera, in parallel (axonometric) or in perspective, and drawn
//! with hidden lines removed. Coordinates are metres on the projection plane: for parallel cameras the plane
//! through the target; for perspective cameras the plane through the target perpendicular to the viewing
//! direction, so objects at the target appear at true size and the view's scale applies on sheets.

use std::collections::{BTreeMap, BTreeSet};

use kanavos_drawing::{Drawing, Item, Stroke, pt};
use kanavos_model::{Camera, Document, Id, Phase, ViewKind};
use kanavos_units::Locale;

use crate::meshops::{Eye, V3, cross, dot, normalize, sub};
use crate::project::{ProjItem, Projection, is_profile, merge_collinear, project, visible};
use crate::style::{Pens, phase_stroke};
use crate::{Scene, SceneItem, SceneOptions, ViewError, scene};

/// Options for 3D line drawings.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDOptions {
    /// Scale denominator; `None` uses the view's scale.
    pub scale: Option<f64>,
    /// Language of generated text.
    pub locale: Locale,
    /// Hidden layer prefixes (in addition to the view's own).
    pub hidden_layers: BTreeSet<String>,
    /// Edges between faces meeting at more than this angle (degrees) are drawn.
    pub crease_angle_deg: f64,
    /// Draw the outer contour heavier.
    pub profile_lines: bool,
}

impl Default for ThreeDOptions {
    fn default() -> Self {
        ThreeDOptions {
            scale: None,
            locale: Locale::El,
            hidden_layers: BTreeSet::new(),
            crease_angle_deg: 15.0,
            profile_lines: true,
        }
    }
}

/// Camera basis: right, up and forward unit vectors.
fn basis(c: &Camera) -> (V3, V3, V3) {
    let eye = c.eye.to_array();
    let target = c.target.to_array();
    let f = normalize(sub(target, eye)).unwrap_or([0.0, 1.0, 0.0]);
    let r = normalize(cross(f, [0.0, 0.0, 1.0])).unwrap_or([1.0, 0.0, 0.0]);
    let u = cross(r, f);
    (r, u, f)
}

/// The projection of a camera. Parallel cameras map onto the plane through the target; perspective cameras
/// clip at a near plane and map with the projective transform onto the plane at the target distance.
pub(crate) fn camera_projection(c: &Camera) -> Projection {
    let eye = c.eye.to_array();
    let target = c.target.to_array();
    let (r, u, f) = basis(c);
    let dist = crate::meshops::length(sub(target, eye)).max(1e-3);
    if c.parallel {
        Projection {
            eye: Eye::Parallel(f),
            rows: [
                [r[0], r[1], r[2], -dot(r, target)],
                [u[0], u[1], u[2], -dot(u, target)],
                [f[0], f[1], f[2], -dot(f, eye)],
            ],
            clips: Vec::new(),
            perspective: None,
        }
    } else {
        let near = (dist * 0.01).max(0.05);
        Projection {
            eye: Eye::Perspective(eye),
            rows: [
                [r[0], r[1], r[2], -dot(r, eye)],
                [u[0], u[1], u[2], -dot(u, eye)],
                [f[0], f[1], f[2], -dot(f, eye)],
            ],
            clips: vec![[0.0, 0.0, 1.0, -near]],
            perspective: Some(dist),
        }
    }
}

/// Draws a 3D view as a hidden-line drawing.
pub fn three_d(
    doc: &Document,
    view: Id,
    opts: &ThreeDOptions,
    scene_in: Option<&Scene>,
) -> Result<Drawing, ViewError> {
    let v = doc.view(view).ok_or(ViewError::NotFound(view))?;
    let ViewKind::ThreeD { camera } = &v.kind else { return Err(ViewError::WrongKind(view)) };
    let scale = opts.scale.unwrap_or(f64::from(v.scale));
    let owned;
    let sc = match scene_in {
        Some(s) => s,
        None => {
            owned = scene(doc, &SceneOptions::default());
            &owned
        }
    };
    let mut d = camera_drawing(doc, sc, camera, scale, opts);
    crate::filter_layers(&mut d, &v.hidden_layers);
    crate::filter_layers(&mut d, &opts.hidden_layers);
    Ok(d)
}

/// A hidden-line drawing of a scene through a camera, in projection-plane metres.
pub fn camera_drawing(
    doc: &Document,
    sc: &Scene,
    camera: &Camera,
    scale: f64,
    opts: &ThreeDOptions,
) -> Drawing {
    let pens = Pens::for_scale(scale);
    let mut by_element: BTreeMap<Id, Vec<&SceneItem>> = BTreeMap::new();
    for it in &sc.items {
        by_element.entry(it.element).or_default().push(it);
    }
    let items: Vec<ProjItem<'_>> = by_element
        .iter()
        .filter_map(|(id, its)| {
            let first = its.first()?;
            Some(ProjItem {
                element: *id,
                category: first.category,
                phase: doc.element(*id).map(|e| e.phase).unwrap_or(Phase::New),
                meshes: its.iter().map(|i| &i.mesh).collect(),
            })
        })
        .collect();
    let proj = camera_projection(camera);
    let projected = project(&items, &proj, opts.crease_angle_deg);
    // Depth tolerance: 1 µm along the view, mapped into the projective depth for perspective views.
    let eps = match proj.perspective {
        None => 1e-6,
        Some(dist) => {
            let zmax = sc
                .bbox()
                .map(|b| {
                    let corners = [
                        [b.min.x, b.min.y, b.min.z],
                        [b.max.x, b.max.y, b.max.z],
                        [b.min.x, b.max.y, b.min.z],
                        [b.max.x, b.min.y, b.max.z],
                    ];
                    corners.iter().map(|c| proj.pre(*c)[2]).fold(1.0_f64, f64::max)
                })
                .unwrap_or(1.0);
            (dist * 1e-6 / (zmax * zmax)).max(1e-15)
        }
    };
    let (vis, occ) = visible(&projected, &[], &[], eps);
    let vis = merge_collinear(vis);
    let mut d = Drawing::model(scale);
    let offset = scale * 0.4 / 1000.0;
    for s in &vis {
        let Some(it) = items.get(s.tag as usize) else { continue };
        let w = if opts.profile_lines && is_profile(&occ, s, offset) { pens.profile } else { pens.beyond };
        d.push(
            Item::line(
                pt(s.a[0], s.a[1]),
                pt(s.b[0], s.b[1]),
                phase_stroke(Stroke::solid(w), it.phase),
                format!("{}.edge", it.category.key()),
            )
            .with_source(it.element.0),
        );
    }
    d
}
