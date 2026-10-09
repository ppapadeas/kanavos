//! The viewport renderer.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use kanavos_geom::{Bbox3, Point3, Vec3};

use crate::RenderError;
use crate::camera::{Projection, Viewport};
use crate::gpu::{
    FLAG_EDGES, FLAG_SILHOUETTES, FrameUniforms, Pipelines, ST_GHOST, ST_HIDDEN, ST_HOVER, ST_SELECTED,
    STATE_TEXTURE_WIDTH, Target, Targets,
};
use crate::headless::{Image, read_rgba8};
use crate::math::Mat4;
use crate::pick::{PickResult, PickTicket, Picker, pick_window};
use crate::prep::cut_segments;
use crate::scene::{Scene, SceneMesh};
use crate::store::{GrowBuffer, Store};
use crate::view::{MAX_CLIP_PLANES, ViewParams, linear_rgba, plane_to_render_space};

/// Fixed settings chosen when the renderer is created.
#[derive(Debug, Clone, PartialEq)]
pub struct RendererConfig {
    /// Multisample count: 1 (off) or 4. Other values are rounded to one of these.
    pub msaa_samples: u32,
    /// Faces meeting at more than this angle (degrees) get a hard edge: separate normals and an edge line.
    pub crease_angle_deg: f64,
    /// Pick tolerance in physical pixels: the nearest object within this radius of the cursor is picked.
    pub pick_radius: u32,
    /// Size of the square sun shadow map in texels.
    pub shadow_map_size: u32,
}

impl Default for RendererConfig {
    fn default() -> Self {
        RendererConfig { msaa_samples: 4, crease_angle_deg: 20.0, pick_radius: 3, shadow_map_size: 2048 }
    }
}

/// Counters for the last frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Objects in the scene.
    pub objects: usize,
    /// Triangles in the scene.
    pub triangles: usize,
    /// Edge segments in the scene (feature edges and silhouette candidates).
    pub edges: usize,
    /// Draw calls issued.
    pub draw_calls: usize,
    /// Transparent meshes drawn.
    pub transparent_meshes: usize,
    /// Geometry chunks.
    pub chunks: usize,
    /// Whether the shadow map was re-rendered this frame.
    pub shadow_updated: bool,
}

/// The rendered image of one frame.
#[derive(Debug, Clone)]
pub struct RenderOutput {
    /// The image (format `Rgba8UnormSrgb`, usable as a texture and as a copy source).
    pub texture: wgpu::Texture,
    /// An `Rgba8Unorm` view of the image: sampling returns gamma-encoded values, which is what egui expects.
    /// Register this view with `egui_wgpu::Renderer::register_native_texture`.
    pub view: wgpu::TextureView,
    /// Size in physical pixels.
    pub size: [u32; 2],
    /// Changes whenever the texture is recreated (resize). When it differs from the value seen last frame,
    /// update the egui registration with `update_egui_texture_from_wgpu_texture`.
    pub generation: u64,
    /// Counters.
    pub stats: RenderStats,
}

/// What [`Renderer::set_scene`] changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneUpdate {
    /// Objects added or whose meshes changed.
    pub changed: usize,
    /// Objects removed.
    pub removed: usize,
    /// Objects left as they were.
    pub unchanged: usize,
}

/// Values derived from the view parameters for one frame.
struct FrameSetup {
    uniforms: FrameUniforms,
    /// World (render space) → clip.
    view_proj: Mat4,
    /// Eye in render space.
    eye: [f32; 3],
    /// Viewing direction.
    forward: [f32; 3],
    /// Shadow-map cache key, when shadows are on.
    shadow_key: Option<u64>,
    /// Cut-line cache key, when clip planes are active.
    cut_key: Option<u64>,
    /// Active clip planes in render space.
    clip_planes: Vec<[f32; 4]>,
    /// Whether ambient occlusion runs.
    ao: bool,
}

/// Outlines along the clip planes, recomputed when the planes, the geometry or the visibility change.
#[derive(Debug, Default)]
struct CutLines {
    key: Option<u64>,
    buffer: Option<GrowBuffer>,
    count: u32,
}

/// GPU renderer for a 3D viewport.
///
/// One renderer serves one viewport. It owns copies of the scene meshes on the GPU, the per-object display state
/// (selection, hover, hidden, ghosted) and offscreen render targets sized to the viewport.
#[derive(Debug)]
pub struct Renderer {
    config: RendererConfig,
    msaa: u32,
    cos_crease: f64,
    pipelines: Pipelines,
    frame_buffer: wgpu::Buffer,
    pick_buffer: wgpu::Buffer,
    shadow_sampler: wgpu::Sampler,
    shadow_map: Target,
    dummy_depth: Target,
    state_texture: Target,
    state_rows: u32,
    scene_bind_group: wgpu::BindGroup,
    shadow_bind_group: wgpu::BindGroup,
    pick_bind_group: wgpu::BindGroup,
    targets: Option<Targets>,
    generation: u64,
    shadow_key: Option<u64>,
    cut_lines: CutLines,
    store: Store,
    origin: Point3,
    selection: HashSet<u64>,
    hovered: Option<u64>,
    hidden: HashSet<u64>,
    ghosted: HashSet<u64>,
    hidden_categories: HashSet<u32>,
    flags_dirty: bool,
    visibility_revision: u64,
    picker: Picker,
    max_texture_size: u32,
}

fn state_texture(device: &wgpu::Device, rows: u32) -> Target {
    Target::new(
        device,
        "kanavos object state",
        [STATE_TEXTURE_WIDTH, rows.max(1)],
        wgpu::TextureFormat::R32Uint,
        1,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        &[],
    )
}

fn make_scene_bind_group(
    device: &wgpu::Device,
    pipelines: &Pipelines,
    uniforms: &wgpu::Buffer,
    state: &wgpu::TextureView,
    shadow: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout: &pipelines.scene_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(state) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(shadow) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn uniform_buffer(device: &wgpu::Device, label: &str) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: std::mem::size_of::<FrameUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn to_world(origin: Point3, b: Option<([f32; 3], [f32; 3])>) -> Option<Bbox3> {
    let (lo, hi) = b?;
    let p = |c: [f32; 3]| Point3::new(f64::from(c[0]), f64::from(c[1]), f64::from(c[2])) + origin.to_vec();
    Some(Bbox3 { min: p(lo), max: p(hi) })
}

fn hash_words(words: impl IntoIterator<Item = u64>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for w in words {
        h ^= w;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

impl Renderer {
    /// Creates a renderer on a device. Use the device and queue that egui uses (`RenderState::device`) so that
    /// the output texture can be shown directly.
    ///
    /// Fails when the device rejects the shaders (for example with a very old graphics driver); the interface
    /// should then explain that the 3D view is unavailable and carry on.
    pub fn new(device: &wgpu::Device, config: RendererConfig) -> Result<Renderer, RenderError> {
        let msaa = if config.msaa_samples > 1 { 4 } else { 1 };
        let pipelines = Pipelines::new(device, msaa).map_err(RenderError::Gpu)?;
        let frame_buffer = uniform_buffer(device, "kanavos frame uniforms");
        let pick_buffer = uniform_buffer(device, "kanavos pick uniforms");
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kanavos shadow sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..wgpu::SamplerDescriptor::default()
        });
        let max_texture_size = device.limits().max_texture_dimension_2d.max(1);
        let shadow_size = config.shadow_map_size.clamp(256, max_texture_size.min(8192));
        let shadow_map = Target::new(
            device,
            "kanavos shadow map",
            [shadow_size, shadow_size],
            crate::gpu::DEPTH_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            &[],
        );
        let dummy_depth = Target::new(
            device,
            "kanavos placeholder depth",
            [1, 1],
            crate::gpu::DEPTH_FORMAT,
            1,
            wgpu::TextureUsages::TEXTURE_BINDING,
            &[],
        );
        let state_rows = 1;
        let state = state_texture(device, state_rows);
        let scene_bind_group = make_scene_bind_group(
            device,
            &pipelines,
            &frame_buffer,
            &state.view,
            &shadow_map.view,
            &shadow_sampler,
            "kanavos scene",
        );
        let shadow_bind_group = make_scene_bind_group(
            device,
            &pipelines,
            &frame_buffer,
            &state.view,
            &dummy_depth.view,
            &shadow_sampler,
            "kanavos shadow pass",
        );
        let pick_bind_group = make_scene_bind_group(
            device,
            &pipelines,
            &pick_buffer,
            &state.view,
            &dummy_depth.view,
            &shadow_sampler,
            "kanavos pick",
        );
        let picker = Picker::new(device, config.pick_radius);
        let cos_crease = config.crease_angle_deg.clamp(0.0, 180.0).to_radians().cos();
        Ok(Renderer {
            config,
            msaa,
            cos_crease,
            pipelines,
            frame_buffer,
            pick_buffer,
            shadow_sampler,
            shadow_map,
            dummy_depth,
            state_texture: state,
            state_rows,
            scene_bind_group,
            shadow_bind_group,
            pick_bind_group,
            targets: None,
            generation: 0,
            shadow_key: None,
            cut_lines: CutLines::default(),
            store: Store::default(),
            origin: Point3::ORIGIN,
            selection: HashSet::new(),
            hovered: None,
            hidden: HashSet::new(),
            ghosted: HashSet::new(),
            hidden_categories: HashSet::new(),
            flags_dirty: true,
            visibility_revision: 0,
            picker,
            max_texture_size,
        })
    }

    /// The configuration.
    pub fn config(&self) -> &RendererConfig {
        &self.config
    }

    /// Whether this device supports the ambient occlusion passes ([`crate::DisplayStyle::ambient_occlusion`]
    /// is ignored otherwise).
    pub fn ambient_occlusion_supported(&self) -> bool {
        self.pipelines.post.is_some()
    }

    // -----------------------------------------------------------------------------------------------------
    // Scene content

    /// World position that mesh coordinates are relative to.
    pub fn origin(&self) -> Point3 {
        self.origin
    }

    /// Replaces the scene, updating only what changed: objects (meshes grouped by id) whose content is identical
    /// to what the renderer already holds are kept as they are. A different origin replaces everything.
    pub fn set_scene(&mut self, scene: &Scene) -> SceneUpdate {
        if (scene.origin - self.origin).length() > 1e-9 {
            self.store.clear();
            self.origin = scene.origin;
            self.flags_dirty = true;
        }
        // Group meshes by id, keeping the order in which ids first appear.
        let mut index: HashMap<u64, usize> = HashMap::new();
        let mut groups: Vec<(u64, Vec<&SceneMesh>)> = Vec::new();
        for m in &scene.meshes {
            match index.get(&m.id) {
                Some(&i) => {
                    if let Some(g) = groups.get_mut(i) {
                        g.1.push(m);
                    }
                }
                None => {
                    index.insert(m.id, groups.len());
                    groups.push((m.id, vec![m]));
                }
            }
        }
        let stale: Vec<u64> =
            self.store.objects.keys().filter(|id| !index.contains_key(id)).copied().collect();
        let mut update = SceneUpdate::default();
        for id in stale {
            if self.store.remove(id) {
                update.removed += 1;
            }
        }
        let (changed, unchanged) = self.store.upsert_many(&groups, self.cos_crease);
        update.changed = changed;
        update.unchanged = unchanged;
        if update.changed + update.removed > 0 {
            self.flags_dirty = true;
            self.visibility_revision += 1;
        }
        update
    }

    /// Adds or replaces one object (all meshes with this id; their `id` fields are ignored). An empty slice
    /// removes it. Returns whether anything changed. Positions must be relative to [`Renderer::origin`].
    pub fn update_object(&mut self, id: u64, meshes: &[SceneMesh]) -> bool {
        let refs: Vec<&SceneMesh> = meshes.iter().collect();
        let changed = self.store.upsert(id, &refs, self.cos_crease);
        if changed {
            self.flags_dirty = true;
            self.visibility_revision += 1;
        }
        changed
    }

    /// Removes one object. Returns whether it existed.
    pub fn remove_object(&mut self, id: u64) -> bool {
        let removed = self.store.remove(id);
        if removed {
            self.flags_dirty = true;
            self.visibility_revision += 1;
        }
        removed
    }

    /// Removes every object (display state such as the selection is kept).
    pub fn clear(&mut self) {
        self.store.clear();
        self.flags_dirty = true;
        self.visibility_revision += 1;
    }

    /// Whether an object is in the scene.
    pub fn contains(&self, id: u64) -> bool {
        self.store.objects.contains_key(&id)
    }

    /// Number of objects.
    pub fn object_count(&self) -> usize {
        self.store.objects.len()
    }

    /// Ids of all objects, in no particular order.
    pub fn object_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.store.objects.keys().copied()
    }

    // -----------------------------------------------------------------------------------------------------
    // Bounds

    /// Bounds of every object, world coordinates.
    pub fn scene_bounds(&self) -> Option<Bbox3> {
        to_world(self.origin, self.store.bounds_where(|_, _| true))
    }

    /// Bounds of the objects that are not hidden, world coordinates.
    pub fn visible_bounds(&self) -> Option<Bbox3> {
        to_world(self.origin, self.store.bounds_where(|id, r| self.visible(id, r.category)))
    }

    /// Bounds of some objects (for "zoom to selection"), world coordinates.
    pub fn bounds_of(&self, ids: impl IntoIterator<Item = u64>) -> Option<Bbox3> {
        let wanted: HashSet<u64> = ids.into_iter().collect();
        to_world(self.origin, self.store.bounds_where(|id, _| wanted.contains(&id)))
    }

    // -----------------------------------------------------------------------------------------------------
    // Display state

    fn visible(&self, id: u64, category: u32) -> bool {
        !self.hidden.contains(&id) && !self.hidden_categories.contains(&category)
    }

    /// Whether an object exists and is not hidden (directly or by category).
    pub fn is_visible(&self, id: u64) -> bool {
        self.store.objects.get(&id).is_some_and(|r| self.visible(id, r.category))
    }

    /// Replaces the selection (highlighted with a tint and thicker edges in the selection colour).
    pub fn set_selection(&mut self, ids: impl IntoIterator<Item = u64>) {
        let s: HashSet<u64> = ids.into_iter().collect();
        if s != self.selection {
            self.selection = s;
            self.flags_dirty = true;
        }
    }

    /// The selected ids.
    pub fn selection(&self) -> &HashSet<u64> {
        &self.selection
    }

    /// Sets the object under the pointer (highlighted in the hover colour).
    pub fn set_hovered(&mut self, id: Option<u64>) {
        if id != self.hovered {
            self.hovered = id;
            self.flags_dirty = true;
        }
    }

    /// The hovered id.
    pub fn hovered(&self) -> Option<u64> {
        self.hovered
    }

    /// Replaces the set of hidden objects (not drawn, not pickable, no shadows).
    pub fn set_hidden(&mut self, ids: impl IntoIterator<Item = u64>) {
        let s: HashSet<u64> = ids.into_iter().collect();
        if s != self.hidden {
            self.hidden = s;
            self.flags_dirty = true;
            self.visibility_revision += 1;
        }
    }

    /// Replaces the set of ghosted objects (drawn faint and desaturated, for example storeys other than the
    /// active one; not pickable, no shadows).
    pub fn set_ghosted(&mut self, ids: impl IntoIterator<Item = u64>) {
        let s: HashSet<u64> = ids.into_iter().collect();
        if s != self.ghosted {
            self.ghosted = s;
            self.flags_dirty = true;
            self.visibility_revision += 1;
        }
    }

    /// Hides or shows every object of a category.
    pub fn set_category_hidden(&mut self, category: u32, hidden: bool) {
        let changed = if hidden {
            self.hidden_categories.insert(category)
        } else {
            self.hidden_categories.remove(&category)
        };
        if changed {
            self.flags_dirty = true;
            self.visibility_revision += 1;
        }
    }

    // -----------------------------------------------------------------------------------------------------
    // GPU upload

    /// Uploads pending scene and display-state changes. Called by [`Renderer::render`] and
    /// [`Renderer::request_pick`]; call it yourself to move upload work elsewhere in the frame.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.store.upload(device, queue);
        let needed = self.store.slot_capacity().max(1) as u32;
        let rows = needed.div_ceil(STATE_TEXTURE_WIDTH).max(1);
        if rows > self.state_rows {
            let rows = rows.next_power_of_two().min(self.max_texture_size);
            self.state_rows = rows;
            self.state_texture = state_texture(device, rows);
            self.rebuild_bind_groups(device);
            self.flags_dirty = true;
        }
        if self.flags_dirty {
            self.flags_dirty = false;
            let mut words = vec![ST_HIDDEN; (STATE_TEXTURE_WIDTH * self.state_rows) as usize];
            for (slot, entry) in self.store.slot_ids.iter().enumerate() {
                let Some(id) = entry else { continue };
                let category = self.store.objects.get(id).map(|r| r.category).unwrap_or(0);
                let mut bits = 0;
                if !self.visible(*id, category) {
                    bits |= ST_HIDDEN;
                }
                if self.ghosted.contains(id) {
                    bits |= ST_GHOST;
                }
                if self.selection.contains(id) {
                    bits |= ST_SELECTED;
                }
                if self.hovered == Some(*id) {
                    bits |= ST_HOVER;
                }
                if let Some(w) = words.get_mut(slot) {
                    *w = bits;
                }
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.state_texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&words),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(STATE_TEXTURE_WIDTH * 4),
                    rows_per_image: Some(self.state_rows),
                },
                wgpu::Extent3d {
                    width: STATE_TEXTURE_WIDTH,
                    height: self.state_rows,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn rebuild_bind_groups(&mut self, device: &wgpu::Device) {
        let state = &self.state_texture.view;
        self.scene_bind_group = make_scene_bind_group(
            device,
            &self.pipelines,
            &self.frame_buffer,
            state,
            &self.shadow_map.view,
            &self.shadow_sampler,
            "kanavos scene",
        );
        self.shadow_bind_group = make_scene_bind_group(
            device,
            &self.pipelines,
            &self.frame_buffer,
            state,
            &self.dummy_depth.view,
            &self.shadow_sampler,
            "kanavos shadow pass",
        );
        self.pick_bind_group = make_scene_bind_group(
            device,
            &self.pipelines,
            &self.pick_buffer,
            state,
            &self.dummy_depth.view,
            &self.shadow_sampler,
            "kanavos pick",
        );
    }

    fn clamp_size(&self, size: [u32; 2]) -> [u32; 2] {
        size.map(|s| s.clamp(1, self.max_texture_size))
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        self.targets = Some(Targets::new(device, &self.pipelines, &self.frame_buffer, size, self.msaa));
        self.generation += 1;
    }

    // -----------------------------------------------------------------------------------------------------
    // Frame setup

    fn frame_setup(&self, params: &ViewParams, size: [u32; 2]) -> FrameSetup {
        let style = &params.style;
        let camera = &params.camera;
        let viewport = Viewport::new(f64::from(size[0]), f64::from(size[1]));
        let bounds = self.visible_bounds();
        let depth = camera.depth_range(bounds.as_ref());
        let view = camera.view_matrix_relative(self.origin);
        let proj = camera.projection_matrix(viewport.aspect(), depth);
        let view_proj = proj.mul(&view);
        let inv_proj = proj.inverse().unwrap_or(Mat4::IDENTITY);
        let basis = camera.basis();
        let eye = camera.eye() - self.origin;
        let perspective = camera.projection == Projection::Perspective;
        let f32x3 = |v: Vec3| [v.x as f32, v.y as f32, v.z as f32];
        let lin = |c| linear_rgba(c);
        let ppp =
            if params.pixels_per_point.is_finite() { params.pixels_per_point.clamp(0.25, 8.0) } else { 1.0 };

        // Sun and shadows.
        let sun = style.sun.direction();
        let render_bounds = self.store.bounds_where(|id, r| self.visible(id, r.category));
        let shadows_on = style.shadows && style.sun.intensity > 0.0 && render_bounds.is_some();
        let (light_view_proj, texel_world) = match render_bounds {
            Some(b) if shadows_on => light_matrix(b, sun, self.shadow_map.texture.width()),
            _ => (Mat4::IDENTITY, 0.0),
        };
        let shadow_size = self.shadow_map.texture.width().max(1);

        // Grid: spacing follows the zoom level; lines align with whole world coordinates.
        let level = (camera.distance.max(0.1) / 6.0).log10();
        let minor = 10f64.powf(level.floor());
        let major = minor * 10.0;
        let minor_strength = 1.0 - (level - level.floor());
        let half = (camera.distance * 4.0).clamp(25.0, 20_000.0);
        let target = camera.target - self.origin;

        let mut clip = [[0.0f32; 4]; MAX_CLIP_PLANES];
        let mut clip_count = 0u32;
        for (dst, plane) in clip.iter_mut().zip(params.clip_planes.iter()) {
            *dst = plane_to_render_space(plane, self.origin);
            clip_count += 1;
        }
        let mut flags = 0;
        if style.edges {
            flags |= FLAG_EDGES;
        }
        if style.silhouettes {
            flags |= FLAG_SILHOUETTES;
        }
        let ao = style.ambient_occlusion
            && style.ao_strength > 0.0
            && style.ao_radius > 0.0
            && self.pipelines.post.is_some();
        let pixels_per_metre = proj.at(1, 1) * f64::from(size[1]) * 0.5;

        let uniforms = FrameUniforms {
            view_proj: view_proj.to_cols_f32(),
            view: view.to_cols_f32(),
            proj: proj.to_cols_f32(),
            inv_proj: inv_proj.to_cols_f32(),
            light_view_proj: light_view_proj.to_cols_f32(),
            eye: [eye.x as f32, eye.y as f32, eye.z as f32, if perspective { 1.0 } else { 0.0 }],
            forward: {
                let f = f32x3(basis.forward);
                [f[0], f[1], f[2], depth.near as f32]
            },
            viewport: {
                let (w, h) = (size[0] as f32, size[1] as f32);
                [w, h, 1.0 / w, 1.0 / h]
            },
            sun_dir: {
                let s = f32x3(sun);
                [s[0], s[1], s[2], style.sun.intensity.max(0.0)]
            },
            sun_color: lin(style.sun_color),
            sky_color: lin(style.sky_color),
            ground_color: lin(style.ground_color),
            bg_top: lin(style.background_top),
            bg_bottom: lin(style.background_bottom),
            edge_color: lin(style.edge_color),
            select_color: lin(style.selection_color),
            hover_color: lin(style.hover_color),
            cut_color: lin(style.cut_color),
            grid_color: lin(style.grid_color),
            grid_a: [
                target.x as f32,
                target.y as f32,
                half as f32,
                (style.grid_elevation - self.origin.z) as f32,
            ],
            grid_b: [
                self.origin.x.rem_euclid(major) as f32,
                self.origin.y.rem_euclid(major) as f32,
                minor as f32,
                minor_strength as f32,
            ],
            edge_params: [
                (style.edge_width.max(0.25) * ppp),
                0.0015,
                (camera.view_height() * 0.002) as f32,
                6.0 * ppp,
            ],
            shadow_params: [
                (texel_world * 1.5) as f32,
                style.shadow_strength.clamp(0.0, 1.0),
                1.0 / shadow_size as f32,
                if shadows_on { 1.0 } else { 0.0 },
            ],
            light_params: [
                style.ambient.max(0.0),
                style.fill.max(0.0),
                style.ghost_alpha.clamp(0.0, 1.0),
                0.35,
            ],
            ao_params: [
                style.ao_radius.max(0.0),
                style.ao_strength.clamp(0.0, 1.0),
                pixels_per_metre as f32,
                if ao { 1.0 } else { 0.0 },
            ],
            clip,
            counts: [clip_count, flags, STATE_TEXTURE_WIDTH, 0],
        };
        let shadow_key = shadows_on.then(|| {
            let mut words = vec![
                self.store.revision,
                self.visibility_revision,
                u64::from(shadow_size),
                sun.x.to_bits(),
                sun.y.to_bits(),
                sun.z.to_bits(),
            ];
            for c in clip.iter().take(clip_count as usize) {
                words.extend(c.iter().map(|v| u64::from(v.to_bits())));
            }
            hash_words(words)
        });
        let clip_planes: Vec<[f32; 4]> = clip.iter().take(clip_count as usize).copied().collect();
        let cut_key = (!clip_planes.is_empty()).then(|| {
            let mut words = vec![self.store.revision, self.visibility_revision];
            words.extend(clip_planes.iter().flatten().map(|v| u64::from(v.to_bits())));
            hash_words(words)
        });
        FrameSetup {
            uniforms,
            view_proj,
            eye: [eye.x as f32, eye.y as f32, eye.z as f32],
            forward: f32x3(basis.forward),
            shadow_key,
            cut_key,
            clip_planes,
            ao,
        }
    }

    /// Recomputes the outlines along the clip planes when they are out of date. Only objects whose bounds
    /// straddle a plane are examined.
    fn update_cut_lines(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, setup: &FrameSetup) {
        if setup.cut_key.is_none() || setup.cut_key == self.cut_lines.key {
            return;
        }
        self.cut_lines.key = setup.cut_key;
        let mut lines = Vec::new();
        for (id, record) in &self.store.objects {
            if !self.visible(*id, record.category) || self.ghosted.contains(id) {
                continue;
            }
            let Some((lo, hi)) = record.bounds else { continue };
            for plane in &setup.clip_planes {
                let [nx, ny, nz, d] = *plane;
                // Signed distances of the box extremes along the plane normal.
                let pick = |n: f32, l: f32, h: f32| if n >= 0.0 { (n * l, n * h) } else { (n * h, n * l) };
                let (ax, bx) = pick(nx, lo[0], hi[0]);
                let (ay, by) = pick(ny, lo[1], hi[1]);
                let (az, bz) = pick(nz, lo[2], hi[2]);
                if ax + ay + az + d > 0.0 || bx + by + bz + d < 0.0 {
                    continue;
                }
                for part in record.parts.iter().filter(|p| !p.transparent) {
                    cut_segments(part, *plane, record.slot, &mut lines);
                }
            }
        }
        GrowBuffer::write(
            &mut self.cut_lines.buffer,
            device,
            queue,
            "kanavos cut lines",
            wgpu::BufferUsages::VERTEX,
            bytemuck::cast_slice(&lines),
        );
        self.cut_lines.count = lines.len() as u32;
    }

    // -----------------------------------------------------------------------------------------------------
    // Rendering

    /// Renders a frame into the renderer's own texture and submits the work. Returns the image to display.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &ViewParams,
    ) -> RenderOutput {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kanavos frame") });
        let out = self.encode(device, queue, &mut encoder, params);
        queue.submit([encoder.finish()]);
        out
    }

    /// Records a frame into `encoder` without submitting it. Uniforms and uploads are written through `queue`
    /// and take effect at the next submission, so record at most one frame per submission.
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        params: &ViewParams,
    ) -> RenderOutput {
        self.prepare(device, queue);
        let size = self.clamp_size(params.size);
        self.ensure_targets(device, size);
        let setup = self.frame_setup(params, size);
        queue.write_buffer(&self.frame_buffer, 0, bytemuck::bytes_of(&setup.uniforms));
        self.update_cut_lines(device, queue, &setup);

        let mut stats = RenderStats {
            objects: self.store.objects.len(),
            chunks: self
                .store
                .chunks
                .iter()
                .filter(|c| c.gpu.index_count > 0 || c.gpu.edge_count > 0)
                .count(),
            ..RenderStats::default()
        };
        let (triangles, edges) = self.store.totals();
        stats.triangles = triangles;
        stats.edges = edges;

        // Sun shadow map, re-rendered only when something that affects it changed.
        if setup.shadow_key.is_some() && setup.shadow_key != self.shadow_key {
            self.shadow_key = setup.shadow_key;
            stats.shadow_updated = true;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kanavos shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_map.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.shadow);
            pass.set_bind_group(0, &self.shadow_bind_group, &[]);
            stats.draw_calls += self.draw_opaque_chunks(&mut pass, None);
        }

        let Some(targets) = self.targets.as_ref() else {
            // Unreachable: ensure_targets always creates them.
            return self.placeholder_output(device, size, stats);
        };

        // Ambient occlusion runs when requested and supported; the main pass then resolves into an intermediate
        // target that the composite pass darkens into the output.
        let post = if setup.ao { targets.post.as_ref().zip(self.pipelines.post.as_ref()) } else { None };

        // Main pass.
        {
            let resolve = match post {
                Some((pt, _)) => &pt.scene.view,
                None => &targets.output.view,
            };
            let color = match &targets.color_ms {
                Some(ms) => wgpu::RenderPassColorAttachment {
                    view: &ms.view,
                    depth_slice: None,
                    resolve_target: Some(resolve),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Discard,
                    },
                },
                None => wgpu::RenderPassColorAttachment {
                    view: resolve,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                },
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kanavos main pass"),
                color_attachments: &[Some(color)],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: if post.is_some() { wgpu::StoreOp::Store } else { wgpu::StoreOp::Discard },
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_pipeline(&self.pipelines.background);
            pass.draw(0..3, 0..1);
            stats.draw_calls += 1;

            pass.set_pipeline(&self.pipelines.opaque);
            stats.draw_calls += self.draw_opaque_chunks(&mut pass, Some(&setup.view_proj));

            if params.style.grid {
                pass.set_pipeline(&self.pipelines.grid);
                pass.draw(0..6, 0..1);
                stats.draw_calls += 1;
            }

            let highlight = !self.selection.is_empty() || self.hovered.is_some();
            if params.style.edges || params.style.silhouettes || highlight {
                pass.set_pipeline(&self.pipelines.edges);
                stats.draw_calls += self.draw_edges(&mut pass, &setup.view_proj);
            }
            // Outlines of the cut, while clipping.
            if setup.cut_key.is_some()
                && let Some(cb) = &self.cut_lines.buffer
                && self.cut_lines.count > 0
            {
                pass.set_pipeline(&self.pipelines.edges);
                pass.set_vertex_buffer(0, cb.buffer.slice(..cb.len));
                pass.draw(0..6, 0..self.cut_lines.count);
                stats.draw_calls += 1;
            }
            if params.style.show_hidden_selection && !self.selection.is_empty() {
                pass.set_pipeline(&self.pipelines.edges_occluded);
                stats.draw_calls += self.draw_edges(&mut pass, &setup.view_proj);
            }
            if !self.ghosted.is_empty() {
                pass.set_pipeline(&self.pipelines.ghost);
                stats.draw_calls += self.draw_opaque_chunks(&mut pass, Some(&setup.view_proj));
            }

            // Transparent meshes, back to front.
            let t = &self.store.transparent;
            if let (Some(vb), Some(ib)) = (&t.vertices, &t.indices)
                && !t.draws.is_empty()
            {
                let mut order: Vec<(f32, usize)> = t
                    .draws
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        let v = [
                            d.centre[0] - setup.eye[0],
                            d.centre[1] - setup.eye[1],
                            d.centre[2] - setup.eye[2],
                        ];
                        (v[0] * setup.forward[0] + v[1] * setup.forward[1] + v[2] * setup.forward[2], i)
                    })
                    .collect();
                order.sort_by(|a, b| b.0.total_cmp(&a.0));
                pass.set_pipeline(&self.pipelines.transparent);
                pass.set_vertex_buffer(0, vb.buffer.slice(..vb.len));
                pass.set_index_buffer(ib.buffer.slice(..ib.len), wgpu::IndexFormat::Uint32);
                for (_, i) in order {
                    if let Some(d) = t.draws.get(i) {
                        pass.draw_indexed(d.first_index..d.first_index + d.index_count, 0, 0..1);
                        stats.draw_calls += 1;
                        stats.transparent_meshes += 1;
                    }
                }
            }
        }

        // Ambient occlusion and composite.
        if let Some((pt, pp)) = post {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("kanavos ambient occlusion"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &pt.ao.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&pp.ao);
                pass.set_bind_group(0, &pt.ao_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kanavos composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.output.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pp.composite);
            pass.set_bind_group(0, &pt.composite_bind_group, &[]);
            pass.draw(0..3, 0..1);
            stats.draw_calls += 2;
        }

        RenderOutput {
            texture: targets.output.texture.clone(),
            view: targets.output_unorm.clone(),
            size,
            generation: self.generation,
            stats,
        }
    }

    fn placeholder_output(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
        stats: RenderStats,
    ) -> RenderOutput {
        let t = Targets::new(device, &self.pipelines, &self.frame_buffer, size, self.msaa);
        let out = RenderOutput {
            texture: t.output.texture.clone(),
            view: t.output_unorm.clone(),
            size,
            generation: self.generation + 1,
            stats,
        };
        self.targets = Some(t);
        self.generation += 1;
        out
    }

    /// Draws the opaque geometry of every chunk that may be inside `cull` (all chunks when `None`).
    fn draw_opaque_chunks(&self, pass: &mut wgpu::RenderPass<'_>, cull: Option<&Mat4>) -> usize {
        let mut calls = 0;
        for chunk in &self.store.chunks {
            let g = &chunk.gpu;
            if let (Some(vb), Some(ib)) = (&g.vertices, &g.indices)
                && g.index_count > 0
                && cull.is_none_or(|m| chunk.may_be_visible(m))
            {
                pass.set_vertex_buffer(0, vb.buffer.slice(..vb.len));
                pass.set_index_buffer(ib.buffer.slice(..ib.len), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..g.index_count, 0, 0..1);
                calls += 1;
            }
        }
        calls
    }

    /// Draws the edges of every chunk that may be inside `cull`.
    fn draw_edges(&self, pass: &mut wgpu::RenderPass<'_>, cull: &Mat4) -> usize {
        let mut calls = 0;
        for chunk in &self.store.chunks {
            let g = &chunk.gpu;
            if let Some(eb) = &g.edges
                && g.edge_count > 0
                && chunk.may_be_visible(cull)
            {
                pass.set_vertex_buffer(0, eb.buffer.slice(..eb.len));
                pass.draw(0..6, 0..g.edge_count);
                calls += 1;
            }
        }
        calls
    }

    /// Renders a frame and reads it back into memory (blocking). Intended for tests, thumbnails and headless
    /// export, not for interactive use.
    pub fn render_to_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &ViewParams,
    ) -> Result<Image, RenderError> {
        let out = self.render(device, queue, params);
        read_rgba8(device, queue, &out.texture)
    }

    // -----------------------------------------------------------------------------------------------------
    // Picking

    /// Starts an asynchronous pick at a pixel position (physical pixels, top-left origin, same size as
    /// `params.size`). The result arrives through [`Renderer::poll_picks`], usually one or two frames later.
    ///
    /// Cost: one extra vertex pass over the scene into a tiny target, plus a 2 KB readback. Fails with
    /// [`RenderError::PickBusy`] when four picks are already in flight.
    pub fn request_pick(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &ViewParams,
        pixel: [f64; 2],
    ) -> Result<PickTicket, RenderError> {
        if !(pixel[0].is_finite() && pixel[1].is_finite()) {
            return Err(RenderError::InvalidInput("pick position is not finite".into()));
        }
        self.picker.collect();
        let Some(slot) = self.picker.free_slot() else {
            // Give finished readbacks a chance to complete before reporting busy.
            let _ = device.poll(wgpu::PollType::Poll);
            self.picker.collect();
            return Err(RenderError::PickBusy);
        };
        self.prepare(device, queue);
        let size = self.clamp_size(params.size);
        let mut setup = self.frame_setup(params, size);
        let viewport = Viewport::new(f64::from(size[0]), f64::from(size[1]));
        let window = pick_window(pixel, viewport, self.picker.size);
        let narrowed = window.narrow.mul(&setup.view_proj);
        setup.uniforms.view_proj = narrowed.to_cols_f32();
        queue.write_buffer(&self.pick_buffer, 0, bytemuck::bytes_of(&setup.uniforms));

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kanavos pick") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kanavos pick pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.picker.ids.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.picker.depths.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.picker.depth.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipelines.pick);
            pass.set_bind_group(0, &self.pick_bind_group, &[]);
            self.draw_opaque_chunks(&mut pass, Some(&narrowed));
            let t = &self.store.transparent;
            if let (Some(vb), Some(ib)) = (&t.vertices, &t.indices)
                && !t.draws.is_empty()
            {
                pass.set_vertex_buffer(0, vb.buffer.slice(..vb.len));
                pass.set_index_buffer(ib.buffer.slice(..ib.len), wgpu::IndexFormat::Uint32);
                let count = t.draws.iter().map(|d| d.first_index + d.index_count).max().unwrap_or(0);
                pass.draw_indexed(0..count, 0, 0..1);
            }
        }
        self.picker.encode_copy(&mut encoder, slot);
        let submission = queue.submit([encoder.finish()]);
        let inv = setup.view_proj.inverse();
        let ticket = self.picker.begin(
            slot,
            submission,
            &window,
            viewport,
            inv,
            self.origin,
            std::sync::Arc::clone(&self.store.slot_ids),
        );
        Ok(ticket)
    }

    /// Returns the picks that have completed since the last call, without blocking.
    pub fn poll_picks(&mut self, device: &wgpu::Device) -> Vec<(PickTicket, PickResult)> {
        let _ = device.poll(wgpu::PollType::Poll);
        self.picker.collect();
        self.picker.drain()
    }

    /// Picks at a pixel and waits for the answer.
    ///
    /// Cost: this blocks the calling thread until the GPU has finished all work submitted so far (typically well
    /// under a millisecond to a few milliseconds for a house). Prefer [`Renderer::request_pick`] for hover
    /// feedback; a blocking pick on click is fine.
    pub fn pick(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &ViewParams,
        pixel: [f64; 2],
    ) -> Result<PickResult, RenderError> {
        let ticket = match self.request_pick(device, queue, params, pixel) {
            Ok(t) => t,
            Err(RenderError::PickBusy) => {
                // Wait for the picks in flight, keep their results for poll_picks, and try again.
                let _ = device.poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(Duration::from_secs(5)),
                });
                self.picker.collect();
                self.request_pick(device, queue, params, pixel)?
            }
            Err(e) => return Err(e),
        };
        let submission = self.picker.submission_of(ticket);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: submission,
                timeout: Some(Duration::from_secs(5)),
            })
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        // The map callback runs during poll; give it a few more polls if the driver is slow to report.
        for _ in 0..100 {
            self.picker.collect();
            if let Some(r) = self.picker.take(ticket) {
                return Ok(r);
            }
            if !self.picker.is_pending(ticket) {
                break;
            }
            let _ = device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_millis(50)),
            });
        }
        Err(RenderError::Timeout)
    }
}

/// Orthographic sun projection enclosing a box (render space). Returns the matrix and the world size of one
/// shadow-map texel.
fn light_matrix(bounds: ([f32; 3], [f32; 3]), sun: Vec3, map_size: u32) -> (Mat4, f64) {
    let (lo, hi) = bounds;
    let lo = Point3::new(f64::from(lo[0]), f64::from(lo[1]), f64::from(lo[2]));
    let hi = Point3::new(f64::from(hi[0]), f64::from(hi[1]), f64::from(hi[2]));
    let centre = lo.lerp(hi, 0.5);
    let radius = (lo.distance(hi) * 0.5).max(0.5);
    let forward = -sun.normalized().unwrap_or(Vec3::Z);
    let helper = if forward.z.abs() > 0.99 { Vec3::Y } else { Vec3::Z };
    let right = forward.cross(helper).normalized().unwrap_or(Vec3::X);
    let up = right.cross(forward);
    let eye = centre - forward * (2.0 * radius);
    let view = Mat4::view(eye, right, up, forward);
    let b = Bbox3 { min: lo, max: hi };
    let (mut hx, mut hy): (f64, f64) = (0.01, 0.01);
    for c in crate::camera::bbox_corners(&b) {
        let v = c - centre;
        hx = hx.max(v.dot(right).abs());
        hy = hy.max(v.dot(up).abs());
    }
    let (hx, hy) = (hx * 1.02, hy * 1.02);
    let proj = Mat4::orthographic(hx, hy, radius * 0.9, radius * 3.1);
    let texel = 2.0 * hx.max(hy) / f64::from(map_size.max(1));
    (proj.mul(&view), texel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_matrix_contains_scene() {
        let b = ([-5.0, -3.0, 0.0], [7.0, 4.0, 6.0]);
        for sun in [Vec3::new(0.3, -0.5, 0.8), Vec3::Z, Vec3::new(1.0, 0.0, 0.05)] {
            let (m, texel) = light_matrix(b, sun.normalized().unwrap(), 2048);
            assert!(texel > 0.0 && texel < 0.02);
            let bb = Bbox3 { min: Point3::new(-5.0, -3.0, 0.0), max: Point3::new(7.0, 4.0, 6.0) };
            for c in crate::camera::bbox_corners(&bb) {
                let p = m.transform_point(c).unwrap();
                assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0, "{p:?}");
                assert!(p.z > 0.0 && p.z < 1.0, "{p:?}");
            }
        }
    }

    #[test]
    fn renderer_can_move_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<Renderer>();
        assert_send::<RenderOutput>();
    }

    #[test]
    fn hash_words_differs() {
        assert_ne!(hash_words([1, 2, 3]), hash_words([1, 2, 4]));
        assert_eq!(hash_words([5, 6]), hash_words([5, 6]));
    }
}
