//! Windows, and the frame, sash, sill, shutter and roller-shutter parts that glazed doors share with them.
//!
//! Canonical coordinates (see `host.rs`): the outer face is `ye`, sashes open towards the inner face `yi`.
//! The frame sits in a band across the wall, `setback` from the outer face.

use std::f64::consts::FRAC_PI_2;

use crate::host::{Host, Swing};
use crate::params::P;
use crate::parts::{Parts, color};
use crate::sym::{Pen, Sym};

/// Layer names of one opening kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layers {
    /// Frames, sashes, leaves.
    pub main: &'static str,
    /// Swing arcs and sliding arrows.
    pub swing: &'static str,
    /// Parts above the cut plane (roller boxes, overhead tracks, lintels).
    pub above: &'static str,
    /// Sills and thresholds.
    pub sill: &'static str,
    /// External shutters.
    pub shutter: &'static str,
}

/// Layers of window symbols.
pub(crate) const WINDOW: Layers = Layers {
    main: "window.symbol",
    swing: "window.symbol.swing",
    above: "window.symbol.above",
    sill: "window.symbol.sill",
    shutter: "window.symbol.shutter",
};

/// Layers of door symbols.
pub(crate) const DOOR: Layers = Layers {
    main: "door.symbol",
    swing: "door.symbol.swing",
    above: "door.symbol.above",
    sill: "door.symbol.threshold",
    shutter: "door.symbol.shutter",
};

/// Face width of sash profiles.
const SASH: f64 = 0.065;
/// Overlap of sliding panes at their meeting stiles.
const OVERLAP: f64 = 0.05;
/// Height of the external sill slab.
const SILL_OUT_T: f64 = 0.03;
/// Height of the internal sill slab.
const SILL_IN_T: f64 = 0.02;
/// Projection of the internal sill beyond the inner face.
const SILL_IN_PROJ: f64 = 0.02;
/// How far sills run into the jambs on each side.
const EARS: f64 = 0.03;

/// Frame face width, limited to a quarter of the opening width.
pub(crate) fn frame_w(host: &Host, p: &P) -> f64 {
    p.get("frame").min(host.w / 4.0)
}

/// The frame band across the wall `(y0, y1)`; `y0` faces the outer face.
pub(crate) fn band(host: &Host, p: &P) -> (f64, f64) {
    let t = host.t();
    let fd = p.get("frame_depth").min(t);
    let sb = p.get("setback").min(t - fd).max(0.0);
    (host.ye + sb, host.ye + sb + fd)
}

/// Which jamb of a sash carries its hinges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hinge {
    /// Hinged at `x0`.
    Left,
    /// Hinged at `x1`.
    Right,
}

/// One casement sash between `x0` and `x1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sash {
    pub x0: f64,
    pub x1: f64,
    pub hinge: Hinge,
}

impl Sash {
    fn swing(&self, y: f64, thick: f64, angle: f64) -> Swing {
        match self.hinge {
            Hinge::Left => Swing { hx: self.x0, y, dir: 1.0, len: self.x1 - self.x0, thick, angle },
            Hinge::Right => Swing { hx: self.x1, y, dir: -1.0, len: self.x1 - self.x0, thick, angle },
        }
    }
    /// x of the handle (on the free stile).
    fn handle_x(&self) -> f64 {
        match self.hinge {
            Hinge::Left => self.x1 - SASH / 2.0,
            Hinge::Right => self.x0 + SASH / 2.0,
        }
    }
}

/// Sashes and mullions of a casement window with `n` leaves (1–3) between the jambs `xa … xb`.
///
/// Two leaves meet without a mullion; three leaves are one single leaf, a mullion, and a pair.
pub(crate) fn casement_layout(xa: f64, xb: f64, n: usize, f: f64) -> (Vec<Sash>, Vec<(f64, f64)>) {
    match n {
        0 | 1 => (vec![Sash { x0: xa, x1: xb, hinge: Hinge::Left }], Vec::new()),
        2 => {
            let mid = (xa + xb) / 2.0;
            (
                vec![
                    Sash { x0: xa, x1: mid, hinge: Hinge::Left },
                    Sash { x0: mid, x1: xb, hinge: Hinge::Right },
                ],
                vec![],
            )
        }
        _ => {
            let a = xa + (xb - xa - f) / 3.0;
            let mid = (a + f + xb) / 2.0;
            (
                vec![
                    Sash { x0: xa, x1: a, hinge: Hinge::Left },
                    Sash { x0: a + f, x1: mid, hinge: Hinge::Left },
                    Sash { x0: mid, x1: xb, hinge: Hinge::Right },
                ],
                vec![(a, a + f)],
            )
        }
    }
}

/// One sliding pane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pane {
    pub x0: f64,
    pub x1: f64,
    /// Track index, 0 nearest the outer face.
    pub track: usize,
    /// Sliding direction: +1 towards +x, −1 towards −x, 0 fixed.
    pub dir: i8,
}

/// Panes and number of tracks of a sliding window or door with `n` panes (2–4) between `xa … xb`.
///
/// Two panes use two tracks; three panes use three; four panes use two tracks with the middle pair sliding
/// outwards over the fixed end panes.
pub(crate) fn sliding_layout(xa: f64, xb: f64, n: usize) -> (Vec<Pane>, usize) {
    let len = xb - xa;
    match n {
        0..=2 => {
            let mid = (xa + xb) / 2.0;
            (
                vec![
                    Pane { x0: xa, x1: mid + OVERLAP / 2.0, track: 0, dir: 1 },
                    Pane { x0: mid - OVERLAP / 2.0, x1: xb, track: 1, dir: -1 },
                ],
                2,
            )
        }
        3 => {
            let pw = (len + 2.0 * OVERLAP) / 3.0;
            let panes = (0..3)
                .map(|k| {
                    let x0 = xa + k as f64 * (pw - OVERLAP);
                    Pane { x0, x1: x0 + pw, track: k, dir: [1, 0, -1].get(k).copied().unwrap_or(0) }
                })
                .collect();
            (panes, 3)
        }
        _ => {
            let pw = (len + 2.0 * OVERLAP) / 4.0;
            let mid = (xa + xb) / 2.0;
            (
                vec![
                    Pane { x0: xa, x1: xa + pw, track: 0, dir: 0 },
                    Pane { x0: xa + pw - OVERLAP, x1: mid, track: 1, dir: -1 },
                    Pane { x0: mid, x1: xb - pw + OVERLAP, track: 1, dir: 1 },
                    Pane { x0: xb - pw, x1: xb, track: 0, dir: 0 },
                ],
                2,
            )
        }
    }
}

/// Centre and thickness of a sliding track.
fn track(y0: f64, y1: f64, tracks: usize, k: usize) -> (f64, f64) {
    let slot = (y1 - y0) / tracks.max(1) as f64;
    (y0 + (k as f64 + 0.5) * slot, (slot * 0.7).max(0.012))
}

/// Jambs and mullions in plan: cut profiles at 1:50, the band outline at smaller scales.
pub(crate) fn frame_symbol(sym: &mut Sym, host: &Host, f: f64, y0: f64, y1: f64, mullions: &[(f64, f64)]) {
    let w2 = host.w2();
    if sym.fine() {
        sym.rect(-w2, y0, -w2 + f, y1, Pen::Frame);
        sym.rect(w2 - f, y0, w2, y1, Pen::Frame);
        for (a, b) in mullions {
            sym.rect(*a, y0, *b, y1, Pen::Frame);
        }
    } else {
        sym.rect(-w2, y0, w2, y1, Pen::Frame);
    }
}

/// Closed sashes in plan: stiles cut at 1:50 and the glass between them.
fn sashes_symbol(sym: &mut Sym, sashes: &[Sash], y0: f64, y1: f64) {
    let ym = (y0 + y1) / 2.0;
    for s in sashes {
        let sp = SASH.min((s.x1 - s.x0) / 3.0);
        if sym.fine() {
            let (sy0, sy1) = (y0 + 0.01, y1 - 0.005);
            sym.rect(s.x0, sy0, s.x0 + sp, sy1, Pen::Frame);
            sym.rect(s.x1 - sp, sy0, s.x1, sy1, Pen::Frame);
            sym.line((s.x0 + sp, ym), (s.x1 - sp, ym), Pen::Fine);
        } else {
            sym.line((s.x0, ym), (s.x1, ym), Pen::Fine);
        }
    }
}

/// Sills in plan: the projecting part of the external sill and the internal sill.
pub(crate) fn sills_symbol(sym: &mut Sym, host: &Host, p: &P, layers: &Layers) {
    let w2 = host.w2();
    sym.layer(layers.sill);
    let proj = p.get("sill_projection");
    if p.flag("sill_out") && proj > 0.002 {
        let (x0, x1, y) = (-w2 - EARS, w2 + EARS, host.ye - proj);
        sym.poly(&[(x0, host.ye), (x0, y), (x1, y), (x1, host.ye)], false, Pen::Fine);
    }
    if p.flag("sill_in") {
        let (x0, x1, y) = (-w2 - SILL_IN_PROJ, w2 + SILL_IN_PROJ, host.yi + SILL_IN_PROJ);
        sym.poly(&[(x0, host.yi), (x0, y), (x1, y), (x1, host.yi)], false, Pen::Fine);
    }
    sym.layer(layers.main);
}

/// Threshold of a door in plan: lines along both faces across the opening.
pub(crate) fn threshold_symbol(sym: &mut Sym, host: &Host, layers: &Layers) {
    let w2 = host.w2();
    sym.layer(layers.sill);
    sym.line((-w2, host.yi), (w2, host.yi), Pen::Fine);
    sym.line((-w2, host.ye), (w2, host.ye), Pen::Fine);
    sym.layer(layers.main);
}

/// Number of shutter leaves for an opening.
fn shutter_leaves(host: &Host) -> usize {
    if host.w >= 0.75 { 2 } else { 1 }
}

const SHUTTER_T: f64 = 0.03;
const SHUTTER_GAP: f64 = 0.01;

/// External shutters opened flat against the façade, in plan.
pub(crate) fn shutters_symbol(sym: &mut Sym, host: &Host, layers: &Layers) {
    let n = shutter_leaves(host);
    let ws = host.w / n as f64;
    let w2 = host.w2();
    let y1 = host.ye - SHUTTER_GAP;
    let y0 = y1 - SHUTTER_T;
    sym.layer(layers.shutter);
    sym.rect(-w2 - ws, y0, -w2, y1, Pen::Fine);
    if n == 2 {
        sym.rect(w2, y0, w2 + ws, y1, Pen::Fine);
    }
    sym.layer(layers.main);
}

/// Depth of the roller-shutter box: from the outer face to just inside the frame.
fn roller_box_y(host: &Host, y1: f64) -> (f64, f64) {
    (host.ye + 0.01, (y1 + 0.02).min(host.yi - 0.005))
}

/// Roller shutter in plan: the box above the cut plane (dashed) and the guide rails at the jambs.
pub(crate) fn roller_symbol(sym: &mut Sym, host: &Host, y0: f64, y1: f64, layers: &Layers) {
    let w2 = host.w2();
    let (b0, b1) = roller_box_y(host, y1);
    sym.layer(layers.above);
    sym.rect(-w2, b0, w2, b1, Pen::Above);
    sym.layer(layers.main);
    let (g0, g1) = ((y0 - 0.045).max(host.ye), y0 - 0.005);
    if g1 > g0 + 0.005 {
        sym.rect(-w2, g0, -w2 + 0.03, g1, Pen::Frame);
        sym.rect(w2 - 0.03, g0, w2, g1, Pen::Frame);
    }
}

/// The outline of a window above the cut plane: frame band and glass, dashed.
pub(crate) fn overhead_symbol(sym: &mut Sym, host: &Host, p: &P, layers: &Layers) {
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    sym.layer(layers.above);
    sym.rect(-w2, y0, w2, y1, Pen::Above);
    sym.line((-w2, (y0 + y1) / 2.0), (w2, (y0 + y1) / 2.0), Pen::Above);
}

/// Casement windows (and the glazed leaves of casement balcony doors).
///
/// Windows show closed sashes with their swing (thin, on the swing layer); doors show the leaves open.
pub(crate) fn casement_symbol(sym: &mut Sym, host: &Host, p: &P, n: usize, door: bool) {
    let layers = if door { DOOR } else { WINDOW };
    let f = frame_w(host, p);
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    let (sashes, mullions) = casement_layout(-w2 + f, w2 - f, n, f);
    sym.layer(layers.main);
    frame_symbol(sym, host, f, y0, y1, &mullions);
    if door {
        let angle = p.angle("swing");
        let thick = p.get("leaf");
        for s in &sashes {
            s.swing(y1, thick, angle).draw(sym, Pen::Frame, layers.main, layers.swing);
        }
        if p.flag("threshold") {
            threshold_symbol(sym, host, &layers);
        }
    } else {
        sashes_symbol(sym, &sashes, y0, y1);
        for s in &sashes {
            s.swing(y1, 0.0, FRAC_PI_2).draw(sym, Pen::Fine, layers.swing, layers.swing);
        }
        sills_symbol(sym, host, p, &layers);
    }
    accessories_symbol(sym, host, p, y0, y1, &layers);
}

fn accessories_symbol(sym: &mut Sym, host: &Host, p: &P, y0: f64, y1: f64, layers: &Layers) {
    if p.flag("shutters") {
        shutters_symbol(sym, host, layers);
    }
    if p.flag("roller") {
        roller_symbol(sym, host, y0, y1, layers);
    }
}

/// Sliding windows and doors: panes on tracks with arrows showing which way they slide.
pub(crate) fn sliding_symbol(sym: &mut Sym, host: &Host, p: &P, door: bool) {
    let layers = if door { DOOR } else { WINDOW };
    let f = frame_w(host, p);
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    let (panes, tracks) = sliding_layout(-w2 + f, w2 - f, p.count("panes"));
    sym.layer(layers.main);
    frame_symbol(sym, host, f, y0, y1, &[]);
    for pane in &panes {
        let (yc, pt) = track(y0, y1, tracks, pane.track);
        if sym.fine() {
            sym.rect(pane.x0, yc - pt / 2.0, pane.x1, yc + pt / 2.0, Pen::Frame);
        } else {
            sym.line((pane.x0, yc), (pane.x1, yc), Pen::Frame);
        }
    }
    sym.layer(layers.swing);
    let ya = host.yi + sym.mm(2.5);
    for pane in &panes {
        if pane.dir != 0 {
            let xc = (pane.x0 + pane.x1) / 2.0;
            let half = ((pane.x1 - pane.x0) * 0.25).min(0.3);
            let d = f64::from(pane.dir);
            sym.arrow((xc - d * half, ya), (xc + d * half, ya), Pen::Fine);
        }
    }
    sym.layer(layers.main);
    if door {
        if p.flag("threshold") {
            threshold_symbol(sym, host, &layers);
        }
    } else {
        sills_symbol(sym, host, p, &layers);
    }
    accessories_symbol(sym, host, p, y0, y1, &layers);
}

/// Fixed windows: frame, mullions and glass.
fn fixed_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let f = frame_w(host, p);
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    let bays = mullion_bays(-w2 + f, w2 - f, p.count("mullions"), f);
    let mullions: Vec<(f64, f64)> =
        bays.windows(2).filter_map(|w| Some((w.first()?.1, w.get(1)?.0))).collect();
    sym.layer(WINDOW.main);
    frame_symbol(sym, host, f, y0, y1, &mullions);
    let ym = (y0 + y1) / 2.0;
    if sym.fine() {
        for (a, b) in &bays {
            sym.line((*a, ym), (*b, ym), Pen::Fine);
        }
    } else {
        sym.line((-w2, ym), (w2, ym), Pen::Fine);
    }
    sills_symbol(sym, host, p, &WINDOW);
    accessories_symbol(sym, host, p, y0, y1, &WINDOW);
}

/// Glass bays between `m` equally spaced mullions of width `f`.
fn mullion_bays(xa: f64, xb: f64, m: usize, f: f64) -> Vec<(f64, f64)> {
    let m = m.min(12);
    let bay = (xb - xa - m as f64 * f) / (m + 1) as f64;
    if bay < 0.05 {
        return vec![(xa, xb)];
    }
    (0..=m)
        .map(|k| {
            let a = xa + k as f64 * (bay + f);
            (a, a + bay)
        })
        .collect()
}

/// Plan symbol of a window style.
pub(crate) fn symbol(id: &str, sym: &mut Sym, host: &Host, p: &P) {
    match id {
        "window.top_hung" => {
            let f = frame_w(host, p);
            let (y0, y1) = band(host, p);
            let w2 = host.w2();
            sym.layer(WINDOW.main);
            frame_symbol(sym, host, f, y0, y1, &[]);
            sashes_symbol(sym, &[Sash { x0: -w2 + f, x1: w2 - f, hinge: Hinge::Left }], y0, y1);
            sills_symbol(sym, host, p, &WINDOW);
            accessories_symbol(sym, host, p, y0, y1, &WINDOW);
        }
        "window.casement_1" => casement_symbol(sym, host, p, 1, false),
        "window.casement_3" => casement_symbol(sym, host, p, 3, false),
        "window.tilt_turn" => casement_symbol(sym, host, p, p.count("leaves"), false),
        "window.sliding" => sliding_symbol(sym, host, p, false),
        "window.fixed" => fixed_symbol(sym, host, p),
        _ => casement_symbol(sym, host, p, 2, false),
    }
}

// ---------------------------------------------------------------------------------------------------------
// 3D

/// The outer frame: jambs, head, bottom rail (windows) and mullions, between `zb` and `zt`.
pub(crate) fn frame_parts(
    parts: &mut Parts,
    host: &Host,
    f: f64,
    (y0, y1): (f64, f64),
    (zb, zt): (f64, f64),
    bottom: bool,
) {
    let w2 = host.w2();
    parts.bx([-w2, y0, zb], [-w2 + f, y1, zt], color::FRAME);
    parts.bx([w2 - f, y0, zb], [w2, y1, zt], color::FRAME);
    parts.bx([-w2 + f, y0, zt - f], [w2 - f, y1, zt], color::FRAME);
    if bottom {
        parts.bx([-w2 + f, y0, zb], [w2 - f, y1, zb + f], color::FRAME);
    } else {
        // A low threshold profile under door leaves.
        parts.bx([-w2 + f, y0, zb], [w2 - f, y1, zb + 0.015], color::METAL);
    }
}

/// A sash or glazed leaf: profile ring with glass, solid (panel) up to `panel_top` when given.
pub(crate) fn sash_parts(
    parts: &mut Parts,
    (x0, x1): (f64, f64),
    (y0, y1): (f64, f64),
    (z0, z1): (f64, f64),
    panel_top: Option<f64>,
    body: kanavos_drawing::Rgba,
) {
    let sp = SASH.min((x1 - x0) / 3.0).min((z1 - z0) / 3.0);
    if sp <= 0.0 {
        return;
    }
    let zg0 = panel_top.map(|z| z.min(z1 - 2.0 * sp).max(z0 + sp)).unwrap_or(z0 + sp);
    parts.bx([x0, y0, z0], [x0 + sp, y1, z1], body);
    parts.bx([x1 - sp, y0, z0], [x1, y1, z1], body);
    parts.bx([x0 + sp, y0, z0], [x1 - sp, y1, zg0], body);
    parts.bx([x0 + sp, y0, z1 - sp], [x1 - sp, y1, z1], body);
    let ym = (y0 + y1) / 2.0;
    parts.bx([x0 + sp, ym - 0.006, zg0], [x1 - sp, ym + 0.006, z1 - sp], color::GLASS);
}

/// A window lever on the inner face `y` of a sash, pointing down.
pub(crate) fn window_handle(parts: &mut Parts, x: f64, y: f64, z: f64) {
    parts.bx([x - 0.016, y, z - 0.04], [x + 0.016, y + 0.012, z + 0.04], color::METAL);
    parts.bx([x - 0.01, y + 0.012, z - 0.13], [x + 0.01, y + 0.03, z + 0.012], color::METAL);
}

/// Sills in 3D; returns the height the frame stands on.
pub(crate) fn sills_parts(parts: &mut Parts, host: &Host, p: &P, (y0, y1): (f64, f64)) -> f64 {
    let w2 = host.w2();
    let mut zb = 0.0;
    if p.flag("sill_out") {
        let proj = p.get("sill_projection");
        parts.bx([-w2 - EARS, host.ye - proj, 0.0], [w2 + EARS, y0, SILL_OUT_T], color::MARBLE);
        zb = SILL_OUT_T;
    }
    if p.flag("sill_in") {
        parts.bx(
            [-w2 - SILL_IN_PROJ, y1, 0.0],
            [w2 + SILL_IN_PROJ, host.yi + SILL_IN_PROJ, SILL_IN_T],
            color::MARBLE,
        );
    }
    zb
}

/// A marble threshold across the wall; returns its top.
pub(crate) fn threshold_parts(parts: &mut Parts, host: &Host) -> f64 {
    let w2 = host.w2();
    parts.bx([-w2, host.ye - 0.02, 0.0], [w2, host.yi, 0.02], color::MARBLE);
    0.02
}

/// One louvred shutter leaf.
fn louvred_leaf(parts: &mut Parts, (x0, x1): (f64, f64), (y0, y1): (f64, f64), (z0, z1): (f64, f64)) {
    let (stile, rail) = (0.05_f64.min((x1 - x0) / 4.0), 0.07);
    if z1 - z0 < 3.0 * rail {
        parts.bx([x0, y0, z0], [x1, y1, z1], color::SHUTTER);
        return;
    }
    parts.bx([x0, y0, z0], [x0 + stile, y1, z1], color::SHUTTER);
    parts.bx([x1 - stile, y0, z0], [x1, y1, z1], color::SHUTTER);
    parts.bx([x0 + stile, y0, z0], [x1 - stile, y1, z0 + rail], color::SHUTTER);
    parts.bx([x0 + stile, y0, z1 - rail], [x1 - stile, y1, z1], color::SHUTTER);
    let ym = (y0 + y1) / 2.0;
    let mut z = z0 + rail + 0.008;
    while z + 0.035 < z1 - rail {
        parts.bx([x0 + stile, ym - 0.006, z], [x1 - stile, ym + 0.006, z + 0.035], color::SHUTTER);
        z += 0.05;
    }
}

/// External shutters opened flat against the façade.
pub(crate) fn shutters_parts(parts: &mut Parts, host: &Host, z: (f64, f64)) {
    let n = shutter_leaves(host);
    let ws = host.w / n as f64;
    let w2 = host.w2();
    let y1 = host.ye - SHUTTER_GAP;
    let y = (y1 - SHUTTER_T, y1);
    louvred_leaf(parts, (-w2 - ws, -w2), y, z);
    if n == 2 {
        louvred_leaf(parts, (w2, w2 + ws), y, z);
    }
}

/// Roller-shutter box at the head and guide rails at the jambs; returns the height below the box.
pub(crate) fn roller_parts(parts: &mut Parts, host: &Host, p: &P, (y0, y1): (f64, f64), zb: f64) -> f64 {
    let w2 = host.w2();
    let rb = p.get("roller_box").min(host.h * 0.4);
    let top = host.h - rb;
    let (b0, b1) = roller_box_y(host, y1);
    parts.bx([-w2, b0, top], [w2, b1, host.h], color::WHITE);
    let (g0, g1) = ((y0 - 0.045).max(host.ye), y0 - 0.005);
    if g1 > g0 + 0.005 {
        parts.bx([-w2, g0, zb], [-w2 + 0.03, g1, top], color::FRAME);
        parts.bx([w2 - 0.03, g0, zb], [w2, g1, top], color::FRAME);
        // The curtain, rolled almost fully up: one slat showing under the box.
        parts.bx(
            [-w2 + 0.015, (g0 + g1) / 2.0 - 0.005, top - 0.05],
            [w2 - 0.015, (g0 + g1) / 2.0 + 0.005, top],
            color::FRAME,
        );
    }
    top
}

/// Accessories shared by windows and glazed doors; returns the top of the frame.
fn accessories_parts(parts: &mut Parts, host: &Host, p: &P, band: (f64, f64), zb: f64) -> f64 {
    let zt = if p.flag("roller") { roller_parts(parts, host, p, band, zb) } else { host.h };
    if p.flag("shutters") {
        shutters_parts(parts, host, (zb, zt));
    }
    zt
}

/// Casement windows and casement balcony doors.
pub(crate) fn casement_parts(parts: &mut Parts, host: &Host, p: &P, n: usize, door: bool, top_hung: bool) {
    let f = frame_w(host, p);
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    let zb = if door {
        if p.flag("threshold") { threshold_parts(parts, host) } else { 0.0 }
    } else {
        sills_parts(parts, host, p, (y0, y1))
    };
    let zt = accessories_parts(parts, host, p, (y0, y1), zb);
    if zt - zb < 3.0 * f {
        return;
    }
    let (sashes, mullions) = casement_layout(-w2 + f, w2 - f, n, f);
    frame_parts(parts, host, f, (y0, y1), (zb, zt), !door);
    let sz0 = if door { zb + 0.015 } else { zb + f };
    let mut sz1 = zt - f;
    let tr = if door { 0.0 } else { p.get("transom") };
    if tr > 0.05 && sz1 - tr > sz0 + 0.3 {
        // Transom bar and fixed top light.
        let zt0 = sz1 - tr;
        parts.bx([-w2 + f, y0, zt0 - f], [w2 - f, y1, zt0], color::FRAME);
        let ym = (y0 + y1) / 2.0;
        parts.bx([-w2 + f, ym - 0.006, zt0], [w2 - f, ym + 0.006, sz1], color::GLASS);
        sz1 = zt0 - f;
    }
    for (a, b) in &mullions {
        parts.bx([*a, y0, sz0], [*b, y1, sz1], color::FRAME);
    }
    let sy = (y0 + 0.01, y1);
    let panel = if door { Some(sz0 + (sz1 - sz0) * (1.0 - p.fraction("glazing"))) } else { None };
    let zh = if door { 1.05_f64.min(sz1 - 0.1) } else { (sz0 + sz1) / 2.0 };
    for (i, s) in sashes.iter().enumerate() {
        sash_parts(parts, (s.x0, s.x1), sy, (sz0, sz1), panel, color::FRAME);
        // Handles on the free stile of single leaves and of the second leaf of a pair.
        let has_handle = match sashes.len() {
            1 => true,
            2 => i == 1,
            _ => i != 1,
        };
        if p.flag("handle") && has_handle {
            if top_hung {
                window_handle(parts, (s.x0 + s.x1) / 2.0, y1, sz0 + 0.12);
            } else {
                window_handle(parts, s.handle_x(), y1, zh);
            }
        }
    }
}

/// Sliding windows and sliding balcony doors.
pub(crate) fn sliding_parts(parts: &mut Parts, host: &Host, p: &P, door: bool) {
    let f = frame_w(host, p);
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    let zb = if door {
        if p.flag("threshold") { threshold_parts(parts, host) } else { 0.0 }
    } else {
        sills_parts(parts, host, p, (y0, y1))
    };
    let zt = accessories_parts(parts, host, p, (y0, y1), zb);
    if zt - zb < 3.0 * f {
        return;
    }
    frame_parts(parts, host, f, (y0, y1), (zb, zt), true);
    let (panes, tracks) = sliding_layout(-w2 + f, w2 - f, p.count("panes"));
    let (z0, z1) = (zb + f * 0.6, zt - f * 0.6);
    let panel = if door { Some(z0 + 0.12) } else { None };
    for pane in &panes {
        let (yc, pt) = track(y0, y1, tracks, pane.track);
        sash_parts(parts, (pane.x0, pane.x1), (yc - pt / 2.0, yc + pt / 2.0), (z0, z1), panel, color::FRAME);
        if pane.dir != 0 && p.flag("handle") {
            let x = if pane.dir > 0 { pane.x1 - 0.035 } else { pane.x0 + 0.035 };
            let zc = if door { 1.05_f64.min(z1 - 0.1) } else { (z0 + z1) / 2.0 };
            parts.bx(
                [x - 0.008, yc + pt / 2.0, zc - 0.1],
                [x + 0.008, yc + pt / 2.0 + 0.02, zc + 0.1],
                color::METAL,
            );
        }
    }
}

fn fixed_parts(parts: &mut Parts, host: &Host, p: &P) {
    let f = frame_w(host, p);
    let (y0, y1) = band(host, p);
    let w2 = host.w2();
    let zb = sills_parts(parts, host, p, (y0, y1));
    let zt = accessories_parts(parts, host, p, (y0, y1), zb);
    if zt - zb < 3.0 * f {
        return;
    }
    frame_parts(parts, host, f, (y0, y1), (zb, zt), true);
    let mut z1 = zt - f;
    let ym = (y0 + y1) / 2.0;
    let tr = p.get("transom");
    if tr > 0.05 && z1 - tr > zb + f + 0.3 {
        let zt0 = z1 - tr;
        parts.bx([-w2 + f, y0, zt0 - f], [w2 - f, y1, zt0], color::FRAME);
        parts.bx([-w2 + f, ym - 0.006, zt0], [w2 - f, ym + 0.006, z1], color::GLASS);
        z1 = zt0 - f;
    }
    let bays = mullion_bays(-w2 + f, w2 - f, p.count("mullions"), f);
    for w in bays.windows(2) {
        if let [a, b] = w {
            parts.bx([a.1, y0, zb + f], [b.0, y1, z1], color::FRAME);
        }
    }
    for (a, b) in &bays {
        parts.bx([*a, ym - 0.006, zb + f], [*b, ym + 0.006, z1], color::GLASS);
    }
}

/// 3D parts of a window style.
pub(crate) fn parts(id: &str, parts: &mut Parts, host: &Host, p: &P) {
    match id {
        "window.casement_1" => casement_parts(parts, host, p, 1, false, false),
        "window.casement_3" => casement_parts(parts, host, p, 3, false, false),
        "window.tilt_turn" => casement_parts(parts, host, p, p.count("leaves"), false, false),
        "window.top_hung" => casement_parts(parts, host, p, 1, false, true),
        "window.sliding" => sliding_parts(parts, host, p, false),
        "window.fixed" => fixed_parts(parts, host, p),
        _ => casement_parts(parts, host, p, 2, false, false),
    }
}
