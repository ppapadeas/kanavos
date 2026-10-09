//! Sweeping a profile along a polyline (rails, handrails, cornices) with mitred joints.

use kanavos_geom::{Point2, Point3, Polygon2, PolygonWithHoles, Vec3};

use crate::{Mesh, SolidError, triangulate};

/// Local frame of a path segment: `side` is horizontal to the left of travel, `up` completes the frame.
fn frame(t: Vec3) -> (Vec3, Vec3) {
    let side = Vec3::Z.cross(t).normalized().unwrap_or(Vec3::Y);
    let up = t.cross(side).normalized().unwrap_or(Vec3::Z);
    (side, up)
}

/// Sweeps a closed `profile` along an open `path`.
///
/// Profile coordinates: `x` to the left of travel (horizontal), `y` up (perpendicular to the path in its vertical
/// plane). At interior path points the profile is cut by the mitre plane (the bisector of the two segments), so
/// joints are closed; the ends are capped. The result is closed and oriented outwards.
///
/// Limits: the mitre of a joint turning back on itself (more than about 170°) is extremely long; such joints
/// are rejected. Failure modes: fewer than two distinct path points or a degenerate profile give
/// [`SolidError::Degenerate`].
pub fn sweep(path: &[Point3], profile: &Polygon2) -> Result<Mesh, SolidError> {
    let mut pts: Vec<Point3> = Vec::with_capacity(path.len());
    for &p in path {
        if pts.last().is_none_or(|q| q.distance(p) > 1e-9) {
            pts.push(p);
        }
    }
    if pts.len() < 2 {
        return Err(SolidError::Degenerate("a sweep path needs two distinct points"));
    }
    let prof = profile.to_ccw();
    if prof.len() < 3 || prof.area().as_m2() <= 1e-14 {
        return Err(SolidError::Degenerate("sweep profile has no area"));
    }
    let dirs: Vec<Vec3> =
        pts.windows(2).filter_map(|w| if let [a, b] = w { (*b - *a).normalized() } else { None }).collect();
    // Ring of profile points at each path point.
    let mut rings: Vec<Vec<Point3>> = Vec::with_capacity(pts.len());
    for (i, &p) in pts.iter().enumerate() {
        let t_in = if i == 0 { None } else { dirs.get(i - 1).copied() };
        let t_out = dirs.get(i).copied();
        let t_seg = t_out.or(t_in).unwrap_or(Vec3::X);
        let (side, up) = frame(t_seg);
        let mut ring = Vec::with_capacity(prof.len());
        // Mitre plane normal at interior points.
        let mitre = match (t_in, t_out) {
            (Some(a), Some(b)) => {
                if a.dot(b) < -0.985 {
                    return Err(SolidError::Degenerate("the path turns back on itself"));
                }
                (a + b).normalized()
            }
            _ => None,
        };
        for q in &prof.vertices {
            let [x, y] = q.xy();
            let mut v = p + side * x + up * y;
            if let Some(n) = mitre {
                // Slide along the segment direction onto the mitre plane through p.
                let denom = t_seg.dot(n);
                if denom.abs() > 1e-9 {
                    let s = (p - v).dot(n) / denom;
                    v = v + t_seg * s;
                }
            }
            ring.push(v);
        }
        rings.push(ring);
    }
    let mut mesh = Mesh::new();
    let m = prof.len();
    for w in rings.windows(2) {
        let [a, b] = w else { continue };
        for k in 0..m {
            let (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
                (a.get(k), a.get((k + 1) % m), b.get(k), b.get((k + 1) % m))
            else {
                continue;
            };
            // The profile is counter-clockwise in (side, up), whose normal is the travel direction: its edges'
            // outward side faces run a0 → a1 → b1 → b0.
            mesh.push_quad(a0, a1, b1, b0);
        }
    }
    // End caps.
    let (tri_pts, tris) = triangulate(&PolygonWithHoles::from(prof.clone()))?;
    let cap = |ring: &Vec<Point3>, flip: bool, mesh: &mut Mesh| {
        let base = mesh.positions.len() as u32;
        for p in &tri_pts {
            // Triangulation points are the profile vertices in order.
            let idx = prof.vertices.iter().position(|q| (*q - *p).length() < 1e-12).unwrap_or(0);
            mesh.push_vertex(ring.get(idx).copied().unwrap_or(Point3::ORIGIN));
        }
        for t in &tris {
            if flip {
                mesh.push_triangle(base + t[0], base + t[2], base + t[1]);
            } else {
                mesh.push_triangle(base + t[0], base + t[1], base + t[2]);
            }
        }
    };
    // The start cap faces backwards, the end cap forwards.
    if let Some(first) = rings.first() {
        cap(first, true, &mut mesh);
    }
    if let Some(last) = rings.last() {
        cap(last, false, &mut mesh);
    }
    let mesh = mesh.welded(1e-9);
    // Orientation check: flip if the volume came out negative.
    Ok(if mesh.volume() < 0.0 { mesh.flipped() } else { mesh })
}

/// A rectangular profile `width × height` centred on the path horizontally, with its top `top_offset` above it.
pub fn rect_profile(width: f64, height: f64, top_offset: f64) -> Polygon2 {
    let w = width / 2.0;
    Polygon2::new(vec![
        Point2::new(-w, top_offset - height),
        Point2::new(w, top_offset - height),
        Point2::new(w, top_offset),
        Point2::new(-w, top_offset),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_sweep_is_a_box() {
        let m =
            sweep(&[Point3::new(0.0, 0.0, 1.0), Point3::new(4.0, 0.0, 1.0)], &rect_profile(0.05, 0.04, 0.0))
                .unwrap();
        assert!(m.is_closed(1e-9));
        assert!((m.volume() - 4.0 * 0.05 * 0.04).abs() < 1e-12);
    }

    #[test]
    fn mitred_l_sweep_keeps_centreline_volume() {
        let path = [Point3::new(0.0, 0.0, 1.0), Point3::new(4.0, 0.0, 1.0), Point3::new(4.0, 3.0, 1.0)];
        let prof = rect_profile(0.1, 0.05, 0.025);
        let m = sweep(&path, &prof).unwrap();
        assert!(m.is_closed(1e-9));
        // For a profile centred on the path, a mitred corner keeps volume = area × centreline length.
        assert!((m.volume() - 7.0 * 0.1 * 0.05).abs() < 1e-9, "volume {}", m.volume());
    }

    #[test]
    fn sloped_sweep() {
        let path = [Point3::new(0.0, 0.0, 0.0), Point3::new(3.0, 0.0, 1.5)];
        let m = sweep(&path, &rect_profile(0.05, 0.05, 0.025)).unwrap();
        let len = (9.0f64 + 2.25).sqrt();
        assert!((m.volume() - len * 0.0025).abs() < 1e-9);
    }
}
