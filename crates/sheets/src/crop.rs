//! Cropping a model-space drawing to a viewport's crop rectangle.
//!
//! Lines and polylines are clipped segment by segment (Liang–Barsky); filled polygons ring by ring
//! (Sutherland–Hodgman against the four sides, which is exact for a rectangular window); arcs and circles are
//! kept when their extent overlaps the window and dropped otherwise; texts are kept when their anchor lies
//! inside. Tolerance: none needed — the clip is exact in floating point and degenerate pieces (zero length or
//! fewer than three vertices) are dropped.

use kanavos_drawing::{Drawing, Item, Pt, Rect, Shape, pt};

fn clip_segment(a: Pt, b: Pt, r: &Rect) -> Option<(Pt, Pt)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [(-dx, a.x - r.min.x), (dx, r.max.x - a.x), (-dy, a.y - r.min.y), (dy, r.max.y - a.y)] {
        if p.abs() < 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    if t0 > t1 {
        return None;
    }
    let s = pt(a.x + dx * t0, a.y + dy * t0);
    let e = pt(a.x + dx * t1, a.y + dy * t1);
    (s.distance(e) > 1e-12).then_some((s, e))
}

fn clip_ring(ring: &[Pt], r: &Rect) -> Vec<Pt> {
    // Each edge: (inside test, intersection with the edge line).
    type Side = (fn(Pt, &Rect) -> bool, fn(Pt, Pt, &Rect) -> Pt);
    let sides: [Side; 4] = [
        (|p, r| p.x >= r.min.x, |a, b, r| ix(a, b, r.min.x)),
        (|p, r| p.x <= r.max.x, |a, b, r| ix(a, b, r.max.x)),
        (|p, r| p.y >= r.min.y, |a, b, r| iy(a, b, r.min.y)),
        (|p, r| p.y <= r.max.y, |a, b, r| iy(a, b, r.max.y)),
    ];
    let mut out: Vec<Pt> = ring.to_vec();
    for (inside, cut) in sides {
        let input = std::mem::take(&mut out);
        let Some(&last) = input.last() else { break };
        let mut prev = last;
        for &cur in &input {
            match (inside(cur, r), inside(prev, r)) {
                (true, true) => out.push(cur),
                (true, false) => {
                    out.push(cut(prev, cur, r));
                    out.push(cur);
                }
                (false, true) => out.push(cut(prev, cur, r)),
                (false, false) => {}
            }
            prev = cur;
        }
    }
    out
}

fn ix(a: Pt, b: Pt, x: f64) -> Pt {
    let t = if (b.x - a.x).abs() < 1e-15 { 0.0 } else { (x - a.x) / (b.x - a.x) };
    pt(x, a.y + (b.y - a.y) * t)
}

fn iy(a: Pt, b: Pt, y: f64) -> Pt {
    let t = if (b.y - a.y).abs() < 1e-15 { 0.0 } else { (y - a.y) / (b.y - a.y) };
    pt(a.x + (b.x - a.x) * t, y)
}

fn inside(p: Pt, r: &Rect) -> bool {
    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
}

/// The drawing cut to a rectangle given by two opposite corners (in drawing coordinates).
pub fn crop(drawing: &Drawing, a: Pt, b: Pt) -> Drawing {
    let r = Rect { min: pt(a.x.min(b.x), a.y.min(b.y)), max: pt(a.x.max(b.x), a.y.max(b.y)) };
    let mut out = Drawing { units: drawing.units, scale: drawing.scale, items: Vec::new() };
    for item in &drawing.items {
        let with = |shape: Shape| Item { shape, ..item.clone() };
        match &item.shape {
            Shape::Line { a, b } => {
                if let Some((s, e)) = clip_segment(*a, *b, &r) {
                    out.push(with(Shape::Line { a: s, b: e }));
                }
            }
            Shape::Polyline { points, closed } => {
                let mut segs: Vec<(Pt, Pt)> = points
                    .windows(2)
                    .filter_map(|w| match w {
                        [p, q] => Some((*p, *q)),
                        _ => None,
                    })
                    .collect();
                if *closed && let (Some(f), Some(l)) = (points.first(), points.last()) {
                    segs.push((*l, *f));
                }
                if points.iter().all(|p| inside(*p, &r)) {
                    out.push(item.clone());
                    continue;
                }
                // Re-join consecutive clipped pieces into polylines.
                let mut run: Vec<Pt> = Vec::new();
                for (p, q) in segs {
                    match clip_segment(p, q, &r) {
                        Some((s, e)) => {
                            let joins = run.last().is_some_and(|l| l.distance(s) < 1e-9);
                            if !joins && run.len() >= 2 {
                                out.push(with(Shape::Polyline {
                                    points: std::mem::take(&mut run),
                                    closed: false,
                                }));
                            }
                            if !joins {
                                run.clear();
                                run.push(s);
                            }
                            run.push(e);
                        }
                        None => {
                            if run.len() >= 2 {
                                out.push(with(Shape::Polyline {
                                    points: std::mem::take(&mut run),
                                    closed: false,
                                }));
                            }
                            run.clear();
                        }
                    }
                }
                if run.len() >= 2 {
                    out.push(with(Shape::Polyline { points: run, closed: false }));
                }
            }
            Shape::Polygon { rings } => {
                let clipped: Vec<Vec<Pt>> =
                    rings.iter().map(|ring| clip_ring(ring, &r)).filter(|ring| ring.len() >= 3).collect();
                if !clipped.is_empty() {
                    out.push(with(Shape::Polygon { rings: clipped }));
                }
            }
            Shape::Arc { .. } | Shape::Circle { .. } => {
                let pts = item.shape.extent_points();
                let overlaps = pts.iter().any(|p| inside(*p, &r));
                if overlaps {
                    out.push(item.clone());
                }
            }
            Shape::Text(t) => {
                if inside(t.position, &r) {
                    out.push(item.clone());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::{Stroke, weights};

    #[test]
    fn lines_and_polygons_are_cut_to_the_window() {
        let mut d = Drawing::model(100.0);
        d.push(Item::line(pt(-5.0, 1.0), pt(15.0, 1.0), Stroke::solid(weights::THIN), "a"));
        d.push(Item::line(pt(-5.0, 20.0), pt(15.0, 20.0), Stroke::solid(weights::THIN), "b"));
        d.push(Item::polygon(
            vec![vec![pt(-2.0, -2.0), pt(5.0, -2.0), pt(5.0, 5.0), pt(-2.0, 5.0)]],
            None,
            None,
            "c",
        ));
        let c = crop(&d, pt(0.0, 0.0), pt(10.0, 10.0));
        assert_eq!(c.items.len(), 2);
        let b = c.bounds().unwrap();
        assert!(b.min.x >= -1e-9 && b.max.x <= 10.0 + 1e-9 && b.min.y >= -1e-9 && b.max.y <= 10.0 + 1e-9);
        match &c.items[1].shape {
            Shape::Polygon { rings } => {
                let ring = &rings[0];
                let area: f64 = (0..ring.len())
                    .map(|i| {
                        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
                        p.x * q.y - q.x * p.y
                    })
                    .sum::<f64>()
                    / 2.0;
                assert!((area.abs() - 25.0).abs() < 1e-9);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn polylines_split_where_they_leave_the_window() {
        let mut d = Drawing::model(100.0);
        d.push(Item::polyline(
            vec![pt(1.0, 1.0), pt(20.0, 1.0), pt(20.0, 2.0), pt(1.0, 2.0)],
            false,
            Stroke::solid(weights::THIN),
            "p",
        ));
        let c = crop(&d, pt(0.0, 0.0), pt(10.0, 10.0));
        assert_eq!(c.items.len(), 2);
    }
}
