//! Dash patterns: splitting a polyline into visible dashes.
//!
//! Local stand-in for the `dash` helper planned in `kanavos-drawing`; [`dash_polyline`] is the single entry point
//! so that it can be swapped for the shared one later.

use kanavos_drawing::{Pt, pt};

/// Upper bound on the number of dashes produced for one polyline. Beyond it the polyline is returned solid, which
/// only happens when the caller asks for a pattern far below screen resolution.
pub const MAX_DASHES: usize = 50_000;

/// Splits a polyline into dashes.
///
/// `pattern` holds alternating on/off lengths in drawing units, starting with a dash (as
/// `LinePattern::dashes_mm` gives them, converted from paper millimetres by the caller). The pattern runs
/// continuously across vertices. A zero-length "on" entry makes a dot, returned as a two-point piece of zero
/// length that the caller draws as a round dot.
///
/// Returns the visible pieces. An empty or degenerate pattern (no positive length) returns the polyline
/// unchanged as one piece; so does a pattern that would produce more than [`MAX_DASHES`] dashes.
///
/// Tolerance: none needed; lengths are compared with `<`/`>` only.
pub fn dash_polyline(points: &[Pt], closed: bool, pattern: &[f64]) -> Vec<Vec<Pt>> {
    let mut pts: Vec<Pt> = points.to_vec();
    if closed && let Some(first) = points.first().copied() {
        pts.push(first);
    }
    if pts.len() < 2 {
        return Vec::new();
    }
    let pattern: Vec<f64> = pattern.iter().map(|v| if v.is_finite() { v.abs() } else { 0.0 }).collect();
    let period: f64 = pattern.iter().sum();
    let total: f64 = pts.windows(2).map(seg_len).sum();
    if pattern.len() < 2
        || period <= 0.0
        || !total.is_finite()
        || total / period * (pattern.len() as f64) > MAX_DASHES as f64
    {
        return vec![pts];
    }
    let mut pieces: Vec<Vec<Pt>> = Vec::new();
    // State of the pattern walk: index of the current element, length left in it.
    let mut idx = 0usize;
    let mut left = pattern.first().copied().unwrap_or(0.0);
    let mut current: Vec<Pt> = Vec::new();
    for w in pts.windows(2) {
        let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else { continue };
        let len = a.distance(b);
        if len <= 0.0 {
            continue;
        }
        let mut t = 0.0;
        loop {
            let on = idx.is_multiple_of(2);
            if on && left <= 0.0 {
                // A dot: a zero-length dash at the current position.
                let p = lerp(a, b, t / len);
                pieces.push(vec![p, p]);
            }
            let remaining = len - t;
            if left >= remaining {
                // The element runs past the end of this edge: carry it into the next one.
                if on {
                    if current.is_empty() {
                        current.push(lerp(a, b, t / len));
                    }
                    current.push(b);
                }
                left -= remaining;
                break;
            }
            if on && left > 0.0 {
                if current.is_empty() {
                    current.push(lerp(a, b, t / len));
                }
                current.push(lerp(a, b, (t + left) / len));
                pieces.push(std::mem::take(&mut current));
            }
            t += left;
            idx = (idx + 1) % pattern.len();
            left = pattern.get(idx).copied().unwrap_or(0.0);
        }
    }
    if current.len() >= 2 {
        pieces.push(current);
    }
    pieces
}

fn seg_len(w: &[Pt]) -> f64 {
    match (w.first(), w.get(1)) {
        (Some(a), Some(b)) => a.distance(*b),
        _ => 0.0,
    }
}

fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// Total length of the visible pieces (for tests).
#[cfg(test)]
pub fn visible_length(pieces: &[Vec<Pt>]) -> f64 {
    pieces.iter().map(|p| p.windows(2).map(seg_len).sum::<f64>()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_pattern_returns_the_line() {
        let pieces = dash_polyline(&[pt(0.0, 0.0), pt(1.0, 0.0)], false, &[]);
        assert_eq!(pieces.len(), 1);
    }

    #[test]
    fn dashes_follow_the_pattern() {
        // 10 units, pattern 3 on 1 off: dashes at 0-3, 4-7, 8-10.
        let pieces = dash_polyline(&[pt(0.0, 0.0), pt(10.0, 0.0)], false, &[3.0, 1.0]);
        assert_eq!(pieces.len(), 3, "{pieces:?}");
        assert!((visible_length(&pieces) - 8.0).abs() < 1e-9);
        let last = pieces.last().unwrap();
        assert!((last.first().unwrap().x - 8.0).abs() < 1e-9);
    }

    #[test]
    fn pattern_continues_around_corners() {
        // An L of 2 + 2 with pattern 3/1: the first dash turns the corner.
        let pieces = dash_polyline(&[pt(0.0, 0.0), pt(2.0, 0.0), pt(2.0, 2.0)], false, &[3.0, 1.0]);
        let first = pieces.first().unwrap();
        assert_eq!(first.len(), 3, "dash keeps the corner vertex: {first:?}");
        assert!((visible_length(&pieces) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn closed_polylines_include_the_closing_edge() {
        let square = [pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let pieces = dash_polyline(&square, true, &[0.5, 0.5]);
        assert!((visible_length(&pieces) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn dots_are_zero_length_pieces() {
        let pieces = dash_polyline(&[pt(0.0, 0.0), pt(4.0, 0.0)], false, &[0.0, 1.0]);
        assert!(pieces.len() >= 4, "{pieces:?}");
        assert!(pieces.iter().all(|p| p.len() == 2));
    }

    #[test]
    fn absurdly_fine_patterns_fall_back_to_solid() {
        let pieces = dash_polyline(&[pt(0.0, 0.0), pt(1000.0, 0.0)], false, &[1e-6, 1e-6]);
        assert_eq!(pieces.len(), 1);
    }
}
