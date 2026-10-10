//! The menu bar: File, Edit, View, Model, Help.
//!
//! Menu items push [`Action`]s; the shell performs them. Labels come from the catalogue and show their
//! shortcuts; Undo and Redo name the step they act on.

use std::path::PathBuf;

use egui::{Button, RichText, Ui};
use kanavos_units::Locale;
use kanavos_update::Channel;

use crate::actions::{Action, Panel, Window};
use crate::exports::ExportKind;
use crate::imports::ImportKind;
use crate::names::{tr, trf};
use crate::state::AppState;
use crate::theme::{Theme, Tokens};

/// What the menu needs besides the application state.
pub struct MenuContext<'a> {
    /// Recent files.
    pub recent: &'a [PathBuf],
    /// Current theme.
    pub theme: Theme,
    /// Current UI scale.
    pub ui_scale: f32,
    /// Project tree shown.
    pub tree: bool,
    /// Inspector shown.
    pub inspector: bool,
    /// Split view on.
    pub split: bool,
    /// The focused tab shows a table (CSV export applies).
    pub table_focused: bool,
    /// Check for updates at start.
    pub update_check: bool,
    /// Which releases are offered.
    pub update_channel: Channel,
    /// The version on offer, when a check found one.
    pub update_available: Option<String>,
}

fn item(ui: &mut Ui, label: String, shortcut: &str, enabled: bool) -> bool {
    let mut b = Button::new(label);
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    let clicked = ui.add_enabled(enabled, b).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn check(ui: &mut Ui, label: String, on: bool, shortcut: &str) -> bool {
    let mut b = Button::new(label).selected(on);
    if !shortcut.is_empty() {
        b = b.shortcut_text(shortcut);
    }
    let clicked = ui.add(b).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Shows the menu bar.
pub fn bar(ui: &mut Ui, state: &AppState, mc: &MenuContext<'_>, actions: &mut Vec<Action>) {
    let locale = state.locale;
    egui::MenuBar::new().ui(ui, |ui| {
        file_menu(ui, state, mc, actions);
        edit_menu(ui, state, actions);
        view_menu(ui, locale, mc, actions);
        model_menu(ui, locale, actions);
        help_menu(ui, locale, mc, actions);
        // Project name and unsaved marker on the right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let t = Tokens::of(ui);
            let dirty = state.is_dirty();
            let name = state.title();
            let text = if dirty { format!("{name}  •") } else { name };
            let r = ui.label(RichText::new(text).color(t.text_muted));
            if dirty {
                r.on_hover_text(tr(locale, "ui.status.unsaved"));
            }
        });
    });
}

fn file_menu(ui: &mut Ui, state: &AppState, mc: &MenuContext<'_>, actions: &mut Vec<Action>) {
    let locale = state.locale;
    ui.menu_button(tr(locale, "ui.menu.file"), |ui| {
        if item(ui, tr(locale, "ui.menu.new"), "Ctrl+N", true) {
            actions.push(Action::New);
        }
        if item(ui, tr(locale, "ui.menu.open"), "Ctrl+O", true) {
            actions.push(Action::Open);
        }
        ui.menu_button(tr(locale, "ui.menu.recent"), |ui| {
            if mc.recent.is_empty() {
                ui.add_enabled(false, Button::new(tr(locale, "ui.menu.recent_none")));
            }
            for path in mc.recent {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let r = ui.add(Button::new(name)).on_hover_text(path.display().to_string());
                if r.clicked() {
                    actions.push(Action::OpenPath(path.clone()));
                    ui.close();
                }
            }
        });
        if item(ui, tr(locale, "ui.menu.open_sample"), "", true) {
            actions.push(Action::OpenSample);
        }
        ui.separator();
        if item(ui, tr(locale, "ui.menu.save"), "Ctrl+S", true) {
            actions.push(Action::Save);
        }
        if item(ui, tr(locale, "ui.menu.save_as"), "Ctrl+Shift+S", true) {
            actions.push(Action::SaveAs);
        }
        ui.separator();
        ui.menu_button(tr(locale, "ui.menu.export"), |ui| {
            for k in ExportKind::ALL {
                let enabled = if k == ExportKind::Csv { mc.table_focused } else { true };
                let mut label = tr(locale, k.label_key());
                if !k.available() {
                    label = format!("{label}  ({})", tr(locale, "ui.menu.soon"));
                }
                if item(ui, label, "", enabled) {
                    actions.push(Action::Export(k));
                }
            }
        });
        ui.menu_button(tr(locale, "ui.menu.import"), |ui| {
            for k in ImportKind::ALL {
                let mut label = tr(locale, k.label_key());
                if !k.available() {
                    label = format!("{label}  ({})", tr(locale, "ui.menu.soon"));
                }
                if item(ui, label, "", true) {
                    actions.push(Action::Import(k));
                }
            }
        });
        ui.separator();
        if item(ui, tr(locale, "ui.menu.exit"), "Alt+F4", true) {
            actions.push(Action::Exit);
        }
    });
}

fn edit_menu(ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
    let locale = state.locale;
    ui.menu_button(tr(locale, "ui.menu.edit"), |ui| {
        let undo = match state.undo_label() {
            Some(l) => trf(locale, "ui.menu.undo_named", &[("step", &l)]),
            None => tr(locale, "ui.menu.undo"),
        };
        if item(ui, undo, "Ctrl+Z", state.session.can_undo()) {
            actions.push(Action::Undo);
        }
        let redo = match state.redo_label() {
            Some(l) => trf(locale, "ui.menu.redo_named", &[("step", &l)]),
            None => tr(locale, "ui.menu.redo"),
        };
        if item(ui, redo, "Ctrl+Y", state.session.can_redo()) {
            actions.push(Action::Redo);
        }
        ui.separator();
        if item(ui, tr(locale, "ui.menu.delete"), "Del", !state.selection.is_empty()) {
            actions.push(Action::Delete);
        }
        if item(ui, tr(locale, "ui.menu.select_all"), "Ctrl+A", true) {
            actions.push(Action::SelectAll);
        }
        if item(ui, tr(locale, "ui.menu.select_none"), "Esc", !state.selection.is_empty()) {
            actions.push(Action::SelectNone);
        }
        ui.separator();
        let doc = state.doc();
        ui.add_enabled_ui(!state.selection.is_empty(), |ui| {
            ui.menu_button(tr(locale, "ui.menu.copy_to_storey"), |ui| {
                for s in doc.storeys().into_iter().rev() {
                    if s.id == state.active_storey {
                        continue;
                    }
                    if item(ui, s.name.clone(), "", true) {
                        actions.push(Action::CopyToStorey(s.id));
                    }
                }
            });
        });
    });
}

fn view_menu(ui: &mut Ui, locale: Locale, mc: &MenuContext<'_>, actions: &mut Vec<Action>) {
    ui.menu_button(tr(locale, "ui.menu.view"), |ui| {
        if check(ui, tr(locale, "ui.menu.tree"), mc.tree, "") {
            actions.push(Action::TogglePanel(Panel::Tree));
        }
        if check(ui, tr(locale, "ui.menu.inspector"), mc.inspector, "") {
            actions.push(Action::TogglePanel(Panel::Inspector));
        }
        if item(ui, tr(locale, "ui.menu.history"), "", true) {
            actions.push(Action::TogglePanel(Panel::History));
        }
        if item(ui, tr(locale, "ui.menu.toggle_panels"), "Tab", true) {
            actions.push(Action::TogglePanels);
        }
        if check(ui, tr(locale, "ui.menu.split"), mc.split, "") {
            actions.push(Action::ToggleSplit);
        }
        ui.separator();
        if item(ui, tr(locale, "ui.menu.zoom_extents"), "Home", true) {
            actions.push(Action::ZoomExtents);
        }
        ui.separator();
        ui.menu_button(tr(locale, "ui.menu.theme"), |ui| {
            if check(ui, tr(locale, "ui.menu.theme_light"), mc.theme == Theme::Light, "") {
                actions.push(Action::Theme(Theme::Light));
            }
            if check(ui, tr(locale, "ui.menu.theme_dark"), mc.theme == Theme::Dark, "") {
                actions.push(Action::Theme(Theme::Dark));
            }
        });
        ui.menu_button(tr(locale, "ui.menu.language"), |ui| {
            if check(ui, "Ελληνικά".into(), locale == Locale::El, "") {
                actions.push(Action::Language(Locale::El));
            }
            if check(ui, "English".into(), locale == Locale::En, "") {
                actions.push(Action::Language(Locale::En));
            }
        });
        ui.menu_button(tr(locale, "ui.menu.ui_scale"), |ui| {
            for s in [0.9_f32, 1.0, 1.1, 1.25, 1.5] {
                let on = (mc.ui_scale - s).abs() < 0.01;
                if check(ui, format!("{} %", (s * 100.0).round()), on, "") {
                    actions.push(Action::Scale(s));
                }
            }
        });
    });
}

fn model_menu(ui: &mut Ui, locale: Locale, actions: &mut Vec<Action>) {
    ui.menu_button(tr(locale, "ui.menu.model"), |ui| {
        if item(ui, tr(locale, "ui.menu.new_section"), "", true) {
            actions.push(Action::NewSection);
        }
        if item(ui, tr(locale, "ui.menu.new_elevation"), "", true) {
            actions.push(Action::NewElevation);
        }
        if item(ui, tr(locale, "ui.menu.new_sheet"), "", true) {
            actions.push(Action::NewSheet);
        }
        if item(ui, tr(locale, "ui.menu.permit_set"), "", true) {
            actions.push(Action::PermitSet);
        }
        let detect = item(ui, tr(locale, "ui.menu.detect_spaces"), "", true);
        if detect {
            actions.push(Action::DetectSpaces);
        }
        ui.separator();
        for (w, key) in [
            (Window::Materials, "ui.window.materials"),
            (Window::BuildUps, "ui.window.buildups"),
            (Window::Storeys, "ui.window.storeys"),
            (Window::Project, "ui.window.project"),
            (Window::Site, "ui.window.site"),
            (Window::Defaults, "ui.window.defaults"),
        ] {
            if item(ui, format!("{}…", tr(locale, key)), "", true) {
                actions.push(Action::OpenWindow(w));
            }
        }
    });
}

fn help_menu(ui: &mut Ui, locale: Locale, mc: &MenuContext<'_>, actions: &mut Vec<Action>) {
    ui.menu_button(tr(locale, "ui.menu.help"), |ui| {
        if item(ui, tr(locale, "ui.menu.shortcuts"), "F1", true) {
            actions.push(Action::OpenWindow(Window::Shortcuts));
        }
        if item(ui, tr(locale, "ui.palette.button"), "Ctrl+K", true) {
            actions.push(Action::CommandPalette);
        }
        if item(ui, tr(locale, "ui.menu.open_sample"), "", true) {
            actions.push(Action::OpenSample);
        }
        ui.separator();
        if let Some(version) = &mc.update_available
            && item(ui, trf(locale, "ui.menu.install_update", &[("version", version)]), "", true)
        {
            actions.push(Action::ShowUpdate);
        }
        if item(ui, tr(locale, "ui.menu.check_updates"), "", true) {
            actions.push(Action::CheckForUpdates);
        }
        let at_start = ui
            .add(Button::new(tr(locale, "ui.menu.update_at_start")).selected(mc.update_check))
            .on_hover_text(tr(locale, "ui.update.privacy"));
        if at_start.clicked() {
            actions.push(Action::SetUpdateCheck(!mc.update_check));
            ui.close();
        }
        ui.menu_button(tr(locale, "ui.menu.update_channel"), |ui| {
            for c in Channel::ALL {
                if check(ui, tr(locale, channel_key(c)), mc.update_channel == c, "") {
                    actions.push(Action::SetUpdateChannel(c));
                }
            }
        });
        ui.separator();
        if item(ui, tr(locale, "ui.menu.about"), "", true) {
            actions.push(Action::OpenWindow(Window::About));
        }
    });
}

/// Catalogue key of the name of an update channel.
pub fn channel_key(c: Channel) -> &'static str {
    match c {
        Channel::Stable => "ui.update.channel.stable",
        Channel::Test => "ui.update.channel.test",
    }
}
