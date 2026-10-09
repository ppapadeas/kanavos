//! DXF import: entities to a drawing in metres, with blocks expanded and survey data collected.
//!
//! Supported entities: LINE, LWPOLYLINE (with bulges), POLYLINE/VERTEX (2D, 3D, polyface and polygon meshes),
//! CIRCLE, ARC, ELLIPSE, SPLINE (NURBS evaluation, or fit points), TEXT, MTEXT, ATTRIB, POINT, INSERT and MINSERT
//! (nested, with BYBLOCK properties and layer 0 inheritance), DIMENSION (its graphics block), SOLID, TRACE, 3DFACE,
//! HATCH (boundaries; solid hatches also filled) and LEADER. Other entities are counted and skipped.
//!
//! Coordinates in an entity's object coordinate system (OCS) are mapped to world coordinates with the arbitrary
//! axis algorithm of the DXF reference, so mirrored entities (extrusion 0, 0, −1) come out right.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::TAU;

use kanavos_drawing::geometry::arc_points;
use kanavos_drawing::{
    Affine, Anchor, Drawing, Fill, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt, weights,
};

use crate::DxfError;
use crate::aci::aci_to_rgb;
use crate::codepage::Codepage;
use crate::mtext::{decode_percent_codes, mtext_lines};
use crate::reader::{Decoder, Obj, Pair, group_entities, header, objects, sections, tokenize};
use crate::survey::{ClosedPolyline, DxfImport, DxfLayer, PointSource, SurveyPoint, extents};

/// Options for [`read_dxf`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOptions {
    /// Metres per drawing unit, overriding `$INSUNITS`.
    pub metres_per_unit: Option<f64>,
    /// Metres per drawing unit when the file declares no units (Greek surveys are drawn in metres).
    pub unitless_metres_per_unit: f64,
    /// Code page for files that do not name one, or name one that is not supported.
    pub fallback_codepage: Codepage,
    /// Scale denominator of the resulting drawing (sets text heights and line patterns in paper millimetres).
    pub scale: f64,
    /// Also import entities in paper space.
    pub include_paper_space: bool,
    /// Read elevations from texts such as `+125.34` (outside dimensions).
    pub text_elevations: bool,
    /// Largest distance between a point and an elevation text for them to be paired, metres. Three text heights
    /// are always allowed.
    pub elevation_search_radius: f64,
    /// Largest number of drawing items created; the rest of the file is skipped with a warning. Block expansion
    /// also stops after ten times as many entities have been processed.
    pub max_items: usize,
    /// Deepest block nesting expanded.
    pub max_block_depth: usize,
    /// Chord tolerance for arcs, ellipses and splines, paper millimetres at the import scale.
    pub curve_tolerance_mm: f64,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            metres_per_unit: None,
            unitless_metres_per_unit: 1.0,
            fallback_codepage: Codepage::Ansi1253,
            scale: 200.0,
            include_paper_space: false,
            text_elevations: true,
            elevation_search_radius: 2.0,
            max_items: 2_000_000,
            max_block_depth: 16,
            curve_tolerance_mm: 0.05,
        }
    }
}

/// Metres per unit for an `$INSUNITS` value.
pub fn insunits_to_metres(code: i64) -> Option<f64> {
    Some(match code {
        1 => 0.0254,
        2 => 0.3048,
        3 => 1609.344,
        4 => 0.001,
        5 => 0.01,
        6 => 1.0,
        7 => 1000.0,
        8 => 2.54e-8,
        9 => 2.54e-5,
        10 => 0.9144,
        11 => 1e-10,
        12 => 1e-9,
        13 => 1e-6,
        14 => 0.1,
        15 => 10.0,
        16 => 100.0,
        17 => 1e9,
        _ => return None,
    })
}

/// Parses an elevation text such as `+125.34`, `125,34`, `Η=125.34`, `Z: 125.340 m`. A decimal part of one to
/// three digits is required (plain integers are usually point numbers), and the value must lie between −500 and
/// 3000 m.
pub fn parse_elevation(s: &str) -> Option<f64> {
    let mut t = s.trim();
    for prefix in ["Η", "H", "Z", "Ζ", "h", "z", "ΥΨ", "Υψ", "Υ"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            let rest = rest.trim_start();
            if let Some(r) = rest.strip_prefix(['=', ':']) {
                t = r.trim_start();
                break;
            }
        }
    }
    for suffix in ["m", "μ", "M"] {
        t = t.strip_suffix(suffix).map(str::trim_end).unwrap_or(t);
    }
    let t = t.strip_prefix('+').unwrap_or(t);
    let (neg, body) = match t.strip_prefix(['-', '−']) {
        Some(b) => (true, b),
        None => (false, t),
    };
    let sep = body.find(['.', ','])?;
    let (int, frac) = (body.get(..sep)?, body.get(sep + 1..)?);
    if int.is_empty() || !int.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !(1..=3).contains(&frac.len()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: f64 = format!("{int}.{frac}").parse().ok()?;
    let v = if neg { -v } else { v };
    (-500.0..=3000.0).contains(&v).then_some(v)
}

/// Point-number-like labels: short alphanumeric strings without spaces.
fn looks_like_label(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.chars().count() <= 12
        && s.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'))
}

const ELEVATION_TAGS: &[&str] =
    &["Z", "H", "ELEV", "ELEVATION", "HEIGHT", "ALT", "ΥΨ", "ΥΨΟΜΕΤΡΟ", "ΥΨΟΣ", "Η"];
const LABEL_TAGS: &[&str] =
    &["NO", "NR", "ID", "PT", "POINT", "NUM", "NUMBER", "NAME", "ΑΡ", "Α/Α", "ΑΑ", "ΣΗΜΕΙΟ"];

#[derive(Debug, Clone)]
struct LayerDef {
    name: String,
    color: Rgba,
    on: bool,
    frozen: bool,
    linetype: String,
    weight: Option<f64>,
}

struct Block<'a> {
    base: [f64; 3],
    ents: Vec<Obj<'a>>,
    xref: bool,
}

/// Inherited state while expanding blocks.
#[derive(Debug, Clone)]
struct Ctx {
    /// Entity coordinates (drawing units, block space) → world metres.
    xf: Affine,
    depth: usize,
    /// Layer that replaces layer "0" inside a block (the insert's layer).
    layer0: Option<String>,
    /// BYBLOCK colour, line type and weight.
    color: Rgba,
    linetype: LinePattern,
    weight: f64,
    in_dimension: bool,
    /// World Z = z_offset + z_scale · entity Z.
    z_offset: f64,
    z_scale: f64,
}

/// An elevation text waiting to be paired with a point.
struct ElevText {
    at: Pt,
    z: f64,
    radius: f64,
    layer: String,
}

struct Importer<'a> {
    opts: &'a ImportOptions,
    dec: Decoder,
    layers: HashMap<String, LayerDef>,
    linetypes: HashMap<String, Vec<f64>>,
    ltscale: f64,
    blocks: HashMap<String, Block<'a>>,
    mpu: f64,
    /// Paper millimetres per world metre.
    mm_per_m: f64,
    drawing: Drawing,
    points: Vec<SurveyPoint>,
    elev_texts: Vec<ElevText>,
    closed: Vec<ClosedPolyline>,
    warnings: Vec<String>,
    counts: BTreeMap<String, usize>,
    item_counts: HashMap<String, usize>,
    full: bool,
    /// Entities processed, including every block expansion.
    visits: usize,
}

/// Reads an ASCII DXF file (R12 to current versions).
pub fn read_dxf(data: &[u8], opts: &ImportOptions) -> Result<DxfImport, DxfError> {
    let pairs = tokenize(data)?;
    let secs = sections(&pairs);
    if secs.is_empty() {
        return Err(DxfError::Malformed { line: 1, reason: "no SECTION found; not a DXF file".into() });
    }
    let empty: Vec<Pair<'_>> = Vec::new();
    let vars = secs.iter().find(|s| s.name == "HEADER").map(|s| header(s.pairs)).unwrap_or_default();
    let var = |name: &str| vars.get(name).unwrap_or(&empty);
    let version = var("$ACADVER").first().map(|p| p.keyword()).unwrap_or_else(|| "AC1009".into());
    let utf8 = version.strip_prefix("AC").and_then(|v| v.parse::<u32>().ok()).is_some_and(|v| v >= 1021);
    let named_cp = var("$DWGCODEPAGE").first().and_then(|p| Codepage::from_dxf_name(&p.keyword()));
    let codepage = if utf8 { Codepage::Utf8 } else { named_cp.unwrap_or(opts.fallback_codepage) };
    let mut warnings = Vec::new();
    if !utf8
        && named_cp.is_none()
        && let Some(p) = var("$DWGCODEPAGE").first()
    {
        warnings.push(format!(
            "code page {} is not supported; text read as {}",
            p.keyword(),
            opts.fallback_codepage.dxf_name()
        ));
    }
    let insunits = var("$INSUNITS").first().and_then(Pair::i64).unwrap_or(0);
    let mpu = opts
        .metres_per_unit
        .filter(|v| v.is_finite() && *v > 0.0)
        .or_else(|| insunits_to_metres(insunits))
        .unwrap_or(opts.unitless_metres_per_unit);
    let ltscale = var("$LTSCALE").first().and_then(Pair::f64).filter(|v| *v > 0.0).unwrap_or(1.0);
    let scale = if opts.scale.is_finite() && opts.scale > 0.0 { opts.scale } else { 200.0 };

    let mut imp = Importer {
        opts,
        dec: Decoder { utf8, codepage },
        layers: HashMap::new(),
        linetypes: HashMap::new(),
        ltscale,
        blocks: HashMap::new(),
        mpu,
        mm_per_m: 1000.0 / scale,
        drawing: Drawing::model(scale),
        points: Vec::new(),
        elev_texts: Vec::new(),
        closed: Vec::new(),
        warnings,
        counts: BTreeMap::new(),
        item_counts: HashMap::new(),
        full: false,
        visits: 0,
    };
    if let Some(s) = secs.iter().find(|s| s.name == "TABLES") {
        imp.tables(s.pairs);
    }
    if let Some(s) = secs.iter().find(|s| s.name == "BLOCKS") {
        imp.blocks(s.pairs);
    }
    let root = Ctx {
        xf: Affine::scale_translate(mpu, 0.0, 0.0),
        depth: 0,
        layer0: None,
        color: Rgba::BLACK,
        linetype: LinePattern::Solid,
        weight: weights::FINE,
        in_dimension: false,
        z_offset: 0.0,
        z_scale: mpu,
    };
    if let Some(s) = secs.iter().find(|s| s.name == "ENTITIES") {
        for e in group_entities(objects(s.pairs)) {
            imp.entity(&e, &root);
        }
    }
    if opts.include_paper_space
        && let Some(b) = imp.blocks.get("*PAPER_SPACE")
    {
        let ents = b.ents.clone();
        for e in &ents {
            imp.entity(e, &root);
        }
    }
    imp.pair_elevation_texts();
    let extents = extents(&imp.drawing, &imp.points);
    let mut layers: Vec<DxfLayer> = imp
        .layers
        .values()
        .map(|l| DxfLayer {
            name: l.name.clone(),
            color: l.color,
            visible: l.on && !l.frozen,
            frozen: l.frozen,
            items: imp.item_counts.get(&l.name).copied().unwrap_or(0),
        })
        .collect();
    for (name, n) in &imp.item_counts {
        if !layers.iter().any(|l| &l.name == name) {
            layers.push(DxfLayer {
                name: name.clone(),
                color: Rgba::BLACK,
                visible: true,
                frozen: false,
                items: *n,
            });
        }
    }
    layers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(DxfImport {
        drawing: imp.drawing,
        layers,
        points: imp.points,
        closed_polylines: imp.closed,
        extents,
        version,
        codepage,
        metres_per_unit: mpu,
        warnings: imp.warnings,
        entity_counts: imp.counts,
    })
}

/// Unit X and Y axes of an object coordinate system with normal `n` (arbitrary axis algorithm).
fn ocs_axes(n: [f64; 3]) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !(len.is_finite() && len > 1e-12) {
        return None;
    }
    let n = [n[0] / len, n[1] / len, n[2] / len];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    };
    let norm = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        norm(cross([0.0, 1.0, 0.0], n))
    } else {
        norm(cross([0.0, 0.0, 1.0], n))
    };
    let ay = norm(cross(n, ax));
    Some((ax, ay, n))
}

/// The plan projection of an OCS: (x, y) at OCS height `z` → world (x, y).
fn ocs_affine(n: [f64; 3], z: f64) -> Affine {
    let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
    if close(n[0], 0.0) && close(n[1], 0.0) && n[2] > 0.0 {
        return Affine::IDENTITY;
    }
    match ocs_axes(n) {
        Some((ax, ay, nn)) => Affine { a: ax[0], b: ay[0], c: ax[1], d: ay[1], tx: z * nn[0], ty: z * nn[1] },
        None => Affine::IDENTITY,
    }
}

fn is_similarity(m: &Affine) -> bool {
    let s2 = m.a * m.a + m.c * m.c;
    let t2 = m.b * m.b + m.d * m.d;
    let dot = m.a * m.b + m.c * m.d;
    s2 > 0.0 && (s2 - t2).abs() <= 1e-9 * s2 && dot.abs() <= 1e-9 * s2
}

/// Points of a bulge arc from `a` to `b` (both ends included), or just the ends for a straight segment.
fn bulge_points(a: Pt, b: Pt, bulge: f64, tol: f64) -> Vec<Pt> {
    let c = a.distance(b);
    if !(bulge.is_finite() && bulge.abs() > 1e-9 && c > 1e-12) {
        return vec![a, b];
    }
    let sweep = 4.0 * bulge.atan();
    let r = c / (2.0 * (sweep / 2.0).sin()).abs();
    let mid = a.midpoint(b);
    let (ux, uy) = ((b.x - a.x) / c, (b.y - a.y) / c);
    let off = c / 4.0 * (1.0 / bulge - bulge);
    let centre = pt(mid.x - uy * off, mid.y + ux * off);
    let start = (a.y - centre.y).atan2(a.x - centre.x);
    let mut pts = arc_points(centre, r, start, sweep, tol);
    // Snap the ends to the exact vertices.
    if let Some(f) = pts.first_mut() {
        *f = a;
    }
    if let Some(l) = pts.last_mut() {
        *l = b;
    }
    pts
}

/// Points of a NURBS curve by de Boor's algorithm, `samples` evenly spaced in the parameter domain.
fn nurbs_points(
    degree: usize,
    knots: &[f64],
    ctrl: &[[f64; 3]],
    weights: &[f64],
    samples: usize,
) -> Option<Vec<Pt>> {
    let n = ctrl.len();
    if degree == 0 || n <= degree || knots.len() != n + degree + 1 {
        return None;
    }
    let (t0, t1) = (*knots.get(degree)?, *knots.get(n)?);
    // Also rejects NaN knots.
    if t1.partial_cmp(&t0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let w = |i: usize| weights.get(i).copied().filter(|w| *w > 0.0).unwrap_or(1.0);
    let mut out = Vec::with_capacity(samples + 1);
    for s in 0..=samples {
        let t = t0 + (t1 - t0) * s as f64 / samples as f64;
        // Knot span k with knots[k] <= t < knots[k + 1], clamped to the last non-empty span.
        let mut k = degree;
        while k + 1 < n && knots.get(k + 1).is_some_and(|kn| *kn <= t) {
            k += 1;
        }
        let mut d: Vec<[f64; 3]> = (0..=degree)
            .map(|j| {
                let i = k + j - degree;
                let p = ctrl.get(i).copied().unwrap_or([0.0; 3]);
                let wi = w(i);
                [p[0] * wi, p[1] * wi, wi]
            })
            .collect();
        for r in 1..=degree {
            for j in (r..=degree).rev() {
                let i = k + j - degree;
                let (ki, kd) = (knots.get(i).copied()?, knots.get(i + degree + 1 - r).copied()?);
                let alpha = if kd - ki > 0.0 { (t - ki) / (kd - ki) } else { 0.0 };
                let (prev, cur) = (*d.get(j - 1)?, *d.get(j)?);
                if let Some(slot) = d.get_mut(j) {
                    *slot = [
                        (1.0 - alpha) * prev[0] + alpha * cur[0],
                        (1.0 - alpha) * prev[1] + alpha * cur[1],
                        (1.0 - alpha) * prev[2] + alpha * cur[2],
                    ];
                }
            }
        }
        let p = *d.get(degree)?;
        if p[2].abs() < 1e-300 {
            return None;
        }
        out.push(pt(p[0] / p[2], p[1] / p[2]));
    }
    Some(out)
}

impl<'a> Importer<'a> {
    fn warn(&mut self, msg: String) {
        if self.warnings.len() < 200 && !self.warnings.contains(&msg) {
            self.warnings.push(msg);
        }
    }

    fn tables(&mut self, pairs: &'a [Pair<'a>]) {
        for o in objects(pairs) {
            match o.kind.as_str() {
                "LAYER" => {
                    let Some(name) = o.s(2, &self.dec).map(|s| s.trim().to_owned()) else { continue };
                    let aci = o.i(62).unwrap_or(7);
                    let flags = o.i(70).unwrap_or(0);
                    let color = match o.i(420) {
                        Some(tc) => true_color(tc),
                        None => aci_to_rgb(aci.unsigned_abs().min(255) as i32),
                    };
                    let weight = o.i(370).filter(|w| *w >= 0).map(|w| w as f64 / 100.0);
                    let def = LayerDef {
                        name: name.clone(),
                        color,
                        on: aci >= 0,
                        frozen: flags & 1 != 0,
                        linetype: o
                            .s(6, &self.dec)
                            .unwrap_or_else(|| "CONTINUOUS".into())
                            .trim()
                            .to_uppercase(),
                        weight,
                    };
                    self.layers.insert(name.to_uppercase(), def);
                }
                "LTYPE" => {
                    let Some(name) = o.s(2, &self.dec) else { continue };
                    let elements: Vec<f64> =
                        o.pairs.iter().filter(|p| p.code == 49).filter_map(Pair::f64).collect();
                    self.linetypes.insert(name.trim().to_uppercase(), elements);
                }
                _ => {}
            }
        }
    }

    fn blocks(&mut self, pairs: &'a [Pair<'a>]) {
        let objs = group_entities(objects(pairs));
        let mut current: Option<(String, Block<'a>)> = None;
        for o in objs {
            match o.kind.as_str() {
                "BLOCK" => {
                    let name = o.s(2, &self.dec).unwrap_or_default().trim().to_uppercase();
                    let base = o.point(10).unwrap_or([0.0; 3]);
                    let xref = o.i(70).unwrap_or(0) & 4 != 0;
                    current = Some((name, Block { base, ents: Vec::new(), xref }));
                }
                "ENDBLK" => {
                    if let Some((name, b)) = current.take() {
                        self.blocks.insert(name, b);
                    }
                }
                _ => {
                    if let Some((_, b)) = current.as_mut() {
                        b.ents.push(o);
                    }
                }
            }
        }
    }

    fn layer_def(&self, name: &str) -> Option<&LayerDef> {
        self.layers.get(&name.to_uppercase())
    }

    /// The layer an entity is drawn on: its own (spelled as in the layer table), or the insert's for layer 0
    /// inside a block.
    fn effective_layer(&self, e: &Obj<'_>, ctx: &Ctx) -> String {
        let name = e
            .s(8, &self.dec)
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "0".into());
        match (&ctx.layer0, name.as_str()) {
            (Some(l), "0") => l.clone(),
            _ => self.layer_def(&name).map(|l| l.name.clone()).unwrap_or(name),
        }
    }

    fn color(&self, e: &Obj<'_>, layer: &str, ctx: &Ctx) -> Rgba {
        if let Some(tc) = e.i(420) {
            return true_color(tc);
        }
        match e.i(62) {
            Some(0) => ctx.color,
            Some(256) | None => self.layer_def(layer).map(|l| l.color).unwrap_or(Rgba::BLACK),
            Some(n) => aci_to_rgb(n.unsigned_abs().min(255) as i32),
        }
    }

    fn pattern(&self, name: &str, ltscale: f64) -> LinePattern {
        let Some(elements) = self.linetypes.get(name) else { return LinePattern::Solid };
        let mut out: Vec<f64> = Vec::new();
        for v in elements {
            let on = *v >= 0.0;
            let len = v.abs() * self.ltscale * ltscale * self.mpu * self.mm_per_m;
            match out.len() {
                0 => {
                    if !on {
                        out.push(0.0);
                    }
                    out.push(len);
                }
                n => {
                    let last_on = n % 2 == 1;
                    if on == last_on {
                        if let Some(l) = out.last_mut() {
                            *l += len;
                        }
                    } else {
                        out.push(len);
                    }
                }
            }
        }
        let total: f64 = out.iter().sum();
        if out.len() < 2 || !(total.is_finite() && total > 0.05) {
            LinePattern::Solid
        } else {
            LinePattern::Custom(out)
        }
    }

    fn stroke(&self, e: &Obj<'_>, layer: &str, ctx: &Ctx) -> Stroke {
        let color = self.color(e, layer, ctx);
        let ldef = self.layer_def(layer);
        let weight = match e.i(370) {
            Some(w) if w >= 0 => w as f64 / 100.0,
            Some(-2) => ctx.weight,
            _ => ldef.and_then(|l| l.weight).unwrap_or(weights::FINE),
        };
        let lt_name = e.s(6, &self.dec).map(|s| s.trim().to_uppercase()).unwrap_or_else(|| "BYLAYER".into());
        let ltscale = e.f(48).filter(|v| *v > 0.0).unwrap_or(1.0);
        let pattern = match lt_name.as_str() {
            "BYBLOCK" => ctx.linetype.clone(),
            "BYLAYER" => match ldef.map(|l| l.linetype.as_str()) {
                Some(n) if n != "CONTINUOUS" => self.pattern(n, ltscale),
                _ => LinePattern::Solid,
            },
            "CONTINUOUS" => LinePattern::Solid,
            n => self.pattern(n, ltscale),
        };
        Stroke { weight_mm: weight.max(0.0), color, pattern }
    }

    /// Curve tolerance in entity units for a transform.
    fn tol(&self, xf: &Affine) -> f64 {
        let world = self.opts.curve_tolerance_mm / self.mm_per_m;
        world / xf.scale_factor().max(1e-300)
    }

    fn push(&mut self, item: Item) {
        if self.full {
            return;
        }
        if self.drawing.items.len() >= self.opts.max_items {
            self.full = true;
            self.warn(format!("more than {} items: the rest of the file was skipped", self.opts.max_items));
            return;
        }
        *self.item_counts.entry(item.layer.clone()).or_default() += 1;
        self.drawing.push(item);
    }

    /// Adds a shape given in entity coordinates, mapped to world coordinates by `xf`.
    fn emit(&mut self, shape: Shape, stroke: Option<Stroke>, fill: Option<Fill>, layer: &str, xf: &Affine) {
        if let Shape::Text(t) = &shape {
            // Text keeps reading left to right: its direction vector is mapped, never its glyphs.
            let mut t = t.clone();
            let (s, c) = t.rotation.sin_cos();
            let (dx, dy) = (xf.a * c + xf.b * s, xf.c * c + xf.d * s);
            t.position = xf.apply(t.position);
            t.rotation = dy.atan2(dx);
            self.push(Item { shape: Shape::Text(t), stroke, fill, layer: layer.to_owned(), source: None });
            return;
        }
        let item = Item { shape, stroke, fill, layer: layer.to_owned(), source: None };
        if is_similarity(xf) {
            let one = Drawing { units: self.drawing.units, scale: self.drawing.scale, items: vec![item] };
            if let Some(it) = one.transformed(xf).items.into_iter().next() {
                self.push(it);
            }
            return;
        }
        // Non-uniform scale: curves become polylines.
        let tol = self.tol(xf);
        let map = |p: &Pt| xf.apply(*p);
        let shape = match &item.shape {
            Shape::Arc { centre, radius, start, sweep } => Shape::Polyline {
                points: arc_points(*centre, *radius, *start, *sweep, tol).iter().map(map).collect(),
                closed: false,
            },
            Shape::Circle { centre, radius } => {
                let mut pts = arc_points(*centre, *radius, 0.0, TAU, tol);
                pts.pop();
                Shape::Polyline { points: pts.iter().map(map).collect(), closed: true }
            }
            Shape::Line { a, b } => Shape::Line { a: map(a), b: map(b) },
            Shape::Polyline { points, closed } => {
                Shape::Polyline { points: points.iter().map(map).collect(), closed: *closed }
            }
            Shape::Polygon { rings } => {
                Shape::Polygon { rings: rings.iter().map(|r| r.iter().map(map).collect()).collect() }
            }
            Shape::Text(t) => Shape::Text(t.clone()),
        };
        self.push(Item { shape, ..item });
    }

    fn count(&mut self, kind: &str) {
        *self.counts.entry(kind.to_owned()).or_default() += 1;
    }

    fn entity(&mut self, e: &Obj<'a>, ctx: &Ctx) {
        if self.full {
            return;
        }
        // Nested blocks can multiply without drawing anything (a block of ten inserts of a block of ten inserts…):
        // bound the work, not only the output.
        self.visits += 1;
        if self.visits > self.opts.max_items.saturating_mul(10) {
            self.full = true;
            self.warn("block expansion too large: the rest of the file was skipped".to_owned());
            return;
        }
        self.count(&e.kind);
        if e.i(67) == Some(1) && ctx.depth == 0 && !self.opts.include_paper_space {
            return;
        }
        if e.i(60) == Some(1) {
            return;
        }
        let layer = self.effective_layer(e, ctx);
        match e.kind.as_str() {
            "LINE" => {
                let (Some(a), Some(b)) = (e.point(10), e.point(11)) else { return self.bad(e) };
                let st = self.stroke(e, &layer, ctx);
                self.emit(
                    Shape::Line { a: pt(a[0], a[1]), b: pt(b[0], b[1]) },
                    Some(st),
                    None,
                    &layer,
                    &ctx.xf,
                );
            }
            "POINT" => self.point(e, &layer, ctx),
            "CIRCLE" | "ARC" => {
                let (Some(c), Some(r)) = (e.point(10), e.f(40)) else { return self.bad(e) };
                let xf = ctx.xf.then(&ocs_affine(e.extrusion(), c[2]));
                let st = self.stroke(e, &layer, ctx);
                let shape = if e.kind == "CIRCLE" {
                    Shape::Circle { centre: pt(c[0], c[1]), radius: r }
                } else {
                    let a0 = e.f_or(50, 0.0);
                    let mut sweep = (e.f_or(51, 360.0) - a0).rem_euclid(360.0);
                    if sweep < 1e-12 {
                        sweep = 360.0;
                    }
                    Shape::Arc {
                        centre: pt(c[0], c[1]),
                        radius: r,
                        start: a0.to_radians(),
                        sweep: sweep.to_radians(),
                    }
                };
                self.emit(shape, Some(st), None, &layer, &xf);
            }
            "LWPOLYLINE" => self.lwpolyline(e, &layer, ctx),
            "POLYLINE" => self.polyline(e, &layer, ctx),
            "ELLIPSE" => self.ellipse(e, &layer, ctx),
            "SPLINE" => self.spline(e, &layer, ctx),
            "TEXT" | "ATTRIB" => self.text(e, &layer, ctx),
            "MTEXT" => self.mtext(e, &layer, ctx),
            "INSERT" | "MINSERT" => self.insert(e, &layer, ctx),
            "DIMENSION" => {
                let name = e.s(2, &self.dec).unwrap_or_default().trim().to_uppercase();
                let sub = Ctx {
                    layer0: Some(layer.clone()),
                    color: self.color(e, &layer, ctx),
                    in_dimension: true,
                    depth: ctx.depth + 1,
                    ..ctx.clone()
                };
                self.expand(&name, &sub);
            }
            "SOLID" | "TRACE" | "3DFACE" => {
                let pts: Vec<Pt> =
                    [10, 11, 12, 13].iter().filter_map(|c| e.point(*c)).map(|p| pt(p[0], p[1])).collect();
                if pts.len() < 3 {
                    return self.bad(e);
                }
                let mut ring: Vec<Pt> = if e.kind == "3DFACE" {
                    pts.clone()
                } else {
                    // SOLID and TRACE list their corners in the order 1, 2, 4, 3.
                    [0usize, 1, 3, 2].iter().filter_map(|i| pts.get(*i).copied()).collect()
                };
                ring.dedup_by(|a, b| a.distance(*b) < 1e-12);
                let elev = e.point(10).map(|p| p[2]).unwrap_or(0.0);
                let xf =
                    if e.kind == "3DFACE" { ctx.xf } else { ctx.xf.then(&ocs_affine(e.extrusion(), elev)) };
                let st = self.stroke(e, &layer, ctx);
                let fill = (e.kind != "3DFACE").then_some(Fill::Solid(st.color));
                let shape = if fill.is_some() {
                    Shape::Polygon { rings: vec![ring] }
                } else {
                    Shape::Polyline { points: ring, closed: true }
                };
                self.emit(shape, Some(st), fill, &layer, &xf);
            }
            "HATCH" => self.hatch(e, &layer, ctx),
            "LEADER" => {
                let pts = repeated_points(e.pairs, 10);
                if pts.len() >= 2 {
                    let st = self.stroke(e, &layer, ctx);
                    self.emit(
                        Shape::Polyline { points: pts, closed: false },
                        Some(st),
                        None,
                        &layer,
                        &ctx.xf,
                    );
                }
            }
            "ATTDEF" | "SEQEND" | "VERTEX" | "ENDBLK" | "BLOCK" => {}
            other => {
                let msg = format!("{other} entities are not imported");
                self.warn(msg);
            }
        }
    }

    fn bad(&mut self, e: &Obj<'_>) {
        let msg = format!("line {}: {} entity without the data it needs, skipped", e.line, e.kind);
        self.warn(msg);
    }

    fn point(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let Some(p) = e.point(10) else { return self.bad(e) };
        let w = ctx.xf.apply(pt(p[0], p[1]));
        let z = ctx.z_offset + ctx.z_scale * p[2];
        self.points.push(SurveyPoint {
            position: [w.x, w.y, z],
            has_elevation: z.abs() > 1e-9,
            source: PointSource::Point,
            label: None,
            layer: layer.to_owned(),
        });
        // A small circle so the point shows in the underlay.
        let st = self.stroke(e, layer, ctx);
        let r = 0.4 / self.mm_per_m;
        self.emit(Shape::Circle { centre: w, radius: r }, Some(st), None, layer, &Affine::IDENTITY);
    }

    fn record_closed(&mut self, layer: &str, world: &[Pt], ctx: &Ctx) {
        if ctx.depth == 0 && world.len() >= 3 {
            self.closed.push(ClosedPolyline::new(layer, world.iter().map(|p| [p.x, p.y]).collect()));
        }
    }

    fn lwpolyline(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let mut verts: Vec<(Pt, f64)> = Vec::new();
        for p in e.pairs {
            match p.code {
                10 => verts.push((pt(p.f64().unwrap_or(0.0), 0.0), 0.0)),
                20 => {
                    if let Some(v) = verts.last_mut() {
                        v.0.y = p.f64().unwrap_or(0.0);
                    }
                }
                42 => {
                    if let Some(v) = verts.last_mut() {
                        v.1 = p.f64().unwrap_or(0.0);
                    }
                }
                _ => {}
            }
        }
        let closed = e.i(70).unwrap_or(0) & 1 != 0;
        let xf = ctx.xf.then(&ocs_affine(e.extrusion(), e.f_or(38, 0.0)));
        self.bulged(e, verts, closed, layer, ctx, &xf);
    }

    /// A polyline with bulges, flattened, drawn, and recorded when closed.
    fn bulged(
        &mut self,
        e: &Obj<'_>,
        verts: Vec<(Pt, f64)>,
        closed: bool,
        layer: &str,
        ctx: &Ctx,
        xf: &Affine,
    ) {
        if verts.len() < 2 {
            return;
        }
        let tol = self.tol(xf);
        let n = verts.len();
        let segs = if closed { n } else { n - 1 };
        let mut pts: Vec<Pt> = Vec::with_capacity(n);
        for i in 0..segs {
            let (Some((a, b)), Some((c, _))) = (verts.get(i), verts.get((i + 1) % n)) else { continue };
            let arc = bulge_points(*a, *c, *b, tol);
            if pts.is_empty() {
                pts.extend(arc);
            } else {
                pts.extend(arc.into_iter().skip(1));
            }
        }
        // A closed polyline repeats its first point at the end of the last segment.
        let explicit_close =
            !closed && n > 2 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9);
        if closed || explicit_close {
            pts.pop();
        }
        let is_closed = closed || explicit_close;
        let st = self.stroke(e, layer, ctx);
        let world: Vec<Pt> = pts.iter().map(|p| xf.apply(*p)).collect();
        if is_closed {
            self.record_closed(layer, &world, ctx);
        }
        self.emit(Shape::Polyline { points: pts, closed: is_closed }, Some(st), None, layer, xf);
    }

    fn polyline(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let flags = e.i(70).unwrap_or(0);
        let st = self.stroke(e, layer, ctx);
        let vertex = |v: &Obj<'_>| v.point(10).map(|p| (p, v.f_or(42, 0.0), v.i(70).unwrap_or(0)));
        let verts: Vec<([f64; 3], f64, i64)> = e.children.iter().filter_map(vertex).collect();
        if flags & 64 != 0 {
            // Polyface mesh: coordinate vertices, then face records with 1-based indices.
            let coords: Vec<Pt> =
                verts.iter().filter(|(_, _, f)| f & 64 != 0).map(|(p, _, _)| pt(p[0], p[1])).collect();
            for face in e.children.iter().filter(|v| v.i(70).unwrap_or(0) & 192 == 128) {
                let ring: Vec<Pt> = [71, 72, 73, 74]
                    .iter()
                    .filter_map(|c| face.i(*c))
                    .filter(|i| *i != 0)
                    .filter_map(|i| coords.get(i.unsigned_abs() as usize - 1).copied())
                    .collect();
                if ring.len() >= 2 {
                    self.emit(
                        Shape::Polyline { points: ring, closed: true },
                        Some(st.clone()),
                        None,
                        layer,
                        &ctx.xf,
                    );
                }
            }
            return;
        }
        if flags & 16 != 0 {
            // Polygon mesh: M × N vertices, drawn as grid lines.
            let (m, n) = (e.i(71).unwrap_or(0).max(0) as usize, e.i(72).unwrap_or(0).max(0) as usize);
            let pts: Vec<Pt> = verts.iter().map(|(p, _, _)| pt(p[0], p[1])).collect();
            if m.checked_mul(n) == Some(pts.len()) && m > 0 && n > 0 {
                for i in 0..m {
                    let row: Vec<Pt> = (0..n).filter_map(|j| pts.get(i * n + j).copied()).collect();
                    self.emit(
                        Shape::Polyline { points: row, closed: flags & 32 != 0 },
                        Some(st.clone()),
                        None,
                        layer,
                        &ctx.xf,
                    );
                }
                for j in 0..n {
                    let col: Vec<Pt> = (0..m).filter_map(|i| pts.get(i * n + j).copied()).collect();
                    self.emit(
                        Shape::Polyline { points: col, closed: flags & 1 != 0 },
                        Some(st.clone()),
                        None,
                        layer,
                        &ctx.xf,
                    );
                }
            }
            return;
        }
        // Spline frame control points are not part of the curve.
        let verts: Vec<&([f64; 3], f64, i64)> = verts.iter().filter(|(_, _, f)| f & 16 == 0).collect();
        let closed = flags & 1 != 0;
        if flags & 8 != 0 {
            // 3D polyline: world coordinates.
            let pts: Vec<(Pt, f64)> = verts.iter().map(|(p, _, _)| (pt(p[0], p[1]), 0.0)).collect();
            self.bulged(e, pts, closed, layer, ctx, &ctx.xf.clone());
        } else {
            let elev = e.point(10).map(|p| p[2]).unwrap_or(0.0);
            let xf = ctx.xf.then(&ocs_affine(e.extrusion(), elev));
            let pts: Vec<(Pt, f64)> = verts.iter().map(|(p, b, _)| (pt(p[0], p[1]), *b)).collect();
            self.bulged(e, pts, closed, layer, ctx, &xf);
        }
    }

    fn ellipse(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let (Some(c), Some(maj)) = (e.point(10), e.point(11)) else { return self.bad(e) };
        let ratio = e.f_or(40, 1.0);
        let n = e.extrusion();
        let minor = [
            ratio * (n[1] * maj[2] - n[2] * maj[1]),
            ratio * (n[2] * maj[0] - n[0] * maj[2]),
            ratio * (n[0] * maj[1] - n[1] * maj[0]),
        ];
        let (t0, mut t1) = (e.f_or(41, 0.0), e.f_or(42, TAU));
        if t1 <= t0 {
            t1 += TAU;
        }
        let full = (t1 - t0 - TAU).abs() < 1e-9;
        let r = maj[0].hypot(maj[1]).max(1e-12);
        let tol = self.tol(&ctx.xf).min(r);
        let step = 2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos();
        let steps = if step > 0.0 { ((t1 - t0) / step).ceil().clamp(8.0, 4096.0) as usize } else { 64 };
        let mut pts: Vec<Pt> = (0..=steps)
            .map(|i| {
                let t = t0 + (t1 - t0) * i as f64 / steps as f64;
                pt(c[0] + t.cos() * maj[0] + t.sin() * minor[0], c[1] + t.cos() * maj[1] + t.sin() * minor[1])
            })
            .collect();
        if full {
            pts.pop();
        }
        let st = self.stroke(e, layer, ctx);
        self.emit(Shape::Polyline { points: pts, closed: full }, Some(st), None, layer, &ctx.xf);
    }

    fn spline(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let degree = e.i(71).unwrap_or(3).clamp(1, 25) as usize;
        let knots: Vec<f64> = e.pairs.iter().filter(|p| p.code == 40).filter_map(Pair::f64).collect();
        let weights: Vec<f64> = e.pairs.iter().filter(|p| p.code == 41).filter_map(Pair::f64).collect();
        let ctrl: Vec<[f64; 3]> = repeated_points(e.pairs, 10).iter().map(|p| [p.x, p.y, 0.0]).collect();
        let fit = repeated_points(e.pairs, 11);
        let closed = e.i(70).unwrap_or(0) & 1 != 0;
        let samples = (ctrl.len() * 12).clamp(16, 4000);
        let pts = nurbs_points(degree, &knots, &ctrl, &weights, samples)
            .or_else(|| (fit.len() >= 2).then(|| fit.clone()))
            .or_else(|| (ctrl.len() >= 2).then(|| ctrl.iter().map(|p| pt(p[0], p[1])).collect()));
        let Some(mut pts) = pts else { return self.bad(e) };
        if closed && pts.len() > 2 && pts.first().zip(pts.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-9)
        {
            pts.pop();
        }
        let st = self.stroke(e, layer, ctx);
        self.emit(Shape::Polyline { points: pts, closed }, Some(st), None, layer, &ctx.xf);
    }

    fn text_item(&mut self, t: Text, e: &Obj<'_>, layer: &str, ctx: &Ctx, xf: &Affine, height_units: f64) {
        let height_m = height_units * xf.scale_factor();
        let mut t = t;
        t.height_mm = height_m * self.mm_per_m;
        t.color = self.color(e, layer, ctx);
        if self.opts.text_elevations
            && !ctx.in_dimension
            && let Some(z) = parse_elevation(&t.content)
        {
            let at = xf.apply(t.position);
            self.elev_texts.push(ElevText {
                at,
                z,
                radius: self.opts.elevation_search_radius.max(3.0 * height_m),
                layer: layer.to_owned(),
            });
        }
        self.emit(Shape::Text(t), None, None, layer, xf);
    }

    fn text(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        if e.kind == "ATTRIB" && e.i(70).unwrap_or(0) & 1 != 0 {
            return; // invisible attribute
        }
        let Some(p1) = e.point(10) else { return self.bad(e) };
        let content = decode_percent_codes(&e.s(1, &self.dec).unwrap_or_default());
        if content.trim().is_empty() {
            return;
        }
        let h = e.f_or(40, 2.5).abs();
        let mut rot = e.f_or(50, 0.0).to_radians();
        let ha = e.i(72).unwrap_or(0);
        let va = if e.kind == "ATTRIB" { e.i(74).unwrap_or(0) } else { e.i(73).unwrap_or(0) };
        let p2 = e.point(11);
        let (pos, anchor) = match (ha, va, p2) {
            (0, 0, _) | (_, _, None) => (p1, Anchor::BottomLeft),
            (3 | 5, _, Some(p2)) => {
                // Aligned and fit text run from the first to the second point.
                rot = (p2[1] - p1[1]).atan2(p2[0] - p1[0]);
                (p1, Anchor::BottomLeft)
            }
            (4, _, Some(p2)) => (p2, Anchor::Centre),
            (h, v, Some(p2)) => {
                let anchor = match (h, v) {
                    (0, 3) => Anchor::TopLeft,
                    (1, 3) => Anchor::TopCentre,
                    (2, 3) => Anchor::TopRight,
                    (0, 2) => Anchor::MiddleLeft,
                    (1, 2) => Anchor::Centre,
                    (2, 2) => Anchor::MiddleRight,
                    (1, _) => Anchor::BottomCentre,
                    (2, _) => Anchor::BottomRight,
                    _ => Anchor::BottomLeft,
                };
                (p2, anchor)
            }
        };
        let xf = ctx.xf.then(&ocs_affine(e.extrusion(), pos[2]));
        let mut t = Text::new(pt(pos[0], pos[1]), content, 0.0).anchored(anchor);
        t.rotation = rot;
        // Text styles named for bold (as written by the exporter) keep the weight.
        t.bold = e.s(7, &self.dec).is_some_and(|s| s.to_uppercase().contains("BOLD"));
        self.text_item(t, e, layer, ctx, &xf, h);
    }

    fn mtext(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let Some(p) = e.point(10) else { return self.bad(e) };
        let mut raw = String::new();
        for pair in e.pairs.iter().filter(|q| q.code == 3) {
            raw.push_str(&self.dec.text(pair.raw));
        }
        if let Some(last) = e.get(1) {
            raw.push_str(&self.dec.text(last.raw));
        }
        let lines = mtext_lines(&raw);
        if lines.is_empty() {
            return;
        }
        let h = e.f_or(40, 2.5).abs();
        let rot = match e.point(11) {
            Some(d) if d[0].hypot(d[1]) > 1e-12 => d[1].atan2(d[0]),
            _ => e.f_or(50, 0.0),
        };
        let attach = e.i(71).unwrap_or(1).clamp(1, 9);
        let (hfrac, row) = (((attach - 1) % 3) as f64 / 2.0, (attach - 1) / 3);
        let spacing = h * 5.0 / 3.0 * e.f_or(44, 1.0).clamp(0.25, 4.0);
        let total = (lines.len() as f64 - 1.0) * spacing + h;
        let top = match row {
            0 => 0.0,
            1 => total / 2.0,
            _ => total,
        };
        let anchor = match hfrac {
            f if f < 0.25 => Anchor::TopLeft,
            f if f < 0.75 => Anchor::TopCentre,
            _ => Anchor::TopRight,
        };
        let (s, c) = rot.sin_cos();
        for (i, line) in lines.into_iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let dy = top - i as f64 * spacing;
            let at = pt(p[0] - s * dy, p[1] + c * dy);
            let mut t = Text::new(at, line, 0.0).anchored(anchor);
            t.rotation = rot;
            self.text_item(t, e, layer, ctx, &ctx.xf.clone(), h);
        }
    }

    fn insert(&mut self, e: &Obj<'a>, layer: &str, ctx: &Ctx) {
        let name = e.s(2, &self.dec).unwrap_or_default().trim().to_uppercase();
        let ins = e.point(10).unwrap_or([0.0; 3]);
        let (sx, sy, sz) = (e.f_or(41, 1.0), e.f_or(42, 1.0), e.f_or(43, 1.0));
        let rot = e.f_or(50, 0.0).to_radians();
        let (cols, rows) = (e.i(70).unwrap_or(1).clamp(1, 10_000), e.i(71).unwrap_or(1).clamp(1, 10_000));
        let (dx, dy) = (e.f_or(44, 0.0), e.f_or(45, 0.0));
        if cols * rows > 10_000 {
            let msg = format!("line {}: array insert of {} copies skipped", e.line, cols * rows);
            self.warn(msg);
            return;
        }
        let color = self.color(e, layer, ctx);
        let stroke = self.stroke(e, layer, ctx);
        let ocs = ocs_affine(e.extrusion(), ins[2]);
        let base = self.blocks.get(&name).map(|b| b.base).unwrap_or([0.0; 3]);
        for r in 0..rows {
            for c in 0..cols {
                let local = Affine { tx: ins[0], ty: ins[1], ..Affine::rotation(rot) }
                    .then(&Affine::scale_translate(1.0, c as f64 * dx, r as f64 * dy))
                    .then(&Affine { a: sx, b: 0.0, c: 0.0, d: sy, tx: 0.0, ty: 0.0 })
                    .then(&Affine::scale_translate(1.0, -base[0], -base[1]));
                let sub = Ctx {
                    xf: ctx.xf.then(&ocs).then(&local),
                    depth: ctx.depth + 1,
                    layer0: Some(layer.to_owned()),
                    color,
                    linetype: stroke.pattern.clone(),
                    weight: stroke.weight_mm,
                    in_dimension: ctx.in_dimension,
                    z_offset: ctx.z_offset + ctx.z_scale * (ins[2] - base[2] * sz),
                    z_scale: ctx.z_scale * sz,
                };
                self.expand(&name, &sub);
            }
        }
        // Attributes are placed in the insert's coordinate system already.
        let mut elevation: Option<f64> = None;
        let mut label: Option<String> = None;
        for a in &e.children {
            let tag = a.s(2, &self.dec).unwrap_or_default().trim().to_uppercase();
            let value = decode_percent_codes(&a.s(1, &self.dec).unwrap_or_default());
            if ELEVATION_TAGS.contains(&tag.as_str()) {
                elevation = elevation.or_else(|| value.trim().replace(',', ".").parse::<f64>().ok());
            } else if LABEL_TAGS.contains(&tag.as_str()) && looks_like_label(&value) {
                label = label.or(Some(value.trim().to_owned()));
            }
            self.entity(a, ctx);
        }
        let w = ctx.xf.then(&ocs).apply(pt(ins[0], ins[1]));
        let z_world = ctx.z_offset + ctx.z_scale * ins[2];
        let has_z = elevation.is_some() || (ins[2].abs() > 1e-9 && ctx.depth == 0);
        if has_z {
            let z = elevation.map(|v| ctx.z_offset + ctx.z_scale * v).unwrap_or(z_world);
            self.points.push(SurveyPoint {
                position: [w.x, w.y, z],
                has_elevation: true,
                source: PointSource::Insert,
                label,
                layer: layer.to_owned(),
            });
        }
    }

    fn expand(&mut self, name: &str, ctx: &Ctx) {
        if ctx.depth > self.opts.max_block_depth {
            self.warn(format!(
                "block {name} nested deeper than {} levels, skipped",
                self.opts.max_block_depth
            ));
            return;
        }
        let Some(block) = self.blocks.get(name) else {
            if !name.is_empty() {
                self.warn(format!("block {name} is used but not defined"));
            }
            return;
        };
        if block.xref {
            self.warn(format!("block {name} refers to an external file, skipped"));
            return;
        }
        let ents = block.ents.clone();
        for e in &ents {
            self.entity(e, ctx);
        }
    }

    fn hatch(&mut self, e: &Obj<'_>, layer: &str, ctx: &Ctx) {
        let paths = hatch_paths(e.pairs, self.tol(&ctx.xf));
        if paths.is_empty() {
            return;
        }
        let elev = e.point(10).map(|p| p[2]).unwrap_or(0.0);
        let xf = ctx.xf.then(&ocs_affine(e.extrusion(), elev));
        let st = self.stroke(e, layer, ctx);
        let solid = e.i(70) == Some(1);
        let fill = solid.then_some(Fill::Solid(st.color));
        self.emit(
            Shape::Polygon { rings: paths },
            Some(Stroke { weight_mm: weights::EXTRA_FINE, ..st }),
            fill,
            layer,
            &xf,
        );
    }

    /// Gives elevations from texts to nearby points without one; texts with no point nearby become points.
    fn pair_elevation_texts(&mut self) {
        let texts = std::mem::take(&mut self.elev_texts);
        for t in texts {
            let near = self
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| (i, t.at.distance(pt(p.position[0], p.position[1]))))
                .filter(|(_, d)| *d <= t.radius)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match near.and_then(|(i, _)| self.points.get_mut(i)) {
                Some(p) if !p.has_elevation => {
                    p.position[2] = t.z;
                    p.has_elevation = true;
                    p.source = PointSource::TextNearPoint;
                }
                Some(p) if (p.position[2] - t.z).abs() < 0.005 => {} // the text labels the point's own elevation
                _ => self.points.push(SurveyPoint {
                    position: [t.at.x, t.at.y, t.z],
                    has_elevation: true,
                    source: PointSource::Text,
                    label: None,
                    layer: t.layer,
                }),
            }
        }
    }
}

/// A 24-bit true colour value (group 420): 0x00RRGGBB.
fn true_color(v: i64) -> Rgba {
    let v = v as u32;
    Rgba::rgb(((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8)
}

/// Points from repeated `code` / `code + 10` pairs, in order.
fn repeated_points(pairs: &[Pair<'_>], code: i32) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::new();
    for p in pairs {
        if p.code == code {
            out.push(pt(p.f64().unwrap_or(0.0), 0.0));
        } else if p.code == code + 10
            && let Some(last) = out.last_mut()
        {
            last.y = p.f64().unwrap_or(0.0);
        }
    }
    out
}

/// Boundary paths of a HATCH entity as rings (OCS), arcs flattened with `tol`.
fn hatch_paths(pairs: &[Pair<'_>], tol: f64) -> Vec<Vec<Pt>> {
    let mut rings = Vec::new();
    let Some(start) = pairs.iter().position(|p| p.code == 91) else { return rings };
    let count = pairs.get(start).and_then(Pair::i64).unwrap_or(0).max(0) as usize;
    let mut i = start + 1;
    let next = |i: &mut usize, code: i32| -> Option<f64> {
        // Advance to the next pair with this code (within a small window, to stay aligned).
        let mut j = *i;
        while let Some(p) = pairs.get(j) {
            if p.code == code {
                *i = j + 1;
                return p.f64();
            }
            if j > *i + 64 {
                break;
            }
            j += 1;
        }
        None
    };
    // Counts come from the file: never trust them beyond the number of pairs that could hold the data.
    let limit = |v: Option<f64>| (v.unwrap_or(0.0).max(0.0) as usize).min(pairs.len());
    for _ in 0..count.min(pairs.len()) {
        let Some(flags) = next(&mut i, 92) else { break };
        let flags = flags as i64;
        let mut ring: Vec<Pt> = Vec::new();
        if flags & 2 != 0 {
            let has_bulge = next(&mut i, 72).unwrap_or(0.0) != 0.0;
            let _closed = next(&mut i, 73);
            let n = limit(next(&mut i, 93));
            let mut verts: Vec<(Pt, f64)> = Vec::with_capacity(n);
            for _ in 0..n {
                let Some(x) = next(&mut i, 10) else { break };
                let y = next(&mut i, 20).unwrap_or(0.0);
                let b = if has_bulge && pairs.get(i).is_some_and(|p| p.code == 42) {
                    next(&mut i, 42).unwrap_or(0.0)
                } else {
                    0.0
                };
                verts.push((pt(x, y), b));
            }
            let m = verts.len();
            for k in 0..m {
                let (Some((a, b)), Some((c, _))) = (verts.get(k), verts.get((k + 1) % m)) else { continue };
                // Every segment but its end point, which starts the next one.
                let seg = bulge_points(*a, *c, *b, tol);
                let keep = seg.len().saturating_sub(1);
                ring.extend(seg.into_iter().take(keep));
            }
        } else {
            let edges = limit(next(&mut i, 93));
            for _ in 0..edges {
                let Some(kind) = next(&mut i, 72) else { break };
                match kind as i64 {
                    1 => {
                        let a = pt(next(&mut i, 10).unwrap_or(0.0), next(&mut i, 20).unwrap_or(0.0));
                        let _b = pt(next(&mut i, 11).unwrap_or(0.0), next(&mut i, 21).unwrap_or(0.0));
                        ring.push(a);
                    }
                    2 => {
                        let c = pt(next(&mut i, 10).unwrap_or(0.0), next(&mut i, 20).unwrap_or(0.0));
                        let r = next(&mut i, 40).unwrap_or(0.0);
                        let (a0, a1) = (next(&mut i, 50).unwrap_or(0.0), next(&mut i, 51).unwrap_or(360.0));
                        let ccw = next(&mut i, 73).unwrap_or(1.0) != 0.0;
                        let (s, mut sweep) = if ccw { (a0, a1 - a0) } else { (-a0, -(a1 - a0)) };
                        if ccw && sweep <= 0.0 {
                            sweep += 360.0;
                        } else if !ccw && sweep >= 0.0 {
                            sweep -= 360.0;
                        }
                        let pts = arc_points(c, r, s.to_radians(), sweep.to_radians(), tol);
                        let keep = pts.len().saturating_sub(1);
                        ring.extend(pts.into_iter().take(keep));
                    }
                    3 => {
                        let c = pt(next(&mut i, 10).unwrap_or(0.0), next(&mut i, 20).unwrap_or(0.0));
                        let m = pt(next(&mut i, 11).unwrap_or(0.0), next(&mut i, 21).unwrap_or(0.0));
                        let ratio = next(&mut i, 40).unwrap_or(1.0);
                        let (a0, a1) = (next(&mut i, 50).unwrap_or(0.0), next(&mut i, 51).unwrap_or(360.0));
                        let ccw = next(&mut i, 73).unwrap_or(1.0) != 0.0;
                        let (s, mut sweep) = if ccw { (a0, a1 - a0) } else { (-a0, -(a1 - a0)) };
                        if ccw && sweep <= 0.0 {
                            sweep += 360.0;
                        } else if !ccw && sweep >= 0.0 {
                            sweep -= 360.0;
                        }
                        let sweep = sweep.clamp(-360.0, 360.0);
                        let n = ((sweep.abs() / 5.0).ceil() as usize).clamp(4, 72);
                        for k in 0..n {
                            let t = (s + sweep * k as f64 / n as f64).to_radians();
                            ring.push(pt(
                                c.x + t.cos() * m.x - t.sin() * m.y * ratio,
                                c.y + t.cos() * m.y + t.sin() * m.x * ratio,
                            ));
                        }
                    }
                    4 => {
                        // Spline edge: use its control points (fit points follow if any).
                        let _deg = next(&mut i, 94);
                        let _rational = next(&mut i, 73);
                        let _periodic = next(&mut i, 74);
                        let nk = limit(next(&mut i, 95));
                        let nc = limit(next(&mut i, 96));
                        for _ in 0..nk {
                            if next(&mut i, 40).is_none() {
                                break;
                            }
                        }
                        for _ in 0..nc {
                            let Some(x) = next(&mut i, 10) else { break };
                            let y = next(&mut i, 20).unwrap_or(0.0);
                            ring.push(pt(x, y));
                        }
                    }
                    _ => break,
                }
            }
        }
        // Skip the source object references.
        if pairs.get(i).is_some_and(|p| p.code == 97) {
            let n = pairs.get(i).and_then(Pair::i64).unwrap_or(0).max(0) as usize;
            i = i.saturating_add(1).saturating_add(n.min(pairs.len()));
        }
        ring.dedup_by(|a, b| a.distance(*b) < 1e-12);
        if ring.len() >= 3 {
            rings.push(ring);
        }
    }
    rings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevations_parse_strictly() {
        assert_eq!(parse_elevation("+125.34"), Some(125.34));
        assert_eq!(parse_elevation("125,34"), Some(125.34));
        assert_eq!(parse_elevation("Η=125.340 m"), Some(125.34));
        assert_eq!(parse_elevation("Z: -2.5"), Some(-2.5));
        assert_eq!(parse_elevation("125"), None);
        assert_eq!(parse_elevation("12.3456"), None);
        assert_eq!(parse_elevation("Σ12"), None);
        assert_eq!(parse_elevation("4500.00"), None);
        assert_eq!(parse_elevation("1.2.3"), None);
    }

    #[test]
    fn bulges_make_arcs() {
        // Half circle from (0, 0) to (2, 0), counter-clockwise: passes (1, -1).
        let pts = bulge_points(pt(0.0, 0.0), pt(2.0, 0.0), 1.0, 1e-4);
        assert!(pts.iter().any(|p| p.distance(pt(1.0, -1.0)) < 1e-3));
        assert!(pts.iter().all(|p| (p.distance(pt(1.0, 0.0)) - 1.0).abs() < 1e-9));
        assert_eq!(bulge_points(pt(0.0, 0.0), pt(1.0, 0.0), 0.0, 1e-4).len(), 2);
    }

    #[test]
    fn mirrored_ocs_flips_x() {
        let m = ocs_affine([0.0, 0.0, -1.0], 0.0);
        let p = m.apply(pt(2.0, 3.0));
        assert!(p.distance(pt(-2.0, 3.0)) < 1e-12);
        assert!(m.is_mirroring());
        assert_eq!(ocs_affine([0.0, 0.0, 1.0], 5.0), Affine::IDENTITY);
    }

    #[test]
    fn nurbs_reproduce_a_quadratic_circle_arc() {
        // Quarter circle as a rational quadratic: weights 1, √2/2, 1.
        let ctrl = [[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let w = [1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0];
        let knots = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let pts = nurbs_points(2, &knots, &ctrl, &w, 20).unwrap();
        for p in &pts {
            assert!((p.distance(pt(0.0, 0.0)) - 1.0).abs() < 1e-12);
        }
        assert!(pts.last().unwrap().distance(pt(0.0, 1.0)) < 1e-12);
        assert!(nurbs_points(2, &[0.0, 1.0], &ctrl, &w, 4).is_none());
    }
}
