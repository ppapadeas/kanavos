//! Vertex welding, closedness checks and normals.

use std::collections::HashMap;

use kanavos_geom::{Point3, Vec3};

use crate::Mesh;

type Cell = (i64, i64, i64);

fn cell(p: Point3, size: f64) -> Cell {
    ((p.x / size).floor() as i64, (p.y / size).floor() as i64, (p.z / size).floor() as i64)
}

impl Mesh {
    /// The same surface with vertices closer than `eps` metres merged and degenerate triangles (repeated
    /// indices) removed. Vertex order follows first occurrence.
    ///
    /// Tolerance: two vertices merge when they are within `eps` of the first vertex kept for that position (a
    /// spatial hash with cells of size `eps`, checking neighbouring cells). Non-finite positions are kept apart.
    pub fn welded(&self, eps: f64) -> Mesh {
        let eps = if eps.is_finite() && eps > 0.0 { eps } else { 1e-9 };
        let mut grid: HashMap<Cell, Vec<u32>> = HashMap::new();
        let mut positions: Vec<Point3> = Vec::with_capacity(self.positions.len());
        let mut remap: Vec<u32> = Vec::with_capacity(self.positions.len());
        for &p in &self.positions {
            let found = if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
                let c = cell(p, eps);
                let mut hit = None;
                'search: for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            if let Some(list) = grid.get(&(c.0 + dx, c.1 + dy, c.2 + dz)) {
                                for &k in list {
                                    if positions.get(k as usize).is_some_and(|q| q.distance(p) <= eps) {
                                        hit = Some(k);
                                        break 'search;
                                    }
                                }
                            }
                        }
                    }
                }
                match hit {
                    Some(k) => Some(k),
                    None => {
                        let k = positions.len() as u32;
                        positions.push(p);
                        grid.entry(c).or_default().push(k);
                        None
                    }
                }
            } else {
                positions.push(p);
                None
            };
            remap.push(found.unwrap_or(positions.len() as u32 - 1));
        }
        let mut triangles = Vec::with_capacity(self.triangles.len());
        for t in &self.triangles {
            let (Some(&a), Some(&b), Some(&c)) =
                (remap.get(t[0] as usize), remap.get(t[1] as usize), remap.get(t[2] as usize))
            else {
                continue;
            };
            if a != b && b != c && a != c {
                triangles.push([a, b, c]);
            }
        }
        Mesh { positions, triangles }
    }

    /// Number of edges that are not shared by exactly two triangles with opposite directions. Zero for a closed,
    /// consistently oriented (watertight) mesh. Call on a welded mesh.
    pub fn open_edge_count(&self) -> usize {
        let mut count: HashMap<(u32, u32), i32> = HashMap::new();
        for t in &self.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *count.entry((a, b)).or_insert(0) += 1;
            }
        }
        count.iter().filter(|((a, b), n)| **n != 1 || count.get(&(*b, *a)).copied().unwrap_or(0) != 1).count()
    }

    /// Whether the mesh is closed and consistently oriented after welding within `eps`.
    pub fn is_closed(&self, eps: f64) -> bool {
        let w = self.welded(eps);
        !w.triangles.is_empty() && w.open_edge_count() == 0
    }

    /// Area-weighted vertex normals, one per position (smooth shading of a welded mesh).
    pub fn vertex_normals(&self) -> Vec<Vec3> {
        let mut n = vec![Vec3::ZERO; self.positions.len()];
        for t in &self.triangles {
            let (Some(&a), Some(&b), Some(&c)) = (
                self.positions.get(t[0] as usize),
                self.positions.get(t[1] as usize),
                self.positions.get(t[2] as usize),
            ) else {
                continue;
            };
            let fnrm = (b - a).cross(c - a);
            for &i in t {
                if let Some(v) = n.get_mut(i as usize) {
                    *v += fnrm;
                }
            }
        }
        n.into_iter().map(|v| v.normalized().unwrap_or(Vec3::ZERO)).collect()
    }

    /// A copy whose vertices are split along sharp edges, so that [`Mesh::vertex_normals`] shades flat faces
    /// flat and curved surfaces smooth. Faces meeting at a vertex share it only when their normals differ by
    /// less than `crease_angle` radians.
    pub fn split_creases(&self, crease_angle: f64) -> Mesh {
        let cos_limit = crease_angle.cos();
        let normals = self.face_normals();
        // Triangles around each vertex.
        let mut around: Vec<Vec<(usize, usize)>> = vec![Vec::new(); self.positions.len()];
        for (ti, t) in self.triangles.iter().enumerate() {
            for (corner, &v) in t.iter().enumerate() {
                if let Some(list) = around.get_mut(v as usize) {
                    list.push((ti, corner));
                }
            }
        }
        let mut out = Mesh { positions: Vec::new(), triangles: self.triangles.clone() };
        for (v, list) in around.iter().enumerate() {
            let Some(&p) = self.positions.get(v) else { continue };
            // Greedy clusters by normal.
            let mut clusters: Vec<(Vec3, u32)> = Vec::new();
            for &(ti, corner) in list {
                let nrm = normals.get(ti).copied().unwrap_or(Vec3::ZERO);
                let idx = match clusters.iter().find(|(cn, _)| cn.dot(nrm) >= cos_limit) {
                    Some(&(_, k)) => k,
                    None => {
                        let k = out.push_vertex(p);
                        clusters.push((nrm, k));
                        k
                    }
                };
                if let Some(tri) = out.triangles.get_mut(ti)
                    && let Some(slot) = tri.get_mut(corner)
                {
                    *slot = idx;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::{cuboid, extrude_polygon};
    use kanavos_geom::{Point3, Polygon2};

    #[test]
    fn welding_a_cuboid_closes_it() {
        let m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 2.0, 3.0));
        assert_eq!(m.positions.len(), 24);
        let w = m.welded(1e-9);
        assert_eq!(w.positions.len(), 8);
        assert_eq!(w.open_edge_count(), 0);
        assert!(m.is_closed(1e-9));
        assert!((w.volume() - 6.0).abs() < 1e-12);
    }

    #[test]
    fn open_mesh_is_detected() {
        let mut m = cuboid(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0));
        m.triangles.pop();
        assert!(!m.is_closed(1e-9));
    }

    #[test]
    fn normals_and_creases() {
        let m =
            extrude_polygon(&Polygon2::from_m(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]), 0.0, 1.0)
                .unwrap()
                .welded(1e-9);
        // Smooth normals at a cube corner point diagonally outwards.
        let n = m.vertex_normals();
        assert!(n.iter().all(|v| (v.length() - 1.0).abs() < 1e-9));
        // Split at 30°: every corner gets three vertices, one per face.
        let s = m.split_creases(30f64.to_radians());
        assert_eq!(s.positions.len(), 24);
        let sn = s.vertex_normals();
        assert!(sn.iter().all(|v| v.x.abs() > 0.999 || v.y.abs() > 0.999 || v.z.abs() > 0.999));
    }
}
