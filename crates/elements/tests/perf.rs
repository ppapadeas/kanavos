//! Performance budget: the walls of a 200-wall storey resolve in under 20 ms (release build).
//!
//! `cargo test -p kanavos-elements --release --test perf -- --nocapture` prints the timings.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::time::Instant;

use common::fx;
use kanavos_elements::StoreyGeometry;
use kanavos_geom::Length;
use kanavos_model::OpeningKind;

/// A 10 × 10 grid of 4 m rooms: 220 walls meeting in corners, tees and four-way nodes; exterior build-up on the
/// perimeter, interior walls inside, a door in every interior wall.
fn grid() -> common::Fx {
    let mut f = fx();
    let n = 10;
    let s = 4.0;
    for line in 0..=n {
        for seg in 0..n {
            let (a, b) = (seg as f64 * s, (seg + 1) as f64 * s);
            let c = line as f64 * s;
            let edge = line == 0 || line == n;
            for (p, q) in [((a, c), (b, c)), ((c, a), (c, b))] {
                let w = if edge { f.ext_wall(p, q) } else { f.int_wall(p, q) };
                if !edge {
                    f.opening(w, OpeningKind::Door, 2.0, 0.9, 0.0, 2.1);
                }
            }
        }
    }
    f
}

#[test]
fn two_hundred_walls_resolve_within_budget() {
    let f = grid();
    let walls = f.doc.walls_on(f.ground).count();
    assert_eq!(walls, 220);
    // Warm up, then take the best of five.
    let _ = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
    let mut best = f64::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
        best = best.min(t.elapsed().as_secs_f64());
        assert_eq!(geo.walls().count(), 220);
    }
    let geo = StoreyGeometry::compute(&f.doc, f.ground).unwrap();
    let t = Instant::now();
    let cut = Length::m(1.1);
    let mut pieces = 0;
    for (id, _) in geo.walls() {
        pieces += geo.plan_parts(&f.doc, id, cut).unwrap().cut.len();
    }
    let plan = t.elapsed().as_secs_f64();
    // The per-element path (what a view calling `plan_parts` for every element does).
    let t = Instant::now();
    for (id, _) in geo.walls() {
        let _ = kanavos_elements::plan_parts(&f.doc, id, cut).unwrap();
    }
    let single = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut volume = 0.0;
    for (id, _) in geo.walls() {
        volume += geo.solids(&f.doc, id).unwrap().iter().map(|s| s.mesh.volume()).sum::<f64>();
    }
    let solids = t.elapsed().as_secs_f64();
    println!(
        "storey junctions: {:.2} ms; plan parts of all walls: {:.2} ms (one by one: {:.2} ms); solids: {:.2} ms; \
         {pieces} cut pieces, {volume:.2} m³",
        best * 1e3,
        plan * 1e3,
        single * 1e3,
        solids * 1e3
    );
    if !cfg!(debug_assertions) {
        assert!(best < 0.020, "200-wall storey took {:.1} ms", best * 1e3);
    }
}
