//! Interactive viewer for the sample house, showing how an egui application drives `kanavos-render`.
//!
//! ```text
//! cargo run -p kanavos-render --example viewer --release
//! ```
//!
//! The window is driven by winit, egui-winit and egui-wgpu directly (the same pieces eframe is built from);
//! the viewport code in `Viewer::viewport_ui` is what an eframe application would put in `App::ui`, with the
//! render state taken from `frame.wgpu_render_state()`.
//!
//! Mouse: left drag orbits about the point under the cursor, right or middle drag pans, the wheel zooms towards
//! the cursor, a click selects (and prints the id), a double click fits the model.
//!
//! Keys: E edges, S shadows, A ambient occlusion, G grid, B section box, H ghost the roof, P perspective or
//! parallel, F fit, 1 top, 2 front, 3 right, 4 back, 5 left, 6–9 isometric views.
//!
//! `KANAVOS_AUTOCLOSE_FRAMES=60` closes the window after 60 frames and prints the id picked at the centre of
//! the viewport; with `KANAVOS_SCREENSHOT=path.png` it also saves a screenshot of the whole window first.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_wgpu::wgpu;
use kanavos_geom::{Bbox3, Point3};
use kanavos_render::demo::{self, ids};
use kanavos_render::{
    DisplayStyle, OrbitCamera, PickTicket, Projection, Renderer, RendererConfig, StandardView, ViewParams,
    Viewport,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Font files tried at start-up (egui's bundled fonts are not used).
const FONT_CANDIDATES: &[&str] = &[
    "C:/Windows/Fonts/segoeui.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
];

/// Standard views by key.
const VIEW_KEYS: [(egui::Key, StandardView); 9] = [
    (egui::Key::Num1, StandardView::Top),
    (egui::Key::Num2, StandardView::Front),
    (egui::Key::Num3, StandardView::Right),
    (egui::Key::Num4, StandardView::Back),
    (egui::Key::Num5, StandardView::Left),
    (egui::Key::Num6, StandardView::IsoSouthWest),
    (egui::Key::Num7, StandardView::IsoSouthEast),
    (egui::Key::Num8, StandardView::IsoNorthEast),
    (egui::Key::Num9, StandardView::IsoNorthWest),
];

const TRANSITION_SECONDS: f64 = 0.35;

/// A camera animation between two views.
struct Transition {
    from: OrbitCamera,
    to: OrbitCamera,
    start: Instant,
}

/// Where the smoke-test mode is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closing {
    Running,
    AwaitingScreenshot,
    Done,
}

/// The viewer state: everything an interface keeps for one 3D viewport.
struct Viewer {
    renderer: Renderer,
    camera: OrbitCamera,
    style: DisplayStyle,
    section_box: bool,
    ghost_roof: bool,
    /// egui texture id of the viewport image and the renderer generation it was registered for.
    texture: Option<(egui::TextureId, u64)>,
    /// Hover pick in flight.
    hover_ticket: Option<PickTicket>,
    /// World point under the cursor, from the latest hover pick.
    hover_point: Option<Point3>,
    /// Pivot and pan anchor for the current drag.
    drag_anchor: Option<Point3>,
    transition: Option<Transition>,
    status: String,
    has_font: bool,
    frames: u64,
    autoclose: Option<u64>,
    closing: Closing,
    screenshot_requested: bool,
    last_frame: Instant,
    frame_ms: f32,
    viewport: Viewport,
}

fn load_font(ctx: &egui::Context) -> bool {
    for path in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::empty();
            fonts.font_data.insert("ui".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.insert(egui::FontFamily::Proportional, vec!["ui".to_owned()]);
            fonts.families.insert(egui::FontFamily::Monospace, vec!["ui".to_owned()]);
            ctx.set_fonts(fonts);
            return true;
        }
    }
    false
}

fn house_bounds(renderer: &Renderer) -> Option<Bbox3> {
    renderer.bounds_of([ids::SLAB, ids::ROOF_SOUTH, ids::ROOF_NORTH, ids::TERRACE, ids::PERGOLA])
}

impl Viewer {
    fn new(ctx: &egui::Context, rs: &egui_wgpu::RenderState) -> Result<Viewer, String> {
        let info = rs.adapter.get_info();
        println!("adapter: {} ({:?})", info.name, info.backend);
        let has_font = load_font(ctx);
        if !has_font {
            println!("no system font found; the side panel is hidden, use the keyboard shortcuts");
        }
        let mut renderer = Renderer::new(&rs.device, RendererConfig::default()).map_err(|e| e.to_string())?;
        let started = Instant::now();
        let update = renderer.set_scene(&demo::sample_house());
        println!("scene: {} objects prepared in {:?}", update.changed, started.elapsed());
        let mut camera = OrbitCamera::looking_at(Point3::ORIGIN, StandardView::IsoSouthWest, 20.0);
        if let Some(b) = house_bounds(&renderer) {
            camera.fit_to(&b, Viewport::new(1280.0, 800.0));
        }
        let autoclose =
            std::env::var("KANAVOS_AUTOCLOSE_FRAMES").ok().and_then(|v| v.trim().parse::<u64>().ok());
        Ok(Viewer {
            renderer,
            camera,
            style: DisplayStyle::default(),
            section_box: false,
            ghost_roof: false,
            texture: None,
            hover_ticket: None,
            hover_point: None,
            drag_anchor: None,
            transition: None,
            status: String::new(),
            has_font,
            frames: 0,
            autoclose,
            closing: Closing::Running,
            screenshot_requested: false,
            last_frame: Instant::now(),
            frame_ms: 0.0,
            viewport: Viewport::new(1.0, 1.0),
        })
    }

    /// One egui frame: side panel and viewport.
    fn ui(&mut self, ui: &mut egui::Ui, rs: &egui_wgpu::RenderState) {
        let now = Instant::now();
        let ms = now.duration_since(self.last_frame).as_secs_f32() * 1000.0;
        self.frame_ms = if self.frame_ms > 0.0 { self.frame_ms * 0.9 + ms * 0.1 } else { ms };
        self.last_frame = now;
        self.keyboard(ui.ctx());
        if self.has_font {
            self.side_panel(ui);
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.viewport_ui(ui, rs));
    }

    fn params(&self, size: [u32; 2], ppp: f32) -> ViewParams {
        let mut p = ViewParams::new(self.camera, size, ppp);
        p.style = self.style.clone();
        if self.section_box {
            let o = demo::HOUSE_ORIGIN;
            p.set_section_box(&Bbox3 {
                min: Point3::new(o.x - 20.0, o.y - 20.0, o.z - 2.0),
                max: Point3::new(o.x + 30.0, o.y + 30.0, o.z + 1.4),
            });
        }
        p
    }

    fn go_to(&mut self, view: StandardView) {
        let mut to = self.camera;
        to.set_view(view);
        if let Some(b) = house_bounds(&self.renderer) {
            to.fit_to(&b, self.viewport);
        }
        self.transition = Some(Transition { from: self.camera, to, start: Instant::now() });
    }

    fn fit(&mut self) {
        let mut to = self.camera;
        if let Some(b) = self.renderer.visible_bounds() {
            to.fit_to(&b, self.viewport);
        }
        self.transition = Some(Transition { from: self.camera, to, start: Instant::now() });
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        if pressed(egui::Key::E) {
            self.style.edges = !self.style.edges;
        }
        if pressed(egui::Key::S) {
            self.style.shadows = !self.style.shadows;
        }
        if pressed(egui::Key::A) {
            self.style.ambient_occlusion = !self.style.ambient_occlusion;
        }
        if pressed(egui::Key::G) {
            self.style.grid = !self.style.grid;
        }
        if pressed(egui::Key::B) {
            self.section_box = !self.section_box;
        }
        if pressed(egui::Key::H) {
            self.ghost_roof = !self.ghost_roof;
        }
        if pressed(egui::Key::P) {
            self.camera.projection = match self.camera.projection {
                Projection::Perspective => Projection::Parallel,
                Projection::Parallel => Projection::Perspective,
            };
        }
        if pressed(egui::Key::F) {
            self.fit();
        }
        for (key, view) in VIEW_KEYS {
            if pressed(key) {
                self.go_to(view);
            }
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("kanavos_viewer_controls").default_size(190.0).show(ui, |ui| {
            ui.heading("Display");
            ui.checkbox(&mut self.style.edges, "Edges (E)");
            ui.checkbox(&mut self.style.silhouettes, "Silhouettes");
            ui.checkbox(&mut self.style.shadows, "Shadows (S)");
            ui.checkbox(&mut self.style.ambient_occlusion, "Ambient occlusion (A)");
            ui.checkbox(&mut self.style.grid, "Grid (G)");
            ui.checkbox(&mut self.section_box, "Section box (B)");
            ui.checkbox(&mut self.ghost_roof, "Ghost roof (H)");
            let mut parallel = self.camera.projection == Projection::Parallel;
            if ui.checkbox(&mut parallel, "Parallel projection (P)").changed() {
                self.camera.projection =
                    if parallel { Projection::Parallel } else { Projection::Perspective };
            }
            ui.add(egui::Slider::new(&mut self.style.sun.azimuth_deg, 0.0..=360.0).text("Sun bearing"));
            ui.add(egui::Slider::new(&mut self.style.sun.altitude_deg, 5.0..=85.0).text("Sun height"));
            ui.separator();
            ui.heading("Views");
            ui.horizontal_wrapped(|ui| {
                for (label, view) in [
                    ("Top", StandardView::Top),
                    ("Front", StandardView::Front),
                    ("Right", StandardView::Right),
                    ("Back", StandardView::Back),
                    ("Left", StandardView::Left),
                    ("SW", StandardView::IsoSouthWest),
                    ("SE", StandardView::IsoSouthEast),
                    ("NE", StandardView::IsoNorthEast),
                    ("NW", StandardView::IsoNorthWest),
                ] {
                    if ui.button(label).clicked() {
                        self.go_to(view);
                    }
                }
            });
            if ui.button("Fit (F)").clicked() {
                self.fit();
            }
            ui.separator();
            ui.label(&self.status);
            ui.label(format!("frame {:.1} ms", self.frame_ms));
        });
    }

    /// Draws the X/Y/Z orientation gizmo in the bottom-left corner of the viewport.
    fn gizmo(&self, painter: &egui::Painter, rect: egui::Rect) {
        let centre = egui::pos2(rect.left() + 48.0, rect.bottom() - 48.0);
        let mut axes = self.camera.axis_gizmo();
        axes.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        for a in axes {
            let (name, color) = if a.axis.x > 0.5 {
                ("X", egui::Color32::from_rgb(196, 70, 60))
            } else if a.axis.y > 0.5 {
                ("Y", egui::Color32::from_rgb(70, 150, 70))
            } else {
                ("Z", egui::Color32::from_rgb(60, 100, 200))
            };
            let dir = egui::vec2(a.screen[0] as f32, a.screen[1] as f32);
            let tip = centre + dir * 32.0;
            painter.line_segment([centre, tip], egui::Stroke::new(2.5, color));
            painter.circle_filled(tip, 3.5, color);
            if self.has_font {
                painter.text(
                    tip + dir * 10.0,
                    egui::Align2::CENTER_CENTER,
                    name,
                    egui::FontId::proportional(12.0),
                    color,
                );
            }
        }
    }

    /// The 3D viewport: navigation, picking, rendering and display. In an eframe application this is the body
    /// of the viewport widget, with `rs = frame.wgpu_render_state()`.
    fn viewport_ui(&mut self, ui: &mut egui::Ui, rs: &egui_wgpu::RenderState) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let size =
            [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        self.viewport = Viewport::new(f64::from(size[0]), f64::from(size[1]));
        let (device, queue) = (&rs.device, &rs.queue);
        // Cursor positions in physical pixels relative to the viewport, as the renderer expects.
        let to_px =
            |p: egui::Pos2| [f64::from((p.x - rect.left()) * ppp), f64::from((p.y - rect.top()) * ppp)];

        // Camera animation.
        if let Some(t) = &self.transition {
            let s = (t.start.elapsed().as_secs_f64() / TRANSITION_SECONDS).min(1.0);
            let eased = s * s * (3.0 - 2.0 * s);
            self.camera = t.from.interpolate(&t.to, eased);
            if s >= 1.0 {
                self.transition = None;
            }
            ui.ctx().request_repaint();
        }

        // Navigation.
        let params = self.params(size, ppp);
        if response.drag_started() {
            self.transition = None;
            self.drag_anchor = response
                .interact_pointer_pos()
                .and_then(|p| self.renderer.pick(device, queue, &params, to_px(p)).ok())
                .and_then(|hit| hit.position);
        }
        let delta = response.drag_delta() * ppp;
        let (dx, dy) = (f64::from(delta.x), f64::from(delta.y));
        if response.dragged_by(egui::PointerButton::Primary) {
            let k = kanavos_render::ORBIT_RADIANS_PER_PIXEL;
            match self.drag_anchor {
                Some(pivot) => self.camera.orbit_about(pivot, -dx * k, dy * k),
                None => self.camera.orbit(-dx * k, dy * k),
            }
        } else if response.dragged_by(egui::PointerButton::Secondary)
            || response.dragged_by(egui::PointerButton::Middle)
        {
            self.camera.pan_pixels(dx, dy, self.viewport, self.drag_anchor);
        }
        if response.drag_stopped() {
            self.drag_anchor = None;
        }
        if let Some(pos) = response.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.transition = None;
                let factor = (-f64::from(scroll) * 0.0025).exp();
                self.camera.zoom_at(factor, to_px(pos), self.viewport, self.hover_point);
            }
        }
        if response.double_clicked() {
            self.fit();
        } else if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            match self.renderer.pick(device, queue, &params, to_px(pos)) {
                Ok(hit) => {
                    println!("picked {:?} at {:?}", hit.id, hit.position);
                    self.renderer.set_selection(hit.id);
                    self.status = match (hit.id, hit.position) {
                        (Some(id), Some(p)) => format!("picked {id} at\n{:.3}, {:.3}, {:.3}", p.x, p.y, p.z),
                        _ => "nothing picked".to_owned(),
                    };
                }
                Err(e) => self.status = format!("pick failed: {e}"),
            }
        }

        // Hover: one asynchronous pick in flight at a time.
        for (ticket, hit) in self.renderer.poll_picks(device) {
            if Some(ticket) == self.hover_ticket {
                self.hover_ticket = None;
                self.hover_point = hit.position;
                self.renderer.set_hovered(hit.id);
            }
        }
        let params = self.params(size, ppp);
        match response.hover_pos() {
            Some(pos) if self.hover_ticket.is_none() && !response.dragged() => {
                self.hover_ticket = self.renderer.request_pick(device, queue, &params, to_px(pos)).ok();
            }
            None => self.renderer.set_hovered(None),
            _ => {}
        }
        if self.hover_ticket.is_some() {
            ui.ctx().request_repaint();
        }

        self.renderer.set_ghosted(if self.ghost_roof {
            vec![ids::ROOF_SOUTH, ids::ROOF_NORTH]
        } else {
            vec![]
        });

        // Render, register the texture with egui (again after a resize), and show it.
        let out = self.renderer.render(device, queue, &params);
        let id = {
            let mut egui_renderer = rs.renderer.write();
            match self.texture {
                Some((id, generation)) if generation == out.generation => id,
                Some((id, _)) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(
                        device,
                        &out.view,
                        wgpu::FilterMode::Nearest,
                        id,
                    );
                    id
                }
                None => egui_renderer.register_native_texture(device, &out.view, wgpu::FilterMode::Nearest),
            }
        };
        self.texture = Some((id, out.generation));
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(id, rect, uv, egui::Color32::WHITE);
        self.gizmo(ui.painter(), rect);
        if self.has_font {
            let s = out.stats;
            let text =
                format!("{} objects · {} triangles · {} draw calls", s.objects, s.triangles, s.draw_calls);
            ui.painter().text(
                rect.right_bottom() + egui::vec2(-10.0, -8.0),
                egui::Align2::RIGHT_BOTTOM,
                text,
                egui::FontId::proportional(12.0),
                egui::Color32::from_gray(70),
            );
        }

        // Smoke-test mode.
        self.frames += 1;
        let Some(limit) = self.autoclose else { return };
        ui.ctx().request_repaint();
        match self.closing {
            Closing::Running if self.frames >= limit => {
                let centre = [f64::from(size[0]) * 0.5, f64::from(size[1]) * 0.5];
                match self.renderer.pick(device, queue, &params, centre) {
                    Ok(hit) => println!("picked at centre: {:?} at {:?}", hit.id, hit.position),
                    Err(e) => println!("pick at centre failed: {e}"),
                }
                println!(
                    "rendered {} frames, frame time {:.2} ms, {:?}",
                    self.frames, self.frame_ms, out.stats
                );
                if std::env::var_os("KANAVOS_SCREENSHOT").is_some() {
                    self.screenshot_requested = true;
                    self.closing = Closing::AwaitingScreenshot;
                } else {
                    self.closing = Closing::Done;
                }
            }
            Closing::AwaitingScreenshot => {
                let shot = ui.input(|i| {
                    i.raw.events.iter().find_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                });
                if let Some(image) = shot {
                    if let Some(path) = std::env::var_os("KANAVOS_SCREENSHOT") {
                        let path = std::path::Path::new(&path);
                        match save_png(&image, path) {
                            Ok(()) => println!("screenshot written to {}", path.display()),
                            Err(e) => println!("screenshot failed: {e}"),
                        }
                    }
                    self.closing = Closing::Done;
                }
            }
            _ => {}
        }
    }
}

fn save_png(image: &egui::ColorImage, path: &std::path::Path) -> Result<(), String> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------------------------------------------
// Window and egui plumbing (what eframe does for an application).

struct Running {
    window: Arc<Window>,
    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
    painter: egui_wgpu::winit::Painter,
    render_state: egui_wgpu::RenderState,
    viewer: Viewer,
    next_repaint: Option<Instant>,
}

#[derive(Default)]
struct App {
    running: Option<Running>,
    error: Option<String>,
}

impl App {
    fn start(event_loop: &ActiveEventLoop) -> Result<Running, String> {
        let attributes = Window::default_attributes()
            .with_title("Kanavos viewer")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(event_loop.create_window(attributes).map_err(|e| e.to_string())?);
        let egui_ctx = egui::Context::default();
        let mut painter = pollster::block_on(egui_wgpu::winit::Painter::new(
            egui_ctx.clone(),
            egui_wgpu::WgpuConfiguration::default(),
            false,
            egui_wgpu::RendererOptions::default(),
        ));
        pollster::block_on(painter.set_window(egui::ViewportId::ROOT, Some(Arc::clone(&window))))
            .map_err(|e| e.to_string())?;
        let render_state = painter.render_state().ok_or("no wgpu render state")?;
        let egui_winit = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            painter.max_texture_side(),
        );
        let viewer = Viewer::new(&egui_ctx, &render_state)?;
        window.request_redraw();
        Ok(Running { window, egui_ctx, egui_winit, painter, render_state, viewer, next_repaint: None })
    }
}

impl Running {
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let mut input = self.egui_winit.take_egui_input(&self.window);
        self.painter.handle_screenshots(&mut input.events);
        let (viewer, rs) = (&mut self.viewer, &self.render_state);
        let mut output = self.egui_ctx.run_ui(input, |ui| viewer.ui(ui, rs));
        self.egui_winit.handle_platform_output(&self.window, std::mem::take(&mut output.platform_output));
        let primitives =
            self.egui_ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        let capture = if std::mem::take(&mut self.viewer.screenshot_requested) {
            vec![egui::UserData::default()]
        } else {
            vec![]
        };
        self.painter.paint_and_update_textures(
            egui::ViewportId::ROOT,
            output.pixels_per_point,
            [0.0, 0.0, 0.0, 1.0],
            &primitives,
            &mut output.textures_delta,
            capture,
            &self.window,
        );
        if self.viewer.closing == Closing::Done {
            event_loop.exit();
            return;
        }
        let delay = output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.repaint_delay);
        match delay {
            Some(d) if d.is_zero() => self.window.request_redraw(),
            Some(d) if d < Duration::from_secs(3600) => self.next_repaint = Some(Instant::now() + d),
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match App::start(event_loop) {
            Ok(r) => self.running = Some(r),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(r) = self.running.as_mut() else { return };
        let response = r.egui_winit.on_window_event(&r.window, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    r.painter.on_window_resized(egui::ViewportId::ROOT, w, h);
                }
                r.window.request_redraw();
            }
            WindowEvent::RedrawRequested => r.redraw(event_loop),
            _ => {}
        }
        if response.repaint {
            r.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(r) = self.running.as_mut() else { return };
        match r.next_repaint {
            Some(t) if Instant::now() >= t => {
                r.next_repaint = None;
                r.window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(t) => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn main() -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = App::default();
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
