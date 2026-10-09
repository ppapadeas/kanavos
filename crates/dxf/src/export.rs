//! DXF export, format version AC1015 (R2000).
//!
//! R2000 is the oldest version with lightweight polylines, hatch entities and lineweights, all of which drawings
//! need, and it is read by practically every CAD program. The file follows the published DXF reference: HEADER,
//! CLASSES, TABLES (all nine tables with their required entries), BLOCKS (model and paper space), ENTITIES and
//! OBJECTS (root dictionary, groups and the two layouts), with a handle on every object and owner handles.
//!
//! Layer mapping (see `specs/drawing/dxf-layers.md`):
//!
//! - every item layer becomes a DXF layer of the same name (characters DXF forbids become `_`); the layer takes the
//!   most common colour, lineweight and line type of its items, and items that match use BYLAYER;
//! - hatch lines go on `<layer>.hatch` as LINE entities; hatch backgrounds and solid fills are SOLID HATCH
//!   entities on the item's layer;
//! - line types are written as `KV_*` linetype definitions with lengths already scaled to the drawing.

use std::collections::BTreeMap;

use kanavos_drawing::fonts::Fonts;
use kanavos_drawing::geometry::{FLATTEN_TOLERANCE_MM, fill_rings};
use kanavos_drawing::hatch::{HatchOptions, expand_fill};
use kanavos_drawing::page::content_bounds;
use kanavos_drawing::text::text_width_mm;
use kanavos_drawing::{
    Drawing, DrawingUnits, Fill, Item, LinePattern, Pt, Rect, Rgba, Shape, Stroke, Text, pt,
};

use crate::aci::{lineweight, rgb_to_aci};
use crate::codepage::{Codepage, encode_dxf_text};

/// Coordinate units of the exported file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxfUnits {
    /// Metres (`$INSUNITS` 6).
    Metres,
    /// Millimetres (`$INSUNITS` 4).
    Millimetres,
}

impl DxfUnits {
    fn insunits(self) -> i32 {
        match self {
            DxfUnits::Metres => 6,
            DxfUnits::Millimetres => 4,
        }
    }
}

/// Options for [`write_dxf`].
#[derive(Debug, Clone, PartialEq)]
pub struct DxfExportOptions {
    /// Output units; `None` keeps the drawing's own (metres for model drawings, millimetres for paper drawings).
    /// Coordinates are scaled accordingly.
    pub units: Option<DxfUnits>,
    /// Code page for text: [`Codepage::Ansi1253`] writes Greek as single bytes; any other non-UTF-8 code page
    /// writes characters it lacks as `\U+XXXX` escapes. The default, [`Codepage::Ansi1252`], keeps the file pure
    /// ASCII apart from Western accents, with Greek as escapes, which every reader understands.
    pub codepage: Codepage,
    /// Font file named by the text styles (regular).
    pub font_file: String,
    /// Font file named by the bold text style.
    pub bold_font_file: String,
    /// Limits for hatch expansion. Areas whose hatch is too dense get their background only.
    pub hatch: HatchOptions,
}

impl Default for DxfExportOptions {
    fn default() -> Self {
        DxfExportOptions {
            units: None,
            codepage: Codepage::Ansi1252,
            font_file: "arial.ttf".to_owned(),
            bold_font_file: "arialbd.ttf".to_owned(),
            hatch: HatchOptions::default(),
        }
    }
}

/// Characters not allowed in DXF symbol names.
const FORBIDDEN: &[char] = &['<', '>', '/', '\\', '"', ':', ';', '?', '*', '|', '=', '`', ','];

/// The DXF layer name for an item layer.
pub fn layer_name(layer: &str) -> String {
    let s: String = layer
        .trim()
        .chars()
        .map(|c| if FORBIDDEN.contains(&c) || c.is_control() { '_' } else { c })
        .collect();
    let s: String = s.chars().take(255).collect();
    if s.is_empty() { "0".to_owned() } else { s }
}

/// The layer for hatch lines of items on `layer`.
pub fn hatch_layer_name(layer: &str) -> String {
    layer_name(&format!("{}.hatch", layer_name(layer)))
}

/// Formats a real number for DXF: the shortest decimal that reads back to the same value, with at least one
/// decimal and never an exponent (Rust's `Display` for floats never uses exponents).
fn real(v: f64) -> String {
    if !v.is_finite() || v.abs() < f64::MIN_POSITIVE {
        return "0.0".to_owned();
    }
    let s = v.to_string();
    if s.contains('.') { s } else { format!("{s}.0") }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum LtKey {
    Continuous,
    Named(&'static str, Vec<i64>),
    Custom(Vec<i64>),
}

/// Style of one entity: what it would need explicitly.
#[derive(Debug, Clone, PartialEq)]
struct Style {
    color: i32,
    weight: i32,
    linetype: String,
}

#[derive(Default)]
struct LayerStats {
    colors: BTreeMap<i32, usize>,
    weights: BTreeMap<i32, usize>,
    linetypes: BTreeMap<String, usize>,
}

fn most_common<K: Clone + Ord>(m: &BTreeMap<K, usize>) -> Option<K> {
    m.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(k, _)| k.clone())
}

/// Writes group code / value pairs.
struct Out {
    buf: Vec<u8>,
    cp: Codepage,
    next: u64,
}

impl Out {
    fn pair(&mut self, code: i32, value: &str) {
        self.buf.extend_from_slice(format!("{code:>3}\r\n").as_bytes());
        self.buf.extend_from_slice(&encode_dxf_text(value, self.cp));
        self.buf.extend_from_slice(b"\r\n");
    }
    fn int(&mut self, code: i32, v: i64) {
        self.pair(code, &v.to_string());
    }
    fn real(&mut self, code: i32, v: f64) {
        self.pair(code, &real(v));
    }
    fn point(&mut self, code: i32, p: Pt) {
        self.real(code, p.x);
        self.real(code + 10, p.y);
        self.real(code + 20, 0.0);
    }
    fn handle(&mut self) -> String {
        let h = format!("{:X}", self.next);
        self.next += 1;
        h
    }
}

/// Fixed handles of the file structure.
mod h {
    pub const VPORT_T: &str = "1";
    pub const LTYPE_T: &str = "2";
    pub const LAYER_T: &str = "3";
    pub const STYLE_T: &str = "4";
    pub const VIEW_T: &str = "5";
    pub const UCS_T: &str = "6";
    pub const APPID_T: &str = "7";
    pub const DIMSTYLE_T: &str = "8";
    pub const BLOCK_RECORD_T: &str = "9";
    pub const ROOT_DICT: &str = "A";
    pub const GROUP_DICT: &str = "B";
    pub const LAYOUT_DICT: &str = "C";
    pub const MODEL_BR: &str = "D";
    pub const PAPER_BR: &str = "E";
    pub const MODEL_BLOCK: &str = "F";
    pub const MODEL_END: &str = "10";
    pub const PAPER_BLOCK: &str = "11";
    pub const PAPER_END: &str = "12";
    pub const MODEL_LAYOUT: &str = "13";
    pub const PAPER_LAYOUT: &str = "14";
    /// First handle handed out dynamically.
    pub const FIRST_FREE: u64 = 0x20;
}

/// Exports a drawing as an ASCII DXF file (AC1015). Text positions use the built-in font metrics, so the output is
/// identical on every machine.
pub fn write_dxf(drawing: &Drawing, opts: &DxfExportOptions) -> Vec<u8> {
    let fonts = Fonts::builtin();
    let native = match drawing.units {
        DrawingUnits::Metres => DxfUnits::Metres,
        DrawingUnits::Millimetres => DxfUnits::Millimetres,
    };
    let units = opts.units.unwrap_or(native);
    let factor = match (native, units) {
        (DxfUnits::Metres, DxfUnits::Millimetres) => 1000.0,
        (DxfUnits::Millimetres, DxfUnits::Metres) => 0.001,
        _ => 1.0,
    };
    let unit = drawing.paper_mm(1.0) * factor; // output units per paper millimetre
    let map = |p: Pt| pt(p.x * factor, p.y * factor);

    let mut e = Exporter {
        drawing,
        factor,
        unit,
        out: Out { buf: Vec::new(), cp: opts.codepage, next: h::FIRST_FREE },
        layers: BTreeMap::new(),
        linetypes: BTreeMap::new(),
        styles: Vec::new(),
        fonts: &fonts,
        hatch: opts.hatch,
    };
    e.collect();
    let entities = e.entities(&map);
    let tables = e.tables(opts);
    let objects = e.objects();
    let next = e.out.next;

    let bounds = content_bounds(drawing, &fonts)
        .map(|b| Rect { min: map(b.min), max: map(b.max) })
        .unwrap_or(Rect { min: pt(0.0, 0.0), max: pt(0.0, 0.0) });
    let mut head = Out { buf: Vec::new(), cp: opts.codepage, next: 0 };
    head.pair(0, "SECTION");
    head.pair(2, "HEADER");
    let var = |o: &mut Out, name: &str| o.pair(9, name);
    var(&mut head, "$ACADVER");
    head.pair(1, "AC1015");
    var(&mut head, "$DWGCODEPAGE");
    head.pair(3, opts.codepage.dxf_name());
    var(&mut head, "$INSBASE");
    head.point(10, pt(0.0, 0.0));
    var(&mut head, "$EXTMIN");
    head.point(10, bounds.min);
    var(&mut head, "$EXTMAX");
    head.point(10, bounds.max);
    var(&mut head, "$LIMMIN");
    head.real(10, bounds.min.x);
    head.real(20, bounds.min.y);
    var(&mut head, "$LIMMAX");
    head.real(10, bounds.max.x);
    head.real(20, bounds.max.y);
    var(&mut head, "$LTSCALE");
    head.real(40, 1.0);
    var(&mut head, "$TEXTSTYLE");
    head.pair(7, "Standard");
    var(&mut head, "$CLAYER");
    head.pair(8, "0");
    var(&mut head, "$CELTYPE");
    head.pair(6, "ByLayer");
    var(&mut head, "$CECOLOR");
    head.int(62, 256);
    var(&mut head, "$CELWEIGHT");
    head.int(370, -1);
    var(&mut head, "$LWDISPLAY");
    head.int(290, 1);
    var(&mut head, "$MEASUREMENT");
    head.int(70, 1);
    var(&mut head, "$INSUNITS");
    head.int(70, i64::from(units.insunits()));
    var(&mut head, "$LUNITS");
    head.int(70, 2);
    var(&mut head, "$LUPREC");
    head.int(70, 3);
    var(&mut head, "$PSLTSCALE");
    head.int(70, 1);
    var(&mut head, "$HANDSEED");
    head.pair(5, &format!("{next:X}"));
    head.pair(0, "ENDSEC");
    head.pair(0, "SECTION");
    head.pair(2, "CLASSES");
    head.pair(0, "ENDSEC");

    let mut file = head.buf;
    file.extend_from_slice(&tables);
    file.extend_from_slice(&entities);
    file.extend_from_slice(&objects);
    file.extend_from_slice(b"  0\r\nEOF\r\n");
    file
}

struct Exporter<'a> {
    drawing: &'a Drawing,
    factor: f64,
    unit: f64,
    out: Out,
    layers: BTreeMap<String, LayerStats>,
    linetypes: BTreeMap<LtKey, String>,
    /// Final layer table: name, colour, weight, linetype name.
    styles: Vec<(String, Style)>,
    fonts: &'a Fonts,
    hatch: HatchOptions,
}

impl Exporter<'_> {
    fn lt_key(&self, p: &LinePattern) -> LtKey {
        let q = |v: &[f64]| v.iter().map(|x| (x * self.unit * 1e6).round() as i64).collect::<Vec<i64>>();
        let dashes = p.dashes_mm();
        if dashes.is_empty()
            || !dashes.iter().all(|d| d.is_finite() && *d >= 0.0)
            || dashes.iter().sum::<f64>() <= 0.0
        {
            return LtKey::Continuous;
        }
        match p {
            LinePattern::Solid => LtKey::Continuous,
            LinePattern::Dashed => LtKey::Named("KV_DASHED", q(&dashes)),
            LinePattern::Hidden => LtKey::Named("KV_HIDDEN", q(&dashes)),
            LinePattern::Dotted => LtKey::Named("KV_DOTTED", q(&dashes)),
            LinePattern::DashDot => LtKey::Named("KV_DASHDOT", q(&dashes)),
            LinePattern::DashDotDot => LtKey::Named("KV_DASHDOTDOT", q(&dashes)),
            LinePattern::Custom(_) => LtKey::Custom(q(&dashes)),
        }
    }

    fn linetype_name(&mut self, p: &LinePattern) -> String {
        let key = self.lt_key(p);
        if key == LtKey::Continuous {
            return "Continuous".to_owned();
        }
        let n = self.linetypes.len() + 1;
        self.linetypes
            .entry(key.clone())
            .or_insert_with(|| match key {
                LtKey::Named(name, _) => name.to_owned(),
                _ => format!("KV_CUSTOM{n}"),
            })
            .clone()
    }

    fn stroke_style(&mut self, s: &Stroke) -> Style {
        Style {
            color: rgb_to_aci(s.color),
            weight: lineweight(s.weight_mm),
            linetype: self.linetype_name(&s.pattern),
        }
    }

    fn note(&mut self, layer: &str, st: &Style) {
        let l = self.layers.entry(layer.to_owned()).or_default();
        *l.colors.entry(st.color).or_default() += 1;
        *l.weights.entry(st.weight).or_default() += 1;
        *l.linetypes.entry(st.linetype.clone()).or_default() += 1;
    }

    /// First pass: layer statistics and line types.
    fn collect(&mut self) {
        self.layers.entry("0".to_owned()).or_default();
        let items = self.drawing.items.clone();
        for item in &items {
            let layer = layer_name(&item.layer);
            if let Some(s) = &item.stroke {
                let st = self.stroke_style(s);
                self.note(&layer, &st);
            }
            match (&item.shape, &item.fill) {
                (Shape::Text(t), _) => {
                    let st = Style { color: rgb_to_aci(t.color), weight: 25, linetype: "Continuous".into() };
                    self.note(&layer, &st);
                }
                (_, Some(Fill::Hatch { pattern, color, .. }))
                    if *pattern != kanavos_drawing::HatchPattern::None =>
                {
                    let w = lineweight(kanavos_drawing::hatch::pattern_weight_mm(*pattern));
                    let st = Style { color: rgb_to_aci(*color), weight: w, linetype: "Continuous".into() };
                    self.note(&hatch_layer_name(&item.layer), &st);
                }
                _ => {
                    self.layers.entry(layer).or_default();
                }
            }
        }
        let names: Vec<String> = self.layers.keys().cloned().collect();
        for name in names {
            let l = self.layers.get(&name);
            let st = Style {
                color: l.and_then(|l| most_common(&l.colors)).unwrap_or(7),
                weight: l.and_then(|l| most_common(&l.weights)).unwrap_or(25),
                linetype: l.and_then(|l| most_common(&l.linetypes)).unwrap_or_else(|| "Continuous".into()),
            };
            self.styles.push((name, st));
        }
    }

    fn layer_style(&self, layer: &str) -> Option<&Style> {
        self.styles.iter().find(|(n, _)| n == layer).map(|(_, s)| s)
    }

    /// Common entity header: type, handle, owner, layer, and the style codes that differ from the layer.
    fn entity(&mut self, kind: &str, layer: &str, st: Option<&Style>) {
        let handle = self.out.handle();
        self.out.pair(0, kind);
        self.out.pair(5, &handle);
        self.out.pair(330, h::MODEL_BR);
        self.out.pair(100, "AcDbEntity");
        self.out.pair(8, layer);
        if let Some(st) = st {
            let base = self.layer_style(layer).cloned();
            if base.as_ref().map(|b| &b.linetype) != Some(&st.linetype) {
                self.out.pair(6, &st.linetype);
            }
            if base.as_ref().map(|b| b.color) != Some(st.color) {
                self.out.int(62, i64::from(st.color));
            }
            if base.as_ref().map(|b| b.weight) != Some(st.weight) {
                self.out.int(370, i64::from(st.weight));
            }
        }
    }

    fn entities(&mut self, map: &dyn Fn(Pt) -> Pt) -> Vec<u8> {
        self.out.pair(0, "SECTION");
        self.out.pair(2, "ENTITIES");
        let items = self.drawing.items.clone();
        for item in &items {
            self.item(item, map);
        }
        self.out.pair(0, "ENDSEC");
        std::mem::take(&mut self.out.buf)
    }

    fn item(&mut self, item: &Item, map: &dyn Fn(Pt) -> Pt) {
        let layer = layer_name(&item.layer);
        if let Shape::Text(t) = &item.shape {
            self.text(t, &layer, map);
            return;
        }
        if let Some(fill) = &item.fill {
            self.fill(item, fill, &layer, map);
        }
        let Some(stroke) = &item.stroke else { return };
        let st = self.stroke_style(stroke);
        match &item.shape {
            Shape::Line { a, b } => {
                self.entity("LINE", &layer, Some(&st));
                self.out.pair(100, "AcDbLine");
                self.out.point(10, map(*a));
                self.out.point(11, map(*b));
            }
            Shape::Polyline { points, closed } => self.lwpolyline(points, *closed, &layer, &st, map),
            Shape::Polygon { rings } => {
                for r in rings {
                    self.lwpolyline(r, true, &layer, &st, map);
                }
            }
            Shape::Circle { centre, radius } => {
                self.entity("CIRCLE", &layer, Some(&st));
                self.out.pair(100, "AcDbCircle");
                self.out.point(10, map(*centre));
                self.out.real(40, radius * self.factor);
            }
            Shape::Arc { centre, radius, start, sweep } => {
                if sweep.abs() >= std::f64::consts::TAU - 1e-12 {
                    self.entity("CIRCLE", &layer, Some(&st));
                    self.out.pair(100, "AcDbCircle");
                    self.out.point(10, map(*centre));
                    self.out.real(40, radius * self.factor);
                } else {
                    let (a0, a1) =
                        if *sweep >= 0.0 { (*start, start + sweep) } else { (start + sweep, *start) };
                    self.entity("ARC", &layer, Some(&st));
                    self.out.pair(100, "AcDbCircle");
                    self.out.point(10, map(*centre));
                    self.out.real(40, radius * self.factor);
                    self.out.pair(100, "AcDbArc");
                    self.out.real(50, a0.to_degrees().rem_euclid(360.0));
                    self.out.real(51, a1.to_degrees().rem_euclid(360.0));
                }
            }
            Shape::Text(_) => {}
        }
    }

    fn lwpolyline(&mut self, points: &[Pt], closed: bool, layer: &str, st: &Style, map: &dyn Fn(Pt) -> Pt) {
        if points.len() < 2 {
            return;
        }
        self.entity("LWPOLYLINE", layer, Some(st));
        self.out.pair(100, "AcDbPolyline");
        self.out.int(90, points.len() as i64);
        self.out.int(70, i64::from(closed));
        for p in points {
            let q = map(*p);
            self.out.real(10, q.x);
            self.out.real(20, q.y);
        }
    }

    fn fill(&mut self, item: &Item, fill: &Fill, layer: &str, map: &dyn Fn(Pt) -> Pt) {
        let rings = fill_rings(&item.shape, self.drawing.paper_mm(FLATTEN_TOLERANCE_MM));
        if rings.is_empty() {
            return;
        }
        let solid = match fill {
            Fill::Solid(c) => Some(*c),
            Fill::Hatch { background, .. } => *background,
        };
        if let Some(c) = solid {
            // Circles keep an exact boundary: two half-circle bulges.
            let paths: Vec<Vec<(Pt, f64)>> = match &item.shape {
                Shape::Circle { centre, radius } => {
                    vec![vec![(centre.offset(*radius, 0.0), 1.0), (centre.offset(-radius, 0.0), 1.0)]]
                }
                _ => rings.iter().map(|r| r.iter().map(|p| (*p, 0.0)).collect()).collect(),
            };
            self.solid_hatch(&paths, layer, c, map);
        }
        if let Ok(Some(lines)) = expand_fill(fill, &rings, self.drawing, &self.hatch) {
            let hl = hatch_layer_name(&item.layer);
            let st = Style {
                color: rgb_to_aci(lines.color),
                weight: lineweight(lines.weight_mm),
                linetype: "Continuous".into(),
            };
            for [a, b] in &lines.segments {
                self.entity("LINE", &hl, Some(&st));
                self.out.pair(100, "AcDbLine");
                self.out.point(10, map(*a));
                self.out.point(11, map(*b));
            }
        }
    }

    fn solid_hatch(&mut self, paths: &[Vec<(Pt, f64)>], layer: &str, color: Rgba, map: &dyn Fn(Pt) -> Pt) {
        let st = Style { color: rgb_to_aci(color), weight: -1, linetype: "Continuous".into() };
        let handle = self.out.handle();
        self.out.pair(0, "HATCH");
        self.out.pair(5, &handle);
        self.out.pair(330, h::MODEL_BR);
        self.out.pair(100, "AcDbEntity");
        self.out.pair(8, layer);
        if self.layer_style(layer).map(|b| b.color) != Some(st.color) {
            self.out.int(62, i64::from(st.color));
        }
        self.out.pair(100, "AcDbHatch");
        self.out.point(10, pt(0.0, 0.0));
        self.out.real(210, 0.0);
        self.out.real(220, 0.0);
        self.out.real(230, 1.0);
        self.out.pair(2, "SOLID");
        self.out.int(70, 1);
        self.out.int(71, 0);
        self.out.int(91, paths.len() as i64);
        for (i, path) in paths.iter().enumerate() {
            let bulged = path.iter().any(|(_, b)| b.abs() > 0.0);
            // Polyline boundary; the first is the external one.
            self.out.int(92, if i == 0 { 3 } else { 2 });
            self.out.int(72, i64::from(bulged));
            self.out.int(73, 1);
            self.out.int(93, path.len() as i64);
            for (p, b) in path {
                let q = map(*p);
                self.out.real(10, q.x);
                self.out.real(20, q.y);
                if bulged {
                    self.out.real(42, *b);
                }
            }
            self.out.int(97, 0);
        }
        self.out.int(75, 0); // odd parity: the even-odd rule
        self.out.int(76, 1);
        self.out.int(98, 0);
    }

    fn text(&mut self, t: &Text, layer: &str, map: &dyn Fn(Pt) -> Pt) {
        if t.content.trim().is_empty() || !(t.height_mm.is_finite() && t.height_mm > 0.0) {
            return;
        }
        let st = Style { color: rgb_to_aci(t.color), weight: 25, linetype: "Continuous".into() };
        let height = self.drawing.paper_mm(t.height_mm) * self.factor;
        let rot = if t.rotation.is_finite() { t.rotation } else { 0.0 };
        let anchor = map(t.position);
        let h_align = match t.anchor.h_fraction() {
            f if f < 0.25 => 0,
            f if f < 0.75 => 1,
            _ => 2,
        };
        let v_align = match t.anchor.v_fraction() {
            f if f < 0.25 => 0,
            f if f < 0.75 => 2,
            _ => 3,
        };
        // First alignment point: the start of the baseline (readers recompute it from the second point).
        let w = self.drawing.paper_mm(text_width_mm(t, self.fonts)) * self.factor;
        let (s, c) = rot.sin_cos();
        let (dx, dy) = (-t.anchor.h_fraction() * w, -t.anchor.v_fraction() * height);
        let first = pt(anchor.x + c * dx - s * dy, anchor.y + s * dx + c * dy);
        self.entity("TEXT", layer, Some(&st));
        self.out.pair(100, "AcDbText");
        self.out.point(10, first);
        self.out.real(40, height);
        self.out.pair(1, &dxf_text_content(&t.content));
        if rot.abs() > 1e-12 {
            self.out.real(50, rot.to_degrees().rem_euclid(360.0));
        }
        self.out.pair(7, if t.bold { "Bold" } else { "Standard" });
        if h_align != 0 {
            self.out.int(72, h_align);
        }
        if h_align != 0 || v_align != 0 {
            self.out.point(11, anchor);
        }
        self.out.pair(100, "AcDbText");
        if v_align != 0 {
            self.out.int(73, v_align);
        }
    }

    fn table_head(&mut self, name: &str, handle: &str, count: usize) {
        self.out.pair(0, "TABLE");
        self.out.pair(2, name);
        self.out.pair(5, handle);
        self.out.pair(330, "0");
        self.out.pair(100, "AcDbSymbolTable");
        self.out.int(70, count as i64);
    }

    fn record(&mut self, kind: &str, table: &str, subclass: &str) {
        let handle = self.out.handle();
        self.out.pair(0, kind);
        self.out.pair(if kind == "DIMSTYLE" { 105 } else { 5 }, &handle);
        self.out.pair(330, table);
        self.out.pair(100, "AcDbSymbolTableRecord");
        self.out.pair(100, subclass);
    }

    fn tables(&mut self, opts: &DxfExportOptions) -> Vec<u8> {
        let bounds = content_bounds(self.drawing, self.fonts);
        self.out.pair(0, "SECTION");
        self.out.pair(2, "TABLES");

        self.table_head("VPORT", h::VPORT_T, 1);
        self.record("VPORT", h::VPORT_T, "AcDbViewportTableRecord");
        self.out.pair(2, "*Active");
        self.out.int(70, 0);
        let (cx, cy, height, aspect) = match bounds {
            Some(b) => {
                let c = b.centre();
                let hgt = (b.height() * self.factor * 1.1).max(1e-6);
                (
                    c.x * self.factor,
                    c.y * self.factor,
                    hgt,
                    (b.width() / b.height().max(1e-12)).clamp(0.1, 10.0),
                )
            }
            None => (0.0, 0.0, 10.0, 1.5),
        };
        for (code, v) in
            [(10, 0.0), (20, 0.0), (11, 1.0), (21, 1.0), (12, cx), (22, cy), (13, 0.0), (23, 0.0)]
        {
            self.out.real(code, v);
        }
        for (code, v) in [(14, 1.0), (24, 1.0), (15, 1.0), (25, 1.0), (16, 0.0), (26, 0.0), (36, 1.0)] {
            self.out.real(code, v);
        }
        for (code, v) in [(17, 0.0), (27, 0.0), (37, 0.0), (40, height), (41, aspect), (42, 50.0)] {
            self.out.real(code, v);
        }
        for (code, v) in [(43, 0.0), (44, 0.0), (50, 0.0), (51, 0.0)] {
            self.out.real(code, v);
        }
        for (code, v) in [(71, 0), (72, 100), (73, 1), (74, 3), (75, 0), (76, 0), (77, 0), (78, 0)] {
            self.out.int(code, v);
        }
        self.out.pair(0, "ENDTAB");

        let lts: Vec<(LtKey, String)> = self.linetypes.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        self.table_head("LTYPE", h::LTYPE_T, 3 + lts.len());
        for name in ["ByBlock", "ByLayer", "Continuous"] {
            self.record("LTYPE", h::LTYPE_T, "AcDbLinetypeTableRecord");
            self.out.pair(2, name);
            self.out.int(70, 0);
            self.out.pair(3, if name == "Continuous" { "Solid line" } else { "" });
            self.out.int(72, 65);
            self.out.int(73, 0);
            self.out.real(40, 0.0);
        }
        for (key, name) in lts {
            let dashes: Vec<f64> = match &key {
                LtKey::Named(_, v) | LtKey::Custom(v) => v.iter().map(|x| *x as f64 / 1e6).collect(),
                LtKey::Continuous => Vec::new(),
            };
            let mut dashes = dashes;
            if dashes.len() % 2 == 1 {
                dashes.extend(dashes.clone());
            }
            self.record("LTYPE", h::LTYPE_T, "AcDbLinetypeTableRecord");
            self.out.pair(2, &name);
            self.out.int(70, 0);
            self.out.pair(3, &format!("Kanavos {}", name.trim_start_matches("KV_").to_lowercase()));
            self.out.int(72, 65);
            self.out.int(73, dashes.len() as i64);
            self.out.real(40, dashes.iter().sum());
            for (i, d) in dashes.iter().enumerate() {
                self.out.real(49, if i % 2 == 0 { *d } else { -d });
                self.out.int(74, 0);
            }
        }
        self.out.pair(0, "ENDTAB");

        let styles = self.styles.clone();
        self.table_head("LAYER", h::LAYER_T, styles.len());
        for (name, st) in &styles {
            self.record("LAYER", h::LAYER_T, "AcDbLayerTableRecord");
            self.out.pair(2, name);
            self.out.int(70, 0);
            self.out.int(62, i64::from(st.color));
            self.out.pair(6, &st.linetype);
            self.out.int(370, i64::from(if st.weight < 0 { -3 } else { st.weight }));
        }
        self.out.pair(0, "ENDTAB");

        self.table_head("STYLE", h::STYLE_T, 2);
        for (name, font) in [("Standard", &opts.font_file), ("Bold", &opts.bold_font_file)] {
            self.record("STYLE", h::STYLE_T, "AcDbTextStyleTableRecord");
            self.out.pair(2, name);
            self.out.int(70, 0);
            self.out.real(40, 0.0);
            self.out.real(41, 1.0);
            self.out.real(50, 0.0);
            self.out.int(71, 0);
            self.out.real(42, 2.5 * self.unit);
            self.out.pair(3, font);
            self.out.pair(4, "");
        }
        self.out.pair(0, "ENDTAB");

        self.table_head("VIEW", h::VIEW_T, 0);
        self.out.pair(0, "ENDTAB");
        self.table_head("UCS", h::UCS_T, 0);
        self.out.pair(0, "ENDTAB");

        self.table_head("APPID", h::APPID_T, 1);
        self.record("APPID", h::APPID_T, "AcDbRegAppTableRecord");
        self.out.pair(2, "ACAD");
        self.out.int(70, 0);
        self.out.pair(0, "ENDTAB");

        self.table_head("DIMSTYLE", h::DIMSTYLE_T, 1);
        self.out.pair(100, "AcDbDimStyleTable");
        self.out.int(71, 0);
        self.record("DIMSTYLE", h::DIMSTYLE_T, "AcDbDimStyleTableRecord");
        self.out.pair(2, "Standard");
        self.out.int(70, 0);
        self.out.pair(0, "ENDTAB");

        self.table_head("BLOCK_RECORD", h::BLOCK_RECORD_T, 2);
        for (handle, name, layout) in
            [(h::MODEL_BR, "*Model_Space", h::MODEL_LAYOUT), (h::PAPER_BR, "*Paper_Space", h::PAPER_LAYOUT)]
        {
            self.out.pair(0, "BLOCK_RECORD");
            self.out.pair(5, handle);
            self.out.pair(330, h::BLOCK_RECORD_T);
            self.out.pair(100, "AcDbSymbolTableRecord");
            self.out.pair(100, "AcDbBlockTableRecord");
            self.out.pair(2, name);
            self.out.pair(340, layout);
        }
        self.out.pair(0, "ENDTAB");
        self.out.pair(0, "ENDSEC");

        self.out.pair(0, "SECTION");
        self.out.pair(2, "BLOCKS");
        for (begin, end, owner, name, paper) in [
            (h::MODEL_BLOCK, h::MODEL_END, h::MODEL_BR, "*Model_Space", false),
            (h::PAPER_BLOCK, h::PAPER_END, h::PAPER_BR, "*Paper_Space", true),
        ] {
            self.out.pair(0, "BLOCK");
            self.out.pair(5, begin);
            self.out.pair(330, owner);
            self.out.pair(100, "AcDbEntity");
            if paper {
                self.out.int(67, 1);
            }
            self.out.pair(8, "0");
            self.out.pair(100, "AcDbBlockBegin");
            self.out.pair(2, name);
            self.out.int(70, 0);
            self.out.point(10, pt(0.0, 0.0));
            self.out.pair(3, name);
            self.out.pair(1, "");
            self.out.pair(0, "ENDBLK");
            self.out.pair(5, end);
            self.out.pair(330, owner);
            self.out.pair(100, "AcDbEntity");
            if paper {
                self.out.int(67, 1);
            }
            self.out.pair(8, "0");
            self.out.pair(100, "AcDbBlockEnd");
        }
        self.out.pair(0, "ENDSEC");
        std::mem::take(&mut self.out.buf)
    }

    fn objects(&mut self) -> Vec<u8> {
        let o = &mut self.out;
        o.pair(0, "SECTION");
        o.pair(2, "OBJECTS");
        o.pair(0, "DICTIONARY");
        o.pair(5, h::ROOT_DICT);
        o.pair(330, "0");
        o.pair(100, "AcDbDictionary");
        o.int(281, 1);
        o.pair(3, "ACAD_GROUP");
        o.pair(350, h::GROUP_DICT);
        o.pair(3, "ACAD_LAYOUT");
        o.pair(350, h::LAYOUT_DICT);
        for (handle, entries) in [
            (h::GROUP_DICT, Vec::new()),
            (h::LAYOUT_DICT, vec![("Layout1", h::PAPER_LAYOUT), ("Model", h::MODEL_LAYOUT)]),
        ] {
            o.pair(0, "DICTIONARY");
            o.pair(5, handle);
            o.pair(102, "{ACAD_REACTORS");
            o.pair(330, h::ROOT_DICT);
            o.pair(102, "}");
            o.pair(330, h::ROOT_DICT);
            o.pair(100, "AcDbDictionary");
            o.int(281, 1);
            for (name, target) in entries {
                o.pair(3, name);
                o.pair(350, target);
            }
        }
        for (handle, name, owner, flags, tab, model) in [
            (h::MODEL_LAYOUT, "Model", h::MODEL_BR, 1712, 0, true),
            (h::PAPER_LAYOUT, "Layout1", h::PAPER_BR, 688, 1, false),
        ] {
            o.pair(0, "LAYOUT");
            o.pair(5, handle);
            o.pair(102, "{ACAD_REACTORS");
            o.pair(330, h::LAYOUT_DICT);
            o.pair(102, "}");
            o.pair(330, h::LAYOUT_DICT);
            o.pair(100, "AcDbPlotSettings");
            o.pair(1, "");
            o.pair(2, "none_device");
            o.pair(4, "");
            o.pair(6, "");
            for code in [40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 140, 141] {
                o.real(code, 0.0);
            }
            o.real(142, 1.0);
            o.real(143, 1.0);
            o.int(70, flags);
            o.int(72, 1);
            o.int(73, 0);
            o.int(74, if model { 1 } else { 5 });
            o.pair(7, "");
            o.int(75, 16);
            o.real(147, 1.0);
            o.real(148, 0.0);
            o.real(149, 0.0);
            o.pair(100, "AcDbLayout");
            o.pair(1, name);
            o.int(70, 1);
            o.int(71, tab);
            for (code, v) in [(10, 0.0), (20, 0.0), (11, 420.0), (21, 297.0), (12, 0.0), (22, 0.0), (32, 0.0)]
            {
                o.real(code, v);
            }
            for (code, v) in [(14, 0.0), (24, 0.0), (34, 0.0), (15, 0.0), (25, 0.0), (35, 0.0), (146, 0.0)] {
                o.real(code, v);
            }
            for (code, v) in [(13, 0.0), (23, 0.0), (33, 0.0), (16, 1.0), (26, 0.0), (36, 0.0)] {
                o.real(code, v);
            }
            for (code, v) in [(17, 0.0), (27, 1.0), (37, 0.0)] {
                o.real(code, v);
            }
            o.int(76, 0);
            o.pair(330, owner);
        }
        o.pair(0, "ENDSEC");
        std::mem::take(&mut o.buf)
    }
}

/// Text content for a TEXT entity: `%%` sequences are control codes in DXF, so a literal `%%` is doubled to
/// `%%%%%%` (each `%%%` is one percent sign).
fn dxf_text_content(s: &str) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    if s.contains("%%") { s.replace('%', "%%%") } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::{Anchor, HatchPattern, weights};

    fn sample() -> Drawing {
        let mut d = Drawing::model(50.0);
        d.push(Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 0.25), pt(0.0, 0.25)]],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            Some(Stroke::solid(weights::EXTRA_THICK)),
            "wall.cut",
        ));
        d.push(Item::line(
            pt(0.0, 1.0),
            pt(4.0, 1.0),
            Stroke::patterned(weights::THIN, LinePattern::Dashed),
            "roof",
        ));
        d.push(Item::text(
            Text::new(pt(2.0, 1.5), "Κάτοψη 1:50", 3.5).anchored(Anchor::BottomCentre),
            "text",
        ));
        d
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("ASCII output")
    }

    #[test]
    fn file_structure() {
        let out = text(&write_dxf(&sample(), &DxfExportOptions::default()));
        assert!(out.starts_with("  0\r\nSECTION\r\n  2\r\nHEADER\r\n  9\r\n$ACADVER\r\n  1\r\nAC1015\r\n"));
        assert!(out.ends_with("  0\r\nEOF\r\n"));
        for section in ["HEADER", "CLASSES", "TABLES", "BLOCKS", "ENTITIES", "OBJECTS"] {
            assert!(out.contains(&format!("  2\r\n{section}\r\n")), "{section}");
        }
        for table in ["VPORT", "LTYPE", "LAYER", "STYLE", "VIEW", "UCS", "APPID", "DIMSTYLE", "BLOCK_RECORD"]
        {
            assert!(out.contains(&format!("  0\r\nTABLE\r\n  2\r\n{table}\r\n")), "{table}");
        }
        assert_eq!(out.matches("SECTION\r\n").count(), out.matches("ENDSEC\r\n").count());
        assert!(out.contains("$INSUNITS\r\n 70\r\n6\r\n"));
        // Text is ASCII with Greek as escapes by default.
        assert!(out.contains("\\U+039A\\U+03AC"));
    }

    #[test]
    fn handles_are_unique_and_below_the_seed() {
        let out = text(&write_dxf(&sample(), &DxfExportOptions::default()));
        let lines: Vec<&str> = out.split("\r\n").collect();
        let mut handles = std::collections::HashSet::new();
        let mut seed = 0;
        for w in lines.chunks_exact(2) {
            let code = w[0].trim();
            if code == "5" || code == "105" {
                let v = u64::from_str_radix(w[1], 16).unwrap();
                if seed == 0 {
                    // The first handle-coded value is the $HANDSEED in the header.
                    seed = v;
                    continue;
                }
                assert!(handles.insert(v), "duplicate handle {v:X}");
            }
        }
        assert!(handles.iter().all(|h| *h < seed), "handles below seed {seed:X}");
        assert!(handles.len() > 30);
    }

    #[test]
    fn layers_take_the_common_style_and_hatches_get_their_own_layer() {
        let out = text(&write_dxf(&sample(), &DxfExportOptions::default()));
        assert!(
            out.contains("  2\r\nwall.cut\r\n 70\r\n0\r\n 62\r\n7\r\n  6\r\nContinuous\r\n370\r\n70\r\n")
        );
        assert!(out.contains("  2\r\nwall.cut.hatch\r\n"));
        assert!(out.contains("  2\r\nroof\r\n 70\r\n0\r\n 62\r\n7\r\n  6\r\nKV_DASHED\r\n370\r\n25\r\n"));
        // At 1:50 in metres the dash is 3 mm × 0.05 = 0.15 m.
        assert!(out.contains("  2\r\nKV_DASHED\r\n"));
        assert!(out.contains(" 49\r\n0.15\r\n 74\r\n0\r\n 49\r\n-0.075\r\n"));
        // A solid background hatch.
        assert!(out.contains("  2\r\nSOLID\r\n 70\r\n1\r\n"));
    }

    #[test]
    fn units_scale_coordinates() {
        let opts = DxfExportOptions { units: Some(DxfUnits::Millimetres), ..DxfExportOptions::default() };
        let out = text(&write_dxf(&sample(), &opts));
        assert!(out.contains("$INSUNITS\r\n 70\r\n4\r\n"));
        assert!(out.contains(" 11\r\n4000.0\r\n 21\r\n1000.0\r\n"));
        // Text height 3.5 mm at 1:50 = 175 mm.
        assert!(out.contains(" 40\r\n175.0\r\n"));
    }

    #[test]
    fn greek_code_page_writes_single_bytes() {
        let opts = DxfExportOptions { codepage: Codepage::Ansi1253, ..DxfExportOptions::default() };
        let out = write_dxf(&sample(), &opts);
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("$DWGCODEPAGE\r\n  3\r\nANSI_1253"));
        // "Κά" is 0xCA 0xDC in Windows-1253.
        assert!(out.windows(2).any(|w| w == [0xCA, 0xDC]));
    }

    #[test]
    fn names_numbers_and_text_content() {
        assert_eq!(layer_name("walls/cut:1"), "walls_cut_1");
        assert_eq!(layer_name("  "), "0");
        assert_eq!(real(1.0), "1.0");
        assert_eq!(real(-0.0), "0.0");
        assert_eq!(real(4_202_000.123), "4202000.123");
        assert_eq!(real(f64::NAN), "0.0");
        assert_eq!(dxf_text_content("50%"), "50%");
        assert_eq!(dxf_text_content("a%%b"), "a%%%%%%b");
    }
}
