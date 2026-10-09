//! The tool palette: every tool as an icon button, in groups, with name, shortcut and a short description in
//! the tooltip. Single-key shortcuts work whenever no text field has focus (handled by the shell).

use egui::{RichText, Ui};
use kanavos_tools::ToolId;
use kanavos_units::Locale;

use crate::actions::Action;
use crate::icons::{self, Icon};
use crate::names;
use crate::theme::{Tokens, size};

/// Tool groups in palette order: selection, building parts, annotation, drafting, views and site.
pub const GROUPS: [&[ToolId]; 5] = [
    &[ToolId::Select, ToolId::Measure],
    &[
        ToolId::Wall,
        ToolId::Column,
        ToolId::Beam,
        ToolId::Slab,
        ToolId::Roof,
        ToolId::Stair,
        ToolId::Railing,
        ToolId::Door,
        ToolId::Window,
        ToolId::Void,
        ToolId::Space,
        ToolId::Object,
    ],
    &[ToolId::Dimension, ToolId::Text, ToolId::Level, ToolId::Grid],
    &[ToolId::Line, ToolId::Polyline, ToolId::Arc, ToolId::Circle, ToolId::Fill],
    &[ToolId::Section, ToolId::Elevation, ToolId::Plot],
];

/// Width of the palette column.
pub const WIDTH: f32 = 2.0 * size::TOOL_BUTTON + 14.0;

/// Tooltip and accessible label of a tool: name and shortcut.
pub fn tool_label(locale: Locale, id: ToolId) -> String {
    match id.shortcut() {
        Some(k) => format!("{} ({k})", names::tool(locale, id)),
        None => names::tool(locale, id),
    }
}

/// Shows the palette.
pub fn ui(ui: &mut Ui, active: ToolId, locale: Locale, actions: &mut Vec<Action>) {
    let t = Tokens::of(ui);
    ui.add_space(6.0);
    for (gi, tools) in GROUPS.iter().enumerate() {
        if gi > 0 {
            ui.add_space(3.0);
            crate::widgets::rule(ui);
            ui.add_space(3.0);
        }
        for pair in tools.chunks(2) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.add_space(4.0);
                for &id in pair {
                    let label = tool_label(locale, id);
                    let r = icons::button(ui, Icon::Tool(id), active == id, size::TOOL_BUTTON, "");
                    let r = r.on_hover_ui(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(names::tool(locale, id)).family(crate::fonts::strong()));
                            if let Some(k) = id.shortcut() {
                                crate::widgets::pill(ui, &k.to_string(), t.text_muted, t.hover);
                            }
                        });
                        ui.label(RichText::new(names::tool_hint(locale, id)).color(t.text_muted));
                    });
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            true,
                            active == id,
                            label.clone(),
                        )
                    });
                    if r.clicked() {
                        actions.push(Action::Tool(id));
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_appears_once_in_the_palette() {
        let mut seen: Vec<ToolId> = GROUPS.iter().flat_map(|t| t.iter().copied()).collect();
        seen.sort();
        let mut all = ToolId::ALL.to_vec();
        all.sort();
        assert_eq!(seen, all);
    }

    #[test]
    fn labels_carry_shortcuts() {
        assert!(tool_label(Locale::En, ToolId::Wall).ends_with("(W)"));
        assert!(!tool_label(Locale::En, ToolId::Beam).contains('('));
    }
}
