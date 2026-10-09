//! 3D part builder and the colour palette of objects.

use std::f64::consts::TAU;

use kanavos_drawing::Rgba;
use kanavos_geom::{Point3, Polygon2, PolygonWithHoles};
use kanavos_solid::{Mesh, cuboid, extrude};

use crate::Part;
use crate::shape::{P2, ellipse_pts, segments};

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
    /// Sanitary ceramic.
    pub(crate) const PORCELAIN: Rgba = Rgba::rgb(226, 229, 232);
    /// White lacquer and appliances.
    pub(crate) const WHITE: Rgba = Rgba::rgb(243, 243, 241);
    /// Cabinet fronts.
    pub(crate) const FRONT: Rgba = Rgba::rgb(228, 224, 216);
    /// Light timber.
    pub(crate) const OAK: Rgba = Rgba::rgb(196, 166, 126);
    /// Dark timber.
    pub(crate) const WALNUT: Rgba = Rgba::rgb(128, 96, 70);
    /// Upholstery.
    pub(crate) const FABRIC: Rgba = Rgba::rgb(164, 160, 154);
    /// Cushions.
    pub(crate) const CUSHION: Rgba = Rgba::rgb(186, 181, 172);
    /// Bed linen.
    pub(crate) const LINEN: Rgba = Rgba::rgb(238, 235, 228);
    /// Duvet.
    pub(crate) const DUVET: Rgba = Rgba::rgb(205, 212, 218);
    /// Worktops.
    pub(crate) const STONE: Rgba = Rgba::rgb(198, 194, 186);
    /// Stainless steel.
    pub(crate) const STEEL: Rgba = Rgba::rgb(196, 199, 202);
    /// Black glass of hobs and oven doors.
    pub(crate) const BLACK_GLASS: Rgba = Rgba::rgb(40, 42, 46);
    /// Foliage.
    pub(crate) const FOLIAGE: Rgba = Rgba::rgb(118, 156, 96);
    /// Darker foliage.
    pub(crate) const FOLIAGE_DARK: Rgba = Rgba::rgb(94, 132, 80);
    /// Bark.
    pub(crate) const BARK: Rgba = Rgba::rgb(112, 88, 64);
    /// Car body.
    pub(crate) const CAR: Rgba = Rgba::rgb(150, 162, 178);
    /// Tyres.
    pub(crate) const TYRE: Rgba = Rgba::rgb(46, 46, 50);
    /// Tinted car glass.
    pub(crate) const TINTED: Rgba = Rgba::new(70, 82, 96, 200);
    /// Parasol canvas.
    pub(crate) const CANVAS: Rgba = Rgba::rgb(234, 228, 212);
    /// Book spines, three muted tones.
    pub(crate) const BOOKS: [Rgba; 3] =
        [Rgba::rgb(150, 120, 110), Rgba::rgb(120, 135, 150), Rgba::rgb(180, 170, 140)];
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

    /// Adds all parts of another builder after mapping their vertices (the map must preserve orientation).
    pub(crate) fn add_mapped(&mut self, other: Parts, f: impl Fn(Point3) -> Point3) {
        for p in other.parts {
            self.add(p.mesh.map_positions(&f), p.color);
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

    /// A vertical prism of a plan outline between two heights.
    pub(crate) fn prism(&mut self, pts: &[P2], z: (f64, f64), color: Rgba) {
        self.prism_holes(pts, &[], z, color);
    }

    /// A vertical prism with through-holes.
    pub(crate) fn prism_holes(&mut self, outer: &[P2], holes: &[Vec<P2>], z: (f64, f64), color: Rgba) {
        if outer.len() < 3 || (z.1 - z.0).abs() < 1e-4 {
            return;
        }
        let shape = PolygonWithHoles {
            outer: Polygon2::from_m(outer),
            holes: holes.iter().filter(|h| h.len() >= 3).map(|h| Polygon2::from_m(h)).collect(),
        };
        if let Ok(m) = extrude(&shape, z.0, z.1) {
            self.add(m, color);
        }
    }

    /// A vertical cylinder.
    pub(crate) fn cyl(&mut self, c: P2, r: f64, z: (f64, f64), color: Rgba) {
        if r > 1e-4 {
            self.prism(&ellipse_pts(c, r, r), z, color);
        }
    }

    /// A prism along X: the outline is given in the `(y, z)` plane.
    pub(crate) fn prism_x(&mut self, yz: &[P2], x: (f64, f64), color: Rgba) {
        let mut tmp = Parts::new();
        tmp.prism(yz, x, color);
        // (u, v, w) → (w, u, v) is a cyclic permutation: orientation is preserved.
        self.add_mapped(tmp, |p| Point3::new(p.z, p.x, p.y));
    }

    /// A prism along Y: the outline is given in the `(x, z)` plane.
    pub(crate) fn prism_y(&mut self, xz: &[P2], y: (f64, f64), color: Rgba) {
        let mut tmp = Parts::new();
        tmp.prism(xz, y, color);
        // (u, v, w) → (u, w, v) swaps two axes: flip the triangles to keep normals outward.
        for p in tmp.parts {
            self.add(p.mesh.map_positions(|q| Point3::new(q.x, q.z, q.y)).flipped(), p.color);
        }
    }

    /// An ellipsoid with radii `r` about `c`.
    pub(crate) fn ellipsoid(&mut self, c: [f64; 3], r: [f64; 3], color: Rgba) {
        if !ok3(c) || !ok3(r) || r.iter().any(|v| *v < 1e-4) {
            return;
        }
        let lon = segments(r[0].max(r[1])).min(24);
        let lat = (lon / 2).max(6);
        let mut m = Mesh::new();
        let p = |phi: f64, theta: f64| {
            Point3::new(
                c[0] + r[0] * phi.sin() * theta.cos(),
                c[1] + r[1] * phi.sin() * theta.sin(),
                c[2] + r[2] * phi.cos(),
            )
        };
        let top = m.push_vertex(Point3::new(c[0], c[1], c[2] + r[2]));
        let bottom = m.push_vertex(Point3::new(c[0], c[1], c[2] - r[2]));
        let mut rings: Vec<Vec<u32>> = Vec::new();
        for i in 1..lat {
            let phi = std::f64::consts::PI * i as f64 / lat as f64;
            rings.push((0..lon).map(|j| m.push_vertex(p(phi, TAU * j as f64 / lon as f64))).collect());
        }
        let at = |ring: &Vec<u32>, j: usize| ring.get(j % lon).copied().unwrap_or(top);
        if let Some(first) = rings.first() {
            for j in 0..lon {
                m.push_triangle(top, at(first, j), at(first, j + 1));
            }
        }
        for w in rings.windows(2) {
            if let [upper, lower] = w {
                for j in 0..lon {
                    let (a, b, cc, d) = (at(upper, j), at(lower, j), at(lower, j + 1), at(upper, j + 1));
                    m.push_triangle(a, b, cc);
                    m.push_triangle(a, cc, d);
                }
            }
        }
        if let Some(last) = rings.last() {
            for j in 0..lon {
                m.push_triangle(bottom, at(last, j + 1), at(last, j));
            }
        }
        self.add(m, color);
    }

    /// A cone (or pyramid with many sides) with its base at `z.0` and apex at `z.1`.
    pub(crate) fn cone(&mut self, c: P2, r: f64, z: (f64, f64), color: Rgba) {
        if r < 1e-4 || (z.1 - z.0).abs() < 1e-4 {
            return;
        }
        let ring = ellipse_pts(c, r, r);
        let mut m = Mesh::new();
        let apex = m.push_vertex(Point3::new(c.0, c.1, z.1));
        let centre = m.push_vertex(Point3::new(c.0, c.1, z.0));
        let idx: Vec<u32> = ring.iter().map(|q| m.push_vertex(Point3::new(q.0, q.1, z.0))).collect();
        let n = idx.len();
        for j in 0..n {
            let (a, b) =
                (idx.get(j).copied().unwrap_or(centre), idx.get((j + 1) % n).copied().unwrap_or(centre));
            if z.1 > z.0 {
                m.push_triangle(a, b, apex);
                m.push_triangle(centre, b, a);
            } else {
                m.push_triangle(b, a, apex);
                m.push_triangle(centre, a, b);
            }
        }
        self.add(m, color);
    }

    /// The parts.
    pub(crate) fn into_parts(self) -> Vec<Part> {
        self.parts
    }
}
