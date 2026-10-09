//! Doors.
//!
//! Canonical coordinates (see `host.rs`): leaves open towards the inner face `yi`; single leaves hinge on the
//! jamb at `x = −w/2`; sliding leaves park towards +x.

use crate::host::{Host, Swing};
use crate::params::P;
use crate::parts::{Parts, color};
use crate::sym::{Pen, Sym};
use crate::windows::{self, DOOR};

/// Height of door handles above the floor.
const HANDLE_Z: f64 = 1.05;
/// Gap under leaves without a threshold.
const FLOOR_GAP: f64 = 0.008;

/// Frame of a hinged door, flush with the face the door opens to.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    /// Face width.
    f: f64,
    /// Band across the wall `(y0, y1)`, `y1` on the opening side.
    y: (f64, f64),
}

impl Frame {
    fn swing(host: &Host, p: &P) -> Frame {
        let f = windows::frame_w(host, p);
        let fd = p.get("frame_depth").min(host.t());
        Frame { f, y: (host.yi - fd, host.yi) }
    }
    fn centred(host: &Host, p: &P) -> Frame {
        let f = windows::frame_w(host, p);
        let fd = p.get("frame_depth").min(host.t());
        Frame { f, y: (host.mid() - fd / 2.0, host.mid() + fd / 2.0) }
    }
    /// Clear width between the jambs `(x0, x1)`.
    fn clear(&self, host: &Host) -> (f64, f64) {
        (-host.w2() + self.f, host.w2() - self.f)
    }
}

/// The leaves of a hinged door: `(hinge x, direction, width)`.
fn hinged_leaves(id: &str, x0: f64, x1: f64, p: &P) -> Vec<(f64, f64, f64)> {
    let clear = x1 - x0;
    match id {
        "door.double_swing" => vec![(x0, 1.0, clear / 2.0), (x1, -1.0, clear / 2.0)],
        "door.unequal_double" => {
            let main = p.get("main_leaf").min(clear - 0.2).max(clear.min(0.3));
            vec![(x0, 1.0, main), (x1, -1.0, clear - main)]
        }
        _ => vec![(x0, 1.0, clear)],
    }
}

// ---------------------------------------------------------------------------------------------------------
// Plan

fn swing_symbol(id: &str, sym: &mut Sym, host: &Host, p: &P) {
    let fr = Frame::swing(host, p);
    let (x0, x1) = fr.clear(host);
    sym.layer(DOOR.main);
    windows::frame_symbol(sym, host, fr.f, fr.y.0, fr.y.1, &[]);
    let (thick, angle) = (p.get("leaf"), p.angle("swing"));
    for (hx, dir, len) in hinged_leaves(id, x0, x1, p) {
        Swing { hx, y: fr.y.1, dir, len, thick, angle }.draw(sym, Pen::Leaf, DOOR.main, DOOR.swing);
    }
    if p.flag("threshold") {
        windows::threshold_symbol(sym, host, &DOOR);
    }
}

fn entrance_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let fr = Frame::swing(host, p);
    let (x0, x1) = fr.clear(host);
    let sl = side_light(host, p, &fr);
    let xm = x1 - sl - fr.f;
    sym.layer(DOOR.main);
    windows::frame_symbol(sym, host, fr.f, fr.y.0, fr.y.1, &[(xm, xm + fr.f)]);
    let ym = (fr.y.0 + fr.y.1) / 2.0;
    sym.line((xm + fr.f, ym), (x1, ym), Pen::Fine);
    Swing { hx: x0, y: fr.y.1, dir: 1.0, len: xm - x0, thick: p.get("leaf"), angle: p.angle("swing") }.draw(
        sym,
        Pen::Leaf,
        DOOR.main,
        DOOR.swing,
    );
    if p.flag("threshold") {
        windows::threshold_symbol(sym, host, &DOOR);
    }
}

/// Width of the side light, leaving at least 0.6 m for the leaf.
fn side_light(host: &Host, p: &P, fr: &Frame) -> f64 {
    let (x0, x1) = fr.clear(host);
    p.get("sidelight").min(x1 - x0 - fr.f - 0.6).max(0.0)
}

/// Two leaves on parallel tracks inside the frame.
fn bypass_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let fr = Frame::centred(host, p);
    let (x0, x1) = fr.clear(host);
    sym.layer(DOOR.main);
    windows::frame_symbol(sym, host, fr.f, fr.y.0, fr.y.1, &[]);
    let lt = p.get("leaf");
    let yc = (fr.y.0 + fr.y.1) / 2.0;
    let mid = (x0 + x1) / 2.0;
    let leaves = [(x0, mid + 0.025, yc + 0.004, 1.0), (mid - 0.025, x1, yc - 0.004 - lt, -1.0)];
    for (a, b, y, _) in leaves {
        leaf_rect(sym, (a, b), (y, y + lt));
    }
    sym.layer(DOOR.swing);
    for (a, b, _, d) in leaves {
        let y = if d > 0.0 { host.yi + sym.mm(2.5) } else { host.ye - sym.mm(2.5) };
        let (c, half) = ((a + b) / 2.0, ((b - a) * 0.25).min(0.3));
        sym.arrow((c - d * half, y), (c + d * half, y), Pen::Fine);
    }
}

/// A leaf in plan: a rectangle at 1:50, its centre line at smaller scales.
fn leaf_rect(sym: &mut Sym, (x0, x1): (f64, f64), (y0, y1): (f64, f64)) {
    if sym.fine() {
        sym.rect(x0, y0, x1, y1, Pen::Leaf);
    } else {
        let y = (y0 + y1) / 2.0;
        sym.line((x0, y), (x1, y), Pen::Leaf);
    }
}

/// One leaf hung on a rail on the inner face, parking towards +x.
fn surface_sliding_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let w2 = host.w2();
    let lt = p.get("leaf");
    let lining = 0.02_f64.min(host.w / 8.0);
    sym.layer(DOOR.main);
    sym.rect(-w2, host.ye, -w2 + lining, host.yi, Pen::Frame);
    sym.rect(w2 - lining, host.ye, w2, host.yi, Pen::Frame);
    let y0 = host.yi + 0.01;
    leaf_rect(sym, (-w2 - 0.05, w2 + 0.05), (y0, y0 + lt));
    sym.layer(DOOR.above);
    sym.line((-w2 - 0.05, y0 + lt / 2.0), (w2 + host.w + 0.1, y0 + lt / 2.0), Pen::Above);
    sym.layer(DOOR.swing);
    let y = y0 + lt + sym.mm(2.5);
    let half = (host.w * 0.25).min(0.3);
    sym.arrow((-half, y), (half, y), Pen::Fine);
}

/// Leaves sliding into pockets inside the wall (one pocket towards +x, or one each side).
fn pocket_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let fr = Frame::centred(host, p);
    let w2 = host.w2();
    let lt = p.get("leaf");
    let yc = host.mid();
    let two = p.count("leaves") >= 2;
    sym.layer(DOOR.main);
    windows::frame_symbol(sym, host, fr.f, fr.y.0, fr.y.1, &[]);
    let (x0, x1) = fr.clear(host);
    let leaves: Vec<(f64, f64, f64)> =
        if two { vec![(x0, 0.0, -1.0), (0.0, x1, 1.0)] } else { vec![(x0, x1, 1.0)] };
    for (a, b, d) in &leaves {
        let (a, b) = if *d > 0.0 { (*a, b + 0.03) } else { (a - 0.03, *b) };
        leaf_rect(sym, (a, b), (yc - lt / 2.0, yc + lt / 2.0));
    }
    // The pockets, hidden inside the wall.
    for (a, b, d) in &leaves {
        let len = b - a + 0.05;
        let (p0, p1) = if *d > 0.0 { (w2, w2 + len) } else { (-w2 - len, -w2) };
        sym.rect(p0, yc - lt / 2.0 - 0.012, p1, yc + lt / 2.0 + 0.012, Pen::Hidden);
    }
    sym.layer(DOOR.swing);
    let y = host.yi + sym.mm(2.5);
    for (a, b, d) in &leaves {
        let (c, half) = ((a + b) / 2.0, ((b - a) * 0.25).min(0.3));
        sym.arrow((c - d * half, y), (c + d * half, y), Pen::Fine);
    }
}

/// Folding door: panels gathered towards the jamb at −x, drawn part-open as a zigzag.
fn folding_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let fr = Frame::swing(host, p);
    let (x0, x1) = fr.clear(host);
    let n = p.count("panels").max(2);
    let pw = (x1 - x0) / n as f64;
    let a = 60_f64.to_radians();
    let y = fr.y.1;
    sym.layer(DOOR.main);
    windows::frame_symbol(sym, host, fr.f, fr.y.0, fr.y.1, &[]);
    let pts: Vec<(f64, f64)> = (0..=n)
        .map(|k| (x0 + k as f64 * pw * a.cos(), if k % 2 == 1 { y + pw * a.sin() } else { y }))
        .collect();
    if sym.fine() {
        let lt = p.get("leaf");
        for w in pts.windows(2) {
            if let [s, e] = w {
                // Each panel as a thin rectangle on the side away from the opening.
                let (dx, dy) = (e.0 - s.0, e.1 - s.1);
                let l = dx.hypot(dy).max(1e-9);
                let (mut nx, mut ny) = (dy / l, -dx / l);
                if ny > 0.0 {
                    (nx, ny) = (-nx, -ny);
                }
                let (nx, ny) = (nx * lt, ny * lt);
                sym.poly(&[*s, *e, (e.0 + nx, e.1 + ny), (s.0 + nx, s.1 + ny)], true, Pen::Leaf);
            }
        }
    } else {
        sym.poly(&pts, false, Pen::Leaf);
    }
    sym.layer(DOOR.above);
    if let Some(end) = pts.last() {
        sym.line(*end, (x1, y), Pen::Above);
    }
    sym.layer(DOOR.swing);
    sym.arc((x0, y), pw, 0.0, a, Pen::Fine);
}

/// Sectional garage door: the closed door on the inner face, overhead tracks and the raised door dashed.
fn garage_symbol(sym: &mut Sym, host: &Host, p: &P) {
    let w2 = host.w2();
    let y = host.yi;
    let lt = p.get("leaf");
    sym.layer(DOOR.main);
    sym.rect(-w2, y - 0.04, -w2 + 0.04, y, Pen::Frame);
    sym.rect(w2 - 0.04, y - 0.04, w2, y, Pen::Frame);
    leaf_rect(sym, (-w2 - 0.04, w2 + 0.04), (y, y + lt));
    let reach = host.h + 0.3;
    sym.layer(DOOR.above);
    for x in [-w2 - 0.065, w2 + 0.065] {
        sym.line((x, y), (x, y + reach), Pen::Above);
    }
    sym.rect(-w2, y + 0.3, w2, y + reach, Pen::Above);
    sym.layer(DOOR.swing);
    sym.arrow(
        (0.0, y + lt + sym.mm(2.0)),
        (0.0, y + lt + sym.mm(2.0) + 0.5_f64.min(host.h / 3.0)),
        Pen::Fine,
    );
}

/// Plan symbol of a door style.
pub(crate) fn symbol(id: &str, sym: &mut Sym, host: &Host, p: &P) {
    match id {
        "door.sliding" => bypass_symbol(sym, host, p),
        "door.sliding_single" => surface_sliding_symbol(sym, host, p),
        "door.pocket" => pocket_symbol(sym, host, p),
        "door.folding" => folding_symbol(sym, host, p),
        "door.entrance" => entrance_symbol(sym, host, p),
        "door.balcony" => windows::casement_symbol(sym, host, p, p.count("leaves").clamp(1, 2), true),
        "door.balcony_sliding" => windows::sliding_symbol(sym, host, p, true),
        "door.garage" => garage_symbol(sym, host, p),
        _ => swing_symbol(id, sym, host, p),
    }
}

// ---------------------------------------------------------------------------------------------------------
// 3D

/// Jambs and head of a door frame.
fn frame_parts(parts: &mut Parts, host: &Host, fr: &Frame, mullion: Option<f64>) {
    let w2 = host.w2();
    let (y0, y1) = fr.y;
    let f = fr.f;
    parts.bx([-w2, y0, 0.0], [-w2 + f, y1, host.h], color::FRAME);
    parts.bx([w2 - f, y0, 0.0], [w2, y1, host.h], color::FRAME);
    parts.bx([-w2 + f, y0, host.h - f], [w2 - f, y1, host.h], color::FRAME);
    if let Some(x) = mullion {
        parts.bx([x, y0, 0.0], [x + f, y1, host.h - f], color::FRAME);
    }
}

/// A timber leaf with an optional glazed upper part.
fn leaf_parts(parts: &mut Parts, x: (f64, f64), y: (f64, f64), z: (f64, f64), glazing: f64) {
    let (x0, x1) = x;
    let (z0, z1) = z;
    let s = 0.12_f64.min((x1 - x0) / 4.0);
    let zg0 = (z0 + (z1 - z0) * (1.0 - glazing)).max(z0 + s);
    let zg1 = z1 - s;
    if glazing < 0.01 || zg1 < zg0 + 0.05 {
        parts.bx([x0, y.0, z0], [x1, y.1, z1], color::LEAF);
        return;
    }
    parts.bx([x0, y.0, z0], [x1, y.1, zg0], color::LEAF);
    parts.bx([x0, y.0, zg1], [x1, y.1, z1], color::LEAF);
    parts.bx([x0, y.0, zg0], [x0 + s, y.1, zg1], color::LEAF);
    parts.bx([x1 - s, y.0, zg0], [x1, y.1, zg1], color::LEAF);
    let ym = (y.0 + y.1) / 2.0;
    parts.bx([x0 + s, ym - 0.006, zg0], [x1 - s, ym + 0.006, zg1], color::GLASS);
}

/// A lever handle on both faces of a leaf, at `x`, pointing towards the hinge (`to_hinge` = ±1).
fn lever(parts: &mut Parts, x: f64, to_hinge: f64, (ya, yb): (f64, f64), z: f64) {
    for (face, out) in [(ya.min(yb), -1.0), (ya.max(yb), 1.0)] {
        parts.bx([x - 0.025, face, z - 0.035], [x + 0.025, face + out * 0.01, z + 0.035], color::METAL);
        parts.bx(
            [x - 0.009, face + out * 0.01, z - 0.009],
            [x + 0.009, face + out * 0.05, z + 0.009],
            color::METAL,
        );
        parts.bx(
            [x - to_hinge * 0.01, face + out * 0.04, z - 0.01],
            [x + to_hinge * 0.13, face + out * 0.06, z + 0.01],
            color::METAL,
        );
    }
}

/// A flush pull or bar handle on both faces of a sliding leaf.
fn pull(parts: &mut Parts, x: f64, (ya, yb): (f64, f64), z: f64) {
    for (face, out) in [(ya.min(yb), -1.0), (ya.max(yb), 1.0)] {
        parts.bx([x - 0.012, face, z - 0.12], [x + 0.012, face + out * 0.004, z + 0.12], color::DARK);
    }
}

fn swing_parts(id: &str, parts: &mut Parts, host: &Host, p: &P) {
    let fr = Frame::swing(host, p);
    let (x0, x1) = fr.clear(host);
    frame_parts(parts, host, &fr, None);
    let z0 = if p.flag("threshold") { windows::threshold_parts(parts, host) } else { FLOOR_GAP };
    let lt = p.get("leaf");
    let y = (fr.y.1 - lt, fr.y.1);
    let z1 = host.h - fr.f;
    let glazing = p.fraction("glazing");
    let leaves = hinged_leaves(id, x0, x1, p);
    for (i, (hx, dir, len)) in leaves.iter().enumerate() {
        let (a, b) = if *dir > 0.0 { (*hx, hx + len) } else { (hx - len, *hx) };
        leaf_parts(parts, (a + 0.002, b - 0.002), y, (z0, z1), glazing);
        if p.flag("handle") && i == 0 {
            let free = hx + dir * len;
            lever(parts, free - dir * 0.065, -dir, y, HANDLE_Z.min(z1 - 0.1));
        }
    }
}

fn entrance_parts(parts: &mut Parts, host: &Host, p: &P) {
    let fr = Frame::swing(host, p);
    let (x0, x1) = fr.clear(host);
    let sl = side_light(host, p, &fr);
    let xm = x1 - sl - fr.f;
    frame_parts(parts, host, &fr, Some(xm));
    let z0 = if p.flag("threshold") { windows::threshold_parts(parts, host) } else { FLOOR_GAP };
    let lt = p.get("leaf");
    let y = (fr.y.1 - lt, fr.y.1);
    let z1 = host.h - fr.f;
    leaf_parts(parts, (x0 + 0.002, xm - 0.002), y, (z0, z1), p.fraction("glazing"));
    if p.flag("handle") {
        lever(parts, xm - 0.07, -1.0, y, HANDLE_Z.min(z1 - 0.1));
        // A long pull bar on the outer face.
        let yo = y.0 - 0.06;
        parts.bx(
            [xm - 0.09, yo, z0 + 0.6],
            [xm - 0.06, yo + 0.03, z0 + 1.6_f64.min(z1 - z0 - 0.1)],
            color::METAL,
        );
        parts.bx([xm - 0.085, yo + 0.03, z0 + 0.7], [xm - 0.065, y.0, z0 + 0.72], color::METAL);
        parts.bx(
            [xm - 0.085, yo + 0.03, z0 + 1.48_f64.min(z1 - z0 - 0.2)],
            [xm - 0.065, y.0, z0 + 1.5_f64.min(z1 - z0 - 0.18)],
            color::METAL,
        );
    }
    if sl > 0.05 {
        let ym = (fr.y.0 + fr.y.1) / 2.0;
        parts.bx([xm + fr.f, fr.y.0, 0.0], [x1, fr.y.1, 0.12], color::FRAME);
        parts.bx([xm + fr.f, ym - 0.008, 0.12], [x1, ym + 0.008, z1], color::GLASS);
    }
}

fn bypass_parts(parts: &mut Parts, host: &Host, p: &P) {
    let fr = Frame::centred(host, p);
    let (x0, x1) = fr.clear(host);
    frame_parts(parts, host, &fr, None);
    let lt = p.get("leaf");
    let yc = (fr.y.0 + fr.y.1) / 2.0;
    let mid = (x0 + x1) / 2.0;
    let z = (FLOOR_GAP, host.h - fr.f);
    let glazing = p.fraction("glazing");
    let a = (yc + 0.004, yc + 0.004 + lt);
    let b = (yc - 0.004 - lt, yc - 0.004);
    leaf_parts(parts, (x0, mid + 0.025), a, z, glazing);
    leaf_parts(parts, (mid - 0.025, x1), b, z, glazing);
    if p.flag("handle") {
        let zh = HANDLE_Z.min(z.1 - 0.15);
        pull(parts, x0 + 0.06, a, zh);
        pull(parts, x1 - 0.06, b, zh);
    }
}

fn surface_sliding_parts(parts: &mut Parts, host: &Host, p: &P) {
    let w2 = host.w2();
    let lt = p.get("leaf");
    let lining = 0.02_f64.min(host.w / 8.0);
    parts.bx([-w2, host.ye, 0.0], [-w2 + lining, host.yi, host.h], color::FRAME);
    parts.bx([w2 - lining, host.ye, 0.0], [w2, host.yi, host.h], color::FRAME);
    parts.bx([-w2 + lining, host.ye, host.h - lining], [w2 - lining, host.yi, host.h], color::FRAME);
    let y = (host.yi + 0.01, host.yi + 0.01 + lt);
    leaf_parts(parts, (-w2 - 0.05, w2 + 0.05), y, (FLOOR_GAP, host.h + 0.04), p.fraction("glazing"));
    // Rail above the opening, long enough for the leaf to park beside it.
    parts.bx(
        [-w2 - 0.06, host.yi, host.h + 0.06],
        [w2 + host.w + 0.1, host.yi + 0.03, host.h + 0.1],
        color::METAL,
    );
    if p.flag("handle") {
        let x = -w2 + 0.04;
        parts.bx([x - 0.012, y.1, 0.85], [x + 0.012, y.1 + 0.035, 1.25], color::METAL);
    }
}

fn pocket_parts(parts: &mut Parts, host: &Host, p: &P) {
    let fr = Frame::centred(host, p);
    let (x0, x1) = fr.clear(host);
    frame_parts(parts, host, &fr, None);
    let lt = p.get("leaf");
    let yc = host.mid();
    let y = (yc - lt / 2.0, yc + lt / 2.0);
    let z = (FLOOR_GAP, host.h - fr.f);
    let glazing = p.fraction("glazing");
    let zh = HANDLE_Z.min(z.1 - 0.15);
    if p.count("leaves") >= 2 {
        leaf_parts(parts, (x0, -0.002), y, z, glazing);
        leaf_parts(parts, (0.002, x1), y, z, glazing);
        if p.flag("handle") {
            pull(parts, -0.05, y, zh);
            pull(parts, 0.05, y, zh);
        }
    } else {
        leaf_parts(parts, (x0 + 0.002, x1), y, z, glazing);
        if p.flag("handle") {
            pull(parts, x0 + 0.06, y, zh);
        }
    }
}

fn folding_parts(parts: &mut Parts, host: &Host, p: &P) {
    let fr = Frame::swing(host, p);
    let (x0, x1) = fr.clear(host);
    frame_parts(parts, host, &fr, None);
    let n = p.count("panels").max(2);
    let pw = (x1 - x0) / n as f64;
    let lt = p.get("leaf");
    let y = (fr.y.1 - lt, fr.y.1);
    let z1 = host.h - fr.f;
    for k in 0..n {
        let a = x0 + k as f64 * pw;
        leaf_parts(parts, (a + 0.003, a + pw - 0.003), y, (FLOOR_GAP, z1), p.fraction("glazing"));
    }
    if p.flag("handle") {
        lever(parts, x1 - 0.06, -1.0, y, HANDLE_Z.min(z1 - 0.1));
    }
}

fn garage_parts(parts: &mut Parts, host: &Host, p: &P) {
    let w2 = host.w2();
    let y = host.yi;
    let lt = p.get("leaf");
    let panel = kanavos_drawing::Rgba::rgb(206, 208, 209);
    parts.bx([-w2, y - 0.04, 0.0], [-w2 + 0.04, y, host.h], color::METAL);
    parts.bx([w2 - 0.04, y - 0.04, 0.0], [w2, y, host.h], color::METAL);
    let n = p.count("sections").max(1);
    let sh = (host.h + 0.04) / n as f64;
    for k in 0..n {
        let z0 = k as f64 * sh;
        parts.bx([-w2 - 0.04, y + 0.004, z0 + 0.002], [w2 + 0.04, y + 0.004 + lt, z0 + sh - 0.002], panel);
    }
    // Vertical and overhead tracks.
    let reach = host.h + 0.3;
    for x in [-w2 - 0.065, w2 + 0.065] {
        parts.bx([x - 0.025, y, 0.0], [x + 0.025, y + 0.08, host.h + 0.1], color::METAL);
        parts.bx([x - 0.025, y + 0.08, host.h + 0.05], [x + 0.025, y + reach, host.h + 0.1], color::METAL);
    }
    if p.flag("handle") {
        parts.bx([-0.1, y + 0.004 + lt, 0.25], [0.1, y + lt + 0.03, 0.29], color::DARK);
    }
}

/// 3D parts of a door style.
pub(crate) fn parts(id: &str, parts: &mut Parts, host: &Host, p: &P) {
    match id {
        "door.sliding" => bypass_parts(parts, host, p),
        "door.sliding_single" => surface_sliding_parts(parts, host, p),
        "door.pocket" => pocket_parts(parts, host, p),
        "door.folding" => folding_parts(parts, host, p),
        "door.entrance" => entrance_parts(parts, host, p),
        "door.balcony" => windows::casement_parts(parts, host, p, p.count("leaves").clamp(1, 2), true, false),
        "door.balcony_sliding" => windows::sliding_parts(parts, host, p, true),
        "door.garage" => garage_parts(parts, host, p),
        _ => swing_parts(id, parts, host, p),
    }
}
