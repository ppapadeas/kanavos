//! Things drawn over the drawing while a tool works: snap glyphs, inference guides, the snap label and the
//! typed-input box next to the cursor. Colours carry meaning only (ux.md §6): blue for selection, green for
//! inference.

use egui::{Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use kanavos_tools::{CursorHint, HandleRole, InputField, SnapHint, TextEntry};

/// Selection colour.
pub const SELECTION: Color32 = Color32::from_rgb(24, 104, 222);
/// Selection fill tint.
pub const SELECTION_TINT: Color32 = Color32::from_rgba_premultiplied(9, 40, 86, 40);
/// Hover highlight.
pub const HOVER: Color32 = Color32::from_rgba_premultiplied(13, 57, 122, 140);
/// Inference colour (snap glyphs, guides, labels).
pub const INFERENCE: Color32 = Color32::from_rgb(0, 140, 105);
/// Warning colour (an operation that cannot be done as previewed).
pub const WARNING: Color32 = Color32::from_rgb(200, 48, 32);

/// The egui cursor for a tool's cursor hint.
pub fn cursor_icon(hint: CursorHint) -> CursorIcon {
    match hint {
        CursorHint::Arrow => CursorIcon::Default,
        CursorHint::Crosshair => CursorIcon::Crosshair,
        CursorHint::Move => CursorIcon::Move,
        CursorHint::Grab => CursorIcon::Grab,
    }
}

/// Draws the glyph of a snap kind centred on `p`. Each kind has its own shape so the reason for a snap can be read
/// without the label: square (end), triangle (middle), corner bracket (corner), circle (centre), cross
/// (intersection), diamond (on edge), right-angle mark (perpendicular), plus (extension), wedge (angle lock), small
/// grid cross (grid), two slanted strokes (parallel), circle on a line (alignment), bracketed line (wall face),
/// dash-dot line (centreline), three lines (layer boundary), crossed square (insertion point), padlock (locked
/// direction).
pub fn snap_glyph(painter: &egui::Painter, p: Pos2, hint: SnapHint, color: Color32) {
    let s = 5.5;
    let st = Stroke::new(1.6, color);
    let halo = Stroke::new(3.6, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
    let mut lines: Vec<[Pos2; 2]> = Vec::new();
    let mut poly: Option<Vec<Pos2>> = None;
    let mut circle: Option<f32> = None;
    match hint {
        SnapHint::Free => return,
        SnapHint::Endpoint => {
            poly = Some(vec![p + vec2(-s, -s), p + vec2(s, -s), p + vec2(s, s), p + vec2(-s, s)])
        }
        SnapHint::Midpoint => {
            poly =
                Some(vec![p + vec2(0.0, -s * 1.2), p + vec2(s * 1.1, s * 0.8), p + vec2(-s * 1.1, s * 0.8)])
        }
        SnapHint::Corner => {
            lines.push([p + vec2(-s, -s), p + vec2(-s, s)]);
            lines.push([p + vec2(-s, s), p + vec2(s, s)]);
            lines.push([p + vec2(-s * 0.2, -s * 0.2), p + vec2(-s * 0.2, s * 0.2)]);
        }
        SnapHint::Centre => circle = Some(s),
        SnapHint::Intersection => {
            lines.push([p + vec2(-s, -s), p + vec2(s, s)]);
            lines.push([p + vec2(-s, s), p + vec2(s, -s)]);
        }
        SnapHint::OnEdge => {
            poly = Some(vec![p + vec2(0.0, -s), p + vec2(s, 0.0), p + vec2(0.0, s), p + vec2(-s, 0.0)])
        }
        SnapHint::Perpendicular => {
            lines.push([p + vec2(-s, s), p + vec2(s, s)]);
            lines.push([p + vec2(-s, s), p + vec2(-s, -s)]);
            lines.push([p + vec2(-s, 0.0), p + vec2(0.0, 0.0)]);
            lines.push([p + vec2(0.0, 0.0), p + vec2(0.0, s)]);
        }
        SnapHint::Extension => {
            lines.push([p + vec2(-s, 0.0), p + vec2(s, 0.0)]);
            lines.push([p + vec2(0.0, -s), p + vec2(0.0, s)]);
        }
        SnapHint::Angle => {
            lines.push([p + vec2(-s, s), p + vec2(s, s)]);
            lines.push([p + vec2(-s, s), p + vec2(s * 0.6, -s)]);
        }
        SnapHint::Grid => {
            lines.push([p + vec2(-s * 0.6, 0.0), p + vec2(s * 0.6, 0.0)]);
            lines.push([p + vec2(0.0, -s * 0.6), p + vec2(0.0, s * 0.6)]);
        }
        SnapHint::Parallel => {
            lines.push([p + vec2(-s, s * 0.6), p + vec2(-s * 0.1, -s)]);
            lines.push([p + vec2(s * 0.1, s), p + vec2(s, -s * 0.6)]);
        }
        SnapHint::Alignment => {
            circle = Some(s * 0.7);
            lines.push([p + vec2(-s * 1.4, 0.0), p + vec2(s * 1.4, 0.0)]);
        }
        SnapHint::Face => {
            lines.push([p + vec2(-s, 0.0), p + vec2(s, 0.0)]);
            lines.push([p + vec2(-s, -s * 0.6), p + vec2(-s, s * 0.6)]);
            lines.push([p + vec2(s, -s * 0.6), p + vec2(s, s * 0.6)]);
        }
        SnapHint::Centreline => {
            lines.push([p + vec2(-s * 1.3, 0.0), p + vec2(-s * 0.45, 0.0)]);
            lines.push([p + vec2(-s * 0.1, 0.0), p + vec2(s * 0.1, 0.0)]);
            lines.push([p + vec2(s * 0.45, 0.0), p + vec2(s * 1.3, 0.0)]);
        }
        SnapHint::LayerBoundary => {
            for dy in [-s * 0.6, 0.0, s * 0.6] {
                lines.push([p + vec2(-s, dy), p + vec2(s, dy)]);
            }
        }
        SnapHint::Insertion => {
            poly = Some(vec![p + vec2(-s, -s), p + vec2(s, -s), p + vec2(s, s), p + vec2(-s, s)]);
            lines.push([p + vec2(-s, -s), p + vec2(s, s)]);
            lines.push([p + vec2(-s, s), p + vec2(s, -s)]);
        }
        SnapHint::Locked => {
            poly = Some(vec![
                p + vec2(-s * 0.8, -s * 0.1),
                p + vec2(s * 0.8, -s * 0.1),
                p + vec2(s * 0.8, s),
                p + vec2(-s * 0.8, s),
            ]);
            lines.push([p + vec2(-s * 0.45, -s * 0.1), p + vec2(-s * 0.45, -s * 0.7)]);
            lines.push([p + vec2(-s * 0.45, -s * 0.7), p + vec2(s * 0.45, -s * 0.7)]);
            lines.push([p + vec2(s * 0.45, -s * 0.7), p + vec2(s * 0.45, -s * 0.1)]);
        }
    }
    for l in &lines {
        painter.line_segment(*l, halo);
    }
    for l in &lines {
        painter.line_segment(*l, st);
    }
    if let Some(pts) = poly {
        painter.add(egui::Shape::closed_line(pts.clone(), halo));
        painter.add(egui::Shape::closed_line(pts, st));
    }
    if let Some(r) = circle {
        painter.circle_stroke(p, r, halo);
        painter.circle_stroke(p, r, st);
        painter.circle_filled(p, 1.5, color);
    }
}

/// Draws a dashed screen line (inference guide).
pub fn dashed(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
    let len = (b - a).length();
    if len < 1.0 {
        return;
    }
    let dir = (b - a) / len;
    let (on, off) = (6.0, 4.0);
    let mut t = 0.0;
    while t < len {
        let e = (t + on).min(len);
        painter.line_segment([a + dir * t, a + dir * e], Stroke::new(1.0, color));
        t += on + off;
    }
}

/// Draws a label in a light pill with its top-left corner at `at`. Returns its rectangle.
pub fn label(painter: &egui::Painter, at: Pos2, text: &str, color: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(12.0), color);
    let rect = Rect::from_min_size(at, galley.size() + vec2(10.0, 4.0));
    painter.rect(
        rect,
        CornerRadius::same(4),
        Color32::from_rgba_unmultiplied(255, 255, 255, 230),
        Stroke::new(1.0, color.gamma_multiply(0.6)),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(5.0, 2.0), galley, color);
    rect
}

/// Draws the typed-input box: one row per field ("label  value"), the active field outlined in the selection
/// colour, typed (locked) values in black and live values in grey. `at` is the preferred top-left corner; the box
/// is kept inside `bounds`.
pub fn fields_box(painter: &egui::Painter, at: Pos2, fields: &[InputField], bounds: Rect) -> Option<Rect> {
    if fields.is_empty() {
        return None;
    }
    let font = FontId::proportional(13.0);
    let rows: Vec<_> = fields
        .iter()
        .map(|f| {
            let l = painter.layout_no_wrap(f.label.clone(), font.clone(), Color32::from_gray(90));
            let v_color = if f.locked { Color32::BLACK } else { Color32::from_gray(110) };
            let v = painter.layout_no_wrap(f.value.clone(), font.clone(), v_color);
            (f, l, v)
        })
        .collect();
    let label_w = rows.iter().map(|(_, l, _)| l.size().x).fold(0.0, f32::max);
    let value_w = rows.iter().map(|(_, _, v)| v.size().x).fold(40.0, f32::max);
    let row_h = 20.0;
    let size = vec2(label_w + value_w + 30.0, row_h * rows.len() as f32 + 6.0);
    let mut min = at;
    if min.x + size.x > bounds.max.x {
        min.x = (bounds.max.x - size.x).max(bounds.min.x);
    }
    if min.y + size.y > bounds.max.y {
        min.y = (bounds.max.y - size.y).max(bounds.min.y);
    }
    let rect = Rect::from_min_size(min, size);
    painter.rect(
        rect,
        CornerRadius::same(4),
        Color32::from_rgba_unmultiplied(252, 252, 252, 240),
        Stroke::new(1.0, Color32::from_gray(170)),
        StrokeKind::Inside,
    );
    for (i, (f, l, v)) in rows.into_iter().enumerate() {
        let y = rect.min.y + 3.0 + row_h * i as f32;
        let value_rect = Rect::from_min_size(
            pos2(rect.min.x + label_w + 14.0, y + 1.0),
            vec2(value_w + 10.0, row_h - 2.0),
        );
        if f.active {
            painter.rect(
                value_rect,
                CornerRadius::same(3),
                Color32::from_rgba_unmultiplied(225, 236, 252, 255),
                Stroke::new(1.2, SELECTION),
                StrokeKind::Inside,
            );
        }
        painter.galley(pos2(rect.min.x + 6.0, y + (row_h - l.size().y) / 2.0), l, Color32::from_gray(90));
        let vy = y + (row_h - v.size().y) / 2.0;
        let caret_x = value_rect.min.x + 6.0 + v.size().x;
        painter.galley(pos2(value_rect.min.x + 5.0, vy), v, Color32::BLACK);
        if f.active && f.locked {
            // Caret after the typed value.
            painter.line_segment(
                [pos2(caret_x, value_rect.min.y + 3.0), pos2(caret_x, value_rect.max.y - 3.0)],
                Stroke::new(1.0, Color32::BLACK),
            );
        }
    }
    Some(rect)
}

/// Draws a centred hint in the canvas (empty states).
pub fn hint(painter: &egui::Painter, rect: Rect, text: &str) {
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(15.0),
        Color32::from_gray(120),
    );
}

/// A small cross marking a point the pointer is aligned with (alignment tracking).
pub fn tracking_cross(painter: &egui::Painter, p: Pos2, color: Color32) {
    let s = 4.0;
    let st = Stroke::new(1.2, color);
    painter.line_segment([p + vec2(-s, -s), p + vec2(s, s)], st);
    painter.line_segment([p + vec2(-s, s), p + vec2(s, -s)], st);
}

/// An editing handle: squares for ends and vertices, a small circle with a plus for inserting a vertex, a
/// circle for moving, a bar for an opening's jamb. Hot handles (under the pointer) are filled.
pub fn handle(painter: &egui::Painter, p: Pos2, role: HandleRole, hot: bool) {
    let fill = if hot { SELECTION } else { Color32::WHITE };
    let ink = if hot { Color32::WHITE } else { SELECTION };
    let st = Stroke::new(1.3, SELECTION);
    match role {
        HandleRole::End | HandleRole::Vertex => {
            let r = Rect::from_center_size(p, vec2(8.0, 8.0));
            painter.rect(r, CornerRadius::ZERO, fill, st, StrokeKind::Middle);
        }
        HandleRole::InsertVertex => {
            painter.circle(p, 4.0, fill, st);
            painter.line_segment([p + vec2(-2.2, 0.0), p + vec2(2.2, 0.0)], Stroke::new(1.0, ink));
            painter.line_segment([p + vec2(0.0, -2.2), p + vec2(0.0, 2.2)], Stroke::new(1.0, ink));
        }
        HandleRole::Move => {
            painter.circle(p, 5.5, fill, st);
            for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
                painter.line_segment([p + d * 1.5, p + d * 3.8], Stroke::new(1.0, ink));
            }
        }
        HandleRole::Width => {
            let r = Rect::from_center_size(p, vec2(5.0, 11.0));
            painter.rect(r, CornerRadius::same(1), fill, st, StrokeKind::Middle);
        }
    }
}

/// The text box of the text tool: the text so far at its printed size (rotated, one line per row), a caret
/// after the last character and a light frame. `at` is the anchor of the first line's baseline on screen.
pub fn text_entry(painter: &egui::Painter, at: Pos2, entry: &TextEntry, pt_per_mm: f64) {
    let cap = (entry.size_mm * pt_per_mm).clamp(6.0, 200.0);
    let size = (cap / 0.7) as f32;
    let font = FontId::proportional(size);
    let angle = if entry.rotation.is_finite() { -(entry.rotation as f32) } else { 0.0 };
    let rot = egui::emath::Rot2::from_angle(angle);
    let line_step = (cap * 1.6) as f32;
    let lines: Vec<&str> = entry.text.split('\n').collect();
    let mut widest = size * 0.6;
    let mut last_end = vec2(0.0, 0.0);
    for (i, line) in lines.iter().enumerate() {
        let galley = painter.layout_no_wrap((*line).to_owned(), font.clone(), Color32::BLACK);
        let baseline = galley
            .rows
            .first()
            .map(|r| r.pos.y + r.glyphs.first().map(|g| g.pos.y).unwrap_or(r.size.y * 0.8))
            .unwrap_or(size * 0.8);
        let local = vec2(0.0, line_step * i as f32);
        let origin = at + rot * (local - vec2(0.0, baseline));
        widest = widest.max(galley.size().x);
        last_end = local + vec2(galley.size().x, 0.0);
        painter.add(egui::epaint::TextShape::new(origin, galley, Color32::BLACK).with_angle(angle));
    }
    // Caret.
    let caret_top = at + rot * (last_end + vec2(1.5, -(cap as f32)));
    let caret_bottom = at + rot * (last_end + vec2(1.5, size * 0.15));
    painter.line_segment([caret_top, caret_bottom], Stroke::new(1.2, SELECTION));
    // Frame around the text.
    let h = line_step * (lines.len().max(1) as f32 - 1.0);
    let pad = 4.0;
    let corners = [
        vec2(-pad, -(cap as f32) - pad),
        vec2(widest + pad + 3.0, -(cap as f32) - pad),
        vec2(widest + pad + 3.0, h + size * 0.3 + pad),
        vec2(-pad, h + size * 0.3 + pad),
    ]
    .map(|c| at + rot * c);
    for k in 0..4 {
        if let (Some(a), Some(b)) = (corners.get(k), corners.get((k + 1) % 4)) {
            dashed(painter, *a, *b, SELECTION);
        }
    }
}
