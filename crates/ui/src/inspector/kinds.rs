//! Inspector sections for every element type, grouped by what people change most (docs/ux.md §2).

use egui::Ui;
use kanavos_drawing::HatchPattern;
use kanavos_engine::AnyCommand;
use kanavos_geom::Point2;
use kanavos_model::{
    Beam, BuildUpUsage, Column, Dimension, Document, DraftShape, Drafting, ElementKind, GridLine, Id,
    LevelMark, Object, Opening, OpeningKind, Parapet, Profile, Railing, ReferenceLine, Roof, RoofShape, Slab,
    Space, Stair, Structure, TextAlign, TextNote, Underlay, Wall,
};
use kanavos_objects::{ObjectCategory, ParamKind};
use kanavos_units::{Angle, AreaDisplay, Length, LengthDisplay, Locale};

use super::form::Form;
use crate::names::{self, tr, trf};
use crate::widgets;

/// Wall or slab structure as a drop-down choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StructureChoice {
    BuildUp(Id),
    Basic,
}

fn structure_choice(s: &Structure) -> StructureChoice {
    match s {
        Structure::Layered { build_up } => StructureChoice::BuildUp(*build_up),
        Structure::Basic { .. } => StructureChoice::Basic,
    }
}

/// Build-ups for a usage (and "any"), labelled with their thickness, plus the single-material choice.
fn structure_options(doc: &Document, usage: BuildUpUsage, locale: Locale) -> Vec<(StructureChoice, String)> {
    let mut v: Vec<(StructureChoice, String)> = doc
        .build_ups()
        .filter(|b| b.usage == usage || b.usage == BuildUpUsage::Any)
        .map(|b| {
            let cm = widgets::format_number(b.thickness().as_cm(), 1, locale);
            (StructureChoice::BuildUp(b.id), format!("{} · {cm} cm", b.name))
        })
        .collect();
    v.push((StructureChoice::Basic, tr(locale, "ui.insp.basic_material")));
    v
}

/// Material options.
pub fn material_options(doc: &Document) -> Vec<(Id, String)> {
    doc.materials().map(|m| (m.id, m.name.clone())).collect()
}

/// Structure rows: build-up or single material with thickness, total thickness and U-value.
fn structure_rows<E: Clone>(
    f: &mut Form<E>,
    ui: &mut Ui,
    doc: &Document,
    usage: BuildUpUsage,
    get: impl Fn(&E) -> Structure + Copy,
    set: impl Fn(&mut E, Structure) + Copy,
) {
    let locale = f.locale;
    let options = structure_options(doc, usage, locale);
    let fallback_material = doc.materials().next().map(|m| m.id).unwrap_or_default();
    f.choice(
        ui,
        &tr(locale, "ui.insp.structure"),
        &options,
        |e| structure_choice(&get(e)),
        |e, c| {
            let current = get(e);
            let next = match c {
                StructureChoice::BuildUp(build_up) => Structure::Layered { build_up },
                StructureChoice::Basic => Structure::Basic {
                    material: doc.structure_core_material(&current).unwrap_or(fallback_material),
                    thickness: doc.structure_thickness(&current).unwrap_or(Length::cm(25.0)),
                },
            };
            set(e, next);
        },
    );
    if f.shared(|e| matches!(get(e), Structure::Basic { .. })) == Some(true) {
        let mats = material_options(doc);
        f.choice(
            ui,
            &tr(locale, "ui.insp.material"),
            &mats,
            |e| match get(e) {
                Structure::Basic { material, .. } => material,
                Structure::Layered { .. } => Id(0),
            },
            |e, m| {
                if let Structure::Basic { thickness, .. } = get(e) {
                    set(e, Structure::Basic { material: m, thickness });
                }
            },
        );
        f.length(
            ui,
            &tr(locale, "ui.insp.thickness"),
            |e| doc.structure_thickness(&get(e)).unwrap_or(Length::ZERO),
            |e, t| {
                if let Structure::Basic { material, .. } = get(e) {
                    set(e, Structure::Basic { material, thickness: t });
                }
            },
        );
    } else {
        let thickness = f.shared_length(|e| doc.structure_thickness(&get(e)).unwrap_or(Length::ZERO));
        let text =
            thickness.map(|t| LengthDisplay(t, locale).to_string()).unwrap_or_else(|| widgets::MIXED.into());
        f.read(ui, &tr(locale, "ui.insp.thickness"), &text);
        if let Some(Structure::Layered { build_up }) = f.shared(get)
            && let Some(b) = doc.build_up(build_up)
        {
            let u = b.u_value(|m| doc.material(m).and_then(|m| m.conductivity));
            let text = u
                .map(|u| format!("{} W/m²K", widgets::format_number(u, 3, locale)))
                .unwrap_or_else(|| tr(locale, "ui.insp.u_unknown"));
            f.read(ui, &tr(locale, "ui.insp.u_value"), &text);
        }
    }
}

fn point_rows<E: Clone>(
    f: &mut Form<E>,
    ui: &mut Ui,
    label_x: &str,
    label_y: &str,
    get: impl Fn(&E) -> Point2 + Copy,
    set: impl Fn(&mut E, Point2) + Copy,
) {
    f.length(ui, label_x, |e| get(e).x, |e, x| set(e, Point2::from_lengths(x, get(e).y)));
    f.length(ui, label_y, |e| get(e).y, |e, y| set(e, Point2::from_lengths(get(e).x, y)));
}

/// Shows the section for a group of walls.
pub fn walls(ui: &mut Ui, doc: &Document, items: Vec<(Id, Wall)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Wall, locale, "wall");
    widgets::section(ui, "insp-wall-structure", &tr(locale, "ui.insp.group.structure"), true, |ui| {
        widgets::grid(ui, "wall-structure", |ui| {
            structure_rows(
                &mut f,
                ui,
                doc,
                BuildUpUsage::Wall,
                |w| w.structure.clone(),
                |w, s| w.structure = s,
            );
            let refs: Vec<(ReferenceLine, String)> =
                [ReferenceLine::Left, ReferenceLine::Centre, ReferenceLine::Right]
                    .into_iter()
                    .map(|r| (r, names::reference(locale, r)))
                    .collect();
            f.choice(ui, &tr(locale, "ui.insp.reference"), &refs, |w| w.reference, |w, r| w.reference = r);
        });
    });
    widgets::section(ui, "insp-wall-geometry", &tr(locale, "ui.insp.group.geometry"), true, |ui| {
        widgets::grid(ui, "wall-geometry", |ui| {
            f.length(ui, &tr(locale, "ui.insp.height"), |w| w.height, |w, h| w.height = h);
            f.length(ui, &tr(locale, "ui.insp.base_offset"), |w| w.base_offset, |w, b| w.base_offset = b);
            let straight = f.shared(|w| w.bulge.abs() < 1e-12) == Some(true);
            if straight {
                f.length(ui, &tr(locale, "ui.insp.length"), Wall::length, |w, len| {
                    if let Some(d) = w.direction() {
                        w.end = w.start + d * len.as_m();
                    }
                });
            } else {
                let len = f.shared_length(Wall::length).map(|l| LengthDisplay(l, locale).to_string());
                f.read(ui, &tr(locale, "ui.insp.length"), &len.unwrap_or_else(|| widgets::MIXED.into()));
            }
            if f.single().is_some() {
                point_rows(
                    &mut f,
                    ui,
                    &tr(locale, "ui.insp.start_x"),
                    &tr(locale, "ui.insp.start_y"),
                    |w| w.start,
                    |w, p| w.start = p,
                );
                point_rows(
                    &mut f,
                    ui,
                    &tr(locale, "ui.insp.end_x"),
                    &tr(locale, "ui.insp.end_y"),
                    |w| w.end,
                    |w, p| w.end = p,
                );
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of doors, windows or voids.
pub fn openings(ui: &mut Ui, doc: &Document, items: Vec<(Id, Opening)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Opening, locale, "opening");
    let kind = f.shared(|o| o.kind);
    widgets::section(ui, "insp-opening-type", &tr(locale, "ui.insp.group.type"), true, |ui| {
        widgets::grid(ui, "opening-type", |ui| {
            if let Some(kind) = kind {
                let category = match kind {
                    OpeningKind::Door => ObjectCategory::Door,
                    OpeningKind::Window => ObjectCategory::Window,
                    OpeningKind::Void => ObjectCategory::Void,
                };
                let styles: Vec<(String, String)> = kanavos_objects::of_category(category)
                    .map(|d| (d.id.to_owned(), names::object(locale, d)))
                    .collect();
                f.choice(ui, &tr(locale, "ui.insp.style"), &styles, |o| o.style.clone(), |o, s| o.style = s);
            } else {
                f.read(ui, &tr(locale, "ui.insp.style"), widgets::MIXED);
            }
            f.length(ui, &tr(locale, "ui.insp.width"), |o| o.width, |o, v| o.width = v);
            f.length(ui, &tr(locale, "ui.insp.height"), |o| o.height, |o, v| o.height = v);
            f.length(ui, &tr(locale, "ui.insp.sill"), |o| o.sill, |o, v| o.sill = v);
            let head = f.shared_length(Opening::head).map(|h| LengthDisplay(h, locale).to_string());
            f.read(ui, &tr(locale, "ui.insp.head"), &head.unwrap_or_else(|| widgets::MIXED.into()));
        });
    });
    widgets::section(ui, "insp-opening-placement", &tr(locale, "ui.insp.group.placement"), true, |ui| {
        widgets::grid(ui, "opening-placement", |ui| {
            f.length(ui, &tr(locale, "ui.insp.position"), |o| o.offset, |o, v| o.offset = v);
            f.check(ui, &tr(locale, "ui.insp.flip_side"), |o| o.flip_side, |o, v| o.flip_side = v);
            f.check(ui, &tr(locale, "ui.insp.flip_hand"), |o| o.flip_hand, |o, v| o.flip_hand = v);
            if let Some(o) = f.single() {
                f.read(
                    ui,
                    &tr(locale, "ui.insp.host"),
                    &format!("{} {}", names::category(locale, kanavos_model::Category::Wall), o.host),
                );
            }
        });
    });
    let style = f.shared(|o| o.style.clone());
    if let Some(def) = style.as_deref().and_then(kanavos_objects::find)
        && !def.params.is_empty()
    {
        widgets::section(ui, "insp-opening-params", &tr(locale, "ui.insp.group.parameters"), false, |ui| {
            widgets::grid(ui, "opening-params", |ui| {
                for p in &def.params {
                    let label = names::object_param(locale, p.label, p.key);
                    let key = p.key;
                    let get = move |o: &Opening| kanavos_objects::param(Some(def), &o.params, key, p.default);
                    let set = move |o: &mut Opening, v: f64| {
                        o.params.insert(key.to_owned(), v.clamp(p.min, p.max));
                    };
                    param_row(&mut f, ui, &label, p.kind, get, set);
                }
            });
        });
    }
    f.finish()
}

fn param_row<E: Clone>(
    f: &mut Form<E>,
    ui: &mut Ui,
    label: &str,
    kind: ParamKind,
    get: impl Fn(&E) -> f64 + Copy,
    set: impl Fn(&mut E, f64) + Copy,
) {
    match kind {
        ParamKind::Length => f.length(ui, label, |e| Length::m(get(e)), |e, v| set(e, v.as_m())),
        ParamKind::Count => f.number(ui, label, 0, "", get, |e, v| set(e, v.round())),
        ParamKind::Toggle => f.check(ui, label, |e| get(e) > 0.5, |e, v| set(e, if v { 1.0 } else { 0.0 })),
        ParamKind::Angle => f.number(ui, label, 1, "°", get, set),
    }
}

/// Shows the section for a group of slabs.
pub fn slabs(ui: &mut Ui, doc: &Document, items: Vec<(Id, Slab)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Slab, locale, "slab");
    widgets::section(ui, "insp-slab", &tr(locale, "ui.insp.group.structure"), true, |ui| {
        widgets::grid(ui, "slab", |ui| {
            structure_rows(
                &mut f,
                ui,
                doc,
                BuildUpUsage::Slab,
                |s| s.structure.clone(),
                |s, v| s.structure = v,
            );
            f.length(ui, &tr(locale, "ui.insp.top_offset"), |s| s.top_offset, |s, v| s.top_offset = v);
            if let Some(s) = f.single() {
                let area = AreaDisplay(s.outline.area(), locale).to_string();
                let holes = s.outline.holes.len().to_string();
                f.read(ui, &tr(locale, "ui.insp.area"), &area);
                f.read(ui, &tr(locale, "ui.insp.holes"), &holes);
            }
        });
    });
    f.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileKind {
    Rectangle,
    Circle,
}

fn profile_kind(p: &Profile) -> ProfileKind {
    match p {
        Profile::Rectangle { .. } => ProfileKind::Rectangle,
        Profile::Circle { .. } => ProfileKind::Circle,
    }
}

fn profile_rows<E: Clone>(
    f: &mut Form<E>,
    ui: &mut Ui,
    get: impl Fn(&E) -> Profile + Copy,
    set: impl Fn(&mut E, Profile) + Copy,
) {
    let locale = f.locale;
    let kinds = vec![
        (ProfileKind::Rectangle, tr(locale, "ui.insp.profile.rectangle")),
        (ProfileKind::Circle, tr(locale, "ui.insp.profile.circle")),
    ];
    f.choice(
        ui,
        &tr(locale, "ui.insp.profile"),
        &kinds,
        |e| profile_kind(&get(e)),
        |e, k| {
            let next = match (k, get(e)) {
                (ProfileKind::Rectangle, Profile::Circle { diameter }) => {
                    Profile::Rectangle { width: diameter, depth: diameter }
                }
                (ProfileKind::Circle, Profile::Rectangle { width, .. }) => {
                    Profile::Circle { diameter: width }
                }
                (_, p) => p,
            };
            set(e, next);
        },
    );
    match f.shared(|e| profile_kind(&get(e))) {
        Some(ProfileKind::Rectangle) => {
            let dims = |p: Profile| match p {
                Profile::Rectangle { width, depth } => (width, depth),
                Profile::Circle { diameter } => (diameter, diameter),
            };
            f.length(
                ui,
                &tr(locale, "ui.insp.width"),
                |e| dims(get(e)).0,
                |e, w| {
                    let (_, d) = dims(get(e));
                    set(e, Profile::Rectangle { width: w, depth: d });
                },
            );
            f.length(
                ui,
                &tr(locale, "ui.insp.depth"),
                |e| dims(get(e)).1,
                |e, d| {
                    let (w, _) = dims(get(e));
                    set(e, Profile::Rectangle { width: w, depth: d });
                },
            );
        }
        Some(ProfileKind::Circle) => {
            let dia = |p: Profile| match p {
                Profile::Circle { diameter } => diameter,
                Profile::Rectangle { width, .. } => width,
            };
            f.length(
                ui,
                &tr(locale, "ui.insp.diameter"),
                |e| dia(get(e)),
                |e, v| set(e, Profile::Circle { diameter: v }),
            );
        }
        None => {}
    }
}

/// Shows the section for a group of columns.
pub fn columns(ui: &mut Ui, doc: &Document, items: Vec<(Id, Column)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Column, locale, "column");
    let mats = material_options(doc);
    widgets::section(ui, "insp-column", &tr(locale, "ui.insp.group.structure"), true, |ui| {
        widgets::grid(ui, "column", |ui| {
            profile_rows(&mut f, ui, |c| c.profile, |c, p| c.profile = p);
            f.choice(ui, &tr(locale, "ui.insp.material"), &mats, |c| c.material, |c, m| c.material = m);
            f.length(ui, &tr(locale, "ui.insp.height"), |c| c.height, |c, v| c.height = v);
            f.length(ui, &tr(locale, "ui.insp.base_offset"), |c| c.base_offset, |c, v| c.base_offset = v);
            f.angle(ui, &tr(locale, "ui.insp.rotation"), |c| c.rotation, |c, v| c.rotation = v);
            if f.single().is_some() {
                point_rows(&mut f, ui, "X", "Y", |c| c.position, |c, p| c.position = p);
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of beams.
pub fn beams(ui: &mut Ui, doc: &Document, items: Vec<(Id, Beam)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Beam, locale, "beam");
    let mats = material_options(doc);
    widgets::section(ui, "insp-beam", &tr(locale, "ui.insp.group.structure"), true, |ui| {
        widgets::grid(ui, "beam", |ui| {
            profile_rows(&mut f, ui, |b| b.profile, |b, p| b.profile = p);
            f.choice(ui, &tr(locale, "ui.insp.material"), &mats, |b| b.material, |b, m| b.material = m);
            f.length(ui, &tr(locale, "ui.insp.top_offset"), |b| b.top_offset, |b, v| b.top_offset = v);
            let len =
                f.shared_length(|b| b.start.distance(b.end)).map(|l| LengthDisplay(l, locale).to_string());
            f.read(ui, &tr(locale, "ui.insp.length"), &len.unwrap_or_else(|| widgets::MIXED.into()));
            if f.single().is_some() {
                point_rows(
                    &mut f,
                    ui,
                    &tr(locale, "ui.insp.start_x"),
                    &tr(locale, "ui.insp.start_y"),
                    |b| b.start,
                    |b, p| b.start = p,
                );
                point_rows(
                    &mut f,
                    ui,
                    &tr(locale, "ui.insp.end_x"),
                    &tr(locale, "ui.insp.end_y"),
                    |b| b.end,
                    |b, p| b.end = p,
                );
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of stairs.
pub fn stairs(ui: &mut Ui, doc: &Document, items: Vec<(Id, Stair)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Stair, locale, "stair");
    let mats = material_options(doc);
    widgets::section(ui, "insp-stair", &tr(locale, "ui.insp.group.geometry"), true, |ui| {
        widgets::grid(ui, "stair", |ui| {
            let shapes: Vec<_> =
                names::STAIR_SHAPES.into_iter().map(|s| (s, names::stair_shape(locale, s))).collect();
            f.choice(ui, &tr(locale, "ui.insp.shape"), &shapes, |s| s.shape, |s, v| s.shape = v);
            f.length(ui, &tr(locale, "ui.insp.width"), |s| s.width, |s, v| s.width = v);
            f.length(ui, &tr(locale, "ui.insp.rise"), |s| s.rise, |s, v| s.rise = v);
            f.count(ui, &tr(locale, "ui.insp.risers"), |s| s.risers, |s, v| s.risers = v.max(2));
            let riser = f
                .shared_length(Stair::riser_height)
                .map(|r| format!("{} cm", widgets::format_number(r.as_cm(), 1, locale)));
            f.read(ui, &tr(locale, "ui.insp.riser_height"), &riser.unwrap_or_else(|| widgets::MIXED.into()));
            f.length(ui, &tr(locale, "ui.insp.going"), |s| s.going, |s, v| s.going = v);
            f.length(ui, &tr(locale, "ui.insp.gap"), |s| s.gap, |s, v| s.gap = v);
            f.length(ui, &tr(locale, "ui.insp.waist"), |s| s.thickness, |s, v| s.thickness = v);
            f.choice(ui, &tr(locale, "ui.insp.material"), &mats, |s| s.material, |s, m| s.material = m);
            f.length(ui, &tr(locale, "ui.insp.base_offset"), |s| s.base_offset, |s, v| s.base_offset = v);
            f.angle(ui, &tr(locale, "ui.insp.direction"), |s| s.direction, |s, v| s.direction = v);
            if f.single().is_some() {
                point_rows(&mut f, ui, "X", "Y", |s| s.origin, |s, p| s.origin = p);
            }
        });
    });
    f.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoofType {
    Flat,
    Pitched,
}

/// Shows the section for a group of roofs.
pub fn roofs(ui: &mut Ui, doc: &Document, items: Vec<(Id, Roof)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Roof, locale, "roof");
    let roof_type = |r: &Roof| match r.shape {
        RoofShape::Flat { .. } => RoofType::Flat,
        RoofShape::Pitched { .. } => RoofType::Pitched,
    };
    widgets::section(ui, "insp-roof", &tr(locale, "ui.insp.group.structure"), true, |ui| {
        widgets::grid(ui, "roof", |ui| {
            let types = vec![
                (RoofType::Flat, tr(locale, "ui.roof.flat")),
                (RoofType::Pitched, tr(locale, "ui.insp.roof_pitched")),
            ];
            f.choice(ui, &tr(locale, "ui.insp.kind"), &types, roof_type, |r, k| {
                r.shape = match k {
                    RoofType::Flat => RoofShape::Flat { parapet: None },
                    RoofType::Pitched => {
                        RoofShape::Pitched { slopes: vec![Some(Angle::deg(22.0)); r.outline.len()] }
                    }
                };
            });
            f.length(ui, &tr(locale, "ui.insp.base"), |r| r.base, |r, v| r.base = v);
            structure_rows(
                &mut f,
                ui,
                doc,
                BuildUpUsage::Roof,
                |r| r.structure.clone(),
                |r, s| r.structure = s,
            );
            match f.shared(roof_type) {
                Some(RoofType::Flat) => {
                    let parapet = |r: &Roof| match r.shape {
                        RoofShape::Flat { parapet } => parapet,
                        RoofShape::Pitched { .. } => None,
                    };
                    f.check(
                        ui,
                        &tr(locale, "ui.insp.parapet"),
                        |r| parapet(r).is_some(),
                        |r, on| {
                            r.shape = RoofShape::Flat {
                                parapet: on.then_some(Parapet {
                                    height: Length::m(1.0),
                                    thickness: Length::cm(20.0),
                                }),
                            };
                        },
                    );
                    if f.shared(|r| parapet(r).is_some()) == Some(true) {
                        let ph = |r: &Roof| parapet(r).map(|p| p.height).unwrap_or(Length::ZERO);
                        let pt = |r: &Roof| parapet(r).map(|p| p.thickness).unwrap_or(Length::ZERO);
                        f.length(ui, &tr(locale, "ui.insp.parapet_height"), ph, |r, v| {
                            r.shape =
                                RoofShape::Flat { parapet: Some(Parapet { height: v, thickness: pt(r) }) };
                        });
                        f.length(ui, &tr(locale, "ui.insp.parapet_thickness"), pt, |r, v| {
                            r.shape =
                                RoofShape::Flat { parapet: Some(Parapet { height: ph(r), thickness: v }) };
                        });
                    }
                }
                Some(RoofType::Pitched) => {
                    let first_pitch = |r: &Roof| match &r.shape {
                        RoofShape::Pitched { slopes } => {
                            slopes.iter().flatten().next().copied().unwrap_or(Angle::ZERO)
                        }
                        RoofShape::Flat { .. } => Angle::ZERO,
                    };
                    f.angle(ui, &tr(locale, "ui.insp.pitch_all"), first_pitch, |r, a| {
                        if let RoofShape::Pitched { slopes } = &mut r.shape {
                            for s in slopes.iter_mut().flatten() {
                                *s = a;
                            }
                        }
                    });
                }
                None => {}
            }
        });
    });
    // Per-edge slopes for a single pitched roof.
    let edges = f.single().and_then(|r| match &r.shape {
        RoofShape::Pitched { slopes } => Some(slopes.len()),
        RoofShape::Flat { .. } => None,
    });
    if let Some(n) = edges {
        widgets::section(ui, "insp-roof-edges", &tr(locale, "ui.insp.group.edges"), false, |ui| {
            widgets::grid(ui, "roof-edges", |ui| {
                for i in 0..n {
                    let label = trf(locale, "ui.insp.edge_n", &[("n", &(i + 1).to_string())]);
                    let slope = move |r: &Roof| match &r.shape {
                        RoofShape::Pitched { slopes } => slopes.get(i).copied().flatten(),
                        RoofShape::Flat { .. } => None,
                    };
                    let set_slope = move |r: &mut Roof, v: Option<Angle>| {
                        if let RoofShape::Pitched { slopes } = &mut r.shape
                            && let Some(s) = slopes.get_mut(i)
                        {
                            *s = v;
                        }
                    };
                    widgets::label(ui, &label);
                    ui.horizontal(|ui| {
                        let sloped = f.shared(|r| slope(r).is_some());
                        if let Some(on) = widgets::tri_check(ui, sloped, &tr(locale, "ui.insp.sloped")) {
                            f.apply(|r| set_slope(r, if on { Some(Angle::deg(22.0)) } else { None }));
                        }
                        if sloped == Some(true) {
                            let deg = f.shared_number(|r| slope(r).map(|a| a.as_deg()).unwrap_or(0.0));
                            if let Some(v) = widgets::number_field(ui, ("roof-edge", i), deg, 1, "°", locale)
                            {
                                f.apply(|r| set_slope(r, Some(Angle::deg(v))));
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        });
    }
    f.finish()
}

/// Shows the section for a group of railings.
pub fn railings(ui: &mut Ui, doc: &Document, items: Vec<(Id, Railing)>, locale: Locale) -> Vec<AnyCommand> {
    let mut f = Form::new(items, ElementKind::Railing, locale, "railing");
    let mats = material_options(doc);
    widgets::section(ui, "insp-railing", &tr(locale, "ui.insp.group.geometry"), true, |ui| {
        widgets::grid(ui, "railing", |ui| {
            f.length(ui, &tr(locale, "ui.insp.height"), |r| r.height, |r, v| r.height = v);
            f.length(ui, &tr(locale, "ui.insp.base_offset"), |r| r.base_offset, |r, v| r.base_offset = v);
            f.length(ui, &tr(locale, "ui.insp.post_spacing"), |r| r.post_spacing, |r, v| r.post_spacing = v);
            let infills: Vec<_> = names::INFILLS.into_iter().map(|i| (i, names::infill(locale, i))).collect();
            f.choice(ui, &tr(locale, "ui.insp.infill"), &infills, |r| r.infill, |r, v| r.infill = v);
            f.choice(ui, &tr(locale, "ui.insp.material"), &mats, |r| r.material, |r, m| r.material = m);
            if let Some(r) = f.single() {
                let len: Length = r
                    .path
                    .windows(2)
                    .filter_map(|w| match w {
                        [a, b] => Some(a.distance(*b)),
                        _ => None,
                    })
                    .sum();
                f.read(ui, &tr(locale, "ui.insp.length"), &LengthDisplay(len, locale).to_string());
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of spaces.
pub fn spaces(ui: &mut Ui, doc: &Document, items: Vec<(Id, Space)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Space, locale, "space");
    widgets::section(ui, "insp-space", &tr(locale, "ui.insp.group.space"), true, |ui| {
        widgets::grid(ui, "space", |ui| {
            f.text(ui, &tr(locale, "ui.insp.name"), |s| s.name.clone(), |s, v| s.name = v);
            f.text(ui, &tr(locale, "ui.insp.number"), |s| s.number.clone(), |s, v| s.number = v);
            let cats: Vec<_> =
                names::SPACE_CATEGORIES.into_iter().map(|c| (c, names::space_category(locale, c))).collect();
            f.choice(ui, &tr(locale, "ui.insp.category"), &cats, |s| s.category, |s, v| s.category = v);
            f.check(
                ui,
                &tr(locale, "ui.insp.own_height"),
                |s| s.height.is_some(),
                |s, on| {
                    s.height = if on { Some(s.height.unwrap_or(Length::m(2.8))) } else { None };
                },
            );
            if f.shared(|s| s.height.is_some()) == Some(true) {
                f.length(
                    ui,
                    &tr(locale, "ui.insp.height"),
                    |s| s.height.unwrap_or(Length::ZERO),
                    |s, v| s.height = Some(v),
                );
            }
            f.text(ui, &tr(locale, "ui.insp.finish"), |s| s.finish.clone(), |s, v| s.finish = v);
            let total: f64 = f.items.iter().map(|(_, s)| s.boundary.area().as_m2()).sum();
            let label =
                if f.items.len() > 1 { tr(locale, "ui.insp.area_total") } else { tr(locale, "ui.insp.area") };
            f.read(ui, &label, &AreaDisplay(kanavos_units::Area::m2(total), locale).to_string());
            if let Some(s) = f.single() {
                let per = LengthDisplay(s.boundary.perimeter(), locale).to_string();
                f.read(ui, &tr(locale, "ui.insp.perimeter"), &per);
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of placed objects.
pub fn objects(ui: &mut Ui, doc: &Document, items: Vec<(Id, Object)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Object, locale, "object");
    widgets::section(ui, "insp-object", &tr(locale, "ui.insp.group.type"), true, |ui| {
        widgets::grid(ui, "object", |ui| {
            let defs: Vec<(String, String)> = kanavos_objects::catalogue()
                .iter()
                .filter(|d| !d.category.is_opening())
                .map(|d| {
                    (
                        d.id.to_owned(),
                        format!(
                            "{} · {}",
                            names::object_category(locale, d.category),
                            names::object(locale, d)
                        ),
                    )
                })
                .collect();
            f.choice(
                ui,
                &tr(locale, "ui.insp.definition"),
                &defs,
                |o| o.definition.clone(),
                |o, v| {
                    o.definition = v;
                    o.params.clear();
                },
            );
            let def = f.shared(|o| o.definition.clone()).and_then(|d| kanavos_objects::find(&d));
            for (key, label) in
                [("width", "ui.insp.width"), ("depth", "ui.insp.depth"), ("height", "ui.insp.height")]
            {
                let fallback = |o: &Object| {
                    let d = kanavos_objects::find(&o.definition);
                    let base = match key {
                        "width" => d.map(|d| d.width),
                        "depth" => d.map(|d| d.depth),
                        _ => d.map(|d| d.height),
                    };
                    kanavos_objects::param(d, &o.params, key, base.unwrap_or(1.0))
                };
                f.length(
                    ui,
                    &tr(locale, label),
                    |o| Length::m(fallback(o)),
                    |o, v| {
                        o.params.insert(key.to_owned(), v.as_m());
                    },
                );
            }
            if let Some(def) = def {
                for p in &def.params {
                    let label = names::object_param(locale, p.label, p.key);
                    let key = p.key;
                    let get = move |o: &Object| kanavos_objects::param(Some(def), &o.params, key, p.default);
                    let set = move |o: &mut Object, v: f64| {
                        o.params.insert(key.to_owned(), v.clamp(p.min, p.max));
                    };
                    param_row(&mut f, ui, &label, p.kind, get, set);
                }
            }
        });
    });
    widgets::section(ui, "insp-object-placement", &tr(locale, "ui.insp.group.placement"), true, |ui| {
        widgets::grid(ui, "object-placement", |ui| {
            f.angle(ui, &tr(locale, "ui.insp.rotation"), |o| o.rotation, |o, v| o.rotation = v);
            f.length(ui, &tr(locale, "ui.insp.elevation"), |o| o.elevation, |o, v| o.elevation = v);
            f.check(ui, &tr(locale, "ui.insp.mirrored"), |o| o.mirrored, |o, v| o.mirrored = v);
            if f.single().is_some() {
                point_rows(&mut f, ui, "X", "Y", |o| o.position, |o, p| o.position = p);
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of text notes.
pub fn texts(ui: &mut Ui, doc: &Document, items: Vec<(Id, TextNote)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Text, locale, "text");
    widgets::section(ui, "insp-text", &tr(locale, "ui.insp.group.text"), true, |ui| {
        let content = f.shared(|t| t.content.clone());
        let width = ui.available_width() - 8.0;
        if let Some(v) = widgets::multiline_field(ui, "text-content", content.as_deref(), width)
            && !v.trim().is_empty()
        {
            f.apply(|t| t.content = v.clone());
        }
        widgets::grid(ui, "text", |ui| {
            f.number(
                ui,
                &tr(locale, "ui.insp.text_size"),
                1,
                "mm",
                |t| t.size_mm,
                |t, v| t.size_mm = v.max(0.5),
            );
            f.angle(ui, &tr(locale, "ui.insp.rotation"), |t| t.rotation, |t, v| t.rotation = v);
            let aligns = vec![
                (TextAlign::Left, tr(locale, "ui.align.left")),
                (TextAlign::Centre, tr(locale, "ui.align.centre")),
                (TextAlign::Right, tr(locale, "ui.align.right")),
            ];
            f.choice(ui, &tr(locale, "ui.insp.align"), &aligns, |t| t.align, |t, v| t.align = v);
            f.check(ui, &tr(locale, "ui.insp.bold"), |t| t.bold, |t, v| t.bold = v);
        });
    });
    f.finish()
}

/// Shows the section for a group of dimensions.
pub fn dimensions(
    ui: &mut Ui,
    doc: &Document,
    items: Vec<(Id, Dimension)>,
    locale: Locale,
) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Dimension, locale, "dimension");
    widgets::section(ui, "insp-dimension", &tr(locale, "ui.insp.group.geometry"), true, |ui| {
        widgets::grid(ui, "dimension", |ui| {
            f.angle(ui, &tr(locale, "ui.insp.direction"), |d| d.direction, |d, v| d.direction = v);
            if let Some(d) = f.single() {
                f.read(ui, &tr(locale, "ui.insp.points"), &d.points.len().to_string());
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of level marks.
pub fn levels(ui: &mut Ui, doc: &Document, items: Vec<(Id, LevelMark)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Level, locale, "level");
    widgets::section(ui, "insp-level", &tr(locale, "ui.insp.group.annotation"), true, |ui| {
        widgets::grid(ui, "level", |ui| {
            f.check(
                ui,
                &tr(locale, "ui.insp.level_auto"),
                |l| l.value.is_none(),
                |l, auto| {
                    l.value = if auto { None } else { Some(l.value.unwrap_or(Length::ZERO)) };
                },
            );
            if f.shared(|l| l.value.is_some()) == Some(true) {
                f.length(
                    ui,
                    &tr(locale, "ui.insp.level_value"),
                    |l| l.value.unwrap_or(Length::ZERO),
                    |l, v| l.value = Some(v),
                );
            }
            f.text(ui, &tr(locale, "ui.insp.label"), |l| l.label.clone(), |l, v| l.label = v);
        });
    });
    f.finish()
}

/// Shows the section for a group of drafting items.
pub fn drafting(ui: &mut Ui, doc: &Document, items: Vec<(Id, Drafting)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Drafting, locale, "drafting");
    widgets::section(ui, "insp-drafting", &tr(locale, "ui.insp.group.style"), true, |ui| {
        widgets::grid(ui, "drafting", |ui| {
            let weights: Vec<(u32, String)> = names::LINE_WEIGHTS
                .iter()
                .map(|w| {
                    ((w * 100.0).round() as u32, format!("{} mm", widgets::format_number(*w, 2, locale)))
                })
                .collect();
            f.choice(
                ui,
                &tr(locale, "ui.insp.line_weight"),
                &weights,
                |d| (d.style.weight_mm * 100.0).round() as u32,
                |d, v| {
                    d.style.weight_mm = f64::from(v) / 100.0;
                },
            );
            let patterns: Vec<_> = names::line_patterns()
                .into_iter()
                .map(|p| {
                    let label = names::line_pattern(locale, &p);
                    (p, label)
                })
                .collect();
            f.choice(
                ui,
                &tr(locale, "ui.insp.line_pattern"),
                &patterns,
                |d| d.style.pattern.clone(),
                |d, v| d.style.pattern = v,
            );
            widgets::label(ui, &tr(locale, "ui.insp.color"));
            match f.shared(|d| d.style.color) {
                Some(c) => {
                    if let Some(n) = widgets::color_field(ui, c) {
                        f.apply(|d| d.style.color = n);
                    }
                }
                None => {
                    widgets::value(ui, widgets::MIXED);
                }
            }
            ui.end_row();
            let fills = f.shared(|d| matches!(d.shape, DraftShape::Fill { .. })) == Some(true);
            if fills {
                let hatches: Vec<(HatchPattern, String)> =
                    names::HATCHES.into_iter().map(|h| (h, names::hatch(locale, h))).collect();
                f.choice(
                    ui,
                    &tr(locale, "ui.insp.hatch"),
                    &hatches,
                    |d| d.style.hatch,
                    |d, v| d.style.hatch = v,
                );
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of grid lines.
pub fn grids(ui: &mut Ui, doc: &Document, items: Vec<(Id, GridLine)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Grid, locale, "grid");
    widgets::section(ui, "insp-grid", &tr(locale, "ui.insp.group.annotation"), true, |ui| {
        widgets::grid(ui, "gridline", |ui| {
            f.text(ui, &tr(locale, "ui.insp.label"), |g| g.label.clone(), |g, v| g.label = v);
            if f.single().is_some() {
                point_rows(
                    &mut f,
                    ui,
                    &tr(locale, "ui.insp.start_x"),
                    &tr(locale, "ui.insp.start_y"),
                    |g| g.start,
                    |g, p| g.start = p,
                );
                point_rows(
                    &mut f,
                    ui,
                    &tr(locale, "ui.insp.end_x"),
                    &tr(locale, "ui.insp.end_y"),
                    |g| g.end,
                    |g, p| g.end = p,
                );
            }
        });
    });
    f.finish()
}

/// Shows the section for a group of underlays.
pub fn underlays(ui: &mut Ui, doc: &Document, items: Vec<(Id, Underlay)>, locale: Locale) -> Vec<AnyCommand> {
    let _ = doc;
    let mut f = Form::new(items, ElementKind::Underlay, locale, "underlay");
    widgets::section(ui, "insp-underlay", &tr(locale, "ui.insp.group.annotation"), true, |ui| {
        widgets::grid(ui, "underlay", |ui| {
            f.text(ui, &tr(locale, "ui.insp.name"), |u| u.name.clone(), |u, v| u.name = v);
            if let Some(u) = f.single() {
                let source = if u.source.is_empty() { "—".to_owned() } else { u.source.clone() };
                f.read(ui, &tr(locale, "ui.insp.source"), &source);
            }
        });
    });
    f.finish()
}
