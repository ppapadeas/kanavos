//! Point lists for common outlines, shared by plan symbols and 3D parts.
//!
//! All functions return open rings (the last point does not repeat the first) in counter-clockwise order, in
//! metres. They never fail: degenerate sizes give degenerate (but finite) rings, which callers skip.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// A 2D point as `(x, y)` metres.
pub(crate) type P2 = (f64, f64);

/// Number of segments for a full circle of radius `r`: about one every 4 cm, between 12 and 48.
pub(crate) fn segments(r: f64) -> usize {
    let n = (r.abs() * TAU / 0.04).ceil();
    if n.is_finite() { (n as usize).clamp(12, 48) } else { 12 }
}

/// Points of an elliptic arc from `start` sweeping `sweep` radians, `n` segments (`n + 1` points).
pub(crate) fn arc_pts(c: P2, rx: f64, ry: f64, start: f64, sweep: f64, n: usize) -> Vec<P2> {
    let n = n.max(1);
    (0..=n)
        .map(|i| {
            let a = start + sweep * i as f64 / n as f64;
            (c.0 + rx * a.cos(), c.1 + ry * a.sin())
        })
        .collect()
}

/// A full ellipse.
pub(crate) fn ellipse_pts(c: P2, rx: f64, ry: f64) -> Vec<P2> {
    let n = segments(rx.max(ry));
    let mut pts = arc_pts(c, rx, ry, 0.0, TAU, n);
    pts.pop();
    pts
}

/// A rectangle with rounded corners: radius `rf` at the two corners on `y0`, `rb` at the two on `y1`.
/// Radii are limited to half the shorter side.
pub(crate) fn rrect2_pts(x0: f64, y0: f64, x1: f64, y1: f64, rf: f64, rb: f64) -> Vec<P2> {
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));
    let lim = ((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    let rf = rf.clamp(0.0, lim);
    let rb = rb.clamp(0.0, lim);
    let mut pts = Vec::new();
    let mut corner = |cx: f64, cy: f64, r: f64, a0: f64, x: f64, y: f64| {
        if r < 1e-4 {
            pts.push((x, y));
        } else {
            pts.extend(arc_pts((cx, cy), r, r, a0, FRAC_PI_2, 6));
        }
    };
    corner(x1 - rf, y0 + rf, rf, -FRAC_PI_2, x1, y0);
    corner(x1 - rb, y1 - rb, rb, 0.0, x1, y1);
    corner(x0 + rb, y1 - rb, rb, FRAC_PI_2, x0, y1);
    corner(x0 + rf, y0 + rf, rf, PI, x0, y0);
    pts
}

/// A rectangle with all four corners rounded by `r`.
pub(crate) fn rrect_pts(x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> Vec<P2> {
    rrect2_pts(x0, y0, x1, y1, r, r)
}

/// An egg (the plan shape of a WC or bidet bowl): symmetric about `x = cx`, narrower at `y_front` than at
/// `y_back`, widest `2 × half_w`.
pub(crate) fn egg_pts(cx: f64, y_front: f64, y_back: f64, half_w: f64) -> Vec<P2> {
    let cy = (y_front + y_back) / 2.0;
    let ry = (y_back - y_front).abs() / 2.0;
    let n = segments(ry.max(half_w)).max(24);
    // The width factor grows towards the back (sin t > 0 is the back half).
    let raw: Vec<P2> = (0..n)
        .map(|i| {
            let t = TAU * i as f64 / n as f64;
            (t.cos() * (1.0 + 0.22 * t.sin()), cy + ry * t.sin())
        })
        .collect();
    let max = raw.iter().map(|p| p.0.abs()).fold(0.0, f64::max);
    let k = if max > 1e-9 { half_w / max } else { 0.0 };
    raw.into_iter().map(|(x, y)| (cx + x * k, y)).collect()
}

/// An organic, lobed outline (a tree crown or shrub seen from above) of outer radius `r`. `depth` is the
/// relative depth of the notches between lobes (0 … 0.5); `seed` varies the lobes deterministically.
pub(crate) fn blob_pts(c: P2, r: f64, lobes: usize, depth: f64, seed: u32) -> Vec<P2> {
    let lobes = lobes.max(3);
    let depth = depth.clamp(0.0, 0.5);
    let mut pts = Vec::with_capacity(lobes * 8);
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut next = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(state >> 8) / f64::from(1u32 << 24)
    };
    // Uneven lobe widths read as natural foliage.
    let widths: Vec<f64> = (0..lobes).map(|_| 0.7 + 0.6 * next()).collect();
    let total: f64 = widths.iter().sum();
    let phase = next() * TAU;
    let mut a0 = phase;
    for w in &widths {
        let span = TAU * w / total;
        let amp = depth * (0.6 + 0.4 * next());
        let reach = 1.0 - 0.06 * next();
        for k in 0..8 {
            let u = k as f64 / 8.0;
            let rr = r * reach * (1.0 - amp + amp * (PI * u).sin().sqrt());
            let a = a0 + span * u;
            pts.push((c.0 + rr * a.cos(), c.1 + rr * a.sin()));
        }
        a0 += span;
    }
    pts
}

/// `p` rotated about the origin by `a` radians.
pub(crate) fn rotate(p: P2, a: f64) -> P2 {
    let (s, c) = a.sin_cos();
    (p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(pts: &[P2]) -> f64 {
        let n = pts.len();
        (0..n)
            .map(|i| {
                let a = pts[i];
                let b = pts[(i + 1) % n];
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<f64>()
            / 2.0
    }

    #[test]
    fn rings_are_counter_clockwise() {
        assert!(area(&rrect_pts(0.0, 0.0, 2.0, 1.0, 0.2)) > 0.0);
        assert!(area(&ellipse_pts((0.0, 0.0), 1.0, 0.5)) > 0.0);
        assert!(area(&egg_pts(0.0, 0.0, 0.6, 0.18)) > 0.0);
        assert!(area(&blob_pts((0.0, 0.0), 2.0, 9, 0.2, 7)) > 0.0);
    }

    #[test]
    fn rounded_rectangle_keeps_its_box_and_area() {
        let pts = rrect_pts(0.0, 0.0, 2.0, 1.0, 0.1);
        let a = area(&pts);
        let expected = 2.0 - (4.0 - PI) * 0.01;
        assert!((a - expected).abs() < 1e-3, "{a}");
        assert!(
            pts.iter().all(|p| p.0 >= -1e-12 && p.0 <= 2.0 + 1e-12 && p.1 >= -1e-12 && p.1 <= 1.0 + 1e-12)
        );
    }

    #[test]
    fn egg_is_narrower_at_the_front() {
        let pts = egg_pts(0.0, 0.0, 0.6, 0.18);
        let width_at =
            |y: f64| pts.iter().filter(|p| (p.1 - y).abs() < 0.05).map(|p| p.0.abs()).fold(0.0, f64::max);
        assert!(width_at(0.12) < width_at(0.48));
        let max = pts.iter().map(|p| p.0.abs()).fold(0.0, f64::max);
        assert!((max - 0.18).abs() < 1e-9);
    }
}
