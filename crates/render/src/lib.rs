//! GPU rendering of the 3D model viewport (wgpu).
//!
//! Layer L8. See `docs/architecture.md` and ADR-0007.
//!
//! The crate is independent of any user-interface toolkit. It renders into its own offscreen texture, which the
//! interface shows like any other image; with egui the texture is registered through `egui_wgpu`.
//!
//! # Pieces
//!
//! - [`Scene`] / [`SceneMesh`]: coloured triangle meshes grouped by object id, stored relative to a scene origin
//!   so that projected survey coordinates keep full precision.
//! - [`Renderer`]: GPU copy of the scene (batched into a few large buffers, updated incrementally per object),
//!   display state (selection, hover, hidden, ghosted), offscreen targets and picking.
//! - [`OrbitCamera`]: navigation (orbit, pan, zoom to cursor, fit, standard views), projection matrices, rays.
//! - [`ViewParams`] / [`DisplayStyle`]: per-frame camera, size, style and clip planes (section box).
//! - [`headless`]: device creation without a window and texture readback, for tests and batch export.
//!
//! # Look
//!
//! A calm, drawing-like model view: a light background gradient, material colours under soft sky/ground light
//! and a sun with shadows, screen-space ambient occlusion in corners, crisp dark feature edges, silhouettes of
//! curved surfaces, sorted transparent glass, and a subtle ground grid.
//!
//! # Integration with egui (eframe 0.36, egui-wgpu 0.36)
//!
//! ```ignore
//! // Once, at start-up (cc: &eframe::CreationContext):
//! let rs = cc.wgpu_render_state.as_ref().expect("wgpu backend");
//! let mut renderer = kanavos_render::Renderer::new(&rs.device, Default::default())?;
//! renderer.set_scene(&scene);
//!
//! // Every frame, inside `App::ui` (frame: &mut eframe::Frame):
//! let rs = frame.wgpu_render_state().expect("wgpu backend");
//! let rect = ui.available_rect_before_wrap();
//! let ppp = ui.ctx().pixels_per_point();
//! let size = [(rect.width() * ppp) as u32, (rect.height() * ppp) as u32];
//! let params = kanavos_render::ViewParams::new(camera, size, ppp);
//! let out = renderer.render(&rs.device, &rs.queue, &params);
//! let mut egui_renderer = rs.renderer.write();
//! let id = match texture {
//!     Some((id, generation)) if generation == out.generation => id,
//!     Some((id, _)) => {
//!         egui_renderer.update_egui_texture_from_wgpu_texture(&rs.device, &out.view, wgpu::FilterMode::Nearest, id);
//!         id
//!     }
//!     None => egui_renderer.register_native_texture(&rs.device, &out.view, wgpu::FilterMode::Nearest),
//! };
//! texture = Some((id, out.generation));
//! ui.painter().image(id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
//! ```
//!
//! Picking: call [`Renderer::request_pick`] with the cursor in physical pixels relative to the viewport's
//! top-left corner, then collect results with [`Renderer::poll_picks`] on later frames (or use the blocking
//! [`Renderer::pick`] on click).

#![forbid(unsafe_code)]

mod camera;
pub mod demo;
mod gpu;
pub mod headless;
mod math;
mod pick;
mod prep;
mod renderer;
mod scene;
mod store;
mod view;

pub use camera::{
    CameraBasis, DEFAULT_FOV_Y, DepthRange, GizmoAxis, MAX_DISTANCE, MIN_DISTANCE, ORBIT_RADIANS_PER_PIXEL,
    OrbitCamera, Projection, ScreenPoint, StandardView, Viewport,
};
pub use headless::{HeadlessGpu, Image};
pub use math::{Mat4, Ray, wrap_angle};
pub use pick::{PickResult, PickTicket};
pub use renderer::{RenderOutput, RenderStats, Renderer, RendererConfig, SceneUpdate};
pub use scene::{Scene, SceneMesh, suggest_origin};
pub use view::{DisplayStyle, MAX_CLIP_PLANES, Sun, ViewParams, section_box_planes};

/// The `wgpu` version this crate is built against, re-exported so that callers use the same types.
pub use wgpu;

/// Errors from rendering, picking and readback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// No GPU adapter (not even a software one) is available.
    NoAdapter(String),
    /// The device reported an error or could not be created.
    Gpu(String),
    /// Too many picks are in flight; try again next frame.
    PickBusy,
    /// The GPU did not answer in time.
    Timeout,
    /// Reading data back from the GPU failed.
    Readback(String),
    /// An argument was out of range.
    InvalidInput(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::NoAdapter(why) => write!(f, "no graphics adapter available: {why}"),
            RenderError::Gpu(why) => write!(f, "graphics device error: {why}"),
            RenderError::PickBusy => write!(f, "too many picks in flight"),
            RenderError::Timeout => write!(f, "the graphics device did not respond in time"),
            RenderError::Readback(why) => write!(f, "could not read back from the graphics device: {why}"),
            RenderError::InvalidInput(why) => write!(f, "invalid input: {why}"),
        }
    }
}

impl std::error::Error for RenderError {}
