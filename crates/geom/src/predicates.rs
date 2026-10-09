//! Robust geometric predicates.
//!
//! [`orient2d`] decides on which side of a line a point lies. It first evaluates the determinant in plain
//! floating point and accepts the sign when it exceeds a forward error bound; otherwise it re-evaluates the
//! determinant exactly with floating-point expansions. Method: J. R. Shewchuk, "Adaptive Precision
//! Floating-Point Arithmetic and Fast Robust Geometric Predicates", Discrete & Computational Geometry 18 (1997),
//! sections 2 and 4 (error-free transformations `two_sum` and `two_product`, expansion growth).
//!
//! The exact path is only taken for nearly collinear inputs, so the predicate costs about as much as the naive
//! formula in practice.

use std::cmp::Ordering;

/// Error-free sum: returns `(s, e)` with `s = fl(a + b)` and `a + b = s + e` exactly (Knuth's two-sum).
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    let err = (a - (s - bb)) + (b - bb);
    (s, err)
}

/// Error-free product: returns `(p, e)` with `p = fl(a·b)` and `a·b = p + e` exactly (uses a fused multiply-add).
fn two_product(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let e = a.mul_add(b, -p);
    (p, e)
}

/// Adds a double to a non-overlapping expansion (Shewchuk's Grow-Expansion with zero elimination).
fn grow(expansion: &mut Vec<f64>, b: f64) {
    let mut q = b;
    let mut out = Vec::with_capacity(expansion.len() + 1);
    for &e in expansion.iter() {
        let (s, err) = two_sum(q, e);
        if err != 0.0 {
            out.push(err);
        }
        q = s;
    }
    if q != 0.0 {
        out.push(q);
    }
    *expansion = out;
}

/// Sign of an exactly evaluated sum of products `Σ a_i·b_i`.
fn exact_sign_of_products(terms: &[(f64, f64)]) -> Ordering {
    let mut exp: Vec<f64> = Vec::with_capacity(terms.len() * 2);
    for &(a, b) in terms {
        let (p, e) = two_product(a, b);
        grow(&mut exp, e);
        grow(&mut exp, p);
    }
    // In a non-overlapping expansion with zeros removed, the last component dominates the sum.
    match exp.last() {
        Some(v) if *v > 0.0 => Ordering::Greater,
        Some(v) if *v < 0.0 => Ordering::Less,
        _ => Ordering::Equal,
    }
}

/// Orientation of the triple `(a, b, c)`: [`Ordering::Greater`] when `c` lies to the left of the directed line
/// `a → b` (a counter-clockwise turn), [`Ordering::Less`] when it lies to the right, [`Ordering::Equal`] when the
/// three points are exactly collinear.
///
/// Exact for all finite inputs (no tolerance). Non-finite inputs return [`Ordering::Equal`].
pub fn orient2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Ordering {
    let detleft = (a[0] - c[0]) * (b[1] - c[1]);
    let detright = (a[1] - c[1]) * (b[0] - c[0]);
    let det = detleft - detright;
    if !det.is_finite() {
        return Ordering::Equal;
    }
    // Shewchuk's error bound for the floating-point evaluation (ccwerrboundA = (3 + 16ε)ε).
    const EPS: f64 = f64::EPSILON * 0.5;
    const BOUND: f64 = (3.0 + 16.0 * EPS) * EPS;
    let detsum = detleft.abs() + detright.abs();
    if det.abs() > BOUND * detsum {
        return if det > 0.0 { Ordering::Greater } else { Ordering::Less };
    }
    // Exact fallback. Expanding the determinant removes the rounded differences:
    // (ax-cx)(by-cy) - (ay-cy)(bx-cx) = ax·by - ax·cy - cx·by - ay·bx + ay·cx + cy·bx.
    let [ax, ay] = a;
    let [bx, by] = b;
    let [cx, cy] = c;
    exact_sign_of_products(&[(ax, by), (-ax, cy), (-cx, by), (-ay, bx), (ay, cx), (cy, bx)])
}

/// Signed doubled area of the triangle `(a, b, c)` in plain floating point: positive for counter-clockwise
/// order. Use [`orient2d`] when only the sign matters and it must be exact.
pub fn cross3(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_orientations() {
        assert_eq!(orient2d([0.0, 0.0], [1.0, 0.0], [0.0, 1.0]), Ordering::Greater);
        assert_eq!(orient2d([0.0, 0.0], [1.0, 0.0], [0.0, -1.0]), Ordering::Less);
        assert_eq!(orient2d([0.0, 0.0], [1.0, 1.0], [2.0, 2.0]), Ordering::Equal);
    }

    #[test]
    fn nearly_collinear_points_are_decided_exactly() {
        // Points on the line y = x shifted by one unit in the last place: the naive formula is unreliable here.
        let a = [0.5, 0.5];
        let b = [12.0, 12.0];
        let c_on = [24.0, 24.0];
        assert_eq!(orient2d(a, b, c_on), Ordering::Equal);
        let above = [24.0, f64::from_bits(24.0f64.to_bits() + 1)];
        let below = [24.0, f64::from_bits(24.0f64.to_bits() - 1)];
        assert_eq!(orient2d(a, b, above), Ordering::Greater);
        assert_eq!(orient2d(a, b, below), Ordering::Less);
    }

    #[test]
    fn exact_path_agrees_with_rational_reasoning() {
        // Grid of tiny perturbations around a collinear configuration (Shewchuk's classic test).
        let a = [0.5, 0.5];
        let b = [12.0, 12.0];
        for i in 0..32u64 {
            for j in 0..32u64 {
                let x = f64::from_bits(0.5f64.to_bits() + i);
                let y = f64::from_bits(0.5f64.to_bits() + j);
                let o = orient2d([x, y], a, b);
                // Exact value: (x - 12)(-11.5) - (y - 12)(-11.5) = -11.5 (x - y).
                let expected = y.partial_cmp(&x).unwrap_or(Ordering::Equal);
                assert_eq!(o, expected, "i={i} j={j}");
            }
        }
    }
}
