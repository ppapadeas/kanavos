//! Trees, shrubs and cars. All are centred on their insertion point; a car's front points towards +y.

use std::f64::consts::TAU;

use kanavos_drawing::Rgba;

use crate::Bx;
use crate::params::P;
use crate::parts::{Parts, color};
use crate::shape::{P2, blob_pts, ellipse_pts, rrect_pts, rrect2_pts};
use crate::sym::{Pen, Sym};

/// An outline scaled from a circle of radius `r` to the box's aspect ratio.
fn stretched(pts: Vec<P2>, b: &Bx) -> Vec<P2> {
    let k = if b.w() > 1e-9 { b.d() / b.w() } else { 1.0 };
    pts.into_iter().map(|(x, y)| (x, b.cy() + (y - b.cy()) * k)).collect()
}

fn tree_symbol(sym: &mut Sym, b: &Bx) {
    let r = b.w() / 2.0;
    let c = (b.cx(), b.cy());
    sym.poly(&stretched(blob_pts(c, r, 11, 0.16, 3), b), true, Pen::Fine);
    let trunk = (b.w() * 0.03).clamp(0.06, 0.3);
    sym.circle(c, trunk, Pen::Fine);
    if sym.fine() {
        sym.poly(&stretched(blob_pts(c, r * 0.6, 7, 0.22, 11), b), true, Pen::Detail);
        // Main branches.
        for k in 0..6 {
            let a = 0.4 + TAU * k as f64 / 6.0;
            let (s, cc) = a.sin_cos();
            let k2 = if b.w() > 1e-9 { b.d() / b.w() } else { 1.0 };
            let p0 = (c.0 + trunk * cc, c.1 + trunk * s * k2);
            let p1 = (c.0 + r * 0.45 * cc, c.1 + r * 0.45 * s * k2);
            sym.line(p0, p1, Pen::Detail);
        }
    }
}

fn tree_parts(parts: &mut Parts, b: &Bx, p: &P) {
    let h = b.h.max(1.0);
    let trunk = p.get("trunk").min(h * 0.6);
    let crown = h - trunk;
    let (rx, ry) = (b.w() / 2.0, b.d() / 2.0);
    let rt = (b.w() * 0.03).clamp(0.06, 0.3);
    let c = (b.cx(), b.cy());
    parts.cyl(c, rt, (0.0, trunk + crown * 0.4), color::BARK);
    let zc = trunk + crown / 2.0;
    // A core and a ring of overlapping masses at varying heights read as foliage.
    parts.ellipsoid([c.0, c.1, zc + crown * 0.02], [rx * 0.72, ry * 0.72, crown * 0.47], color::FOLIAGE);
    for k in 0..7 {
        let a = 0.3 + TAU * k as f64 / 7.0;
        let dz = [0.08, -0.12, 0.15, -0.05, 0.1, -0.14, 0.02].get(k).copied().unwrap_or(0.0) * crown;
        let colour = if k % 2 == 0 { color::FOLIAGE_DARK } else { color::FOLIAGE };
        parts.ellipsoid(
            [c.0 + rx * 0.5 * a.cos(), c.1 + ry * 0.5 * a.sin(), zc + dz],
            [rx * 0.42, ry * 0.42, crown * 0.3],
            colour,
        );
    }
}

fn shrub_symbol(sym: &mut Sym, b: &Bx) {
    let c = (b.cx(), b.cy());
    sym.poly(&stretched(blob_pts(c, b.w() / 2.0, 13, 0.22, 5), b), true, Pen::Fine);
    if sym.fine() {
        sym.poly(&stretched(blob_pts(c, b.w() * 0.25, 7, 0.25, 9), b), true, Pen::Detail);
    }
    sym.circle(c, 0.02, Pen::Detail);
}

fn shrub_parts(parts: &mut Parts, b: &Bx) {
    let h = b.h.max(0.2);
    let (rx, ry) = (b.w() / 2.0, b.d() / 2.0);
    let c = (b.cx(), b.cy());
    parts.ellipsoid([c.0, c.1, h * 0.5], [rx * 0.9, ry * 0.9, h * 0.5], color::FOLIAGE);
    parts.ellipsoid(
        [c.0 + rx * 0.3, c.1 - ry * 0.25, h * 0.42],
        [rx * 0.55, ry * 0.55, h * 0.4],
        color::FOLIAGE_DARK,
    );
}

/// Key stations of a car along its length, as fractions of the depth from the centre.
struct CarShape {
    /// Base of the windscreen.
    screen: f64,
    /// Top of the windscreen (front of the roof).
    roof_front: f64,
    /// Rear of the roof.
    roof_back: f64,
    /// Base of the rear window.
    rear: f64,
    /// Half-width of the glasshouse at the belt line.
    belt: f64,
    /// Half-width of the roof.
    roof: f64,
}

fn car_shape(b: &Bx) -> CarShape {
    let (d, w, cy) = (b.d(), b.w(), b.cy());
    CarShape {
        screen: cy + 0.2 * d,
        roof_front: cy + 0.07 * d,
        roof_back: cy - 0.21 * d,
        rear: cy - 0.31 * d,
        belt: 0.45 * w,
        roof: 0.37 * w,
    }
}

fn car_symbol(sym: &mut Sym, b: &Bx) {
    let s = car_shape(b);
    let cx = b.cx();
    let body = rrect2_pts(b.x0, b.y0, b.x1, b.y1, (b.w() * 0.17).min(0.3), (b.w() * 0.25).min(0.45));
    sym.poly(&body, true, Pen::Fine);
    sym.poly(
        &[
            (cx - s.belt, s.rear),
            (cx + s.belt, s.rear),
            (cx + s.belt * 0.97, s.screen),
            (cx - s.belt * 0.97, s.screen),
        ],
        true,
        Pen::Fine,
    );
    sym.rect(cx - s.roof, s.roof_back, cx + s.roof, s.roof_front, Pen::Fine);
    if sym.fine() {
        for sx in [-1.0, 1.0] {
            sym.line((cx + sx * s.roof, s.roof_front), (cx + sx * s.belt * 0.97, s.screen), Pen::Detail);
            sym.line((cx + sx * s.roof, s.roof_back), (cx + sx * s.belt, s.rear), Pen::Detail);
            // Wing mirrors.
            let mx = cx + sx * (b.w() / 2.0 + 0.06);
            sym.rect(mx - 0.05, s.screen - 0.1, mx + 0.05, s.screen - 0.04, Pen::Detail);
        }
    }
}

fn car_parts(parts: &mut Parts, b: &Bx) {
    let s = car_shape(b);
    let (cx, w) = (b.cx(), b.w());
    let (yf, yr) = (b.y1, b.y0);
    let h = b.h.max(1.0);
    let belt_z = h * 0.6;
    let profile = [
        (yr, 0.3),
        (yf, 0.3),
        (yf, 0.62),
        (yf - 0.06, 0.7),
        (s.screen + 0.25, belt_z - 0.04),
        (s.screen, belt_z),
        (s.rear, belt_z),
        (yr + 0.12, belt_z - 0.06),
        (yr, 0.66),
    ];
    parts.prism_x(&profile, (cx - w / 2.0, cx + w / 2.0), color::CAR);
    let glass = [(s.screen, belt_z), (s.roof_front, h - 0.05), (s.roof_back, h - 0.05), (s.rear, belt_z)];
    parts.prism_x(&glass, (cx - s.belt, cx + s.belt), color::TINTED);
    parts.prism(
        &rrect_pts(cx - s.roof, s.roof_back - 0.02, cx + s.roof, s.roof_front + 0.02, 0.05),
        (h - 0.05, h),
        color::CAR,
    );
    let wr = (h * 0.21).min(0.34);
    for wy in [b.cy() + 0.3 * b.d(), b.cy() - 0.3 * b.d()] {
        let wheel = ellipse_pts((wy, wr), wr, wr);
        let hub = ellipse_pts((wy, wr), wr * 0.55, wr * 0.55);
        for (x0, x1, hx0, hx1) in [
            (cx + w / 2.0 - 0.22, cx + w / 2.0 - 0.01, cx + w / 2.0 - 0.012, cx + w / 2.0 - 0.004),
            (cx - w / 2.0 + 0.01, cx - w / 2.0 + 0.22, cx - w / 2.0 + 0.004, cx - w / 2.0 + 0.012),
        ] {
            parts.prism_x(&wheel, (x0, x1), color::TYRE);
            parts.prism_x(&hub, (hx0, hx1), color::METAL);
        }
    }
    // Bumpers and lights.
    parts.bx([cx - w / 2.0 + 0.05, yf - 0.02, 0.3], [cx + w / 2.0 - 0.05, yf + 0.03, 0.42], color::DARK);
    parts.bx([cx - w / 2.0 + 0.05, yr - 0.03, 0.3], [cx + w / 2.0 - 0.05, yr + 0.02, 0.42], color::DARK);
    for sx in [-1.0, 1.0] {
        let (a, bb) = (cx + sx * w * 0.3, cx + sx * w * 0.45);
        parts.bx([a.min(bb), yf - 0.01, 0.55], [a.max(bb), yf + 0.005, 0.62], color::WHITE);
        parts.bx([a.min(bb), yr - 0.005, 0.55], [a.max(bb), yr + 0.01, 0.62], Rgba::rgb(170, 70, 64));
    }
}

/// Plan symbol of an outdoor definition; `false` if `id` is not one.
pub(crate) fn symbol(id: &str, sym: &mut Sym, b: &Bx, _p: &P) -> bool {
    match id {
        "vegetation.tree" => tree_symbol(sym, b),
        "vegetation.shrub" => shrub_symbol(sym, b),
        "vehicle.car" => car_symbol(sym, b),
        _ => return false,
    }
    true
}

/// 3D parts of an outdoor definition; `false` if `id` is not one.
pub(crate) fn parts(id: &str, parts: &mut Parts, b: &Bx, p: &P) -> bool {
    match id {
        "vegetation.tree" => tree_parts(parts, b, p),
        "vegetation.shrub" => shrub_parts(parts, b),
        "vehicle.car" => car_parts(parts, b),
        _ => return false,
    }
    true
}
