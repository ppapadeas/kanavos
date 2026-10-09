//! Build-ups manager: layer editor with material, thickness and core flag; add, remove and reorder layers; live
//! layer-stack preview, total thickness and U-value (`BuildUp::u_value`, ISO 6946).

use egui::{RichText, Ui, Vec2};
use kanavos_engine::commands::{CreateBuildUp, DeleteBuildUp, UpdateBuildUp};
use kanavos_model::{BuildUp, BuildUpLayer, BuildUpUsage, Document, Id};
use kanavos_units::{Length, Locale};

use super::{material_name, paint_hatch};
use crate::fuzzy;
use crate::icons::{self, Icon};
use crate::names::{self, tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets::{self, color32};

/// Interface state of the build-ups manager.
#[derive(Default)]
pub struct BuildUpsWindow {
    selected: Option<Id>,
    filter: String,
}

/// Total thickness and U-value text of a build-up.
pub fn summary(doc: &Document, b: &BuildUp, locale: Locale) -> (String, String) {
    let thickness = format!("{} cm", widgets::format_number(b.thickness().as_cm(), 1, locale));
    let u = b
        .u_value(|m| doc.material(m).and_then(|m| m.conductivity))
        .map(|u| format!("U = {} W/m²K", widgets::format_number(u, 3, locale)))
        .unwrap_or_else(|| tr(locale, "ui.insp.u_unknown"));
    (thickness, u)
}

impl BuildUpsWindow {
    /// Shows the manager.
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        let doc = state.doc().clone();
        if self.selected.is_none_or(|id| doc.build_up(id).is_none()) {
            self.selected = doc.build_ups().next().map(|b| b.id);
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(250.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(tr(locale, "ui.mgr.filter"))
                        .desired_width(250.0),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("buildups-list")
                    .max_height(ui.available_height() - 36.0)
                    .show(ui, |ui| {
                        for usage in names::USAGES {
                            let items: Vec<&BuildUp> = doc.build_ups().filter(|b| b.usage == usage).collect();
                            let ranked: Vec<&BuildUp> =
                                fuzzy::rank(&self.filter, items.into_iter().map(|b| (b, b.name.clone())))
                                    .into_iter()
                                    .map(|(b, _)| b)
                                    .collect();
                            if ranked.is_empty() {
                                continue;
                            }
                            ui.label(RichText::new(names::usage(locale, usage)).small().color(t.text_muted));
                            for b in ranked {
                                let (thickness, _) = summary(&doc, b, locale);
                                let selected = self.selected == Some(b.id);
                                if crate::tree::row(
                                    ui,
                                    Some(Icon::Layers),
                                    &b.name,
                                    &thickness,
                                    selected,
                                    0.0,
                                )
                                .response
                                .clicked()
                                {
                                    self.selected = Some(b.id);
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
                ui.horizontal(|ui| {
                    if icons::button(ui, Icon::Plus, false, 26.0, &tr(locale, "ui.mgr.new")).clicked()
                        && let Some(m) = doc.materials().next().map(|m| m.id)
                    {
                        let b = BuildUp {
                            id: Id(0),
                            name: tr(locale, "ui.mgr.new_buildup"),
                            usage: BuildUpUsage::Wall,
                            layers: vec![BuildUpLayer {
                                material: m,
                                thickness: Length::cm(20.0),
                                core: true,
                            }],
                        };
                        if let Ok(out) = state.run(CreateBuildUp { build_up: b }.into(), false) {
                            self.selected = out.created.first().copied();
                        }
                    }
                    let current = self.selected.and_then(|id| doc.build_up(id)).cloned();
                    if icons::button_enabled(
                        ui,
                        Icon::Copy,
                        false,
                        current.is_some(),
                        26.0,
                        &tr(locale, "ui.mgr.duplicate"),
                    )
                    .clicked()
                        && let Some(b) = current.clone()
                    {
                        let name = trf(locale, "ui.tree.copy_of", &[("name", &b.name)]);
                        if let Ok(out) =
                            state.run(CreateBuildUp { build_up: BuildUp { name, ..b } }.into(), false)
                        {
                            self.selected = out.created.first().copied();
                        }
                    }
                    let in_use = current.as_ref().is_some_and(|b| doc.build_up_in_use(b.id));
                    let label =
                        if in_use { tr(locale, "ui.mgr.in_use") } else { tr(locale, "ui.mgr.delete") };
                    if icons::button_enabled(
                        ui,
                        Icon::Trash,
                        false,
                        current.is_some() && !in_use,
                        26.0,
                        &label,
                    )
                    .clicked()
                        && let Some(b) = current
                    {
                        let _ = state.run(DeleteBuildUp { id: b.id }.into(), false);
                    }
                });
            });
            ui.separator();
            let Some(b) = self.selected.and_then(|id| doc.build_up(id)).cloned() else {
                widgets::hint(ui, &tr(locale, "ui.mgr.nothing_selected"));
                return;
            };
            ui.vertical(|ui| {
                egui::ScrollArea::vertical().id_salt("buildup-detail").show(ui, |ui| detail(ui, state, b));
            });
        });
    }
}

fn detail(ui: &mut Ui, state: &mut AppState, b: BuildUp) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let doc = state.doc().clone();
    let mut next = b.clone();
    widgets::grid(ui, ("bu-grid", b.id), |ui| {
        widgets::label(ui, &tr(locale, "ui.insp.name"));
        if let Some(v) = widgets::text_field(ui, ("bu-name", b.id), Some(&b.name), 260.0)
            && !v.trim().is_empty()
        {
            next.name = v;
        }
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.mgr.usage"));
        let usages: Vec<_> = names::USAGES.into_iter().map(|u| (u, names::usage(locale, u))).collect();
        if let Some(u) = widgets::choice(ui, ("bu-usage", b.id), Some(&b.usage), &usages, 160.0) {
            next.usage = u;
        }
        ui.end_row();
    });
    ui.add_space(8.0);
    let order_key = if b.usage == BuildUpUsage::Wall { "ui.mgr.layers_wall" } else { "ui.mgr.layers_slab" };
    ui.label(RichText::new(tr(locale, "ui.mgr.layers")).family(crate::fonts::strong()));
    ui.label(RichText::new(tr(locale, order_key)).small().color(t.text_muted));
    ui.add_space(4.0);
    let mats = crate::inspector::material_options(&doc);
    let n = b.layers.len();
    let mut swap: Option<(usize, usize)> = None;
    let mut remove: Option<usize> = None;
    egui::Grid::new(("bu-layers", b.id)).num_columns(6).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
        ui.label(RichText::new("#").small().color(t.text_muted));
        ui.label(RichText::new(tr(locale, "ui.insp.material")).small().color(t.text_muted));
        ui.label(RichText::new(tr(locale, "ui.insp.thickness")).small().color(t.text_muted));
        ui.label(RichText::new(tr(locale, "ui.mgr.core")).small().color(t.text_muted));
        ui.label(RichText::new("λ").small().color(t.text_muted));
        ui.label("");
        ui.end_row();
        for (i, layer) in b.layers.iter().enumerate() {
            ui.horizontal(|ui| {
                if let Some(m) = doc.material(layer.material) {
                    widgets::swatch(ui, m.color, 12.0);
                }
                ui.label((i + 1).to_string());
            });
            if let Some(m) =
                widgets::choice(ui, ("bu-layer-mat", b.id, i), Some(&layer.material), &mats, 200.0)
                && let Some(l) = next.layers.get_mut(i)
            {
                l.material = m;
            }
            if let Some(v) = widgets::cm_field(ui, ("bu-layer-t", b.id, i), Some(layer.thickness), locale)
                && let Some(l) = next.layers.get_mut(i)
            {
                l.thickness = v;
            }
            let mut core = layer.core;
            if ui.checkbox(&mut core, "").on_hover_text(tr(locale, "ui.mgr.core_hint")).changed()
                && let Some(l) = next.layers.get_mut(i)
            {
                l.core = core;
            }
            let lambda = doc.material(layer.material).and_then(|m| m.conductivity);
            ui.label(
                RichText::new(
                    lambda.map(|v| widgets::format_number(v, 3, locale)).unwrap_or_else(|| "—".into()),
                )
                .small()
                .color(t.text_muted),
            );
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if icons::button_enabled(ui, Icon::ArrowUp, false, i > 0, 20.0, &tr(locale, "ui.mgr.move_up"))
                    .clicked()
                {
                    swap = Some((i, i - 1));
                }
                if icons::button_enabled(
                    ui,
                    Icon::ArrowDown,
                    false,
                    i + 1 < n,
                    20.0,
                    &tr(locale, "ui.mgr.move_down"),
                )
                .clicked()
                {
                    swap = Some((i, i + 1));
                }
                if icons::button_enabled(
                    ui,
                    Icon::Trash,
                    false,
                    n > 1,
                    20.0,
                    &tr(locale, "ui.mgr.remove_layer"),
                )
                .clicked()
                {
                    remove = Some(i);
                }
            });
            ui.end_row();
        }
    });
    if let Some((a, c)) = swap
        && a < next.layers.len()
        && c < next.layers.len()
    {
        next.layers.swap(a, c);
    }
    if let Some(i) = remove
        && i < next.layers.len()
        && next.layers.len() > 1
    {
        next.layers.remove(i);
    }
    ui.add_space(4.0);
    if ui.button(format!("+ {}", tr(locale, "ui.mgr.add_layer"))).clicked() {
        let material =
            next.layers.last().map(|l| l.material).or_else(|| doc.materials().next().map(|m| m.id));
        if let Some(material) = material {
            next.layers.push(BuildUpLayer { material, thickness: Length::cm(5.0), core: false });
        }
    }
    ui.add_space(10.0);
    // Preview and performance.
    let (thickness, u) = summary(&doc, &next, locale);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(trf(locale, "ui.mgr.total", &[("t", &thickness)])).family(crate::fonts::strong()),
        );
        ui.label(RichText::new("·").color(t.text_muted));
        ui.label(RichText::new(u).color(t.text));
    });
    ui.add_space(4.0);
    stack_preview(ui, &doc, &next, locale);
    if next != b {
        let _ = state.run(UpdateBuildUp { build_up: next }.into(), false);
    }
}

/// Draws the layers to scale: side by side for walls (left face to right face), stacked for slabs and roofs (top
/// to bottom), each in its material colour with its hatch indication and name.
fn stack_preview(ui: &mut Ui, doc: &Document, b: &BuildUp, locale: Locale) {
    let t = Tokens::of(ui);
    let total = b.thickness().as_m();
    if total <= 0.0 {
        return;
    }
    let horizontal = b.usage == BuildUpUsage::Wall;
    let size = if horizontal {
        Vec2::new(ui.available_width().min(460.0), 120.0)
    } else {
        Vec2::new(ui.available_width().min(460.0), 150.0)
    };
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, t.sunken);
    let inner = rect.shrink(8.0);
    let mut offset = 0.0_f32;
    let extent = if horizontal { inner.width() } else { inner.height() };
    for layer in &b.layers {
        let frac = (layer.thickness.as_m() / total) as f32;
        let len = (extent * frac).max(1.0);
        let r = if horizontal {
            egui::Rect::from_min_size(
                egui::pos2(inner.left() + offset, inner.top()),
                Vec2::new(len, inner.height() - 22.0),
            )
        } else {
            egui::Rect::from_min_size(
                egui::pos2(inner.left() + 150.0, inner.top() + offset),
                Vec2::new(inner.width() - 150.0, len),
            )
        };
        let (bg, pattern) = doc
            .material(layer.material)
            .map(|m| (m.cut_background.map(color32).unwrap_or_else(|| color32(m.color)), m.cut_pattern))
            .unwrap_or((egui::Color32::GRAY, kanavos_drawing::HatchPattern::None));
        paint_hatch(p, r, pattern, bg, egui::Color32::from_gray(50));
        let stroke =
            if layer.core { egui::Stroke::new(1.6, t.text) } else { egui::Stroke::new(0.8, t.border_strong) };
        p.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
        let label = format!("{} cm", widgets::format_number(layer.thickness.as_cm(), 1, locale));
        if horizontal {
            if r.width() > 26.0 {
                p.text(
                    egui::pos2(r.center().x, inner.bottom() - 8.0),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(10.5),
                    t.text_muted,
                );
            }
        } else if r.height() > 9.0 {
            let name = material_name(doc, layer.material);
            p.text(
                egui::pos2(inner.left(), r.center().y),
                egui::Align2::LEFT_CENTER,
                format!("{name} · {label}"),
                egui::FontId::proportional(10.5),
                t.text_muted,
            );
        }
        offset += len;
    }
}
