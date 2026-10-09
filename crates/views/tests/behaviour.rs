//! Behaviour of derived views on the fixture house: phases, underlays, north arrow, automatic dimensions,
//! view-owned annotation, flipped sections, the scene cache and titles.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod common;

use kanavos_drawing::{Drawing, Fill, Item, LinePattern, Rgba, Shape, Stroke};
use kanavos_geom::{Angle, Length, Point2, Point3};
use kanavos_model::{
    Dimension, Element, ElementKind, Id, LevelMark, Phase, TextAlign, TextNote, Underlay, View, ViewKind,
};
use kanavos_units::Locale;
use kanavos_views::{
    PlanOptions, SceneCache, SceneOptions, ViewError, auto_dimensions, element_scene_items, plan, scene,
    section_frame, view_drawing, view_title,
};

fn walls_of(doc: &kanavos_model::Document, storey: Id) -> Vec<Id> {
    doc.walls_on(storey).map(|(id, _)| id).collect()
}

fn with_phase(doc: &kanavos_model::Document, id: Id, phase: Phase) -> kanavos_model::Document {
    let mut e = doc.element(id).unwrap().clone();
    e.phase = phase;
    doc.with_element(e)
}

fn items_of(d: &Drawing, source: Id) -> Vec<&Item> {
    d.items.iter().filter(|i| i.source == Some(source.0)).collect()
}

#[test]
fn phases_change_fills_and_strokes() {
    let h = common::house();
    let walls = walls_of(&h.doc, h.ground);
    let (existing, demolished) = (walls[0], walls[5]);
    let doc = with_phase(&h.doc, existing, Phase::Existing);
    let doc = with_phase(&doc, demolished, Phase::Demolished);
    let d = plan(&doc, h.ground, &PlanOptions::default()).unwrap();
    common::save_png(&d, "plan-phases.png");
    // Existing: solid grey fill and grey outline.
    let ex = items_of(&d, existing);
    assert!(ex.iter().any(|i| matches!(i.fill, Some(Fill::Solid(c)) if c == Rgba::rgb(200, 200, 200))));
    assert!(
        ex.iter().filter(|i| i.layer == "wall.cut").all(|i| i.stroke.as_ref().unwrap().color != Rgba::BLACK)
    );
    // Demolished: white fill and a dashed outline.
    let de = items_of(&d, demolished);
    assert!(de.iter().any(|i| matches!(i.fill, Some(Fill::Solid(Rgba::WHITE)))));
    let outlines: Vec<&&Item> = de.iter().filter(|i| i.layer == "wall.cut").collect();
    assert!(!outlines.is_empty());
    assert!(outlines.iter().all(|i| i.stroke.as_ref().unwrap().pattern != LinePattern::Solid));
}

#[test]
fn underlays_are_grey_and_under_everything() {
    let h = common::house();
    let mut under = Drawing::model(1.0);
    under.push(Item::line(
        kanavos_drawing::pt(-5.0, -5.0),
        kanavos_drawing::pt(20.0, -5.0),
        Stroke::solid(0.35),
        "plot",
    ));
    let (doc, id) = h.doc.with_new_id();
    let doc = doc.with_element(Element::on_storey(
        id,
        h.ground,
        ElementKind::Underlay(Underlay {
            name: "Τοπογραφικό".into(),
            source: "survey.dxf".into(),
            drawing: under,
        }),
    ));
    let d = plan(&doc, h.ground, &PlanOptions::default()).unwrap();
    let first = d.items.first().unwrap();
    assert_eq!(first.layer, "underlay.plot");
    assert_ne!(first.stroke.as_ref().unwrap().color, Rgba::BLACK);
}

#[test]
fn north_arrow_follows_the_georeference() {
    let h = common::house();
    let d = plan(&h.doc, h.ground, &PlanOptions::default()).unwrap();
    // The arrow's letter sits beyond its tip: with a rotation of −12° (clockwise), north leans to the right.
    let letter = d.layer_items("north.arrow").find_map(|i| match &i.shape {
        Shape::Text(t) => Some(t.position),
        _ => None,
    });
    let circle = d.layer_items("north.arrow").find_map(|i| match &i.shape {
        Shape::Circle { centre, .. } => Some(*centre),
        _ => None,
    });
    let (l, c) = (letter.unwrap(), circle.unwrap());
    let ang = (l.y - c.y).atan2(l.x - c.x).to_degrees();
    assert!((ang - (90.0 - 12.0)).abs() < 1e-6, "angle {ang}");
}

#[test]
fn automatic_dimensions_cover_every_side() {
    let h = common::house();
    let dims = auto_dimensions(&h.doc, h.ground);
    let span = |d: &Dimension| {
        let u = kanavos_geom::Vec2::from_angle(d.direction);
        let ts: Vec<f64> = d.points.iter().map(|p| (*p - d.line_point).dot(u)).collect();
        ts.iter().cloned().fold(f64::MIN, f64::max) - ts.iter().cloned().fold(f64::MAX, f64::min)
    };
    let overall: Vec<f64> = dims.iter().filter(|d| d.points.len() == 2).map(span).collect();
    assert_eq!(overall.iter().filter(|s| (**s - 12.31).abs() < 1e-6).count(), 2, "{overall:?}");
    assert_eq!(overall.iter().filter(|s| (**s - 9.31).abs() < 1e-6).count(), 2, "{overall:?}");
    // The south façade chain includes both jambs of the 1 m entrance door (x = 7.7 and 8.7).
    let south = dims
        .iter()
        .filter(|d| d.points.len() > 2)
        .find(|d| d.line_point.y.as_m() < -0.2 && d.direction.as_deg().abs() < 1.0)
        .unwrap();
    for x in [7.7, 8.7] {
        assert!(south.points.iter().any(|p| (p.x.as_m() - x).abs() < 1e-6), "jamb {x} in {:?}", south.points);
    }
}

#[test]
fn section_draws_its_own_annotation_and_is_not_mirrored_when_flipped() {
    let h = common::house();
    let mut doc = h.doc.clone();
    for kind in [
        ElementKind::Dimension(Dimension {
            points: vec![Point2::new(2.0, 0.0), Point2::new(2.0, 3.2)],
            line_point: Point2::new(1.0, 0.0),
            direction: Angle::deg(90.0),
        }),
        ElementKind::Text(TextNote {
            position: Point2::new(4.0, 1.5),
            content: "Καθιστικό".into(),
            size_mm: 2.5,
            rotation: Angle::ZERO,
            align: TextAlign::Centre,
            bold: false,
        }),
        ElementKind::Level(LevelMark { position: Point2::new(1.0, 2.25), value: None, label: String::new() }),
    ] {
        let (d2, id) = doc.with_new_id();
        let mut e = Element::on_storey(id, h.ground, kind);
        e.storey = None;
        e.view = Some(h.section);
        doc = d2.with_element(e);
    }
    let d = view_drawing(&doc, h.section, Locale::El).unwrap();
    let texts: Vec<String> = d
        .items
        .iter()
        .filter_map(|i| match &i.shape {
            Shape::Text(t) => Some(t.content.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "3,20"), "dimension text");
    assert!(texts.iter().any(|t| t == "Καθιστικό"));
    assert!(texts.iter().any(|t| t == "+2,25"));

    // Flipping the view keeps the drawing readable: x still runs from 0 to the line's length.
    let v = doc.view(h.section).unwrap().clone();
    let ViewKind::Section { start, end, depth, marker, .. } = v.kind.clone() else { panic!() };
    let flipped =
        doc.with_view(View { kind: ViewKind::Section { start, end, depth, flip: true, marker }, ..v });
    let f = section_frame(&flipped, h.section).unwrap();
    assert!(f.origin.coincides(end, &Default::default()));
    assert!((f.look.x - f.along.perp().x).abs() < 1e-12 && (f.look.y - f.along.perp().y).abs() < 1e-12);
    let d = view_drawing(&flipped, h.section, Locale::El).unwrap();
    let b = d
        .layer_items("wall.cut")
        .flat_map(|i| i.shape.extent_points())
        .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.x), a.1.max(p.x)));
    assert!(b.0 >= -1e-9 && b.1 <= f.length + 1e-9, "{b:?}");
}

#[test]
fn scene_cache_reuses_unchanged_elements() {
    let h = common::house();
    let opts = SceneOptions::default();
    let mut cache = SceneCache::new();
    let first = cache.scene(&h.doc, &opts);
    assert_eq!(first, scene(&h.doc, &opts));
    let misses = cache.stats().misses;
    // Nothing changed: every element is a hit.
    let again = cache.scene(&h.doc, &opts);
    assert_eq!(again, first);
    assert_eq!(cache.stats().misses, misses);

    // Moving one wall recomputes that wall, its openings and its neighbours, nothing else.
    let wall = walls_of(&h.doc, h.ground)[3];
    let mut e = h.doc.element(wall).unwrap().clone();
    if let ElementKind::Wall(w) = &mut e.kind {
        w.height = Length::m(3.0);
    }
    let doc = h.doc.with_element(e);
    let before = cache.stats().misses;
    let changed = cache.scene(&doc, &opts);
    let recomputed = cache.stats().misses - before;
    assert!(recomputed > 1 && recomputed < 20, "recomputed {recomputed}");
    assert_eq!(changed, scene(&doc, &opts));
    assert_eq!(
        element_scene_items(&doc, wall),
        changed.items.iter().filter(|i| i.element == wall).cloned().collect::<Vec<_>>()
    );

    // Deleted elements leave the cache.
    let doc = doc.without_element(wall).unwrap();
    let _ = cache.scene(&doc, &opts);
    assert_eq!(cache.stats().entries, doc.elements().filter(|e| e.category().is_building_part()).count());
}

#[test]
fn storey_filter_and_section_box() {
    let h = common::house();
    let only_ground =
        SceneOptions { storeys: Some([h.ground].into_iter().collect()), ..SceneOptions::default() };
    let sc = scene(&h.doc, &only_ground);
    assert!(sc.items.iter().all(|i| i.storey == Some(h.ground)));
    let all = scene(&h.doc, &SceneOptions::default());
    let b = kanavos_geom::Bbox3 { min: Point3::new(-1.0, -2.0, -1.0), max: Point3::new(14.0, 4.5, 4.0) };
    let clipped = kanavos_views::clip_scene_to_box(&all, &b);
    let cb = clipped.bbox().unwrap();
    assert!(cb.max.y <= 4.5 + 1e-9 && cb.max.z <= 4.0 + 1e-9);
    assert!(clipped.items.len() < all.items.len());
}

#[test]
fn titles_and_wrong_kinds() {
    let h = common::house();
    assert_eq!(view_title(&h.doc, h.section, Locale::El).unwrap(), "Τομή Α-Α");
    assert_eq!(view_title(&h.doc, h.section, Locale::En).unwrap(), "Section Α-Α");
    assert_eq!(view_title(&h.doc, h.axo, Locale::El).unwrap(), "Αξονομετρικό");
    let table = h.doc.views().find(|v| matches!(v.kind, ViewKind::AreaTable)).unwrap().id;
    assert_eq!(view_drawing(&h.doc, table, Locale::El), Err(ViewError::WrongKind(table)));
    assert_eq!(view_drawing(&h.doc, Id(999_999), Locale::El), Err(ViewError::NotFound(Id(999_999))));
}

fn poly_area(d: &Drawing, layer: &str) -> f64 {
    d.layer_items(layer)
        .filter_map(|i| match &i.shape {
            Shape::Polygon { rings } => Some(
                rings
                    .iter()
                    .map(|r| {
                        let n = r.len();
                        (0..n).map(|k| r[k].x * r[(k + 1) % n].y - r[(k + 1) % n].x * r[k].y).sum::<f64>()
                            / 2.0
                    })
                    .sum::<f64>()
                    .abs(),
            ),
            _ => None,
        })
        .sum()
}

#[test]
fn a_rotated_house_draws_like_the_original() {
    let h = common::house();
    let t = kanavos_geom::Transform2::rotation_about(Point2::new(6.0, 4.5), Angle::deg(30.0));
    let mut doc = h.doc.clone();
    for e in h.doc.elements() {
        let mut c = e.clone();
        c.kind = e.kind.transformed(&t);
        doc = doc.with_element(c);
    }
    for v in h.doc.views() {
        let mut v = v.clone();
        if let ViewKind::Section { start, end, .. } | ViewKind::Elevation { start, end, .. } = &mut v.kind {
            *start = t.apply(*start);
            *end = t.apply(*end);
        }
        doc = doc.with_view(v);
    }
    let p0 = plan(&h.doc, h.ground, &PlanOptions::default()).unwrap();
    let p1 = plan(&doc, h.ground, &PlanOptions::default()).unwrap();
    common::save_png(&p1, "rotated-plan.png");
    let (a0, a1) = (poly_area(&p0, "wall.layer"), poly_area(&p1, "wall.layer"));
    assert!((a0 - a1).abs() < 1e-6 * a0, "{a0} vs {a1}");
    // Automatic dimensions follow the building's axes.
    for d in auto_dimensions(&doc, h.ground) {
        let off = (d.direction.as_deg() - 30.0).rem_euclid(90.0);
        assert!(off.min(90.0 - off) < 1e-6, "{}", d.direction.as_deg());
    }
    // The section of the rotated house cuts the same areas.
    let s0 = view_drawing(&h.doc, h.section, Locale::El).unwrap();
    let s1 = view_drawing(&doc, h.section, Locale::El).unwrap();
    common::save_png(&s1, "rotated-section.png");
    let (c0, c1) = (poly_area(&s0, "wall.layer"), poly_area(&s1, "wall.layer"));
    assert!((c0 - c1).abs() < 1e-3 * c0, "{c0} vs {c1}");
}

#[test]
fn empty_projects_draw_without_failing() {
    let doc = kanavos_model::starter();
    let ground = doc.storeys()[0].id;
    let d = plan(&doc, ground, &PlanOptions::default()).unwrap();
    assert!(d.items.iter().all(|i| i.layer.starts_with("north") || i.layer.contains("marker")));
    assert!(auto_dimensions(&doc, ground).is_empty());
    let (doc, sid) = doc.with_new_id();
    let doc = doc.with_view(View {
        id: sid,
        name: "Τομή".into(),
        kind: ViewKind::Section {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(10.0, 0.0),
            depth: Length::m(5.0),
            flip: false,
            marker: "Α".into(),
        },
        scale: 50,
        hidden_layers: Default::default(),
    });
    let s = view_drawing(&doc, sid, Locale::El).unwrap();
    assert!(s.items.is_empty());
    let axo = doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD { .. })).unwrap().id;
    assert!(view_drawing(&doc, axo, Locale::El).unwrap().items.is_empty());
    // A view line of zero length cannot be drawn.
    let (doc, zid) = doc.with_new_id();
    let doc = doc.with_view(View {
        id: zid,
        name: "Όψη".into(),
        kind: ViewKind::Elevation {
            start: Point2::new(1.0, 1.0),
            end: Point2::new(1.0, 1.0),
            depth: Length::ZERO,
            flip: false,
            marker: "Γ".into(),
        },
        scale: 50,
        hidden_layers: Default::default(),
    });
    assert_eq!(view_drawing(&doc, zid, Locale::El), Err(ViewError::WrongKind(zid)));
}
