//! Picking: which object is under a pixel, and where.
//!
//! A pick renders object slots and depth into a tiny (2r+1)² target centred on the cursor, using a projection
//! narrowed to that window, copies the result into a mappable buffer and reads it back asynchronously. The
//! vertex work is one pass over the scene; rasterisation and bandwidth are negligible. The nearest hit within
//! the window wins, which makes thin objects easy to pick.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use kanavos_geom::Point3;

use crate::camera::Viewport;
use crate::gpu::{DEPTH_FORMAT, Target};
use crate::math::Mat4;

/// Row pitch of the readback buffer (wgpu requires multiples of 256 bytes).
const ROW_PITCH: u32 = 256;
/// Readback buffers, i.e. how many picks may be in flight at once.
const PICK_SLOTS: usize = 4;

const IDLE: u8 = 0;
const PENDING: u8 = 1;
const READY: u8 = 2;
const FAILED: u8 = 3;

/// Identifies a pick request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PickTicket(pub u64);

/// What lies under a pixel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickResult {
    /// The object hit, or `None` for background.
    pub id: Option<u64>,
    /// World position of the hit (metres), when something was hit.
    pub position: Option<Point3>,
    /// Pixel that was hit (may differ from the requested one by up to the pick radius), physical pixels.
    pub pixel: [f64; 2],
    /// Normalised device depth of the hit (reverse-Z: 1 at the near plane).
    pub depth: Option<f32>,
}

/// Data needed to interpret a readback.
#[derive(Debug, Clone)]
struct PendingPick {
    ticket: PickTicket,
    /// Top-left pixel of the pick window.
    corner: [i64; 2],
    viewport: Viewport,
    inv_view_proj: Option<Mat4>,
    origin: Point3,
    slot_ids: Arc<Vec<Option<u64>>>,
    submission: Option<wgpu::SubmissionIndex>,
}

#[derive(Debug)]
struct ReadbackSlot {
    buffer: wgpu::Buffer,
    state: Arc<AtomicU8>,
    pending: Option<PendingPick>,
}

/// Pick targets and readback buffers.
#[derive(Debug)]
pub(crate) struct Picker {
    /// Window size in pixels (2r + 1).
    pub size: u32,
    pub ids: Target,
    pub depths: Target,
    pub depth: Target,
    slots: Vec<ReadbackSlot>,
    next_ticket: u64,
    completed: VecDeque<(PickTicket, PickResult)>,
}

/// Window geometry for a pick at a cursor position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PickWindow {
    /// Top-left pixel of the window.
    pub corner: [i64; 2],
    /// Matrix applied after the projection that maps the window to the whole clip-space square.
    pub narrow: Mat4,
}

/// The window of `size` pixels centred on the pixel containing `cursor`.
pub(crate) fn pick_window(cursor: [f64; 2], viewport: Viewport, size: u32) -> PickWindow {
    let r = i64::from(size / 2);
    let px = [cursor[0].floor() as i64, cursor[1].floor() as i64];
    let centre = [px[0] as f64 + 0.5, px[1] as f64 + 0.5];
    let [cx, cy] = viewport.to_ndc(centre[0], centre[1]);
    let s = f64::from(size.max(1));
    let sx = viewport.width.max(1.0) / s;
    let sy = viewport.height.max(1.0) / s;
    let narrow = Mat4::from_rows(
        [sx, 0.0, 0.0, -cx * sx],
        [0.0, sy, 0.0, -cy * sy],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    PickWindow { corner: [px[0] - r, px[1] - r], narrow }
}

/// Index of the best hit in a `size × size` window of slot values (0 = background): the centre when it is a hit,
/// otherwise the nearest hit, preferring the nearer surface on ties.
pub(crate) fn best_hit(ids: &[u32], depths: &[f32], size: u32) -> Option<usize> {
    let r = i64::from(size / 2);
    let n = i64::from(size);
    let mut best: Option<(i64, f32, usize)> = None;
    for (i, &id) in ids.iter().enumerate() {
        if id == 0 {
            continue;
        }
        let (x, y) = (i as i64 % n - r, i as i64 / n - r);
        let d2 = x * x + y * y;
        let depth = depths.get(i).copied().unwrap_or(0.0);
        let better = match best {
            None => true,
            Some((bd2, bdepth, _)) => d2 < bd2 || (d2 == bd2 && depth > bdepth),
        };
        if better {
            best = Some((d2, depth, i));
        }
    }
    best.map(|(_, _, i)| i)
}

impl Picker {
    pub fn new(device: &wgpu::Device, radius: u32) -> Picker {
        let size = (2 * radius.min(31) + 1).max(1);
        use wgpu::TextureUsages as U;
        let ids = Target::new(
            device,
            "kanavos pick ids",
            [size, size],
            wgpu::TextureFormat::R32Uint,
            1,
            U::RENDER_ATTACHMENT | U::COPY_SRC,
            &[],
        );
        let depths = Target::new(
            device,
            "kanavos pick depth values",
            [size, size],
            wgpu::TextureFormat::R32Float,
            1,
            U::RENDER_ATTACHMENT | U::COPY_SRC,
            &[],
        );
        let depth = Target::new(
            device,
            "kanavos pick depth",
            [size, size],
            DEPTH_FORMAT,
            1,
            U::RENDER_ATTACHMENT,
            &[],
        );
        let slots = (0..PICK_SLOTS)
            .map(|i| ReadbackSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("kanavos pick readback {i}")),
                    size: u64::from(2 * ROW_PITCH * size),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(IDLE)),
                pending: None,
            })
            .collect();
        Picker { size, ids, depths, depth, slots, next_ticket: 1, completed: VecDeque::new() }
    }

    /// A free readback slot, if any.
    pub fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.pending.is_none() && s.state.load(Ordering::Acquire) == IDLE)
    }

    /// Records the copy of the pick targets into a slot's buffer.
    pub fn encode_copy(&self, encoder: &mut wgpu::CommandEncoder, slot: usize) {
        let Some(s) = self.slots.get(slot) else { return };
        let extent = wgpu::Extent3d { width: self.size, height: self.size, depth_or_array_layers: 1 };
        for (target, offset) in [(&self.ids, 0u64), (&self.depths, u64::from(ROW_PITCH * self.size))] {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &s.buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(ROW_PITCH),
                        rows_per_image: Some(self.size),
                    },
                },
                extent,
            );
        }
    }

    /// Starts mapping a slot after its commands were submitted.
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        &mut self,
        slot: usize,
        submission: wgpu::SubmissionIndex,
        window: &PickWindow,
        viewport: Viewport,
        inv_view_proj: Option<Mat4>,
        origin: Point3,
        slot_ids: Arc<Vec<Option<u64>>>,
    ) -> PickTicket {
        let ticket = PickTicket(self.next_ticket);
        self.next_ticket += 1;
        if let Some(s) = self.slots.get_mut(slot) {
            s.state.store(PENDING, Ordering::Release);
            let state = Arc::clone(&s.state);
            s.buffer.map_async(wgpu::MapMode::Read, .., move |result| {
                state.store(if result.is_ok() { READY } else { FAILED }, Ordering::Release);
            });
            s.pending = Some(PendingPick {
                ticket,
                corner: window.corner,
                viewport,
                inv_view_proj,
                origin,
                slot_ids,
                submission: Some(submission),
            });
        }
        ticket
    }

    /// Submission index of a pending pick.
    pub fn submission_of(&self, ticket: PickTicket) -> Option<wgpu::SubmissionIndex> {
        self.slots
            .iter()
            .filter_map(|s| s.pending.as_ref())
            .find(|p| p.ticket == ticket)
            .and_then(|p| p.submission.clone())
    }

    /// Collects finished readbacks into the completed queue. Call after `device.poll`.
    pub fn collect(&mut self) {
        let size = self.size;
        for s in &mut self.slots {
            let state = s.state.load(Ordering::Acquire);
            if state != READY && state != FAILED {
                continue;
            }
            let Some(p) = s.pending.take() else {
                s.state.store(IDLE, Ordering::Release);
                continue;
            };
            let result = if state == READY {
                let r = match s.buffer.get_mapped_range(..) {
                    Ok(view) => Some(decode(&view, size, &p)),
                    Err(_) => None,
                };
                s.buffer.unmap();
                r
            } else {
                None
            };
            s.state.store(IDLE, Ordering::Release);
            let result = result.unwrap_or(PickResult {
                id: None,
                position: None,
                pixel: [
                    p.corner[0] as f64 + f64::from(size / 2) + 0.5,
                    p.corner[1] as f64 + f64::from(size / 2) + 0.5,
                ],
                depth: None,
            });
            self.completed.push_back((p.ticket, result));
        }
    }

    /// Takes every completed pick.
    pub fn drain(&mut self) -> Vec<(PickTicket, PickResult)> {
        self.completed.drain(..).collect()
    }

    /// Takes one completed pick by ticket.
    pub fn take(&mut self, ticket: PickTicket) -> Option<PickResult> {
        let i = self.completed.iter().position(|(t, _)| *t == ticket)?;
        self.completed.remove(i).map(|(_, r)| r)
    }

    /// Whether a ticket is still waiting for the GPU.
    pub fn is_pending(&self, ticket: PickTicket) -> bool {
        self.slots.iter().any(|s| s.pending.as_ref().is_some_and(|p| p.ticket == ticket))
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    match bytes.get(offset..offset + 4) {
        Some(&[a, b, c, d]) => u32::from_le_bytes([a, b, c, d]),
        _ => 0,
    }
}

fn decode(bytes: &[u8], size: u32, p: &PendingPick) -> PickResult {
    let n = size as usize;
    let pitch = ROW_PITCH as usize;
    let depth_base = pitch * n;
    let mut ids = Vec::with_capacity(n * n);
    let mut depths = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            ids.push(read_u32(bytes, y * pitch + x * 4));
            depths.push(f32::from_bits(read_u32(bytes, depth_base + y * pitch + x * 4)));
        }
    }
    let centre =
        [p.corner[0] as f64 + f64::from(size / 2) + 0.5, p.corner[1] as f64 + f64::from(size / 2) + 0.5];
    let Some(i) = best_hit(&ids, &depths, size) else {
        return PickResult { id: None, position: None, pixel: centre, depth: None };
    };
    let slot = ids.get(i).copied().unwrap_or(0).saturating_sub(1);
    let id = p.slot_ids.get(slot as usize).copied().flatten();
    let depth = depths.get(i).copied();
    let pixel = [p.corner[0] as f64 + (i % n) as f64 + 0.5, p.corner[1] as f64 + (i / n) as f64 + 0.5];
    let position = match (p.inv_view_proj, depth) {
        (Some(inv), Some(d)) => {
            let [nx, ny] = p.viewport.to_ndc(pixel[0], pixel[1]);
            inv.transform_point(Point3::new(nx, ny, f64::from(d))).map(|rel| p.origin + rel.to_vec())
        }
        _ => None,
    };
    PickResult { id, position, pixel, depth }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_hit_wins() {
        let mut ids = vec![0u32; 9];
        let depths = vec![0.5f32; 9];
        ids[4] = 3;
        ids[0] = 7;
        assert_eq!(best_hit(&ids, &depths, 3), Some(4));
    }

    #[test]
    fn nearest_hit_when_centre_is_background() {
        let mut ids = vec![0u32; 25];
        let mut depths = vec![0.0f32; 25];
        ids[0] = 1; // corner, distance² 8
        ids[7] = 2; // (2,1) → offset (0,-1), distance² 1
        ids[11] = 3; // (1,2) → offset (-1,0), distance² 1, nearer surface
        depths[7] = 0.2;
        depths[11] = 0.6;
        assert_eq!(best_hit(&ids, &depths, 5), Some(11));
        assert_eq!(best_hit(&[0; 25], &[0.0; 25], 5), None);
    }

    #[test]
    fn window_maps_cursor_pixel_to_centre_texel() {
        let vp = Viewport::new(800.0, 600.0);
        let w = pick_window([400.7, 120.2], vp, 7);
        assert_eq!(w.corner, [397, 117]);
        // The cursor pixel centre maps to the clip-space centre of the window.
        let [nx, ny] = vp.to_ndc(400.5, 120.5);
        let p = w.narrow.transform_point(Point3::new(nx, ny, 0.5)).unwrap();
        assert!(p.x.abs() < 1e-9 && p.y.abs() < 1e-9 && (p.z - 0.5).abs() < 1e-12);
        // The left edge of the window maps to −1.
        let [ex, _] = vp.to_ndc(397.0, 120.5);
        let e = w.narrow.transform_point(Point3::new(ex, ny, 0.5)).unwrap();
        assert!((e.x + 1.0).abs() < 1e-9);
    }
}
