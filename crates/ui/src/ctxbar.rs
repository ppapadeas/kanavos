//! The context bar (docs/ux.md §2): the active tool, its key parameters, and search.
//!
//! Parameters edit [`kanavos_tools::ToolSettings`] directly; they are interface state, not document changes, so
//! they never enter the undo history. Lengths accept typed values with units and arithmetic like every field.

use egui::{Align, Layout, RichText, Ui};
use kanavos_drawing::HatchPattern;
use kanavos_model::{
    BuildUpUsage, Document, Id, Parapet, Profile, ReferenceLine, StairShape, Structure, TextAlign,
};
use kanavos_objects::ObjectCategory;
use kanavos_tools::{DimensionOrientation, OutlineMode, RoofKind, ToolId, ToolSettings, WallMode};
use kanavos_units::{Angle, Length, Locale};

use crate::actions::Action;
use crate::fuzzy;
use crate::icons::{self, Icon};
use crate::names::{self, tr};
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets;

/// Interface state of the context bar (the object picker's search text).
#[derive(Default)]
pub struct ContextBar {
    object_query: String,
    object_category: Option<ObjectCategory>,
}

fn param_label(ui: &mut Ui, text: &str) {
    let t = Tokens::of(ui);
    ui.label(RichText::new(text).small().color(t.text_muted));
}

fn gap(ui: &mut Ui) {
    ui.add_space(6.0);
    let t = Tokens::of(ui);
    let r = ui.available_rect_before_wrap();
    let x = ui.cursor().left();
    ui.painter().vline(x, (r.center().y - 9.0)..=(r.center().y + 9.0), egui::Stroke::new(1.0, t.border));
    ui.add_space(8.0);
}

/// A build-up drop-down for a tool: `None` means the document default for the usage.
fn build_up_combo(
    ui: &mut Ui,
    id: &str,
    doc: &Document,
    usage: BuildUpUsage,
    default: Option<Id>,
    value: &mut Option<Structure>,
    locale: Locale,
) {
    let label_of = |b: Id| {
        doc.build_up(b)
            .map(|b| format!("{} · {} cm", b.name, widgets::format_number(b.thickness().as_cm(), 1, locale)))
            .unwrap_or_else(|| "—".into())
    };
    let current = match value {
        None => match default {
            Some(d) => format!("{} ({})", label_of(d), tr(locale, "ui.ctx.default")),
            None => tr(locale, "ui.ctx.default"),
        },
        Some(Structure::Layered { build_up }) => label_of(*build_up),
        Some(Structure::Basic { material, thickness }) => format!(
            "{} · {} cm",
            doc.material(*material).map(|m| m.name.as_str()).unwrap_or("—"),
            widgets::format_number(thickness.as_cm(), 1, locale)
        ),
    };
    let current = widgets::ellipsize(&current, widgets::chars_for(230.0));
    egui::ComboBox::from_id_salt(id).width(230.0).selected_text(current).truncate().show_ui(ui, |ui| {
        if ui.selectable_label(value.is_none(), tr(locale, "ui.ctx.use_default")).clicked() {
            *value = None;
        }
        ui.separator();
        for b in doc.build_ups().filter(|b| b.usage == usage || b.usage == BuildUpUsage::Any) {
            let selected = matches!(value, Some(Structure::Layered { build_up }) if *build_up == b.id);
            if ui.selectable_label(selected, label_of(b.id)).clicked() {
                *value = Some(Structure::Layered { build_up: b.id });
            }
        }
        ui.separator();
        let basic = matches!(value, Some(Structure::Basic { .. }));
        if ui.selectable_label(basic, tr(locale, "ui.insp.basic_material")).clicked() && !basic {
            let material = doc.materials().next().map(|m| m.id).unwrap_or_default();
            *value = Some(Structure::Basic { material, thickness: Length::cm(25.0) });
        }
    });
    if let Some(Structure::Basic { material, thickness }) = value {
        let mats = crate::inspector::material_options(doc);
        if let Some(m) = widgets::choice(ui, (id, "material"), Some(material), &mats, 150.0) {
            *material = m;
        }
        if let Some(t) = widgets::cm_field(ui, (id, "thickness"), Some(*thickness), locale) {
            *thickness = t;
        }
    }
}

/// A material drop-down where `None` is the document default.
fn material_combo(
    ui: &mut Ui,
    id: &str,
    doc: &Document,
    value: &mut Option<Id>,
    default: Option<Id>,
    locale: Locale,
) {
    let name = |m: Id| doc.material(m).map(|m| m.name.clone()).unwrap_or_else(|| "—".into());
    let current = match value {
        Some(m) => name(*m),
        None => format!("{} ({})", default.map(name).unwrap_or_default(), tr(locale, "ui.ctx.default")),
    };
    let current = widgets::ellipsize(&current, widgets::chars_for(170.0));
    egui::ComboBox::from_id_salt(id).width(170.0).selected_text(current).truncate().show_ui(ui, |ui| {
        if ui.selectable_label(value.is_none(), tr(locale, "ui.ctx.use_default")).clicked() {
            *value = None;
        }
        ui.separator();
        for m in doc.materials() {
            if ui.selectable_label(*value == Some(m.id), &m.name).clicked() {
                *value = Some(m.id);
            }
        }
    });
}

/// A length that can follow an automatic value (e.g. the storey height): shows the automatic value muted and a
/// reset button when a value was typed.
fn optional_length(ui: &mut Ui, id: &str, value: &mut Option<Length>, automatic: Length, locale: Locale) {
    let shown = value.unwrap_or(automatic);
    if let Some(v) = widgets::length_field(ui, id, Some(shown), locale) {
        *value = Some(v);
    }
    if value.is_some() {
        if icons::button(ui, Icon::Close, false, 18.0, &tr(locale, "ui.ctx.reset_auto")).clicked() {
            *value = None;
        }
    } else {
        let t = Tokens::of(ui);
        ui.label(RichText::new(tr(locale, "ui.ctx.auto")).small().color(t.text_faint));
    }
}

fn profile_fields(ui: &mut Ui, id: &str, profile: &mut Profile, locale: Locale) {
    let is_circle = matches!(profile, Profile::Circle { .. });
    let options =
        vec![(false, tr(locale, "ui.insp.profile.rectangle")), (true, tr(locale, "ui.insp.profile.circle"))];
    if let Some(c) = widgets::choice(ui, (id, "kind"), Some(&is_circle), &options, 110.0) {
        *profile = match (*profile, c) {
            (Profile::Rectangle { width, .. }, true) => Profile::Circle { diameter: width },
            (Profile::Circle { diameter }, false) => Profile::Rectangle { width: diameter, depth: diameter },
            (p, _) => p,
        };
    }
    match profile {
        Profile::Rectangle { width, depth } => {
            if let Some(v) = widgets::length_field(ui, (id, "w"), Some(*width), locale) {
                *width = v;
            }
            ui.label("×");
            if let Some(v) = widgets::length_field(ui, (id, "d"), Some(*depth), locale) {
                *depth = v;
            }
        }
        Profile::Circle { diameter } => {
            param_label(ui, "Ø");
            if let Some(v) = widgets::length_field(ui, (id, "dia"), Some(*diameter), locale) {
                *diameter = v;
            }
        }
    }
}

fn length_param(ui: &mut Ui, id: &str, label: &str, value: &mut Length, locale: Locale) {
    param_label(ui, label);
    if let Some(v) = widgets::length_field(ui, id, Some(*value), locale) {
        *value = v;
    }
}

/// A row of mutually exclusive choices.
fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, String)]) {
    ui.spacing_mut().item_spacing.x = 2.0;
    for (v, label) in options {
        if ui.selectable_label(*value == *v, label).clicked() {
            *value = *v;
        }
    }
    ui.spacing_mut().item_spacing.x = 6.0;
}

/// How an outline tool gets its outline: by corners, as a rectangle, or by clicking inside walls.
fn outline_modes(ui: &mut Ui, mode: &mut OutlineMode, locale: Locale, pick_first: bool) {
    let pick = (OutlineMode::PickInside, tr(locale, "ui.ctx.mode.pick"));
    let polygon = (OutlineMode::Polygon, tr(locale, "ui.ctx.mode.polygon"));
    let rectangle = (OutlineMode::Rectangle, tr(locale, "ui.ctx.mode.rectangle"));
    let options = if pick_first { [pick, polygon, rectangle] } else { [polygon, rectangle, pick] };
    param_label(ui, &tr(locale, "ui.ctx.outline"));
    if let Some(m) = widgets::choice(ui, ("ctx-outline", pick_first), Some(&*mode), &options, 150.0) {
        *mode = m;
    }
}

/// An angle parameter in degrees.
fn angle_param(ui: &mut Ui, id: &str, label: &str, value: &mut Angle, locale: Locale) {
    param_label(ui, label);
    if let Some(v) = widgets::number_field(ui, id, Some(value.as_deg()), 1, "°", locale) {
        *value = Angle::deg(v);
    }
}

impl ContextBar {
    /// Shows the bar.
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        let doc = state.doc().clone();
        let storey_height = doc.storey(state.active_storey).map(|s| s.height).unwrap_or(Length::m(3.0));
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let tool = state.tool;
            let icon_rect = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover()).0;
            icons::paint(ui.painter(), icon_rect, Icon::Tool(tool), t.accent);
            ui.label(RichText::new(names::tool(locale, tool)).family(crate::fonts::strong()).color(t.text));
            gap(ui);
            // Parameters scroll sideways when they do not fit; search stays at the right edge.
            let width = (ui.available_width() - 250.0).max(120.0);
            egui::ScrollArea::horizontal()
                .id_salt("context-params")
                .max_width(width)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        self.params(ui, &mut state.tool_settings, tool, &doc, storey_height, locale);
                    });
                });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                search_button(ui, locale, actions);
            });
        });
    }

    fn params(
        &mut self,
        ui: &mut Ui,
        s: &mut ToolSettings,
        tool: ToolId,
        doc: &Document,
        storey_height: Length,
        locale: Locale,
    ) {
        let t = Tokens::of(ui);
        match tool {
            ToolId::Wall => {
                let modes = [
                    (WallMode::Chain, tr(locale, "ui.ctx.mode.chain")),
                    (WallMode::Rectangle, tr(locale, "ui.ctx.mode.rectangle")),
                    (WallMode::Arc, tr(locale, "ui.ctx.mode.arc")),
                ];
                segmented(ui, &mut s.wall.mode, &modes);
                gap(ui);
                param_label(ui, &tr(locale, "ui.insp.structure"));
                let default = doc.defaults().exterior_wall;
                build_up_combo(
                    ui,
                    "ctx-wall",
                    doc,
                    BuildUpUsage::Wall,
                    default,
                    &mut s.wall.structure,
                    locale,
                );
                param_label(ui, &tr(locale, "ui.insp.height"));
                optional_length(ui, "ctx-wall-h", &mut s.wall.height, storey_height, locale);
                param_label(ui, &tr(locale, "ui.insp.reference"));
                for (r, icon) in [
                    (ReferenceLine::Left, Icon::RefLeft),
                    (ReferenceLine::Centre, Icon::RefCentre),
                    (ReferenceLine::Right, Icon::RefRight),
                ] {
                    if icons::button(ui, icon, s.wall.reference == r, 24.0, &names::reference(locale, r))
                        .clicked()
                    {
                        s.wall.reference = r;
                    }
                }
                let label = tr(locale, "ui.insp.base_offset");
                length_param(ui, "ctx-wall-base", &label, &mut s.wall.base_offset, locale);
            }
            ToolId::Slab => {
                outline_modes(ui, &mut s.slab.mode, locale, false);
                gap(ui);
                param_label(ui, &tr(locale, "ui.insp.structure"));
                let default = doc.defaults().floor_slab;
                build_up_combo(
                    ui,
                    "ctx-slab",
                    doc,
                    BuildUpUsage::Slab,
                    default,
                    &mut s.slab.structure,
                    locale,
                );
                let label = tr(locale, "ui.insp.top_offset");
                length_param(ui, "ctx-slab-top", &label, &mut s.slab.top_offset, locale);
            }
            ToolId::Column => {
                param_label(ui, &tr(locale, "ui.insp.profile"));
                profile_fields(ui, "ctx-col", &mut s.column.profile, locale);
                param_label(ui, &tr(locale, "ui.insp.material"));
                let default = doc.defaults().structural_material;
                material_combo(ui, "ctx-col-mat", doc, &mut s.column.material, default, locale);
                param_label(ui, &tr(locale, "ui.insp.height"));
                optional_length(ui, "ctx-col-h", &mut s.column.height, storey_height, locale);
                angle_param(
                    ui,
                    "ctx-col-rot",
                    &tr(locale, "ui.insp.rotation"),
                    &mut s.column.rotation,
                    locale,
                );
                ui.checkbox(&mut s.column.pick_rotation, tr(locale, "ui.ctx.pick_rotation"))
                    .on_hover_text(tr(locale, "ui.ctx.pick_rotation_hint"));
            }
            ToolId::Beam => {
                param_label(ui, &tr(locale, "ui.insp.profile"));
                profile_fields(ui, "ctx-beam", &mut s.beam.profile, locale);
                param_label(ui, &tr(locale, "ui.insp.material"));
                let default = doc.defaults().structural_material;
                material_combo(ui, "ctx-beam-mat", doc, &mut s.beam.material, default, locale);
                param_label(ui, &tr(locale, "ui.insp.top_offset"));
                optional_length(ui, "ctx-beam-top", &mut s.beam.top_offset, storey_height, locale);
            }
            ToolId::Door | ToolId::Window => {
                let door = tool == ToolId::Door;
                let category = if door { ObjectCategory::Door } else { ObjectCategory::Window };
                let styles: Vec<(String, String)> = kanavos_objects::of_category(category)
                    .map(|d| (d.id.to_owned(), names::object(locale, d)))
                    .collect();
                param_label(ui, &tr(locale, "ui.insp.style"));
                let style = if door { &mut s.opening.door_style } else { &mut s.opening.window_style };
                if let Some(v) = widgets::choice(ui, ("ctx-style", door), Some(&*style), &styles, 200.0) {
                    // Picking a style also takes its catalogue size, the usual starting point.
                    if let Some(def) = kanavos_objects::find(&v) {
                        let size = (Length::m(def.width), Length::m(def.height));
                        if door {
                            s.opening.door_size = size;
                        } else {
                            s.opening.window_size = size;
                            s.opening.window_sill = Length::m(def.sill);
                        }
                    }
                    *style = v;
                }
                let size = if door { &mut s.opening.door_size } else { &mut s.opening.window_size };
                let (w_id, h_id) =
                    if door { ("ctx-door-w", "ctx-door-h") } else { ("ctx-win-w", "ctx-win-h") };
                length_param(ui, w_id, &tr(locale, "ui.insp.width"), &mut size.0, locale);
                length_param(ui, h_id, &tr(locale, "ui.insp.height"), &mut size.1, locale);
                if !door {
                    let label = tr(locale, "ui.insp.sill");
                    length_param(ui, "ctx-win-sill", &label, &mut s.opening.window_sill, locale);
                }
                let label = tr(locale, "ui.ctx.step");
                length_param(ui, "ctx-open-step", &label, &mut s.opening.step, locale);
            }
            ToolId::Void => {
                let label = tr(locale, "ui.insp.width");
                length_param(ui, "ctx-void-w", &label, &mut s.opening.void_size.0, locale);
                let label = tr(locale, "ui.insp.height");
                length_param(ui, "ctx-void-h", &label, &mut s.opening.void_size.1, locale);
                let label = tr(locale, "ui.ctx.step");
                length_param(ui, "ctx-void-step", &label, &mut s.opening.step, locale);
            }
            ToolId::Stair => {
                let shapes: Vec<_> =
                    names::STAIR_SHAPES.into_iter().map(|x| (x, names::stair_shape(locale, x))).collect();
                param_label(ui, &tr(locale, "ui.insp.shape"));
                if let Some(v) = widgets::choice(ui, "ctx-stair-shape", Some(&s.stair.shape), &shapes, 170.0)
                {
                    s.stair.shape = v;
                }
                let uses: Vec<_> = kanavos_model::StairUse::ALL
                    .into_iter()
                    .map(|u| (u, names::stair_use(locale, u)))
                    .collect();
                let muted = Tokens::of(ui).text_muted;
                ui.label(RichText::new(tr(locale, "ui.insp.stair_use")).small().color(muted))
                    .on_hover_text(tr(locale, "ui.insp.stair_use_hint"));
                if let Some(v) = widgets::choice(ui, "ctx-stair-use", Some(&s.stair.usage), &uses, 220.0) {
                    s.stair.usage = v;
                }
                length_param(ui, "ctx-stair-w", &tr(locale, "ui.insp.width"), &mut s.stair.width, locale);
                length_param(ui, "ctx-stair-g", &tr(locale, "ui.insp.going"), &mut s.stair.going, locale);
                length_param(
                    ui,
                    "ctx-stair-r",
                    &tr(locale, "ui.ctx.max_riser"),
                    &mut s.stair.max_riser,
                    locale,
                );
                length_param(ui, "ctx-stair-t", &tr(locale, "ui.insp.waist"), &mut s.stair.thickness, locale);
                if matches!(s.stair.shape, StairShape::HalfTurnLeft | StairShape::HalfTurnRight) {
                    length_param(ui, "ctx-stair-gap", &tr(locale, "ui.insp.gap"), &mut s.stair.gap, locale);
                }
            }
            ToolId::Roof => {
                outline_modes(ui, &mut s.roof.mode, locale, false);
                gap(ui);
                let kinds: Vec<_> =
                    names::ROOF_KINDS.into_iter().map(|k| (k, names::roof_kind(locale, k))).collect();
                param_label(ui, &tr(locale, "ui.insp.kind"));
                if let Some(k) = widgets::choice(ui, "ctx-roof-kind", Some(&s.roof.kind), &kinds, 110.0) {
                    s.roof.kind = k;
                }
                if s.roof.kind == RoofKind::Flat {
                    let mut on = s.roof.parapet.is_some();
                    if ui.checkbox(&mut on, tr(locale, "ui.insp.parapet")).changed() {
                        s.roof.parapet =
                            on.then_some(Parapet { height: Length::m(0.6), thickness: Length::cm(15.0) });
                    }
                    if let Some(p) = &mut s.roof.parapet {
                        length_param(
                            ui,
                            "ctx-parapet-h",
                            &tr(locale, "ui.insp.height"),
                            &mut p.height,
                            locale,
                        );
                    }
                } else {
                    param_label(ui, &tr(locale, "ui.insp.pitch"));
                    let pitch = Some(s.roof.pitch.as_deg());
                    if let Some(v) = widgets::number_field(ui, "ctx-roof-pitch", pitch, 1, "°", locale) {
                        s.roof.pitch = Angle::deg(v.clamp(1.0, 85.0));
                    }
                }
                length_param(
                    ui,
                    "ctx-roof-over",
                    &tr(locale, "ui.ctx.overhang"),
                    &mut s.roof.overhang,
                    locale,
                );
                param_label(ui, &tr(locale, "ui.insp.base"));
                optional_length(ui, "ctx-roof-base", &mut s.roof.base, storey_height, locale);
                let default = if s.roof.kind == RoofKind::Flat {
                    doc.defaults().flat_roof
                } else {
                    doc.defaults().pitched_roof
                };
                build_up_combo(
                    ui,
                    "ctx-roof",
                    doc,
                    BuildUpUsage::Roof,
                    default,
                    &mut s.roof.structure,
                    locale,
                );
            }
            ToolId::Railing => {
                length_param(ui, "ctx-rail-h", &tr(locale, "ui.insp.height"), &mut s.railing.height, locale);
                let infills: Vec<_> =
                    names::INFILLS.into_iter().map(|i| (i, names::infill(locale, i))).collect();
                param_label(ui, &tr(locale, "ui.insp.infill"));
                if let Some(v) =
                    widgets::choice(ui, "ctx-rail-infill", Some(&s.railing.infill), &infills, 120.0)
                {
                    s.railing.infill = v;
                }
                let label = tr(locale, "ui.insp.post_spacing");
                length_param(ui, "ctx-rail-posts", &label, &mut s.railing.post_spacing, locale);
            }
            ToolId::Space => {
                outline_modes(ui, &mut s.space.mode, locale, true);
                gap(ui);
                param_label(ui, &tr(locale, "ui.insp.name"));
                ui.add(egui::TextEdit::singleline(&mut s.space.name).desired_width(140.0));
                let cats: Vec<_> = names::SPACE_CATEGORIES
                    .into_iter()
                    .map(|c| (c, names::space_category(locale, c)))
                    .collect();
                param_label(ui, &tr(locale, "ui.insp.category"));
                if let Some(v) = widgets::choice(ui, "ctx-space-cat", Some(&s.space.category), &cats, 180.0) {
                    s.space.category = v;
                }
                ui.checkbox(&mut s.space.auto_number, tr(locale, "ui.ctx.auto_number"))
                    .on_hover_text(tr(locale, "ui.ctx.auto_number_hint"));
            }
            ToolId::Object => {
                self.object_picker(ui, &mut s.object, locale);
                let p = &mut s.object_placement;
                angle_param(ui, "ctx-obj-rot", &tr(locale, "ui.insp.rotation"), &mut p.rotation, locale);
                ui.checkbox(&mut p.pick_rotation, tr(locale, "ui.ctx.pick_rotation"))
                    .on_hover_text(tr(locale, "ui.ctx.pick_rotation_hint"));
                ui.checkbox(&mut p.mirrored, tr(locale, "ui.insp.mirrored"));
            }
            ToolId::Text => {
                param_label(ui, &tr(locale, "ui.insp.text_size"));
                if let Some(v) =
                    widgets::number_field(ui, "ctx-text-size", Some(s.text.size_mm), 1, "mm", locale)
                {
                    s.text.size_mm = v.clamp(0.5, 50.0);
                }
                let aligns = [
                    (TextAlign::Left, tr(locale, "ui.align.left")),
                    (TextAlign::Centre, tr(locale, "ui.align.centre")),
                    (TextAlign::Right, tr(locale, "ui.align.right")),
                ];
                segmented(ui, &mut s.text.align, &aligns);
                ui.checkbox(&mut s.text.bold, tr(locale, "ui.insp.bold"));
            }
            ToolId::Level => {
                param_label(ui, &tr(locale, "ui.insp.label"));
                ui.add(egui::TextEdit::singleline(&mut s.level.label).desired_width(140.0));
            }
            ToolId::Grid => {
                param_label(ui, &tr(locale, "ui.insp.label"));
                let mut text = s.grid.label.clone().unwrap_or_default();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(70.0)
                        .hint_text(tr(locale, "ui.ctx.grid_next")),
                );
                if r.changed() {
                    s.grid.label = if text.trim().is_empty() { None } else { Some(text) };
                }
            }
            ToolId::Section | ToolId::Elevation => {
                let label = tr(locale, "ui.ctx.depth");
                length_param(ui, "ctx-section-depth", &label, &mut s.section.depth, locale);
                param_label(ui, &tr(locale, "ui.ctx.scale"));
                let scales: Vec<(u32, String)> =
                    [20, 50, 100, 200, 500].into_iter().map(|n| (n, format!("1:{n}"))).collect();
                if let Some(v) =
                    widgets::choice(ui, "ctx-section-scale", Some(&s.section.scale), &scales, 80.0)
                {
                    s.section.scale = v;
                }
            }
            ToolId::Dimension => {
                let orientations = [
                    (DimensionOrientation::Aligned, tr(locale, "ui.ctx.dim.aligned")),
                    (DimensionOrientation::Horizontal, tr(locale, "ui.ctx.dim.horizontal")),
                    (DimensionOrientation::Vertical, tr(locale, "ui.ctx.dim.vertical")),
                ];
                segmented(ui, &mut s.dimension.orientation, &orientations);
            }
            ToolId::Line | ToolId::Polyline | ToolId::Arc | ToolId::Circle | ToolId::Fill => {
                if tool == ToolId::Fill {
                    outline_modes(ui, &mut s.fill_mode, locale, true);
                    gap(ui);
                }
                let weights: Vec<(u32, String)> = names::LINE_WEIGHTS
                    .iter()
                    .map(|w| {
                        ((w * 100.0).round() as u32, format!("{} mm", widgets::format_number(*w, 2, locale)))
                    })
                    .collect();
                param_label(ui, &tr(locale, "ui.insp.line_weight"));
                let current = (s.draft.weight_mm * 100.0).round() as u32;
                if let Some(v) = widgets::choice(ui, "ctx-draft-w", Some(&current), &weights, 90.0) {
                    s.draft.weight_mm = f64::from(v) / 100.0;
                }
                let patterns: Vec<_> = names::line_patterns()
                    .into_iter()
                    .map(|p| {
                        let l = names::line_pattern(locale, &p);
                        (p, l)
                    })
                    .collect();
                param_label(ui, &tr(locale, "ui.insp.line_pattern"));
                if let Some(v) = widgets::choice(ui, "ctx-draft-p", Some(&s.draft.pattern), &patterns, 130.0)
                {
                    s.draft.pattern = v;
                }
                if let Some(c) = widgets::color_field(ui, s.draft.color) {
                    s.draft.color = c;
                }
                if tool == ToolId::Fill {
                    let hatches: Vec<(HatchPattern, String)> =
                        names::HATCHES.into_iter().map(|h| (h, names::hatch(locale, h))).collect();
                    param_label(ui, &tr(locale, "ui.insp.hatch"));
                    if let Some(v) = widgets::choice(ui, "ctx-draft-h", Some(&s.draft.hatch), &hatches, 130.0)
                    {
                        s.draft.hatch = v;
                    }
                }
            }
            ToolId::Select | ToolId::Measure | ToolId::Plot => {
                ui.label(RichText::new(names::tool_hint(locale, tool)).color(t.text_muted));
            }
        }
    }

    fn object_picker(&mut self, ui: &mut Ui, object: &mut String, locale: Locale) {
        param_label(ui, &tr(locale, "ui.insp.definition"));
        let current = widgets::ellipsize(&names::object_by_id(locale, object), widgets::chars_for(220.0));
        let t = Tokens::of(ui);
        egui::ComboBox::from_id_salt("ctx-object").width(220.0).height(420.0).selected_text(current).show_ui(
            ui,
            |ui| {
                ui.set_min_width(300.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.object_query)
                        .hint_text(tr(locale, "ui.ctx.object_search"))
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(self.object_category.is_none(), tr(locale, "ui.ctx.all")).clicked()
                    {
                        self.object_category = None;
                    }
                    for c in names::OBJECT_CATEGORIES.into_iter().filter(|c| !c.is_opening()) {
                        if kanavos_objects::of_category(c).next().is_none() {
                            continue;
                        }
                        if ui
                            .selectable_label(
                                self.object_category == Some(c),
                                names::object_category(locale, c),
                            )
                            .clicked()
                        {
                            self.object_category = Some(c);
                        }
                    }
                });
                ui.separator();
                let defs = kanavos_objects::catalogue()
                    .iter()
                    .filter(|d| !d.category.is_opening())
                    .filter(|d| self.object_category.is_none_or(|c| c == d.category))
                    .map(|d| {
                        let text = format!(
                            "{} {} {}",
                            names::object(Locale::El, d),
                            names::object(Locale::En, d),
                            names::object_category(locale, d.category)
                        );
                        (d, text)
                    });
                let ranked = fuzzy::rank(&self.object_query, defs);
                if ranked.is_empty() {
                    ui.label(RichText::new(tr(locale, "ui.palette.no_results")).color(t.text_muted));
                }
                for (d, _) in ranked {
                    let label = format!(
                        "{}   ·   {}",
                        names::object(locale, d),
                        names::object_category(locale, d.category)
                    );
                    if ui.selectable_label(*object == d.id, label).clicked() {
                        *object = d.id.to_owned();
                    }
                }
            },
        );
    }
}

/// The search entry at the right of the bar.
fn search_button(ui: &mut Ui, locale: Locale, actions: &mut Vec<Action>) {
    let t = Tokens::of(ui);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(230.0, 26.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, 6.0, if response.hovered() { t.hover } else { t.sunken });
        p.rect_stroke(rect, 6.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let icon_rect =
            egui::Rect::from_center_size(rect.left_center() + egui::vec2(15.0, 0.0), egui::vec2(16.0, 16.0));
        icons::paint(p, icon_rect, Icon::Search, t.text_muted);
        p.text(
            rect.left_center() + egui::vec2(28.0, 0.0),
            egui::Align2::LEFT_CENTER,
            tr(locale, "ui.palette.button"),
            egui::FontId::proportional(crate::theme::size::BODY),
            t.text_muted,
        );
        p.text(
            rect.right_center() - egui::vec2(8.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            "Ctrl+K",
            egui::FontId::proportional(crate::theme::size::SMALL),
            t.text_faint,
        );
    }
    let label = tr(locale, "ui.palette.button");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
    if response.on_hover_text(tr(locale, "ui.palette.hint")).clicked() {
        actions.push(Action::CommandPalette);
    }
}
