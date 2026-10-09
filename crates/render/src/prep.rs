//! CPU preparation of scene meshes for the GPU: validation, vertex welding, crease-aware normals, feature and
//! silhouette edges, closedness.
//!
//! Tolerances: vertices closer than [`WELD_QUANTUM`] (0.1 mm, the project tolerance) are treated as the same
//! point when building adjacency. Triangles whose area is negligible relative to their edge lengths are dropped.
//! Points that straddle a quantisation cell boundary may fail to weld; the only effect is an extra edge line.

use bytemuck::{Pod, Zeroable};

use crate::scene::SceneMesh;

/// Welding grid, metres.
pub(crate) const WELD_QUANTUM: f64 = 1e-4;

/// Edge between faces meeting at more than the crease angle, on an open boundary, or non-manifold: always drawn.
pub(crate) const EDGE_FEATURE: u32 = 0;
/// Edge between faces meeting at less than the crease angle: drawn only where it is on the silhouette.
pub(crate) const EDGE_SMOOTH: u32 = 1;

/// One surface vertex as uploaded to the GPU (28 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct SurfaceVertex {
    /// Position relative to the scene origin.
    pub position: [f32; 3],
    /// Unit normal as snorm16; `w` is 32767 when the mesh is a closed solid, 0 otherwise.
    pub normal: [i16; 4],
    /// sRGB colour with linear alpha.
    pub color: [u8; 4],
    /// Object slot (index into the object state texture).
    pub slot: u32,
}

/// One edge line as uploaded to the GPU (40 bytes), drawn as an instanced screen-space quad.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct EdgeInstance {
    /// Start point.
    pub p0: [f32; 3],
    /// End point.
    pub p1: [f32; 3],
    /// Normal of the first adjacent face, snorm8.
    pub n0: [i8; 4],
    /// Normal of the second adjacent face (equal to `n0` on boundaries), snorm8.
    pub n1: [i8; 4],
    /// Object slot.
    pub slot: u32,
    /// [`EDGE_FEATURE`] or [`EDGE_SMOOTH`].
    pub kind: u32,
}

/// A mesh ready for upload.
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedPart {
    /// Vertices.
    pub vertices: Vec<SurfaceVertex>,
    /// Triangle indices into `vertices`.
    pub indices: Vec<u32>,
    /// Edge lines.
    pub edges: Vec<EdgeInstance>,
    /// Drawn in the transparent pass.
    pub transparent: bool,
    /// Bounding box (min, max) of the valid triangles.
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

impl PreparedPart {
    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Centre of the bounding box.
    pub fn centre(&self) -> [f32; 3] {
        match self.bounds {
            Some((lo, hi)) => [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), 0.5 * (lo[2] + hi[2])],
            None => [0.0; 3],
        }
    }

    /// Whether the mesh was found to be closed (every edge shared by exactly two triangles).
    #[cfg(test)]
    pub fn closed(&self) -> bool {
        self.vertices.first().is_some_and(|v| v.normal[3] != 0)
    }
}

/// A valid triangle with its geometric normal.
struct Tri {
    /// Original vertex indices.
    v: [u32; 3],
    /// Unit normal.
    n: [f64; 3],
    /// Twice the area.
    area2: f64,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn to_f64(p: [f32; 3]) -> [f64; 3] {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

fn snorm16(n: [f64; 3], w: i16) -> [i16; 4] {
    let q = |c: f64| (c.clamp(-1.0, 1.0) * 32767.0).round() as i16;
    [q(n[0]), q(n[1]), q(n[2]), w]
}

fn snorm8(n: [f64; 3]) -> [i8; 4] {
    let q = |c: f64| (c.clamp(-1.0, 1.0) * 127.0).round() as i8;
    [q(n[0]), q(n[1]), q(n[2]), 0]
}

fn quantize(p: [f64; 3]) -> [i64; 3] {
    p.map(|c| (c / WELD_QUANTUM).round() as i64)
}

/// Validated triangles of a mesh.
fn valid_triangles(mesh: &SceneMesh) -> Vec<Tri> {
    let mut tris = Vec::with_capacity(mesh.indices.len() / 3);
    for chunk in mesh.indices.chunks_exact(3) {
        let &[a, b, c] = chunk else { continue };
        let (Some(pa), Some(pb), Some(pc)) =
            (mesh.positions.get(a as usize), mesh.positions.get(b as usize), mesh.positions.get(c as usize))
        else {
            continue;
        };
        let (pa, pb, pc) = (to_f64(*pa), to_f64(*pb), to_f64(*pc));
        if !pa.iter().chain(pb.iter()).chain(pc.iter()).all(|x| x.is_finite()) {
            continue;
        }
        let (ab, ac) = (sub(pb, pa), sub(pc, pa));
        let n = cross(ab, ac);
        let area2 = len(n);
        let scale = dot(ab, ab) + dot(ac, ac);
        let usable = area2.is_finite() && area2 > 0.0 && area2 > 1e-9 * scale;
        if !usable {
            continue;
        }
        tris.push(Tri { v: [a, b, c], n: n.map(|x| x / area2), area2 });
    }
    tris
}

/// Prepares one mesh. `cos_crease` is the cosine of the crease angle: adjacent faces whose normals differ by
/// more than the crease angle get a hard edge (separate normals and an edge line).
pub(crate) fn prepare_mesh(mesh: &SceneMesh, slot: u32, cos_crease: f64) -> PreparedPart {
    let tris = valid_triangles(mesh);
    let transparent = mesh.is_transparent();
    if tris.is_empty() {
        return PreparedPart { transparent, ..PreparedPart::default() };
    }
    let pos = |i: u32| mesh.positions.get(i as usize).copied().map(to_f64).unwrap_or([0.0; 3]);

    // Weld referenced vertices by quantised position.
    let mut keyed: Vec<([i64; 3], u32)> =
        tris.iter().flat_map(|t| t.v).map(|i| (quantize(pos(i)), i)).collect();
    keyed.sort_unstable();
    keyed.dedup_by_key(|k| k.1);
    let mut welded = vec![u32::MAX; mesh.positions.len()];
    let mut welded_count = 0u32;
    let mut last_key = None;
    for (key, i) in &keyed {
        if last_key != Some(*key) {
            welded_count += 1;
            last_key = Some(*key);
        }
        if let Some(w) = welded.get_mut(*i as usize) {
            *w = welded_count - 1;
        }
    }
    let weld = |i: u32| welded.get(i as usize).copied().unwrap_or(u32::MAX);

    // Edge adjacency through sorted half-edges: (min welded, max welded, triangle, corner).
    let mut half_edges: Vec<(u32, u32, u32, u8)> = Vec::with_capacity(tris.len() * 3);
    for (ti, t) in tris.iter().enumerate() {
        for k in 0..3u8 {
            let a = t.v.get(usize::from(k)).copied().unwrap_or(0);
            let b = t.v.get(usize::from((k + 1) % 3)).copied().unwrap_or(0);
            let (wa, wb) = (weld(a), weld(b));
            if wa != wb {
                half_edges.push((wa.min(wb), wa.max(wb), ti as u32, k));
            }
        }
    }
    half_edges.sort_unstable();

    let mut closed = true;
    let mut edges = Vec::new();
    let mut start = 0;
    while start < half_edges.len() {
        let key = half_edges.get(start).map(|h| (h.0, h.1));
        let mut end = start + 1;
        while end < half_edges.len() && half_edges.get(end).map(|h| (h.0, h.1)) == key {
            end += 1;
        }
        let group = half_edges.get(start..end).unwrap_or(&[]);
        start = end;
        let Some(&(_, _, t0, k0)) = group.first() else { continue };
        let Some(first) = tris.get(t0 as usize) else { continue };
        let a = first.v.get(usize::from(k0)).copied().unwrap_or(0);
        let b = first.v.get(usize::from((k0 + 1) % 3)).copied().unwrap_or(0);
        let n0 = first.n;
        let (kind, n1) = match group {
            [_] => {
                closed = false;
                (EDGE_FEATURE, n0)
            }
            [_, (_, _, t1, _)] => {
                let n1 = tris.get(*t1 as usize).map(|t| t.n).unwrap_or(n0);
                let d = dot(n0, n1);
                if d < cos_crease {
                    (EDGE_FEATURE, n1)
                } else if d < 1.0 - 1e-6 {
                    (EDGE_SMOOTH, n1)
                } else {
                    continue;
                }
            }
            [_, (_, _, t1, _), ..] => {
                closed = false;
                (EDGE_FEATURE, tris.get(*t1 as usize).map(|t| t.n).unwrap_or(n0))
            }
            [] => continue,
        };
        let (pa, pb) = (pos(a), pos(b));
        edges.push(EdgeInstance {
            p0: pa.map(|c| c as f32),
            p1: pb.map(|c| c as f32),
            n0: snorm8(n0),
            n1: snorm8(n1),
            slot,
            kind,
        });
    }

    let closed_w = if closed { i16::MAX } else { 0 };
    let color = [mesh.color.r, mesh.color.g, mesh.color.b, mesh.color.a];
    let (vertices, indices) = match &mesh.normals {
        Some(normals) if normals.len() == mesh.positions.len() => {
            build_with_normals(mesh, &tris, normals, slot, color, closed_w)
        }
        _ => build_smoothed(mesh, &tris, &weld, welded_count as usize, cos_crease, slot, color, closed_w),
    };

    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for v in &vertices {
        for ((l, h), c) in lo.iter_mut().zip(hi.iter_mut()).zip(v.position) {
            *l = l.min(c);
            *h = h.max(c);
        }
    }
    let bounds = (lo[0] <= hi[0]).then_some((lo, hi));
    PreparedPart { vertices, indices, edges, transparent, bounds }
}

/// Vertices from caller-supplied normals: one output vertex per referenced input vertex.
fn build_with_normals(
    mesh: &SceneMesh,
    tris: &[Tri],
    normals: &[[f32; 3]],
    slot: u32,
    color: [u8; 4],
    closed_w: i16,
) -> (Vec<SurfaceVertex>, Vec<u32>) {
    let mut remap = vec![u32::MAX; mesh.positions.len()];
    let mut vertices = Vec::new();
    let mut indices = Vec::with_capacity(tris.len() * 3);
    for t in tris {
        for &i in &t.v {
            let Some(slot_ref) = remap.get_mut(i as usize) else { continue };
            if *slot_ref == u32::MAX {
                *slot_ref = vertices.len() as u32;
                let given = normals.get(i as usize).copied().map(to_f64).unwrap_or(t.n);
                let l = len(given);
                let n = if l.is_finite() && l > 1e-12 { given.map(|c| c / l) } else { t.n };
                let p = mesh.positions.get(i as usize).copied().unwrap_or([0.0; 3]);
                vertices.push(SurfaceVertex { position: p, normal: snorm16(n, closed_w), color, slot });
            }
            indices.push(*slot_ref);
        }
    }
    (vertices, indices)
}

/// Vertices with normals averaged across faces within the crease angle, split where faces meet more sharply.
#[allow(clippy::too_many_arguments)]
fn build_smoothed(
    mesh: &SceneMesh,
    tris: &[Tri],
    weld: &dyn Fn(u32) -> u32,
    welded_count: usize,
    cos_crease: f64,
    slot: u32,
    color: [u8; 4],
    closed_w: i16,
) -> (Vec<SurfaceVertex>, Vec<u32>) {
    // Triangles around each welded vertex (compressed adjacency lists).
    let mut offsets = vec![0u32; welded_count + 1];
    for t in tris {
        for &i in &t.v {
            if let Some(o) = offsets.get_mut(weld(i) as usize + 1) {
                *o += 1;
            }
        }
    }
    for i in 1..offsets.len() {
        let prev = offsets.get(i - 1).copied().unwrap_or(0);
        if let Some(o) = offsets.get_mut(i) {
            *o += prev;
        }
    }
    let mut fill = offsets.clone();
    let mut around = vec![0u32; offsets.last().copied().unwrap_or(0) as usize];
    for (ti, t) in tris.iter().enumerate() {
        for &i in &t.v {
            if let Some(f) = fill.get_mut(weld(i) as usize) {
                if let Some(slot_ref) = around.get_mut(*f as usize) {
                    *slot_ref = ti as u32;
                }
                *f += 1;
            }
        }
    }

    // Corner normals, then deduplicate corners sharing a welded vertex and a quantised normal.
    let mut corners: Vec<(u32, [i16; 4], u32)> = Vec::with_capacity(tris.len() * 3);
    for (ti, t) in tris.iter().enumerate() {
        for (k, &i) in t.v.iter().enumerate() {
            let w = weld(i);
            let lo = offsets.get(w as usize).copied().unwrap_or(0) as usize;
            let hi = offsets.get(w as usize + 1).copied().unwrap_or(0) as usize;
            let mut sum = [0.0f64; 3];
            for &u in around.get(lo..hi).unwrap_or(&[]) {
                if let Some(o) = tris.get(u as usize)
                    && dot(t.n, o.n) >= cos_crease
                {
                    for (s, c) in sum.iter_mut().zip(o.n) {
                        *s += c * o.area2;
                    }
                }
            }
            let l = len(sum);
            let n = if l > 1e-300 { sum.map(|c| c / l) } else { t.n };
            corners.push((w, snorm16(n, closed_w), (ti * 3 + k) as u32));
        }
    }
    corners.sort_unstable();
    let mut vertices = Vec::new();
    let mut indices = vec![0u32; tris.len() * 3];
    let mut last: Option<(u32, [i16; 4])> = None;
    for (w, n, corner) in corners {
        if last != Some((w, n)) {
            last = Some((w, n));
            let ti = corner as usize / 3;
            let k = corner as usize % 3;
            let i = tris.get(ti).and_then(|t| t.v.get(k)).copied().unwrap_or(0);
            let p = mesh.positions.get(i as usize).copied().unwrap_or([0.0; 3]);
            vertices.push(SurfaceVertex { position: p, normal: n, color, slot });
        }
        if let Some(ix) = indices.get_mut(corner as usize) {
            *ix = vertices.len() as u32 - 1;
        }
    }
    (vertices, indices)
}

/// 64-bit FNV-1a style hash over 32-bit words; used to detect unchanged objects in `set_scene`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WordHash(u64);

impl WordHash {
    /// A fresh hash.
    pub fn new() -> WordHash {
        WordHash(0xcbf2_9ce4_8422_2325)
    }

    /// Mixes one word.
    pub fn word(&mut self, w: u32) {
        self.0 ^= u64::from(w);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    /// Mixes a 64-bit value.
    pub fn u64(&mut self, v: u64) {
        self.word(v as u32);
        self.word((v >> 32) as u32);
    }

    /// Mixes a whole mesh.
    pub fn mesh(&mut self, m: &SceneMesh) {
        self.u64(m.id);
        self.word(m.category);
        self.word(u32::from_le_bytes([m.color.r, m.color.g, m.color.b, m.color.a]));
        self.u64(m.positions.len() as u64);
        for p in &m.positions {
            for c in p {
                self.word(c.to_bits());
            }
        }
        match &m.normals {
            Some(ns) => {
                self.word(1);
                for n in ns {
                    for c in n {
                        self.word(c.to_bits());
                    }
                }
            }
            None => self.word(0),
        }
        self.u64(m.indices.len() as u64);
        for &i in &m.indices {
            self.word(i);
        }
    }

    /// The hash value.
    pub fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::Rgba;
    use kanavos_geom::{Point2, Point3};
    use kanavos_solid::{cuboid, cylinder};

    fn same(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    fn cos_deg(d: f64) -> f64 {
        d.to_radians().cos()
    }

    fn scene_mesh(m: &kanavos_solid::Mesh, color: Rgba) -> SceneMesh {
        SceneMesh::from_solid_mesh(1, m, Point3::ORIGIN, color, 0)
    }

    #[test]
    fn cube_has_twelve_feature_edges_and_flat_normals() {
        let m = scene_mesh(&cuboid(Point3::ORIGIN, Point3::new(1.0, 2.0, 3.0)), Rgba::WHITE);
        let p = prepare_mesh(&m, 5, cos_deg(20.0));
        assert!(p.closed());
        assert_eq!(p.edges.len(), 12);
        assert!(p.edges.iter().all(|e| e.kind == EDGE_FEATURE && e.slot == 5));
        // 6 faces × 4 corners, every face flat.
        assert_eq!(p.vertices.len(), 24);
        assert_eq!(p.triangle_count(), 12);
        for v in &p.vertices {
            let n = v.normal;
            let axis = n.iter().take(3).filter(|c| c.unsigned_abs() == 32767).count();
            assert_eq!(axis, 1, "axis-aligned normal {n:?}");
            assert_eq!(n[3], i16::MAX, "closed flag");
        }
        let (lo, hi) = p.bounds.unwrap();
        assert!(same(lo, [0.0, 0.0, 0.0]));
        assert!(same(hi, [1.0, 2.0, 3.0]));
    }

    #[test]
    fn cylinder_sides_are_smooth_with_silhouette_edges() {
        let m = scene_mesh(&cylinder(Point2::new(0.0, 0.0), 0.5, 0.0, 3.0, 24).unwrap(), Rgba::WHITE);
        let p = prepare_mesh(&m, 0, cos_deg(20.0));
        assert!(p.closed());
        let feature = p.edges.iter().filter(|e| e.kind == EDGE_FEATURE).count();
        let smooth = p.edges.iter().filter(|e| e.kind == EDGE_SMOOTH).count();
        assert_eq!(feature, 48, "top and bottom rims");
        assert_eq!(smooth, 24, "vertical facet edges");
        // Side vertices: one per rim point and rim (24 × 2), caps: 24 × 2, all distinct normals per cap.
        assert_eq!(p.vertices.len(), 24 * 2 + 24 * 2);
    }

    #[test]
    fn open_quad_is_not_closed() {
        let m = SceneMesh {
            id: 1,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            indices: vec![0, 1, 2, 0, 2, 3],
            color: Rgba::WHITE,
            ..SceneMesh::default()
        };
        let p = prepare_mesh(&m, 0, cos_deg(20.0));
        assert!(!p.closed());
        assert_eq!(p.edges.len(), 4, "boundary only; the diagonal is coplanar");
        assert_eq!(p.vertices.len(), 4);
        assert_eq!(p.vertices[0].normal[3], 0);
    }

    #[test]
    fn degenerate_and_invalid_triangles_are_skipped() {
        let m = SceneMesh {
            id: 1,
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [f32::NAN, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            indices: vec![0, 1, 2, 0, 1, 3, 0, 1, 9, 0, 1, 4, 7],
            color: Rgba::WHITE,
            ..SceneMesh::default()
        };
        let p = prepare_mesh(&m, 0, cos_deg(20.0));
        assert_eq!(p.triangle_count(), 1);
    }

    #[test]
    fn supplied_normals_are_used() {
        let m = SceneMesh {
            id: 1,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Some(vec![[0.0, 0.0, 2.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]),
            indices: vec![0, 1, 2],
            color: Rgba::new(10, 20, 30, 100),
            ..SceneMesh::default()
        };
        let p = prepare_mesh(&m, 3, cos_deg(20.0));
        assert!(p.transparent);
        assert_eq!(p.vertices[0].normal, [0, 0, 32767, 0]);
        assert_eq!(p.vertices[1].normal, [0, 0, 32767, 0], "zero normal falls back to the face normal");
        assert_eq!(p.vertices[2].normal, [32767, 0, 0, 0]);
        assert_eq!(p.vertices[2].color, [10, 20, 30, 100]);
    }

    #[test]
    fn welding_joins_duplicated_vertices() {
        // Two triangles of a folded sheet with duplicated vertices along the fold.
        let m = SceneMesh {
            id: 1,
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.000_01, 1.0, 0.0],
                [1.0, 1.0, 1.0],
            ],
            indices: vec![0, 1, 2, 3, 5, 4],
            color: Rgba::WHITE,
            ..SceneMesh::default()
        };
        let p = prepare_mesh(&m, 0, cos_deg(20.0));
        // Shared fold edge is a feature edge (45° fold) counted once; four boundary edges.
        assert_eq!(p.edges.len(), 5);
    }

    #[test]
    fn hash_changes_with_content() {
        let a = scene_mesh(&cuboid(Point3::ORIGIN, Point3::new(1.0, 1.0, 1.0)), Rgba::WHITE);
        let mut b = a.clone();
        let h = |m: &SceneMesh| {
            let mut w = WordHash::new();
            w.mesh(m);
            w.finish()
        };
        assert_eq!(h(&a), h(&b));
        b.color = Rgba::BLACK;
        assert_ne!(h(&a), h(&b));
    }
}
