//! Attribute managers and information windows (docs/ux.md §1.2: nothing is modal).
//!
//! Every window is non-modal and live: edits are engine commands applied as soon as a field is committed, so
//! the drawings update immediately and every change can be undone.

mod buildups;
mod info;
mod materials;
mod project;
mod storeys;

use std::collections::HashSet;

use egui::{Color32, Rect, Stroke, Ui, Vec2};
use kanavos_drawing::{HatchPattern, Rgba};
use kanavos_model::Document;

use crate::actions::{Action, Window};
use crate::names::tr;
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets::color32;

pub use buildups::BuildUpsWindow;
pub use materials::MaterialsWindow;

/// The attribute managers and information windows, and which are open.
#[derive(Default)]
pub struct Managers {
    open: HashSet<Window>,
    materials: MaterialsWindow,
    buildups: BuildUpsWindow,
    storeys: storeys::StoreysWindow,
}

impl Managers {
    /// Opens (or brings forward) a window.
    pub fn open(&mut self, w: Window) {
        self.open.insert(w);
    }

    /// Whether a window is open.
    pub fn is_open(&self, w: Window) -> bool {
        self.open.contains(&w)
    }

    /// Closes every window; returns whether any was open.
    pub fn close_all(&mut self) -> bool {
        let any = !self.open.is_empty();
        self.open.clear();
        any
    }

    /// Shows the open windows.
    pub fn ui(&mut self, ctx: &egui::Context, state: &mut AppState, actions: &mut Vec<Action>) {
        let locale = state.locale;
        let all = [
            Window::Materials,
            Window::BuildUps,
            Window::Storeys,
            Window::Project,
            Window::Site,
            Window::Defaults,
            Window::Shortcuts,
            Window::About,
        ];
        for w in all {
            if !self.open.contains(&w) {
                continue;
            }
            let mut open = true;
            let (title, size) = match w {
                Window::Materials => (tr(locale, "ui.window.materials"), [720.0, 480.0]),
                Window::BuildUps => (tr(locale, "ui.window.buildups"), [760.0, 520.0]),
                Window::Storeys => (tr(locale, "ui.window.storeys"), [720.0, 320.0]),
                Window::Project => (tr(locale, "ui.window.project"), [460.0, 460.0]),
                Window::Site => (tr(locale, "ui.window.site"), [460.0, 560.0]),
                Window::Defaults => (tr(locale, "ui.window.defaults"), [460.0, 320.0]),
                Window::Shortcuts => (tr(locale, "ui.window.shortcuts"), [460.0, 520.0]),
                Window::About => (tr(locale, "ui.window.about"), [480.0, 380.0]),
            };
            let centre = ctx.content_rect().center();
            egui::Window::new(egui::RichText::new(title).family(crate::fonts::strong()))
                .id(egui::Id::new(("kanavos-window", format!("{w:?}"))))
                .default_pos(centre)
                .pivot(egui::Align2::CENTER_CENTER)
                .open(&mut open)
                .default_size(size)
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| match w {
                    Window::Materials => self.materials.ui(ui, state),
                    Window::BuildUps => self.buildups.ui(ui, state),
                    Window::Storeys => self.storeys.ui(ui, state, actions),
                    Window::Project => project::project_ui(ui, state),
                    Window::Site => project::site_ui(ui, state, actions),
                    Window::Defaults => project::defaults_ui(ui, state),
                    Window::Shortcuts => info::shortcuts_ui(ui, locale),
                    Window::About => info::about_ui(ui, locale),
                });
            if !open {
                self.open.remove(&w);
            }
        }
    }
}

/// Paints a small preview of a hatch pattern over a background (an indication, not the printed hatch).
pub fn hatch_preview(
    ui: &mut Ui,
    pattern: HatchPattern,
    background: Option<Rgba>,
    size: Vec2,
) -> egui::Response {
    let t = Tokens::of(ui);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_hatch(
            ui.painter(),
            rect,
            pattern,
            background.map(color32).unwrap_or(Color32::WHITE),
            Color32::from_gray(40),
        );
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    }
    response
}

/// Paints a hatch indication into a rectangle.
pub fn paint_hatch(p: &egui::Painter, rect: Rect, pattern: HatchPattern, bg: Color32, ink: Color32) {
    p.rect_filled(rect, 0.0, bg);
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let stroke = Stroke::new(0.8, ink);
    let (w, h) = (rect.width(), rect.height());
    let diag = |step: f32, dir: f32| {
        let mut x = -h;
        while x < w + h {
            let a = rect.left_bottom() + Vec2::new(x, 0.0);
            let b = a + Vec2::new(h * dir, -h);
            p.line_segment([a, b], stroke);
            x += step;
        }
    };
    match pattern {
        HatchPattern::None => {}
        HatchPattern::Diagonal => diag(6.0, 1.0),
        HatchPattern::Masonry => diag(3.0, 1.0),
        HatchPattern::CrossDiagonal => {
            diag(6.0, 1.0);
            let mut x = -h;
            while x < w + h {
                let a = rect.left_top() + Vec2::new(x, 0.0);
                p.line_segment([a, a + Vec2::new(h, h)], stroke);
                x += 6.0;
            }
        }
        HatchPattern::Concrete | HatchPattern::Dots | HatchPattern::Gravel | HatchPattern::Earth => {
            let step = match pattern {
                HatchPattern::Dots => 4.0,
                HatchPattern::Gravel => 7.0,
                _ => 5.0,
            };
            let mut row = 0;
            let mut y = rect.top() + step / 2.0;
            while y < rect.bottom() {
                let offset = if row % 2 == 0 { 0.0 } else { step / 2.0 };
                let mut x = rect.left() + step / 2.0 + offset;
                while x < rect.right() {
                    let c = egui::pos2(x, y);
                    match pattern {
                        HatchPattern::Gravel => {
                            p.circle_stroke(c, 1.8, stroke);
                        }
                        HatchPattern::Earth => {
                            p.line_segment([c + Vec2::new(-1.5, 1.5), c + Vec2::new(1.5, -1.5)], stroke);
                        }
                        HatchPattern::Concrete if row % 2 == 1 => {
                            p.add(egui::Shape::convex_polygon(
                                vec![
                                    c + Vec2::new(0.0, -1.5),
                                    c + Vec2::new(1.5, 1.2),
                                    c + Vec2::new(-1.5, 1.2),
                                ],
                                Color32::TRANSPARENT,
                                stroke,
                            ));
                        }
                        _ => {
                            p.circle_filled(c, 0.7, ink);
                        }
                    }
                    x += step;
                }
                y += step;
                row += 1;
            }
        }
        HatchPattern::Insulation => {
            let amp = (h / 2.0 - 2.0).max(2.0);
            let pts: Vec<egui::Pos2> = (0..=40)
                .map(|i| {
                    let x = rect.left() + w * i as f32 / 40.0;
                    let y = rect.center().y + amp * (i as f32 * 0.9).sin();
                    egui::pos2(x, y)
                })
                .collect();
            p.add(egui::Shape::line(pts, stroke));
        }
        HatchPattern::Wood | HatchPattern::Horizontal => {
            let step = if pattern == HatchPattern::Wood { 3.0 } else { 5.0 };
            let mut y = rect.top() + step / 2.0;
            while y < rect.bottom() {
                p.hline(rect.x_range(), y, stroke);
                y += step;
            }
        }
        HatchPattern::Grid => {
            let mut y = rect.top() + 4.0;
            while y < rect.bottom() {
                p.hline(rect.x_range(), y, stroke);
                y += 6.0;
            }
            let mut x = rect.left() + 4.0;
            while x < rect.right() {
                p.vline(x, rect.y_range(), stroke);
                x += 6.0;
            }
        }
    }
}

/// Material name, or "—" when missing.
pub fn material_name(doc: &Document, id: kanavos_model::Id) -> String {
    doc.material(id).map(|m| m.name.clone()).unwrap_or_else(|| "—".into())
}
