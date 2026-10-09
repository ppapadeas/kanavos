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
    fn write(
        slot: &mut Option<GrowBuffer>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        usage: wgpu::BufferUsages,
        data: &[u8],
    ) {
        if data.is_empty() {
            *slot = None;
            return;
        }
        let len = data.len() as u64;
        if let Some(existing) = slot.as_mut()
            && existing.buffer.size() >= len
            && existing.buffer.size() <= len.saturating_mul(4)
        {
            queue.write_buffer(&existing.buffer, 0, data);
            existing.len = len;
            return;
        }
        // Round up to a multiple of 4 with ~25 % headroom for later edits.
        let capacity = (len + len / 4 + 3) & !3;
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
        let existing = self.objects.remove(&id);
        let slot = match &existing {
            Some(r) => r.slot,
            None => self.allocate_slot(id),
        };
        let parts: Vec<PreparedPart> = meshes.iter().map(|m| prepare_mesh(m, slot, cos_crease)).collect();
        let bounds = parts.iter().map(|p| p.bounds).fold(None, union_bounds);
        let category = meshes.first().map(|m| m.category).unwrap_or(0);
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
        true
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
            for id in &chunk.members {
                let Some(rec) = self.objects.get(id) else { continue };
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
