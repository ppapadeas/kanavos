//! Performance of view derivation on a sample-size model. Budgets are checked in release builds only:
//! `cargo test --release -p kanavos-views --test perf -- --nocapture` prints the measurements.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::time::{Duration, Instant};

use kanavos_geom::{Length, Point2, Point3};
use kanavos_model::{View, ViewKind};
use kanavos_solid::Mesh;
use kanavos_units::Locale;
use kanavos_views::{
    PlanOptions, Scene, SceneCache, SceneOptions, SectionOptions, plan, scene, section_with,
};

/// Every triangle split into four through its edge midpoints: the same surfaces with four times the triangles,
/// standing in for the finer meshes of detailed objects and curved elements.
fn subdivide(m: &Mesh) -> Mesh {
    let mut out = Mesh::new();
    for [a, b, c] in m.triangle_points() {
        let mid = |p: Point3, q: Point3| p.lerp(q, 0.5);
        let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
        for [p, q, r] in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
            let i = out.push_vertex(p);
            let j = out.push_vertex(q);
            let k = out.push_vertex(r);
            out.push_triangle(i, j, k);
        }
    }
    out
}

fn best_of<T>(n: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    // Debug builds only check that everything runs; timings matter in release builds.
    let n = if cfg!(debug_assertions) { 1 } else { n };
    let mut best = Duration::MAX;
    let mut out = None;
    for _ in 0..n {
        let t = Instant::now();
        let r = f();
        best = best.min(t.elapsed());
        out = Some(r);
    }
    (best, out.expect("at least one run"))
}

#[test]
fn sample_size_model_meets_budgets() {
    let h = common::houses(6);
    let doc = h.doc.clone();
    let (doc, sid) = doc.with_new_id();
    let doc = doc.with_view(View {
        id: sid,
        name: "Τομή".into(),
        kind: ViewKind::Section {
            start: Point2::new(-2.5, 2.0),
            end: Point2::new(14.0 * 6.0, 2.0),
            depth: Length::m(12.0),
            flip: false,
            marker: "Α".into(),
        },
        scale: 50,
        hidden_layers: Default::default(),
    });
    let (doc, eid) = doc.with_new_id();
    let doc = doc.with_view(View {
        id: eid,
        name: "Όψη".into(),
        kind: ViewKind::Elevation {
            start: Point2::new(-2.5, -4.0),
            end: Point2::new(14.0 * 6.0, -4.0),
            depth: Length::m(20.0),
            flip: false,
            marker: "Β".into(),
        },
        scale: 50,
        hidden_layers: Default::default(),
    });
    let ground_elements = doc.elements_on(h.ground).count();

    let (t_scene, sc) = best_of(3, || scene(&doc, &SceneOptions::default()));
    let mut cache = SceneCache::new();
    let _ = cache.scene(&doc, &SceneOptions::default());
    let (t_cached, _) = best_of(3, || cache.scene(&doc, &SceneOptions::default()));
    let dense = Scene {
        items: sc
            .items
            .iter()
            .map(|i| kanavos_views::SceneItem { mesh: subdivide(&i.mesh), ..i.clone() })
            .collect(),
    };
    let opts = SectionOptions { locale: Locale::El, ..SectionOptions::default() };
    let (t_section, sd) = best_of(3, || section_with(&doc, sid, &opts, Some(&sc)).unwrap());
    let (t_section_full, _) = best_of(3, || {
        let sc = cache.scene(&doc, &SceneOptions::default());
        section_with(&doc, sid, &opts, Some(&sc)).unwrap()
    });
    let (t_dense, dd) = best_of(3, || section_with(&doc, sid, &opts, Some(&dense)).unwrap());
    let (t_elev, ed) = best_of(3, || section_with(&doc, eid, &opts, Some(&sc)).unwrap());
    let (t_elev_dense, _) = best_of(3, || section_with(&doc, eid, &opts, Some(&dense)).unwrap());
    let (t_plan, pd) = best_of(3, || plan(&doc, h.ground, &PlanOptions::default()).unwrap());

    eprintln!(
        "model: {} elements, {} scene items, {} triangles; ground storey: {} elements",
        doc.element_count(),
        sc.items.len(),
        sc.triangle_count(),
        ground_elements
    );
    eprintln!("scene (cold): {t_scene:?}; scene from cache: {t_cached:?}");
    eprintln!("section: {t_section:?} ({} items); with cached scene: {t_section_full:?}", sd.items.len());
    eprintln!(
        "section of the dense scene ({} triangles): {t_dense:?} ({} items)",
        dense.triangle_count(),
        dd.items.len()
    );
    eprintln!("elevation: {t_elev:?} ({} items); dense: {t_elev_dense:?}", ed.items.len());
    eprintln!("ground plan: {t_plan:?} ({} items)", pd.items.len());
    common::save_png(&sd, "perf-section.png");
    common::save_png(&dd, "perf-section-dense.png");

    assert!(doc.element_count() >= 300);
    assert!(dense.triangle_count() >= 80_000);
    assert!(ground_elements >= 190);
    if !cfg!(debug_assertions) {
        assert!(t_section_full < Duration::from_millis(300), "section {t_section_full:?}");
        assert!(t_dense < Duration::from_millis(300), "dense section {t_dense:?}");
        assert!(t_plan < Duration::from_millis(50), "plan {t_plan:?}");
    }
}
