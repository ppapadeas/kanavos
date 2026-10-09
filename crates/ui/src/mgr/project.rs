//! Project information (title block), site and building terms, and element defaults.

use egui::{RichText, Ui};
use kanavos_engine::commands::{SetDefaults, SetProjectInfo, SetSite};
use kanavos_model::{BuildUpUsage, Defaults, Document, Id};
use kanavos_tools::ToolId;
use kanavos_units::{Angle, AreaDisplay};

use crate::actions::Action;
use crate::icons::{self, Icon};
use crate::names::{tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;
use crate::widgets;

/// Project information: the fields printed in title blocks.
pub fn project_ui(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;
    let info = state.doc().info().clone();
    let mut next = info.clone();
    widgets::hint(ui, &tr(locale, "ui.mgr.project_hint"));
    ui.add_space(6.0);
    widgets::grid(ui, "project-info", |ui| {
        let fields: [(&str, &str, String); 8] = [
            ("ui.project.name", "name", info.name.clone()),
            ("ui.project.location", "location", info.location.clone()),
            ("ui.project.municipality", "municipality", info.municipality.clone()),
            ("ui.project.owner", "owner", info.owner.clone()),
            ("ui.project.designer", "designer", info.designer.clone()),
            ("ui.project.designer_details", "designer_details", info.designer_details.clone()),
            ("ui.project.date", "date", info.date.clone()),
            ("ui.project.revision", "revision", info.revision.clone()),
        ];
        for (label, key, value) in fields {
            widgets::label(ui, &tr(locale, label));
            if let Some(v) = widgets::text_field(ui, ("project", key), Some(&value), 260.0) {
                match key {
                    "name" => next.name = v,
                    "location" => next.location = v,
                    "municipality" => next.municipality = v,
                    "owner" => next.owner = v,
                    "designer" => next.designer = v,
                    "designer_details" => next.designer_details = v,
                    "date" => next.date = v,
                    _ => next.revision = v,
                }
            }
            ui.end_row();
        }
        widgets::label(ui, &tr(locale, "ui.project.notes"));
        if let Some(v) = widgets::multiline_field(ui, "project-notes", Some(&info.notes), 260.0) {
            next.notes = v;
        }
        ui.end_row();
    });
    if next != info {
        let _ = state.run(SetProjectInfo { info: next }.into(), false);
    }
}

/// Site and building terms: plot, ΝΟΚ terms, georeferencing.
pub fn site_ui(ui: &mut Ui, state: &mut AppState, actions: &mut Vec<Action>) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let site = state.doc().site().clone();
    let mut next = site.clone();
    egui::ScrollArea::vertical().id_salt("site-scroll").show(ui, |ui| {
        widgets::section(ui, "site-plot", &tr(locale, "ui.site.plot"), true, |ui| {
            widgets::grid(ui, "site-plot-grid", |ui| {
                widgets::label(ui, &tr(locale, "ui.site.plot_area"));
                match &site.plot {
                    Some(p) => {
                        widgets::value(
                            ui,
                            &trf(
                                locale,
                                "ui.site.plot_summary",
                                &[
                                    ("a", &AreaDisplay(p.area(), locale).to_string()),
                                    ("n", &p.len().to_string()),
                                ],
                            ),
                        );
                    }
                    None => {
                        ui.label(RichText::new(tr(locale, "ui.tree.no_plot")).color(t.warning));
                    }
                }
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button(tr(locale, "ui.nok.draw_plot")).clicked() {
                    actions.push(Action::Tool(ToolId::Plot));
                }
                if site.plot.is_some() && ui.button(tr(locale, "ui.site.clear_plot")).clicked() {
                    next.plot = None;
                }
            });
        });
        widgets::section(ui, "site-terms", &tr(locale, "ui.site.terms"), true, |ui| {
            widgets::grid(ui, "site-terms-grid", |ui| {
                widgets::label(ui, &tr(locale, "ui.site.coefficient"));
                if let Some(v) =
                    widgets::number_field(ui, "site-sd", Some(site.terms.building_coefficient), 2, "", locale)
                {
                    next.terms.building_coefficient = v.max(0.0);
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.coverage"));
                if let Some(v) = widgets::number_field(
                    ui,
                    "site-k",
                    Some(site.terms.coverage_ratio * 100.0),
                    0,
                    "%",
                    locale,
                ) {
                    next.terms.coverage_ratio = (v / 100.0).clamp(0.0, 1.0);
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.max_height"));
                if let Some(v) = widgets::length_field(ui, "site-h", Some(site.terms.max_height), locale) {
                    next.terms.max_height = v;
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.max_floors"));
                ui.horizontal(|ui| {
                    let floors = site.terms.max_floors.map(f64::from);
                    if let Some(v) = widgets::number_field(ui, "site-floors", floors, 0, "", locale) {
                        next.terms.max_floors = Some(v.round().clamp(0.0, 200.0) as u32);
                    }
                    if site.terms.max_floors.is_some()
                        && icons::button(ui, Icon::Close, false, 18.0, &tr(locale, "ui.mgr.clear")).clicked()
                    {
                        next.terms.max_floors = None;
                    }
                });
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.volume_coefficient"));
                ui.horizontal(|ui| {
                    if let Some(v) =
                        widgets::number_field(ui, "site-vc", site.terms.volume_coefficient, 2, "", locale)
                    {
                        next.terms.volume_coefficient = Some(v.max(0.0));
                    }
                    if site.terms.volume_coefficient.is_some()
                        && icons::button(ui, Icon::Close, false, 18.0, &tr(locale, "ui.mgr.clear")).clicked()
                    {
                        next.terms.volume_coefficient = None;
                    }
                });
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.setback"));
                ui.horizontal(|ui| {
                    if let Some(v) = widgets::length_field(ui, "site-d", site.terms.setback, locale) {
                        next.terms.setback = Some(v);
                    }
                    if site.terms.setback.is_some() {
                        if icons::button(ui, Icon::Close, false, 18.0, &tr(locale, "ui.mgr.clear")).clicked()
                        {
                            next.terms.setback = None;
                        }
                    } else {
                        ui.label(
                            RichText::new(tr(locale, "ui.site.setback_rule")).small().color(t.text_faint),
                        );
                    }
                });
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.ground_level"));
                if let Some(v) = widgets::length_field(ui, "site-gl", Some(site.ground_level), locale) {
                    next.ground_level = v;
                }
                ui.end_row();
            });
            if let Some(p) = &site.plot {
                let a = p.area().as_m2();
                let text = trf(
                    locale,
                    "ui.site.limits",
                    &[
                        ("c", &widgets::format_number(a * site.terms.coverage_ratio, 2, locale)),
                        ("f", &widgets::format_number(a * site.terms.building_coefficient, 2, locale)),
                    ],
                );
                widgets::hint(ui, &text);
            }
        });
        widgets::section(ui, "site-georef", &tr(locale, "ui.site.georef"), true, |ui| {
            widgets::grid(ui, "site-geo-grid", |ui| {
                widgets::label(ui, &tr(locale, "ui.site.crs"));
                widgets::value(ui, &format!("EPSG:{} · ΕΓΣΑ87", site.georef.epsg));
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.easting"));
                if let Some(v) =
                    widgets::number_field(ui, "site-e", Some(site.georef.easting), 2, "m", locale)
                {
                    next.georef.easting = v;
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.northing"));
                if let Some(v) =
                    widgets::number_field(ui, "site-n", Some(site.georef.northing), 2, "m", locale)
                {
                    next.georef.northing = v;
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.rotation"));
                if let Some(v) =
                    widgets::number_field(ui, "site-rot", Some(site.georef.rotation.as_deg()), 3, "°", locale)
                {
                    next.georef.rotation = Angle::deg(v);
                }
                ui.end_row();
                widgets::label(ui, &tr(locale, "ui.site.altitude"));
                if let Some(v) =
                    widgets::number_field(ui, "site-alt", Some(site.georef.altitude), 2, "m", locale)
                {
                    next.georef.altitude = v;
                }
                ui.end_row();
            });
            widgets::hint(ui, &tr(locale, "ui.site.georef_hint"));
        });
    });
    if next != site {
        let _ = state.run(SetSite { site: next }.into(), false);
    }
}

fn build_up_default(
    ui: &mut Ui,
    doc: &Document,
    id: &str,
    usage: BuildUpUsage,
    value: Option<Id>,
) -> Option<Option<Id>> {
    let mut options: Vec<(Option<Id>, String)> = vec![(None, "—".into())];
    options.extend(
        doc.build_ups()
            .filter(|b| b.usage == usage || b.usage == BuildUpUsage::Any)
            .map(|b| (Some(b.id), b.name.clone())),
    );
    widgets::choice(ui, id, Some(&value), &options, 260.0)
}

/// Default build-ups and materials used by the tools.
pub fn defaults_ui(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;
    let doc = state.doc().clone();
    let d = doc.defaults().clone();
    let mut next: Defaults = d.clone();
    widgets::hint(ui, &tr(locale, "ui.mgr.defaults_hint"));
    ui.add_space(6.0);
    widgets::grid(ui, "defaults-grid", |ui| {
        let rows: [(&str, BuildUpUsage, Option<Id>); 6] = [
            ("ui.defaults.exterior_wall", BuildUpUsage::Wall, d.exterior_wall),
            ("ui.defaults.interior_wall", BuildUpUsage::Wall, d.interior_wall),
            ("ui.defaults.floor_slab", BuildUpUsage::Slab, d.floor_slab),
            ("ui.defaults.ground_slab", BuildUpUsage::Slab, d.ground_slab),
            ("ui.defaults.flat_roof", BuildUpUsage::Roof, d.flat_roof),
            ("ui.defaults.pitched_roof", BuildUpUsage::Roof, d.pitched_roof),
        ];
        for (key, usage, value) in rows {
            widgets::label(ui, &tr(locale, key));
            if let Some(v) = build_up_default(ui, &doc, key, usage, value) {
                match key {
                    "ui.defaults.exterior_wall" => next.exterior_wall = v,
                    "ui.defaults.interior_wall" => next.interior_wall = v,
                    "ui.defaults.floor_slab" => next.floor_slab = v,
                    "ui.defaults.ground_slab" => next.ground_slab = v,
                    "ui.defaults.flat_roof" => next.flat_roof = v,
                    _ => next.pitched_roof = v,
                }
            }
            ui.end_row();
        }
        let mut mats: Vec<(Option<Id>, String)> = vec![(None, "—".into())];
        mats.extend(doc.materials().map(|m| (Some(m.id), m.name.clone())));
        widgets::label(ui, &tr(locale, "ui.defaults.structural_material"));
        if let Some(v) = widgets::choice(ui, "def-struct", Some(&d.structural_material), &mats, 260.0) {
            next.structural_material = v;
        }
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.defaults.railing_material"));
        if let Some(v) = widgets::choice(ui, "def-rail", Some(&d.railing_material), &mats, 260.0) {
            next.railing_material = v;
        }
        ui.end_row();
    });
    if next != d {
        let _ = state.run(SetDefaults { defaults: next }.into(), false);
    }
}
