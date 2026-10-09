//! Furniture: beds, storage, seating, tables and the garden set.
//!
//! Local coordinates follow [`crate::Bx`]: front edge at `y0` (the side a person approaches from), back at `y1`.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use kanavos_drawing::Rgba;
use kanavos_geom::Point3;

use crate::Bx;
use crate::params::P;
use crate::parts::{Parts, color};
use crate::shape::{rotate, rrect_pts};
use crate::sym::{Pen, Sym, placement};

/// Seat width and depth of dining chairs.
pub(crate) const CHAIR: (f64, f64) = (0.44, 0.46);
/// Gap between a table edge and the front of its chairs.
const CHAIR_GAP: f64 = 0.04;

// ---------------------------------------------------------------------------------------------------------
// Chairs

/// A chair in plan, centred on the origin, the sitter facing −y (backrest at +y).
pub(crate) fn chair_symbol(sym: &mut Sym, w: f64, d: f64) {
    let (x0, x1, y0, y1) = (-w / 2.0, w / 2.0, -d / 2.0, d / 2.0);
    let back = 0.05_f64.min(d / 4.0);
    sym.rrect(x0, y0, x1, y1 - back, 0.035, Pen::Fine);
    sym.rrect(x0, y1 - back, x1, y1, 0.01, Pen::Fine);
}

/// A chair in 3D, centred on the origin, the sitter facing −y.
pub(crate) fn chair_parts(parts: &mut Parts, (w, d, h): (f64, f64, f64), wood: Rgba, seat: Rgba) {
    let (x0, x1, y0, y1) = (-w / 2.0, w / 2.0, -d / 2.0, d / 2.0);
    let leg = 0.035_f64.min(w / 6.0);
    let zs = 0.45_f64.min(h * 0.55);
    for (lx, ly) in [(x0, y0), (x1 - leg, y0)] {
        parts.bx([lx, ly, 0.0], [lx + leg, ly + leg, zs - 0.04], wood);
    }
    // Rear legs run up into the backrest posts.
    for lx in [x0, x1 - leg] {
        parts.bx([lx, y1 - leg, 0.0], [lx + leg, y1, h], wood);
    }
    parts.bx([x0, y0, zs - 0.04], [x1, y1 - leg, zs], seat);
    parts.bx([x0 + leg, y1 - 0.03, h - 0.24], [x1 - leg, y1 - 0.008, h - 0.03], wood);
}

/// Centres and facing angles of the chairs around a rectangular table.
fn rect_table_seats(b: &Bx, n: usize) -> Vec<(f64, f64, f64)> {
    let gap = CHAIR_GAP + CHAIR.1 / 2.0;
    let per_side = ((b.w() - 0.1) / 0.55).floor().max(1.0) as usize;
    let n = n.min(2 * per_side + 2);
    let ends = n.saturating_sub(2 * per_side).min(2);
    let long = n - ends;
    let (front, back) = (long.div_ceil(2), long / 2);
    let mut seats = Vec::with_capacity(n);
    for k in 0..front {
        seats.push((b.x0 + b.w() * (k as f64 + 0.5) / front as f64, b.y0 - gap, PI));
    }
    for k in 0..back {
        seats.push((b.x0 + b.w() * (k as f64 + 0.5) / back as f64, b.y1 + gap, 0.0));
    }
    if ends >= 1 {
        seats.push((b.x0 - gap, b.cy(), FRAC_PI_2));
    }
    if ends >= 2 {
        seats.push((b.x1 + gap, b.cy(), -FRAC_PI_2));
    }
    seats
}

/// Centres and facing angles of the chairs around a round or oval table.
fn round_table_seats(b: &Bx, n: usize) -> Vec<(f64, f64, f64)> {
    let gap = CHAIR_GAP + CHAIR.1 / 2.0;
    let (rx, ry) = (b.w() / 2.0 + gap, b.d() / 2.0 + gap);
    (0..n)
        .map(|k| {
            let t = -FRAC_PI_2 + TAU * k as f64 / n.max(1) as f64;
            (b.cx() + rx * t.cos(), b.cy() + ry * t.sin(), t - FRAC_PI_2)
        })
        .collect()
}

fn chairs_symbol(sym: &mut Sym, seats: &[(f64, f64, f64)]) {
    for (x, y, a) in seats {
        let mut c = sym.sibling();
        chair_symbol(&mut c, CHAIR.0, CHAIR.1);
        sym.append_mapped(c, &placement(*x, *y, *a));
    }
}

fn chairs_parts(parts: &mut Parts, seats: &[(f64, f64, f64)], wood: Rgba, seat: Rgba) {
    let mut chair = Parts::new();
    chair_parts(&mut chair, (CHAIR.0, CHAIR.1, 0.88), wood, seat);
    for (x, y, a) in seats {
        let (x, y, a) = (*x, *y, *a);
        parts.add_mapped(chair.clone(), move |q| {
            let (rx, ry) = rotate((q.x, q.y), a);
            Point3::new(x + rx, y + ry, q.z)
        });
    }
}

// ---------------------------------------------------------------------------------------------------------
// Beds

const HEADBOARD: f64 = 0.06;

fn pillows(b: &Bx) -> Vec<(f64, f64, f64, f64)> {
    let n = if b.w() >= 1.2 { 2 } else { 1 };
    let gap = 0.08;
    let pw = (b.w() - 0.2 - (n - 1) as f64 * gap) / n as f64;
    let y1 = b.y1 - HEADBOARD - 0.06;
    let y0 = (y1 - 0.36).max(b.y0 + 0.2);
    (0..n)
        .map(|k| {
            let x = b.x0 + 0.1 + k as f64 * (pw + gap);
            (x, y0, x + pw, y1)
        })
        .collect()
}

fn bed_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y1 - HEADBOARD), (b.x1, b.y1 - HEADBOARD), Pen::Fine);
    for (x0, y0, x1, y1) in pillows(b) {
        sym.rrect(x0, y0, x1, y1, 0.06, Pen::Fine);
    }
    if sym.fine() {
        // The duvet's top edge with one corner turned back.
        let yf = b.y1 - HEADBOARD - 0.5;
        let a = 0.35_f64.min(b.w() / 3.0);
        sym.poly(&[(b.x0, yf), (b.x1 - a, yf), (b.x1, yf - a)], false, Pen::Fine);
        sym.line((b.x1 - a, yf), (b.x1, yf), Pen::Detail);
    }
}

fn bed_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.3);
    let leg = 0.05;
    for (x, y) in [
        (b.x0, b.y0),
        (b.x1 - leg, b.y0),
        (b.x0, b.y1 - HEADBOARD - leg),
        (b.x1 - leg, b.y1 - HEADBOARD - leg),
    ] {
        parts.bx([x, y, 0.0], [x + leg, y + leg, 0.1], color::WALNUT);
    }
    let base = h - 0.22;
    parts.bx([b.x0, b.y0, 0.1], [b.x1, b.y1 - HEADBOARD, base], color::OAK);
    parts.bx([b.x0 + 0.015, b.y0 + 0.015, base], [b.x1 - 0.015, b.y1 - HEADBOARD - 0.01, h], color::LINEN);
    parts.bx([b.x0, b.y1 - HEADBOARD, 0.0], [b.x1, b.y1, h + 0.5], color::OAK);
    for (x0, y0, x1, y1) in pillows(b) {
        parts.bx([x0, y0, h], [x1, y1, h + 0.1], color::LINEN);
    }
    let yf = b.y1 - HEADBOARD - 0.5;
    parts.bx([b.x0 - 0.01, b.y0 - 0.01, h - 0.18], [b.x1 + 0.01, yf, h + 0.04], color::DUVET);
}

fn cot_symbol(sym: &mut Sym, b: &Bx) {
    let r = 0.04_f64.min(b.w() / 6.0);
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.rect(b.x0 + r, b.y0 + r, b.x1 - r, b.y1 - r, Pen::Fine);
    if sym.fine() {
        // Bars along the long sides.
        let n = ((b.d() - 2.0 * r) / 0.08).floor().max(1.0) as usize;
        for k in 1..n {
            let y = b.y0 + r + k as f64 * (b.d() - 2.0 * r) / n as f64;
            sym.line((b.x0, y), (b.x0 + r, y), Pen::Detail);
            sym.line((b.x1 - r, y), (b.x1, y), Pen::Detail);
        }
    }
}

fn cot_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.5);
    let r = 0.04_f64.min(b.w() / 6.0);
    for (x, y) in [(b.x0, b.y0), (b.x1 - r, b.y0), (b.x0, b.y1 - r), (b.x1 - r, b.y1 - r)] {
        parts.bx([x, y, 0.0], [x + r, y + r, h], color::WHITE);
    }
    let zm = 0.3;
    // Top and bottom rails.
    for (z0, z1) in [(zm - 0.04, zm), (h - 0.04, h)] {
        parts.bx([b.x0 + r, b.y0, z0], [b.x1 - r, b.y0 + r, z1], color::WHITE);
        parts.bx([b.x0 + r, b.y1 - r, z0], [b.x1 - r, b.y1, z1], color::WHITE);
        parts.bx([b.x0, b.y0 + r, z0], [b.x0 + r, b.y1 - r, z1], color::WHITE);
        parts.bx([b.x1 - r, b.y0 + r, z0], [b.x1, b.y1 - r, z1], color::WHITE);
    }
    // Bars.
    let bar = 0.02;
    let n = ((b.d() - 2.0 * r) / 0.08).floor().max(1.0) as usize;
    for k in 1..n {
        let y = b.y0 + r + k as f64 * (b.d() - 2.0 * r) / n as f64;
        for x in [b.x0 + r / 2.0, b.x1 - r / 2.0] {
            parts.bx(
                [x - bar / 2.0, y - bar / 2.0, zm],
                [x + bar / 2.0, y + bar / 2.0, h - 0.04],
                color::WHITE,
            );
        }
    }
    let m = ((b.w() - 2.0 * r) / 0.08).floor().max(1.0) as usize;
    for k in 1..m {
        let x = b.x0 + r + k as f64 * (b.w() - 2.0 * r) / m as f64;
        for y in [b.y0 + r / 2.0, b.y1 - r / 2.0] {
            parts.bx(
                [x - bar / 2.0, y - bar / 2.0, zm],
                [x + bar / 2.0, y + bar / 2.0, h - 0.04],
                color::WHITE,
            );
        }
    }
    parts.bx([b.x0 + r, b.y0 + r, zm - 0.04], [b.x1 - r, b.y1 - r, zm + 0.1], color::LINEN);
}

// ---------------------------------------------------------------------------------------------------------
// Storage

fn nightstand_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.02), (b.x1, b.y0 + 0.02), Pen::Detail);
    let r = (b.w().min(b.d()) / 4.0).min(0.11);
    sym.circle((b.cx(), b.cy() + 0.02), r, Pen::Fine);
    if sym.fine() {
        sym.circle((b.cx(), b.cy() + 0.02), r * 0.3, Pen::Detail);
    }
}

fn legs(parts: &mut Parts, b: &Bx, size: f64, z: f64, inset: f64, c: Rgba) {
    let (x0, x1, y0, y1) = (b.x0 + inset, b.x1 - inset - size, b.y0 + inset, b.y1 - inset - size);
    for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        parts.bx([x, y, 0.0], [x + size, y + size, z], c);
    }
}

/// Drawer fronts stacked between `z0` and `z1` with a bar handle each.
fn drawers(parts: &mut Parts, b: &Bx, n: usize, (z0, z1): (f64, f64), front: Rgba) {
    let n = n.max(1);
    let dh = (z1 - z0) / n as f64;
    for k in 0..n {
        let za = z0 + k as f64 * dh;
        parts.bx([b.x0 + 0.003, b.y0, za + 0.002], [b.x1 - 0.003, b.y0 + 0.02, za + dh - 0.002], front);
        let hw = (b.w() * 0.15).min(0.08);
        parts.bx(
            [b.cx() - hw, b.y0 - 0.02, za + dh / 2.0 - 0.006],
            [b.cx() + hw, b.y0, za + dh / 2.0 + 0.006],
            color::METAL,
        );
    }
}

fn nightstand_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.2);
    legs(parts, b, 0.03, 0.1, 0.02, color::WALNUT);
    parts.bx([b.x0, b.y0 + 0.02, 0.1], [b.x1, b.y1, h], color::WALNUT);
    drawers(parts, b, 2, (0.12, h - 0.02), color::OAK);
    let (cx, cy) = (b.cx(), b.cy() + 0.02);
    let r = (b.w().min(b.d()) / 4.0).min(0.11);
    parts.cyl((cx, cy), r * 0.45, (h, h + 0.03), color::WHITE);
    parts.cyl((cx, cy), 0.012, (h + 0.03, h + 0.25), color::METAL);
    parts.cyl((cx, cy), r, (h + 0.22, h + 0.4), color::LINEN);
}

fn wardrobe_symbol(sym: &mut Sym, b: &Bx, doors: usize) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.02), (b.x1, b.y0 + 0.02), Pen::Detail);
    let n = doors.max(1);
    for k in 1..n {
        let x = b.x0 + b.w() * k as f64 / n as f64;
        sym.line((x, b.y0), (x, b.y0 + 0.02), Pen::Detail);
    }
    // Hanging rail with hangers.
    let yc = b.y0 + 0.02 + (b.d() - 0.02) / 2.0;
    sym.line((b.x0 + 0.03, yc), (b.x1 - 0.03, yc), Pen::Detail);
    if sym.fine() {
        let half = ((b.d() - 0.1) / 2.0).clamp(0.05, 0.21);
        let mut x = b.x0 + 0.1;
        while x < b.x1 - 0.08 {
            sym.line((x - 0.035, yc - half), (x + 0.035, yc + half), Pen::Detail);
            x += 0.12;
        }
    }
}

fn wardrobe_parts(parts: &mut Parts, b: &Bx, doors: usize) {
    let h = b.h.max(0.5);
    parts.bx([b.x0 + 0.02, b.y0 + 0.06, 0.0], [b.x1 - 0.02, b.y1, 0.08], color::DARK);
    parts.bx([b.x0, b.y0 + 0.02, 0.08], [b.x1, b.y1, h], color::WHITE);
    let n = doors.max(1);
    let dw = b.w() / n as f64;
    for k in 0..n {
        let xa = b.x0 + k as f64 * dw;
        parts.bx([xa + 0.002, b.y0, 0.083], [xa + dw - 0.002, b.y0 + 0.02, h - 0.003], color::FRONT);
        // Handles at the meeting edges.
        let hx = if k % 2 == 0 { xa + dw - 0.05 } else { xa + 0.05 };
        if n > 1 || k == 0 {
            parts.bx([hx - 0.008, b.y0 - 0.025, 0.9], [hx + 0.008, b.y0, 1.3_f64.min(h - 0.1)], color::METAL);
        }
    }
}

fn chest_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.02), (b.x1, b.y0 + 0.02), Pen::Detail);
    if sym.fine() {
        let hw = (b.w() * 0.15).min(0.08);
        sym.rect(b.cx() - hw, b.y0 - 0.02, b.cx() + hw, b.y0, Pen::Detail);
    }
}

fn chest_parts(parts: &mut Parts, b: &Bx, n: usize) {
    let h = b.h.max(0.3);
    legs(parts, b, 0.035, 0.1, 0.02, color::WALNUT);
    parts.bx([b.x0, b.y0 + 0.02, 0.1], [b.x1, b.y1, h], color::OAK);
    drawers(parts, b, n, (0.12, h - 0.02), color::FRONT);
}

fn bookcase_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0 + 0.02, b.y1 - 0.015), (b.x1 - 0.02, b.y1 - 0.015), Pen::Detail);
    let bays = (b.w() / 0.8).round().max(1.0) as usize;
    for k in 1..bays {
        let x = b.x0 + b.w() * k as f64 / bays as f64;
        sym.line((x, b.y0), (x, b.y1 - 0.015), Pen::Detail);
    }
    sym.line((b.x0 + 0.02, b.y0), (b.x0 + 0.02, b.y1), Pen::Detail);
    sym.line((b.x1 - 0.02, b.y0), (b.x1 - 0.02, b.y1), Pen::Detail);
    // Tall furniture carries a diagonal.
    if b.h > 1.5 {
        sym.line((b.x0, b.y0), (b.x1, b.y1 - 0.015), Pen::Detail);
    }
}

fn bookcase_parts(parts: &mut Parts, b: &Bx, shelves: usize) {
    let h = b.h.max(0.3);
    let t = 0.02;
    parts.bx([b.x0, b.y0, 0.0], [b.x0 + t, b.y1, h], color::OAK);
    parts.bx([b.x1 - t, b.y0, 0.0], [b.x1, b.y1, h], color::OAK);
    parts.bx([b.x0 + t, b.y0, h - t], [b.x1 - t, b.y1, h], color::OAK);
    parts.bx([b.x0 + t, b.y0 + 0.02, 0.0], [b.x1 - t, b.y1, 0.08], color::OAK);
    parts.bx([b.x0 + t, b.y1 - 0.01, 0.08], [b.x1 - t, b.y1, h - t], color::OAK);
    let bays = (b.w() / 0.8).round().max(1.0) as usize;
    let bw = (b.w() - 2.0 * t) / bays as f64;
    for k in 1..bays {
        let x = b.x0 + t + k as f64 * bw;
        parts.bx([x - t / 2.0, b.y0, 0.08], [x + t / 2.0, b.y1 - 0.01, h - t], color::OAK);
    }
    let n = shelves.max(1);
    let gap = (h - t - 0.08) / (n + 1) as f64;
    let mut seed = 7u32;
    for level in 0..=n {
        let z = 0.08 + level as f64 * gap;
        if level > 0 {
            parts.bx([b.x0 + t, b.y0 + 0.01, z - t / 2.0], [b.x1 - t, b.y1 - 0.01, z + t / 2.0], color::OAK);
        }
        if level == n || gap < 0.15 {
            continue;
        }
        // A few books on every other shelf.
        if level % 2 == 1 {
            continue;
        }
        for bay in 0..bays {
            let mut x = b.x0 + t + bay as f64 * bw + 0.03;
            let end = x + bw * 0.6;
            let mut i = 0usize;
            while x < end {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let r = f64::from((seed >> 16) & 0xff) / 255.0;
                let bwid = 0.06 + 0.1 * r;
                let bh = (0.17 + 0.1 * r).min(gap - 0.04);
                let c = color::BOOKS.get(i % 3).copied().unwrap_or(color::OAK);
                parts.bx(
                    [x, b.y0 + 0.04, z + t / 2.0],
                    [(x + bwid).min(end), b.y1 - 0.02, z + t / 2.0 + bh],
                    c,
                );
                x += bwid + 0.004;
                i += 1;
            }
        }
    }
}

fn tv_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.02), (b.x1, b.y0 + 0.02), Pen::Detail);
    if sym.fine() {
        let tw = (b.w() - 0.3).clamp(0.3, 1.25);
        let y = b.y1 - 0.18_f64.min(b.d() / 2.0);
        sym.rect(b.cx() - tw / 2.0, y - 0.03, b.cx() + tw / 2.0, y + 0.03, Pen::Fine);
        sym.rect(b.cx() - 0.15, y - 0.09, b.cx() + 0.15, y + 0.06, Pen::Detail);
    }
}

fn tv_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.2);
    legs(parts, b, 0.03, 0.1, 0.03, color::DARK);
    parts.bx([b.x0, b.y0 + 0.02, 0.1], [b.x1, b.y1, h], color::WALNUT);
    let n = (b.w() / 0.6).round().max(1.0) as usize;
    let dw = b.w() / n as f64;
    for k in 0..n {
        let xa = b.x0 + k as f64 * dw;
        parts.bx([xa + 0.002, b.y0, 0.103], [xa + dw - 0.002, b.y0 + 0.02, h - 0.003], color::OAK);
    }
    let tw = (b.w() - 0.3).clamp(0.3, 1.25);
    let y = b.y1 - 0.18_f64.min(b.d() / 2.0);
    parts.bx([b.cx() - 0.15, y - 0.09, h], [b.cx() + 0.15, y + 0.06, h + 0.012], color::DARK);
    parts.bx([b.cx() - 0.03, y + 0.0, h], [b.cx() + 0.03, y + 0.03, h + 0.12], color::DARK);
    parts.bx(
        [b.cx() - tw / 2.0, y - 0.025, h + 0.08],
        [b.cx() + tw / 2.0, y + 0.0, h + 0.08 + tw * 0.57],
        color::DARK,
    );
}

// ---------------------------------------------------------------------------------------------------------
// Seating

/// Width of sofa arms and depth of their backrests.
fn sofa_dims(b: &Bx, arm: f64) -> (f64, f64) {
    (arm.min(b.w() / 6.0), 0.22_f64.min(b.d() / 3.0))
}

fn sofa_symbol(sym: &mut Sym, b: &Bx, seats: usize, arm: f64) {
    let (aw, bd) = sofa_dims(b, arm);
    sym.rrect(b.x0, b.y0, b.x1, b.y1, 0.04, Pen::Fine);
    sym.line((b.x0 + aw, b.y1 - bd), (b.x1 - aw, b.y1 - bd), Pen::Fine);
    sym.line((b.x0 + aw, b.y0), (b.x0 + aw, b.y1 - bd), Pen::Fine);
    sym.line((b.x1 - aw, b.y0), (b.x1 - aw, b.y1 - bd), Pen::Fine);
    let n = seats.max(1);
    let cw = (b.w() - 2.0 * aw) / n as f64;
    for k in 1..n {
        let x = b.x0 + aw + k as f64 * cw;
        sym.line((x, b.y0 + 0.03), (x, b.y1 - bd), Pen::Detail);
    }
    if sym.fine() {
        sym.line((b.x0 + aw, b.y0 + 0.03), (b.x1 - aw, b.y0 + 0.03), Pen::Detail);
    }
}

fn sofa_parts(parts: &mut Parts, b: &Bx, seats: usize, arm: f64) {
    let (aw, bd) = sofa_dims(b, arm);
    let h = b.h.max(0.5);
    legs(parts, b, 0.04, 0.1, 0.03, color::DARK);
    parts.bx([b.x0, b.y0, 0.1], [b.x1, b.y1, 0.3], color::FABRIC);
    parts.bx([b.x0, b.y0, 0.3], [b.x0 + aw, b.y1, 0.62], color::FABRIC);
    parts.bx([b.x1 - aw, b.y0, 0.3], [b.x1, b.y1, 0.62], color::FABRIC);
    parts.bx([b.x0 + aw, b.y1 - bd, 0.3], [b.x1 - aw, b.y1, h], color::FABRIC);
    let n = seats.max(1);
    let cw = (b.w() - 2.0 * aw) / n as f64;
    for k in 0..n {
        let xa = b.x0 + aw + k as f64 * cw;
        parts.bx([xa + 0.005, b.y0 + 0.01, 0.3], [xa + cw - 0.005, b.y1 - bd, 0.44], color::CUSHION);
        parts.bx([xa + 0.01, b.y1 - bd - 0.13, 0.44], [xa + cw - 0.01, b.y1 - bd, h - 0.06], color::CUSHION);
    }
}

fn corner_sofa_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    let (aw, bd) = sofa_dims(b, 0.2);
    let sd = p.get("seat_depth").min(b.w().min(b.d()) * 0.6);
    let (x0, y0, x1, y1) = (b.x0, b.y0, b.x1, b.y1);
    sym.poly(
        &[(x0, y0), (x0 + sd, y0), (x0 + sd, y1 - sd), (x1, y1 - sd), (x1, y1), (x0, y1)],
        true,
        Pen::Fine,
    );
    // Backrests along the back and the left side, arms at both free ends.
    sym.poly(&[(x0 + bd, y0 + aw), (x0 + bd, y1 - bd), (x1 - aw, y1 - bd)], false, Pen::Fine);
    sym.line((x1 - aw, y1 - sd), (x1 - aw, y1 - bd), Pen::Fine);
    sym.line((x0 + bd, y0 + aw), (x0 + sd, y0 + aw), Pen::Fine);
    // Seat cushions: the corner seat, then the back and side runs.
    sym.line((x0 + sd, y1 - sd), (x0 + sd, y1 - bd), Pen::Detail);
    sym.line((x0 + bd, y1 - sd), (x0 + sd, y1 - sd), Pen::Detail);
    let n = p.count("seats").max(1);
    let run = x1 - aw - (x0 + sd);
    for k in 1..n {
        let x = x0 + sd + run * k as f64 / n as f64;
        sym.line((x, y1 - sd), (x, y1 - bd), Pen::Detail);
    }
    let side = y1 - sd - (y0 + aw);
    let m = (side / 0.65).round().max(1.0) as usize;
    for k in 1..m {
        let y = y0 + aw + side * k as f64 / m as f64;
        sym.line((x0 + bd, y), (x0 + sd, y), Pen::Detail);
    }
}

fn corner_sofa_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let (aw, bd) = sofa_dims(b, 0.2);
    let sd = p.get("seat_depth").min(b.w().min(b.d()) * 0.6);
    let h = b.h.max(0.5);
    let (x0, y0, x1, y1) = (b.x0, b.y0, b.x1, b.y1);
    for (x, y) in [
        (x0 + 0.03, y0 + 0.03),
        (x0 + sd - 0.07, y0 + 0.03),
        (x1 - 0.07, y1 - sd + 0.03),
        (x1 - 0.07, y1 - 0.07),
        (x0 + 0.03, y1 - 0.07),
    ] {
        parts.bx([x, y, 0.0], [x + 0.04, y + 0.04, 0.1], color::DARK);
    }
    let outline = [(x0, y0), (x0 + sd, y0), (x0 + sd, y1 - sd), (x1, y1 - sd), (x1, y1), (x0, y1)];
    parts.prism(&outline, (0.1, 0.3), color::FABRIC);
    parts.bx([x0, y1 - bd, 0.3], [x1 - aw, y1, h], color::FABRIC);
    parts.bx([x0, y0 + aw, 0.3], [x0 + bd, y1 - bd, h], color::FABRIC);
    parts.bx([x1 - aw, y1 - sd, 0.3], [x1, y1, 0.62], color::FABRIC);
    parts.bx([x0, y0, 0.3], [x0 + sd, y0 + aw, 0.62], color::FABRIC);
    let cushion = |parts: &mut Parts, a: [f64; 2], c: [f64; 2]| {
        parts.bx([a[0] + 0.005, a[1] + 0.005, 0.3], [c[0] - 0.005, c[1] - 0.005, 0.44], color::CUSHION);
    };
    cushion(parts, [x0 + bd, y1 - sd], [x0 + sd, y1 - bd]);
    let n = p.count("seats").max(1);
    let run = x1 - aw - (x0 + sd);
    for k in 0..n {
        let xa = x0 + sd + run * k as f64 / n as f64;
        cushion(parts, [xa, y1 - sd], [xa + run / n as f64, y1 - bd]);
        parts.bx(
            [xa + 0.01, y1 - bd - 0.13, 0.44],
            [xa + run / n as f64 - 0.01, y1 - bd, h - 0.06],
            color::CUSHION,
        );
    }
    let side = y1 - sd - (y0 + aw);
    let m = (side / 0.65).round().max(1.0) as usize;
    for k in 0..m {
        let ya = y0 + aw + side * k as f64 / m as f64;
        cushion(parts, [x0 + bd, ya], [x0 + sd, ya + side / m as f64]);
        parts.bx(
            [x0 + bd, ya + 0.01, 0.44],
            [x0 + bd + 0.13, ya + side / m as f64 - 0.01, h - 0.06],
            color::CUSHION,
        );
    }
}

fn office_chair_symbol(sym: &mut Sym, b: &Bx) {
    let (cx, cy) = (b.cx(), b.cy());
    let r = b.w().min(b.d()) / 2.0 - 0.03;
    let (sw, sd) = ((b.w() * 0.78).min(0.5), (b.d() * 0.72).min(0.46));
    let seat = (cx - sw / 2.0, cy - sd / 2.0 - 0.02, cx + sw / 2.0, cy + sd / 2.0 - 0.02);
    // Five-star base, drawn where it shows beyond the seat.
    for k in 0..5 {
        let a = -FRAC_PI_2 + TAU * k as f64 / 5.0 + PI / 5.0;
        let (dx, dy) = (a.cos(), a.sin());
        let tx = if dx.abs() > 1e-9 { (sw / 2.0) / dx.abs() } else { f64::INFINITY };
        let ty = if dy.abs() > 1e-9 { (sd / 2.0) / dy.abs() } else { f64::INFINITY };
        let t0 = tx.min(ty).min(r);
        sym.line((cx + dx * t0, cy + dy * t0), (cx + dx * r, cy + dy * r), Pen::Detail);
        if sym.fine() {
            sym.circle((cx + dx * r, cy + dy * r), 0.025, Pen::Detail);
        }
    }
    sym.rrect(seat.0, seat.1, seat.2, seat.3, 0.07, Pen::Fine);
    let by = seat.3;
    sym.rrect(cx - sw * 0.45, by - 0.01, cx + sw * 0.45, by + 0.06, 0.03, Pen::Fine);
    for s in [-1.0, 1.0] {
        let x = cx + s * (sw / 2.0 + 0.015);
        sym.rrect(x - 0.025, cy - 0.12, x + 0.025, cy + 0.1, 0.02, Pen::Fine);
    }
}

fn office_chair_parts(parts: &mut Parts, b: &Bx) {
    let (cx, cy) = (b.cx(), b.cy());
    let h = b.h.max(0.7);
    let r = b.w().min(b.d()) / 2.0 - 0.03;
    let (sw, sd) = ((b.w() * 0.78).min(0.5), (b.d() * 0.72).min(0.46));
    let seat_c = Rgba::rgb(84, 86, 92);
    for k in 0..5 {
        let a = -FRAC_PI_2 + TAU * k as f64 / 5.0 + PI / 5.0;
        let mut leg = Parts::new();
        leg.bx([0.0, -0.02, 0.05], [r, 0.02, 0.09], color::DARK);
        leg.cyl((r - 0.01, 0.0), 0.025, (0.0, 0.05), color::DARK);
        parts.add_mapped(leg, move |q| {
            let (x, y) = rotate((q.x, q.y), a);
            Point3::new(cx + x, cy + y, q.z)
        });
    }
    parts.cyl((cx, cy), 0.03, (0.09, 0.42), color::METAL);
    let seat = rrect_pts(cx - sw / 2.0, cy - sd / 2.0 - 0.02, cx + sw / 2.0, cy + sd / 2.0 - 0.02, 0.07);
    parts.prism(&seat, (0.42, 0.5), seat_c);
    let by = cy + sd / 2.0 - 0.02;
    parts.bx([cx - 0.03, by - 0.06, 0.42], [cx + 0.03, by + 0.03, 0.62], color::DARK);
    let back = rrect_pts(cx - sw * 0.45, 0.58, cx + sw * 0.45, h, 0.06);
    parts.prism_y(&back, (by, by + 0.06), seat_c);
    for s in [-1.0, 1.0] {
        let x = cx + s * (sw / 2.0 + 0.015);
        parts.bx([x - 0.012, cy - 0.02, 0.5], [x + 0.012, cy + 0.02, 0.66], color::DARK);
        parts.bx([x - 0.025, cy - 0.12, 0.66], [x + 0.025, cy + 0.1, 0.69], color::DARK);
    }
}

// ---------------------------------------------------------------------------------------------------------
// Tables

fn coffee_table_symbol(sym: &mut Sym, b: &Bx) {
    sym.rrect(b.x0, b.y0, b.x1, b.y1, 0.03, Pen::Fine);
    if sym.fine() {
        sym.rect(b.x0 + 0.05, b.y0 + 0.05, b.x1 - 0.05, b.y1 - 0.05, Pen::Detail);
    }
}

fn coffee_table_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.2);
    parts.prism(&rrect_pts(b.x0, b.y0, b.x1, b.y1, 0.03), (h - 0.04, h), color::OAK);
    legs(parts, b, 0.04, h - 0.04, 0.04, color::WALNUT);
    parts.bx([b.x0 + 0.06, b.y0 + 0.06, 0.1], [b.x1 - 0.06, b.y1 - 0.06, 0.12], color::WALNUT);
}

fn table_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.4);
    parts.bx([b.x0, b.y0, h - 0.04], [b.x1, b.y1, h], color::OAK);
    legs(parts, b, 0.07, h - 0.04, 0.06, color::OAK);
    parts.bx([b.x0 + 0.08, b.y0 + 0.08, h - 0.12], [b.x1 - 0.08, b.y0 + 0.1, h - 0.04], color::OAK);
    parts.bx([b.x0 + 0.08, b.y1 - 0.1, h - 0.12], [b.x1 - 0.08, b.y1 - 0.08, h - 0.04], color::OAK);
}

fn round_table_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.4);
    let (rx, ry) = (b.w() / 2.0, b.d() / 2.0);
    parts.prism(&crate::shape::ellipse_pts((b.cx(), b.cy()), rx, ry), (h - 0.04, h), color::OAK);
    parts.cyl((b.cx(), b.cy()), 0.06, (0.03, h - 0.04), color::WALNUT);
    parts.cyl((b.cx(), b.cy()), (rx.min(ry) * 0.5).min(0.3), (0.0, 0.03), color::WALNUT);
}

fn desk_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    if sym.fine() {
        let mw = (b.w() * 0.4).min(0.6);
        let y = b.y1 - 0.18_f64.min(b.d() / 3.0);
        sym.rect(b.cx() - mw / 2.0, y - 0.02, b.cx() + mw / 2.0, y + 0.02, Pen::Detail);
        sym.rect(b.cx() - 0.1, y + 0.02, b.cx() + 0.1, y + 0.08, Pen::Detail);
        let kw = (b.w() * 0.3).min(0.44);
        let ky = b.y0 + 0.1_f64.min(b.d() / 5.0);
        sym.rect(b.cx() - kw / 2.0, ky, b.cx() + kw / 2.0, ky + 0.14, Pen::Detail);
    }
}

fn desk_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.4);
    let top = h - 0.03;
    parts.bx([b.x0, b.y0, top], [b.x1, b.y1, h], color::OAK);
    parts.bx([b.x0 + 0.02, b.y0 + 0.05, 0.0], [b.x0 + 0.05, b.y1 - 0.03, top], color::WHITE);
    let pw = (b.w() * 0.3).min(0.42);
    let ped = Bx { x0: b.x1 - pw - 0.02, y0: b.y0 + 0.03, x1: b.x1 - 0.02, y1: b.y1 - 0.05, h: top };
    parts.bx([ped.x0, ped.y0 + 0.02, 0.0], [ped.x1, ped.y1, top], color::WHITE);
    drawers(parts, &ped, 3, (0.04, top - 0.02), color::FRONT);
    parts.bx([b.x0 + 0.05, b.y1 - 0.05, top - 0.3], [ped.x0, b.y1 - 0.03, top], color::WHITE);
    let mw = (b.w() * 0.4).min(0.6);
    let y = b.y1 - 0.18_f64.min(b.d() / 3.0);
    parts.bx([b.cx() - 0.1, y + 0.0, h], [b.cx() + 0.1, y + 0.1, h + 0.01], color::DARK);
    parts.bx([b.cx() - 0.02, y + 0.03, h], [b.cx() + 0.02, y + 0.06, h + 0.16], color::DARK);
    parts.bx(
        [b.cx() - mw / 2.0, y - 0.02, h + 0.1],
        [b.cx() + mw / 2.0, y + 0.02, h + 0.1 + mw * 0.6],
        color::DARK,
    );
    let kw = (b.w() * 0.3).min(0.44);
    let ky = b.y0 + 0.1_f64.min(b.d() / 5.0);
    parts.bx([b.cx() - kw / 2.0, ky, h], [b.cx() + kw / 2.0, ky + 0.14, h + 0.015], color::DARK);
}

fn garden_table_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    if sym.fine() {
        let n = (b.d() / 0.09).floor().max(1.0) as usize;
        for k in 1..n {
            let y = b.y0 + b.d() * k as f64 / n as f64;
            sym.line((b.x0, y), (b.x1, y), Pen::Detail);
        }
    }
    chairs_symbol(sym, &rect_table_seats(b, p.count("chairs")));
    if p.flag("parasol") {
        sym.layer("object.symbol.above");
        sym.circle((b.cx(), b.cy()), PARASOL_R, Pen::Above);
        sym.layer("object.symbol");
        sym.circle((b.cx(), b.cy()), 0.025, Pen::Fine);
    }
}

/// Radius of the garden parasol.
const PARASOL_R: f64 = 1.25;

fn garden_table_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let h = b.h.max(0.4);
    let n = (b.d() / 0.09).floor().max(1.0) as usize;
    let sw = b.d() / n as f64;
    for k in 0..n {
        let y = b.y0 + k as f64 * sw;
        parts.bx([b.x0, y + 0.006, h - 0.03], [b.x1, y + sw - 0.006, h], color::OAK);
    }
    parts.bx([b.x0 + 0.04, b.y0 + 0.04, h - 0.08], [b.x1 - 0.04, b.y0 + 0.07, h - 0.03], color::OAK);
    parts.bx([b.x0 + 0.04, b.y1 - 0.07, h - 0.08], [b.x1 - 0.04, b.y1 - 0.04, h - 0.03], color::OAK);
    legs(parts, b, 0.05, h - 0.03, 0.04, color::OAK);
    chairs_parts(parts, &rect_table_seats(b, p.count("chairs")), color::OAK, color::OAK);
    if p.flag("parasol") {
        parts.cyl((b.cx(), b.cy()), 0.02, (0.0, 2.4), color::METAL);
        parts.cone((b.cx(), b.cy()), PARASOL_R, (2.0, 2.4), color::CANVAS);
    }
}

// ---------------------------------------------------------------------------------------------------------

/// Plan symbol of a furniture definition; `false` if `id` is not furniture.
pub(crate) fn symbol(id: &str, sym: &mut Sym, b: &Bx, p: &P) -> bool {
    match id {
        "furniture.bed_single" | "furniture.bed_double" => bed_symbol(sym, b),
        "furniture.cot" => cot_symbol(sym, b),
        "furniture.nightstand" => nightstand_symbol(sym, b),
        "furniture.wardrobe" => wardrobe_symbol(sym, b, p.count("doors")),
        "furniture.chest_of_drawers" => chest_symbol(sym, b),
        "furniture.sofa_2" | "furniture.sofa" => sofa_symbol(sym, b, p.count("seats"), 0.2),
        "furniture.sofa_corner" => corner_sofa_symbol(sym, b, p),
        "furniture.armchair" => sofa_symbol(sym, b, 1, 0.18),
        "furniture.coffee_table" => coffee_table_symbol(sym, b),
        "furniture.table" => {
            sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
            chairs_symbol(sym, &rect_table_seats(b, p.count("chairs")));
        }
        "furniture.table_round" => {
            sym.ellipse((b.cx(), b.cy()), b.w() / 2.0, b.d() / 2.0, Pen::Fine);
            chairs_symbol(sym, &round_table_seats(b, p.count("chairs")));
        }
        "furniture.chair" => {
            let mut c = sym.sibling();
            chair_symbol(&mut c, b.w(), b.d());
            sym.append_mapped(c, &placement(b.cx(), b.cy(), 0.0));
        }
        "furniture.desk" => desk_symbol(sym, b),
        "furniture.office_chair" => office_chair_symbol(sym, b),
        "furniture.bookcase" => bookcase_symbol(sym, b),
        "furniture.tv_unit" => tv_symbol(sym, b),
        "furniture.garden_table" => garden_table_symbol(sym, b, p),
        _ => return false,
    }
    true
}

/// 3D parts of a furniture definition; `false` if `id` is not furniture.
pub(crate) fn parts(id: &str, parts: &mut Parts, b: &Bx, p: &P) -> bool {
    match id {
        "furniture.bed_single" | "furniture.bed_double" => bed_parts(parts, b),
        "furniture.cot" => cot_parts(parts, b),
        "furniture.nightstand" => nightstand_parts(parts, b),
        "furniture.wardrobe" => wardrobe_parts(parts, b, p.count("doors")),
        "furniture.chest_of_drawers" => chest_parts(parts, b, p.count("drawers")),
        "furniture.sofa_2" | "furniture.sofa" => sofa_parts(parts, b, p.count("seats"), 0.2),
        "furniture.sofa_corner" => corner_sofa_parts(parts, b, p),
        "furniture.armchair" => sofa_parts(parts, b, 1, 0.18),
        "furniture.coffee_table" => coffee_table_parts(parts, b),
        "furniture.table" => {
            table_parts(parts, b);
            chairs_parts(parts, &rect_table_seats(b, p.count("chairs")), color::OAK, color::CUSHION);
        }
        "furniture.table_round" => {
            round_table_parts(parts, b);
            chairs_parts(parts, &round_table_seats(b, p.count("chairs")), color::OAK, color::CUSHION);
        }
        "furniture.chair" => {
            let mut c = Parts::new();
            chair_parts(&mut c, (b.w(), b.d(), b.h.max(0.5)), color::OAK, color::CUSHION);
            let (cx, cy) = (b.cx(), b.cy());
            parts.add_mapped(c, move |q| Point3::new(q.x + cx, q.y + cy, q.z));
        }
        "furniture.desk" => desk_parts(parts, b),
        "furniture.office_chair" => office_chair_parts(parts, b),
        "furniture.bookcase" => bookcase_parts(parts, b, p.count("shelves")),
        "furniture.tv_unit" => tv_parts(parts, b),
        "furniture.garden_table" => garden_table_parts(parts, b, p),
        _ => return false,
    }
    true
}
