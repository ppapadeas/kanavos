//! The 3D viewport: the model as shaded solids with orbit, pan, zoom, standard views, picking, a storey filter and
//! a section box.
//!
//! The model comes from `kanavos_views::scene` once per revision. It is drawn on the GPU by `kanavos-render`
//! ([`gpu`]) when the application runs on wgpu, and otherwise by the software renderer ([`soft`]): the scene is
//! prepared once per revision ([`scene`]), and every frame its triangles are culled, clipped, sorted back to front
//! and submitted as one egui mesh. Camera, navigation, standard views, the storey filter and the section box are
//! shared by both paths.
//!
//! Behaviour is specified in `specs/ui/viewport-3d.md`.

pub mod camera;
pub mod gpu;
pub mod scene;
pub mod soft;

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use std::time::Instant;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, CursorIcon, Key, PointerButton, Rect, Sense, pos2, vec2};
use kanavos_engine::commands::DeleteElements;
use kanavos_i18n::{t, tf};
use kanavos_model::Id;

use crate::state::AppState;
pub use camera::StandardView;
use camera::{Aabb, OrbitCamera, V3, v3};
use scene::Prepared3d;
use soft::{RenderOptions, SceneRenderer, SoftwareRenderer};

/// Which storeys the viewport shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoreyFilter {
    /// Every storey.
    #[default]
    All,
    /// One storey only.
    Only(Id),
    /// A storey and every storey below it.
    UpTo(Id),
}

/// Timing and size of the last frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Viewport3dStats {
    /// Time spent in [`Viewport3d::ui`], milliseconds.
    pub frame_ms: f32,
    /// Time spent rendering the scene, milliseconds.
    pub render_ms: f32,
    /// Triangles drawn.
    pub triangles: usize,
    /// Edge segments drawn.
    pub edges: usize,
    /// Display triangles in the prepared scene.
    pub scene_triangles: usize,
    /// Time of the last scene preparation, milliseconds.
    pub prepare_ms: f32,
}

/// Wheel zoom per point of scroll.
const WHEEL_ZOOM_PER_PT: f64 = 0.0036;
/// Above this many triangles, edges are skipped while the camera moves.
const EDGES_WHILE_MOVING_LIMIT: usize = 15_000;

/// A 3D viewport.
///
/// The state lives behind a box so that the viewport is cheap to move and store next to the canvases.
pub struct Viewport3d {
    inner: Box<Inner>,
}

impl Default for Viewport3d {
    fn default() -> Self {
        Viewport3d::new()
    }
}

impl Viewport3d {
    /// A new viewport, looking at the model from the south-east.
    pub fn new() -> Viewport3d {
        Viewport3d { inner: Box::new(Inner::new()) }
    }

    /// Shows the viewport.
    pub fn ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame, state: &mut AppState) {
        self.inner.ui(ui, frame, state);
    }

    /// Fits the visible model in the view on the next frame.
    pub fn zoom_extents(&mut self) {
        self.inner.zoom_extents();
    }

    /// Turns to a standard view and fits the model.
    pub fn set_standard_view(&mut self, v: StandardView) {
        self.inner.set_standard_view(v);
    }

    /// Switches between perspective and parallel projection.
    pub fn set_parallel(&mut self, parallel: bool) {
        self.inner.camera.parallel = parallel;
    }

    /// Shows or hides feature edges.
    pub fn set_edges(&mut self, on: bool) {
        self.inner.edges = on;
    }

    /// Sets the storey filter.
    pub fn set_storey_filter(&mut self, f: StoreyFilter) {
        self.inner.storeys = f;
    }

    /// Sets the section box (world metres); an empty or inverted box removes it.
    pub fn set_section_box(&mut self, min: [f64; 3], max: [f64; 3]) {
        self.inner.set_section_box(min, max);
    }

    /// Removes the section box.
    pub fn clear_section_box(&mut self) {
        self.inner.section_box = None;
    }

    /// The camera as stored in a 3D view.
    pub fn camera(&self) -> kanavos_model::Camera {
        self.inner.camera()
    }

    /// Takes the camera of a 3D view.
    pub fn set_camera(&mut self, c: &kanavos_model::Camera) {
        self.inner.set_camera(c);
    }

    /// Timing of the last frame.
    pub fn stats(&self) -> Viewport3dStats {
        self.inner.stats
    }

    /// Whether the GPU renderer draws this viewport (`false` before the first frame and on the software path).
    pub fn uses_gpu(&self) -> bool {
        matches!(self.inner.backend, Backend::Gpu(_))
    }
}

/// The model's scene for one revision, shared by both renderers.
struct SceneData {
    revision: u64,
    scene: Arc<kanavos_views::Scene>,
    /// Element, storey and bounds of every scene item.
    items: Vec<(Id, Option<Id>, Aabb)>,
    bounds: Option<Aabb>,
}

/// Which renderer draws the viewport.
enum Backend {
    /// Not decided yet (decided on the first frame).
    Undecided,
    /// `kanavos-render` on the application's GPU device.
    Gpu(Box<gpu::GpuView>),
    /// The software renderer, with the reason the GPU path is not used.
    Software(String),
}

/// The viewport's state.
struct Inner {
    camera: OrbitCamera,
    target_camera: Option<OrbitCamera>,
    fitted: bool,
    data: Option<SceneData>,
    soft_scene: Option<(u64, Arc<Prepared3d>)>,
    renderer: SoftwareRenderer,
    backend: Backend,
    edges: bool,
    storeys: StoreyFilter,
    section_box: Option<Aabb>,
    moving: bool,
    hover_set: bool,
    hidden: HashSet<Id>,
    caps: Option<(u64, Aabb, Arc<Vec<scene::Cap>>)>,
    stats: Viewport3dStats,
}

impl Inner {
    fn new() -> Inner {
        Inner {
            camera: OrbitCamera::default(),
            target_camera: None,
            fitted: false,
            data: None,
            soft_scene: None,
            renderer: SoftwareRenderer::default(),
            backend: Backend::Undecided,
            edges: true,
            storeys: StoreyFilter::All,
            section_box: None,
            moving: false,
            hover_set: false,
            hidden: HashSet::new(),
            caps: None,
            stats: Viewport3dStats::default(),
        }
    }

    /// Fits the visible model in the view on the next frame.
    pub fn zoom_extents(&mut self) {
        self.fitted = false;
    }

    /// Turns to a standard view and fits the model.
    pub fn set_standard_view(&mut self, v: StandardView) {
        let mut c = self.camera;
        c.set_standard(v);
        if let Some(b) = self.visible_bounds() {
            c.fit(&b, 1.5);
        }
        self.target_camera = Some(c);
    }

    /// Sets or clears the section box (world metres).
    pub fn set_section_box(&mut self, min: [f64; 3], max: [f64; 3]) {
        let b = Aabb { min: v3(min[0], min[1], min[2]), max: v3(max[0], max[1], max[2]) };
        self.section_box = (b.min.x < b.max.x && b.min.y < b.max.y && b.min.z < b.max.z).then_some(b);
    }

    /// The camera as stored in a 3D view.
    pub fn camera(&self) -> kanavos_model::Camera {
        let e = self.camera.eye();
        let tg = self.camera.target;
        kanavos_model::Camera {
            eye: kanavos_geom::Point3::new(e.x, e.y, e.z),
            target: kanavos_geom::Point3::new(tg.x, tg.y, tg.z),
            fov: if self.camera.parallel { self.camera.view_height() } else { self.camera.fov.to_degrees() },
            parallel: self.camera.parallel,
        }
    }

    /// Takes the camera of a 3D view.
    pub fn set_camera(&mut self, c: &kanavos_model::Camera) {
        let eye = v3(c.eye.x, c.eye.y, c.eye.z);
        let target = v3(c.target.x, c.target.y, c.target.z);
        let back = eye - target;
        let d = back.length();
        if !(d.is_finite() && d > 1e-6) {
            return;
        }
        let mut cam = OrbitCamera {
            target,
            yaw: back.y.atan2(back.x),
            pitch: (back.z / d).clamp(-1.0, 1.0).asin().clamp(-camera::MAX_PITCH, camera::MAX_PITCH),
            distance: d,
            parallel: c.parallel,
            ..self.camera
        };
        if c.parallel {
            // `fov` is the visible height: keep it with the current angle.
            cam.distance = (c.fov / (2.0 * (cam.fov / 2.0).tan())).max(camera::DISTANCE_LIMITS.0);
        } else if c.fov > 1.0 && c.fov < 150.0 {
            cam.fov = c.fov.to_radians();
        }
        self.camera = cam;
        self.fitted = true;
    }

    /// The scene of the current revision.
    fn sync_data(&mut self, state: &AppState) {
        let rev = state.session.revision();
        if self.data.as_ref().is_some_and(|d| d.revision == rev) {
            return;
        }
        let started = Instant::now();
        let scene = kanavos_views::scene(state.doc(), &kanavos_views::SceneOptions::default());
        let items: Vec<(Id, Option<Id>, Aabb)> = scene
            .items
            .iter()
            .filter_map(|it| {
                let b = Aabb::from_points(it.mesh.positions.iter().map(|p| v3(p.x, p.y, p.z)))?;
                Some((it.element, it.storey, b))
            })
            .collect();
        let bounds = items.iter().map(|(_, _, b)| *b).reduce(|a, b| a.union(&b));
        self.stats.prepare_ms = started.elapsed().as_secs_f32() * 1000.0;
        self.data = Some(SceneData { revision: rev, scene: Arc::new(scene), items, bounds });
    }

    /// The software renderer's prepared scene (built on first use per revision).
    fn soft_prepared(&mut self) -> Option<Arc<Prepared3d>> {
        let data = self.data.as_ref()?;
        if let Some((r, s)) = &self.soft_scene
            && *r == data.revision
        {
            return Some(s.clone());
        }
        let started = Instant::now();
        let p = Arc::new(Prepared3d::new(&data.scene));
        self.stats.prepare_ms += started.elapsed().as_secs_f32() * 1000.0;
        self.stats.scene_triangles = p.tris.len();
        self.soft_scene = Some((data.revision, p.clone()));
        Some(p)
    }

    /// Elements hidden by the storey filter.
    fn hidden_by_filter(&self, state: &AppState) -> HashSet<Id> {
        let doc = state.doc();
        let elevation = |id: Id| doc.storey(id).map(|s| s.elevation.as_m());
        let Some(data) = &self.data else { return HashSet::new() };
        data.items
            .iter()
            .filter(|(_, storey, _)| match self.storeys {
                StoreyFilter::All => false,
                StoreyFilter::Only(s) => *storey != Some(s),
                StoreyFilter::UpTo(s) => match (storey.and_then(elevation), elevation(s)) {
                    (Some(e), Some(limit)) => e > limit + 1e-6,
                    _ => true,
                },
            })
            .map(|(id, _, _)| *id)
            .collect()
    }

    /// Bounds of the items that pass the storey filter.
    fn visible_bounds(&self) -> Option<Aabb> {
        let data = self.data.as_ref()?;
        data.items
            .iter()
            .filter(|(id, _, _)| !self.hidden.contains(id))
            .map(|(_, _, b)| *b)
            .reduce(|a, b| a.union(&b))
            .or(data.bounds)
    }

    /// The nearest visible item of the software scene under a screen point, and the hit point.
    fn soft_pick(&self, scene: &Prepared3d, proj: &camera::Projection, at: [f64; 2]) -> Option<(Id, V3)> {
        let (o, d) = proj.ray(at);
        let mut best: Option<(f64, usize)> = None;
        for (ii, it) in scene.items.iter().enumerate() {
            if self.hidden.contains(&it.element) {
                continue;
            }
            if let Some(b) = &self.section_box
                && !b.intersects(&it.bounds)
            {
                continue;
            }
            if !camera::ray_hits_box(o, d, &it.bounds) {
                continue;
            }
            for ti in it.tris.clone() {
                let Some(tri) = scene.tris.get(ti) else { continue };
                let [a, b, c] = tri.v.map(|i| scene.positions.get(i as usize).copied().unwrap_or_default());
                if let Some(tt) = camera::ray_triangle(o, d, a, b, c)
                    && best.is_none_or(|(bt, _)| tt < bt)
                {
                    let hit = o + d * tt;
                    if self.section_box.is_none_or(|bx| bx.contains(hit, 1e-6)) {
                        best = Some((tt, ii));
                    }
                }
            }
        }
        best.and_then(|(tt, ii)| scene.items.get(ii).map(|it| (it.element, o + d * tt)))
    }

    /// Chooses the renderer on the first frame: the GPU when the application runs on wgpu and the renderer can be
    /// created, the software renderer otherwise (or when `KANAVOS_3D_SOFTWARE` is set).
    fn decide_backend(&mut self, frame: &eframe::Frame) {
        if !matches!(self.backend, Backend::Undecided) {
            return;
        }
        self.backend = if std::env::var_os("KANAVOS_3D_SOFTWARE").is_some() {
            Backend::Software("KANAVOS_3D_SOFTWARE".into())
        } else {
            match frame.wgpu_render_state() {
                Some(rs) => match gpu::GpuView::new(rs) {
                    Ok(g) => Backend::Gpu(Box::new(g)),
                    Err(e) => Backend::Software(e),
                },
                None => Backend::Software("no wgpu device".into()),
            }
        };
    }

    /// Updates the selection from a click on an element (or on nothing).
    fn click_select(state: &mut AppState, id: Option<Id>, m: egui::Modifiers) {
        match id {
            Some(id) if m.command => {
                if !state.selection.remove(&id) {
                    state.selection.insert(id);
                }
            }
            Some(id) if m.shift => {
                state.selection.insert(id);
            }
            Some(id) => state.selection = [id].into(),
            None if !m.shift && !m.command => state.selection.clear(),
            None => {}
        }
    }

    /// Shows the viewport.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame, state: &mut AppState) {
        let started = Instant::now();
        crate::canvas::shot::tick(ui.ctx(), Some(&mut *state));
        self.sync_data(state);
        self.decide_backend(frame);
        self.hidden = self.hidden_by_filter(state);
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if response.hovered() && ctx.memory(|m| m.focused().is_none()) {
            response.request_focus();
        }
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let centre = [f64::from(rect.center().x), f64::from(rect.center().y)];
        let aspect = size[0] / size[1].max(1.0);
        let size_px =
            [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let to_px =
            |p: egui::Pos2| [f64::from((p.x - rect.left()) * ppp), f64::from((p.y - rect.top()) * ppp)];
        let rs = frame.wgpu_render_state();
        let soft = match self.backend {
            Backend::Gpu(_) => None,
            _ => self.soft_prepared(),
        };

        if !self.fitted {
            if let Some(b) = self.visible_bounds() {
                self.camera.fit(&b, aspect);
            }
            self.fitted = true;
        }

        // Navigation.
        let mut moving = false;
        if response.dragged_by(PointerButton::Primary) {
            let d = response.drag_delta();
            self.camera.orbit(f64::from(d.x), f64::from(d.y));
            self.target_camera = None;
            moving = true;
        }
        if response.dragged_by(PointerButton::Middle) || response.dragged_by(PointerButton::Secondary) {
            let d = response.drag_delta();
            self.camera.pan(f64::from(d.x), f64::from(d.y), size[1]);
            self.target_camera = None;
            moving = true;
        }
        if response.double_clicked_by(PointerButton::Middle) {
            self.fit_animated(aspect);
        }
        let proj = self.camera.projection(size, centre);
        if response.contains_pointer() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (1.0 / f64::from(pinch)) * (-f64::from(scroll) * WHEEL_ZOOM_PER_PT).exp();
            if (factor - 1.0).abs() > 1e-6 {
                let focus = response.hover_pos().and_then(|p| {
                    let at = [f64::from(p.x), f64::from(p.y)];
                    let hit = match (&self.backend, &soft) {
                        (Backend::Gpu(g), _) => g.hover_point,
                        (_, Some(s)) => self.soft_pick(s, &proj, at).map(|(_, hit)| hit),
                        _ => None,
                    };
                    hit.or_else(|| {
                        // No hit: the point on the plane through the target facing the camera.
                        let (o, d) = proj.ray(at);
                        let denom = d.dot(proj.forward);
                        (denom.abs() > 1e-9)
                            .then(|| o + d * ((self.camera.target - o).dot(proj.forward) / denom))
                    })
                });
                self.camera.zoom(factor, focus);
                self.target_camera = None;
                moving = true;
            }
        }
        if response.has_focus() {
            let (home, esc, del) = ui.input(|i| {
                (i.key_pressed(Key::Home), i.key_pressed(Key::Escape), i.key_pressed(Key::Delete))
            });
            if home {
                self.fit_animated(aspect);
            }
            if esc {
                state.selection.clear();
            }
            if del && !state.selection.is_empty() {
                let ids: Vec<Id> = state.selection.iter().copied().collect();
                let _ = state.run_all(vec![DeleteElements { ids }.into()], String::new());
            }
        }
        if let Some(target) = self.target_camera {
            if step_camera(&mut self.camera, &target, 0.3) {
                self.target_camera = None;
            }
            moving = true;
            ctx.request_repaint();
        }
        self.moving = moving;
        let proj = self.camera.projection(size, centre);
        let edges = self.edges;
        let section_box = self.section_box;
        let gpu_params = gpu::GpuView::params(&self.camera, size_px, ppp, edges, section_box);
        let m = ui.input(|i| i.modifiers);
        let pointer = response.hover_pos().filter(|_| response.contains_pointer());
        let painter = painter.with_clip_rect(rect);
        let render_started = Instant::now();
        let mut triangles = 0;
        let mut empty = self.data.as_ref().is_none_or(|d| d.items.is_empty());

        match (&mut self.backend, rs) {
            (Backend::Gpu(g), Some(rs)) => {
                if let Some(d) = &self.data {
                    g.sync_scene(d.revision, &d.scene);
                }
                // Picking and hover.
                if response.clicked_by(PointerButton::Primary)
                    && let Some(p) = response.interact_pointer_pos()
                {
                    let id = g.pick(rs, &gpu_params, to_px(p)).map(|(id, _)| Id(id));
                    Inner::click_select(state, id, m);
                }
                let busy = g.hover(rs, &gpu_params, pointer.filter(|_| !moving).map(to_px));
                if busy {
                    ctx.request_repaint();
                }
                if pointer.is_some() {
                    state.hover = g.hover_id.map(Id);
                    self.hover_set = true;
                } else if self.hover_set {
                    state.hover = None;
                    self.hover_set = false;
                }
                let hidden: HashSet<u64> = self.hidden.iter().map(|i| i.0).collect();
                let selected: HashSet<u64> = state.selection.iter().map(|i| i.0).collect();
                g.set_state(hidden, selected, state.hover.map(|h| h.0));
                g.paint(rs, &painter, rect, &gpu_params);
                triangles = g.stats.triangles;
                empty = g.stats.objects == 0;
            }
            _ => {
                paint_background(&painter, rect);
                if let Some(scene) = soft {
                    if response.clicked_by(PointerButton::Primary)
                        && let Some(p) = response.interact_pointer_pos()
                    {
                        let id =
                            self.soft_pick(&scene, &proj, [f64::from(p.x), f64::from(p.y)]).map(|(id, _)| id);
                        Inner::click_select(state, id, m);
                    }
                    if !moving && let Some(p) = pointer {
                        state.hover =
                            self.soft_pick(&scene, &proj, [f64::from(p.x), f64::from(p.y)]).map(|(id, _)| id);
                        self.hover_set = true;
                    } else if self.hover_set && pointer.is_none() {
                        state.hover = None;
                        self.hover_set = false;
                    }
                    let selected: BTreeSet<Id> = state.selection.clone();
                    let opts = RenderOptions {
                        selected: scene.items.iter().map(|it| selected.contains(&it.element)).collect(),
                        hover: state.hover.and_then(|h| scene.items.iter().position(|it| it.element == h)),
                        visible: scene.items.iter().map(|it| !self.hidden.contains(&it.element)).collect(),
                        edges: edges && !(moving && scene.tris.len() > EDGES_WHILE_MOVING_LIMIT),
                        edge_resolution: if moving { 0.5 } else { 1.0 },
                        occlusion: true,
                        section_box,
                        caps: self.caps_for(&scene, state.session.revision()),
                    };
                    if let Some(b) = self.visible_bounds() {
                        paint_ground(&painter, &proj, &b);
                    }
                    let st = self.renderer.paint(&painter, rect, &scene, &proj, &opts);
                    triangles = st.triangles;
                    self.stats.edges = st.edges;
                }
            }
        }
        self.stats.render_ms = render_started.elapsed().as_secs_f32() * 1000.0;
        self.stats.triangles = triangles;

        if empty {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t(state.locale, "ui.viewport3d.empty"),
                egui::FontId::proportional(15.0),
                Color32::from_gray(110),
            );
        }
        self.toolbar(ui, &ctx, rect, state, aspect);
        let backend = match &self.backend {
            Backend::Gpu(g) => tf(state.locale, "ui.viewport3d.backend.gpu", &[("adapter", &g.adapter)]),
            Backend::Software(why) => tf(state.locale, "ui.viewport3d.backend.software", &[("reason", why)]),
            Backend::Undecided => String::new(),
        };
        painter.text(
            rect.left_bottom() + vec2(10.0, -8.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{}    {}    {}",
                t(state.locale, "ui.viewport3d.hint"),
                tf(state.locale, "ui.viewport3d.triangles", &[("n", &triangles.to_string())]),
                backend
            ),
            egui::FontId::proportional(11.5),
            Color32::from_gray(95),
        );
        if response.hovered() {
            let icon = if response.dragged_by(PointerButton::Primary) {
                CursorIcon::Grabbing
            } else if moving {
                CursorIcon::AllScroll
            } else if state.hover.is_some() {
                CursorIcon::PointingHand
            } else {
                CursorIcon::Default
            };
            ctx.set_cursor_icon(icon);
        }
        self.stats.frame_ms = started.elapsed().as_secs_f32() * 1000.0;
    }

    /// Section caps for the current box, cached per revision and box.
    fn caps_for(&mut self, scene: &Prepared3d, revision: u64) -> Arc<Vec<scene::Cap>> {
        let Some(b) = self.section_box else { return Arc::default() };
        if let Some((r, cb, caps)) = &self.caps
            && *r == revision
            && *cb == b
        {
            return caps.clone();
        }
        let caps = Arc::new(scene::section_caps(scene, &b));
        self.caps = Some((revision, b, caps.clone()));
        caps
    }

    fn fit_animated(&mut self, aspect: f64) {
        if let Some(b) = self.visible_bounds() {
            let mut c = self.camera;
            c.fit(&b, aspect);
            self.target_camera = Some(c);
        }
    }

    /// The overlay toolbar: standard views, projection, edges, section box, storeys, fit.
    fn toolbar(&mut self, ui: &egui::Ui, ctx: &egui::Context, rect: Rect, state: &AppState, aspect: f64) {
        let locale = state.locale;
        let storeys: Vec<(Id, String)> =
            state.doc().storeys().iter().map(|s| (s.id, s.name.clone())).collect();
        let mut standard: Option<StandardView> = None;
        let mut fit = false;
        let mut toggle_box = false;
        let bounds = self.data.as_ref().and_then(|d| d.bounds);
        egui::Area::new(ui.id().with("viewport3d_toolbar"))
            .fixed_pos(rect.left_top() + vec2(8.0, 8.0))
            .order(egui::Order::Foreground)
            .constrain_to(rect)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (v, key) in [
                            (StandardView::Top, "ui.viewport3d.view.top"),
                            (StandardView::Front, "ui.viewport3d.view.front"),
                            (StandardView::Side, "ui.viewport3d.view.side"),
                            (StandardView::Iso, "ui.viewport3d.view.iso"),
                        ] {
                            if ui
                                .small_button(t(locale, key).as_ref())
                                .on_hover_text(t(locale, "ui.viewport3d.view.tip").as_ref())
                                .clicked()
                            {
                                standard = Some(v);
                            }
                        }
                        ui.separator();
                        let label = if self.camera.parallel {
                            "ui.viewport3d.parallel"
                        } else {
                            "ui.viewport3d.perspective"
                        };
                        if ui.small_button(t(locale, label).as_ref()).clicked() {
                            self.camera.parallel = !self.camera.parallel;
                        }
                        ui.checkbox(&mut self.edges, t(locale, "ui.viewport3d.edges").as_ref());
                        let mut boxed = self.section_box.is_some();
                        if ui
                            .checkbox(&mut boxed, t(locale, "ui.viewport3d.section_box").as_ref())
                            .on_hover_text(t(locale, "ui.viewport3d.section_box.tip").as_ref())
                            .changed()
                        {
                            toggle_box = true;
                        }
                        ui.separator();
                        let current = match self.storeys {
                            StoreyFilter::All => t(locale, "ui.viewport3d.storeys_all").into_owned(),
                            StoreyFilter::Only(id) | StoreyFilter::UpTo(id) => {
                                let name = storeys
                                    .iter()
                                    .find(|(s, _)| *s == id)
                                    .map(|(_, n)| n.as_str())
                                    .unwrap_or("?");
                                let key = if matches!(self.storeys, StoreyFilter::Only(_)) {
                                    "ui.viewport3d.storeys_only"
                                } else {
                                    "ui.viewport3d.storeys_up_to"
                                };
                                tf(locale, key, &[("name", name)])
                            }
                        };
                        egui::ComboBox::from_id_salt(ui.id().with("storeys")).selected_text(current).show_ui(
                            ui,
                            |ui| {
                                ui.selectable_value(
                                    &mut self.storeys,
                                    StoreyFilter::All,
                                    t(locale, "ui.viewport3d.storeys_all").as_ref(),
                                );
                                for (id, name) in &storeys {
                                    ui.selectable_value(
                                        &mut self.storeys,
                                        StoreyFilter::UpTo(*id),
                                        tf(locale, "ui.viewport3d.storeys_up_to", &[("name", name)]),
                                    );
                                }
                                for (id, name) in &storeys {
                                    ui.selectable_value(
                                        &mut self.storeys,
                                        StoreyFilter::Only(*id),
                                        tf(locale, "ui.viewport3d.storeys_only", &[("name", name)]),
                                    );
                                }
                            },
                        );
                        if ui.small_button(t(locale, "ui.viewport3d.fit").as_ref()).clicked() {
                            fit = true;
                        }
                    });
                    if let (Some(bx), Some(sb)) = (&mut self.section_box, bounds) {
                        let pad = 1.0;
                        ui.horizontal(|ui| {
                            for (key, v, lo, hi) in [
                                ("ui.viewport3d.box.bottom", &mut bx.min.z, sb.min.z - pad, sb.max.z + pad),
                                ("ui.viewport3d.box.top", &mut bx.max.z, sb.min.z - pad, sb.max.z + pad),
                                ("ui.viewport3d.box.west", &mut bx.min.x, sb.min.x - pad, sb.max.x + pad),
                                ("ui.viewport3d.box.east", &mut bx.max.x, sb.min.x - pad, sb.max.x + pad),
                                ("ui.viewport3d.box.south", &mut bx.min.y, sb.min.y - pad, sb.max.y + pad),
                                ("ui.viewport3d.box.north", &mut bx.max.y, sb.min.y - pad, sb.max.y + pad),
                            ] {
                                ui.label(t(locale, key).as_ref());
                                ui.add(
                                    egui::DragValue::new(v)
                                        .speed(0.05)
                                        .range(lo..=hi)
                                        .suffix(" m")
                                        .max_decimals(2),
                                );
                            }
                        });
                        // Keep the box valid.
                        bx.max.x = bx.max.x.max(bx.min.x + 0.01);
                        bx.max.y = bx.max.y.max(bx.min.y + 0.01);
                        bx.max.z = bx.max.z.max(bx.min.z + 0.01);
                    }
                });
            });
        if let Some(v) = standard {
            self.set_standard_view(v);
        }
        if fit {
            self.fit_animated(aspect);
        }
        if toggle_box {
            if self.section_box.is_some() {
                self.section_box = None;
            } else {
                // Around the selection if any, otherwise the whole model cut at mid-height.
                let sel = self.data.as_ref().and_then(|d| {
                    d.items
                        .iter()
                        .filter(|(id, _, _)| state.selection.contains(id))
                        .map(|(_, _, b)| *b)
                        .reduce(|a, b| a.union(&b))
                });
                self.section_box = match (sel, bounds) {
                    (Some(b), _) => {
                        Some(Aabb { min: b.min - v3(0.5, 0.5, 0.5), max: b.max + v3(0.5, 0.5, 0.5) })
                    }
                    (None, Some(b)) => Some(Aabb {
                        min: b.min - v3(0.1, 0.1, 0.1),
                        max: v3(b.max.x + 0.1, b.max.y + 0.1, b.min.z + (b.max.z - b.min.z) * 0.5),
                    }),
                    _ => None,
                };
            }
        }
    }
}

/// Moves a camera a fraction of the way towards a target (yaw along the shorter way round). Returns `true` when
/// it has arrived.
fn step_camera(c: &mut OrbitCamera, target: &OrbitCamera, k: f64) -> bool {
    let tau = std::f64::consts::TAU;
    let mut dyaw = (target.yaw - c.yaw).rem_euclid(tau);
    if dyaw > std::f64::consts::PI {
        dyaw -= tau;
    }
    c.yaw += dyaw * k;
    c.pitch += (target.pitch - c.pitch) * k;
    c.distance *= (target.distance / c.distance.max(1e-9)).powf(k);
    c.target = c.target + (target.target - c.target) * k;
    let done = dyaw.abs() < 1e-3
        && (target.pitch - c.pitch).abs() < 1e-3
        && (target.distance / c.distance.max(1e-9) - 1.0).abs() < 1e-3
        && (target.target - c.target).length() < 1e-3 * target.distance.max(1.0);
    if done {
        *c = OrbitCamera { parallel: c.parallel, ..*target };
    }
    done
}

/// A light ground plane under the model (seen from above only), extending well beyond it, fading at its rim.
fn paint_ground(painter: &egui::Painter, proj: &camera::Projection, b: &Aabb) {
    let z = b.min.z - 0.005;
    if proj.eye.z <= z {
        return;
    }
    let c = b.centre();
    let r = (b.radius() * 2.5).max(10.0);
    let n = 16;
    let fill = Color32::from_rgb(226, 225, 218);
    let mut m = Mesh::default();
    for i in 0..n {
        for j in 0..n {
            let x0 = c.x - r + 2.0 * r * f64::from(i) / f64::from(n);
            let x1 = c.x - r + 2.0 * r * f64::from(i + 1) / f64::from(n);
            let y0 = c.y - r + 2.0 * r * f64::from(j) / f64::from(n);
            let y1 = c.y - r + 2.0 * r * f64::from(j + 1) / f64::from(n);
            let corners = [v3(x0, y0, z), v3(x1, y0, z), v3(x1, y1, z), v3(x0, y1, z)];
            let pts: Vec<_> = corners.iter().filter_map(|p| proj.project(*p)).collect();
            let [Some(p0), Some(p1), Some(p2), Some(p3)] = [pts.first(), pts.get(1), pts.get(2), pts.get(3)]
            else {
                continue;
            };
            let base = m.vertices.len() as u32;
            for (q, w) in [(p0, corners[0]), (p1, corners[1]), (p2, corners[2]), (p3, corners[3])] {
                // Fade towards the rim.
                let d = ((w.x - c.x).powi(2) + (w.y - c.y).powi(2)).sqrt() / r;
                let a = (1.0 - ((d - 0.55) / 0.45).clamp(0.0, 1.0)) as f32;
                m.vertices.push(Vertex {
                    pos: pos2(q[0] as f32, q[1] as f32),
                    uv: WHITE_UV,
                    color: fill.gamma_multiply(a),
                });
            }
            m.add_triangle(base, base + 1, base + 2);
            m.add_triangle(base, base + 2, base + 3);
        }
    }
    painter.add(egui::Shape::mesh(m));
}

/// A soft sky gradient behind the model.
fn paint_background(painter: &egui::Painter, rect: Rect) {
    let top = Color32::from_rgb(207, 218, 230);
    let bottom = Color32::from_rgb(242, 241, 237);
    let mut m = Mesh::default();
    for (p, c) in [
        (rect.left_top(), top),
        (rect.right_top(), top),
        (rect.right_bottom(), bottom),
        (rect.left_bottom(), bottom),
    ] {
        m.vertices.push(Vertex { pos: pos2(p.x, p.y), uv: WHITE_UV, color: c });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(m));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cameras_round_trip_through_the_model() {
        let mut v = Viewport3d::new();
        let c = kanavos_model::Camera::default();
        v.set_camera(&c);
        let back = v.camera();
        assert!((back.eye.x - c.eye.x).abs() < 1e-9 && (back.eye.z - c.eye.z).abs() < 1e-9);
        assert!((back.fov - c.fov).abs() < 1e-9);
    }

    #[test]
    fn camera_steps_arrive() {
        let mut a = OrbitCamera::default();
        let mut b = a;
        b.yaw += 3.0;
        b.distance *= 3.0;
        b.target = b.target + v3(4.0, 0.0, 0.0);
        let mut arrived = false;
        for _ in 0..200 {
            if step_camera(&mut a, &b, 0.3) {
                arrived = true;
                break;
            }
        }
        assert!(arrived);
    }
}
