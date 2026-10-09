//! GPU rendering of views and viewports.
//!
//! Layer L8. See `docs/architecture.md`.
//!
//! The viewport camera comes first: navigation (orbit, pan, zoom to cursor, fit, standard views), reverse-Z
//! projection matrices and picking rays, as pure `f64` maths that the GPU renderer and the interface share.

mod camera;
mod math;

pub use camera::{
    CameraBasis, DEFAULT_FOV_Y, DepthRange, GizmoAxis, MAX_DISTANCE, MIN_DISTANCE, ORBIT_RADIANS_PER_PIXEL,
    OrbitCamera, Projection, ScreenPoint, StandardView, Viewport,
};
pub use math::{Mat4, Ray, wrap_angle};
