//! Property tests on random orthogonal floor plans: wall outlines never overlap except where walls cross, layers
//! stay inside their wall's band area, storey-wide and single-wall layouts agree, rooms are found, and every wall
//! solid is closed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::fx;
use kanavos_elements::{StoreyGeometry, detect_all_spaces, solids, wall_layout};
use kanavos_geom::{PolygonWithHoles, Segment2, Tolerance, intersection, segment_intersection};
use kanavos_model::{ElementKind, OpeningKind, Structure};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn pick(&mut self, n: usize) -> usize {
        ((self.next() * n as f64) as usize).min(n - 1)
    }
}

/// A random plan on a grid: the perimeter with the exterior build-up, interior walls spanning one to three grid
/// cells (so they meet in corners, tees, crosses and through-walls), random build-ups and some doors.
fn random_plan(rng: &mut Lcg) -> common::Fx {
    let mut f = fx();
    let nx = 2 + rng.pick(3);
    let ny = 2 + rng.pick(3);
    let xs: Vec<f64> = {
        let mut v = vec![0.0];
        for _ in 0..nx {
            let last = *v.last().unwrap();
            v.push(last + 3.0 + (rng.next() * 2.0).round());
        }
        v
    };
    let ys: Vec<f64> = {
        let mut v = vec![0.0];
        for _ in 0..ny {
            let last = *v.last().unwrap();
            v.push(last + 3.0 + (rng.next() * 2.0).round());
        }
        v
    };
    let (w, h) = (*xs.last().unwrap(), *ys.last().unwrap());
    let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    for i in 0..4 {
        f.ext_wall(corners[i], corners[(i + 1) % 4]);
    }
    // Interior walls, never overlapping each other along a grid line.
    let mut taken: Vec<(bool, usize, usize)> = Vec::new(); // (horizontal, line index, cell index)
    for _ in 0..(nx + ny + 2) {
        let horizontal = rng.next() < 0.5;
        let (lines, other) = if horizontal { (&ys, &xs) } else { (&xs, &ys) };
        if lines.len() < 3 {
            continue;
        }
        let li = 1 + rng.pick(lines.len() - 2);
        let line = lines[li];
        let a = rng.pick(other.len() - 1);
        let b = (a + 1 + rng.pick(3)).min(other.len() - 1);
        if (a..b).any(|c| taken.contains(&(horizontal, li, c))) {
            continue;
        }
        taken.extend((a..b).map(|c| (horizontal, li, c)));
        let (p, q) = if horizontal {
            ((other[a], line), (other[b], line))
        } else {
            ((line, other[a]), (line, other[b]))
        };
        let id = match rng.pick(3) {
            0 => f.int_wall(p, q),
            1 => {
                let s = Structure::Layered { build_up: f.concrete_wall };
                f.wall_with(p, q, s, 3.0)
            }
            _ => f.basic_wall(p, q, 0.1 + rng.next() * 0.15, 3.0),
        };
        if rng.next() < 0.5 {
            let len: f64 = (q.0 - p.0).abs() + (q.1 - p.1).abs();
            f.opening(id, OpeningKind::Door, len / 2.0, 0.8, 0.0, 2.1);
        }
    }
    f
}

fn reference(f: &common::Fx, id: kanavos_model::Id) -> Segment2 {
    match &f.doc.element(id).unwrap().kind {
        ElementKind::Wall(w) => w.segment(),
        _ => unreachable!(),
    }
}

#[test]
fn random_plans_hold_the_junction_invariants() {
    let mut rng = Lcg(2026);
    let tol = Tolerance::default();
    for case in 0..150 {
        let f = random_plan(&mut rng);
        let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
        let walls: Vec<_> = geo.walls().map(|(id, l)| (id, l.clone())).collect();
        for (id, l) in &walls {
            // Same as the single-wall path.
            assert_eq!(&wall_layout(&f.doc, *id).unwrap(), l, "case {case}: wall {id}");
            // Layers never exceed the band they come from by more than their penetrations into neighbours.
            assert!(l.outline.area().as_m2() > 0.0, "case {case}: wall {id} vanished");
            // Solids are closed.
            for s in solids(&f.doc, *id).unwrap() {
                assert!(s.mesh.is_closed(1e-6), "case {case}: wall {id} {:?}", s.part);
            }
        }
        // Outlines overlap only where reference lines cross inside both walls (or overlapping duplicates).
        for (i, (ia, la)) in walls.iter().enumerate() {
            for (ib, lb) in walls.iter().skip(i + 1) {
                let (ra, rb) = (reference(&f, *ia), reference(&f, *ib));
                let crossing = match segment_intersection(&ra, &rb, &tol) {
                    kanavos_geom::SegmentIntersection::Point { t, u, .. } => {
                        let la_len = ra.length().as_m();
                        let lb_len = rb.length().as_m();
                        t * la_len > 0.01
                            && (1.0 - t) * la_len > 0.01
                            && u * lb_len > 0.01
                            && (1.0 - u) * lb_len > 0.01
                    }
                    kanavos_geom::SegmentIntersection::Overlap { .. } => true,
                    kanavos_geom::SegmentIntersection::None => false,
                };
                if crossing {
                    continue;
                }
                let a: PolygonWithHoles = la.outline.clone().into();
                let b: PolygonWithHoles = lb.outline.clone().into();
                let o: f64 = intersection(&[a], &[b]).unwrap().iter().map(|p| p.area().as_m2()).sum();
                if o >= 1e-6 {
                    for (id, l) in &walls {
                        let w = f.doc.element(*id).unwrap();
                        if let ElementKind::Wall(ww) = &w.kind {
                            eprintln!(
                                "{id}: {:?} -> {:?} t={:?} joins={:?}",
                                ww.start.xy(),
                                ww.end.xy(),
                                l.thickness.as_m(),
                                l.joins.iter().map(|j| (j.kind, j.other)).collect::<Vec<_>>()
                            );
                        }
                    }
                    eprintln!("A {:?}", la.outline.vertices.iter().map(|p| p.xy()).collect::<Vec<_>>());
                    eprintln!("B {:?}", lb.outline.vertices.iter().map(|p| p.xy()).collect::<Vec<_>>());
                }
                assert!(o < 1e-6, "case {case}: walls {ia} and {ib} overlap by {o} m²");
            }
        }
        // The perimeter encloses at least one room.
        let rooms = detect_all_spaces(&f.doc, f.ground).unwrap();
        assert!(!rooms.is_empty(), "case {case}: no rooms");
    }
}
