//! Antialiased raster rendering of drawings to RGBA images and PNG.
//!
//! Layer L3. See `docs/architecture.md`.
//!
//! Used for view previews and thumbnails. The drawing is placed on a page in paper millimetres (see
//! [`kanavos_drawing::page`]) and rasterised with `tiny-skia`: strokes at their true width with dash arrays, fills
//! with the even-odd rule, hatches expanded by [`kanavos_drawing::hatch`], and text drawn as glyph outlines (or the
//! built-in stroke font) from [`kanavos_drawing::fonts`], so Greek always renders.

use std::fmt;

use kanavos_drawing::fonts::{Fonts, PathEl};
use kanavos_drawing::geometry::{FLATTEN_TOLERANCE_MM, fill_rings, stroke_paths};
use kanavos_drawing::hatch::{HatchOptions, expand_fill};
use kanavos_drawing::page::PageFrame;
use kanavos_drawing::text::layout_text;
use kanavos_drawing::{Drawing, Fill, Item, LinePattern, Pt, Rgba, Shape, Stroke as DStroke};
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, StrokeDash, Transform,
};

/// Largest image rendered, in pixels (width × height).
pub const MAX_PIXELS: u64 = 100_000_000;

/// How big the image is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RasterSize {
    /// A fixed pixel size; the page is scaled to fit and centred.
    Pixels {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
    },
    /// The page at a resolution in dots per inch; the image size follows the page size.
    Dpi(f64),
}

/// Options for [`render`].
#[derive(Debug, Clone, PartialEq)]
pub struct RasterOptions {
    /// Image size.
    pub size: RasterSize,
    /// Page and placement; `None` fits the page to the content plus [`margin_mm`](RasterOptions::margin_mm).
    pub frame: Option<PageFrame>,
    /// Margin around the content when fitting, paper millimetres.
    pub margin_mm: f64,
    /// Background colour; `None` leaves the image transparent.
    pub background: Option<Rgba>,
    /// Thinnest stroke drawn, in pixels, so fine lines stay visible in small previews.
    pub min_stroke_px: f64,
    /// Limits for hatch expansion. Areas whose hatch is too dense get their background only.
    pub hatch: HatchOptions,
}

impl Default for RasterOptions {
    fn default() -> Self {
        RasterOptions {
            size: RasterSize::Dpi(150.0),
            frame: None,
            margin_mm: 5.0,
            background: Some(Rgba::WHITE),
            min_stroke_px: 0.5,
            hatch: HatchOptions::default(),
        }
    }
}

/// An RGBA image, 8 bits per channel, straight (not premultiplied) alpha, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel data, `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

/// Error from rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    /// The requested size is zero, negative or not a number.
    InvalidSize,
    /// The image would exceed [`MAX_PIXELS`].
    TooLarge {
        /// Requested width.
        width: u64,
        /// Requested height.
        height: u64,
    },
    /// PNG encoding failed.
    Encode(String),
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RasterError::InvalidSize => write!(f, "invalid image size"),
            RasterError::TooLarge { width, height } => {
                write!(f, "image of {width} × {height} pixels is too large")
            }
            RasterError::Encode(e) => write!(f, "PNG encoding failed: {e}"),
        }
    }
}

impl std::error::Error for RasterError {}

/// Renders a drawing with the system fonts.
pub fn render(drawing: &Drawing, opts: &RasterOptions) -> Result<RasterImage, RasterError> {
    render_with_fonts(drawing, opts, Fonts::system())
}

/// Renders a drawing to PNG bytes with the system fonts.
pub fn render_png(drawing: &Drawing, opts: &RasterOptions) -> Result<Vec<u8>, RasterError> {
    render_png_with_fonts(drawing, opts, Fonts::system())
}

/// Renders a drawing to PNG bytes with explicit fonts.
pub fn render_png_with_fonts(
    drawing: &Drawing,
    opts: &RasterOptions,
    fonts: &Fonts,
) -> Result<Vec<u8>, RasterError> {
    let pixmap = rasterise(drawing, opts, fonts)?;
    pixmap.encode_png().map_err(|e| RasterError::Encode(e.to_string()))
}

/// Renders a drawing with explicit fonts. Pass [`Fonts::builtin`] for output identical on every machine.
pub fn render_with_fonts(
    drawing: &Drawing,
    opts: &RasterOptions,
    fonts: &Fonts,
) -> Result<RasterImage, RasterError> {
    let pixmap = rasterise(drawing, opts, fonts)?;
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(RasterImage { width: pixmap.width(), height: pixmap.height(), rgba })
}

/// Pixel size and paper → pixel map for a page.
fn layout(frame: &PageFrame, size: RasterSize) -> Result<(u32, u32, Transform, f64), RasterError> {
    let (w_mm, h_mm) = (frame.width_mm, frame.height_mm);
    let (width, height, k, ox, oy) = match size {
        RasterSize::Pixels { width, height } => {
            if width == 0 || height == 0 {
                return Err(RasterError::InvalidSize);
            }
            let k = (f64::from(width) / w_mm).min(f64::from(height) / h_mm);
            let ox = (f64::from(width) - w_mm * k) / 2.0;
            let oy = (f64::from(height) - h_mm * k) / 2.0;
            (u64::from(width), u64::from(height), k, ox, oy)
        }
        RasterSize::Dpi(dpi) => {
            if !(dpi.is_finite() && dpi > 0.0) {
                return Err(RasterError::InvalidSize);
            }
            let k = dpi / 25.4;
            let (w, h) = ((w_mm * k).ceil(), (h_mm * k).ceil());
            if !(w.is_finite() && h.is_finite()) || w < 1.0 || h < 1.0 {
                return Err(RasterError::InvalidSize);
            }
            if w * h > MAX_PIXELS as f64 {
                return Err(RasterError::TooLarge { width: w as u64, height: h as u64 });
            }
            (w as u64, h as u64, k, 0.0, 0.0)
        }
    };
    if !(k.is_finite() && k > 0.0) {
        return Err(RasterError::InvalidSize);
    }
    if width * height > MAX_PIXELS {
        return Err(RasterError::TooLarge { width, height });
    }
    let (w, h) = (u32::try_from(width), u32::try_from(height));
    let (Ok(w), Ok(h)) = (w, h) else { return Err(RasterError::TooLarge { width, height }) };
    // Paper millimetres (Y up) → pixels (Y down).
    let t = Transform::from_row(k as f32, 0.0, 0.0, -(k as f32), ox as f32, (oy + h_mm * k) as f32);
    Ok((w, h, t, k))
}

fn rasterise(drawing: &Drawing, opts: &RasterOptions, fonts: &Fonts) -> Result<Pixmap, RasterError> {
    let frame = match opts.frame {
        Some(f) if f.is_valid() => f,
        _ => PageFrame::fit(drawing, fonts, opts.margin_mm),
    };
    let (w, h, t, k) = layout(&frame, opts.size)?;
    let mut pixmap = Pixmap::new(w, h).ok_or(RasterError::InvalidSize)?;
    if let Some(bg) = opts.background {
        pixmap.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));
    }
    let paper = frame.to_paper_drawing(drawing);
    let min_width_mm = if opts.min_stroke_px.is_finite() { opts.min_stroke_px.max(0.0) / k } else { 0.0 };
    let mut r = Renderer { pixmap: &mut pixmap, t, min_width_mm };
    for item in &paper.items {
        match &item.shape {
            Shape::Text(text) => r.text(&layout_text(text, &paper, fonts), text.color),
            _ => r.item(item, &paper, &opts.hatch),
        }
    }
    Ok(pixmap)
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, c.a);
    p.anti_alias = true;
    p
}

fn rings_path(rings: &[Vec<Pt>]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for r in rings {
        let mut it = r.iter();
        let Some(first) = it.next() else { continue };
        pb.move_to(first.x as f32, first.y as f32);
        for p in it {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    pb.finish()
}

struct Renderer<'a> {
    pixmap: &'a mut Pixmap,
    t: Transform,
    min_width_mm: f64,
}

impl Renderer<'_> {
    fn stroke_style(&self, s: &DStroke) -> Stroke {
        let width = s.weight_mm.max(self.min_width_mm);
        let dashes = s.pattern.dashes_mm();
        let dash = if s.pattern == LinePattern::Solid {
            None
        } else {
            let mut v: Vec<f32> = dashes.iter().map(|d| *d as f32).collect();
            if v.len() % 2 == 1 {
                v.extend_from_slice(&v.clone());
            }
            StrokeDash::new(v, 0.0)
        };
        Stroke {
            width: width as f32,
            line_cap: if dash.is_some() { LineCap::Butt } else { LineCap::Round },
            line_join: LineJoin::Round,
            dash,
            ..Stroke::default()
        }
    }

    fn item(&mut self, item: &Item, paper: &Drawing, hatch: &HatchOptions) {
        if let Some(fill) = &item.fill {
            let rings = fill_rings(&item.shape, FLATTEN_TOLERANCE_MM);
            if let Some(path) = rings_path(&rings) {
                match fill {
                    Fill::Solid(c) => {
                        self.pixmap.fill_path(&path, &paint(*c), FillRule::EvenOdd, self.t, None)
                    }
                    Fill::Hatch { background, .. } => {
                        if let Some(bg) = background {
                            self.pixmap.fill_path(&path, &paint(*bg), FillRule::EvenOdd, self.t, None);
                        }
                        if let Ok(Some(lines)) = expand_fill(fill, &rings, paper, hatch) {
                            let mut pb = PathBuilder::new();
                            for [a, b] in &lines.segments {
                                pb.move_to(a.x as f32, a.y as f32);
                                pb.line_to(b.x as f32, b.y as f32);
                            }
                            if let Some(p) = pb.finish() {
                                let stroke = Stroke {
                                    width: lines.weight_mm.max(self.min_width_mm) as f32,
                                    line_cap: LineCap::Butt,
                                    ..Stroke::default()
                                };
                                self.pixmap.stroke_path(&p, &paint(lines.color), &stroke, self.t, None);
                            }
                        }
                    }
                }
            }
        }
        let Some(s) = &item.stroke else { return };
        let mut pb = PathBuilder::new();
        for (pts, closed) in stroke_paths(&item.shape, FLATTEN_TOLERANCE_MM) {
            let mut it = pts.iter();
            let Some(first) = it.next() else { continue };
            pb.move_to(first.x as f32, first.y as f32);
            for p in it {
                pb.line_to(p.x as f32, p.y as f32);
            }
            if closed {
                pb.close();
            }
        }
        if let Some(path) = pb.finish() {
            let stroke = self.stroke_style(s);
            self.pixmap.stroke_path(&path, &paint(s.color), &stroke, self.t, None);
        }
    }

    fn text(&mut self, placed: &kanavos_drawing::text::PlacedText, color: Rgba) {
        if !placed.fills.is_empty() {
            let mut pb = PathBuilder::new();
            for el in &placed.fills {
                match *el {
                    PathEl::MoveTo(p) => pb.move_to(p[0] as f32, p[1] as f32),
                    PathEl::LineTo(p) => pb.line_to(p[0] as f32, p[1] as f32),
                    PathEl::QuadTo(c, p) => pb.quad_to(c[0] as f32, c[1] as f32, p[0] as f32, p[1] as f32),
                    PathEl::CubicTo(c1, c2, p) => pb.cubic_to(
                        c1[0] as f32,
                        c1[1] as f32,
                        c2[0] as f32,
                        c2[1] as f32,
                        p[0] as f32,
                        p[1] as f32,
                    ),
                    PathEl::Close => pb.close(),
                }
            }
            if let Some(path) = pb.finish() {
                self.pixmap.fill_path(&path, &paint(color), FillRule::Winding, self.t, None);
                if placed.fill_embolden > 0.0 {
                    let s = Stroke {
                        width: placed.fill_embolden as f32,
                        line_join: LineJoin::Round,
                        ..Stroke::default()
                    };
                    self.pixmap.stroke_path(&path, &paint(color), &s, self.t, None);
                }
            }
        }
        if !placed.strokes.is_empty() {
            let mut pb = PathBuilder::new();
            for s in &placed.strokes {
                let mut it = s.iter();
                let Some(first) = it.next() else { continue };
                pb.move_to(first.x as f32, first.y as f32);
                for p in it {
                    pb.line_to(p.x as f32, p.y as f32);
                }
            }
            if let Some(path) = pb.finish() {
                let s = Stroke {
                    width: placed.stroke_width.max(self.min_width_mm) as f32,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                self.pixmap.stroke_path(&path, &paint(color), &s, self.t, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::{HatchPattern, Text, pt, weights};

    fn wall() -> Drawing {
        let mut d = Drawing::model(50.0);
        d.push(Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(2.0, 0.0), pt(2.0, 0.25), pt(0.0, 0.25)]],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            Some(DStroke::solid(weights::EXTRA_THICK)),
            "wall.cut",
        ));
        d.push(Item::text(Text::new(pt(0.0, 0.6), "Τοίχος", 3.5), "text"));
        d
    }

    fn pixel(img: &RasterImage, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width + x) * 4) as usize;
        [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2], img.rgba[i + 3]]
    }

    #[test]
    fn page_size_follows_dpi() {
        let opts = RasterOptions { size: RasterSize::Dpi(254.0), ..RasterOptions::default() };
        let img = render_with_fonts(&wall(), &opts, &Fonts::builtin()).unwrap();
        // 2 m at 1:50 = 40 mm + 10 mm margins = 50 mm → 500 px at 10 px/mm.
        assert_eq!(img.width, 500);
        assert_eq!(img.rgba.len(), (img.width * img.height * 4) as usize);
    }

    #[test]
    fn walls_hatches_and_text_leave_ink() {
        let opts = RasterOptions { size: RasterSize::Dpi(254.0), ..RasterOptions::default() };
        let img = render_with_fonts(&wall(), &opts, &Fonts::builtin()).unwrap();
        // Corner of the page stays white; the wall outline (bottom-left corner at 5 mm, 5 mm) is dark.
        assert_eq!(pixel(&img, 2, 2), [255, 255, 255, 255]);
        let y_bottom = img.height - 50;
        let p = pixel(&img, 50, y_bottom);
        assert!(p[0] < 80, "outline pixel {p:?}");
        // Inside the wall (5 mm tall, so 2.5 mm up is its middle): grey background and black hatch lines.
        let mut dark = 0;
        let mut grey = 0;
        for x in 100..400 {
            let p = pixel(&img, x, y_bottom - 25);
            if p[0] < 100 {
                dark += 1;
            } else if p[0] > 180 && p[0] < 230 {
                grey += 1;
            }
        }
        assert!(dark > 20 && grey > 50, "dark {dark}, grey {grey}");
    }

    #[test]
    fn fixed_pixel_sizes_fit_and_centre() {
        let opts = RasterOptions {
            size: RasterSize::Pixels { width: 200, height: 200 },
            ..RasterOptions::default()
        };
        let img = render_with_fonts(&wall(), &opts, &Fonts::builtin()).unwrap();
        assert_eq!((img.width, img.height), (200, 200));
        let png = render_png_with_fonts(&wall(), &opts, &Fonts::builtin()).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let reader = decoder.read_info().unwrap();
        assert_eq!(reader.info().width, 200);
    }

    #[test]
    fn bad_sizes_are_errors() {
        let d = wall();
        let f = Fonts::builtin();
        let zero =
            RasterOptions { size: RasterSize::Pixels { width: 0, height: 10 }, ..RasterOptions::default() };
        assert_eq!(render_with_fonts(&d, &zero, &f), Err(RasterError::InvalidSize));
        let nan = RasterOptions { size: RasterSize::Dpi(f64::NAN), ..RasterOptions::default() };
        assert_eq!(render_with_fonts(&d, &nan, &f), Err(RasterError::InvalidSize));
        let huge = RasterOptions { size: RasterSize::Dpi(1e6), ..RasterOptions::default() };
        assert!(matches!(render_with_fonts(&d, &huge, &f), Err(RasterError::TooLarge { .. })));
    }

    #[test]
    fn empty_drawings_render_blank() {
        let img = render_with_fonts(&Drawing::paper(), &RasterOptions::default(), &Fonts::builtin()).unwrap();
        assert!(img.rgba.chunks(4).all(|p| p == [255, 255, 255, 255]));
    }
}
