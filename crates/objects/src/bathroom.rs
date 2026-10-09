//! Sanitary fixtures and laundry equipment.
//!
//! Fixtures stand against the wall at `y1`; the user approaches from `y0`.

use std::f64::consts::FRAC_PI_2;

use crate::Bx;
use crate::params::P;
use crate::parts::{Parts, color};
use crate::shape::{P2, egg_pts, ellipse_pts, rrect_pts, rrect2_pts};
use crate::sym::{Pen, Sym};

// ---------------------------------------------------------------------------------------------------------
// WC and bidet

/// Depth of a close-coupled cistern.
fn cistern_d(b: &Bx) -> f64 {
    0.17_f64.min(b.d() / 3.0)
}

fn half_w(b: &Bx) -> f64 {
    (b.w() / 2.0 - 0.005).min(0.19)
}

/// The bowl of a close-coupled WC: an egg that runs under the cistern, cut at the cistern's front `back`.
fn wc_bowl(b: &Bx, back: f64, hw: f64) -> Vec<P2> {
    egg_pts(b.cx(), b.y0, back + 0.08, hw).into_iter().map(|(x, y)| (x, y.min(back))).collect()
}

fn wc_symbol(sym: &mut Sym, b: &Bx) {
    let cd = cistern_d(b);
    let hw = half_w(b);
    sym.rrect(b.x0, b.y1 - cd, b.x1, b.y1, 0.02, Pen::Fine);
    let back = b.y1 - cd;
    sym.poly(&wc_bowl(b, back, hw), true, Pen::Fine);
    if sym.fine() {
        sym.poly(&egg_pts(b.cx(), b.y0 + 0.05, back - 0.04, hw - 0.055), true, Pen::Fine);
    }
}

fn wc_parts(parts: &mut Parts, b: &Bx) {
    let cd = cistern_d(b);
    let hw = half_w(b);
    let back = b.y1 - cd;
    let h = b.h.max(0.5);
    parts.prism(&egg_pts(b.cx(), b.y0 + 0.07, back, hw * 0.55), (0.0, 0.32), color::PORCELAIN);
    parts.bx([b.cx() - hw * 0.5, back - 0.05, 0.0], [b.cx() + hw * 0.5, b.y1 - 0.01, 0.38], color::PORCELAIN);
    parts.prism(&wc_bowl(b, back, hw), (0.3, 0.38), color::PORCELAIN);
    parts.prism(&egg_pts(b.cx(), b.y0 + 0.005, back - 0.005, hw - 0.005), (0.38, 0.4), color::WHITE);
    parts.prism(&egg_pts(b.cx(), b.y0 + 0.012, back - 0.005, hw - 0.012), (0.4, 0.42), color::WHITE);
    parts.prism(&rrect_pts(b.x0, b.y1 - cd, b.x1, b.y1, 0.02), (0.4, h), color::PORCELAIN);
    parts.cyl((b.cx(), b.y1 - cd / 2.0), 0.025, (h, h + 0.008), color::METAL);
}

fn wc_wall_symbol(sym: &mut Sym, b: &Bx) {
    let hw = half_w(b);
    sym.poly(&egg_pts(b.cx(), b.y0, b.y1 - 0.02, hw), true, Pen::Fine);
    sym.rect(b.cx() - hw * 0.75, b.y1 - 0.06, b.cx() + hw * 0.75, b.y1, Pen::Fine);
    if sym.fine() {
        sym.poly(&egg_pts(b.cx(), b.y0 + 0.05, b.y1 - 0.13, hw - 0.055), true, Pen::Fine);
    }
}

fn wc_wall_parts(parts: &mut Parts, b: &Bx) {
    let hw = half_w(b);
    let h = b.h.max(0.3);
    parts.prism(&egg_pts(b.cx(), b.y0, b.y1 - 0.02, hw), (h - 0.16, h - 0.04), color::PORCELAIN);
    parts.prism(
        &egg_pts(b.cx(), b.y0 + 0.08, b.y1 - 0.02, hw * 0.75),
        (h - 0.24, h - 0.16),
        color::PORCELAIN,
    );
    parts.bx(
        [b.cx() - hw * 0.75, b.y1 - 0.06, h - 0.24],
        [b.cx() + hw * 0.75, b.y1, h - 0.04],
        color::PORCELAIN,
    );
    parts.prism(&egg_pts(b.cx(), b.y0 + 0.005, b.y1 - 0.06, hw - 0.005), (h - 0.04, h - 0.02), color::WHITE);
    parts.prism(&egg_pts(b.cx(), b.y0 + 0.012, b.y1 - 0.06, hw - 0.012), (h - 0.02, h), color::WHITE);
    // Flush plate of the concealed cistern.
    parts.bx([b.cx() - 0.12, b.y1 - 0.012, 0.98], [b.cx() + 0.12, b.y1, 1.14], color::WHITE);
    parts.bx([b.cx() - 0.1, b.y1 - 0.016, 1.0], [b.cx() - 0.005, b.y1 - 0.012, 1.12], color::METAL);
    parts.bx([b.cx() + 0.005, b.y1 - 0.016, 1.0], [b.cx() + 0.1, b.y1 - 0.012, 1.12], color::METAL);
}

fn bidet_symbol(sym: &mut Sym, b: &Bx) {
    let hw = half_w(b);
    sym.poly(&egg_pts(b.cx(), b.y0, b.y1 - 0.02, hw), true, Pen::Fine);
    sym.line((b.cx() - hw * 0.7, b.y1), (b.cx() + hw * 0.7, b.y1), Pen::Fine);
    sym.poly(&egg_pts(b.cx(), b.y0 + 0.05, b.y1 - 0.13, hw - 0.05), true, Pen::Fine);
    sym.circle((b.cx(), b.y1 - 0.07), 0.02, Pen::Detail);
    if sym.fine() {
        sym.circle((b.cx(), (b.y0 + b.y1 - 0.08) / 2.0), 0.015, Pen::Detail);
    }
}

fn bidet_parts(parts: &mut Parts, b: &Bx) {
    let hw = half_w(b);
    let h = b.h.max(0.3);
    parts.prism(&egg_pts(b.cx(), b.y0 + 0.07, b.y1 - 0.02, hw * 0.6), (0.0, h - 0.12), color::PORCELAIN);
    let inner = egg_pts(b.cx(), b.y0 + 0.05, b.y1 - 0.13, hw - 0.05);
    parts.prism_holes(
        &egg_pts(b.cx(), b.y0, b.y1 - 0.02, hw),
        std::slice::from_ref(&inner),
        (h - 0.14, h),
        color::PORCELAIN,
    );
    parts.prism(&inner, (h - 0.14, h - 0.12), color::PORCELAIN);
    parts.cyl((b.cx(), b.y1 - 0.07), 0.02, (h, h + 0.1), color::METAL);
    parts.bx([b.cx() - 0.01, b.y1 - 0.15, h + 0.08], [b.cx() + 0.01, b.y1 - 0.07, h + 0.1], color::METAL);
}

// ---------------------------------------------------------------------------------------------------------
// Basins

/// The bowl of a basin: centre and radii.
fn bowl(b: &Bx) -> (P2, f64, f64) {
    let rx = (b.w() / 2.0 - 0.07).max(0.05);
    let ry = ((b.d() - 0.16) / 2.0).max(0.05);
    ((b.cx(), b.y0 + 0.05 + ry), rx, ry)
}

fn tap(b: &Bx) -> P2 {
    (b.cx(), b.y1 - 0.045)
}

fn basin_outline(b: &Bx) -> Vec<P2> {
    rrect2_pts(b.x0, b.y0, b.x1, b.y1, (b.d() * 0.4).min(0.18), 0.02)
}

fn basin_symbol(sym: &mut Sym, b: &Bx, vanity: bool) {
    if vanity {
        sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    } else {
        sym.poly(&basin_outline(b), true, Pen::Fine);
    }
    let (c, rx, ry) = bowl(b);
    sym.ellipse(c, rx, ry, Pen::Fine);
    sym.circle(tap(b), 0.02, Pen::Detail);
    if sym.fine() {
        sym.circle(c, 0.015, Pen::Detail);
    }
}

fn tap_parts(parts: &mut Parts, b: &Bx, z: f64) {
    let (tx, ty) = tap(b);
    parts.cyl((tx, ty), 0.018, (z, z + 0.15), color::METAL);
    parts.bx([tx - 0.01, ty - 0.12, z + 0.13], [tx + 0.01, ty, z + 0.15], color::METAL);
}

fn basin_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let h = b.h.max(0.3);
    let (c, rx, ry) = bowl(b);
    let inner = ellipse_pts(c, rx, ry);
    parts.prism_holes(&basin_outline(b), std::slice::from_ref(&inner), (h - 0.16, h), color::PORCELAIN);
    parts.prism(&inner, (h - 0.16, h - 0.14), color::PORCELAIN);
    if p.flag("pedestal") {
        parts.prism(&ellipse_pts((b.cx(), b.y1 - 0.14), 0.085, 0.075), (0.0, h - 0.16), color::PORCELAIN);
    }
    tap_parts(parts, b, h);
}

fn vanity_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.4);
    let top = h - 0.03;
    parts.bx([b.x0 + 0.02, b.y0 + 0.07, 0.0], [b.x1 - 0.02, b.y1, 0.1], color::DARK);
    parts.bx([b.x0, b.y0 + 0.04, 0.1], [b.x1, b.y1, top], color::WHITE);
    let doors = if b.w() > 0.7 { 2 } else { 1 };
    let dw = b.w() / doors as f64;
    for k in 0..doors {
        let xa = b.x0 + k as f64 * dw;
        parts.bx([xa + 0.002, b.y0 + 0.02, 0.103], [xa + dw - 0.002, b.y0 + 0.04, top - 0.003], color::OAK);
    }
    let (c, rx, ry) = bowl(b);
    let inner = ellipse_pts(c, rx, ry);
    parts.prism_holes(
        &rrect_pts(b.x0, b.y0, b.x1, b.y1, 0.01),
        std::slice::from_ref(&inner),
        (top, h),
        color::PORCELAIN,
    );
    parts.prism_holes(
        &ellipse_pts(c, rx + 0.01, ry + 0.01),
        std::slice::from_ref(&inner),
        (h - 0.16, top),
        color::PORCELAIN,
    );
    parts.prism(&inner, (h - 0.16, h - 0.14), color::PORCELAIN);
    tap_parts(parts, b, h);
}

// ---------------------------------------------------------------------------------------------------------
// Bath and shower

fn bath_inner(b: &Bx) -> Vec<P2> {
    let (mx, my) = (0.07_f64.min(b.w() / 6.0), 0.06_f64.min(b.d() / 6.0));
    rrect_pts(b.x0 + mx, b.y0 + my, b.x1 - mx, b.y1 - my, (b.d() * 0.25).min(0.2))
}

fn bath_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.poly(&bath_inner(b), true, Pen::Fine);
    sym.circle((b.x0 + 0.2_f64.min(b.w() / 5.0), b.cy()), 0.025, Pen::Detail);
}

fn bath_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.2);
    let outer: Vec<P2> = vec![(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)];
    let inner = bath_inner(b);
    parts.prism_holes(&outer, std::slice::from_ref(&inner), (0.0, h), color::WHITE);
    parts.prism(&inner, (0.0, (h - 0.42).max(0.05)), color::WHITE);
}

fn tray_inner(b: &Bx) -> (f64, f64, f64, f64) {
    let m = 0.035_f64.min(b.w().min(b.d()) / 6.0);
    (b.x0 + m, b.y0 + m, b.x1 - m, b.y1 - m)
}

fn shower_symbol(sym: &mut Sym, b: &Bx) {
    let (x0, y0, x1, y1) = tray_inner(b);
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.rect(x0, y0, x1, y1, Pen::Fine);
    sym.line((x0, y0), (x1, y1), Pen::Detail);
    sym.line((x0, y1), (x1, y0), Pen::Detail);
    sym.circle((b.cx(), b.cy()), 0.045, Pen::Detail);
}

fn shower_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.02);
    let (x0, y0, x1, y1) = tray_inner(b);
    let outer: Vec<P2> = vec![(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)];
    let inner: Vec<P2> = vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    parts.prism_holes(&outer, std::slice::from_ref(&inner), (0.0, h), color::WHITE);
    parts.prism(&inner, (0.0, h * 0.6), color::WHITE);
    parts.cyl((b.cx(), b.cy()), 0.045, (h * 0.6, h * 0.6 + 0.002), color::METAL);
}

/// Width of the hinged door of a shower screen.
fn screen_door(b: &Bx, p: &P) -> f64 {
    if p.flag("door") { (b.w() * 0.6).min(0.6) } else { 0.0 }
}

fn shower_screen_symbol(sym: &mut Sym, b: &Bx, p: &P) {
    let ym = b.cy();
    let dw = screen_door(b, p);
    let g = 0.004;
    sym.rect(b.x0, b.y0, b.x0 + 0.02, b.y1, Pen::Fine);
    sym.rect(b.x0 + 0.02, ym - g, b.x1 - dw, ym + g, Pen::Fine);
    if dw > 0.0 {
        // The door opens out of the shower, towards −y.
        let hx = b.x1 - dw;
        sym.rect(hx - g, ym - dw, hx + g, ym, Pen::Fine);
        sym.layer("object.symbol.swing");
        sym.arc((hx, ym), dw, 0.0, -FRAC_PI_2, Pen::Fine);
        sym.layer("object.symbol");
    }
}

fn shower_screen_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let h = b.h.max(0.5);
    let ym = b.cy();
    let dw = screen_door(b, p);
    let g = 0.004;
    parts.bx([b.x0, b.y0, 0.0], [b.x0 + 0.02, b.y1, h], color::METAL);
    parts.bx([b.x0 + 0.02, ym - g, 0.0], [b.x1 - dw - 0.004, ym + g, h], color::GLASS);
    if dw > 0.0 {
        parts.bx([b.x1 - dw, ym - g, 0.01], [b.x1, ym + g, h], color::GLASS);
        for z in [0.3, h - 0.3] {
            parts.bx(
                [b.x1 - dw - 0.02, ym - 0.012, z],
                [b.x1 - dw + 0.03, ym + 0.012, z + 0.06],
                color::METAL,
            );
        }
        parts.bx([b.x1 - 0.06, ym - 0.03, 0.9], [b.x1 - 0.045, ym + 0.03, 1.2], color::METAL);
    }
}

// ---------------------------------------------------------------------------------------------------------
// Laundry and hot water

fn washer_symbol(sym: &mut Sym, b: &Bx) {
    sym.rect(b.x0, b.y0, b.x1, b.y1, Pen::Fine);
    sym.line((b.x0, b.y0 + 0.03), (b.x1, b.y0 + 0.03), Pen::Detail);
    let r = b.w().min(b.d()) * 0.3;
    sym.circle((b.cx(), b.cy() + 0.015), r, Pen::Fine);
    if sym.fine() {
        sym.circle((b.cx(), b.cy() + 0.015), r * 0.72, Pen::Detail);
    }
}

fn washer_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.4);
    parts.bx([b.x0, b.y0 + 0.01, 0.0], [b.x1, b.y1, h], color::WHITE);
    let zc = (h * 0.5).min(0.42);
    let r = (b.w() * 0.29).min(0.18);
    parts.prism_y(&ellipse_pts((b.cx(), zc), r, r), (b.y0 - 0.008, b.y0 + 0.01), color::METAL);
    parts.prism_y(
        &ellipse_pts((b.cx(), zc), r * 0.75, r * 0.75),
        (b.y0 - 0.014, b.y0 - 0.008),
        color::BLACK_GLASS,
    );
    parts.bx([b.x0 + 0.03, b.y0 + 0.004, h - 0.12], [b.x1 - 0.03, b.y0 + 0.01, h - 0.03], color::FRONT);
    parts.prism_y(
        &ellipse_pts((b.x1 - 0.12, h - 0.075), 0.025, 0.025),
        (b.y0 - 0.01, b.y0 + 0.004),
        color::METAL,
    );
}

fn heater_centre(b: &Bx) -> (P2, f64) {
    let r = (b.w().min(b.d()) / 2.0 - 0.01).max(0.05);
    ((b.cx(), b.y1 - 0.02 - r), r)
}

fn heater_symbol(sym: &mut Sym, b: &Bx) {
    let (c, r) = heater_centre(b);
    sym.layer("object.symbol.above");
    sym.circle(c, r, Pen::Above);
    sym.line((c.0 - 0.06, c.1 + r), (c.0 - 0.06, b.y1), Pen::Above);
    sym.line((c.0 + 0.06, c.1 + r), (c.0 + 0.06, b.y1), Pen::Above);
    sym.layer("object.symbol");
}

fn heater_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let (c, r) = heater_centre(b);
    let z0 = p.get("mount");
    let h = b.h.max(0.3);
    parts.cyl(c, r, (z0, z0 + h), color::WHITE);
    parts.bx([c.0 - 0.08, c.1 + r - 0.02, z0 + h - 0.16], [c.0 + 0.08, b.y1, z0 + h - 0.08], color::METAL);
    for dx in [-0.07, 0.07] {
        parts.cyl((c.0 + dx, c.1), 0.012, (z0 - 0.18, z0), color::METAL);
    }
}

/// Plan symbol of a sanitary or laundry definition; `false` if `id` is not one.
pub(crate) fn symbol(id: &str, sym: &mut Sym, b: &Bx, p: &P) -> bool {
    match id {
        "sanitary.wc" => wc_symbol(sym, b),
        "sanitary.wc_wall" => wc_wall_symbol(sym, b),
        "sanitary.bidet" => bidet_symbol(sym, b),
        "sanitary.basin" => basin_symbol(sym, b, false),
        "sanitary.vanity" => basin_symbol(sym, b, true),
        "sanitary.bath" => bath_symbol(sym, b),
        "sanitary.shower" => shower_symbol(sym, b),
        "sanitary.shower_screen" => shower_screen_symbol(sym, b, p),
        "equipment.washing_machine" => washer_symbol(sym, b),
        "equipment.water_heater" => heater_symbol(sym, b),
        _ => return false,
    }
    true
}

/// 3D parts of a sanitary or laundry definition; `false` if `id` is not one.
pub(crate) fn parts(id: &str, parts: &mut Parts, b: &Bx, p: &P) -> bool {
    match id {
        "sanitary.wc" => wc_parts(parts, b),
        "sanitary.wc_wall" => wc_wall_parts(parts, b),
        "sanitary.bidet" => bidet_parts(parts, b),
        "sanitary.basin" => basin_parts(parts, b, p),
        "sanitary.vanity" => vanity_parts(parts, b),
        "sanitary.bath" => bath_parts(parts, b),
        "sanitary.shower" => shower_parts(parts, b),
        "sanitary.shower_screen" => shower_screen_parts(parts, b, p),
        "equipment.washing_machine" => washer_parts(parts, b),
        "equipment.water_heater" => heater_parts(parts, b, p),
        _ => return false,
    }
    true
}
