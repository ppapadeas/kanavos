//! 3D part builder and the colour palette of objects.

use kanavos_drawing::Rgba;
use kanavos_geom::Point3;
use kanavos_solid::{Mesh, cuboid};

use crate::Part;

/// Colours of 3D parts: neutral, light materials so that models read well in shaded views.
pub(crate) mod color {
    use kanavos_drawing::Rgba;

    /// Painted or aluminium frames.
    pub(crate) const FRAME: Rgba = Rgba::rgb(235, 235, 232);
    /// Timber door leaves.
    pub(crate) const LEAF: Rgba = Rgba::rgb(160, 120, 80);
    /// Glass.
    pub(crate) const GLASS: Rgba = Rgba::new(160, 200, 225, 110);
    /// Satin metal (handles, rails).
    pub(crate) const METAL: Rgba = Rgba::rgb(168, 170, 174);
    /// Dark details (plinths, legs, screens).
    pub(crate) const DARK: Rgba = Rgba::rgb(62, 64, 68);
    /// Marble sills and thresholds.
    pub(crate) const MARBLE: Rgba = Rgba::rgb(226, 222, 214);
    /// External shutters.
    pub(crate) const SHUTTER: Rgba = Rgba::rgb(120, 140, 122);
    /// White lacquer and appliances.
    pub(crate) const WHITE: Rgba = Rgba::rgb(243, 243, 241);
    /// Light timber.
    pub(crate) const OAK: Rgba = Rgba::rgb(196, 166, 126);
}

/// Collects coloured meshes, one part per colour.
#[derive(Debug, Clone, Default)]
pub(crate) struct Parts {
    parts: Vec<Part>,
}

fn ok3(v: [f64; 3]) -> bool {
    v.iter().all(|c| c.is_finite())
}

impl Parts {
    /// No parts.
    pub(crate) fn new() -> Parts {
        Parts::default()
    }

    /// Adds a mesh, merging it into the part of the same colour.
    pub(crate) fn add(&mut self, mesh: Mesh, color: Rgba) {
        if mesh.is_empty() || !mesh.positions.iter().all(|p| ok3(p.to_array())) {
            return;
        }
        match self.parts.iter_mut().find(|p| p.color == color) {
            Some(part) => part.mesh.append(&mesh),
            None => self.parts.push(Part { mesh, color }),
        }
    }

    /// An axis-aligned box between two corners; skipped when thinner than 0.1 mm.
    pub(crate) fn bx(&mut self, a: [f64; 3], b: [f64; 3], color: Rgba) {
        if !ok3(a) || !ok3(b) {
            return;
        }
        if a.iter().zip(b.iter()).any(|(p, q)| (p - q).abs() < 1e-4) {
            return;
        }
        self.add(cuboid(Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2])), color);
    }

    /// The parts.
    pub(crate) fn into_parts(self) -> Vec<Part> {
        self.parts
    }
}
