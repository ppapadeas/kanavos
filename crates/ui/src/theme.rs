//! Kanavos colour tokens and interface style (docs/ux.md §6).
//!
//! The palette is our own: quiet neutral surfaces so the drawing carries the colour, one blue for selection and
//! focus, amber for warnings, red for errors and green for "within limits". Every colour the interface paints
//! comes from [`Tokens`]; widgets never hard-code colours.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Theme as EguiTheme, Vec2};
use serde::{Deserialize, Serialize};

/// Light or dark interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Light surfaces, dark text (default).
    #[default]
    Light,
    /// Dark surfaces, light text.
    Dark,
}

/// The colour tokens of one theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    /// Behind everything (window background between panels).
    pub app: Color32,
    /// Panels (tree, inspector, bars).
    pub panel: Color32,
    /// Raised surfaces: windows, popups, input fields.
    pub raised: Color32,
    /// Sunken surfaces: text fields, list backgrounds.
    pub sunken: Color32,
    /// Hairlines between areas.
    pub border: Color32,
    /// Stronger outlines (focused fields, tab underline when inactive).
    pub border_strong: Color32,
    /// Body text.
    pub text: Color32,
    /// Secondary text (labels, units).
    pub text_muted: Color32,
    /// Placeholder and disabled text.
    pub text_faint: Color32,
    /// Selection and focus blue.
    pub accent: Color32,
    /// Text drawn on top of the accent colour.
    pub on_accent: Color32,
    /// Tinted background for selected rows and active tools.
    pub accent_weak: Color32,
    /// Hover background.
    pub hover: Color32,
    /// Warnings (amber).
    pub warning: Color32,
    /// Tinted background behind warnings.
    pub warning_weak: Color32,
    /// Errors (red).
    pub error: Color32,
    /// Tinted background behind errors.
    pub error_weak: Color32,
    /// Within limits (green).
    pub ok: Color32,
    /// Track of meter bars.
    pub track: Color32,
}

/// Light theme tokens.
pub const LIGHT: Tokens = Tokens {
    app: Color32::from_rgb(0xE9, 0xEB, 0xEE),
    panel: Color32::from_rgb(0xF6, 0xF7, 0xF8),
    raised: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    sunken: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    border: Color32::from_rgb(0xD8, 0xDB, 0xE0),
    border_strong: Color32::from_rgb(0xB4, 0xBA, 0xC3),
    text: Color32::from_rgb(0x1E, 0x23, 0x2A),
    text_muted: Color32::from_rgb(0x5A, 0x63, 0x6E),
    text_faint: Color32::from_rgb(0x95, 0x9C, 0xA6),
    accent: Color32::from_rgb(0x1F, 0x6F, 0xD1),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    accent_weak: Color32::from_rgb(0xDD, 0xE9, 0xF9),
    hover: Color32::from_rgb(0xE6, 0xE9, 0xED),
    warning: Color32::from_rgb(0xB7, 0x79, 0x00),
    warning_weak: Color32::from_rgb(0xFB, 0xF0, 0xD6),
    error: Color32::from_rgb(0xC0, 0x34, 0x2B),
    error_weak: Color32::from_rgb(0xFA, 0xE2, 0xDF),
    ok: Color32::from_rgb(0x2E, 0x84, 0x52),
    track: Color32::from_rgb(0xE2, 0xE5, 0xE9),
};

/// Dark theme tokens.
pub const DARK: Tokens = Tokens {
    app: Color32::from_rgb(0x16, 0x18, 0x1C),
    panel: Color32::from_rgb(0x21, 0x24, 0x2A),
    raised: Color32::from_rgb(0x2A, 0x2E, 0x35),
    sunken: Color32::from_rgb(0x1A, 0x1D, 0x22),
    border: Color32::from_rgb(0x36, 0x3B, 0x44),
    border_strong: Color32::from_rgb(0x4E, 0x55, 0x60),
    text: Color32::from_rgb(0xE4, 0xE7, 0xEB),
    text_muted: Color32::from_rgb(0xA3, 0xAA, 0xB4),
    text_faint: Color32::from_rgb(0x6E, 0x76, 0x81),
    accent: Color32::from_rgb(0x4C, 0x8D, 0xF0),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    accent_weak: Color32::from_rgb(0x24, 0x36, 0x52),
    hover: Color32::from_rgb(0x30, 0x35, 0x3D),
    warning: Color32::from_rgb(0xE3, 0xAA, 0x3E),
    warning_weak: Color32::from_rgb(0x3D, 0x32, 0x1C),
    error: Color32::from_rgb(0xEC, 0x6B, 0x60),
    error_weak: Color32::from_rgb(0x45, 0x25, 0x24),
    ok: Color32::from_rgb(0x5F, 0xBF, 0x82),
    track: Color32::from_rgb(0x34, 0x39, 0x41),
};

impl Tokens {
    /// Tokens of a theme.
    pub fn of_theme(theme: Theme) -> &'static Tokens {
        match theme {
            Theme::Light => &LIGHT,
            Theme::Dark => &DARK,
        }
    }

    /// Tokens matching the style a `Ui` is drawn with.
    pub fn of(ui: &egui::Ui) -> &'static Tokens {
        if ui.visuals().dark_mode { &DARK } else { &LIGHT }
    }
}

/// Base sizes in points before the UI scale is applied.
pub mod size {
    /// Body text.
    pub const BODY: f32 = 13.5;
    /// Small text (units, captions, status bar).
    pub const SMALL: f32 = 11.5;
    /// Panel and window headings.
    pub const HEADING: f32 = 14.5;
    /// Height of interactive rows.
    pub const ROW: f32 = 24.0;
    /// Tool palette button.
    pub const TOOL_BUTTON: f32 = 30.0;
}

/// Applies a theme to the context: visuals, text styles and spacing. The UI scale is applied as egui's zoom
/// factor so that everything (including the canvas chrome) scales together.
pub fn apply(ctx: &egui::Context, theme: Theme, ui_scale: f32) {
    let t = Tokens::of_theme(theme);
    let (egui_theme, mut visuals) = match theme {
        Theme::Light => (EguiTheme::Light, egui::Visuals::light()),
        Theme::Dark => (EguiTheme::Dark, egui::Visuals::dark()),
    };
    visuals.panel_fill = t.panel;
    visuals.window_fill = t.raised;
    visuals.window_stroke = Stroke::new(1.0, t.border);
    visuals.window_corner_radius = CornerRadius::same(8);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.faint_bg_color = t.hover;
    visuals.extreme_bg_color = t.sunken;
    visuals.text_edit_bg_color = Some(t.sunken);
    visuals.code_bg_color = t.hover;
    visuals.warn_fg_color = t.warning;
    visuals.error_fg_color = t.error;
    visuals.hyperlink_color = t.accent;
    visuals.selection.bg_fill = t.accent_weak;
    visuals.selection.stroke = Stroke::new(1.0, t.accent);
    visuals.override_text_color = None;
    visuals.collapsing_header_frame = false;
    visuals.indent_has_left_vline = false;
    visuals.striped = false;
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if theme == Theme::Dark { 110 } else { 40 }),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(if theme == Theme::Dark { 100 } else { 32 }),
    };

    let radius = CornerRadius::same(4);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = t.panel;
    w.noninteractive.weak_bg_fill = t.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    w.noninteractive.corner_radius = radius;
    w.inactive.bg_fill = t.raised;
    w.inactive.weak_bg_fill = t.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, t.border);
    w.inactive.fg_stroke = Stroke::new(1.0, t.text);
    w.inactive.corner_radius = radius;
    w.hovered.bg_fill = t.hover;
    w.hovered.weak_bg_fill = t.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, t.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, t.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;
    w.active.bg_fill = t.accent_weak;
    w.active.weak_bg_fill = t.accent_weak;
    w.active.bg_stroke = Stroke::new(1.0, t.accent);
    w.active.fg_stroke = Stroke::new(1.5, t.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;
    w.open.bg_fill = t.hover;
    w.open.weak_bg_fill = t.hover;
    w.open.bg_stroke = Stroke::new(1.0, t.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, t.text);
    w.open.corner_radius = radius;

    ctx.set_visuals_of(egui_theme, visuals);
    ctx.set_theme(egui_theme);
    ctx.style_mut_of(egui_theme, |style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(size::SMALL, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(size::BODY, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(size::BODY, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(size::HEADING, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(size::BODY - 1.0, FontFamily::Monospace)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = Vec2::new(8.0, 5.0);
        s.button_padding = Vec2::new(8.0, 3.0);
        s.interact_size = Vec2::new(36.0, size::ROW - 2.0);
        s.window_margin = Margin::same(12);
        s.menu_margin = Margin::same(6);
        s.indent = 16.0;
        s.combo_width = 140.0;
        s.text_edit_width = 120.0;
        s.icon_width = 14.0;
        s.icon_width_inner = 8.0;
        s.icon_spacing = 6.0;
        s.tooltip_width = 320.0;
        s.menu_width = 220.0;
        style.interaction.tooltip_delay = 0.6;
        style.animation_time = 0.12;
    });
    ctx.set_zoom_factor(ui_scale.clamp(0.75, 2.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b())
    }

    #[test]
    fn text_contrasts_with_panels_in_both_themes() {
        for t in [&LIGHT, &DARK] {
            let gap = (luminance(t.text) - luminance(t.panel)).abs();
            assert!(gap > 150.0, "text/panel contrast too low: {gap}");
            let muted = (luminance(t.text_muted) - luminance(t.panel)).abs();
            assert!(muted > 80.0, "muted text contrast too low: {muted}");
        }
    }

    #[test]
    fn themes_are_distinct() {
        assert_ne!(LIGHT.panel, DARK.panel);
        assert!(luminance(LIGHT.panel) > luminance(DARK.panel));
    }
}
