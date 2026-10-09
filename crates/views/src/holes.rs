//! Stand-in: door and window holes in the 3D solids of straight walls.
//!
//! The shared wall solids in `kanavos-elements` do not cut openings yet (that work is in progress there). Until
//! they do, views rebuild the layer solids of a straight wall with its openings cut out, so sections and
//! elevations show reveals, sills, heads and the frames behind them. The function checks the shared solids
//! first and steps aside as soon as they already have holes (their volume is below the full prism volume), so
//! the switch-over needs no change here; the module can then be deleted.
//!
//! Each layer becomes one closed mesh: the two long faces with the opening rectangles removed (triangulated),
//! the ends, top and bottom caps (minus openings that reach them) and the reveal faces (jambs, sill, head).

use kanavos_elements::{solids, wall_layout};
use kanavos_geom::{Point2, Point3, Polygon2, Vec2};
use kanavos_model::{Document, ElementKind, Id};
use kanavos_solid::{Mesh, triangulate};

use crate::meshops::{V3, cross, dot, sub};
use crate::poly::{self, Shape};

/// An opening as a box in wall coordinates: along the wall from `s0` to `s1`, absolute elevations `z0`–`z1`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cutout {
    s0: f64,
    s1: f64,
    z0: f64,
    z1: f64,
}

/// Layer solids of a straight wall with its openings cut out, as `(mesh, material)` pairs, or `None` when the
/// shared solids already include the openings, the wall is curved, or it hosts no openings.
pub(crate) fn wall_solids_with_openings(doc: &Document, id: Id) -> Option<Vec<(Mesh, Option<Id>)>> {
    let e = doc.element(id)?;
    let ElementKind::Wall(w) = &e.kind else { return None };
    if w.arc().is_some() {
        return None;
    }
    let floor = e.storey.and_then(|s| doc.storey(s)).map(|s| s.elevation.as_m()).unwrap_or(0.0);
    let layout = wall_layout(doc, id).ok()?;
    let len = w.length().as_m();
    let cutouts: Vec<Cutout> = doc
        .openings_in(id)
        .map(|(_, o)| {
            let c = o.offset.as_m();
            let h = o.width.as_m() / 2.0;
            let z0 = floor + o.sill.as_m();
            Cutout {
                s0: (c - h).max(0.0),
                s1: (c + h).min(len),
                z0: z0.max(layout.base),
                z1: (z0 + o.height.as_m()).min(layout.top),
            }
        })
        .filter(|c| c.s1 - c.s0 > 1e-6 && c.z1 - c.z0 > 1e-6)
        .collect();
    if cutouts.is_empty() {
        return None;
    }
    // Do the shared solids already have holes?
    let shared = solids(doc, id).ok()?;
    let actual: f64 = shared.iter().map(|s| s.mesh.volume()).sum();
    let full: f64 =
        layout.layers.iter().map(|l| l.region.area().as_m2()).sum::<f64>() * (layout.top - layout.base);
    if (full - actual).abs() > 1e-6 * full.max(1e-9) {
        return None;
    }
    let along = (w.end - w.start).normalized()?;
    let mut out = Vec::with_capacity(layout.layers.len());
    for l in &layout.layers {
        let mesh = layer_mesh(&l.region, layout.base, layout.top, w.start, along, &cutouts)?;
        out.push((mesh, Some(l.material)));
    }
    Some(out)
}

struct Frame {
    origin: Point2,
    u: Vec2,
    n: Vec2,
}

impl Frame {
    fn local(&self, p: Point2) -> [f64; 2] {
        let d = p - self.origin;
        [d.dot(self.u), d.dot(self.n)]
    }
    fn world(&self, s: f64, t: f64, z: f64) -> Point3 {
        let p = self.origin + self.u * s + self.n * t;
        Point3::from_plan(p, z)
    }
}

fn push_tris(mesh: &mut Mesh, pts: &[Point3], tris: &[[u32; 3]], outward: V3) {
    let base = mesh.positions.len() as u32;
    mesh.positions.extend_from_slice(pts);
    // Orient by the first non-degenerate triangle.
    let flip = tris.iter().find_map(|t| {
        let p = |i: u32| pts.get(i as usize).map(|q| q.to_array()).unwrap_or([0.0; 3]);
        let n = cross(sub(p(t[1]), p(t[0])), sub(p(t[2]), p(t[0])));
        let d = dot(n, outward);
        if d.abs() > 1e-15 { Some(d < 0.0) } else { None }
    });
    for t in tris {
        if flip.unwrap_or(false) {
            mesh.push_triangle(base + t[0], base + t[2], base + t[1]);
        } else {
            mesh.push_triangle(base + t[0], base + t[1], base + t[2]);
        }
    }
}

/// Triangulates planar shapes given in face coordinates and lifts them with `lift`.
fn push_shapes(
    mesh: &mut Mesh,
    shapes: &[Shape],
    lift: impl Fn(f64, f64) -> Point3,
    outward: V3,
) -> Option<()> {
    for s in shapes {
        let p = poly::polygon_of(s)?;
        let (pts, tris) = triangulate(&p).ok()?;
        let lifted: Vec<Point3> = pts.iter().map(|q| lift(q.x.as_m(), q.y.as_m())).collect();
        push_tris(mesh, &lifted, &tris, outward);
    }
    Some(())
}

fn quad(mesh: &mut Mesh, q: [Point3; 4], outward: V3) {
    push_tris(mesh, &q, &[[0, 1, 2], [0, 2, 3]], outward);
}

/// Range of `t` where the line `s = c` crosses a polygon given in `(s, t)` coordinates.
fn t_range(ring: &[[f64; 2]], c: f64) -> Option<(f64, f64)> {
    let n = ring.len();
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for i in 0..n {
        let (Some(a), Some(b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
        let (s0, s1) = (a[0].min(b[0]), a[0].max(b[0]));
        if c < s0 - 1e-9 || c > s1 + 1e-9 || (b[0] - a[0]).abs() < 1e-12 {
            continue;
        }
        let t = a[1] + (c - a[0]) / (b[0] - a[0]) * (b[1] - a[1]);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    if hi > lo { Some((lo, hi)) } else { None }
}

fn layer_mesh(
    band: &Polygon2,
    base: f64,
    top: f64,
    origin: Point2,
    u: Vec2,
    cuts: &[Cutout],
) -> Option<Mesh> {
    let f = Frame { origin, u, n: u.perp() };
    let mut ring: Vec<[f64; 2]> = band.vertices.iter().map(|p| f.local(*p)).collect();
    if poly::signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let smin = ring.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let smax = ring.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let tmin_all = ring.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let tmax_all = ring.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    let cuts: Vec<Cutout> = cuts
        .iter()
        .map(|c| Cutout { s0: c.s0.max(smin), s1: c.s1.min(smax), ..*c })
        .filter(|c| c.s1 - c.s0 > 1e-6)
        .collect();
    let uv = [u.x, u.y, 0.0];
    let nv = [f.n.x, f.n.y, 0.0];
    let mut mesh = Mesh::new();
    for i in 0..n {
        let (Some(&a), Some(&b)) = (ring.get(i), ring.get((i + 1) % n)) else { continue };
        let (ds, dt) = (b[0] - a[0], b[1] - a[1]);
        let len = ds.hypot(dt);
        if len < 1e-9 {
            continue;
        }
        // Outward normal of a counter-clockwise ring edge, in (s, t), then in world.
        let out_st = [dt / len, -ds / len];
        let outward = [uv[0] * out_st[0] + nv[0] * out_st[1], uv[1] * out_st[0] + nv[1] * out_st[1], 0.0];
        let long = dt.abs() < 1e-9 * len.max(1.0);
        if long {
            let (s0, s1) = (a[0].min(b[0]), a[0].max(b[0]));
            let holes: Vec<Shape> = cuts
                .iter()
                .filter(|c| c.s1 > s0 && c.s0 < s1)
                .map(|c| poly::rect(c.s0.max(s0), c.z0, c.s1.min(s1), c.z1))
                .collect();
            let face = poly::difference(&[poly::rect(s0, base, s1, top)], &holes);
            let t = a[1];
            push_shapes(&mut mesh, &face, |s, z| f.world(s, t, z), outward)?;
        } else {
            quad(
                &mut mesh,
                [
                    f.world(a[0], a[1], base),
                    f.world(b[0], b[1], base),
                    f.world(b[0], b[1], top),
                    f.world(a[0], a[1], top),
                ],
                outward,
            );
        }
    }
    // Caps, minus openings that reach them.
    let band_shape: Shape = vec![ring.clone()];
    for (z, up) in [(base, false), (top, true)] {
        let reach: Vec<Shape> = cuts
            .iter()
            .filter(|c| if up { c.z1 >= top - 1e-9 } else { c.z0 <= base + 1e-9 })
            .map(|c| poly::rect(c.s0, tmin_all - 1.0, c.s1, tmax_all + 1.0))
            .collect();
        let cap = poly::difference(std::slice::from_ref(&band_shape), &reach);
        push_shapes(&mut mesh, &cap, |s, t| f.world(s, t, z), [0.0, 0.0, if up { 1.0 } else { -1.0 }])?;
    }
    // Reveals.
    for c in &cuts {
        let mid = 0.5 * (c.s0 + c.s1);
        let Some((t0, t1)) = t_range(&ring, mid) else { continue };
        if c.s0 > smin + 1e-6 {
            let (a0, a1) = t_range(&ring, c.s0).unwrap_or((t0, t1));
            quad(
                &mut mesh,
                [
                    f.world(c.s0, a0, c.z0),
                    f.world(c.s0, a1, c.z0),
                    f.world(c.s0, a1, c.z1),
                    f.world(c.s0, a0, c.z1),
                ],
                uv,
            );
        }
        if c.s1 < smax - 1e-6 {
            let (b0, b1) = t_range(&ring, c.s1).unwrap_or((t0, t1));
            quad(
                &mut mesh,
                [
                    f.world(c.s1, b0, c.z0),
                    f.world(c.s1, b1, c.z0),
                    f.world(c.s1, b1, c.z1),
                    f.world(c.s1, b0, c.z1),
                ],
                [-uv[0], -uv[1], 0.0],
            );
        }
        if c.z0 > base + 1e-6 {
            quad(
                &mut mesh,
                [
                    f.world(c.s0, t0, c.z0),
                    f.world(c.s1, t0, c.z0),
                    f.world(c.s1, t1, c.z0),
                    f.world(c.s0, t1, c.z0),
                ],
                [0.0, 0.0, 1.0],
            );
        }
        if c.z1 < top - 1e-6 {
            quad(
                &mut mesh,
                [
                    f.world(c.s0, t0, c.z1),
                    f.world(c.s1, t0, c.z1),
                    f.world(c.s1, t1, c.z1),
                    f.world(c.s0, t1, c.z1),
                ],
                [0.0, 0.0, -1.0],
            );
        }
    }
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Length;
    use kanavos_model::{Element, Opening, OpeningKind, ReferenceLine, Structure, Wall, starter};

    #[test]
    fn a_window_removes_its_volume_from_the_wall() {
        let doc = starter();
        let storey = doc.storeys()[0].id;
        let material = doc.materials().next().unwrap().id;
        let (doc, wid) = doc.with_new_id();
        let doc = doc.with_element(Element::on_storey(
            wid,
            storey,
            ElementKind::Wall(Wall {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(5.0, 0.0),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Basic { material, thickness: Length::cm(20.0) },
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        ));
        let (doc, oid) = doc.with_new_id();
        let doc = doc.with_element(Element::on_storey(
            oid,
            storey,
            ElementKind::Opening(Opening {
                host: wid,
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
        ));
        let layers = wall_solids_with_openings(&doc, wid).unwrap();
        assert_eq!(layers.len(), 1);
        let v = layers[0].0.volume();
        let expected = 5.0 * 0.2 * 3.0 - 1.2 * 1.4 * 0.2;
        assert!((v - expected).abs() < 1e-9, "volume {v}, expected {expected}");
    }
}
