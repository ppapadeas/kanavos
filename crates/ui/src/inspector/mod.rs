//! The inspector (docs/ux.md §2 and §4): selection-aware properties, the ΝΟΚ meters when nothing is selected,
//! and the history of named steps.
//!
//! Multi-selection shows shared values and "—" for mixed ones; editing a field applies it to every selected
//! element as one undo step. All edits are engine commands.

mod form;
mod kinds;

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use kanavos_engine::AnyCommand;
use kanavos_engine::commands::UpdateElement;
use kanavos_model::{Category, Element, ElementKind, Id, Phase};
use kanavos_rules_gr::NokReport;
use kanavos_units::{
    AreaDisplay, Length, LengthDisplay, Locale, VolumeDisplay, format_decimal, format_level,
};

use crate::actions::{Action, Window};
use crate::icons::{self, Icon};
use crate::names::{self, tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets;

pub use kinds::material_options;

/// Which page of the inspector column is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    /// Properties of the selection, or the ΝΟΚ meters.
    #[default]
    Properties,
    /// History of named steps.
    History,
}

/// The inspector column.
#[derive(Default)]
pub struct Inspector {
    /// Page shown.
    pub page: Page,
}

/// The ΝΟΚ report the inspector shows, with whether a newer one is being computed.
pub struct NokView<'a> {
    /// Last finished report.
    pub report: Option<&'a NokReport>,
    /// A newer report is being computed.
    pub updating: bool,
}

impl Inspector {
    /// Shows the inspector.
    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState, nok: NokView<'_>, actions: &mut Vec<Action>) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (page, key, icon) in [
                (Page::Properties, "ui.insp.page.properties", Icon::Properties),
                (Page::History, "ui.insp.page.history", Icon::History),
            ] {
                let selected = self.page == page;
                let text = RichText::new(tr(locale, key)).color(if selected { t.text } else { t.text_muted });
                let button = egui::Button::new(text).selected(selected).frame_when_inactive(false);
                let r = ui.add(button);
                let _ = icon;
                if r.clicked() {
                    self.page = page;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::button(ui, Icon::ChevronRight, false, 20.0, &tr(locale, "ui.panel.collapse"))
                    .clicked()
                {
                    actions.push(Action::TogglePanel(crate::actions::Panel::Inspector));
                }
            });
        });
        widgets::rule(ui);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(4.0);
            match self.page {
                Page::Properties => {
                    if state.selection.is_empty() {
                        nok_panel(ui, state, &nok, actions);
                        ui.add_space(10.0);
                        storey_panel(ui, state, actions);
                    } else {
                        selection_panel(ui, state, actions);
                    }
                }
                Page::History => history_panel(ui, state, actions),
            }
        });
    }
}

fn heading(ui: &mut Ui, text: &str) {
    let t = Tokens::of(ui);
    ui.label(
        RichText::new(text).family(crate::fonts::strong()).size(crate::theme::size::HEADING).color(t.text),
    );
}

/// Selected elements grouped by category, with counts.
fn counts(elements: &[Element]) -> BTreeMap<Category, usize> {
    let mut m = BTreeMap::new();
    for e in elements {
        *m.entry(e.category()).or_insert(0) += 1;
    }
    m
}

fn selection_panel(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let doc = state.doc().clone();
    let elements: Vec<Element> = state.selection.iter().filter_map(|id| doc.element(*id).cloned()).collect();
    if elements.is_empty() {
        return;
    }
    let by_category = counts(&elements);
    // Header.
    ui.horizontal(|ui| {
        if let [only] = elements.as_slice() {
            icons::inline(ui, category_icon(only.category()), t.accent);
            heading(ui, &names::category(locale, only.category()));
            let id_text = if only.tag.is_empty() {
                only.id.to_string()
            } else {
                format!("{} · {}", only.tag, only.id)
            };
            ui.label(RichText::new(id_text).color(t.text_muted));
        } else {
            icons::inline(ui, Icon::Properties, t.accent);
            heading(ui, &trf(locale, "ui.insp.n_elements", &[("n", &elements.len().to_string())]));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Close, false, 22.0, &tr(locale, "ui.insp.deselect")).clicked() {
                actions.push(Action::SelectNone);
            }
        });
    });
    if by_category.len() > 1 || elements.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            for (cat, n) in &by_category {
                let text = format!("{} {n}", names::category_plural(locale, *cat));
                let r = ui
                    .add(egui::Button::new(RichText::new(text).small()).corner_radius(10.0))
                    .on_hover_text(tr(locale, "ui.insp.narrow_hint"));
                if r.clicked() {
                    state.selection =
                        elements.iter().filter(|e| e.category() == *cat).map(|e| e.id).collect();
                }
            }
        });
    }
    ui.add_space(6.0);

    let mut commands: Vec<AnyCommand> = Vec::new();
    if by_category.len() == 1 || same_type(&elements) {
        commands.extend(type_sections(ui, &doc, &elements, locale));
    } else {
        widgets::hint(ui, &tr(locale, "ui.insp.mixed_types"));
    }
    commands.extend(common_section(ui, state, &elements));
    if let [only] = elements.as_slice() {
        quantities_section(ui, &doc, only, locale);
    }
    if !commands.is_empty() {
        let label = if elements.len() == 1 {
            tr(locale, "ui.insp.change_one")
        } else {
            trf(locale, "ui.insp.change_n", &[("n", &elements.len().to_string())])
        };
        // Several element updates (or a common field and a type field) become one undo step.
        let merged = merge_updates(commands);
        let _ = state.run_all(merged, label);
    }
}

/// Whether every element has the same type (e.g. doors and windows are both openings).
fn same_type(elements: &[Element]) -> bool {
    let mut it = elements.iter().map(|e| e.kind.type_name());
    match it.next() {
        Some(first) => it.all(|n| n == first),
        None => true,
    }
}

/// Merges updates of the same element so that one command carries all changed fields.
fn merge_updates(commands: Vec<AnyCommand>) -> Vec<AnyCommand> {
    let mut merged: Vec<UpdateElement> = Vec::new();
    let mut other: Vec<AnyCommand> = Vec::new();
    for c in commands {
        match c {
            AnyCommand::UpdateElement(u) => match merged.iter_mut().find(|m| m.id == u.id) {
                Some(m) => {
                    if u.kind.is_some() {
                        m.kind = u.kind;
                    }
                    if u.storey.is_some() {
                        m.storey = u.storey;
                    }
                    if u.tag.is_some() {
                        m.tag = u.tag;
                    }
                    if u.phase.is_some() {
                        m.phase = u.phase;
                    }
                    if u.properties.is_some() {
                        m.properties = u.properties;
                    }
                }
                None => merged.push(u),
            },
            c => other.push(c),
        }
    }
    merged.into_iter().map(AnyCommand::from).chain(other).collect()
}

macro_rules! collect_kind {
    ($elements:expr, $variant:ident) => {
        $elements
            .iter()
            .filter_map(|e| match &e.kind {
                ElementKind::$variant(x) => Some((e.id, x.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
}

fn type_sections(
    ui: &mut Ui,
    doc: &kanavos_model::Document,
    elements: &[Element],
    locale: Locale,
) -> Vec<AnyCommand> {
    let Some(first) = elements.first() else { return Vec::new() };
    match &first.kind {
        ElementKind::Wall(_) => kinds::walls(ui, doc, collect_kind!(elements, Wall), locale),
        ElementKind::Opening(_) => kinds::openings(ui, doc, collect_kind!(elements, Opening), locale),
        ElementKind::Slab(_) => kinds::slabs(ui, doc, collect_kind!(elements, Slab), locale),
        ElementKind::Column(_) => kinds::columns(ui, doc, collect_kind!(elements, Column), locale),
        ElementKind::Beam(_) => kinds::beams(ui, doc, collect_kind!(elements, Beam), locale),
        ElementKind::Stair(_) => kinds::stairs(ui, doc, collect_kind!(elements, Stair), locale),
        ElementKind::Roof(_) => kinds::roofs(ui, doc, collect_kind!(elements, Roof), locale),
        ElementKind::Railing(_) => kinds::railings(ui, doc, collect_kind!(elements, Railing), locale),
        ElementKind::Space(_) => kinds::spaces(ui, doc, collect_kind!(elements, Space), locale),
        ElementKind::Object(_) => kinds::objects(ui, doc, collect_kind!(elements, Object), locale),
        ElementKind::Text(_) => kinds::texts(ui, doc, collect_kind!(elements, Text), locale),
        ElementKind::Dimension(_) => kinds::dimensions(ui, doc, collect_kind!(elements, Dimension), locale),
        ElementKind::Level(_) => kinds::levels(ui, doc, collect_kind!(elements, Level), locale),
        ElementKind::Drafting(_) => kinds::drafting(ui, doc, collect_kind!(elements, Drafting), locale),
        ElementKind::Grid(_) => kinds::grids(ui, doc, collect_kind!(elements, Grid), locale),
        ElementKind::Underlay(_) => kinds::underlays(ui, doc, collect_kind!(elements, Underlay), locale),
    }
}

/// Tag, phase and storey, shared by every element type.
fn common_section(ui: &mut Ui, state: &AppState, elements: &[Element]) -> Vec<AnyCommand> {
    let locale = state.locale;
    let doc = state.doc();
    let mut out = Vec::new();
    let shared = |get: &dyn Fn(&Element) -> String| -> Option<String> {
        let mut it = elements.iter().map(get);
        let first = it.next()?;
        if it.all(|v| v == first) { Some(first) } else { None }
    };
    widgets::section(ui, "insp-common", &tr(locale, "ui.insp.group.identity"), true, |ui| {
        widgets::grid(ui, "common", |ui| {
            widgets::label(ui, &tr(locale, "ui.insp.tag"));
            let tag = shared(&|e| e.tag.clone());
            if let Some(v) = widgets::text_field(ui, "common-tag", tag.as_deref(), 170.0) {
                for e in elements {
                    out.push(
                        UpdateElement { id: e.id, tag: Some(v.clone()), ..UpdateElement::default() }.into(),
                    );
                }
            }
            ui.end_row();
            widgets::label(ui, &tr(locale, "ui.insp.phase"));
            let phases: Vec<(Phase, String)> =
                names::PHASES.into_iter().map(|p| (p, names::phase(locale, p))).collect();
            let phase = {
                let mut it = elements.iter().map(|e| e.phase);
                let first = it.next();
                first.filter(|f| it.all(|p| p == *f))
            };
            if let Some(p) = widgets::choice(ui, "common-phase", phase.as_ref(), &phases, 170.0) {
                for e in elements {
                    out.push(UpdateElement { id: e.id, phase: Some(p), ..UpdateElement::default() }.into());
                }
            }
            ui.end_row();
            // Openings follow their host wall's storey; everything else with a storey can move.
            let movable =
                elements.iter().all(|e| e.storey.is_some() && !matches!(e.kind, ElementKind::Opening(_)));
            if movable {
                widgets::label(ui, &tr(locale, "ui.insp.storey"));
                let storeys: Vec<(Id, String)> =
                    doc.storeys().iter().rev().map(|s| (s.id, s.name.clone())).collect();
                let storey = {
                    let mut it = elements.iter().map(|e| e.storey);
                    let first = it.next().flatten();
                    first.filter(|f| it.all(|s| s == Some(*f)))
                };
                if let Some(s) = widgets::choice(ui, "common-storey", storey.as_ref(), &storeys, 170.0) {
                    for e in elements {
                        out.push(
                            UpdateElement { id: e.id, storey: Some(s), ..UpdateElement::default() }.into(),
                        );
                    }
                }
                ui.end_row();
            }
        });
    });
    out
}

fn quantities_section(ui: &mut Ui, doc: &kanavos_model::Document, e: &Element, locale: Locale) {
    let Ok(q) = kanavos_elements::quantities(doc, e.id) else { return };
    let rows: Vec<(String, String)> = [
        q.length.map(|v| (tr(locale, "ui.insp.length"), LengthDisplay(v, locale).to_string())),
        q.height.map(|v| (tr(locale, "ui.insp.height"), LengthDisplay(v, locale).to_string())),
        q.thickness.map(|v| (tr(locale, "ui.insp.thickness"), LengthDisplay(v, locale).to_string())),
        q.area.map(|v| (tr(locale, "ui.insp.net_area"), AreaDisplay(v, locale).to_string())),
        q.gross_area.map(|v| (tr(locale, "ui.insp.gross_area"), AreaDisplay(v, locale).to_string())),
        q.perimeter.map(|v| (tr(locale, "ui.insp.perimeter"), LengthDisplay(v, locale).to_string())),
        q.volume.map(|v| (tr(locale, "ui.insp.volume"), VolumeDisplay(v, locale).to_string())),
    ]
    .into_iter()
    .flatten()
    .collect();
    if rows.is_empty() {
        return;
    }
    widgets::section(ui, "insp-quantities", &tr(locale, "ui.insp.group.quantities"), false, |ui| {
        widgets::grid(ui, "quantities", |ui| {
            for (k, v) in rows {
                widgets::label(ui, &k);
                widgets::value(ui, &v);
                ui.end_row();
            }
        });
    });
}

/// Icon of an element category.
pub fn category_icon(c: Category) -> Icon {
    use kanavos_tools::ToolId;
    Icon::Tool(match c {
        Category::Wall => ToolId::Wall,
        Category::Slab => ToolId::Slab,
        Category::Column => ToolId::Column,
        Category::Beam => ToolId::Beam,
        Category::Door => ToolId::Door,
        Category::Window => ToolId::Window,
        Category::Opening => ToolId::Void,
        Category::Stair => ToolId::Stair,
        Category::Roof => ToolId::Roof,
        Category::Railing => ToolId::Railing,
        Category::Space => ToolId::Space,
        Category::Object => ToolId::Object,
        Category::Dimension => ToolId::Dimension,
        Category::Text => ToolId::Text,
        Category::Level => ToolId::Level,
        Category::Drafting => ToolId::Line,
        Category::Grid => ToolId::Grid,
        Category::Underlay => ToolId::Plot,
    })
}

/// Status colour of a used/allowed ratio: within limits, close to the limit (over 90 %), or over it.
pub fn meter_color(t: &Tokens, ratio: f64) -> egui::Color32 {
    if !ratio.is_finite() || ratio > 1.0 + 1e-9 {
        t.error
    } else if ratio > 0.9 {
        t.warning
    } else {
        t.ok
    }
}

/// One meter: label, value text, bar and margin line.
fn meter(ui: &mut Ui, label: &str, value: &str, ratio: f64, margin: &str, over: bool) {
    let t = Tokens::of(ui);
    let color = meter_color(t, ratio);
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).color(t.text));
        });
    });
    widgets::meter_bar(ui, ratio, color, &format!("{label}: {value}"));
    ui.horizontal(|ui| {
        let pct = if ratio.is_finite() {
            format!("{} %", format_decimal(ratio * 100.0, 0, Locale::En))
        } else {
            "—".into()
        };
        ui.label(RichText::new(pct).small().color(t.text_muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(margin).small().color(if over { t.error } else { t.text_muted }));
        });
    });
    ui.add_space(6.0);
}

fn nok_panel(ui: &mut Ui, state: &AppState, nok: &NokView<'_>, actions: &mut Vec<Action>) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let doc = state.doc();
    let terms = &doc.site().terms;
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Coverage, t.accent);
        heading(ui, &tr(locale, "ui.nok.title"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Gear, false, 22.0, &tr(locale, "ui.nok.edit_terms")).clicked() {
                actions.push(Action::OpenWindow(Window::Site));
            }
            if nok.updating {
                ui.add(egui::Spinner::new().size(12.0));
            }
        });
    });
    let terms_text = trf(
        locale,
        "ui.nok.terms",
        &[
            ("sd", &format_decimal(terms.building_coefficient, 2, locale)),
            ("k", &format_decimal(terms.coverage_ratio * 100.0, 0, locale)),
            ("h", &format_decimal(terms.max_height.as_m(), 2, locale)),
        ],
    );
    ui.label(RichText::new(terms_text).small().color(t.text_muted));
    ui.add_space(6.0);
    let Some(report) = nok.report else {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(14.0));
            ui.label(RichText::new(tr(locale, "ui.nok.computing")).color(t.text_muted));
        });
        return;
    };
    match report.plot_area {
        Some(a) => {
            ui.label(
                RichText::new(trf(locale, "ui.nok.plot_area", &[("a", &AreaDisplay(a, locale).to_string())]))
                    .color(t.text),
            );
        }
        None => {
            egui::Frame::new().fill(t.warning_weak).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    icons::inline(ui, Icon::Warning, t.warning);
                    ui.label(RichText::new(tr(locale, "rules.note.no_plot")).color(t.text));
                });
                ui.horizontal(|ui| {
                    if ui.button(tr(locale, "ui.nok.draw_plot")).clicked() {
                        actions.push(Action::Tool(kanavos_tools::ToolId::Plot));
                    }
                    if ui.button(tr(locale, "ui.nok.edit_terms")).clicked() {
                        actions.push(Action::OpenWindow(Window::Site));
                    }
                });
            });
        }
    }
    ui.add_space(8.0);
    let m = &report.meters;
    let margin_text = |margin: kanavos_units::Area| {
        if margin.as_m2() >= 0.0 {
            trf(locale, "ui.nok.margin", &[("a", &AreaDisplay(margin, locale).to_string())])
        } else {
            trf(locale, "ui.nok.over", &[("a", &AreaDisplay(-margin, locale).to_string())])
        }
    };
    meter(
        ui,
        &tr(locale, "ui.nok.coverage"),
        &m.coverage.label(locale),
        m.coverage.ratio(),
        &margin_text(m.coverage.margin()),
        !m.coverage.within_limit(),
    );
    meter(
        ui,
        &tr(locale, "ui.nok.floor_area"),
        &m.floor_area.label(locale),
        m.floor_area.ratio(),
        &margin_text(m.floor_area.margin()),
        !m.floor_area.within_limit(),
    );
    let (used_h, max_h) = report.height;
    let h_ratio = if max_h.as_m() > 0.0 { used_h.as_m() / max_h.as_m() } else { f64::INFINITY };
    let h_margin = max_h - used_h;
    let h_margin_text = if h_margin.as_m() >= 0.0 {
        trf(locale, "ui.nok.margin_len", &[("l", &LengthDisplay(h_margin, locale).to_string())])
    } else {
        trf(locale, "ui.nok.over_len", &[("l", &LengthDisplay(-h_margin, locale).to_string())])
    };
    let h_value = format!("{} / {}", format_decimal(used_h.as_m(), 2, locale), LengthDisplay(max_h, locale));
    meter(ui, &tr(locale, "ui.nok.height"), &h_value, h_ratio, &h_margin_text, h_margin.as_m() < 0.0);
    if let Some(max_floors) = terms.max_floors {
        let floors = doc.storeys().iter().filter(|s| !s.below_ground).count();
        let ratio = if max_floors > 0 { floors as f64 / f64::from(max_floors) } else { f64::INFINITY };
        let value = format!("{floors} / {max_floors}");
        let over = floors as u32 > max_floors;
        let text = if over { tr(locale, "ui.nok.floors_over") } else { tr(locale, "ui.nok.floors_ok") };
        meter(ui, &tr(locale, "ui.nok.floors"), &value, ratio, &text, over);
    }
    if terms.volume_coefficient.is_some() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(locale, "ui.nok.volume")).color(t.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(tr(locale, "ui.nok.volume_pending")).small().italics().color(t.text_muted),
                );
            });
        });
        ui.add_space(6.0);
    }
    for note in report.notes.iter().filter(|n| n.as_str() != "rules.note.no_plot") {
        ui.horizontal_wrapped(|ui| {
            icons::inline(ui, Icon::Info, t.text_muted);
            ui.label(RichText::new(tr(locale, note)).small().color(t.text_muted));
        });
    }
    // Areas per storey.
    widgets::section(ui, "nok-storeys", &tr(locale, "ui.nok.per_storey"), false, |ui| {
        egui::Grid::new("nok-storey-grid").num_columns(3).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new(tr(locale, "rules.col.storey")).small().color(t.text_muted));
            ui.label(RichText::new(tr(locale, "ui.nok.counted")).small().color(t.text_muted));
            ui.label(RichText::new(tr(locale, "ui.nok.exempt")).small().color(t.text_muted));
            ui.end_row();
            for s in report.storeys.iter().rev() {
                ui.label(&s.name);
                ui.label(AreaDisplay(s.counted, locale).to_string());
                ui.label(AreaDisplay(s.exempt, locale).to_string());
                ui.end_row();
            }
        });
        if ui.button(tr(locale, "ui.nok.open_area_table")).clicked() {
            actions.push(Action::OpenTab(crate::actions::TabTarget::AreaTable));
        }
    });
}

fn storey_panel(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let doc = state.doc();
    let Some(s) = doc.storey(state.active_storey) else { return };
    widgets::rule(ui);
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Storey, t.accent);
        heading(ui, &s.name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::button(ui, Icon::Gear, false, 22.0, &tr(locale, "ui.window.storeys")).clicked() {
                actions.push(Action::OpenWindow(Window::Storeys));
            }
        });
    });
    let n = doc.elements_on(s.id).count();
    widgets::grid(ui, "storey-info", |ui| {
        widgets::label(ui, &tr(locale, "ui.storey.elevation"));
        widgets::value(ui, &format_level(s.elevation, locale));
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.storey.height"));
        widgets::value(ui, &LengthDisplay(s.height, locale).to_string());
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.storey.cut"));
        widgets::value(ui, &LengthDisplay(s.cut_height, locale).to_string());
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.storey.elements"));
        widgets::value(ui, &n.to_string());
        ui.end_row();
    });
    if n == 0 {
        ui.add_space(8.0);
        egui::Frame::new().fill(t.accent_weak).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
            ui.label(RichText::new(tr(locale, "ui.empty.storey")).color(t.text));
        });
    } else {
        ui.add_space(4.0);
        widgets::hint(ui, &tr(locale, "ui.insp.select_hint"));
    }
    let _ = Length::ZERO;
}

fn history_panel(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let past = state.session.history_labels(locale);
    let future = state.redo_labels();
    let current = past.len();
    let start = tr(locale, "ui.history.start");
    if crate::tree::row(ui, Some(Icon::New), &start, "", current == 0, 0.0).response.clicked() && current != 0
    {
        actions.push(Action::UndoTo(0));
    }
    for (i, label) in past.iter().enumerate() {
        let step = i + 1;
        let r = crate::tree::row(ui, None, &format!("{step}. {label}"), "", step == current, 8.0).response;
        if r.clicked() && step != current {
            actions.push(Action::UndoTo(step));
        }
    }
    if !future.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(tr(locale, "ui.history.redo")).small().color(t.text_faint));
        for (j, label) in future.iter().enumerate() {
            let text = RichText::new(format!("{}. {label}", current + j + 1)).italics().color(t.text_faint);
            let r = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
            if r.on_hover_text(tr(locale, "ui.history.redo_hint")).clicked() {
                actions.push(Action::RedoSteps(j + 1));
            }
        }
    }
    if past.is_empty() && future.is_empty() {
        ui.add_space(8.0);
        widgets::hint(ui, &tr(locale, "ui.history.empty"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_colours_follow_the_limits() {
        let t = &crate::theme::LIGHT;
        assert_eq!(meter_color(t, 0.5), t.ok);
        assert_eq!(meter_color(t, 0.95), t.warning);
        assert_eq!(meter_color(t, 1.2), t.error);
        assert_eq!(meter_color(t, f64::INFINITY), t.error);
    }

    #[test]
    fn updates_of_one_element_are_merged() {
        let a = UpdateElement { id: Id(1), tag: Some("Π1".into()), ..UpdateElement::default() };
        let b = UpdateElement { id: Id(1), phase: Some(Phase::Existing), ..UpdateElement::default() };
        let c = UpdateElement { id: Id(2), tag: Some("Π2".into()), ..UpdateElement::default() };
        let merged = merge_updates(vec![a.into(), b.into(), c.into()]);
        assert_eq!(merged.len(), 2);
        let AnyCommand::UpdateElement(first) = &merged[0] else { panic!("not an update") };
        assert_eq!(first.tag.as_deref(), Some("Π1"));
        assert_eq!(first.phase, Some(Phase::Existing));
    }
}
