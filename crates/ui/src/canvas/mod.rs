//! The 2D canvas: shows a derived drawing (plan, section, elevation or sheet), handles navigation, hosts the
//! interactive tools and offers quick actions on the selection.
//!
//! The canvas contains no model logic. It renders the drawings derived by `kanavos-views`/`kanavos-sheets`,
//! turns pointer and keyboard input into tool events, draws what the tools return (preview, snap, typed input)
//! and runs their commands through [`AppState::run_all`].
//!
//! Rendering is split by how often things change:
//! - per drawing revision ([`prepare`]): fill triangulation, hatch lines, bounds, source index, hit-testing;
//! - per zoom level and window ([`tess`]): stroke widths, dashes and levels of detail, tessellated into one mesh;
//! - per frame: the cached mesh is translated and scaled to the current view, text is laid out, overlays drawn.
//!
//! Behaviour is specified in `specs/ui/canvas-*.md`.

mod actions;
pub mod demo;
mod hatch;
mod overlay;
mod prepare;
pub(crate) mod shot;
mod tess;
mod text;
mod view;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use egui::epaint::Mesh;
use egui::{Color32, CursorIcon, Event, Key as EKey, PointerButton, Pos2, Rect as ERect, Sense, pos2, vec2};
use kanavos_drawing::{Drawing, DrawingUnits, Pt, Rect, pt};
use kanavos_engine::AnyCommand;
use kanavos_engine::commands::{DeleteElements, UpdateSheet};
use kanavos_geom::Point2;
use kanavos_i18n::t;
use kanavos_model::Id;
use kanavos_tools::{
    CursorHint, InputField, Key, Modifiers, MouseButton, SnapResult, Tool, ToolContext, ToolEvent, ToolId,
    make_tool,
};
use kanavos_units::Locale;

use crate::state::AppState;
use actions::QuickAction;
use prepare::Prepared;
use tess::{Builder, Style, TessParams};
use view::{ViewAnimation, ViewTransform, ZoomLimits};

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

/// Timing and size of the last frame, for performance checks.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CanvasStats {
    /// Time spent in [`Canvas::ui`], milliseconds.
    pub frame_ms: f32,
    /// Time spent re-tessellating in this frame (0 when the cached mesh was reused), milliseconds.
    pub tessellate_ms: f32,
    /// Time of the last drawing preparation (per revision), milliseconds.
    pub prepare_ms: f32,
    /// Vertices drawn for the drawing itself.
    pub vertices: usize,
    /// Items in the prepared drawing.
    pub items: usize,
    /// Text items drawn this frame.
    pub texts: usize,
}

/// The derived drawing of a revision.
struct Derived {
    revision: u64,
    locale: Locale,
    generation: u64,
    prepared: Arc<Prepared>,
    /// Sheet paper, millimetres.
    page: Option<Rect>,
    /// Sheet viewports: index in the sheet and bounds on the sheet.
    frames: Vec<(usize, Rect)>,
}

/// A tessellated mesh in mesh space.
struct MeshCache {
    generation: u64,
    params: TessParams,
    mesh: Mesh,
}

/// The selection/hover overlay mesh.
struct OverlayCache {
    generation: u64,
    zoom: f64,
    anchor: Pt,
    selection: BTreeSet<u64>,
    hover: Option<u64>,
    mesh: Mesh,
}

/// A viewport being dragged on a sheet.
#[derive(Debug, Clone, Copy)]
struct ViewportDrag {
    index: usize,
    from: Pt,
    to: Pt,
}

/// A pannable, zoomable drawing canvas.
pub struct Canvas {
    /// What is shown.
    pub kind: CanvasKind,
    view: ViewTransform,
    fitted: bool,
    anim: Option<ViewAnimation>,
    fit_request: Option<FitRequest>,
    tool: Box<dyn Tool>,
    tool_id: ToolId,
    derived: Option<Derived>,
    generation: u64,
    base: Option<MeshCache>,
    overlay: Option<OverlayCache>,
    // Tool output kept between events.
    preview: Drawing,
    snap: Option<SnapResult>,
    fields: Vec<InputField>,
    prompt: String,
    cursor_hint: CursorHint,
    cursor_model: Option<Point2>,
    // Input state.
    primary_down: bool,
    panning: bool,
    last_move: Option<Pt>,
    clicked_at: Option<Point2>,
    hover_set: bool,
    // Sheets.
    viewport_selected: Option<usize>,
    viewport_drag: Option<ViewportDrag>,
    // Requests for the shell.
    properties_requested: bool,
    strip_size: egui::Vec2,
    stats: CanvasStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FitRequest {
    Extents,
    Selection,
}

/// Fraction of the remaining distance a view animation covers per frame.
const ANIMATION_STEP: f64 = 0.35;
/// Re-tessellate when the zoom drifts by more than this factor from the cached mesh's zoom.
const RETESSELLATE_FACTOR: f64 = 1.19;
/// Pick radius in screen points.
const PICK_RADIUS_PT: f64 = 6.0;
/// Wheel zoom per point of scroll (a typical wheel notch scrolls 50 points: about ×1.2).
const WHEEL_ZOOM_PER_PT: f64 = 0.0036;

impl Canvas {
    /// A canvas showing `kind`.
    pub fn new(kind: CanvasKind) -> Canvas {
        Canvas {
            kind,
            view: ViewTransform::new(Pt::default(), 40.0, [0.0, 0.0]),
            fitted: false,
            anim: None,
            fit_request: None,
            tool: make_tool(ToolId::Select),
            tool_id: ToolId::Select,
            derived: None,
            generation: 0,
            base: None,
            overlay: None,
            preview: Drawing::default(),
            snap: None,
            fields: Vec::new(),
            prompt: String::new(),
            cursor_hint: CursorHint::Arrow,
            cursor_model: None,
            primary_down: false,
            panning: false,
            last_move: None,
            clicked_at: None,
            hover_set: false,
            viewport_selected: None,
            viewport_drag: None,
            properties_requested: false,
            strip_size: vec2(320.0, 30.0),
            stats: CanvasStats::default(),
        }
    }

    /// Fits the drawing (or the sheet) in the canvas, animated.
    pub fn zoom_extents(&mut self) {
        self.fit_request = Some(FitRequest::Extents);
    }

    /// Fits the selection in the canvas, animated (nothing happens when nothing selected is shown here).
    pub fn zoom_to_selection(&mut self) {
        self.fit_request = Some(FitRequest::Selection);
    }

    /// The pointer position in model coordinates (metres; plan canvases only), after snapping when a tool snapped.
    pub fn cursor_model(&self) -> Option<Point2> {
        self.cursor_model
    }

    /// The prompt of the active tool, for the status bar.
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Whether the user is typing a value into the tool (letter keys then belong to the canvas, not to shortcuts).
    pub fn is_typing(&self) -> bool {
        self.fields.iter().any(|f| f.locked)
    }

    /// Whether the "properties" item of the context menu was chosen since the last call.
    pub fn take_properties_request(&mut self) -> bool {
        std::mem::take(&mut self.properties_requested)
    }

    /// The selected viewport of a sheet canvas (index into the sheet's viewports).
    pub fn selected_viewport(&self) -> Option<usize> {
        self.viewport_selected
    }

    /// Screen points per drawing unit (metres for model views, millimetres for sheets).
    pub fn zoom(&self) -> f64 {
        self.view.zoom
    }

    /// The drawing point at the centre of the canvas.
    pub fn centre(&self) -> [f64; 2] {
        [self.view.centre.x, self.view.centre.y]
    }

    /// Centres the view on a drawing point at a zoom (screen points per drawing unit), without animation; e.g. to
    /// restore a saved view. The zoom is clamped to the canvas limits.
    pub fn look_at(&mut self, centre: [f64; 2], zoom: f64) {
        if centre.iter().all(|v| v.is_finite()) {
            self.view.centre = pt(centre[0], centre[1]);
        }
        self.view.zoom = self.limits().clamp(zoom);
        self.fitted = true;
        self.anim = None;
        self.fit_request = None;
    }

    /// Timing of the last frame.
    pub fn stats(&self) -> CanvasStats {
        self.stats
    }

    fn derive(&mut self, state: &AppState) {
        let rev = state.session.revision();
        if let Some(d) = &self.derived
            && d.revision == rev
            && d.locale == state.locale
        {
            return;
        }
        let started = Instant::now();
        let doc = state.doc();
        let mut page = None;
        let mut frames = Vec::new();
        let drawing = match self.kind {
            CanvasKind::Plan { view } | CanvasKind::Section { view } => {
                kanavos_sheets::render_view(doc, view, state.locale).unwrap_or_default()
            }
            CanvasKind::Sheet { sheet } => {
                if let Some(s) = doc.sheet(sheet) {
                    let (w, h) = s.paper.dimensions_mm();
                    page = Some(Rect { min: pt(0.0, 0.0), max: pt(w, h) });
                    for (i, vp) in s.viewports.iter().enumerate() {
                        let (Some(v), Ok(d)) =
                            (doc.view(vp.view), kanavos_sheets::render_view(doc, vp.view, state.locale))
                        else {
                            continue;
                        };
                        let m = kanavos_sheets::viewport_map(&d, vp, v);
                        if let Some(b) = d.transformed(&m).bounds() {
                            frames.push((i, b));
                        }
                    }
                }
                kanavos_sheets::sheet_drawing(doc, sheet, state.locale).unwrap_or_else(|_| Drawing::paper())
            }
        };
        self.generation += 1;
        let prepared = Arc::new(Prepared::new(&drawing));
        self.stats.prepare_ms = started.elapsed().as_secs_f32() * 1000.0;
        self.stats.items = prepared.items.len();
        self.derived = Some(Derived {
            revision: rev,
            locale: state.locale,
            generation: self.generation,
            prepared,
            page,
            frames,
        });
        if let Some(i) = self.viewport_selected
            && !self.derived.as_ref().is_some_and(|d| d.frames.iter().any(|(j, _)| *j == i))
        {
            self.viewport_selected = None;
        }
    }

    fn units(&self) -> DrawingUnits {
        match self.kind {
            CanvasKind::Sheet { .. } => DrawingUnits::Millimetres,
            _ => DrawingUnits::Metres,
        }
    }

    fn limits(&self) -> ZoomLimits {
        ZoomLimits::for_units(self.units())
    }

    /// The box zoom extents fits: the page for sheets, the drawing otherwise.
    fn extents(&self) -> Option<Rect> {
        let d = self.derived.as_ref()?;
        d.page.or(d.prepared.bounds)
    }

    fn fit_target(&self, b: &Rect, size: [f64; 2], margin: f64) -> ViewAnimation {
        let min_extent = match self.units() {
            DrawingUnits::Metres => 1.0,
            DrawingUnits::Millimetres => 10.0,
        };
        let (centre, zoom) = view::fit(b, size, margin, min_extent, &self.limits());
        ViewAnimation { centre, zoom }
    }

    /// Shows the canvas.
    pub fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let started = Instant::now();
        shot::tick(ui.ctx(), Some(&mut *state));
        text::install_bold(ui.ctx());
        if self.tool_id != state.tool {
            self.tool.cancel();
            self.tool = make_tool(state.tool);
            self.tool_id = state.tool;
            self.preview = Drawing::default();
            self.snap = None;
            self.fields.clear();
            self.prompt.clear();
        }
        self.derive(state);
        let Some(prep) = self.derived.as_ref().map(|d| d.prepared.clone()) else { return };

        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = response.rect;
        let ctx = ui.ctx().clone();
        self.view.screen_centre = [f64::from(rect.center().x), f64::from(rect.center().y)];
        let size = [f64::from(rect.width()), f64::from(rect.height())];

        // Keyboard focus: clicking the canvas takes it, and it keeps Tab and Escape for the tools.
        if response.clicked()
            || response.drag_started()
            || response.secondary_clicked()
            || response.middle_clicked()
        {
            response.request_focus();
        }
        if response.hovered() && ctx.memory(|m| m.focused().is_none()) {
            response.request_focus();
        }
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter { tab: true, escape: true, horizontal_arrows: true, vertical_arrows: true },
            )
        });
        let has_keys = response.has_focus();

        // Background: white paper for model views; a page with a shadow on a neutral ground for sheets.
        let page = self.derived.as_ref().and_then(|d| d.page);
        let ground = if page.is_some() { Color32::from_rgb(132, 136, 142) } else { Color32::WHITE };
        painter.rect_filled(rect, 0.0, ground);

        // First show: fit at once.
        if !self.fitted {
            if let Some(b) = self.extents() {
                let a = self.fit_target(&b, size, 40.0);
                self.view.centre = a.centre;
                self.view.zoom = a.zoom;
            }
            self.fitted = true;
        }
        if let Some(req) = self.fit_request.take() {
            let target = match req {
                FitRequest::Extents => self.extents(),
                FitRequest::Selection => {
                    let sel: Vec<u64> = state.selection.iter().map(|i| i.0).collect();
                    prep.bounds_of(sel.iter())
                }
            };
            if let Some(b) = target {
                self.anim = Some(self.fit_target(&b, size, 60.0));
            }
        }

        let typing_before = self.is_typing();
        self.navigate(ui, &response, rect, size, has_keys, typing_before);

        if let Some(p) = page {
            let r =
                ERect::from_two_pos(self.view.pos(pt(p.min.x, p.max.y)), self.view.pos(pt(p.max.x, p.min.y)));
            let shadow = egui::epaint::Shadow {
                offset: [3, 4],
                blur: 14,
                spread: 0,
                color: Color32::from_black_alpha(90),
            };
            painter.add(shadow.as_shape(r, 0));
            painter.rect_filled(r, 0.0, Color32::WHITE);
        }

        // Input that changes the model or the selection.
        match self.kind {
            CanvasKind::Plan { .. } => self.tool_input(ui, &response, state, has_keys),
            CanvasKind::Section { .. } => self.select_input(ui, &response, state, &prep, has_keys),
            CanvasKind::Sheet { sheet } => self.sheet_input(ui, &response, state, sheet, has_keys),
        }
        // A command may have changed the drawing.
        self.derive(state);
        let Some(prep) = self.derived.as_ref().map(|d| d.prepared.clone()) else { return };

        // Hover (shared with the other views).
        let pointer = response.hover_pos();
        if let Some(p) = pointer.filter(|_| response.contains_pointer() && !self.panning) {
            let at = self.view.drawing_at(p);
            let doc = state.doc();
            let hit =
                prep.hit(at, PICK_RADIUS_PT * self.view.unit_per_point(), |s| doc.element(Id(s)).is_some());
            state.hover = hit.map(Id);
            self.hover_set = true;
        } else if self.hover_set {
            state.hover = None;
            self.hover_set = false;
        }

        // The drawing.
        let tess_started = Instant::now();
        let rebuilt = self.ensure_base(&prep, size, ctx.pixels_per_point());
        self.stats.tessellate_ms = if rebuilt { tess_started.elapsed().as_secs_f32() * 1000.0 } else { 0.0 };
        if let Some(base) = &self.base {
            let k = (self.view.zoom / base.params.zoom) as f32;
            let origin = self.view.pos(base.params.anchor);
            self.stats.vertices = base.mesh.vertices.len();
            if !base.mesh.is_empty() {
                painter.add(egui::Shape::mesh(tess::place(&base.mesh, origin, k)));
            }
        }
        self.paint_overlay(&painter, &prep, state, ctx.pixels_per_point());
        self.stats.texts = self.paint_texts(&painter, &prep, rect, state);

        // Sheet viewport frames.
        self.paint_viewport_frames(&painter, state);

        // Tool preview, snap and typed input.
        self.paint_tool(&painter, &prep, rect, ctx.pixels_per_point());

        // Empty plan: say how to start.
        if matches!(self.kind, CanvasKind::Plan { .. })
            && state.doc().elements_on(state.active_storey).next().is_none()
            && self.preview.items.is_empty()
        {
            overlay::hint(&painter, rect, &t(state.locale, "ui.canvas.empty_plan"));
        }

        if matches!(self.kind, CanvasKind::Plan { .. } | CanvasKind::Section { .. }) {
            self.quick_strip(ui, &ctx, rect, &prep, state);
            self.context_menu(&response, state, &prep);
        }

        // Cursor.
        if response.hovered() {
            let icon = if self.panning {
                CursorIcon::Grabbing
            } else if ui.input(|i| i.key_down(EKey::Space)) {
                CursorIcon::Grab
            } else if self.viewport_drag.is_some() {
                CursorIcon::Move
            } else {
                overlay::cursor_icon(self.cursor_hint)
            };
            ctx.set_cursor_icon(icon);
        }
        if self.anim.is_some() {
            ctx.request_repaint();
        }
        self.stats.frame_ms = started.elapsed().as_secs_f32() * 1000.0;
    }

    /// Pan, zoom, extents and the view animation.
    fn navigate(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rect: ERect,
        size: [f64; 2],
        has_keys: bool,
        typing: bool,
    ) {
        let limits = self.limits();
        let space = ui.input(|i| i.key_down(EKey::Space)) && (response.hovered() || self.panning);
        let pan_drag = response.dragged_by(PointerButton::Middle)
            || (space && response.dragged_by(PointerButton::Primary));
        if pan_drag {
            let d = response.drag_delta();
            self.view.pan(f64::from(d.x), f64::from(d.y));
            self.anim = None;
        }
        self.panning = pan_drag;
        if response.double_clicked_by(PointerButton::Middle) {
            self.fit_request = None;
            if let Some(b) = self.extents() {
                self.anim = Some(self.fit_target(&b, size, 40.0));
            }
        }
        let anchor = response
            .hover_pos()
            .filter(|_| response.contains_pointer())
            .map(|p| [f64::from(p.x), f64::from(p.y)])
            .unwrap_or(self.view.screen_centre);
        if response.contains_pointer() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            let factor = f64::from(pinch) * (f64::from(scroll.y) * WHEEL_ZOOM_PER_PT).exp();
            if (factor - 1.0).abs() > 1e-6 {
                self.anim = None;
                self.view.zoom_about(anchor, factor, &limits);
            }
            if scroll.x.abs() > 0.0 {
                self.view.pan(f64::from(scroll.x), 0.0);
            }
        }
        if has_keys {
            let (zin, zout, home) = ui.input(|i| {
                let plain = !i.modifiers.command && !i.modifiers.alt;
                (
                    plain && !typing && (i.key_pressed(EKey::Plus) || i.key_pressed(EKey::Equals)),
                    plain && !typing && i.key_pressed(EKey::Minus),
                    plain && i.key_pressed(EKey::Home),
                )
            });
            let step = |me: &mut Canvas, f: f64| {
                let mut v = me
                    .anim
                    .map(|a| ViewTransform { centre: a.centre, zoom: a.zoom, ..me.view })
                    .unwrap_or(me.view);
                v.zoom_about(anchor, f, &limits);
                me.anim = Some(ViewAnimation { centre: v.centre, zoom: v.zoom });
            };
            if zin {
                step(self, 1.25);
            }
            if zout {
                step(self, 1.0 / 1.25);
            }
            if home {
                self.zoom_extents();
                if let Some(b) = self.extents() {
                    self.anim = Some(self.fit_target(&b, size, 40.0));
                    self.fit_request = None;
                }
            }
        }
        if let Some(a) = self.anim
            && a.step(&mut self.view, ANIMATION_STEP)
        {
            self.anim = None;
        }
        let _ = rect;
    }

    /// Tessellates the drawing again when needed; returns whether it did.
    fn ensure_base(&mut self, prep: &Prepared, size: [f64; 2], ppp: f32) -> bool {
        let Some(generation) = self.derived.as_ref().map(|d| d.generation) else { return false };
        let visible = self.view.visible(size);
        let zoom = self.view.zoom;
        let reuse = self.base.as_ref().is_some_and(|b| {
            b.generation == generation
                && (b.params.pixels_per_point - ppp).abs() < 1e-3
                && (zoom / b.params.zoom).max(b.params.zoom / zoom) < RETESSELLATE_FACTOR
                && b.params.window.is_none_or(|w| view::contains_rect(&w, &visible))
        });
        if reuse {
            return false;
        }
        let grown = Rect {
            min: pt(visible.min.x - visible.width(), visible.min.y - visible.height()),
            max: pt(visible.max.x + visible.width(), visible.max.y + visible.height()),
        };
        let all_inside = prep.bounds.is_none_or(|b| view::contains_rect(&grown, &b));
        let (window, anchor) = if all_inside {
            (None, prep.bounds.map(|b| b.centre()).unwrap_or(self.view.centre))
        } else {
            (Some(grown), self.view.centre)
        };
        let params = TessParams { zoom, anchor, window, pixels_per_point: ppp };
        let mut mesh = Mesh::default();
        let mut b = Builder::new(params, prep.paper_unit());
        b.add(prep, None, Style::Normal, &mut mesh);
        self.base = Some(MeshCache { generation, params, mesh });
        true
    }

    /// Selection and hover highlights, cached until either changes.
    fn paint_overlay(&mut self, painter: &egui::Painter, prep: &Prepared, state: &AppState, ppp: f32) {
        let Some(base) = &self.base else { return };
        let selection: BTreeSet<u64> =
            state.selection.iter().map(|i| i.0).filter(|s| prep.by_source.contains_key(s)).collect();
        let hover =
            state.hover.map(|h| h.0).filter(|h| !selection.contains(h) && prep.by_source.contains_key(h));
        if selection.is_empty() && hover.is_none() {
            self.overlay = None;
            return;
        }
        let fresh = self.overlay.as_ref().is_some_and(|o| {
            o.generation == base.generation
                && (o.zoom - base.params.zoom).abs() <= f64::EPSILON * o.zoom
                && o.anchor == base.params.anchor
                && o.selection == selection
                && o.hover == hover
        });
        if !fresh {
            let params = TessParams { window: None, pixels_per_point: ppp, ..base.params };
            let mut mesh = Mesh::default();
            let mut b = Builder::new(params, prep.paper_unit());
            if let Some(h) = hover
                && let Some(ix) = prep.by_source.get(&h)
            {
                b.add(
                    prep,
                    Some(ix),
                    Style::Highlight {
                        color: overlay::HOVER,
                        extra: 2.0,
                        tint: Color32::from_rgba_premultiplied(8, 30, 60, 24),
                    },
                    &mut mesh,
                );
            }
            for s in &selection {
                if let Some(ix) = prep.by_source.get(s) {
                    b.add(
                        prep,
                        Some(ix),
                        Style::Highlight {
                            color: overlay::SELECTION,
                            extra: 0.8,
                            tint: overlay::SELECTION_TINT,
                        },
                        &mut mesh,
                    );
                }
            }
            self.overlay = Some(OverlayCache {
                generation: base.generation,
                zoom: base.params.zoom,
                anchor: base.params.anchor,
                selection,
                hover,
                mesh,
            });
        }
        if let Some(o) = &self.overlay
            && !o.mesh.is_empty()
        {
            let k = (self.view.zoom / o.zoom) as f32;
            painter.add(egui::Shape::mesh(tess::place(&o.mesh, self.view.pos(o.anchor), k)));
        }
    }

    /// Draws the visible text items; returns how many were drawn.
    fn paint_texts(&self, painter: &egui::Painter, prep: &Prepared, rect: ERect, state: &AppState) -> usize {
        let visible = self.view.visible([f64::from(rect.width()), f64::from(rect.height())]);
        let pt_per_mm = self.view.zoom * prep.paper_unit();
        let bold = text::bold_family(painter.ctx());
        let mut n = 0;
        for it in &prep.items {
            let Some(tx) = &it.text else { continue };
            let cap = tx.height_mm * pt_per_mm;
            if cap < text::MIN_TEXT_PT || !view::intersects(&visible, &it.bounds) {
                continue;
            }
            let selected = it.source.is_some_and(|s| state.selection.contains(&Id(s)));
            let color = if selected { overlay::SELECTION } else { prepare::color32(tx.color) };
            if text::paint_text(painter, tx, self.view.pos(tx.position), cap, color, bold.as_ref()) {
                n += 1;
            }
        }
        n
    }

    /// The tool's preview drawing, inference guides, snap glyph and label, and typed-input box.
    fn paint_tool(&mut self, painter: &egui::Painter, prep: &Prepared, rect: ERect, ppp: f32) {
        if !self.preview.items.is_empty() {
            let mut d = self.preview.clone();
            if d.units == DrawingUnits::Metres && prep.units == DrawingUnits::Metres {
                d.scale = prep.scale;
            }
            let pp = Prepared::new(&d);
            let params = TessParams {
                zoom: self.view.zoom,
                anchor: self.view.centre,
                window: None,
                pixels_per_point: ppp,
            };
            let mut mesh = Mesh::default();
            Builder::new(params, pp.paper_unit()).add(&pp, None, Style::Normal, &mut mesh);
            if !mesh.is_empty() {
                painter.add(egui::Shape::mesh(tess::place(&mesh, self.view.pos(self.view.centre), 1.0)));
            }
            let bold = text::bold_family(painter.ctx());
            for it in &pp.items {
                if let Some(tx) = &it.text {
                    text::paint_text(
                        painter,
                        tx,
                        self.view.pos(tx.position),
                        tx.height_mm * self.view.zoom * pp.paper_unit(),
                        prepare::color32(tx.color),
                        bold.as_ref(),
                    );
                }
            }
        }
        let mut next_y = None;
        if let Some(s) = &self.snap {
            for g in &s.guides {
                let [ax, ay] = g.start.xy();
                let [bx, by] = g.end.xy();
                overlay::dashed(
                    painter,
                    self.view.pos(pt(ax, ay)),
                    self.view.pos(pt(bx, by)),
                    overlay::INFERENCE,
                );
            }
            let [x, y] = s.point.xy();
            let p = self.view.pos(pt(x, y));
            if rect.contains(p) {
                overlay::snap_glyph(painter, p, s.hint, overlay::INFERENCE);
                if !s.label.is_empty() {
                    let r = overlay::label(painter, p + vec2(14.0, 10.0), &s.label, overlay::INFERENCE);
                    next_y = Some(r.max.y + 4.0);
                }
            }
        }
        if !self.fields.is_empty()
            && let Some(c) = self.cursor_model
        {
            let [x, y] = c.xy();
            let p = self.view.pos(pt(x, y));
            let at = pos2(p.x + 14.0, next_y.unwrap_or(p.y + 12.0));
            overlay::fields_box(painter, at, &self.fields, rect.shrink(4.0));
        }
    }

    /// Feeds pointer and keyboard input to the active tool and applies what it returns.
    fn tool_input(&mut self, ui: &egui::Ui, response: &egui::Response, state: &mut AppState, has_keys: bool) {
        let events = ui.input(|i| i.events.clone());
        let mods_of = |m: &egui::Modifiers| Modifiers { shift: m.shift, ctrl: m.command, alt: m.alt };
        let now_mods = ui.input(|i| mods_of(&i.modifiers));
        let over = response.contains_pointer();
        let space = ui.input(|i| i.key_down(EKey::Space));
        let mut out_events: Vec<ToolEvent> = Vec::new();
        let at_of = |v: &ViewTransform, p: Pos2| {
            let d = v.drawing_at(p);
            Point2::new(d.x, d.y)
        };
        let mut pending_move: Option<Pos2> = None;
        for ev in &events {
            match ev {
                Event::PointerMoved(p) => {
                    if over || self.primary_down {
                        pending_move = Some(*p);
                    }
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers, .. } => {
                    if let Some(m) = pending_move.take() {
                        out_events
                            .push(ToolEvent::Move { at: at_of(&self.view, m), mods: mods_of(modifiers) });
                    }
                    let at = at_of(&self.view, *pos);
                    if *pressed && over && !space && !self.panning {
                        out_events.push(ToolEvent::Move { at, mods: mods_of(modifiers) });
                        out_events.push(ToolEvent::Press {
                            at,
                            button: MouseButton::Primary,
                            mods: mods_of(modifiers),
                        });
                        self.primary_down = true;
                        self.clicked_at = Some(at);
                    } else if !*pressed && self.primary_down {
                        out_events.push(ToolEvent::Release {
                            at,
                            button: MouseButton::Primary,
                            mods: mods_of(modifiers),
                        });
                        self.primary_down = false;
                    }
                }
                Event::Key { key, pressed: true, modifiers, .. } if has_keys => {
                    if modifiers.command && *key != EKey::Delete {
                        continue;
                    }
                    let mods = mods_of(modifiers);
                    let k = match key {
                        EKey::Enter => Some(Key::Enter),
                        EKey::Backspace => Some(Key::Backspace),
                        EKey::Tab => Some(Key::Tab),
                        EKey::Delete => Some(Key::Delete),
                        EKey::F if !modifiers.any() && !self.tool.is_idle() => Some(Key::Flip),
                        EKey::Escape => {
                            if self.tool.is_idle() && state.tool != ToolId::Select {
                                state.tool = ToolId::Select;
                                None
                            } else {
                                Some(Key::Escape)
                            }
                        }
                        _ => None,
                    };
                    if let Some(k) = k {
                        out_events.push(ToolEvent::Key { key: k, mods });
                    }
                }
                Event::Text(s) if has_keys => {
                    // Typed input starts with a digit or a coordinate prefix (ux.md §3); while typing, units and
                    // operators belong to it too.
                    let typing =
                        self.is_typing() || out_events.iter().any(|e| matches!(e, ToolEvent::Text(_)));
                    let accepted: String = s
                        .chars()
                        .filter(|c| {
                            if typing {
                                !c.is_control() && !c.is_whitespace()
                            } else {
                                c.is_ascii_digit() || matches!(c, '@' | '#' | '.' | ',' | '(')
                            }
                        })
                        .collect();
                    if !accepted.is_empty() && !self.tool.is_idle()
                        || (!accepted.is_empty() && self.tool_id != ToolId::Select)
                    {
                        out_events.push(ToolEvent::Text(accepted));
                    }
                }
                _ => {}
            }
        }
        if let Some(m) = pending_move {
            out_events.push(ToolEvent::Move { at: at_of(&self.view, m), mods: now_mods });
        }
        // The view moved under a still pointer: the model position changed.
        if let Some(p) = response.hover_pos().filter(|_| over)
            && !out_events.iter().any(|e| matches!(e, ToolEvent::Move { .. }))
        {
            let d = self.view.drawing_at(p);
            if self.last_move.is_none_or(|l| (l.x - d.x).abs() > 1e-12 || (l.y - d.y).abs() > 1e-12) {
                out_events.push(ToolEvent::Move { at: Point2::new(d.x, d.y), mods: now_mods });
            }
        }
        if response.double_clicked_by(PointerButton::Primary)
            && let Some(p) = response.interact_pointer_pos()
        {
            out_events.push(ToolEvent::DoubleClick { at: at_of(&self.view, p) });
        }
        if !over && !self.primary_down && self.cursor_model.is_some() && out_events.is_empty() {
            self.cursor_model = None;
            self.snap = None;
        }
        if out_events.is_empty() {
            return;
        }
        let pixel = self.view.unit_per_point();
        for ev in out_events {
            if let ToolEvent::Move { at, .. } = &ev {
                let [x, y] = at.xy();
                self.last_move = Some(pt(x, y));
                self.cursor_model = Some(*at);
            }
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
            let is_move = matches!(ev, ToolEvent::Move { .. });
            let out = self.tool.handle(&ctx, &ev);
            self.preview = out.preview;
            if out.snap.is_some() || is_move {
                if let Some(s) = &out.snap {
                    self.cursor_model = Some(s.point);
                }
                self.snap = out.snap;
            }
            self.fields = out.fields;
            self.prompt = out.prompt;
            self.cursor_hint = out.cursor;
            if let Some(sel) = out.selection {
                state.selection = sel;
            }
            if !out.commands.is_empty() {
                let _ = state.run_all(out.commands, String::new());
            }
        }
    }

    /// Click selection in sections and elevations (by drawing item source).
    fn select_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        state: &mut AppState,
        prep: &Prepared,
        has_keys: bool,
    ) {
        self.prompt = t(state.locale, "ui.canvas.prompt.section").into_owned();
        self.cursor_hint = CursorHint::Arrow;
        if response.clicked_by(PointerButton::Primary)
            && let Some(p) = response.interact_pointer_pos()
        {
            let at = self.view.drawing_at(p);
            let doc = state.doc();
            let hit = prep
                .hit(at, PICK_RADIUS_PT * self.view.unit_per_point(), |s| doc.element(Id(s)).is_some())
                .map(Id);
            let m = ui.input(|i| i.modifiers);
            match hit {
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
        if has_keys {
            let (del, esc) = ui.input(|i| (i.key_pressed(EKey::Delete), i.key_pressed(EKey::Escape)));
            if del && !state.selection.is_empty() {
                let ids: Vec<Id> = state.selection.iter().copied().collect();
                let _ = state.run_all(vec![DeleteElements { ids }.into()], String::new());
            }
            if esc {
                state.selection.clear();
            }
        }
    }

    /// Selecting, dragging and removing viewports on a sheet.
    fn sheet_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        state: &mut AppState,
        sheet: Id,
        has_keys: bool,
    ) {
        self.prompt = t(state.locale, "ui.canvas.prompt.sheet").into_owned();
        self.cursor_hint = CursorHint::Arrow;
        let frames: Vec<(usize, Rect)> = self.derived.as_ref().map(|d| d.frames.clone()).unwrap_or_default();
        let space = ui.input(|i| i.key_down(EKey::Space));
        let hit_frame = |v: &ViewTransform, p: Pos2| -> Option<usize> {
            let at = v.drawing_at(p);
            let m = 2.0;
            frames
                .iter()
                .rev()
                .find(|(_, b)| {
                    at.x >= b.min.x - m && at.x <= b.max.x + m && at.y >= b.min.y - m && at.y <= b.max.y + m
                })
                .map(|(i, _)| *i)
        };
        if let Some(p) = response.hover_pos()
            && hit_frame(&self.view, p).is_some()
        {
            self.cursor_hint = CursorHint::Grab;
        }
        if response.drag_started_by(PointerButton::Primary)
            && !space
            && let Some(p) = response.interact_pointer_pos()
        {
            let hit = hit_frame(&self.view, p);
            self.viewport_selected = hit;
            if let Some(index) = hit {
                let at = self.view.drawing_at(p);
                self.viewport_drag = Some(ViewportDrag { index, from: at, to: at });
            }
        }
        if let Some(drag) = &mut self.viewport_drag
            && let Some(p) = response.interact_pointer_pos()
        {
            drag.to = self.view.drawing_at(p);
        }
        if response.drag_stopped_by(PointerButton::Primary)
            && let Some(drag) = self.viewport_drag.take()
        {
            let (dx, dy) = (drag.to.x - drag.from.x, drag.to.y - drag.from.y);
            if dx.hypot(dy) * self.view.zoom > 2.0
                && let Some(mut s) = state.doc().sheet(sheet).cloned()
                && let Some(vp) = s.viewports.get_mut(drag.index)
            {
                vp.centre = [vp.centre[0] + dx, vp.centre[1] + dy];
                let _ = state.run_all(vec![UpdateSheet { sheet: s }.into()], String::new());
            }
        }
        if response.clicked_by(PointerButton::Primary)
            && let Some(p) = response.interact_pointer_pos()
        {
            self.viewport_selected = hit_frame(&self.view, p);
        }
        if has_keys {
            let (del, esc) = ui.input(|i| (i.key_pressed(EKey::Delete), i.key_pressed(EKey::Escape)));
            if esc {
                self.viewport_selected = None;
                self.viewport_drag = None;
            }
            if del
                && let Some(i) = self.viewport_selected
                && let Some(mut s) = state.doc().sheet(sheet).cloned()
                && i < s.viewports.len()
            {
                s.viewports.remove(i);
                self.viewport_selected = None;
                let _ = state.run_all(vec![UpdateSheet { sheet: s }.into()], String::new());
            }
        }
    }

    fn paint_viewport_frames(&self, painter: &egui::Painter, state: &AppState) {
        let Some(d) = &self.derived else { return };
        let _ = state;
        for (i, b) in &d.frames {
            let selected = self.viewport_selected == Some(*i);
            let dragging = self.viewport_drag.filter(|g| g.index == *i);
            if !selected && dragging.is_none() {
                continue;
            }
            let (dx, dy) = dragging.map(|g| (g.to.x - g.from.x, g.to.y - g.from.y)).unwrap_or((0.0, 0.0));
            let a = self.view.pos(pt(b.min.x + dx, b.max.y + dy));
            let c = self.view.pos(pt(b.max.x + dx, b.min.y + dy));
            let r = ERect::from_two_pos(a, c).expand(4.0);
            let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
            for k in 0..4 {
                if let (Some(p), Some(q)) = (corners.get(k), corners.get((k + 1) % 4)) {
                    overlay::dashed(painter, *p, *q, overlay::SELECTION);
                }
            }
            if dragging.is_some() {
                painter.rect_filled(r, 0.0, Color32::from_rgba_premultiplied(6, 26, 56, 22));
            }
        }
    }

    /// Bounds of the selection in drawing units, and on screen.
    fn selection_bounds(&self, prep: &Prepared, state: &AppState) -> Option<(Rect, ERect)> {
        let sel: Vec<u64> = state.selection.iter().map(|i| i.0).collect();
        let b = prep.bounds_of(sel.iter())?;
        let r = ERect::from_two_pos(self.view.pos(pt(b.min.x, b.max.y)), self.view.pos(pt(b.max.x, b.min.y)));
        Some((b, r))
    }

    /// The quick-action strip next to the selection; it never covers the selection.
    fn quick_strip(
        &mut self,
        ui: &egui::Ui,
        ctx: &egui::Context,
        rect: ERect,
        prep: &Prepared,
        state: &mut AppState,
    ) {
        if self.tool_id != ToolId::Select
            || !self.tool.is_idle()
            || self.primary_down
            || state.selection.is_empty()
            || self.anim.is_some()
            || !matches!(self.kind, CanvasKind::Plan { .. })
        {
            return;
        }
        let Some((bounds, sel_rect)) = self.selection_bounds(prep, state) else { return };
        if !rect.intersects(sel_rect) {
            return;
        }
        let actions =
            actions::quick_actions(state.doc(), &state.selection, state.active_storey, self.clicked_at);
        if actions.is_empty() {
            return;
        }
        let size = self.strip_size;
        let gap = 12.0;
        let mut x = (sel_rect.center().x - size.x / 2.0)
            .clamp(rect.min.x + 4.0, (rect.max.x - size.x - 4.0).max(rect.min.x + 4.0));
        let above = sel_rect.min.y - gap - size.y;
        let below = sel_rect.max.y + gap;
        let y = if above >= rect.min.y + 4.0 {
            above
        } else if below + size.y <= rect.max.y - 4.0 {
            below
        } else if sel_rect.max.x + gap + size.x <= rect.max.x {
            x = sel_rect.max.x + gap;
            (sel_rect.center().y - size.y / 2.0).clamp(rect.min.y + 4.0, rect.max.y - size.y - 4.0)
        } else {
            return;
        };
        let strip = ERect::from_min_size(pos2(x, y), size);
        if strip.intersects(sel_rect) {
            return;
        }
        let locale = state.locale;
        let mut chosen: Option<QuickAction> = None;
        let area = egui::Area::new(ui.id().with(("quick_actions", self.kind_id())))
            .fixed_pos(pos2(x, y))
            .order(egui::Order::Foreground)
            .constrain(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(4, 3)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for a in &actions {
                            let label = t(locale, a.key());
                            let b = ui.add(egui::Button::new(label.as_ref()).small().frame(true));
                            let b = b.on_hover_text(t(locale, &a.tip_key()).as_ref());
                            if b.clicked() {
                                chosen = Some(*a);
                            }
                        }
                    });
                });
            });
        self.strip_size = area.response.rect.size();
        if let Some(a) = chosen {
            let cmds = actions::commands(&a, state.doc(), &state.selection, Some(bounds));
            self.run_action(state, cmds, matches!(a, QuickAction::Delete));
        }
    }

    fn run_action(&mut self, state: &mut AppState, cmds: Vec<AnyCommand>, deleting: bool) {
        if cmds.is_empty() {
            return;
        }
        let creates = cmds.iter().any(|c| {
            matches!(c, AnyCommand::TransformElements(t) if t.copy)
                || matches!(c, AnyCommand::CopyToStorey(_))
        });
        if let Ok(out) = state.run_all(cmds, String::new()) {
            if creates && !out.created.is_empty() {
                // Copies on this storey become the selection; copies on another storey leave it alone.
                let here: BTreeSet<Id> = out
                    .created
                    .iter()
                    .copied()
                    .filter(|id| {
                        state.doc().element(*id).is_some_and(|e| e.storey == Some(state.active_storey))
                    })
                    .collect();
                if !here.is_empty() {
                    state.selection = here;
                }
            }
            if deleting {
                state.selection.clear();
            }
        }
        self.clicked_at = None;
    }

    fn kind_id(&self) -> u64 {
        match self.kind {
            CanvasKind::Plan { view } | CanvasKind::Section { view } => view.0,
            CanvasKind::Sheet { sheet } => sheet.0 | (1 << 63),
        }
    }

    /// The right-click menu: delete, copy to storey, zoom to selection, properties.
    fn context_menu(&mut self, response: &egui::Response, state: &mut AppState, prep: &Prepared) {
        if response.secondary_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let at = self.view.drawing_at(p);
            let doc = state.doc();
            let hit = prep
                .hit(at, PICK_RADIUS_PT * self.view.unit_per_point(), |s| doc.element(Id(s)).is_some())
                .map(Id);
            if let Some(id) = hit
                && !state.selection.contains(&id)
            {
                state.selection = [id].into();
            }
            self.clicked_at = Some(Point2::new(at.x, at.y));
        }
        let locale = state.locale;
        let mut cmds: Vec<AnyCommand> = Vec::new();
        let mut deleting = false;
        let mut zoom_sel = false;
        let mut zoom_all = false;
        let mut props = false;
        response.context_menu(|ui| {
            let has_sel = !state.selection.is_empty();
            if has_sel {
                if ui.button(t(locale, "ui.canvas.action.delete").as_ref()).clicked() {
                    cmds.push(DeleteElements { ids: state.selection.iter().copied().collect() }.into());
                    deleting = true;
                    ui.close();
                }
                let current =
                    state.selection.iter().find_map(|id| state.doc().element(*id).and_then(|e| e.storey));
                let storeys: Vec<(Id, String)> = state
                    .doc()
                    .storeys()
                    .iter()
                    .filter(|s| Some(s.id) != current)
                    .map(|s| (s.id, s.name.clone()))
                    .collect();
                if !storeys.is_empty() {
                    ui.menu_button(t(locale, "ui.canvas.menu.copy_to_storey").as_ref(), |ui| {
                        for (id, name) in &storeys {
                            if ui.button(name).clicked() {
                                cmds.extend(actions::copy_to_storey(state.doc(), &state.selection, *id));
                                ui.close();
                            }
                        }
                    });
                }
                if ui.button(t(locale, "ui.canvas.menu.zoom_selection").as_ref()).clicked() {
                    zoom_sel = true;
                    ui.close();
                }
            }
            if ui.button(t(locale, "ui.canvas.menu.zoom_extents").as_ref()).clicked() {
                zoom_all = true;
                ui.close();
            }
            if has_sel && ui.button(t(locale, "ui.canvas.menu.properties").as_ref()).clicked() {
                props = true;
                ui.close();
            }
        });
        if zoom_sel {
            self.zoom_to_selection();
        }
        if zoom_all {
            self.zoom_extents();
        }
        if props {
            self.properties_requested = true;
        }
        if !cmds.is_empty() {
            self.run_action(state, cmds, deleting);
        }
    }
}

/// Size in screen points of one paper millimetre (for tests of the scale logic).
pub fn points_per_paper_mm(zoom: f64, units: DrawingUnits, scale: f64) -> f64 {
    match units {
        DrawingUnits::Metres => zoom * scale / 1000.0,
        DrawingUnits::Millimetres => zoom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_millimetres_follow_the_scale() {
        // 1:50 at 80 points per metre: 4 points per paper millimetre.
        assert!((points_per_paper_mm(80.0, DrawingUnits::Metres, 50.0) - 4.0).abs() < 1e-12);
        assert!((points_per_paper_mm(3.0, DrawingUnits::Millimetres, 1.0) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn canvas_starts_with_the_select_tool() {
        let c = Canvas::new(CanvasKind::Plan { view: Id(1) });
        assert!(!c.is_typing());
        assert!(c.prompt().is_empty());
        assert!(c.cursor_model().is_none());
    }
}
