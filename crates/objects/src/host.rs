//! An opening in its host wall, in canonical coordinates.
//!
//! Generators draw every door and window in one canonical state: leaves and sashes open towards the inner face
//! `yi` (the larger `y`), the outer face is `ye`, and single hinges sit on the jamb at `x = −w/2`. The opening's
//! `flip_side` and `flip_hand` are applied afterwards as reflections, so every state is an exact mirror image.
//!
//! [`OpeningContext::outer_face`] decides which wall face becomes the outer face, and so where leaves open
//! (`specs/objects/orientation.md`); this module only maps that choice onto the canonical drawing.

use std::cmp::Ordering;

use kanavos_drawing::{Affine, Item};
use kanavos_geom::Point3;

use crate::parts::Parts;
use crate::sym::Sym;
use crate::{OpeningContext, Part};

/// The opening and its host wall, canonical.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Host {
    /// Width of the hole in the wall.
    pub w: f64,
    /// Height of the hole.
    pub h: f64,
    /// Inner face: the side leaves and sashes open to.
    pub yi: f64,
    /// Outer face.
    pub ye: f64,
    /// Sill height above the storey floor.
    pub sill: f64,
    reflect_y: bool,
    mirror_x: bool,
}

/// Smallest wall thickness assumed, so that symbols stay drawable in zero-thickness walls.
const MIN_THICKNESS: f64 = 0.02;

impl Host {
    /// The canonical host of an opening, or `None` when its width or height is below 5 cm or not finite.
    pub(crate) fn new(ctx: &OpeningContext<'_>) -> Option<Host> {
        let o = ctx.opening;
        let (w, h) = (o.width.as_m(), o.height.as_m());
        if !(w.is_finite() && h.is_finite() && w >= 0.05 && h >= 0.05) {
            return None;
        }
        let (outer, inner) = (ctx.outer_face(), ctx.inner_face());
        if !(outer.is_finite() && inner.is_finite()) {
            return None;
        }
        // Canonically the outer face is the smaller y, so the drawing is reflected across the wall when the outer
        // face is the larger one. When both faces coincide (a wall of no thickness), the left face counts as the
        // +y side.
        let reflect_y = match outer.partial_cmp(&inner) {
            Some(Ordering::Greater) => true,
            Some(Ordering::Less) => false,
            _ => !o.flip_side,
        };
        let (mut yi, mut ye) = (outer.max(inner), outer.min(inner));
        if yi - ye < MIN_THICKNESS {
            let mid = (yi + ye) / 2.0;
            yi = mid + MIN_THICKNESS / 2.0;
            ye = mid - MIN_THICKNESS / 2.0;
        }
        let sill = o.sill.as_m();
        Some(Host {
            w,
            h,
            yi,
            ye,
            sill: if sill.is_finite() { sill } else { 0.0 },
            reflect_y,
            mirror_x: o.flip_hand,
        })
    }

    /// Wall thickness.
    pub(crate) fn t(&self) -> f64 {
        self.yi - self.ye
    }

    /// Middle of the wall.
    pub(crate) fn mid(&self) -> f64 {
        (self.yi + self.ye) / 2.0
    }

    /// Half the width.
    pub(crate) fn w2(&self) -> f64 {
        self.w / 2.0
    }

    /// The map from canonical to local opening coordinates.
    pub(crate) fn affine(&self) -> Affine {
        Affine {
            a: if self.mirror_x { -1.0 } else { 1.0 },
            b: 0.0,
            c: 0.0,
            d: if self.reflect_y { -1.0 } else { 1.0 },
            tx: 0.0,
            ty: if self.reflect_y { self.yi + self.ye } else { 0.0 },
        }
    }

    /// The symbol in local coordinates.
    pub(crate) fn finish_symbol(&self, sym: Sym) -> Vec<Item> {
        if self.mirror_x || self.reflect_y { sym.into_mapped(&self.affine()) } else { sym.into_items() }
    }

    /// The parts in local coordinates.
    pub(crate) fn finish_parts(&self, parts: Parts) -> Vec<Part> {
        let parts = parts.into_parts();
        if !(self.mirror_x || self.reflect_y) {
            return parts;
        }
        let m = self.affine();
        let flip = self.mirror_x ^ self.reflect_y;
        parts
            .into_iter()
            .map(|p| {
                let mesh = p.mesh.map_positions(|q| Point3::new(m.a * q.x, m.d * q.y + m.ty, q.z));
                Part { mesh: if flip { mesh.flipped() } else { mesh }, color: p.color }
            })
            .collect()
    }
}

/// A hinged leaf or sash drawn open in plan: hinged at `(hx, y)`, running towards `dir · x` when closed and
/// turned by `angle` towards +y.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Swing {
    /// Hinge x.
    pub hx: f64,
    /// Hinge y (the face of the leaf on the opening side).
    pub y: f64,
    /// +1 when the closed leaf runs from the hinge towards +x, −1 towards −x.
    pub dir: f64,
    /// Leaf width.
    pub len: f64,
    /// Leaf thickness (0 draws a single line).
    pub thick: f64,
    /// Opening angle, radians.
    pub angle: f64,
}

impl Swing {
    fn rot(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let (s, c) = (self.dir * self.angle).sin_cos();
        let (dx, dy) = (x - self.hx, y - self.y);
        (self.hx + dx * c - dy * s, self.y + dx * s + dy * c)
    }

    /// Tip of the open leaf.
    pub(crate) fn tip(&self) -> (f64, f64) {
        self.rot((self.hx + self.dir * self.len, self.y))
    }

    /// Draws the open leaf with `pen` on `leaf_layer` and the swing arc (thin) on `swing_layer`.
    pub(crate) fn draw(
        &self,
        sym: &mut Sym,
        pen: crate::sym::Pen,
        leaf_layer: &'static str,
        swing_layer: &'static str,
    ) {
        if self.len < 1e-3 {
            return;
        }
        sym.layer(leaf_layer);
        let far = self.hx + self.dir * self.len;
        if self.thick > 1e-4 && sym.fine() {
            let pts: Vec<(f64, f64)> = [
                (self.hx, self.y),
                (far, self.y),
                (far, self.y - self.thick),
                (self.hx, self.y - self.thick),
            ]
            .iter()
            .map(|p| self.rot(*p))
            .collect();
            sym.poly(&pts, true, pen);
        } else {
            sym.line((self.hx, self.y), self.tip(), pen);
        }
        sym.layer(swing_layer);
        let start = if self.dir > 0.0 { 0.0 } else { std::f64::consts::PI };
        sym.arc((self.hx, self.y), self.len, start, self.dir * self.angle, crate::sym::Pen::Fine);
    }
}
