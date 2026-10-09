//! The 2D canvas: shows a derived drawing (plan, section, elevation or sheet), handles pan and zoom, and feeds
//! pointer and keyboard input to the active tool.
//!
//! First version: draws lines, polylines, polygons and text of the drawing; forwards events to the tool; runs the
//! tool's commands. Hatches, line patterns, selection highlighting, handles, snap markers and typed-input fields
//! are to be built here.

use kanavos_drawing::{Drawing, DrawingUnits, Pt, Shape};
use kanavos_geom::Point2;
use kanavos_model::Id;
use kanavos_tools::{Modifiers, MouseButton, Tool, ToolContext, ToolEvent, ToolId, make_tool};

use crate::state::AppState;

/// What a canvas shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasKind {
    /// A storey plan (through its plan view).
    Plan {
        /// The plan view.
        view: Id,
    },
    /// A section or elevation view.
    Section {
        /// The view.
        view: Id,
    },
    /// A sheet.
    Sheet {
        /// The sheet.
        sheet: Id,
    },
}

/// A pannable, zoomable drawing canvas.
pub struct Canvas {
    /// What is shown.
    pub kind: CanvasKind,
    /// Model coordinates at the centre of the canvas.
    centre: Pt,
    /// Screen pixels per drawing unit.
    zoom: f32,
    tool: Box<dyn Tool>,
    tool_id: ToolId,
    cache: Option<(u64, Drawing)>,
    preview: Drawing,
    fitted: bool,
}

impl Canvas {
    /// A canvas showing `kind`.
    pub fn new(kind: CanvasKind) -> Canvas {
        Canvas {
            kind,
            centre: Pt::default(),
            zoom: 40.0,
            tool: make_tool(ToolId::Select),
            tool_id: ToolId::Select,
            cache: None,
            preview: Drawing::default(),
            fitted: false,
        }
    }

    fn drawing(&mut self, state: &AppState) -> Drawing {
        let rev = state.session.revision();
        if let Some((r, d)) = &self.cache
            && *r == rev
        {
            return d.clone();
        }
        let doc = state.doc();
        let d = match self.kind {
            CanvasKind::Plan { view } | CanvasKind::Section { view } => {
                kanavos_sheets::render_view(doc, view, state.locale).unwrap_or_default()
            }
            CanvasKind::Sheet { sheet } => {
                kanavos_sheets::sheet_drawing(doc, sheet, state.locale).unwrap_or_default()
            }
        };
        self.cache = Some((rev, d.clone()));
        d
    }

    /// Fits the drawing in the canvas on the next frame.
    pub fn zoom_extents(&mut self) {
        self.fitted = false;
    }

    /// Shows the canvas.
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        if self.tool_id != state.tool {
            self.tool.cancel();
            self.tool = make_tool(state.tool);
            self.tool_id = state.tool;
        }
        let drawing = self.drawing(state);
        let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
        if !self.fitted
            && let Some(b) = drawing.bounds()
        {
            self.centre = b.centre();
            let zx = rect.width() / (b.width().max(1.0) as f32 * 1.1);
            let zy = rect.height() / (b.height().max(1.0) as f32 * 1.1);
            self.zoom = zx.min(zy).max(0.01);
            self.fitted = true;
        }
        // Pan with the middle button; zoom about the cursor with the wheel.
        if response.dragged_by(egui::PointerButton::Middle) {
            let d = response.drag_delta();
            self.centre.x -= f64::from(d.x / self.zoom);
            self.centre.y += f64::from(d.y / self.zoom);
        }
        if let Some(hover) = response.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let c = rect.center();
                let before_x = self.centre.x + f64::from((hover.x - c.x) / self.zoom);
                let before_y = self.centre.y - f64::from((hover.y - c.y) / self.zoom);
                self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.001, 100_000.0);
                let after_x = self.centre.x + f64::from((hover.x - c.x) / self.zoom);
                let after_y = self.centre.y - f64::from((hover.y - c.y) / self.zoom);
                self.centre.x += before_x - after_x;
                self.centre.y += before_y - after_y;
            }
        }
        let (centre, zoom) = (self.centre, self.zoom);
        let to_screen = move |p: Pt| -> egui::Pos2 {
            egui::pos2(
                rect.center().x + (p.x - centre.x) as f32 * zoom,
                rect.center().y - (p.y - centre.y) as f32 * zoom,
            )
        };
        let to_model = move |s: egui::Pos2| -> Pt {
            Pt {
                x: centre.x + f64::from((s.x - rect.center().x) / zoom),
                y: centre.y - f64::from((s.y - rect.center().y) / zoom),
            }
        };

        paint(&painter, &drawing, &to_screen, self.zoom, &state.selection);
        paint(&painter, &self.preview, &to_screen, self.zoom, &state.selection);

        // Tools act in plan canvases.
        if let CanvasKind::Plan { .. } = self.kind {
            let mods = ui.input(|i| Modifiers {
                shift: i.modifiers.shift,
                ctrl: i.modifiers.command,
                alt: i.modifiers.alt,
            });
            let mut events = Vec::new();
            if let Some(pos) = response.hover_pos() {
                let m = to_model(pos);
                let at = Point2::new(m.x, m.y);
                events.push(ToolEvent::Move { at, mods });
                if response.drag_started_by(egui::PointerButton::Primary) || response.clicked() {
                    events.push(ToolEvent::Press { at, button: MouseButton::Primary, mods });
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) || response.clicked() {
                    events.push(ToolEvent::Release { at, button: MouseButton::Primary, mods });
                }
                if response.double_clicked() {
                    events.push(ToolEvent::DoubleClick { at });
                }
            }
            let pixel =
                if drawing.units == DrawingUnits::Metres { 1.0 / f64::from(self.zoom) } else { 0.001 };
            for ev in events {
                let doc = state.doc().clone();
                let selection = state.selection.clone();
                let ctx = ToolContext {
                    doc: &doc,
                    storey: state.active_storey,
                    selection: &selection,
                    pixel_size: pixel,
                    snap: &state.snap,
                    settings: &state.tool_settings,
                    locale: state.locale,
                };
                let out = self.tool.handle(&ctx, &ev);
                self.preview = out.preview;
                if let Some(sel) = out.selection {
                    state.selection = sel;
                }
                if !out.commands.is_empty() {
                    let _ = state.run_all(out.commands, String::new());
                }
            }
        }
    }
}

fn color(c: kanavos_drawing::Rgba) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Paints a drawing (simplified: no hatches or dash patterns yet).
fn paint(
    painter: &egui::Painter,
    d: &Drawing,
    to_screen: &dyn Fn(Pt) -> egui::Pos2,
    zoom: f32,
    selection: &std::collections::BTreeSet<Id>,
) {
    let px_per_mm = match d.units {
        DrawingUnits::Metres => zoom * d.scale as f32 / 1000.0,
        DrawingUnits::Millimetres => zoom,
    };
    for item in &d.items {
        let selected = item.source.is_some_and(|s| selection.contains(&Id(s)));
        let stroke = item.stroke.as_ref().map(|s| {
            let c = if selected { egui::Color32::from_rgb(30, 110, 230) } else { color(s.color) };
            egui::Stroke::new((s.weight_mm as f32 * px_per_mm * 0.5).clamp(0.6, 6.0), c)
        });
        match &item.shape {
            Shape::Line { a, b } => {
                if let Some(s) = stroke {
                    painter.line_segment([to_screen(*a), to_screen(*b)], s);
                }
            }
            Shape::Polyline { points, closed } => {
                if let Some(s) = stroke {
                    let mut pts: Vec<egui::Pos2> = points.iter().map(|p| to_screen(*p)).collect();
                    if *closed && let Some(f) = pts.first().copied() {
                        pts.push(f);
                    }
                    painter.add(egui::Shape::line(pts, s));
                }
            }
            Shape::Polygon { rings } => {
                if let Some(outer) = rings.first() {
                    let pts: Vec<egui::Pos2> = outer.iter().map(|p| to_screen(*p)).collect();
                    let fill = match &item.fill {
                        Some(kanavos_drawing::Fill::Solid(c)) => color(*c),
                        Some(kanavos_drawing::Fill::Hatch { background: Some(c), .. }) => color(*c),
                        Some(kanavos_drawing::Fill::Hatch { .. }) => egui::Color32::from_gray(225),
                        None => egui::Color32::TRANSPARENT,
                    };
                    let fill =
                        if selected { egui::Color32::from_rgba_unmultiplied(30, 110, 230, 80) } else { fill };
                    painter.add(egui::Shape::Path(egui::epaint::PathShape {
                        points: pts,
                        closed: true,
                        fill,
                        stroke: stroke.unwrap_or(egui::Stroke::NONE).into(),
                    }));
                }
            }
            Shape::Arc { centre, radius, start, sweep } => {
                if let Some(s) = stroke {
                    let n = 24;
                    let pts: Vec<egui::Pos2> = (0..=n)
                        .map(|i| {
                            let a = start + sweep * f64::from(i) / f64::from(n);
                            to_screen(Pt { x: centre.x + radius * a.cos(), y: centre.y + radius * a.sin() })
                        })
                        .collect();
                    painter.add(egui::Shape::line(pts, s));
                }
            }
            Shape::Circle { centre, radius } => {
                if let Some(s) = stroke {
                    painter.circle_stroke(to_screen(*centre), *radius as f32 * zoom, s);
                }
            }
            Shape::Text(t) => {
                let size = (t.height_mm as f32 * px_per_mm * 1.4).clamp(1.0, 200.0);
                if size >= 4.0 {
                    let align = match t.anchor {
                        kanavos_drawing::Anchor::Centre => egui::Align2::CENTER_CENTER,
                        kanavos_drawing::Anchor::BottomCentre => egui::Align2::CENTER_BOTTOM,
                        kanavos_drawing::Anchor::TopCentre => egui::Align2::CENTER_TOP,
                        kanavos_drawing::Anchor::MiddleLeft => egui::Align2::LEFT_CENTER,
                        kanavos_drawing::Anchor::MiddleRight => egui::Align2::RIGHT_CENTER,
                        _ => egui::Align2::LEFT_BOTTOM,
                    };
                    painter.text(
                        to_screen(t.position),
                        align,
                        &t.content,
                        egui::FontId::proportional(size),
                        color(t.color),
                    );
                }
            }
        }
    }
}
