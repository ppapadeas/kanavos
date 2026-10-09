//! A tiny rasteriser and PNG writer for visual checks of plan geometry (test helper only).
//!
//! Polygons are filled with the even-odd rule by scanlines; lines are drawn with Bresenham's algorithm. PNG output
//! uses uncompressed deflate blocks (RFC 1950/1951) with CRC-32 (ISO 3309) and Adler-32 checksums, which every
//! image viewer reads.

use std::io::Write;

use kanavos_geom::{Bbox2, Point2, Polygon2, PolygonWithHoles, Segment2};

pub type Rgb = [u8; 3];

pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pixels: Vec<Rgb>,
    min: Point2,
    scale: f64,
}

impl Canvas {
    /// A white canvas showing `bounds` (grown by 5 %) at `px_per_m` pixels per metre.
    pub fn new(bounds: Bbox2, px_per_m: f64) -> Canvas {
        let m = (bounds.width().as_m().max(bounds.height().as_m())) * 0.05 + 0.2;
        let b = bounds.expanded(kanavos_geom::Length::m(m));
        let width = ((b.width().as_m() * px_per_m).ceil() as usize).clamp(16, 4000);
        let height = ((b.height().as_m() * px_per_m).ceil() as usize).clamp(16, 4000);
        Canvas { width, height, pixels: vec![[255, 255, 255]; width * height], min: b.min, scale: px_per_m }
    }

    fn to_px(&self, p: Point2) -> (f64, f64) {
        let [x, y] = p.xy();
        ((x - self.min.x.as_m()) * self.scale, self.height as f64 - (y - self.min.y.as_m()) * self.scale)
    }

    /// Fills a triangle given in canvas coordinates with a depth per corner, keeping the nearest (smallest
    /// depth) colour per pixel.
    pub fn triangle(&mut self, depth: &mut Vec<f64>, pts: [(Point2, f64); 3], c: Rgb) {
        if depth.len() != self.width * self.height {
            *depth = vec![f64::INFINITY; self.width * self.height];
        }
        let p: Vec<(f64, f64, f64)> = pts
            .iter()
            .map(|(q, z)| {
                let (x, y) = self.to_px(*q);
                (x, y, *z)
            })
            .collect();
        let (x0, y0, z0) = p[0];
        let (x1, y1, z1) = p[1];
        let (x2, y2, z2) = p[2];
        let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
        if area.abs() < 1e-9 {
            return;
        }
        let minx = x0.min(x1).min(x2).floor().max(0.0) as usize;
        let maxx = (x0.max(x1).max(x2).ceil() as usize).min(self.width.saturating_sub(1));
        let miny = y0.min(y1).min(y2).floor().max(0.0) as usize;
        let maxy = (y0.max(y1).max(y2).ceil() as usize).min(self.height.saturating_sub(1));
        for py in miny..=maxy {
            for px in minx..=maxx {
                let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
                let w0 = ((x1 - x) * (y2 - y) - (x2 - x) * (y1 - y)) / area;
                let w1 = ((x2 - x) * (y0 - y) - (x0 - x) * (y2 - y)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = w0 * z0 + w1 * z1 + w2 * z2;
                let i = py * self.width + px;
                if z < depth[i] {
                    depth[i] = z;
                    self.pixels[i] = c;
                }
            }
        }
    }

    fn set(&mut self, x: i64, y: i64, c: Rgb) {
        if x >= 0
            && y >= 0
            && (x as usize) < self.width
            && (y as usize) < self.height
            && let Some(px) = self.pixels.get_mut(y as usize * self.width + x as usize)
        {
            *px = c;
        }
    }

    /// Fills polygons with holes (even-odd).
    pub fn fill(&mut self, polys: &[PolygonWithHoles], c: Rgb) {
        let rings: Vec<Vec<(f64, f64)>> = polys
            .iter()
            .flat_map(|p| {
                p.rings()
                    .map(|r| r.vertices.iter().map(|q| self.to_px(*q)).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            })
            .collect();
        for row in 0..self.height {
            let y = row as f64 + 0.5;
            let mut xs: Vec<f64> = Vec::new();
            for r in &rings {
                let n = r.len();
                for i in 0..n {
                    let (x0, y0) = r[i];
                    let (x1, y1) = r[(i + 1) % n];
                    if (y0 > y) != (y1 > y) {
                        xs.push(x0 + (y - y0) / (y1 - y0) * (x1 - x0));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks(2) {
                if let [a, b] = pair {
                    for x in (a.round() as i64)..(b.round() as i64) {
                        self.set(x, row as i64, c);
                    }
                }
            }
        }
    }

    pub fn fill_polygon(&mut self, p: &Polygon2, c: Rgb) {
        self.fill(&[p.clone().into()], c);
    }

    /// Draws a segment.
    pub fn line(&mut self, s: &Segment2, c: Rgb) {
        let (x0, y0) = self.to_px(s.start);
        let (x1, y1) = self.to_px(s.end);
        let (mut x, mut y) = (x0.round() as i64, y0.round() as i64);
        let (xe, ye) = (x1.round() as i64, y1.round() as i64);
        let dx = (xe - x).abs();
        let dy = -(ye - y).abs();
        let sx = if x < xe { 1 } else { -1 };
        let sy = if y < ye { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set(x, y, c);
            if x == xe && y == ye {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draws a dashed segment.
    pub fn dashed(&mut self, s: &Segment2, c: Rgb) {
        let len = s.length().as_m();
        let dash = 6.0 / self.scale;
        let n = ((len / dash).ceil() as usize).max(1);
        for i in (0..n).step_by(2) {
            let a = s.at(i as f64 / n as f64);
            let b = s.at(((i + 1) as f64 / n as f64).min(1.0));
            self.line(&Segment2::new(a, b), c);
        }
    }

    /// Outlines polygons.
    pub fn outline(&mut self, polys: &[PolygonWithHoles], c: Rgb) {
        for p in polys {
            for r in p.rings() {
                for e in r.edges() {
                    self.line(&e, c);
                }
            }
        }
    }

    /// Writes the canvas as an 8-bit RGB PNG.
    pub fn write_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        let mut raw = Vec::with_capacity((self.width * 3 + 1) * self.height);
        for row in 0..self.height {
            raw.push(0u8); // filter: none
            for px in &self.pixels[row * self.width..(row + 1) * self.width] {
                raw.extend_from_slice(px);
            }
        }
        let mut z = vec![0x78, 0x01];
        for (i, chunk) in raw.chunks(65_535).enumerate() {
            let last = (i + 1) * 65_535 >= raw.len();
            z.push(u8::from(last));
            let len = chunk.len() as u16;
            z.extend_from_slice(&len.to_le_bytes());
            z.extend_from_slice(&(!len).to_le_bytes());
            z.extend_from_slice(chunk);
        }
        z.extend_from_slice(&adler32(&raw).to_be_bytes());
        let mut out = Vec::new();
        out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(self.width as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.height as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &z);
        chunk(&mut out, b"IEND", &[]);
        let mut f = std::fs::File::create(path)?;
        f.write_all(&out)
    }
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut c = Vec::with_capacity(4 + data.len());
    c.extend_from_slice(kind);
    c.extend_from_slice(data);
    out.extend_from_slice(&c);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// A colour per material id, stable across runs.
pub fn material_colour(id: Option<kanavos_model::Id>) -> Rgb {
    let palette: [Rgb; 8] = [
        [214, 120, 90],
        [240, 230, 160],
        [170, 190, 220],
        [200, 200, 200],
        [150, 200, 150],
        [230, 170, 200],
        [180, 160, 120],
        [120, 170, 170],
    ];
    match id {
        Some(i) => palette[(i.0 as usize) % palette.len()],
        None => [220, 220, 220],
    }
}
