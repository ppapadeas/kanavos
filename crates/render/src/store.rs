//! Scene objects on the CPU and their batched GPU buffers.
//!
//! Objects are grouped into **chunks** of at most [`MAX_CHUNK_OBJECTS`] objects and about
//! [`MAX_CHUNK_VERTICES`] vertices. Each chunk owns one vertex buffer, one index buffer and one edge buffer, so a
//! house of a few thousand elements draws in a few dozen calls, and editing one element rebuilds only its
//! chunk. Transparent meshes live in one shared buffer with a draw range per mesh so they can be sorted back to
//! front every frame.

use std::collections::HashMap;
use std::sync::Arc;

use crate::prep::{EdgeInstance, PreparedPart, SurfaceVertex, WordHash, prepare_mesh};
use crate::scene::SceneMesh;

/// Most objects in one chunk.
pub(crate) const MAX_CHUNK_OBJECTS: usize = 256;
/// Vertex budget of one chunk (a single larger object still gets a chunk of its own).
pub(crate) const MAX_CHUNK_VERTICES: usize = 1 << 20;

/// One object: all meshes sharing an id.
#[derive(Debug)]
pub(crate) struct ObjectRecord {
    /// Dense index used by the GPU.
    pub slot: u32,
    /// Category of the first mesh.
    pub category: u32,
    /// Content hash of the source meshes.
    pub hash: u64,
    /// Prepared meshes.
    pub parts: Vec<PreparedPart>,
    /// Owning chunk.
    pub chunk: usize,
    /// Bounds of all parts (render space).
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

impl ObjectRecord {
    fn opaque_vertices(&self) -> usize {
        self.parts.iter().filter(|p| !p.transparent).map(|p| p.vertices.len()).sum()
    }

    fn has_transparent(&self) -> bool {
        self.parts.iter().any(|p| p.transparent && !p.indices.is_empty())
    }
}

/// A GPU buffer with spare capacity, rewritten in place when the new content fits.
#[derive(Debug)]
pub(crate) struct GrowBuffer {
    /// The buffer.
    pub buffer: wgpu::Buffer,
    /// Bytes in use.
    pub len: u64,
}

impl GrowBuffer {
    /// Writes `data` into the buffer in `slot`, reusing it when the data fits and it is not wastefully large.
    /// Empty data, or data larger than the device's buffer limit, leaves the slot empty (nothing is drawn
    /// rather than the device failing).
    pub(crate) fn write(
        slot: &mut Option<GrowBuffer>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        usage: wgpu::BufferUsages,
        data: &[u8],
    ) {
        let len = data.len() as u64;
        if data.is_empty() || len > device.limits().max_buffer_size {
            *slot = None;
            return;
        }
        if let Some(existing) = slot.as_mut()
            && existing.buffer.size() >= len
            && existing.buffer.size() <= len.saturating_mul(4)
        {
            queue.write_buffer(&existing.buffer, 0, data);
            existing.len = len;
            return;
        }
        // Round up to a multiple of 4 with ~25 % headroom for later edits, within the device limit.
        let capacity = ((len + len / 4 + 3) & !3).min(device.limits().max_buffer_size & !3).max(len);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, data);
        *slot = Some(GrowBuffer { buffer, len });
    }
}

/// GPU buffers of one chunk.
#[derive(Debug, Default)]
pub(crate) struct ChunkGpu {
    /// Opaque vertices.
    pub vertices: Option<GrowBuffer>,
    /// Opaque triangle indices.
    pub indices: Option<GrowBuffer>,
    /// Number of indices.
    pub index_count: u32,
    /// Edge instances of all parts (opaque and transparent).
    pub edges: Option<GrowBuffer>,
    /// Number of edge instances.
    pub edge_count: u32,
}

/// A group of objects drawn together.
#[derive(Debug, Default)]
pub(crate) struct Chunk {
    /// Member object ids.
    pub members: Vec<u64>,
    /// Opaque vertices of the members.
    pub vertex_count: usize,
    /// Needs a rebuild.
    pub dirty: bool,
    /// Uploaded buffers.
    pub gpu: ChunkGpu,
    /// Bounds of all members (render space), for frustum culling; refreshed on upload.
    pub bounds: Option<([f32; 3], [f32; 3])>,
}

impl Chunk {
    /// Whether any part of the chunk can be inside the clip volume of `view_proj` (render space to clip
    /// space, reverse-Z). Conservative: a box is rejected only when all its corners lie outside the same
    /// clip plane.
    pub fn may_be_visible(&self, view_proj: &crate::math::Mat4) -> bool {
        let Some((lo, hi)) = self.bounds else { return false };
        let corners = (0..8).map(|i| {
            let pick = |bit: usize, axis: usize| {
                let (l, h) = (lo.get(axis).copied().unwrap_or(0.0), hi.get(axis).copied().unwrap_or(0.0));
                f64::from(if i & bit == 0 { l } else { h })
            };
            view_proj.transform([pick(1, 0), pick(2, 1), pick(4, 2), 1.0])
        });
        // For each plane, the number of corners outside it: left, right, bottom, top, near (reverse-Z: z > w).
        let mut outside = [0usize; 5];
        for [x, y, z, w] in corners {
            let tests = [x < -w, x > w, y < -w, y > w, z > w];
            for (count, out) in outside.iter_mut().zip(tests) {
                *count += usize::from(out);
            }
        }
        !outside.contains(&8)
    }
}

/// One transparent mesh in the shared transparent buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TransparentDraw {
    /// First index.
    pub first_index: u32,
    /// Index count.
    pub index_count: u32,
    /// Centre for sorting (render space).
    pub centre: [f32; 3],
}

/// All transparent meshes.
#[derive(Debug, Default)]
pub(crate) struct TransparentGpu {
    /// Vertices.
    pub vertices: Option<GrowBuffer>,
    /// Indices.
    pub indices: Option<GrowBuffer>,
    /// Draw ranges.
    pub draws: Vec<TransparentDraw>,
}

/// The scene content.
#[derive(Debug, Default)]
pub(crate) struct Store {
    /// Objects by id.
    pub objects: HashMap<u64, ObjectRecord>,
    /// Slot → id, shared copy-on-write with pending picks.
    pub slot_ids: Arc<Vec<Option<u64>>>,
    free_slots: Vec<u32>,
    /// Chunks (never removed; empty chunks are reused).
    pub chunks: Vec<Chunk>,
    /// Transparent buffers.
    pub transparent: TransparentGpu,
    transparent_dirty: bool,
    /// Incremented on every geometry change.
    pub revision: u64,
}

/// Hash of an object's meshes.
pub(crate) fn hash_meshes(meshes: &[&SceneMesh]) -> u64 {
    let mut h = WordHash::new();
    h.u64(meshes.len() as u64);
    for m in meshes {
        h.mesh(m);
    }
    h.finish()
}

fn union_bounds(
    a: Option<([f32; 3], [f32; 3])>,
    b: Option<([f32; 3], [f32; 3])>,
) -> Option<([f32; 3], [f32; 3])> {
    match (a, b) {
        (Some((alo, ahi)), Some((blo, bhi))) => Some((
            [alo[0].min(blo[0]), alo[1].min(blo[1]), alo[2].min(blo[2])],
            [ahi[0].max(bhi[0]), ahi[1].max(bhi[1]), ahi[2].max(bhi[2])],
        )),
        (x, None) => x,
        (None, y) => y,
    }
}

impl Store {
    /// Number of slots ever allocated (the state texture must hold this many).
    pub fn slot_capacity(&self) -> usize {
        self.slot_ids.len()
    }

    fn allocate_slot(&mut self, id: u64) -> u32 {
        let ids = Arc::make_mut(&mut self.slot_ids);
        if let Some(slot) = self.free_slots.pop()
            && let Some(entry) = ids.get_mut(slot as usize)
        {
            *entry = Some(id);
            return slot;
        }
        ids.push(Some(id));
        (ids.len() - 1) as u32
    }

    fn free_slot(&mut self, slot: u32) {
        if let Some(entry) = Arc::make_mut(&mut self.slot_ids).get_mut(slot as usize) {
            *entry = None;
            self.free_slots.push(slot);
        }
    }

    fn choose_chunk(&mut self, vertices: usize) -> usize {
        let fits = |c: &Chunk| {
            c.members.is_empty()
                || (c.members.len() < MAX_CHUNK_OBJECTS && c.vertex_count + vertices <= MAX_CHUNK_VERTICES)
        };
        // Prefer the most recent chunk with room, so that objects added together stay together.
        if let Some(i) = self.chunks.iter().rposition(fits) {
            return i;
        }
        self.chunks.push(Chunk::default());
        self.chunks.len() - 1
    }

    /// Inserts or replaces an object. Returns `false` when the content is unchanged.
    pub fn upsert(&mut self, id: u64, meshes: &[&SceneMesh], cos_crease: f64) -> bool {
        if meshes.is_empty() {
            return self.remove(id);
        }
        let hash = hash_meshes(meshes);
        if self.objects.get(&id).is_some_and(|r| r.hash == hash) {
            return false;
        }
        let slot = self.slot_for(id);
        let parts = meshes.iter().map(|m| prepare_mesh(m, slot, cos_crease)).collect();
        let category = meshes.first().map(|m| m.category).unwrap_or(0);
        self.install(id, slot, category, hash, parts);
        true
    }

    /// Inserts or replaces many objects, preparing their meshes on several threads when there is enough work.
    /// Objects with empty mesh lists are removed. Returns (changed, unchanged).
    pub fn upsert_many(&mut self, items: &[(u64, Vec<&SceneMesh>)], cos_crease: f64) -> (usize, usize) {
        /// Below this many triangles, threads cost more than they save.
        const PARALLEL_TRIANGLES: usize = 20_000;
        let mut work: Vec<(u64, u32, u64, &[&SceneMesh])> = Vec::new();
        let mut unchanged = 0;
        let mut removed = 0;
        for (id, meshes) in items {
            if meshes.is_empty() {
                removed += usize::from(self.remove(*id));
                continue;
            }
            let hash = hash_meshes(meshes);
            if self.objects.get(id).is_some_and(|r| r.hash == hash) {
                unchanged += 1;
                continue;
            }
            let slot = self.slot_for(*id);
            work.push((*id, slot, hash, meshes.as_slice()));
        }
        let triangles: usize = work.iter().flat_map(|w| w.3.iter()).map(|m| m.triangle_count()).sum();
        let threads = std::thread::available_parallelism().map(usize::from).unwrap_or(1).min(16);
        let prepare = |batch: &[(u64, u32, u64, &[&SceneMesh])]| -> Vec<Vec<PreparedPart>> {
            batch
                .iter()
                .map(|(_, slot, _, meshes)| {
                    meshes.iter().map(|m| prepare_mesh(m, *slot, cos_crease)).collect()
                })
                .collect()
        };
        let prepared: Vec<Vec<PreparedPart>> = if threads > 1 && triangles >= PARALLEL_TRIANGLES {
            let per_thread = work.len().div_ceil(threads).max(1);
            std::thread::scope(|scope| {
                let handles: Vec<_> = work
                    .chunks(per_thread)
                    .map(|batch| (batch.len(), scope.spawn(move || prepare(batch))))
                    .collect();
                // A panicking worker would be a bug in mesh preparation; its objects are then left empty, and
                // the results stay aligned with `work`.
                handles
                    .into_iter()
                    .flat_map(|(n, h)| h.join().unwrap_or_else(|_| vec![Vec::new(); n]))
                    .collect()
            })
        } else {
            prepare(&work)
        };
        let changed = work.len().min(prepared.len());
        for ((id, slot, hash, meshes), parts) in work.iter().zip(prepared) {
            let category = meshes.first().map(|m| m.category).unwrap_or(0);
            self.install(*id, *slot, category, *hash, parts);
        }
        (changed + removed, unchanged)
    }

    /// The slot of an existing object, or a newly allocated one.
    fn slot_for(&mut self, id: u64) -> u32 {
        match self.objects.get(&id) {
            Some(r) => r.slot,
            None => self.allocate_slot(id),
        }
    }

    /// Stores prepared parts for an object (new or replaced) and marks its chunk for upload.
    fn install(&mut self, id: u64, slot: u32, category: u32, hash: u64, parts: Vec<PreparedPart>) {
        let existing = self.objects.remove(&id);
        let bounds = parts.iter().map(|p| p.bounds).fold(None, union_bounds);
        let mut record = ObjectRecord { slot, category, hash, parts, chunk: 0, bounds };
        let new_vertices = record.opaque_vertices();
        let mut transparent_changed = record.has_transparent();
        match existing {
            Some(old) => {
                transparent_changed |= old.has_transparent();
                record.chunk = old.chunk;
                if let Some(c) = self.chunks.get_mut(old.chunk) {
                    c.vertex_count = c.vertex_count.saturating_sub(old.opaque_vertices()) + new_vertices;
                    c.dirty = true;
                }
            }
            None => {
                let ci = self.choose_chunk(new_vertices);
                record.chunk = ci;
                if let Some(c) = self.chunks.get_mut(ci) {
                    c.members.push(id);
                    c.vertex_count += new_vertices;
                    c.dirty = true;
                }
            }
        }
        self.transparent_dirty |= transparent_changed;
        self.objects.insert(id, record);
        self.revision += 1;
    }

    /// Removes an object. Returns whether it existed.
    pub fn remove(&mut self, id: u64) -> bool {
        let Some(old) = self.objects.remove(&id) else { return false };
        if let Some(c) = self.chunks.get_mut(old.chunk) {
            c.members.retain(|m| *m != id);
            c.vertex_count = c.vertex_count.saturating_sub(old.opaque_vertices());
            c.dirty = true;
        }
        self.transparent_dirty |= old.has_transparent();
        self.free_slot(old.slot);
        self.revision += 1;
        true
    }

    /// Removes everything.
    pub fn clear(&mut self) {
        let ids: Vec<u64> = self.objects.keys().copied().collect();
        for id in ids {
            self.remove(id);
        }
    }

    /// Uploads dirty chunks and transparent meshes.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        for (ci, chunk) in self.chunks.iter_mut().enumerate() {
            if !chunk.dirty {
                continue;
            }
            chunk.dirty = false;
            let mut vertices: Vec<SurfaceVertex> = Vec::with_capacity(chunk.vertex_count);
            let mut indices: Vec<u32> = Vec::new();
            let mut edges: Vec<EdgeInstance> = Vec::new();
            chunk.bounds = None;
            for id in &chunk.members {
                let Some(rec) = self.objects.get(id) else { continue };
                chunk.bounds = union_bounds(chunk.bounds, rec.bounds);
                for part in &rec.parts {
                    edges.extend_from_slice(&part.edges);
                    if part.transparent {
                        continue;
                    }
                    let base = vertices.len() as u32;
                    vertices.extend_from_slice(&part.vertices);
                    indices.extend(part.indices.iter().map(|i| i + base));
                }
            }
            let label = format!("kanavos chunk {ci}");
            GrowBuffer::write(
                &mut chunk.gpu.vertices,
                device,
                queue,
                &label,
                wgpu::BufferUsages::VERTEX,
                bytemuck::cast_slice(&vertices),
            );
            GrowBuffer::write(
                &mut chunk.gpu.indices,
                device,
                queue,
                &label,
                wgpu::BufferUsages::INDEX,
                bytemuck::cast_slice(&indices),
            );
            GrowBuffer::write(
                &mut chunk.gpu.edges,
                device,
                queue,
                &label,
                wgpu::BufferUsages::VERTEX,
                bytemuck::cast_slice(&edges),
            );
            chunk.gpu.index_count = indices.len() as u32;
            chunk.gpu.edge_count = edges.len() as u32;
        }

        if self.transparent_dirty {
            self.transparent_dirty = false;
            let mut vertices: Vec<SurfaceVertex> = Vec::new();
            let mut indices: Vec<u32> = Vec::new();
            let mut draws = Vec::new();
            for rec in self.objects.values() {
                for part in rec.parts.iter().filter(|p| p.transparent && !p.indices.is_empty()) {
                    let base = vertices.len() as u32;
                    let first_index = indices.len() as u32;
                    vertices.extend_from_slice(&part.vertices);
                    indices.extend(part.indices.iter().map(|i| i + base));
                    draws.push(TransparentDraw {
                        first_index,
                        index_count: part.indices.len() as u32,
                        centre: part.centre(),
                    });
                }
            }
            let t = &mut self.transparent;
            GrowBuffer::write(
                &mut t.vertices,
                device,
                queue,
                "kanavos transparent",
                wgpu::BufferUsages::VERTEX,
                bytemuck::cast_slice(&vertices),
            );
            GrowBuffer::write(
                &mut t.indices,
                device,
                queue,
                "kanavos transparent",
                wgpu::BufferUsages::INDEX,
                bytemuck::cast_slice(&indices),
            );
            t.draws = draws;
        }
    }

    /// Union of object bounds (render space) for objects accepted by `filter`.
    pub fn bounds_where(&self, filter: impl Fn(u64, &ObjectRecord) -> bool) -> Option<([f32; 3], [f32; 3])> {
        self.objects.iter().filter(|(id, r)| filter(**id, r)).map(|(_, r)| r.bounds).fold(None, union_bounds)
    }

    /// Totals for statistics: (triangles, edges).
    pub fn totals(&self) -> (usize, usize) {
        let mut tris = 0;
        let mut edges = 0;
        for r in self.objects.values() {
            for p in &r.parts {
                tris += p.triangle_count();
                edges += p.edges.len();
            }
        }
        (tris, edges)
    }

    /// Chunk membership, for tests.
    #[cfg(test)]
    pub fn chunk_members(&self) -> Vec<Vec<u64>> {
        self.chunks.iter().map(|c| c.members.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::Rgba;
    use kanavos_geom::Point3;
    use kanavos_solid::cuboid;

    fn same(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    fn mesh(id: u64, x: f64, color: Rgba) -> SceneMesh {
        SceneMesh::from_solid_mesh(
            id,
            &cuboid(Point3::new(x, 0.0, 0.0), Point3::new(x + 1.0, 1.0, 1.0)),
            Point3::ORIGIN,
            color,
            0,
        )
    }

    #[test]
    fn upsert_detects_unchanged_content() {
        let mut s = Store::default();
        let m = mesh(1, 0.0, Rgba::WHITE);
        assert!(s.upsert(1, &[&m], 0.9));
        let rev = s.revision;
        assert!(!s.upsert(1, &[&m], 0.9), "same content is a no-op");
        assert_eq!(s.revision, rev);
        let m2 = mesh(1, 2.0, Rgba::WHITE);
        assert!(s.upsert(1, &[&m2], 0.9));
        assert!(s.revision > rev);
        assert_eq!(s.objects.len(), 1);
    }

    #[test]
    fn slots_are_reused_after_removal() {
        let mut s = Store::default();
        for id in 0..3 {
            s.upsert(id, &[&mesh(id, id as f64, Rgba::WHITE)], 0.9);
        }
        let slot1 = s.objects[&1].slot;
        assert!(s.remove(1));
        assert!(!s.remove(1));
        assert_eq!(s.slot_ids[slot1 as usize], None);
        s.upsert(7, &[&mesh(7, 9.0, Rgba::WHITE)], 0.9);
        assert_eq!(s.objects[&7].slot, slot1);
        assert_eq!(s.slot_ids[slot1 as usize], Some(7));
        assert_eq!(s.slot_capacity(), 3);
    }

    #[test]
    fn chunks_fill_up_and_split() {
        let mut s = Store::default();
        for id in 0..(MAX_CHUNK_OBJECTS as u64 + 10) {
            s.upsert(id, &[&mesh(id, 0.0, Rgba::WHITE)], 0.9);
        }
        let members = s.chunk_members();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].len(), MAX_CHUNK_OBJECTS);
        assert_eq!(members[1].len(), 10);
        s.clear();
        assert!(s.objects.is_empty());
        assert!(s.chunk_members().iter().all(|m| m.is_empty()));
    }

    #[test]
    fn batch_preparation_matches_one_by_one() {
        // Enough triangles to take the multi-threaded path.
        let meshes: Vec<SceneMesh> = (0..300u64)
            .map(|id| {
                let c = kanavos_geom::Point2::new(id as f64, 0.0);
                let m = kanavos_solid::cylinder(c, 0.3, 0.0, 1.0 + id as f64 * 0.01, 24).unwrap();
                SceneMesh::from_solid_mesh(id, &m, Point3::ORIGIN, Rgba::WHITE, 0)
            })
            .collect();
        let items: Vec<(u64, Vec<&SceneMesh>)> = meshes.iter().map(|m| (m.id, vec![m])).collect();
        let mut batch = Store::default();
        assert_eq!(batch.upsert_many(&items, 0.9), (300, 0));
        let mut single = Store::default();
        for m in &meshes {
            single.upsert(m.id, &[m], 0.9);
        }
        for m in &meshes {
            let (a, b) = (&batch.objects[&m.id], &single.objects[&m.id]);
            assert_eq!(a.slot, b.slot);
            assert_eq!(a.parts[0].vertices, b.parts[0].vertices);
            assert_eq!(a.parts[0].edges, b.parts[0].edges);
        }
        assert_eq!(batch.chunk_members(), single.chunk_members());
        // Unchanged content is skipped; an empty list removes.
        let mut again = items.clone();
        again[0].1.clear();
        assert_eq!(batch.upsert_many(&again, 0.9), (1, 299));
        assert!(!batch.objects.contains_key(&0));
    }

    #[test]
    fn frustum_culling_is_conservative() {
        use crate::camera::{OrbitCamera, StandardView, Viewport};
        let chunk = |lo: [f32; 3], hi: [f32; 3]| Chunk { bounds: Some((lo, hi)), ..Chunk::default() };
        let camera = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::Front, 10.0);
        let vp = camera.view_projection(Viewport::new(800.0, 600.0), None, Point3::ORIGIN);
        assert!(chunk([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]).may_be_visible(&vp), "in view");
        assert!(
            chunk([-100.0, 5.0, -100.0], [100.0, 6.0, 100.0]).may_be_visible(&vp),
            "straddles the frustum"
        );
        assert!(!chunk([50.0, -1.0, -1.0], [52.0, 1.0, 1.0]).may_be_visible(&vp), "far to the right");
        assert!(!chunk([-1.0, -30.0, -1.0], [1.0, -20.0, 1.0]).may_be_visible(&vp), "behind the eye");
        assert!(!Chunk::default().may_be_visible(&vp), "empty");
    }

    #[test]
    fn bounds_union() {
        let mut s = Store::default();
        s.upsert(1, &[&mesh(1, 0.0, Rgba::WHITE)], 0.9);
        s.upsert(2, &[&mesh(2, 5.0, Rgba::new(0, 0, 0, 100))], 0.9);
        let (lo, hi) = s.bounds_where(|_, _| true).unwrap();
        assert!(same(lo, [0.0, 0.0, 0.0]));
        assert!(same(hi, [6.0, 1.0, 1.0]));
        let (_, hi1) = s.bounds_where(|id, _| id == 1).unwrap();
        assert!(same(hi1, [1.0, 1.0, 1.0]));
        assert_eq!(s.totals().0, 24);
    }
}
