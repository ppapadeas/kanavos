//! Visual and performance checks of the drawing canvas and the 3D viewport, outside the full application.
//!
//! Usage (the screenshot hook is the one built into the canvas, see `canvas::shot`):
//!
//! ```text
//! KANAVOS_SHOT_SCENE=plan|detail|select|sheet|section|3d|split|bench \
//! KANAVOS_SHOT_HOUSES=1 KANAVOS_CANVAS_SHOT=out.png cargo run -p kanavos-ui --example canvas_shot
//! ```
//!
//! `bench` pans and zooms the plan programmatically for `KANAVOS_SHOT_FRAMES` frames (default 600) and prints
//! frame-time statistics instead of taking a screenshot.

use kanavos_engine::Command;
use kanavos_engine::commands::{CreateView, PermitSet};
use kanavos_geom::Point2;
use kanavos_model::{Id, ViewKind};
use kanavos_ui::canvas::{Canvas, CanvasKind, demo};
use kanavos_ui::state::AppState;
use kanavos_ui::viewport3d::Viewport3d;
use kanavos_units::Length;

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    Plan,
    Detail,
    Select,
    Sheet,
    Section,
    ThreeD,
    Split,
    Bench,
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
    started: std::time::Instant,
    first: bool,
}

fn main() -> eframe::Result {
    let scene = match std::env::var("KANAVOS_SHOT_SCENE").unwrap_or_default().as_str() {
        "detail" => Scene::Detail,
        "select" => Scene::Select,
        "sheet" => Scene::Sheet,
        "section" => Scene::Section,
        "3d" => Scene::ThreeD,
        "split" => Scene::Split,
        "bench" => Scene::Bench,
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
        Scene::ThreeD => None,
        Scene::Sheet => Some(Canvas::new(CanvasKind::Sheet { sheet })),
        Scene::Section => Some(Canvas::new(CanvasKind::Section { view: section_view })),
        _ => Some(Canvas::new(CanvasKind::Plan { view: plan_view })),
    };
    let first_wall = state.doc().walls_on(storey).next().map(|(w, _)| w);
    if scene == Scene::Select
        && let Some(w) = first_wall
    {
        state.selection = [w].into();
    }
    let app = Shot {
        state,
        scene,
        canvas,
        viewport: Viewport3d::new(),
        frame: 0,
        frames,
        times: Vec::new(),
        tess: Vec::new(),
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
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| match self.scene {
            Scene::ThreeD => self.viewport.ui(ui, frame, &mut self.state),
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
                    if self.scene == Scene::Bench && self.frame > 5 {
                        // Alternate a slow pan and a zoom in and out about the centre.
                        let t = self.frame as f64 / 60.0;
                        let z = 60.0 * (1.0 + 0.8 * (t * 0.7).sin()).max(0.2);
                        c.look_at([20.0 + 12.0 * (t * 0.5).cos(), 12.0 + 8.0 * (t * 0.5).sin()], z);
                    }
                    c.ui(ui, &mut self.state);
                    let s = c.stats();
                    if self.scene == Scene::Bench && self.frame > 5 {
                        self.times.push(s.frame_ms);
                        self.tess.push(s.tessellate_ms);
                    }
                }
            }
        });
        self.first = false;
        if self.scene == Scene::Bench {
            ui.ctx().request_repaint();
            if self.frame >= self.frames {
                report(&self.times, &self.tess, self.started.elapsed().as_secs_f64(), self.frame);
                if let Some(c) = &self.canvas {
                    eprintln!("last frame: {:?}", c.stats());
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
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
        "canvas frame (CPU in Canvas::ui): avg {avg:.2} ms, median {:.2}, p95 {:.2}, max {:.2} over {n} frames; \
         {rebuilds} re-tessellations, longest {tmax:.2} ms; whole app {:.1} fps",
        p(0.5),
        p(0.95),
        p(1.0),
        frames as f64 / wall
    );
}
