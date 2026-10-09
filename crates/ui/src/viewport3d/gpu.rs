//! The GPU path of the 3D viewport, through `kanavos-render` (ADR-0007).
//!
//! The renderer draws into its own texture, which is shown as an egui image. The viewport keeps its own camera
//! and navigation (shared with the software path) and hands the renderer the scene, the display state (hidden,
//! selected, hovered) and the view parameters every frame. Hover uses asynchronous picks; a click uses a blocking
//! pick.

use std::collections::HashSet;

use kanavos_drawing::Rgba;
use kanavos_geom::{Bbox3, Point3};
use kanavos_model::Category;
use kanavos_render::{
    DisplayStyle, OrbitCamera as GpuCamera, PickTicket, Projection, RenderStats, Renderer, RendererConfig,
    Scene, ViewParams, suggest_origin, wgpu,
};

use super::camera::{Aabb, OrbitCamera, V3, v3};

/// Selection colour, the same blue as in the drawings.
const SELECTION: Rgba = Rgba::rgb(24, 104, 222);
/// Hover colour.
const HOVER: Rgba = Rgba::rgb(110, 160, 235);

/// The renderer and what the viewport keeps for it.
pub struct GpuView {
    renderer: Renderer,
    texture: Option<(egui::TextureId, u64)>,
    scene_revision: Option<u64>,
    hover_ticket: Option<PickTicket>,
    /// World point under the pointer, from the latest hover pick.
    pub hover_point: Option<V3>,
    /// Object under the pointer, from the latest hover pick.
    pub hover_id: Option<u64>,
    /// Counters of the last frame.
    pub stats: RenderStats,
    /// Graphics adapter name.
    pub adapter: String,
}

/// Category number used by the renderer (index in [`Category::ALL`]).
pub fn category_index(c: Category) -> u32 {
    Category::ALL.iter().position(|x| *x == c).unwrap_or(0) as u32
}

impl GpuView {
    /// Creates the renderer on the application's device.
    pub fn new(rs: &egui_wgpu::RenderState) -> Result<GpuView, String> {
        let renderer = Renderer::new(&rs.device, RendererConfig::default()).map_err(|e| e.to_string())?;
        let adapter = rs.adapter.get_info().name;
        Ok(GpuView {
            renderer,
            texture: None,
            scene_revision: None,
            hover_ticket: None,
            hover_point: None,
            hover_id: None,
            stats: RenderStats::default(),
            adapter,
        })
    }

    /// Uploads the scene when the model revision changed (only changed objects are re-uploaded).
    pub fn sync_scene(&mut self, revision: u64, scene: &kanavos_views::Scene) {
        if self.scene_revision == Some(revision) {
            return;
        }
        let origin = suggest_origin(scene.items.iter().map(|i| &i.mesh));
        let mut s = Scene::new(origin);
        for it in &scene.items {
            s.push_solid(it.element.0, &it.mesh, it.color, category_index(it.category));
        }
        self.renderer.set_scene(&s);
        self.scene_revision = Some(revision);
    }

    /// Frame parameters for a camera, a viewport size in physical pixels and the display options.
    pub fn params(
        camera: &OrbitCamera,
        size: [u32; 2],
        ppp: f32,
        edges: bool,
        section_box: Option<Aabb>,
    ) -> ViewParams {
        let cam = GpuCamera {
            target: Point3::new(camera.target.x, camera.target.y, camera.target.z),
            yaw: camera.yaw,
            pitch: camera.pitch,
            distance: camera.distance,
            fov_y: camera.fov,
            projection: if camera.parallel { Projection::Parallel } else { Projection::Perspective },
        };
        let mut p = ViewParams::new(cam, size, ppp);
        p.style =
            DisplayStyle { edges, selection_color: SELECTION, hover_color: HOVER, ..DisplayStyle::default() };
        if let Some(b) = section_box {
            p.set_section_box(&Bbox3 {
                min: Point3::new(b.min.x, b.min.y, b.min.z),
                max: Point3::new(b.max.x, b.max.y, b.max.z),
            });
        }
        p
    }

    /// Sets what is hidden, selected and hovered.
    pub fn set_state(&mut self, hidden: HashSet<u64>, selected: HashSet<u64>, hovered: Option<u64>) {
        self.renderer.set_hidden(hidden);
        self.renderer.set_selection(selected);
        self.renderer.set_hovered(hovered);
    }

    /// The object and world point at a pixel (physical pixels from the viewport's top-left), waiting for the GPU.
    pub fn pick(
        &mut self,
        rs: &egui_wgpu::RenderState,
        params: &ViewParams,
        px: [f64; 2],
    ) -> Option<(u64, V3)> {
        let hit = self.renderer.pick(&rs.device, &rs.queue, params, px).ok()?;
        let p = hit.position?;
        Some((hit.id?, v3(p.x, p.y, p.z)))
    }

    /// Advances hover picking: collects finished picks and starts a new one at `px` (when given and none is in
    /// flight). Returns whether a pick is still in flight (the caller should repaint).
    pub fn hover(&mut self, rs: &egui_wgpu::RenderState, params: &ViewParams, px: Option<[f64; 2]>) -> bool {
        for (ticket, hit) in self.renderer.poll_picks(&rs.device) {
            if Some(ticket) == self.hover_ticket {
                self.hover_ticket = None;
                self.hover_point = hit.position.map(|p| v3(p.x, p.y, p.z));
                self.hover_id = hit.id;
            }
        }
        match px {
            Some(px) if self.hover_ticket.is_none() => {
                self.hover_ticket = self.renderer.request_pick(&rs.device, &rs.queue, params, px).ok();
            }
            None => {
                self.hover_id = None;
                self.hover_point = None;
            }
            _ => {}
        }
        self.hover_ticket.is_some()
    }

    /// Renders a frame and shows it in `rect`.
    pub fn paint(
        &mut self,
        rs: &egui_wgpu::RenderState,
        painter: &egui::Painter,
        rect: egui::Rect,
        params: &ViewParams,
    ) {
        let out = self.renderer.render(&rs.device, &rs.queue, params);
        self.stats = out.stats;
        let id = {
            let mut egui_renderer = rs.renderer.write();
            match self.texture {
                Some((id, generation)) if generation == out.generation => id,
                Some((id, _)) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(
                        &rs.device,
                        &out.view,
                        wgpu::FilterMode::Nearest,
                        id,
                    );
                    id
                }
                None => {
                    egui_renderer.register_native_texture(&rs.device, &out.view, wgpu::FilterMode::Nearest)
                }
            }
        };
        self.texture = Some((id, out.generation));
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        painter.image(id, rect, uv, egui::Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_map_to_distinct_numbers() {
        let set: HashSet<u32> = Category::ALL.iter().map(|c| category_index(*c)).collect();
        assert_eq!(set.len(), Category::ALL.len());
    }

    #[test]
    fn params_carry_camera_style_and_section_box() {
        let cam = OrbitCamera { parallel: true, ..OrbitCamera::default() };
        let b = Aabb { min: v3(0.0, 0.0, 0.0), max: v3(1.0, 2.0, 3.0) };
        let p = GpuView::params(&cam, [800, 600], 1.5, false, Some(b));
        assert_eq!(p.camera.projection, Projection::Parallel);
        assert!((p.camera.distance - cam.distance).abs() < 1e-12);
        assert!(!p.style.edges);
        assert_eq!(p.style.selection_color, SELECTION);
        assert_eq!(p.clip_planes.len(), 6);
    }
}
