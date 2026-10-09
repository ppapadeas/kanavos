//! A small sample house built from engine commands, for visual checks of the canvas and the 3D view, for
//! performance measurements and for tests. It is never added to a document unless asked for (see `shot`).

use std::collections::BTreeMap;

use kanavos_engine::commands::CreateElement;
use kanavos_engine::{AnyCommand, Command, CommandError};
use kanavos_geom::{Angle, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Dimension, Document, ElementKind, Id, Object, Opening, OpeningKind, ReferenceLine, Roof, RoofShape, Slab,
    Space, SpaceCategory, Structure, Wall,
};
use kanavos_units::Length;

/// Commands that build a single-storey house with its origin at `origin` on `storey`: four exterior walls, two
/// interior walls, a front door, two interior doors, four windows, a ground slab, three spaces, three pieces of
/// furniture, a dimension along the front and, on the storey above (if any), a ceiling slab and a hip roof.
///
/// Element ids are found by applying the commands to a scratch copy of `doc`, so the list must be run on the same
/// document, in order, as one batch.
pub fn house_commands(
    doc: &Document,
    storey: Id,
    origin: (f64, f64),
) -> Result<Vec<AnyCommand>, CommandError> {
    let mut scratch = doc.clone();
    let mut cmds: Vec<AnyCommand> = Vec::new();
    let mut run = |scratch: &mut Document, c: AnyCommand| -> Result<Option<Id>, CommandError> {
        let (next, out) = c.apply(scratch)?;
        *scratch = next;
        cmds.push(c);
        Ok(out.created.first().copied())
    };
    let (ox, oy) = origin;
    let p = |x: f64, y: f64| Point2::new(ox + x, oy + y);
    let defaults = doc.defaults().clone();
    let exterior = defaults.exterior_wall.map(|b| Structure::Layered { build_up: b }).ok_or(
        CommandError::InvalidArgument { field: "defaults", reason: "no exterior wall build-up".into() },
    )?;
    let interior = defaults
        .interior_wall
        .map(|b| Structure::Layered { build_up: b })
        .unwrap_or_else(|| exterior.clone());
    let height = doc.storey(storey).map(|s| s.height).ok_or(CommandError::NotFound(storey))?;
    let wall = |a: Point2, b: Point2, s: &Structure| {
        CreateElement::on(
            storey,
            ElementKind::Wall(Wall {
                start: a,
                end: b,
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: s.clone(),
                height,
                base_offset: Length::ZERO,
            }),
        )
    };
    let (w, d) = (10.0, 8.0);
    let west = run(&mut scratch, wall(p(0.0, 0.0), p(0.0, d), &exterior).into())?;
    let north = run(&mut scratch, wall(p(0.0, d), p(w, d), &exterior).into())?;
    let east = run(&mut scratch, wall(p(w, d), p(w, 0.0), &exterior).into())?;
    let south = run(&mut scratch, wall(p(w, 0.0), p(0.0, 0.0), &exterior).into())?;
    let mid = run(&mut scratch, wall(p(6.0, 0.0), p(6.0, d), &interior).into())?;
    let cross = run(&mut scratch, wall(p(6.0, 4.5), p(w, 4.5), &interior).into())?;

    let opening = |host: Option<Id>,
                   kind: OpeningKind,
                   style: &str,
                   offset: f64,
                   width: f64,
                   h: f64,
                   sill: f64,
                   inward: bool| {
        host.map(|host| {
            CreateElement::on(
                storey,
                ElementKind::Opening(Opening {
                    host,
                    kind,
                    style: style.into(),
                    offset: Length::m(offset),
                    width: Length::m(width),
                    height: Length::m(h),
                    sill: Length::m(sill),
                    flip_side: inward,
                    flip_hand: false,
                    params: BTreeMap::new(),
                }),
            )
        })
    };
    let openings = [
        opening(south, OpeningKind::Door, "door.single_swing", 7.0, 1.0, 2.2, 0.0, true),
        opening(south, OpeningKind::Window, "window.casement_2", 2.0, 1.4, 1.4, 0.9, false),
        opening(north, OpeningKind::Window, "window.sliding", 3.5, 2.0, 1.4, 0.9, false),
        opening(west, OpeningKind::Window, "window.casement_2", 4.0, 1.2, 1.4, 0.9, false),
        opening(east, OpeningKind::Window, "window.casement_1", 1.8, 0.8, 1.0, 1.3, false),
        opening(mid, OpeningKind::Door, "door.single_swing", 2.2, 0.9, 2.1, 0.0, false),
        opening(cross, OpeningKind::Door, "door.single_swing", 1.0, 0.8, 2.1, 0.0, false),
    ];
    for o in openings.into_iter().flatten() {
        run(&mut scratch, o.into())?;
    }

    let t = scratch
        .defaults()
        .exterior_wall
        .and_then(|b| scratch.build_up(b))
        .map(|b| b.thickness().as_m() / 2.0)
        .unwrap_or(0.15);
    let rect =
        |x0: f64, y0: f64, x1: f64, y1: f64| Polygon2::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]);
    if let Some(b) = defaults.ground_slab {
        run(
            &mut scratch,
            CreateElement::on(
                storey,
                ElementKind::Slab(Slab {
                    outline: PolygonWithHoles::from(rect(-t, -t, w + t, d + t)),
                    structure: Structure::Layered { build_up: b },
                    top_offset: Length::ZERO,
                }),
            )
            .into(),
        )?;
    }
    // Spaces bounded by the inner faces of the walls.
    let ti = scratch
        .defaults()
        .interior_wall
        .and_then(|b| scratch.build_up(b))
        .map(|b| b.thickness().as_m() / 2.0)
        .unwrap_or(0.07);
    for ((x0, y0, x1, y1), (sx, sy), name, number) in [
        ((t, t, 6.0 - ti, d - t), (3.0, 4.6), "Καθιστικό", "0.01"),
        ((6.0 + ti, t, w - t, 4.5 - ti), (8.0, 2.6), "Υπνοδωμάτιο", "0.02"),
        ((6.0 + ti, 4.5 + ti, w - t, d - t), (8.0, 6.3), "Λουτρό", "0.03"),
    ] {
        run(
            &mut scratch,
            CreateElement::on(
                storey,
                ElementKind::Space(Space {
                    boundary: PolygonWithHoles::from(rect(x0, y0, x1, y1)),
                    name: name.into(),
                    number: number.into(),
                    category: SpaceCategory::MainUse,
                    height: None,
                    stamp: Some(p(sx, sy)),
                    finish: String::new(),
                }),
            )
            .into(),
        )?;
    }
    for (def, x, y, deg) in [
        ("furniture.sofa", 2.6, 7.2, 180.0),
        ("furniture.table", 3.0, 3.2, 0.0),
        ("furniture.bed_double", 8.6, 2.0, 90.0),
    ] {
        run(
            &mut scratch,
            CreateElement::on(
                storey,
                ElementKind::Object(Object {
                    definition: def.into(),
                    position: p(x, y),
                    rotation: Angle::deg(deg),
                    mirrored: false,
                    elevation: Length::ZERO,
                    params: BTreeMap::new(),
                }),
            )
            .into(),
        )?;
    }
    run(
        &mut scratch,
        CreateElement::on(
            storey,
            ElementKind::Dimension(Dimension {
                points: vec![p(-t, -t), p(3.0 - 0.5, -t), p(3.0 + 0.5, -t), p(6.0, -t), p(w + t, -t)],
                line_point: p(0.0, -1.6),
                direction: Angle::deg(0.0),
            }),
        )
        .into(),
    )?;
    if let Some(above) = doc.storey_above(storey).map(|s| s.id) {
        if let Some(b) = defaults.floor_slab {
            run(
                &mut scratch,
                CreateElement::on(
                    above,
                    ElementKind::Slab(Slab {
                        outline: PolygonWithHoles::from(rect(-t, -t, w + t, d + t)),
                        structure: Structure::Layered { build_up: b },
                        top_offset: Length::ZERO,
                    }),
                )
                .into(),
            )?;
        }
        if let Some(b) = defaults.pitched_roof {
            let o = 0.6;
            run(
                &mut scratch,
                CreateElement::on(
                    above,
                    ElementKind::Roof(Roof {
                        outline: rect(-t - o, -t - o, w + t + o, d + t + o),
                        base: Length::m(0.1),
                        structure: Structure::Layered { build_up: b },
                        shape: RoofShape::Pitched { slopes: vec![Some(Angle::deg(25.0)); 4] },
                    }),
                )
                .into(),
            )?;
        }
    }
    Ok(cmds)
}

/// Commands for `n` houses in rows of three, spaced so that they do not touch (for performance checks).
pub fn houses_commands(doc: &Document, storey: Id, n: usize) -> Result<Vec<AnyCommand>, CommandError> {
    let mut scratch = doc.clone();
    let mut all = Vec::new();
    for i in 0..n {
        let origin = ((i % 3) as f64 * 15.0, (i / 3) as f64 * 13.0);
        let cmds = house_commands(&scratch, storey, origin)?;
        for c in &cmds {
            scratch = c.apply(&scratch)?.0;
        }
        all.extend(cmds);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_house_builds_on_the_starter_document() {
        let doc = kanavos_model::starter();
        let storey = doc.storeys()[0].id;
        let cmds = house_commands(&doc, storey, (0.0, 0.0)).unwrap();
        let mut d = doc.clone();
        for c in &cmds {
            d = c.apply(&d).unwrap().0;
        }
        assert!(d.walls_on(storey).count() == 6);
        assert!(d.elements_on(storey).count() >= 20, "{}", d.elements_on(storey).count());
    }
}
