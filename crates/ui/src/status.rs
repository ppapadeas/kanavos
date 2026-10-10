//! The status bar (docs/ux.md §2): tool prompt or notices, pointer position in project and ΕΓΣΑ87
//! coordinates, snap toggles, scale of the focused view, active storey and background work.

use egui::{Align, Layout, RichText, Ui};
use kanavos_geom::Point2;
use kanavos_model::Georef;
use kanavos_units::{Length, Locale, format_decimal, format_level};

use crate::actions::Action;
use crate::icons::{self, Icon};
use crate::names::tr;
use crate::state::AppState;
use crate::theme::Tokens;

/// Converts project coordinates to grid coordinates (easting, northing) with the project's georeference,
/// through `kanavos_geo::GridPlacement`.
pub fn to_grid(georef: &Georef, p: Point2) -> (f64, f64) {
    let placement = kanavos_geo::GridPlacement {
        easting: georef.easting,
        northing: georef.northing,
        rotation: georef.rotation.as_rad(),
        altitude: georef.altitude,
    };
    let [x, y] = p.xy();
    let [e, n] = placement.project_to_grid(x, y);
    (e, n)
}

/// Formats a grid coordinate with thousands grouping by a thin space and two decimals.
pub fn format_grid(v: f64, locale: Locale) -> String {
    let s = format_decimal(v, 2, locale);
    let (int, frac) = match s.find(locale.decimal_separator()) {
        Some(i) => s.split_at(i),
        None => (s.as_str(), ""),
    };
    let (sign, digits) = match int.strip_prefix('-') {
        Some(d) => ("-", d),
        None => ("", int),
    };
    let mut grouped = String::new();
    let n = digits.chars().count();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            grouped.push('\u{2009}');
        }
        grouped.push(ch);
    }
    format!("{sign}{grouped}{frac}")
}

/// What the status bar shows besides the application state.
pub struct StatusInfo {
    /// Background computations in progress.
    pub busy: bool,
    /// The notice is fresh enough to show (errors stay longer than information).
    pub show_notice: bool,
    /// A new version on offer or an update under way; clicking it opens the update window.
    pub update: Option<String>,
}

fn separator(ui: &mut Ui) {
    let t = Tokens::of(ui);
    let r = ui.available_rect_before_wrap();
    ui.add_space(4.0);
    let x = ui.cursor().left();
    ui.painter().vline(x, (r.center().y - 7.0)..=(r.center().y + 7.0), egui::Stroke::new(1.0, t.border));
    ui.add_space(5.0);
}

/// Shows the status bar.
pub fn bar(ui: &mut Ui, state: &mut AppState, info: &StatusInfo, actions: &mut Vec<Action>) {
    let locale = state.locale;
    let t = Tokens::of(ui);
    let small = |s: String| RichText::new(s).size(crate::theme::size::SMALL);
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        // Left: notice or prompt.
        match (&state.notice, info.show_notice) {
            (Some(n), true) => {
                let (icon, color) = if n.error { (Icon::Error, t.error) } else { (Icon::Info, t.accent) };
                icons::inline(ui, icon, color);
                ui.add(
                    egui::Label::new(small(n.text.clone()).color(if n.error { t.error } else { t.text }))
                        .truncate(),
                );
            }
            _ => {
                let prompt = if state.prompt.is_empty() {
                    tr(locale, "ui.status.ready")
                } else {
                    state.prompt.clone()
                };
                ui.add(egui::Label::new(small(prompt).color(t.text_muted)).truncate());
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            // A new version: small, at the far end, never in the way.
            if let Some(text) = &info.update {
                let link = egui::Button::new(small(text.clone()).color(t.accent)).frame(false);
                if ui.add(link).on_hover_text(tr(locale, "ui.update.title")).clicked() {
                    actions.push(Action::ShowUpdate);
                }
                icons::inline(ui, Icon::Info, t.accent);
                separator(ui);
            }
            if info.busy {
                ui.label(small(tr(locale, "ui.status.updating")).color(t.text_muted));
                ui.add(egui::Spinner::new().size(12.0));
                separator(ui);
            }
            // Active storey.
            if let Some(s) = state.doc().storey(state.active_storey) {
                let text = format!("{}  {}", s.name, format_level(s.elevation, locale));
                ui.label(small(text).color(t.text));
                icons::inline(ui, Icon::Storey, t.text_muted);
                separator(ui);
            }
            // Scale.
            if let Some(scale) = state.view_scale {
                ui.label(small(format!("1:{scale}")).color(t.text));
                separator(ui);
            }
            // Snap toggles (right to left: grid, edges, points, master).
            let snap = &mut state.snap;
            let size = 20.0;
            let mut grid_on = snap.grid.is_some();
            if icons::button(
                ui,
                Icon::GridSnap,
                grid_on && snap.enabled,
                size,
                &tr(locale, "ui.status.snap_grid"),
            )
            .clicked()
            {
                grid_on = !grid_on;
                snap.grid = if grid_on { Some(Length::cm(5.0)) } else { None };
            }
            let label = tr(locale, "ui.status.snap_inference");
            if icons::button(ui, Icon::Inference, snap.inference && snap.enabled, size, &label).clicked() {
                snap.inference = !snap.inference;
            }
            if icons::button(
                ui,
                Icon::EdgeSnap,
                snap.edges && snap.enabled,
                size,
                &tr(locale, "ui.status.snap_edges"),
            )
            .clicked()
            {
                snap.edges = !snap.edges;
            }
            if icons::button(
                ui,
                Icon::PointSnap,
                snap.points && snap.enabled,
                size,
                &tr(locale, "ui.status.snap_points"),
            )
            .clicked()
            {
                snap.points = !snap.points;
            }
            if icons::button(ui, Icon::Snap, snap.enabled, size, &tr(locale, "ui.status.snap")).clicked() {
                snap.enabled = !snap.enabled;
            }
            separator(ui);
            // Coordinates.
            match state.cursor {
                Some(p) => {
                    let (e, n) = to_grid(&state.doc().site().georef, p);
                    let grid = format!("ΕΓΣΑ87  E {}  N {}", format_grid(e, locale), format_grid(n, locale));
                    ui.label(small(grid).color(t.text_muted).monospace());
                    let [x, y] = p.xy();
                    let xy =
                        format!("X {}  Y {}", format_decimal(x, 2, locale), format_decimal(y, 2, locale));
                    ui.label(small(xy).color(t.text).monospace());
                }
                None => {
                    ui.label(small(tr(locale, "ui.status.no_cursor")).color(t.text_faint));
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_units::Angle;

    #[test]
    fn grid_coordinates_follow_origin_and_rotation() {
        let mut g = Georef { easting: 476_000.0, northing: 4_097_000.0, ..Georef::default() };
        let (e, n) = to_grid(&g, Point2::new(10.0, 5.0));
        assert!((e - 476_010.0).abs() < 1e-9 && (n - 4_097_005.0).abs() < 1e-9);
        // Rotated 90° counter-clockwise, project +Y points to grid east.
        g.rotation = Angle::deg(90.0);
        let (e, n) = to_grid(&g, Point2::new(0.0, 10.0));
        assert!((e - 476_010.0).abs() < 1e-6 && (n - 4_097_000.0).abs() < 1e-6);
    }

    #[test]
    fn grid_values_are_grouped_with_the_locale_separator() {
        assert_eq!(format_grid(476_532.126, Locale::El), "476\u{2009}532,13");
        assert_eq!(format_grid(4_097_345.88, Locale::En), "4\u{2009}097\u{2009}345.88");
        assert_eq!(format_grid(-12.5, Locale::En), "-12.50");
    }
}
