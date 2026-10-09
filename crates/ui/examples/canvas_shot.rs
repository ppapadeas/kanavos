//! Visual and performance checks of the drawing canvas and the 3D viewport, outside the full application.
//!
//! Usage (the screenshot hook is the one built into the canvas, see `canvas::shot`):
//!
//! ```text
//! KANAVOS_SHOT_SCENE=plan|detail|select|sheet|section|3d|3d-storey|3d-box|3d-select|3d-top|split|bench|bench3d \
//! KANAVOS_SHOT_HOUSES=1 KANAVOS_CANVAS_SHOT=out.png cargo run -p kanavos-ui --example canvas_shot
//! ```
//!
//! `bench` pans and zooms the plan, `bench3d` orbits the 3D view, programmatically for `KANAVOS_SHOT_FRAMES`
//! frames (default 600), and print frame-time statistics instead of taking a screenshot.

use kanavos_engine::Command;
use kanavos_engine::commands::{CreateView, PermitSet};
use kanavos_geom::Point2;
use kanavos_model::{Id, ViewKind};
use kanavos_ui::canvas::{Canvas, CanvasKind, demo};
use kanavos_ui::state::AppState;
use kanavos_ui::viewport3d::{StandardView, StoreyFilter, Viewport3d};
use kanavos_units::Length;

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    Plan,
    Detail,
    Select,
    Sheet,
    Section,
    ThreeD(View3d),
    Split,
    Bench,
    Bench3d,
    Draw,
    Handles,
    TextBox,
}

#[derive(Clone, Copy, PartialEq)]
enum View3d {
    Iso,
    Storey,
    Box,
    Select,
    Top,
}

struct Shot {
    state: AppState,
    scene: Scene,
    canvas: Option<Canvas>,
    viewport: Viewport3d,
    frame: u64,
    frames: u64,
    times: Vec<f32>,
    tess: Vec<f32>,
    /// Whole-frame CPU time reported by eframe (everything but the wait for vsync), milliseconds.
    app: Vec<f32>,
    started: std::time::Instant,
    first: bool,
}

fn main() -> eframe::Result {
    let scene = match std::env::var("KANAVOS_SHOT_SCENE").unwrap_or_default().as_str() {
        "detail" => Scene::Detail,
        "select" => Scene::Select,
        "sheet" => Scene::Sheet,
        "section" => Scene::Section,
        "3d" => Scene::ThreeD(View3d::Iso),
        "3d-storey" => Scene::ThreeD(View3d::Storey),
        "3d-box" => Scene::ThreeD(View3d::Box),
        "3d-select" => Scene::ThreeD(View3d::Select),
        "3d-top" => Scene::ThreeD(View3d::Top),
        "split" => Scene::Split,
        "bench" => Scene::Bench,
        "bench3d" => Scene::Bench3d,
        "draw" => Scene::Draw,
        "handles" => Scene::Handles,
        "text" => Scene::TextBox,
        _ => Scene::Plan,
    };
    let houses: usize = std::env::var("KANAVOS_SHOT_HOUSES").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let frames: u64 = std::env::var("KANAVOS_SHOT_FRAMES").ok().and_then(|s| s.parse().ok()).unwrap_or(600);
    let mut doc = kanavos_model::starter();
    let storey = doc.storeys().first().map(|s| s.id).unwrap_or_default();
    match demo::houses_commands(&doc, storey, houses) {
        Ok(cmds) => {
            for c in &cmds {
                match c.apply(&doc) {
                    Ok((d, _)) => doc = d,
                    Err(e) => eprintln!("command failed: {e}"),
                }
            }
        }
        Err(e) => eprintln!("sample failed: {e}"),
    }
    eprintln!("elements on the storey: {}", doc.elements_on(storey).count());
    let section = CreateView {
        name: "Τομή Α-Α".into(),
        kind: ViewKind::Section {
            start: Point2::new(-2.0, 2.8),
            end: Point2::new(12.0, 2.8),
            depth: Length::m(20.0),
            flip: false,
            marker: "Α".into(),
        },
        scale: 50,
    };
    let mut section_view = Id::default();
    if let Ok((d, out)) = section.apply(&doc) {
        doc = d;
        section_view = out.created.first().copied().unwrap_or_default();
    }
    let mut sheet = Id::default();
    if scene == Scene::Sheet
        && let Ok((d, out)) = PermitSet::default().apply(&doc)
    {
        doc = d;
        // The sheet with the ground-floor plan.
        let plan = doc.plan_view_of(storey).map(|v| v.id);
        sheet = out
            .created
            .iter()
            .copied()
            .find(|s| doc.sheet(*s).is_some_and(|s| s.viewports.iter().any(|v| Some(v.view) == plan)))
            .unwrap_or_default();
    }
    let plan_view = doc.plan_view_of(storey).map(|v| v.id).unwrap_or_default();
    let mut state = AppState::new(doc);
    state.active_storey = storey;
    let canvas = match scene {
        Scene::ThreeD(_) | Scene::Bench3d => None,
        Scene::Sheet => Some(Canvas::new(CanvasKind::Sheet { sheet })),
        Scene::Section => Some(Canvas::new(CanvasKind::Section { view: section_view })),
        _ => Some(Canvas::new(CanvasKind::Plan { view: plan_view })),
    };
    let first_wall = state.doc().walls_on(storey).next().map(|(w, _)| w);
    if matches!(scene, Scene::Select | Scene::Handles | Scene::ThreeD(View3d::Select))
        && let Some(w) = first_wall
    {
        state.selection = [w].into();
    }
    let mut viewport = Viewport3d::new();
    match scene {
        Scene::ThreeD(View3d::Storey) => viewport.set_storey_filter(StoreyFilter::UpTo(storey)),
        Scene::ThreeD(View3d::Box) => viewport.set_section_box([-2.0, -2.0, -1.0], [13.0, 11.0, 1.4]),
        Scene::ThreeD(View3d::Top) => viewport.set_standard_view(StandardView::Top),
        _ => {}
    }
    let app = Shot {
        state,
        scene,
        canvas,
        viewport,
        frame: 0,
        frames,
        times: Vec::new(),
        tess: Vec::new(),
        app: Vec::new(),
        started: std::time::Instant::now(),
        first: true,
    };
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Kanavos canvas check")
            .with_inner_size([1400.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "kanavos-canvas-shot",
        options,
        Box::new(|cc| {
            kanavos_ui::fonts::install(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

impl eframe::App for Shot {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.frame += 1;
        if self.frame > 6
            && let Some(cpu) = frame.info().cpu_usage
        {
            self.app.push(cpu * 1000.0);
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| match self.scene {
            Scene::ThreeD(_) => self.viewport.ui(ui, frame, &mut self.state),
            Scene::Bench3d => {
                if self.frame > 5 {
                    // Orbit continuously, with a slow zoom in and out.
                    let mut c = self.viewport.camera();
                    let t = self.frame as f64 / 60.0;
                    let (dx, dy) = (c.eye.x - c.target.x, c.eye.y - c.target.y);
                    let r = dx.hypot(dy) * (1.0 + 0.004 * (t * 0.8).sin());
                    let a = dy.atan2(dx) + 0.02;
                    c.eye.x = c.target.x + r * a.cos();
                    c.eye.y = c.target.y + r * a.sin();
                    self.viewport.set_camera(&c);
                }
                self.viewport.ui(ui, frame, &mut self.state);
                let s = self.viewport.stats();
                if self.frame == 5 {
                    self.started = std::time::Instant::now();
                }
                if self.frame > 5 {
                    self.times.push(s.frame_ms);
                    self.tess.push(s.render_ms);
                }
            }
            Scene::Split => {
                let w = ui.available_width() / 2.0;
                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::vec2(w, ui.available_height()), |ui| {
                        if let Some(c) = &mut self.canvas {
                            c.ui(ui, &mut self.state);
                        }
                    });
                    ui.allocate_ui(egui::vec2(w, ui.available_height()), |ui| {
                        self.viewport.ui(ui, frame, &mut self.state);
                    });
                });
            }
            _ => {
                if let Some(c) = &mut self.canvas {
                    if self.first && self.scene == Scene::Detail {
                        c.look_at([0.6, 0.4], 900.0);
                    }
                    if self.frame == 4 {
                        script(self.scene, c, &mut self.state);
                    }
                    if self.scene == Scene::Bench && self.frame > 5 {
                        // Alternate a slow pan and a zoom in and out about the centre.
                        let t = self.frame as f64 / 60.0;
                        let z = 60.0 * (1.0 + 0.8 * (t * 0.7).sin()).max(0.2);
                        c.look_at([20.0 + 12.0 * (t * 0.5).cos(), 12.0 + 8.0 * (t * 0.5).sin()], z);
                    }
                    c.ui(ui, &mut self.state);
                    let s = c.stats();
                    if self.frame == 5 {
                        self.started = std::time::Instant::now();
                    }
                    if self.scene == Scene::Bench && self.frame > 5 {
                        self.times.push(s.frame_ms);
                        self.tess.push(s.tessellate_ms);
                    }
                }
            }
        });
        self.first = false;
        if matches!(self.scene, Scene::Bench | Scene::Bench3d) {
            ui.ctx().request_repaint();
            if self.frame == self.frames {
                report(&self.times, &self.tess, self.started.elapsed().as_secs_f64(), self.frame - 5);
                let mut a = self.app.clone();
                a.sort_by(f32::total_cmp);
                let q = |f: f64| a.get(((a.len() as f64 - 1.0) * f) as usize).copied().unwrap_or(0.0);
                eprintln!(
                    "whole frame CPU (eframe, without vsync wait): median {:.2} ms, p95 {:.2}, max {:.2}",
                    q(0.5),
                    q(0.95),
                    q(1.0)
                );
                if let Some(c) = &self.canvas {
                    eprintln!("last frame: {:?}", c.stats());
                } else {
                    eprintln!("last frame: {:?}", self.viewport.stats());
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

/// Drives a tool the way a user would, for the tool scenes.
fn script(scene: Scene, c: &mut Canvas, state: &mut AppState) {
    use kanavos_tools::{Modifiers, MouseButton, ToolEvent, ToolId};
    let m = Modifiers::default();
    let at = |x: f64, y: f64| Point2::new(x, y);
    let click = |c: &mut Canvas, state: &mut AppState, p: Point2| {
        c.send(ToolEvent::Move { at: p, mods: m }, state);
        c.send(ToolEvent::Press { at: p, button: MouseButton::Primary, mods: m }, state);
        c.send(ToolEvent::Release { at: p, button: MouseButton::Primary, mods: m }, state);
    };
    match scene {
        Scene::Draw => {
            state.tool = ToolId::Wall;
            click(c, state, at(10.0, 8.0));
            c.send(ToolEvent::Move { at: at(13.02, 8.03), mods: m }, state);
        }
        Scene::Handles => {
            c.send(ToolEvent::Move { at: at(0.02, 6.0), mods: m }, state);
        }
        Scene::TextBox => {
            state.tool = ToolId::Text;
            click(c, state, at(1.0, 1.0));
            c.send(ToolEvent::Text("Είσοδος".into()), state);
        }
        _ => {}
    }
}

fn report(times: &[f32], tess: &[f32], wall: f64, frames: u64) {
    if times.is_empty() {
        return;
    }
    let mut v = times.to_vec();
    v.sort_by(f32::total_cmp);
    let n = v.len();
    let avg = v.iter().sum::<f32>() / n as f32;
    let p = |q: f64| v.get(((n as f64 - 1.0) * q) as usize).copied().unwrap_or(0.0);
    let rebuilds = tess.iter().filter(|t| **t > 0.0).count();
    let tmax = tess.iter().copied().fold(0.0, f32::max);
    eprintln!(
        "frame CPU in the canvas/viewport ui: avg {avg:.2} ms, median {:.2}, p95 {:.2}, max {:.2} over {n} frames; \
         {rebuilds} re-tessellations, longest {tmax:.2} ms; whole app {:.1} fps",
        p(0.5),
        p(0.95),
        p(1.0),
        frames as f64 / wall
    );
}
