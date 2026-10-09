//! Polygon triangulation by ear clipping, with holes merged into the outer ring through bridges.
//!
//! Method: D. Eberly, "Triangulation by Ear Clipping" (Geometric Tools, 2002–2015), sections 2–5.
//! Complexity O(n²), which is fine for building element profiles (tens to hundreds of vertices).

use kanavos_geom::{Point2, PolygonWithHoles, Vec2};

use crate::SolidError;

/// Triangulates a polygon with holes.
///
/// Returns the vertex list (outer ring first, then each hole, in input order after orientation is normalised)
/// and counter-clockwise triangles indexing into it.
///
/// Failure modes: fewer than three outer vertices, or a self-intersecting input for which no ear can be found.
pub fn triangulate(shape: &PolygonWithHoles) -> Result<(Vec<Point2>, Vec<[u32; 3]>), SolidError> {
    let shape = shape.oriented();
    let mut points: Vec<Point2> = Vec::new();
    let mut ring: Vec<usize> = Vec::new();
    for p in &shape.outer.vertices {
        ring.push(points.len());
        points.push(*p);
    }
    if ring.len() < 3 {
        return Err(SolidError::Degenerate("polygon needs at least three vertices"));
    }
    // Holes, processed from the one reaching furthest in +X.
    let mut holes: Vec<Vec<usize>> = Vec::new();
    for h in &shape.holes {
        if h.len() < 3 {
            continue;
        }
        let idx: Vec<usize> = h
            .vertices
            .iter()
            .map(|p| {
                points.push(*p);
                points.len() - 1
            })
            .collect();
        holes.push(idx);
    }
    let max_x =
        |h: &Vec<usize>| h.iter().filter_map(|&i| points.get(i)).map(|p| p.x.as_m()).fold(f64::MIN, f64::max);
    holes.sort_by(|a, b| max_x(b).total_cmp(&max_x(a)));
    for hole in holes {
        ring = bridge_hole(&points, &ring, &hole)?;
    }
    let tris = ear_clip(&points, ring)?;
    Ok((points, tris))
}

fn pt(points: &[Point2], i: usize) -> Vec2 {
    points.get(i).map(|p| p.to_vec()).unwrap_or(Vec2::ZERO)
}

/// Joins a clockwise hole to a counter-clockwise ring with a two-way bridge edge.
fn bridge_hole(points: &[Point2], ring: &[usize], hole: &[usize]) -> Result<Vec<usize>, SolidError> {
    // Rightmost hole vertex M.
    let (m_pos, &m) = hole
        .iter()
        .enumerate()
        .max_by(|a, b| pt(points, *a.1).x.total_cmp(&pt(points, *b.1).x))
        .ok_or(SolidError::Degenerate("empty hole"))?;
    let mv = pt(points, m);
    // Ray from M towards +X: closest intersection with a ring edge.
    let n = ring.len();
    let mut best: Option<(f64, usize, Vec2)> = None; // (x of hit, ring position of candidate vertex, hit point)
    for k in 0..n {
        let (Some(&ia), Some(&ib)) = (ring.get(k), ring.get((k + 1) % n)) else { continue };
        let (a, b) = (pt(points, ia), pt(points, ib));
        if (a.y > mv.y) == (b.y > mv.y) && (a.y - mv.y).abs() > 1e-15 && (b.y - mv.y).abs() > 1e-15 {
            continue;
        }
        if (a.y - b.y).abs() < 1e-15 {
            // Horizontal edge on the ray line: take the nearer endpoint to the right.
            for (pos, v) in [(k, a), ((k + 1) % n, b)] {
                if v.x >= mv.x && best.is_none_or(|(bx, _, _)| v.x < bx) && (v.y - mv.y).abs() < 1e-12 {
                    best = Some((v.x, pos, v));
                }
            }
            continue;
        }
        let t = (mv.y - a.y) / (b.y - a.y);
        if !(0.0..=1.0).contains(&t) {
            continue;
        }
        let x = a.x + t * (b.x - a.x);
        if x < mv.x - 1e-12 {
            continue;
        }
        if best.is_none_or(|(bx, _, _)| x < bx) {
            // Candidate: the edge endpoint with the larger x.
            let pos = if a.x > b.x { k } else { (k + 1) % n };
            best = Some((x, pos, Vec2::new(x, mv.y)));
        }
    }
    let (_, mut cand_pos, hit) =
        best.ok_or(SolidError::Triangulation("hole is not inside the outline".into()))?;
    let cand = pt(points, *ring.get(cand_pos).unwrap_or(&0));
    // If a reflex ring vertex lies inside triangle (M, hit, candidate), pick the one with the smallest angle to +X.
    let mut best_angle = f64::MAX;
    let mut best_dist = f64::MAX;
    for k in 0..n {
        let Some(&iv) = ring.get(k) else { continue };
        let v = pt(points, iv);
        if (v - cand).length_squared() < 1e-24 {
            continue;
        }
        let prev = pt(points, *ring.get((k + n - 1) % n).unwrap_or(&iv));
        let next = pt(points, *ring.get((k + 1) % n).unwrap_or(&iv));
        let reflex = (v - prev).cross(next - v) <= 0.0;
        if !reflex || !point_in_triangle(v, mv, hit, cand) {
            continue;
        }
        let d = v - mv;
        let angle = d.y.atan2(d.x).abs();
        let dist = d.length_squared();
        if angle < best_angle - 1e-15 || ((angle - best_angle).abs() <= 1e-15 && dist < best_dist) {
            best_angle = angle;
            best_dist = dist;
            cand_pos = k;
        }
    }
    // New ring: ring[..=cand] + hole from M around back to M + cand + rest of ring.
    let mut out = Vec::with_capacity(ring.len() + hole.len() + 2);
    out.extend_from_slice(ring.get(..=cand_pos).unwrap_or(ring));
    let h = hole.len();
    for j in 0..=h {
        if let Some(&v) = hole.get((m_pos + j) % h) {
            out.push(v);
        }
    }
    if let Some(&c) = ring.get(cand_pos) {
        out.push(c);
    }
    out.extend_from_slice(ring.get(cand_pos + 1..).unwrap_or(&[]));
    Ok(out)
}

fn point_in_triangle(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d1 = (b - a).cross(p - a);
    let d2 = (c - b).cross(p - b);
    let d3 = (a - c).cross(p - c);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

fn ear_clip(points: &[Point2], mut ring: Vec<usize>) -> Result<Vec<[u32; 3]>, SolidError> {
    let mut tris = Vec::with_capacity(ring.len().saturating_sub(2));
    let mut guard = 0usize;
    while ring.len() > 3 {
        guard += 1;
        if guard > 100_000 {
            return Err(SolidError::Triangulation("did not converge".into()));
        }
        let n = ring.len();
        let mut clipped = false;
        for i in 0..n {
            let (Some(&ip), Some(&ic), Some(&inx)) =
                (ring.get((i + n - 1) % n), ring.get(i), ring.get((i + 1) % n))
            else {
                continue;
            };
            let (a, b, c) = (pt(points, ip), pt(points, ic), pt(points, inx));
            let cross = (b - a).cross(c - b);
            if cross <= 1e-14 {
                continue;
            }
            let blocked = ring.iter().any(|&k| {
                if k == ip || k == ic || k == inx {
                    return false;
                }
                let v = pt(points, k);
                if (v - a).length_squared() < 1e-24
                    || (v - b).length_squared() < 1e-24
                    || (v - c).length_squared() < 1e-24
                {
                    return false;
                }
                point_in_triangle(v, a, b, c)
            });
            if !blocked {
                tris.push([ip as u32, ic as u32, inx as u32]);
                ring.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            // Remove collinear or degenerate vertices first; give up only if none exist.
            let n = ring.len();
            let degenerate = (0..n).find(|&i| {
                let (Some(&ip), Some(&ic), Some(&inx)) =
                    (ring.get((i + n - 1) % n), ring.get(i), ring.get((i + 1) % n))
                else {
                    return false;
                };
                let (a, b, c) = (pt(points, ip), pt(points, ic), pt(points, inx));
                (b - a).cross(c - b).abs() <= 1e-14
            });
            match degenerate {
                Some(i) => {
                    ring.remove(i);
                }
                None => return Err(SolidError::Triangulation("polygon may be self-intersecting".into())),
            }
        }
    }
    if let [a, b, c] = ring[..] {
        let (pa, pb, pc) = (pt(points, a), pt(points, b), pt(points, c));
        if (pb - pa).cross(pc - pb).abs() > 1e-14 {
            tris.push([a as u32, b as u32, c as u32]);
        }
    }
    Ok(tris)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Polygon2;

    fn area(points: &[Point2], tris: &[[u32; 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                let a = pt(points, t[0] as usize);
                let b = pt(points, t[1] as usize);
                let c = pt(points, t[2] as usize);
                (b - a).cross(c - a) / 2.0
            })
            .sum()
    }

    #[test]
    fn square_has_two_triangles() {
        let s = PolygonWithHoles::from(Polygon2::from_m(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]));
        let (p, t) = triangulate(&s).unwrap();
        assert_eq!(t.len(), 2);
        assert!((area(&p, &t) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn two_holes_keep_area() {
        let s = PolygonWithHoles {
            outer: Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]),
            holes: vec![
                Polygon2::from_m(&[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)]),
                Polygon2::from_m(&[(6.0, 2.0), (8.0, 2.0), (8.0, 5.0), (6.0, 5.0)]),
            ],
        };
        let (p, t) = triangulate(&s).unwrap();
        assert!((area(&p, &t) - (60.0 - 4.0 - 6.0)).abs() < 1e-9, "area {}", area(&p, &t));
        assert!(t.iter().all(|tri| {
            let (a, b, c) = (pt(&p, tri[0] as usize), pt(&p, tri[1] as usize), pt(&p, tri[2] as usize));
            (b - a).cross(c - a) > 0.0
        }));
    }

    #[test]
    fn clockwise_input_is_accepted() {
        let s = PolygonWithHoles::from(Polygon2::from_m(&[(0.0, 0.0), (0.0, 2.0), (3.0, 2.0), (3.0, 0.0)]));
        let (p, t) = triangulate(&s).unwrap();
        assert!((area(&p, &t) - 6.0).abs() < 1e-12);
    }
}
