//! The project tree (docs/ux.md §2): site, storeys, views, sheets and schedules.
//!
//! Clicking an entry opens it in the focused view pane; storeys also become the active storey. Storeys, views and
//! sheets can be renamed in place, duplicated or deleted from their context menu. Every change is an engine
//! command, so it can be undone.

use egui::collapsing_header::CollapsingState;
use egui::{Align, Color32, Id as EguiId, Layout, Rect, RichText, Sense, Ui, Vec2};
use kanavos_engine::AnyCommand;
use kanavos_engine::commands::{
    Batch, CopyToStorey, CreateStorey, DeleteSheet, DeleteStorey, DeleteView, UpdateSheet, UpdateStorey,
    UpdateView,
};
use kanavos_model::{Id, ScheduleKind, ViewKind};
use kanavos_units::{AreaDisplay, Length, format_level};

use crate::actions::{Action, TabTarget, Window};
use crate::icons::{self, Icon};
use crate::names::{SCHEDULES, schedule, tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;

/// What is being renamed in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Renaming {
    Storey(Id),
    View(Id),
    Sheet(Id),
}

/// Something waiting for confirmation before it is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doomed {
    Storey(Id),
    View(Id),
    Sheet(Id),
}

/// Interface state of the project tree.
#[derive(Default)]
pub struct ProjectTree {
    renaming: Option<(Renaming, String)>,
    focus_rename: bool,
    confirm: Option<Doomed>,
}

/// Result of a tree row.
pub struct RowResponse {
    /// The row.
    pub response: egui::Response,
}

/// A full-width tree row: optional icon, text, muted text on the right; hover and selected backgrounds.
pub fn row(
    ui: &mut Ui,
    icon: Option<Icon>,
    text: &str,
    right: &str,
    selected: bool,
    indent: f32,
) -> RowResponse {
    let t = Tokens::of(ui);
    let height = 24.0;
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 4.0, t.accent_weak);
        } else if response.hovered() {
            p.rect_filled(rect, 4.0, t.hover);
        }
        let mut x = rect.left() + 6.0 + indent;
        if let Some(icon) = icon {
            let r = Rect::from_min_size(egui::pos2(x, rect.center().y - 8.0), Vec2::splat(16.0));
            icons::paint(p, r, icon, if selected { t.accent } else { t.text_muted });
            x += 22.0;
        }
        let font = egui::FontId::proportional(crate::theme::size::BODY);
        let right_w = if right.is_empty() {
            0.0
        } else {
            let g = p.layout_no_wrap(
                right.to_owned(),
                egui::FontId::proportional(crate::theme::size::SMALL),
                t.text_muted,
            );
            let w = g.size().x;
            p.galley(egui::pos2(rect.right() - 8.0 - w, rect.center().y - g.size().y / 2.0), g, t.text_muted);
            w + 12.0
        };
        let max_w = (rect.right() - x - right_w - 6.0).max(10.0);
        let job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, t.text);
        let mut job = job;
        job.wrap = egui::text::TextWrapping::truncate_at_width(max_w);
        let g = p.layout_job(job);
        p.galley(egui::pos2(x, rect.center().y - g.size().y / 2.0), g, t.text);
    }
    let label = if right.is_empty() { text.to_owned() } else { format!("{text} {right}") };
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label.clone())
    });
    RowResponse { response }
}

/// A collapsible group header with an icon, a title and optional buttons on the right. Call `.body_unindented`
/// on the result to add the group's rows.
fn group<'u>(
    ui: &'u mut Ui,
    id: &str,
    icon: Icon,
    title: &str,
    buttons: impl FnOnce(&mut Ui),
) -> egui::collapsing_header::HeaderResponse<'u, ()> {
    let t = Tokens::of(ui);
    let state = CollapsingState::load_with_default_open(ui.ctx(), EguiId::new(("tree-group", id)), true);
    state.show_header(ui, |ui| {
        icons::inline(ui, icon, t.text_muted);
        ui.label(RichText::new(title).family(crate::fonts::strong()).color(t.text));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            buttons(ui);
        });
    })
}

/// A small muted caption inside a group.
fn caption(ui: &mut Ui, text: &str, indent: f32) {
    let t = Tokens::of(ui);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.label(RichText::new(text).small().color(t.text_muted));
    });
}

impl ProjectTree {
    /// Shows the tree.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        focused: Option<TabTarget>,
    ) {
        let t = Tokens::of(ui);
        let locale = state.locale;
        let doc = state.doc().clone();
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(tr(locale, "ui.tree.title")).small().color(t.text_muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, Icon::ChevronLeft, false, 20.0, &tr(locale, "ui.panel.collapse"))
                    .clicked()
                {
                    actions.push(Action::TogglePanel(crate::actions::Panel::Tree));
                }
            });
        });
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let name = if doc.info().name.trim().is_empty() {
                tr(locale, "ui.tree.untitled")
            } else {
                doc.info().name.clone()
            };
            let label =
                egui::Label::new(RichText::new(name).family(crate::fonts::strong()).size(15.0).color(t.text))
                    .sense(Sense::click())
                    .truncate();
            if ui.add(label).on_hover_text(tr(locale, "ui.tree.project_hint")).clicked() {
                actions.push(Action::OpenWindow(Window::Project));
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            self.site(ui, state, actions, &doc, focused);
            self.storeys(ui, state, actions, &doc);
            self.views(ui, state, actions, &doc, focused);
            self.sheets(ui, state, actions, &doc, focused);
            schedules(ui, state, actions, focused);
        });
        self.confirm_dialog(ui.ctx(), state);
    }

    fn site(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        doc: &kanavos_model::Document,
        focused: Option<TabTarget>,
    ) {
        let locale = state.locale;
        let mut open_terms = false;
        group(ui, "site", Icon::Site, &tr(locale, "ui.tree.site"), |ui| {
            open_terms =
                icons::button(ui, Icon::Gear, false, 22.0, &tr(locale, "ui.tree.site_terms")).clicked();
        })
        .body_unindented(|ui| {
            let plot = doc.site().plot.as_ref().map(|p| AreaDisplay(p.area(), locale).to_string());
            let right = plot.unwrap_or_else(|| tr(locale, "ui.tree.no_plot"));
            if row(ui, Some(Icon::Gear), &tr(locale, "ui.tree.plot_terms"), &right, false, 12.0)
                .response
                .clicked()
            {
                actions.push(Action::OpenWindow(Window::Site));
            }
            for v in doc.views().filter(|v| matches!(v.kind, ViewKind::SitePlan | ViewKind::CoverageDiagram))
            {
                let ic = if matches!(v.kind, ViewKind::SitePlan) { Icon::Site } else { Icon::Coverage };
                let sel = focused == Some(TabTarget::View(v.id));
                if row(ui, Some(ic), &v.name, &format!("1:{}", v.scale), sel, 12.0).response.clicked() {
                    actions.push(Action::OpenTab(TabTarget::View(v.id)));
                }
            }
        });
        if open_terms {
            actions.push(Action::OpenWindow(Window::Site));
        }
    }

    fn storeys(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        doc: &kanavos_model::Document,
    ) {
        let locale = state.locale;
        let mut add = false;
        let mut manage = false;
        let header = group(ui, "storeys", Icon::Storey, &tr(locale, "ui.tree.storeys"), |ui| {
            manage = icons::button(ui, Icon::Gear, false, 22.0, &tr(locale, "ui.window.storeys")).clicked();
            add = icons::button(ui, Icon::Plus, false, 22.0, &tr(locale, "ui.tree.storey_add")).clicked();
        });
        header.body_unindented(|ui| {
            let storeys: Vec<_> = doc.storeys().into_iter().rev().cloned().collect();
            for s in storeys {
                if self.rename_row(ui, state, Renaming::Storey(s.id)) {
                    continue;
                }
                let active = s.id == state.active_storey;
                let level = format_level(s.elevation, locale);
                let count = doc.elements_on(s.id).count();
                let r = row(ui, Some(Icon::PlanView), &s.name, &level, active, 12.0).response;
                let r = r.on_hover_text(trf(locale, "ui.tree.storey_hint", &[("n", &count.to_string())]));
                if r.clicked() {
                    actions.push(Action::ActivateStorey(s.id));
                }
                if r.double_clicked() {
                    self.start_rename(Renaming::Storey(s.id), &s.name);
                }
                r.context_menu(|ui| {
                    if ui.button(tr(locale, "ui.tree.open_plan")).clicked() {
                        actions.push(Action::ActivateStorey(s.id));
                        ui.close();
                    }
                    if ui.button(tr(locale, "ui.tree.rename")).clicked() {
                        self.start_rename(Renaming::Storey(s.id), &s.name);
                        ui.close();
                    }
                    if ui.button(tr(locale, "ui.tree.duplicate")).clicked() {
                        duplicate_storey(state, s.id);
                        ui.close();
                    }
                    if ui.button(tr(locale, "ui.tree.storey_add_above")).clicked() {
                        add_storey_above(state);
                        ui.close();
                    }
                    if ui.button(tr(locale, "ui.tree.storey_add_below")).clicked() {
                        add_storey_below(state);
                        ui.close();
                    }
                    if ui.button(tr(locale, "ui.tree.properties")).clicked() {
                        actions.push(Action::OpenWindow(Window::Storeys));
                        ui.close();
                    }
                    ui.separator();
                    let can_delete = doc.storeys().len() > 1;
                    if ui.add_enabled(can_delete, egui::Button::new(tr(locale, "ui.tree.delete"))).clicked() {
                        self.confirm = Some(Doomed::Storey(s.id));
                        ui.close();
                    }
                });
            }
        });
        if add {
            add_storey_above(state);
        }
        if manage {
            actions.push(Action::OpenWindow(Window::Storeys));
        }
    }

    fn views(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        doc: &kanavos_model::Document,
        focused: Option<TabTarget>,
    ) {
        let locale = state.locale;
        let mut new_section = false;
        let mut new_elevation = false;
        let header = group(ui, "views", Icon::Views, &tr(locale, "ui.tree.views"), |ui| {
            use kanavos_tools::ToolId;
            new_elevation = icons::button(
                ui,
                Icon::Tool(ToolId::Elevation),
                false,
                22.0,
                &tr(locale, "ui.menu.new_elevation"),
            )
            .clicked();
            new_section = icons::button(
                ui,
                Icon::Tool(ToolId::Section),
                false,
                22.0,
                &tr(locale, "ui.menu.new_section"),
            )
            .clicked();
        });
        header.body_unindented(|ui| {
            type Filter = fn(&ViewKind) -> bool;
            let kinds: [(&str, Filter, Icon); 3] = [
                ("ui.tree.sections", |k| matches!(k, ViewKind::Section { .. }), Icon::SectionView),
                ("ui.tree.elevations", |k| matches!(k, ViewKind::Elevation { .. }), Icon::ElevationView),
                ("ui.tree.three_d", |k| matches!(k, ViewKind::ThreeD { .. }), Icon::Cube),
            ];
            for (key, filter, ic) in kinds {
                let views: Vec<_> = doc.views().filter(|v| filter(&v.kind)).cloned().collect();
                caption(ui, &tr(locale, key), 14.0);
                if views.is_empty() {
                    let t = Tokens::of(ui);
                    ui.horizontal(|ui| {
                        ui.add_space(36.0);
                        ui.label(
                            RichText::new(tr(locale, "ui.tree.none_yet"))
                                .small()
                                .italics()
                                .color(t.text_faint),
                        );
                    });
                }
                for v in views {
                    if self.rename_row(ui, state, Renaming::View(v.id)) {
                        continue;
                    }
                    let sel = focused == Some(TabTarget::View(v.id));
                    let right = if matches!(v.kind, ViewKind::ThreeD { .. }) {
                        String::new()
                    } else {
                        format!("1:{}", v.scale)
                    };
                    let r = row(ui, Some(ic), &v.name, &right, sel, 20.0).response;
                    if r.clicked() {
                        actions.push(Action::OpenTab(TabTarget::View(v.id)));
                    }
                    if r.double_clicked() {
                        self.start_rename(Renaming::View(v.id), &v.name);
                    }
                    r.context_menu(|ui| {
                        if ui.button(tr(locale, "ui.tree.open")).clicked() {
                            actions.push(Action::OpenTab(TabTarget::View(v.id)));
                            ui.close();
                        }
                        if ui.button(tr(locale, "ui.tree.rename")).clicked() {
                            self.start_rename(Renaming::View(v.id), &v.name);
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(tr(locale, "ui.tree.delete")).clicked() {
                            self.confirm = Some(Doomed::View(v.id));
                            ui.close();
                        }
                    });
                }
            }
        });
        if new_section {
            actions.push(Action::NewSection);
        }
        if new_elevation {
            actions.push(Action::NewElevation);
        }
    }

    fn sheets(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        actions: &mut Vec<Action>,
        doc: &kanavos_model::Document,
        focused: Option<TabTarget>,
    ) {
        let locale = state.locale;
        let mut add = false;
        let mut permit = false;
        let header = group(ui, "sheets", Icon::Sheet, &tr(locale, "ui.tree.sheets"), |ui| {
            permit =
                icons::button(ui, Icon::Layers, false, 22.0, &tr(locale, "ui.menu.permit_set")).clicked();
            add = icons::button(ui, Icon::Plus, false, 22.0, &tr(locale, "ui.tree.sheet_add")).clicked();
        });
        header.body_unindented(|ui| {
            let sheets: Vec<_> = doc.sheets().into_iter().cloned().collect();
            if sheets.is_empty() {
                caption(ui, &tr(locale, "ui.tree.no_sheets"), 14.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    if ui.button(tr(locale, "ui.menu.permit_set")).clicked() {
                        actions.push(Action::PermitSet);
                    }
                });
            }
            for s in sheets {
                if self.rename_row(ui, state, Renaming::Sheet(s.id)) {
                    continue;
                }
                let sel = focused == Some(TabTarget::Sheet(s.id));
                let text = format!("{}  {}", s.number, s.name);
                let r =
                    row(ui, Some(Icon::Sheet), &text, &crate::names::paper(locale, s.paper.size), sel, 12.0)
                        .response;
                if r.clicked() {
                    actions.push(Action::OpenTab(TabTarget::Sheet(s.id)));
                }
                if r.double_clicked() {
                    self.start_rename(Renaming::Sheet(s.id), &s.name);
                }
                r.context_menu(|ui| {
                    if ui.button(tr(locale, "ui.tree.open")).clicked() {
                        actions.push(Action::OpenTab(TabTarget::Sheet(s.id)));
                        ui.close();
                    }
                    if ui.button(tr(locale, "ui.tree.rename")).clicked() {
                        self.start_rename(Renaming::Sheet(s.id), &s.name);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(tr(locale, "ui.tree.delete")).clicked() {
                        self.confirm = Some(Doomed::Sheet(s.id));
                        ui.close();
                    }
                });
            }
        });
        if add {
            actions.push(Action::NewSheet);
        }
        if permit {
            actions.push(Action::PermitSet);
        }
    }

    fn start_rename(&mut self, what: Renaming, current: &str) {
        self.renaming = Some((what, current.to_owned()));
        self.focus_rename = true;
    }

    /// Shows the in-place rename field when `what` is being renamed; returns whether it did.
    fn rename_row(&mut self, ui: &mut Ui, state: &mut AppState, what: Renaming) -> bool {
        let Some((target, text)) = &mut self.renaming else { return false };
        if *target != what {
            return false;
        }
        let response = ui.add(egui::TextEdit::singleline(text).desired_width(ui.available_width() - 8.0));
        if self.focus_rename {
            response.request_focus();
            self.focus_rename = false;
        }
        if response.lost_focus() {
            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
            let name = text.trim().to_owned();
            if !cancelled && !name.is_empty() {
                let command: Option<AnyCommand> = match what {
                    Renaming::Storey(id) => {
                        Some(UpdateStorey { id, name: Some(name), ..UpdateStorey::default() }.into())
                    }
                    Renaming::View(id) => Some(
                        UpdateView { id, name: Some(name), kind: None, scale: None, hidden_layers: None }
                            .into(),
                    ),
                    Renaming::Sheet(id) => state.doc().sheet(id).map(|s| {
                        let mut s = s.clone();
                        s.name = name;
                        UpdateSheet { sheet: s }.into()
                    }),
                };
                if let Some(c) = command {
                    let _ = state.run(c, false);
                }
            }
            self.renaming = None;
        }
        true
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(doomed) = self.confirm else { return };
        let locale = state.locale;
        let doc = state.doc();
        let (title, body) = match doomed {
            Doomed::Storey(id) => {
                let name = doc.storey(id).map(|s| s.name.clone()).unwrap_or_default();
                let n = doc.elements_on(id).count().to_string();
                (
                    tr(locale, "ui.confirm.delete_storey"),
                    trf(locale, "ui.confirm.delete_storey_body", &[("name", &name), ("n", &n)]),
                )
            }
            Doomed::View(id) => {
                let name = doc.view(id).map(|v| v.name.clone()).unwrap_or_default();
                (
                    tr(locale, "ui.confirm.delete_view"),
                    trf(locale, "ui.confirm.delete_named", &[("name", &name)]),
                )
            }
            Doomed::Sheet(id) => {
                let name = doc.sheet(id).map(|s| format!("{} {}", s.number, s.name)).unwrap_or_default();
                (
                    tr(locale, "ui.confirm.delete_sheet"),
                    trf(locale, "ui.confirm.delete_named", &[("name", &name)]),
                )
            }
        };
        let mut close = false;
        let mut confirmed = false;
        let modal = egui::Modal::new(EguiId::new("tree-confirm")).show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(RichText::new(title).family(crate::fonts::strong()).size(15.0));
            ui.add_space(6.0);
            ui.label(body);
            ui.add_space(4.0);
            crate::widgets::hint(ui, &tr(locale, "ui.confirm.undo_hint"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let t = Tokens::of(ui);
                let delete =
                    egui::Button::new(RichText::new(tr(locale, "common.delete")).color(Color32::WHITE))
                        .fill(t.error);
                if ui.add(delete).clicked() {
                    confirmed = true;
                }
                if ui.button(tr(locale, "common.cancel")).clicked() {
                    close = true;
                }
            });
        });
        if modal.should_close() {
            close = true;
        }
        if confirmed {
            let command: AnyCommand = match doomed {
                Doomed::Storey(id) => DeleteStorey { id }.into(),
                Doomed::View(id) => DeleteView { id }.into(),
                Doomed::Sheet(id) => DeleteSheet { id }.into(),
            };
            let _ = state.run(command, false);
            close = true;
        }
        if close {
            self.confirm = None;
        }
    }
}

/// Adds a storey on top of the highest one, with the same height.
pub fn add_storey_above(state: &mut AppState) {
    let doc = state.doc();
    let top = doc.storeys().last().map(|s| (s.top(), s.height)).unwrap_or((Length::ZERO, Length::m(3.0)));
    let n = doc.storeys().iter().filter(|s| !s.below_ground).count();
    let name = trf(state.locale, "ui.tree.storey_new_name", &[("n", &n.to_string())]);
    let _ = state.run(CreateStorey::new(name, top.0, top.1).into(), false);
}

/// Adds a below-ground storey under the lowest one.
pub fn add_storey_below(state: &mut AppState) {
    let doc = state.doc();
    let lowest = doc.storeys().first().map(|s| s.elevation).unwrap_or(Length::ZERO);
    let height = Length::m(3.0);
    let mut c = CreateStorey::new(tr(state.locale, "ui.tree.basement"), lowest - height, height);
    c.below_ground = true;
    let _ = state.run(c.into(), false);
}

/// Duplicates a storey with all its elements on top of the building, as one undo step.
pub fn duplicate_storey(state: &mut AppState, id: Id) {
    let doc = state.doc();
    let Some(s) = doc.storey(id) else { return };
    let top = doc.storeys().last().map(|s| s.top()).unwrap_or(Length::ZERO);
    let name = trf(state.locale, "ui.tree.copy_of", &[("name", &s.name)]);
    let mut create = CreateStorey::new(name, top, s.height);
    create.cut_height = Some(s.cut_height);
    // `storey.create` takes the next free id for the storey itself.
    let new_storey = doc.peek_next_id();
    let ids: Vec<Id> = doc.elements_on(id).map(|e| e.id).collect();
    let mut commands: Vec<AnyCommand> = vec![create.into()];
    if !ids.is_empty() {
        commands.push(CopyToStorey { ids, storey: new_storey }.into());
    }
    let label = tr(state.locale, "ui.tree.duplicate_label");
    let _ = state.run(Batch { label, commands }.into(), false);
}

/// The schedules group: the fixed schedules and the area table.
fn schedules(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>, focused: Option<TabTarget>) {
    let locale = state.locale;
    group(ui, "schedules", Icon::Table, &tr(locale, "ui.tree.schedules"), |_| {}).body_unindented(|ui| {
        let kinds: [ScheduleKind; 5] = SCHEDULES;
        for kind in kinds {
            let target = TabTarget::Schedule(kind);
            if row(ui, Some(Icon::Table), &schedule(locale, kind), "", focused == Some(target), 12.0)
                .response
                .clicked()
            {
                actions.push(Action::OpenTab(target));
            }
        }
        let target = TabTarget::AreaTable;
        let name = tr(locale, "ui.tree.area_table");
        if row(ui, Some(Icon::Table), &name, "", focused == Some(target), 12.0).response.clicked() {
            actions.push(Action::OpenTab(target));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicating_a_storey_copies_its_elements_in_one_step() {
        let mut state = AppState::new(crate::sample::sample_house(kanavos_units::Locale::El));
        let ground = state.active_storey;
        let before = state.doc().elements_on(ground).count();
        let storeys = state.doc().storeys().len();
        duplicate_storey(&mut state, ground);
        assert_eq!(state.doc().storeys().len(), storeys + 1);
        let new = state.doc().storeys().last().map(|s| s.id).unwrap();
        assert_eq!(state.doc().elements_on(new).count(), before);
        assert_eq!(state.session.version(), 1);
    }

    #[test]
    fn storeys_are_added_above_and_below() {
        let mut state = AppState::new(kanavos_model::starter());
        add_storey_above(&mut state);
        add_storey_below(&mut state);
        let s = state.doc().storeys();
        assert_eq!(s.len(), 4);
        assert!(s.first().is_some_and(|b| b.below_ground));
    }
}
