//! Tessellation of a prepared drawing into egui meshes for one zoom level, with level of detail.
//!
//! Meshes are built in *mesh space*: drawing units relative to an anchor point, multiplied by the zoom the mesh
//! was built for, Y down. Painting a mesh at a slightly different zoom or after panning is then a per-vertex
//! translate-and-scale ([`place`]), so pan and zoom never re-tessellate until the zoom drifts too far or the view
//! leaves the tessellated window (see `MeshCache` in the canvas).
//!
//! Line weights are true to print (`weight_mm` × points per paper millimetre) with clamps that keep the pen
//! hierarchy readable when zoomed far out or in ([`stroke_width`]).

use egui::epaint::{Mesh, PathShape, PathStroke, TessellationOptions, Tessellator, Vertex, WHITE_UV};
use egui::{Color32, Pos2, pos2};
use kanavos_drawing::{Pt, Rect, Rgba};

use kanavos_drawing::dash::dash_polyline;

use super::prepare::{PItem, PPath, PStroke, Prepared, color32};
use super::view::intersects;

/// Hatches whose lines would be closer than this on screen (points) are drawn as a flat tone instead.
pub const HATCH_MIN_SPACING_PT: f64 = 3.0;
/// Dash patterns whose period would be shorter than this on screen (points) are stretched to it, so that dashed
/// lines stay recognisably dashed when zoomed out.
pub const DASH_MIN_PERIOD_PT: f64 = 6.0;
/// Items smaller than this on screen (points) are skipped.
pub const MIN_ITEM_PT: f64 = 0.35;

/// Screen width in points of a stroke of `weight_mm` paper millimetres at `pt_per_mm` points per paper
/// millimetre: the printed width, clamped to `[0.5 + 1.5·w, 2 + 8·w]` points so thin pens stay visible and thick
/// pens stay usable.
pub fn stroke_width(weight_mm: f64, pt_per_mm: f64) -> f32 {
    let w = weight_mm.max(0.0);
    let min = 0.5 + 1.5 * w;
    let max = 2.0 + 8.0 * w;
    let true_w = w * pt_per_mm;
    let v = if true_w.is_finite() { true_w.clamp(min, max) } else { min };
    v as f32
}

/// How a set of items is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Style {
    /// Colours as in the drawing.
    Normal,
    /// Every stroke in one colour, `extra` points wider; fills tinted with `tint`; hatches omitted.
    Highlight {
        /// Stroke colour.
        color: Color32,
        /// Extra stroke width, points.
        extra: f32,
        /// Fill tint (usually translucent).
        tint: Color32,
    },
}

/// Parameters of one tessellation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TessParams {
    /// Screen points per drawing unit the mesh is built for.
    pub zoom: f64,
    /// Drawing point at the mesh origin.
    pub anchor: Pt,
    /// Only items touching this drawing rectangle are tessellated (`None`: all).
    pub window: Option<Rect>,
    /// Physical pixels per point (for anti-aliasing).
    pub pixels_per_point: f32,
}

impl TessParams {
    /// Mesh-space position of a drawing point.
    pub fn to_mesh(self, p: Pt) -> Pos2 {
        pos2(((p.x - self.anchor.x) * self.zoom) as f32, (-(p.y - self.anchor.y) * self.zoom) as f32)
    }
}

/// Builds meshes from prepared drawings.
pub struct Builder {
    tess: Tessellator,
    params: TessParams,
    pt_per_mm: f64,
    paper_unit: f64,
    /// Width of the anti-aliasing fringe, points (one physical pixel).
    feather: f32,
    /// Number of items tessellated (statistics).
    pub items: usize,
}

impl Builder {
    /// A builder for a drawing whose paper millimetre is `paper_unit` drawing units.
    pub fn new(params: TessParams, paper_unit: f64) -> Builder {
        let options = TessellationOptions {
            feathering: true,
            feathering_size_in_pixels: 1.0,
            coarse_tessellation_culling: false,
            prerasterized_discs: false,
            round_text_to_pixels: false,
            round_line_segments_to_pixels: false,
            round_rects_to_pixels: false,
            ..TessellationOptions::default()
        };
        let ppp = if params.pixels_per_point > 0.0 { params.pixels_per_point } else { 1.0 };
        Builder {
            tess: Tessellator::new(ppp, options, [1, 1], Vec::new()),
            params,
            pt_per_mm: params.zoom * paper_unit,
            paper_unit,
            feather: 1.0 / ppp,
            items: 0,
        }
    }

    /// Appends the items of `prep` selected by `indices` (all when `None`) in painting order.
    pub fn add(&mut self, prep: &Prepared, indices: Option<&[usize]>, style: Style, out: &mut Mesh) {
        match indices {
            Some(ix) => {
                for &i in ix {
                    if let Some(it) = prep.items.get(i) {
                        self.item(it, style, out);
                    }
                }
            }
            None => {
                for it in &prep.items {
                    self.item(it, style, out);
                }
            }
        }
    }

    /// Whether an item is worth drawing in the window at this zoom.
    fn visible(&self, it: &PItem) -> bool {
        if let Some(w) = &self.params.window
            && !intersects(w, &it.bounds)
        {
            return false;
        }
        let size = it.bounds.width().max(it.bounds.height()) * self.params.zoom;
        size >= MIN_ITEM_PT || !it.strokes.is_empty()
    }

    /// Appends one item.
    pub fn item(&mut self, it: &PItem, style: Style, out: &mut Mesh) {
        if !self.visible(it) {
            return;
        }
        self.items += 1;
        if let Some(f) = &it.fill {
            let color = match style {
                Style::Normal => color32(f.color),
                Style::Highlight { tint, .. } => tint,
            };
            if color.a() > 0 {
                self.triangles(&f.verts, &f.tris, color, out);
            }
        }
        if let (Some(h), Style::Normal) = (&it.hatch, style) {
            let spacing_pt = h.spacing * self.params.zoom;
            if spacing_pt < HATCH_MIN_SPACING_PT || h.segments.is_empty() {
                // Flat tone: ink spread over the area.
                if let Some(f) = &it.fill {
                    let tone = tone(h.color, h.background, h.coverage);
                    if tone.a() > 0 {
                        self.triangles(&f.verts, &f.tris, tone, out);
                    }
                }
            } else {
                let width = stroke_width(h.weight_mm, self.pt_per_mm);
                let color = color32(h.color);
                for [a, b] in &h.segments {
                    self.segment(*a, *b, width, color, out);
                }
            }
        }
        for s in &it.strokes {
            let (color, extra) = match style {
                Style::Normal => (color32(s.color), 0.0),
                Style::Highlight { color, extra, .. } => (color, extra),
            };
            self.stroke(s, color, extra, out);
        }
    }

    fn triangles(&self, verts: &[Pt], tris: &[[u32; 3]], color: Color32, out: &mut Mesh) {
        let base = out.vertices.len() as u32;
        let n = verts.len() as u32;
        out.vertices.extend(verts.iter().map(|p| Vertex {
            pos: self.params.to_mesh(*p),
            uv: WHITE_UV,
            color,
        }));
        for t in tris {
            if t.iter().all(|&i| i < n) {
                out.add_triangle(base + t[0], base + t[1], base + t[2]);
            }
        }
    }

    fn segment(&mut self, a: Pt, b: Pt, width: f32, color: Color32, out: &mut Mesh) {
        let (pa, pb) = (self.params.to_mesh(a), self.params.to_mesh(b));
        if (pb - pa).length_sq() < 1e-6 {
            // A dot.
            let r = width.max(0.6) * 0.6;
            self.tess.tessellate_circle(
                egui::epaint::CircleShape { center: pa, radius: r, fill: color, stroke: egui::Stroke::NONE },
                out,
            );
            return;
        }
        line_quad(out, pa, pb, width, color, self.feather);
    }

    fn stroke(&mut self, s: &PStroke, color: Color32, extra: f32, out: &mut Mesh) {
        if color.a() == 0 {
            return;
        }
        let width = stroke_width(s.weight_mm, self.pt_per_mm) + extra;
        let (points, closed) = match &s.path {
            PPath::Poly { points, closed } => (points.clone(), *closed),
            PPath::Arc { centre, radius, start, sweep } => {
                let r_pt = radius * self.params.zoom;
                if r_pt < 0.3 {
                    return;
                }
                let full = sweep.abs() >= std::f64::consts::TAU - 1e-9;
                let n = arc_segments(r_pt, sweep.abs());
                let count = if full { n } else { n + 1 };
                let pts = (0..count)
                    .map(|i| {
                        let a = start + sweep * i as f64 / n as f64;
                        kanavos_drawing::pt(centre.x + radius * a.cos(), centre.y + radius * a.sin())
                    })
                    .collect();
                (pts, full)
            }
        };
        if points.len() < 2 {
            return;
        }
        let pattern: Vec<f64> = s.dashes_mm.iter().map(|v| v * self.paper_unit).collect();
        let period: f64 = pattern.iter().map(|v| v.abs()).sum();
        if pattern.len() >= 2 && period > 0.0 {
            let period_pt = period * self.params.zoom;
            let k = if period_pt < DASH_MIN_PERIOD_PT { DASH_MIN_PERIOD_PT / period_pt } else { 1.0 };
            let pattern: Vec<f64> = pattern.iter().map(|v| v * k).collect();
            for piece in dash_polyline(&points, closed, &pattern, 0.0) {
                let pts: Vec<Pos2> = piece.iter().map(|p| self.params.to_mesh(*p)).collect();
                if let (Some(a), Some(b)) = (pts.first(), pts.last())
                    && pts.len() == 2
                    && (*b - *a).length_sq() < 1e-6
                {
                    self.tess.tessellate_circle(
                        egui::epaint::CircleShape {
                            center: *a,
                            radius: width * 0.5,
                            fill: color,
                            stroke: egui::Stroke::NONE,
                        },
                        out,
                    );
                    continue;
                }
                self.path(pts, false, width, color, out);
            }
        } else {
            let pts: Vec<Pos2> = points.iter().map(|p| self.params.to_mesh(*p)).collect();
            self.path(pts, closed, width, color, out);
        }
    }

    fn path(&mut self, mut pts: Vec<Pos2>, closed: bool, width: f32, color: Color32, out: &mut Mesh) {
        pts.dedup_by(|a, b| (*a - *b).length_sq() < 1e-8);
        if pts.len() < 2 {
            return;
        }
        let shape = PathShape {
            points: pts,
            closed,
            fill: Color32::TRANSPARENT,
            stroke: PathStroke::new(width, color),
        };
        self.tess.tessellate_path(&shape, out);
    }
}

/// A straight line as a quad with anti-aliased sides and butt ends (cheaper than a general path for the many
/// short hatch segments). Lines thinner than the fringe are drawn one fringe wide with proportionally less
/// opacity, as egui does.
pub fn line_quad(out: &mut Mesh, a: Pos2, b: Pos2, width: f32, color: Color32, feather: f32) {
    let d = b - a;
    let len = d.length();
    if !(len > 1e-6 && len.is_finite()) {
        return;
    }
    let n = egui::vec2(-d.y, d.x) / len;
    let clear = Color32::TRANSPARENT;
    let base = out.vertices.len() as u32;
    let mut v = |p: Pos2, c: Color32| out.vertices.push(Vertex { pos: p, uv: WHITE_UV, color: c });
    if width <= feather {
        let c = color.gamma_multiply(width / feather.max(1e-6));
        for p in [a, b] {
            v(p - n * feather, clear);
            v(p, c);
            v(p + n * feather, clear);
        }
        for k in 0..2 {
            out.add_triangle(base + k, base + k + 1, base + 3 + k);
            out.add_triangle(base + k + 1, base + 4 + k, base + 3 + k);
        }
    } else {
        let core = (width - feather) * 0.5;
        let outer = core + feather;
        for p in [a, b] {
            v(p - n * outer, clear);
            v(p - n * core, color);
            v(p + n * core, color);
            v(p + n * outer, clear);
        }
        for k in 0..3 {
            out.add_triangle(base + k, base + k + 1, base + 4 + k);
            out.add_triangle(base + k + 1, base + 5 + k, base + 4 + k);
        }
    }
}

/// Segments for an arc of radius `r_pt` points sweeping `sweep` radians, keeping the chord within a fifth of a
/// point of the true arc.
pub fn arc_segments(r_pt: f64, sweep: f64) -> usize {
    let tol = 0.2_f64;
    let step = if r_pt > tol { 2.0 * (1.0 - tol / r_pt).acos() } else { std::f64::consts::FRAC_PI_2 };
    let n = (sweep / step.max(1e-3)).ceil();
    if n.is_finite() { (n as usize).clamp(2, 720) } else { 2 }
}

/// The flat tone shown for a hatch at small zoom: the hatch colour spread over the background with the given
/// ink coverage; translucent when there is no background.
pub fn tone(line: Rgba, background: Option<Rgba>, coverage: f64) -> Color32 {
    let c = coverage.clamp(0.0, 1.0);
    match background {
        Some(bg) => {
            let mix = |a: u8, b: u8| (f64::from(a) * (1.0 - c) + f64::from(b) * c).round() as u8;
            Color32::from_rgba_unmultiplied(mix(bg.r, line.r), mix(bg.g, line.g), mix(bg.b, line.b), bg.a)
        }
        None => {
            Color32::from_rgba_unmultiplied(line.r, line.g, line.b, (c * f64::from(line.a)).round() as u8)
        }
    }
}

/// A copy of a mesh-space mesh placed on screen: `screen = origin + v · k`.
pub fn place(mesh: &Mesh, origin: Pos2, k: f32) -> Mesh {
    let mut m = Mesh {
        indices: mesh.indices.clone(),
        vertices: Vec::with_capacity(mesh.vertices.len()),
        texture_id: mesh.texture_id,
    };
    m.vertices.extend(
        mesh.vertices
            .iter()
            .map(|v| Vertex { pos: pos2(origin.x + v.pos.x * k, origin.y + v.pos.y * k), ..*v }),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::{Drawing, Fill, HatchPattern, Item, LinePattern, Stroke, pt, weights};

    fn params(zoom: f64) -> TessParams {
        TessParams { zoom, anchor: pt(0.0, 0.0), window: None, pixels_per_point: 1.0 }
    }

    #[test]
    fn widths_are_true_to_print_within_clamps() {
        // 1:50 at 400 points per metre: 8 points per paper mm; a 0.25 mm pen is 2 points.
        assert!((stroke_width(0.25, 8.0) - 2.0).abs() < 1e-6);
        // Far out, thin pens keep a visible minimum and the hierarchy holds.
        let thin = stroke_width(weights::FINE, 0.2);
        let thick = stroke_width(weights::EXTRA_THICK, 0.2);
        assert!(thin >= 0.5 && thick > thin);
        // Far in, widths are capped.
        assert!(stroke_width(weights::EXTRA_THICK, 1000.0) <= 2.0 + 8.0 * 0.7 + 1e-6);
    }

    #[test]
    fn mesh_space_flips_y_around_the_anchor() {
        let p = TessParams { zoom: 10.0, anchor: pt(1.0, 1.0), window: None, pixels_per_point: 1.0 };
        let m = p.to_mesh(pt(2.0, 3.0));
        assert!((m.x - 10.0).abs() < 1e-6 && (m.y + 20.0).abs() < 1e-6);
    }

    fn hatched_wall() -> Prepared {
        let mut d = Drawing::model(50.0);
        d.push(Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 0.25), pt(0.0, 0.25)]],
            Some(Fill::hatch(HatchPattern::Masonry, None)),
            Some(Stroke::solid(weights::THICK)),
            "wall.cut",
        ));
        Prepared::new(&d)
    }

    #[test]
    fn hatches_become_a_tone_when_zoomed_out() {
        let prep = hatched_wall();
        let mut far = Mesh::default();
        Builder::new(params(20.0), prep.paper_unit()).add(&prep, None, Style::Normal, &mut far);
        let mut near = Mesh::default();
        Builder::new(params(400.0), prep.paper_unit()).add(&prep, None, Style::Normal, &mut near);
        // Zoomed in, every hatch line is tessellated: far more vertices.
        assert!(
            near.vertices.len() > far.vertices.len() * 5,
            "{} vs {}",
            near.vertices.len(),
            far.vertices.len()
        );
        assert!(far.is_valid() && near.is_valid());
    }

    #[test]
    fn window_culls_items() {
        let prep = hatched_wall();
        let mut p = params(400.0);
        p.window = Some(Rect { min: pt(100.0, 100.0), max: pt(101.0, 101.0) });
        let mut m = Mesh::default();
        let mut b = Builder::new(p, prep.paper_unit());
        b.add(&prep, None, Style::Normal, &mut m);
        assert!(m.is_empty());
        assert_eq!(b.items, 0);
    }

    #[test]
    fn dashed_lines_produce_several_pieces() {
        let mut d = Drawing::model(50.0);
        d.push(Item::line(
            pt(0.0, 0.0),
            pt(10.0, 0.0),
            Stroke::patterned(weights::FINE, LinePattern::Dashed),
            "x",
        ));
        d.push(Item::line(pt(0.0, 1.0), pt(10.0, 1.0), Stroke::solid(weights::FINE), "x"));
        let prep = Prepared::new(&d);
        let mut dashed = Mesh::default();
        let mut solid_mesh = Mesh::default();
        let mut b = Builder::new(params(100.0), prep.paper_unit());
        b.add(&prep, Some(&[0]), Style::Normal, &mut dashed);
        b.add(&prep, Some(&[1]), Style::Normal, &mut solid_mesh);
        assert!(dashed.vertices.len() > solid_mesh.vertices.len() * 10);
    }

    #[test]
    fn highlight_uses_one_colour() {
        let prep = hatched_wall();
        let blue = Color32::from_rgb(30, 110, 230);
        let mut m = Mesh::default();
        Builder::new(params(400.0), prep.paper_unit()).add(
            &prep,
            None,
            Style::Highlight {
                color: blue,
                extra: 1.0,
                tint: Color32::from_rgba_unmultiplied(30, 110, 230, 60),
            },
            &mut m,
        );
        assert!(!m.is_empty());
        assert!(m.vertices.iter().all(|v| v.color.r() <= 30 || v.color.a() < 255));
    }

    #[test]
    fn arcs_get_more_segments_when_larger() {
        assert!(
            arc_segments(1000.0, std::f64::consts::FRAC_PI_2)
                > arc_segments(10.0, std::f64::consts::FRAC_PI_2)
        );
        assert!(arc_segments(0.01, 1.0) >= 2);
    }

    #[test]
    fn tone_mixes_with_background() {
        let t = tone(Rgba::BLACK, Some(Rgba::WHITE), 0.25);
        assert_eq!(t, Color32::from_rgb(191, 191, 191));
        let t = tone(Rgba::BLACK, None, 0.5);
        assert!(t.a() > 100 && t.a() < 140);
    }

    #[test]
    fn placing_translates_and_scales() {
        let mut m = Mesh::default();
        m.colored_vertex(pos2(10.0, -5.0), Color32::BLACK);
        let p = place(&m, pos2(100.0, 100.0), 2.0);
        let v = p.vertices.first().unwrap();
        assert!((v.pos.x - 120.0).abs() < 1e-6 && (v.pos.y - 90.0).abs() < 1e-6);
    }
}
