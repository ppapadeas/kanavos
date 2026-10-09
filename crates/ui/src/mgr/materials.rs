//! Materials manager: name, category, colour, cut hatch and background, junction priority, λ and density.

use egui::{RichText, Ui, Vec2};
use kanavos_drawing::Rgba;
use kanavos_engine::commands::{CreateMaterial, DeleteMaterial, UpdateMaterial};
use kanavos_model::{Id, Material};

use super::hatch_preview;
use crate::fuzzy;
use crate::icons::{self, Icon};
use crate::names::{self, tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets;

/// Interface state of the materials manager.
#[derive(Default)]
pub struct MaterialsWindow {
    selected: Option<Id>,
    filter: String,
    /// Colour being picked (committed when the picker closes, so a drag is one undo step).
    pending_color: Option<(Id, bool, Rgba)>,
}

impl MaterialsWindow {
    /// Selects a material in the list.
    pub fn select(&mut self, id: Id) {
        self.selected = Some(id);
    }

    /// Shows the manager.
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        let doc = state.doc().clone();
        if self.selected.is_none_or(|id| doc.material(id).is_none()) {
            self.selected = doc.materials().next().map(|m| m.id);
        }
        // Commit a picked colour once the picker has closed.
        if let Some((id, background, c)) = self.pending_color
            && !ui.ctx().any_popup_open()
        {
            self.pending_color = None;
            if let Some(mut m) = doc.material(id).cloned() {
                if background {
                    m.cut_background = Some(c);
                } else {
                    m.color = c;
                }
                let _ = state.run(UpdateMaterial { material: m }.into(), false);
            }
        }
        let doc = state.doc().clone();
        ui.horizontal_top(|ui| {
            // List.
            ui.vertical(|ui| {
                ui.set_width(230.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(tr(locale, "ui.mgr.filter"))
                        .desired_width(230.0),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("materials-list")
                    .max_height(ui.available_height() - 36.0)
                    .show(ui, |ui| {
                        let items = doc.materials().map(|m| {
                            let text = format!("{} {}", m.name, names::material_category(locale, m.category));
                            (m, text)
                        });
                        let mut ranked: Vec<&Material> =
                            fuzzy::rank(&self.filter, items).into_iter().map(|(m, _)| m).collect();
                        if self.filter.trim().is_empty() {
                            ranked.sort_by_key(|m| fuzzy::fold(&m.name));
                        }
                        for m in ranked {
                            let selected = self.selected == Some(m.id);
                            let (rect, r) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), 26.0),
                                egui::Sense::click(),
                            );
                            if selected {
                                ui.painter().rect_filled(rect, 4.0, t.accent_weak);
                            } else if r.hovered() {
                                ui.painter().rect_filled(rect, 4.0, t.hover);
                            }
                            let sw = egui::Rect::from_center_size(
                                rect.left_center() + Vec2::new(14.0, 0.0),
                                Vec2::splat(14.0),
                            );
                            ui.painter().rect_filled(sw, 3.0, widgets::color32(m.color));
                            ui.painter().rect_stroke(
                                sw,
                                3.0,
                                egui::Stroke::new(1.0, t.border_strong),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(
                                rect.left_center() + Vec2::new(28.0, 0.0),
                                egui::Align2::LEFT_CENTER,
                                &m.name,
                                egui::FontId::proportional(crate::theme::size::BODY),
                                t.text,
                            );
                            let name = m.name.clone();
                            r.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    selected,
                                    name.clone(),
                                )
                            });
                            if r.clicked() {
                                self.selected = Some(m.id);
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    if icons::button(ui, Icon::Plus, false, 26.0, &tr(locale, "ui.mgr.new")).clicked() {
                        let material = Material {
                            id: Id(0),
                            name: tr(locale, "ui.mgr.new_material"),
                            category: kanavos_model::MaterialCategory::Other,
                            color: Rgba::rgb(200, 200, 200),
                            cut_pattern: kanavos_drawing::HatchPattern::None,
                            cut_background: None,
                            priority: 50,
                            conductivity: None,
                            density: None,
                        };
                        if let Ok(out) = state.run(CreateMaterial { material }.into(), false) {
                            self.selected = out.created.first().copied();
                        }
                    }
                    let current = self.selected.and_then(|id| doc.material(id)).cloned();
                    if icons::button_enabled(
                        ui,
                        Icon::Copy,
                        false,
                        current.is_some(),
                        26.0,
                        &tr(locale, "ui.mgr.duplicate"),
                    )
                    .clicked()
                        && let Some(m) = current.clone()
                    {
                        let name = trf(locale, "ui.tree.copy_of", &[("name", &m.name)]);
                        if let Ok(out) =
                            state.run(CreateMaterial { material: Material { name, ..m } }.into(), false)
                        {
                            self.selected = out.created.first().copied();
                        }
                    }
                    let in_use = current.as_ref().is_some_and(|m| doc.material_in_use(m.id));
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
                        && let Some(m) = current
                    {
                        let _ = state.run(DeleteMaterial { id: m.id }.into(), false);
                    }
                });
            });
            ui.separator();
            // Details.
            let Some(m) = self.selected.and_then(|id| doc.material(id)).cloned() else {
                widgets::hint(ui, &tr(locale, "ui.mgr.nothing_selected"));
                return;
            };
            ui.vertical(|ui| {
                egui::ScrollArea::vertical().id_salt("material-detail").show(ui, |ui| {
                    self.detail(ui, state, m);
                });
            });
        });
    }

    fn detail(&mut self, ui: &mut Ui, state: &mut AppState, m: Material) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        let mut next = m.clone();
        ui.horizontal(|ui| {
            widgets::swatch(ui, m.color, 20.0);
            ui.label(RichText::new(&m.name).family(crate::fonts::strong()).size(15.0));
            if state.doc().material_in_use(m.id) {
                widgets::pill(ui, &tr(locale, "ui.mgr.used"), t.text_muted, t.hover);
            }
        });
        ui.add_space(6.0);
        widgets::section(ui, "mat-identity", &tr(locale, "ui.mgr.group.identity"), true, |ui| {
            widgets::grid(ui, ("mat-grid-1", m.id), |ui| {
                widgets::label(ui, &tr(locale, "ui.insp.name"));
                if let Some(v) = widgets::text_field(ui, ("mat-name", m.id), Some(&m.name), 220.0)
                    && !v.trim().is_empty()
                {
                    next.name = v;
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.insp.category"));
                let cats: Vec<_> = names::MATERIAL_CATEGORIES
                    .into_iter()
                    .map(|c| (c, names::material_category(locale, c)))
                    .collect();
                if let Some(c) = widgets::choice(ui, ("mat-cat", m.id), Some(&m.category), &cats, 220.0) {
                    next.category = c;
                }
                ui.end_row();
            });
        });
        widgets::section(ui, "mat-graphics", &tr(locale, "ui.mgr.group.graphics"), true, |ui| {
            widgets::grid(ui, ("mat-grid-2", m.id), |ui| {
                widgets::label(ui, &tr(locale, "ui.insp.color"));
                let shown = match self.pending_color {
                    Some((id, false, c)) if id == m.id => c,
                    _ => m.color,
                };
                if let Some(c) = widgets::color_field(ui, shown) {
                    self.pending_color = Some((m.id, false, c));
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.mgr.cut_hatch"));
                ui.horizontal(|ui| {
                    let hatches: Vec<_> =
                        names::HATCHES.into_iter().map(|h| (h, names::hatch(locale, h))).collect();
                    if let Some(h) =
                        widgets::choice(ui, ("mat-hatch", m.id), Some(&m.cut_pattern), &hatches, 160.0)
                    {
                        next.cut_pattern = h;
                    }
                    hatch_preview(ui, next.cut_pattern, next.cut_background, Vec2::new(36.0, 22.0));
                });
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.mgr.cut_background"));
                ui.horizontal(|ui| {
                    let mut has = m.cut_background.is_some();
                    if ui.checkbox(&mut has, "").changed() {
                        next.cut_background = if has { Some(Rgba::rgb(220, 220, 220)) } else { None };
                    }
                    if let Some(bg) = m.cut_background {
                        let shown = match self.pending_color {
                            Some((id, true, c)) if id == m.id => c,
                            _ => bg,
                        };
                        if let Some(c) = widgets::color_field(ui, shown) {
                            self.pending_color = Some((m.id, true, c));
                        }
                    }
                });
                ui.end_row();
            });
        });
        widgets::section(ui, "mat-physics", &tr(locale, "ui.mgr.group.physics"), true, |ui| {
            widgets::grid(ui, ("mat-grid-3", m.id), |ui| {
                widgets::label(ui, &tr(locale, "ui.mgr.priority"));
                if let Some(v) =
                    widgets::number_field(ui, ("mat-prio", m.id), Some(f64::from(m.priority)), 0, "", locale)
                {
                    next.priority = v.round().clamp(0.0, 255.0) as u8;
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.mgr.conductivity"));
                ui.horizontal(|ui| {
                    if let Some(v) =
                        widgets::number_field(ui, ("mat-lambda", m.id), m.conductivity, 3, "W/mK", locale)
                    {
                        next.conductivity = Some(v.max(0.0));
                    }
                    if m.conductivity.is_some()
                        && icons::button(ui, Icon::Close, false, 18.0, &tr(locale, "ui.mgr.clear")).clicked()
                    {
                        next.conductivity = None;
                    }
                });
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.mgr.density"));
                ui.horizontal(|ui| {
                    if let Some(v) =
                        widgets::number_field(ui, ("mat-rho", m.id), m.density, 0, "kg/m³", locale)
                    {
                        next.density = Some(v.max(0.0));
                    }
                    if m.density.is_some()
                        && icons::button(ui, Icon::Close, false, 18.0, &tr(locale, "ui.mgr.clear")).clicked()
                    {
                        next.density = None;
                    }
                });
                ui.end_row();
            });
            widgets::hint(ui, &tr(locale, "ui.mgr.priority_hint"));
        });
        if next != m {
            let _ = state.run(UpdateMaterial { material: next }.into(), false);
        }
    }
}
