//! Kitchen units and appliances on the 0.60 m module.
//!
//! Base units stand against the wall at `y1`; their worktop front edge is `y0`. Heights are to the top of the
//! worktop (or of the appliance).

use crate::Bx;
use crate::params::P;
use crate::parts::{Parts, color};
use crate::shape::{P2, rrect_pts};
use crate::sym::{Pen, Sym};

const WORKTOP_T: f64 = 0.04;
const PLINTH: f64 = 0.1;
/// Depth of the worktop overhang in front of the doors.
const NOSING: f64 = 0.02;
const DOOR_T: f64 = 0.02;

/// Module boundaries along x: `n` equal modules close to `module` wide.
fn modules(b: &Bx, module: f64) -> Vec<(f64, f64)> {
    let n = (b.w() / module.max(0.2)).round().max(1.0) as usize;
    let mw = b.w() / n as f64;
    (0..n).map(|k| (b.x0 + k as f64 * mw, b.x0 + (k + 1) as f64 * mw)).collect()
}

/// Plinth, carcass and doors of base units up to `top`, with bar handles.
fn base_units(parts: &mut Parts, b: &Bx, top: f64, mods: &[(f64, f64)], handles: bool) {
    let yd = b.y0 + NOSING;
    parts.bx([b.x0, yd + 0.05, 0.0], [b.x1, b.y1 - 0.02, PLINTH], color::DARK);
    parts.bx([b.x0, yd + DOOR_T, PLINTH], [b.x1, b.y1, top], color::WHITE);
    for (xa, xb) in mods {
        parts.bx([xa + 0.002, yd, PLINTH + 0.003], [xb - 0.002, yd + DOOR_T, top - 0.003], color::FRONT);
        if handles {
            let xm = (xa + xb) / 2.0;
            let hw = ((xb - xa) * 0.2).min(0.08);
            parts.bx([xm - hw, yd - 0.025, top - 0.065], [xm + hw, yd, top - 0.05], color::METAL);
        }
    }
}

/// A worktop over the box with rectangular cut-outs `(x0, y0, x1, y1)`.
fn worktop(parts: &mut Parts, b: &Bx, holes: &[(f64, f64, f64, f64)]) {
    let outer: Vec<P2> = vec![(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)];
    let holes: Vec<Vec<P2>> =
        holes.iter().map(|(x0, y0, x1, y1)| vec![(*x0, *y0), (*x1, *y0), (*x1, *y1), (*x0, *y1)]).collect();
    parts.prism_holes(&outer, &holes, (b.h - WORKTOP_T, b.h), color::STONE);
}

fn counter_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y1 - 0.02), (b.x1, b.y1 - 0.02), Pen::Detail);
}

fn counter_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let mods = modules(b, p.get("module"));
    base_units(parts, b, b.h - WORKTOP_T, &mods, p.flag("handle"));
    worktop(parts, b, &[]);
    parts.bx([b.x0, b.y1 - 0.015, b.h], [b.x1, b.y1, b.h + 0.05], color::STONE);
}

fn wall_cabinets_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    sym.layer("object.symbol.above");
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Above);
    let mods = modules(b, p.get("module"));
    for (_, xb) in mods.iter().take(mods.len().saturating_sub(1)) {
        sym.line((*xb, b.y0), (*xb, b.y1), Pen::Above);
    }
    sym.layer("object.symbol");
}

fn wall_cabinets_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let z0 = p.get("mount");
    let z1 = z0 + b.h;
    parts.bx([b.x0, b.y0 + DOOR_T, z0], [b.x1, b.y1, z1], color::WHITE);
    for (xa, xb) in modules(b, p.get("module")) {
        parts.bx([xa + 0.002, b.y0, z0 + 0.003], [xb - 0.002, b.y0 + DOOR_T, z1 - 0.003], color::FRONT);
        if p.flag("handle") {
            let xm = (xa + xb) / 2.0;
            let hw = ((xb - xa) * 0.2).min(0.08);
            parts.bx([xm - hw, b.y0 - 0.025, z0 + 0.05], [xm + hw, b.y0, z0 + 0.065], color::METAL);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Sinks

/// Layout of a sink: the steel plate, the bowls and the drainer, all `(x0, y0, x1, y1)`.
struct SinkLayout {
    plate: (f64, f64, f64, f64),
    bowls: Vec<(f64, f64, f64, f64)>,
    drainer: Option<(f64, f64, f64, f64)>,
    tap: P2,
}

fn sink_layout(b: &Bx, p: &P) -> SinkLayout {
    let plate = (b.x0 + 0.03, b.y0 + 0.06, b.x1 - 0.03, b.y1 - 0.05);
    let pw = plate.2 - plate.0;
    let n = p.count("bowls").clamp(1, 2);
    let gap = 0.04;
    let avail = pw - gap * (n + 1) as f64;
    let with_drainer = p.flag("drainer") && avail * 0.45 >= 0.25;
    let bw = if with_drainer { avail * 0.58 / n as f64 } else { avail / n as f64 }.min(0.48);
    let (by0, by1) = (plate.1 + 0.03, plate.3 - 0.08);
    let bowls: Vec<_> = (0..n)
        .map(|k| {
            let x = plate.0 + gap + k as f64 * (bw + gap);
            // A second bowl is the smaller "half" bowl.
            let w = if k == 1 { bw * 0.7 } else { bw };
            (x, by0, x + w, by1)
        })
        .collect();
    let end = bowls.last().map(|bw| bw.2).unwrap_or(plate.0);
    let drainer = with_drainer.then_some((end + gap, by0, plate.2 - gap, by1));
    let tap =
        bowls.first().map(|b0| ((b0.0 + b0.2) / 2.0, plate.3 - 0.035)).unwrap_or((b.cx(), plate.3 - 0.035));
    SinkLayout { plate, bowls, drainer, tap }
}

fn sink_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    let l = sink_layout(b, p);
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y1 - 0.02), (b.x1, b.y1 - 0.02), Pen::Detail);
    sym.rrect(l.plate.0, l.plate.1, l.plate.2, l.plate.3, 0.02, Pen::Fine);
    for (x0, y0, x1, y1) in &l.bowls {
        sym.rrect(*x0, *y0, *x1, *y1, 0.04, Pen::Fine);
        if sym.fine() {
            sym.circle(((x0 + x1) / 2.0, (y0 + y1) / 2.0), 0.025, Pen::Detail);
        }
    }
    if let Some((x0, y0, x1, y1)) = l.drainer
        && sym.fine()
    {
        let mut x = x0 + 0.02;
        while x < x1 - 0.01 {
            sym.line((x, y0 + 0.03), (x, y1 - 0.03), Pen::Detail);
            x += 0.035;
        }
    }
    sym.circle(l.tap, 0.022, Pen::Fine);
    sym.line(l.tap, (l.tap.0, l.tap.1 - 0.16), Pen::Fine);
}

fn sink_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let l = sink_layout(b, p);
    let doors = if b.w() > 0.9 { 2 } else { 1 };
    base_units(parts, b, b.h - WORKTOP_T, &modules(b, b.w() / doors as f64), true);
    worktop(parts, b, &l.bowls);
    let h = b.h;
    let plate: Vec<P2> = rrect_pts(l.plate.0, l.plate.1, l.plate.2, l.plate.3, 0.02);
    let holes: Vec<Vec<P2>> =
        l.bowls.iter().map(|(x0, y0, x1, y1)| vec![(*x0, *y0), (*x1, *y0), (*x1, *y1), (*x0, *y1)]).collect();
    parts.prism_holes(&plate, &holes, (h, h + 0.003), color::STEEL);
    let depth = 0.19;
    let t = 0.006;
    for (x0, y0, x1, y1) in &l.bowls {
        parts.bx([*x0, *y0, h - depth], [*x1, *y1, h - depth + t], color::STEEL);
        parts.bx([*x0, *y0, h - depth], [*x0 + t, *y1, h], color::STEEL);
        parts.bx([*x1 - t, *y0, h - depth], [*x1, *y1, h], color::STEEL);
        parts.bx([*x0 + t, *y0, h - depth], [*x1 - t, *y0 + t, h], color::STEEL);
        parts.bx([*x0 + t, *y1 - t, h - depth], [*x1 - t, *y1, h], color::STEEL);
    }
    let (tx, ty) = l.tap;
    parts.cyl((tx, ty), 0.022, (h + 0.003, h + 0.3), color::METAL);
    parts.bx([tx - 0.012, ty - 0.17, h + 0.27], [tx + 0.012, ty, h + 0.3], color::METAL);
}

// ---------------------------------------------------------------------------------------------------------
// Cooking

/// Burner centres and radii on a cooking plate `(x0, y0, x1, y1)`.
fn burners(plate: (f64, f64, f64, f64), n: usize) -> Vec<(f64, f64, f64)> {
    let (cx, cy) = ((plate.0 + plate.2) / 2.0, (plate.1 + plate.3) / 2.0);
    let (kx, ky) = ((plate.2 - plate.0) / 0.56, (plate.3 - plate.1) / 0.5);
    let k = kx.min(ky);
    let layout: &[(f64, f64, f64)] = match n {
        0 | 1 => &[(0.0, 0.0, 0.11)],
        2 => &[(0.0, -0.11, 0.09), (0.0, 0.11, 0.075)],
        3 => &[(-0.13, -0.11, 0.09), (-0.13, 0.12, 0.07), (0.13, 0.0, 0.1)],
        4 => &[(-0.13, -0.11, 0.1), (-0.13, 0.12, 0.075), (0.13, -0.11, 0.075), (0.13, 0.12, 0.09)],
        5 => &[
            (-0.19, -0.11, 0.075),
            (-0.19, 0.12, 0.065),
            (0.19, -0.11, 0.065),
            (0.19, 0.12, 0.075),
            (0.0, 0.0, 0.09),
        ],
        _ => &[
            (-0.18, -0.11, 0.07),
            (0.0, -0.11, 0.07),
            (0.18, -0.11, 0.07),
            (-0.18, 0.12, 0.065),
            (0.0, 0.12, 0.065),
            (0.18, 0.12, 0.065),
        ],
    };
    layout.iter().map(|(x, y, r)| (cx + x * kx, cy + y * ky, r * k)).collect()
}

fn hob_plate(b: &Bx) -> (f64, f64, f64, f64) {
    (b.x0 + 0.01, b.y0 + 0.05, b.x1 - 0.01, b.y1 - 0.04)
}

fn burners_symbol(sym: &mut Sym, plate: (f64, f64, f64, f64), n: usize) {
    sym.rrect(plate.0, plate.1, plate.2, plate.3, 0.01, Pen::Fine);
    for (x, y, r) in burners(plate, n) {
        sym.circle((x, y), r, Pen::Fine);
        if sym.fine() {
            sym.circle((x, y), r * 0.55, Pen::Detail);
        }
    }
}

fn burners_parts(parts: &mut Parts, plate: (f64, f64, f64, f64), n: usize, z: f64) {
    parts.prism(&rrect_pts(plate.0, plate.1, plate.2, plate.3, 0.01), (z, z + 0.006), color::BLACK_GLASS);
    let ring = kanavos_drawing::Rgba::rgb(96, 98, 104);
    for (x, y, r) in burners(plate, n) {
        parts.cyl((x, y), r, (z + 0.006, z + 0.008), ring);
        parts.cyl((x, y), r * 0.85, (z + 0.006, z + 0.009), color::BLACK_GLASS);
    }
}

fn oven_column_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.02), (b.x1, b.y0 + 0.02), Pen::Detail);
    sym.line((b.x0, b.y0 + 0.02), (b.x1, b.y1), Pen::Detail);
    sym.line((b.x1, b.y0 + 0.02), (b.x0, b.y1), Pen::Detail);
}

fn oven_door(parts: &mut Parts, b: &Bx, yd: f64, (z0, z1): (f64, f64)) {
    parts.bx([b.x0 + 0.003, yd, z0], [b.x1 - 0.003, yd + DOOR_T, z1], color::BLACK_GLASS);
    parts.bx([b.x0 + 0.06, yd - 0.035, z1 - 0.06], [b.x1 - 0.06, yd - 0.015, z1 - 0.04], color::METAL);
    for x in [b.x0 + 0.08, b.x1 - 0.09] {
        parts.bx([x, yd - 0.035, z1 - 0.06], [x + 0.01, yd, z1 - 0.04], color::METAL);
    }
}

fn oven_column_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(1.0);
    let yd = b.y0 + 0.0;
    parts.bx([b.x0, yd + 0.05, 0.0], [b.x1, b.y1 - 0.02, PLINTH], color::DARK);
    parts.bx([b.x0, yd + DOOR_T, PLINTH], [b.x1, b.y1, h], color::WHITE);
    let (o0, o1) = (0.8_f64.min(h * 0.4), 1.4_f64.min(h * 0.7));
    parts.bx([b.x0 + 0.002, yd, PLINTH + 0.003], [b.x1 - 0.002, yd + DOOR_T, o0 - 0.003], color::FRONT);
    oven_door(parts, b, yd, (o0, o1));
    parts.bx([b.x0 + 0.002, yd, o1 + 0.003], [b.x1 - 0.002, yd + DOOR_T, h - 0.003], color::FRONT);
}

fn cooker_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.04), (b.x1, b.y0 + 0.04), Pen::Detail);
    burners_symbol(sym, (b.x0 + 0.02, b.y0 + 0.07, b.x1 - 0.02, b.y1 - 0.03), p.count("burners"));
}

fn cooker_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let h = b.h.max(0.5);
    parts.bx([b.x0, b.y0 + 0.02, 0.0], [b.x1, b.y1, h], color::WHITE);
    burners_parts(parts, (b.x0 + 0.02, b.y0 + 0.07, b.x1 - 0.02, b.y1 - 0.03), p.count("burners"), h);
    oven_door(parts, b, b.y0, (0.12, h - 0.18));
    for k in 0..4 {
        let x = b.x0 + b.w() * (k as f64 + 0.5) / 4.0;
        parts.cyl((x, b.y0 + 0.01), 0.018, (h - 0.1, h - 0.06), color::DARK);
    }
}

fn hob_parts(parts: &mut Parts, b: &Bx, p: &P) {
    burners_parts(parts, hob_plate(b), p.count("burners"), b.h);
}

fn fridge_symbol(sym: &mut Sym, b: &Bx) {
    let yd = b.y0 + 0.05_f64.min(b.d() / 6.0);
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, yd), (b.x1, yd), Pen::Fine);
    sym.line((b.x0, yd), (b.x1, b.y1), Pen::Detail);
    if sym.fine() {
        sym.rect(b.x1 - 0.06, b.y0 - 0.03, b.x1 - 0.04, b.y0, Pen::Detail);
    }
}

fn fridge_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.5);
    let yd = b.y0 + 0.05_f64.min(b.d() / 6.0);
    parts.bx([b.x0, yd, 0.0], [b.x1, b.y1, h], color::WHITE);
    let split = (h * 0.38).min(0.75);
    for (z0, z1) in [(0.02, split - 0.002), (split + 0.002, h)] {
        parts.bx([b.x0 + 0.002, b.y0, z0], [b.x1 - 0.002, yd, z1], color::WHITE);
        let (ha, hb) = if z1 < h - 0.01 { (z1 - 0.35, z1 - 0.05) } else { (z0 + 0.05, z0 + 0.45) };
        parts.bx(
            [b.x1 - 0.06, b.y0 - 0.03, ha.max(z0 + 0.03)],
            [b.x1 - 0.04, b.y0, hb.min(z1 - 0.03)],
            color::METAL,
        );
    }
}

fn dishwasher_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    let (x0, y0, x1, y1) = (b.x0 + 0.03, b.y0 + 0.04, b.x1 - 0.03, b.y1 - 0.05);
    sym.rect(x0, y0, x1, y1, Pen::Hidden);
    sym.line((x0, y0), (x1, y1), Pen::Hidden);
    sym.line((x0, y1), (x1, y0), Pen::Hidden);
}

fn dishwasher_parts(parts: &mut Parts, b: &Bx) {
    base_units(parts, b, b.h - WORKTOP_T, &[(b.x0, b.x1)], false);
    worktop(parts, b, &[]);
    let yd = b.y0 + NOSING;
    parts.bx(
        [b.x0 + 0.04, yd - 0.025, b.h - WORKTOP_T - 0.07],
        [b.x1 - 0.04, yd, b.h - WORKTOP_T - 0.05],
        color::METAL,
    );
}

fn island_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    let ov = p.get("overhang").min(b.d() * 0.5);
    if ov > 0.02 {
        sym.line((b.x0 + 0.02, b.y1 - ov), (b.x1 - 0.02, b.y1 - ov), Pen::Hidden);
    }
    for (x, y) in stools(b, p) {
        sym.circle((x, y), 0.18, Pen::Fine);
        if sym.fine() {
            sym.circle((x, y), 0.12, Pen::Detail);
        }
    }
}

fn stools(b: &Bx, p: &P) -> Vec<P2> {
    let n = p.count("stools");
    let n = n.min((b.w() / 0.42).floor().max(0.0) as usize);
    (0..n).map(|k| (b.x0 + b.w() * (k as f64 + 0.5) / n as f64, b.y1 + 0.2)).collect()
}

fn island_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let ov = p.get("overhang").min(b.d() * 0.5);
    let units = Bx { y1: b.y1 - ov, ..*b };
    base_units(parts, &units, b.h - WORKTOP_T, &modules(&units, 0.6), true);
    worktop(parts, b, &[]);
    for (x, y) in stools(b, p) {
        parts.cyl((x, y), 0.17, (0.0, 0.02), color::DARK);
        parts.cyl((x, y), 0.025, (0.02, 0.62), color::METAL);
        parts.cyl((x, y), 0.18, (0.62, 0.67), color::OAK);
    }
}

/// Plan symbol of a kitchen definition; `false` if `id` is not a kitchen unit.
pub(crate) fn symbol(id: &str, sym: &mut Sym, b: &Bx, p: &P) -> bool {
    match id {
        "kitchen.counter" => counter_symbol(sym, b),
        "kitchen.wall_cabinets" => wall_cabinets_symbol(sym, b, p),
        "kitchen.sink" | "kitchen.sink_double" => sink_symbol(sym, b, p),
        "kitchen.hob" => burners_symbol(sym, hob_plate(b), p.count("burners")),
        "kitchen.cooker" => cooker_symbol(sym, b, p),
        "kitchen.oven" => oven_column_symbol(sym, b),
        "kitchen.fridge" => fridge_symbol(sym, b),
        "kitchen.dishwasher" => dishwasher_symbol(sym, b),
        "kitchen.island" => island_symbol(sym, b, p),
        _ => return false,
    }
    true
}

/// 3D parts of a kitchen definition; `false` if `id` is not a kitchen unit.
pub(crate) fn parts(id: &str, parts: &mut Parts, b: &Bx, p: &P) -> bool {
    match id {
        "kitchen.counter" => counter_parts(parts, b, p),
        "kitchen.wall_cabinets" => wall_cabinets_parts(parts, b, p),
        "kitchen.sink" | "kitchen.sink_double" => sink_parts(parts, b, p),
        "kitchen.hob" => hob_parts(parts, b, p),
        "kitchen.cooker" => cooker_parts(parts, b, p),
        "kitchen.oven" => oven_column_parts(parts, b),
        "kitchen.fridge" => fridge_parts(parts, b),
        "kitchen.dishwasher" => dishwasher_parts(parts, b),
        "kitchen.island" => island_parts(parts, b, p),
        _ => return false,
    }
    true
}
