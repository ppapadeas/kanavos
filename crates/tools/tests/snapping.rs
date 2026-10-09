//! Snapping and inference through the tools, and the snapping budget on a 2,000-element storey
//! (`specs/tools/snapping.md`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use common::*;
use kanavos_geom::{Angle, Point2, Segment2};
use kanavos_model::{
    Column, Document, Element, ElementKind, Id, Profile, ReferenceLine, Structure, Wall, starter,
};
use kanavos_tools::{SnapHint, SnapQuery, SnapSettings, Snapper, ToolId, hit_test};
use kanavos_units::{Length, Locale};

#[test]
fn extension_of_a_recently_hovered_edge() {
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.add_wall((0.0, 0.0), (5.0, 0.0));
    // Hover the wall's left face, then start a wall elsewhere and move along the face's extension.
    h.move_to(2.0, 0.157);
    h.click(9.0, 3.0);
    let out = h.move_to(8.0, 0.16);
    let s = out.snap.unwrap();
    assert!(matches!(s.hint, SnapHint::Extension), "{s:?}");
    assert!(s.label.contains("extension of wall"), "{}", s.label);
    assert!((s.point.y.as_m() - 0.155).abs() < 1e-9);
    assert!(!s.guides.is_empty());
}

#[test]
fn parallel_and_perpendicular_to_a_recently_hovered_edge() {
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.add_wall((0.0, 0.0), (6.0, 3.0));
    h.move_to(2.0, 1.0); // On the centreline.
    h.click(10.0, 0.0);
    // Direction (2, 1) from (10, 0): parallel to the wall.
    let out = h.move_to(14.0, 2.03);
    let s = out.snap.unwrap();
    assert_eq!(s.hint, SnapHint::Parallel, "{s:?}");
    assert!(s.label.starts_with("parallel to wall"));
    // Direction (-1, 2): perpendicular.
    let out = h.move_to(9.0 - 0.02, 2.0);
    let s = out.snap.unwrap();
    assert_eq!(s.hint, SnapHint::Perpendicular, "{s:?}");
}

#[test]
fn shift_locks_the_direction_and_alt_disables_snapping() {
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.add_wall((0.0, 5.0), (8.0, 5.0));
    h.click(0.0, 0.0);
    h.move_to(3.0, 3.0);
    let out = h.move_mods(4.0, 5.5, SHIFT);
    let s = out.snap.unwrap();
    assert!(matches!(s.hint, SnapHint::Locked), "{s:?}");
    assert!((s.point.x.as_m() - s.point.y.as_m()).abs() < 1e-9, "stays on the 45° line");
    let out = h.move_mods(4.0, 4.97, NONE);
    assert_ne!(out.snap.unwrap().hint, SnapHint::Locked);
    let alt = kanavos_tools::Modifiers { alt: true, ..NONE };
    let out = h.move_mods(4.004, 4.998, alt);
    let s = out.snap.unwrap();
    assert_eq!(s.hint, SnapHint::Free);
    assert!(close(s.point, p(4.004, 4.998)));
}

#[test]
fn tracked_points_are_reported_for_drawing() {
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.add_wall((0.0, 0.0), (5.0, 0.0));
    h.move_to(5.01, 0.0);
    let out = h.move_to(3.0, 3.0);
    assert!(out.tracked.iter().any(|q| close(*q, p(5.0, 0.0))));
}

#[test]
fn labels_follow_the_language() {
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.locale = Locale::El;
    let id = h.add_wall((0.0, 0.0), (5.0, 0.0));
    let out = h.move_to(2.51, 0.0);
    let label = out.snap.unwrap().label;
    assert!(label == format!("μέσο τοίχου {}", id.0) || label.starts_with("άξονας τοίχου"), "{label}");
}

/// A storey with 2,000 elements: a 35 × 20 grid of 4 m bays with walls on every bay edge and a column at every
/// node.
fn big_storey() -> (Document, Id, usize) {
    let mut doc = starter();
    let storey = doc.storeys()[0].id;
    let build_up = doc.defaults().exterior_wall.unwrap();
    let material = doc.defaults().structural_material.unwrap();
    let add = |doc: &mut Document, kind: ElementKind| {
        let (d, id) = doc.with_new_id();
        *doc = d.with_element(Element::on_storey(id, storey, kind));
    };
    let wall = |a: (f64, f64), b: (f64, f64)| {
        ElementKind::Wall(Wall {
            start: Point2::new(a.0, a.1),
            end: Point2::new(b.0, b.1),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Layered { build_up },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        })
    };
    let (nx, ny, bay) = (35, 20, 4.0);
    for j in 0..=ny {
        for i in 0..nx {
            let y = f64::from(j) * bay;
            add(&mut doc, wall((f64::from(i) * bay, y), (f64::from(i + 1) * bay, y)));
        }
    }
    for i in 0..=nx {
        for j in 0..ny {
            let x = f64::from(i) * bay;
            add(&mut doc, wall((x, f64::from(j) * bay), (x, f64::from(j + 1) * bay)));
        }
    }
    let mut count = doc.elements_on(storey).count();
    'outer: for j in 0..=ny {
        for i in 0..=nx {
            if count >= 2000 {
                break 'outer;
            }
            add(
                &mut doc,
                ElementKind::Column(Column {
                    position: Point2::new(f64::from(i) * bay + 2.0, f64::from(j) * bay + 2.0),
                    rotation: Angle::ZERO,
                    profile: Profile::Rectangle { width: Length::cm(30.0), depth: Length::cm(30.0) },
                    material,
                    height: Length::m(3.0),
                    base_offset: Length::ZERO,
                }),
            );
            count += 1;
        }
    }
    (doc, storey, count)
}

/// Deterministic pseudo-random positions over the storey.
fn positions(n: usize) -> Vec<Point2> {
    let mut s: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    (0..n).map(|_| Point2::new(next() * 140.0, next() * 80.0)).collect()
}

#[test]
fn snapping_on_a_2000_element_storey_stays_within_budget() {
    let (doc, storey, count) = big_storey();
    assert!(count >= 2000, "{count} elements");
    let t0 = Instant::now();
    let snapper = Snapper::new(&doc, storey);
    let build = t0.elapsed();
    let settings = SnapSettings::default();
    let tracked = [Point2::new(10.0, 10.0), Point2::new(40.0, 22.0), Point2::new(70.0, 50.0)];
    let edges = [(Segment2::new(Point2::new(0.0, 0.155), Point2::new(4.0, 0.155)), Id(1))];
    let exclude = BTreeSet::new();
    let pts = positions(2000);
    // Pixel sizes from close-up to a whole-storey view.
    let mut times: Vec<Duration> = Vec::new();
    for pixel in [0.002, 0.01, 0.05] {
        for (i, raw) in pts.iter().enumerate() {
            let from = (i % 2 == 0).then(|| Point2::new(raw.x.as_m() - 1.3, raw.y.as_m() - 0.7));
            let q = SnapQuery {
                from,
                settings: &settings,
                pixel,
                locale: Locale::El,
                tracked: &tracked,
                edges: &edges,
                lock: None,
                exclude: &exclude,
            };
            let t = Instant::now();
            let r = snapper.snap_query(*raw, &q);
            times.push(t.elapsed());
            std::hint::black_box(r);
        }
    }
    let mean = times.iter().sum::<Duration>() / times.len() as u32;
    times.sort_unstable();
    let p99 = times.get(times.len() * 99 / 100).copied().unwrap_or_default();
    let worst = times.last().copied().unwrap_or_default();
    // Hit testing for hover highlights uses the same index.
    let t = Instant::now();
    for raw in pts.iter().take(500) {
        std::hint::black_box(hit_test(&doc, storey, *raw, 0.06));
    }
    let hit_mean = t.elapsed() / 500;
    println!(
        "snap index over {count} elements: build {build:?}; query mean {mean:?}, p99 {p99:?}, max {worst:?}; {}",
        format_args!("hit test mean {hit_mean:?}")
    );
    // The maximum is reported but not asserted: a single query can be descheduled by the OS.
    assert!(mean < Duration::from_millis(2), "mean snap query {mean:?}");
    assert!(p99 < Duration::from_millis(2), "99th percentile snap query {p99:?}");
    assert!(hit_mean < Duration::from_millis(2), "mean hit test {hit_mean:?}");
}

#[test]
fn a_full_pointer_move_of_the_wall_tool_on_a_large_storey() {
    let (doc, storey, _) = big_storey();
    let mut h = Harness::new(ToolId::Wall).with_snapping();
    h.doc = doc;
    h.storey = storey;
    h.click(2.0, 2.0);
    let pts = positions(300);
    let t = Instant::now();
    for q in &pts {
        h.move_to(q.x.as_m(), q.y.as_m());
    }
    let mean = t.elapsed() / pts.len() as u32;
    println!("wall tool pointer move with preview on a 2,000-element storey: mean {mean:?}");
    assert!(mean < Duration::from_millis(16), "{mean:?}");
}

#[test]
fn moving_a_large_selection_previews_quickly() {
    let (doc, storey, count) = big_storey();
    let mut h = Harness::new(ToolId::Select).with_snapping();
    h.doc = doc;
    h.storey = storey;
    h.selection = h.doc.elements_on(storey).map(|e| e.id).collect();
    h.move_to(2.0, 0.05);
    h.send(kanavos_tools::ToolEvent::Press {
        at: p(2.0, 0.05),
        button: kanavos_tools::MouseButton::Primary,
        mods: NONE,
    });
    let t = Instant::now();
    let n = 50;
    for i in 1..=n {
        let out = h.move_to(2.0 + f64::from(i) * 0.1, 0.05 + f64::from(i) * 0.05);
        assert!(!out.preview.items.is_empty());
    }
    let mean = t.elapsed() / n;
    println!("moving {count} selected elements: mean pointer move {mean:?}");
    assert!(mean < Duration::from_millis(50), "{mean:?}");
    let out = h.send(kanavos_tools::ToolEvent::Release {
        at: p(7.0, 2.55),
        button: kanavos_tools::MouseButton::Primary,
        mods: NONE,
    });
    assert_eq!(out.commands.len(), 1);
}
