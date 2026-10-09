//! Keyboard shortcuts and About windows.

use egui::{RichText, Ui};
use kanavos_tools::ToolId;
use kanavos_units::Locale;

use crate::names::{self, tr, trf};
use crate::theme::Tokens;
use crate::widgets;

/// Global shortcuts of the shell: (keys, catalogue key of the description).
pub const GLOBAL_SHORTCUTS: [(&str, &str); 16] = [
    ("Ctrl+K", "ui.palette.button"),
    ("Ctrl+N", "ui.menu.new"),
    ("Ctrl+O", "ui.menu.open"),
    ("Ctrl+S", "ui.menu.save"),
    ("Ctrl+Shift+S", "ui.menu.save_as"),
    ("Ctrl+Z", "ui.menu.undo"),
    ("Ctrl+Y / Ctrl+Shift+Z", "ui.menu.redo"),
    ("Ctrl+A", "ui.menu.select_all"),
    ("Del", "ui.menu.delete"),
    ("Esc", "ui.shortcut.escape"),
    ("Tab", "ui.menu.toggle_panels"),
    ("Home", "ui.menu.zoom_extents"),
    ("F1", "ui.menu.shortcuts"),
    ("Ctrl+=  /  Ctrl+−", "ui.shortcut.ui_scale"),
    ("Enter", "ui.shortcut.enter"),
    ("Shift / Alt", "ui.shortcut.modifiers"),
];

/// The shortcuts window.
pub fn shortcuts_ui(ui: &mut Ui, locale: Locale) {
    let t = Tokens::of(ui);
    egui::ScrollArea::vertical().id_salt("shortcuts").show(ui, |ui| {
        ui.label(RichText::new(tr(locale, "ui.shortcuts.tools")).family(crate::fonts::strong()));
        egui::Grid::new("shortcut-tools").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
            for id in ToolId::ALL {
                if let Some(k) = id.shortcut() {
                    widgets::pill(ui, &k.to_string(), t.text, t.hover);
                    ui.label(names::tool(locale, id));
                    ui.end_row();
                }
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new(tr(locale, "ui.shortcuts.global")).family(crate::fonts::strong()));
        egui::Grid::new("shortcut-global").num_columns(2).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
            for (keys, key) in GLOBAL_SHORTCUTS {
                widgets::pill(ui, keys, t.text, t.hover);
                ui.label(tr(locale, key));
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        widgets::hint(ui, &tr(locale, "ui.shortcuts.note"));
    });
}

/// The About window: version, licence and the clean-room statement.
pub fn about_ui(ui: &mut Ui, locale: Locale) {
    let t = Tokens::of(ui);
    ui.horizontal(|ui| {
        crate::icons::inline(ui, crate::icons::Icon::Tool(ToolId::Wall), t.accent);
        ui.label(RichText::new("Kanavos").family(crate::fonts::strong()).size(20.0));
        ui.label(RichText::new(env!("CARGO_PKG_VERSION")).color(t.text_muted));
    });
    ui.add_space(4.0);
    ui.label(tr(locale, "ui.about.tagline"));
    ui.add_space(8.0);
    widgets::grid(ui, "about-grid", |ui| {
        widgets::label(ui, &tr(locale, "ui.about.licence"));
        widgets::value(ui, "MIT OR Apache-2.0");
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.about.formats"));
        widgets::value(ui, "IFC 4.3 · DXF · PDF · SVG · glTF · CSV");
        ui.end_row();
        widgets::label(ui, &tr(locale, "ui.about.fonts"));
        widgets::value(ui, &tr(locale, "ui.about.fonts_value"));
        ui.end_row();
    });
    ui.add_space(8.0);
    egui::Frame::new().fill(t.hover).corner_radius(6.0).inner_margin(10.0).show(ui, |ui| {
        ui.label(RichText::new(tr(locale, "ui.about.clean_room_title")).family(crate::fonts::strong()));
        ui.label(RichText::new(tr(locale, "ui.about.clean_room")).color(t.text_muted));
    });
    ui.add_space(6.0);
    widgets::hint(ui, &trf(locale, "ui.about.telemetry", &[]));
}
