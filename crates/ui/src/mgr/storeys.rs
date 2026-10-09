//! Storeys manager: name, elevation, height, cut height and below-ground flag of every storey; add, duplicate
//! and delete.

use egui::{RichText, Ui};
use kanavos_engine::commands::{DeleteStorey, UpdateStorey};
use kanavos_model::Id;
use kanavos_units::format_level;

use crate::actions::Action;
use crate::icons::{self, Icon};
use crate::names::{tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets;

/// Interface state of the storeys manager.
pub struct StoreysWindow {
    shift_above: bool,
    confirm_delete: Option<Id>,
}

impl Default for StoreysWindow {
    fn default() -> Self {
        StoreysWindow { shift_above: true, confirm_delete: None }
    }
}

impl StoreysWindow {
    /// Shows the manager.
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        let doc = state.doc().clone();
        ui.horizontal(|ui| {
            if ui.button(format!("+ {}", tr(locale, "ui.tree.storey_add_above"))).clicked() {
                crate::tree::add_storey_above(state);
            }
            if ui.button(format!("+ {}", tr(locale, "ui.tree.storey_add_below"))).clicked() {
                crate::tree::add_storey_below(state);
            }
            ui.checkbox(&mut self.shift_above, tr(locale, "ui.mgr.shift_above"))
                .on_hover_text(tr(locale, "ui.mgr.shift_above_hint"));
        });
        ui.add_space(6.0);
        let mut update: Option<UpdateStorey> = None;
        egui::ScrollArea::vertical().id_salt("storeys-table").show(ui, |ui| {
            egui::Grid::new("storeys-grid").num_columns(7).striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                for key in [
                    "ui.insp.name",
                    "ui.storey.elevation",
                    "ui.storey.height",
                    "ui.storey.cut",
                    "ui.storey.below",
                    "ui.storey.elements",
                    "",
                ] {
                    let text = if key.is_empty() { String::new() } else { tr(locale, key) };
                    ui.label(RichText::new(text).small().color(t.text_muted));
                }
                ui.end_row();
                for s in doc.storeys().into_iter().rev() {
                    let base =
                        UpdateStorey { id: s.id, shift_above: self.shift_above, ..UpdateStorey::default() };
                    ui.horizontal(|ui| {
                        // The active storey is marked; other rows keep the same indent so names line up.
                        let mark =
                            if s.id == state.active_storey { t.accent } else { egui::Color32::TRANSPARENT };
                        icons::inline(ui, Icon::Check, mark);
                        if let Some(v) = widgets::text_field(ui, ("storey-name", s.id), Some(&s.name), 150.0)
                            && !v.trim().is_empty()
                        {
                            update = Some(UpdateStorey { name: Some(v), ..base.clone() });
                        }
                    });
                    if let Some(v) =
                        widgets::length_field(ui, ("storey-elev", s.id), Some(s.elevation), locale)
                    {
                        update = Some(UpdateStorey { elevation: Some(v), ..base.clone() });
                    }
                    if let Some(v) = widgets::length_field(ui, ("storey-h", s.id), Some(s.height), locale) {
                        update = Some(UpdateStorey { height: Some(v), ..base.clone() });
                    }
                    if let Some(v) =
                        widgets::length_field(ui, ("storey-cut", s.id), Some(s.cut_height), locale)
                    {
                        update = Some(UpdateStorey { cut_height: Some(v), ..base.clone() });
                    }
                    let mut below = s.below_ground;
                    if ui.checkbox(&mut below, "").changed() {
                        update = Some(UpdateStorey { below_ground: Some(below), ..base.clone() });
                    }
                    ui.label(doc.elements_on(s.id).count().to_string());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if icons::button(ui, Icon::PlanView, false, 22.0, &tr(locale, "ui.tree.open_plan"))
                            .clicked()
                        {
                            actions.push(Action::ActivateStorey(s.id));
                        }
                        if icons::button(ui, Icon::Copy, false, 22.0, &tr(locale, "ui.tree.duplicate"))
                            .clicked()
                        {
                            crate::tree::duplicate_storey(state, s.id);
                        }
                        let can = doc.storeys().len() > 1;
                        if icons::button_enabled(
                            ui,
                            Icon::Trash,
                            false,
                            can,
                            22.0,
                            &tr(locale, "ui.tree.delete"),
                        )
                        .clicked()
                        {
                            self.confirm_delete = Some(s.id);
                        }
                    });
                    ui.end_row();
                }
            });
        });
        ui.add_space(4.0);
        let top = doc.storeys().last().map(|s| format_level(s.top(), locale)).unwrap_or_default();
        widgets::hint(ui, &trf(locale, "ui.mgr.building_top", &[("l", &top)]));
        if let Some(u) = update {
            let _ = state.run(u.into(), false);
        }
        if let Some(id) = self.confirm_delete {
            let name = doc.storey(id).map(|s| s.name.clone()).unwrap_or_default();
            let n = doc.elements_on(id).count().to_string();
            ui.add_space(6.0);
            egui::Frame::new().fill(t.error_weak).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.label(trf(locale, "ui.confirm.delete_storey_body", &[("name", &name), ("n", &n)]));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new(tr(locale, "common.delete")).color(t.error)).clicked() {
                        let _ = state.run(DeleteStorey { id }.into(), false);
                        self.confirm_delete = None;
                    }
                    if ui.button(tr(locale, "common.cancel")).clicked() {
                        self.confirm_delete = None;
                    }
                });
            });
        }
    }
}
