//! Shared widgets: value fields that accept typed values with units and arithmetic, mixed-value display,
//! property grids, section headers, meter bars and colour swatches.
//!
//! Value fields show the current value formatted for the interface language. When focused they become a text
//! field: the user types `3,20`, `320cm`, `3.20+0.25` or `2*1,5`, and `Enter` or leaving the field commits.
//! `Esc` abandons the edit. A field showing several different values (a mixed multi-selection) reads "—"; typing
//! a value there sets it on every selected element.

use std::fmt::Debug;
use std::hash::Hash;

use egui::text::{CCursor, CCursorRange};
use egui::{Color32, Id, Key, Response, RichText, Sense, Stroke, TextEdit, Ui, Vec2, WidgetInfo, WidgetType};
use kanavos_drawing::Rgba;
use kanavos_tools::input::eval_length;
use kanavos_units::{Length, Locale, Tolerance, format_decimal};

use crate::names::tr;
use crate::theme::Tokens;

/// Text shown for a mixed value.
pub const MIXED: &str = "—";

/// Formats a length in metres for display: two decimals, or three when the millimetre matters (`0,125`).
pub fn format_length(value: Length, locale: Locale) -> String {
    let three = format_decimal(value.as_m(), 3, locale);
    if three.ends_with('0') { format_decimal(value.as_m(), 2, locale) } else { three }
}

/// Formats a plain number with up to `decimals` decimals, dropping trailing zeros after the first `min`.
pub fn format_number(value: f64, decimals: usize, locale: Locale) -> String {
    format_decimal(value, decimals, locale)
}

/// Reads a length typed by the user: metres by default, `cm`/`mm`/`m` suffixes, either decimal separator,
/// `+ - * /` and parentheses. Returns `None` for anything else.
pub fn parse_length_text(text: &str) -> Option<Length> {
    let s = text.trim();
    if s.is_empty() || s == MIXED {
        return None;
    }
    eval_length(s).ok()
}

/// Reads a plain number typed by the user (either decimal separator, arithmetic allowed). A trailing `°` or
/// `%` is ignored; unit letters are rejected.
pub fn parse_number_text(text: &str) -> Option<f64> {
    let s = text.trim().trim_end_matches(['°', '%']).trim();
    if s.is_empty() || s == MIXED || s.chars().any(char::is_alphabetic) {
        return None;
    }
    eval_length(s).ok().map(Length::as_m).filter(|v| v.is_finite())
}

/// Reads a thickness typed in centimetres: bare numbers and arithmetic are centimetres (`8`, `2*9`), explicit
/// units are honoured (`80mm`, `0.08m`).
pub fn parse_cm_text(text: &str) -> Option<Length> {
    let s = text.trim();
    if s.chars().any(char::is_alphabetic) {
        parse_length_text(s)
    } else {
        parse_number_text(s).map(Length::cm)
    }
}

/// What kind of value a [`value_field`] edits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldKind {
    /// A length in metres (unit suffix shown).
    Length,
    /// A length shown in centimetres (layer thicknesses); bare numbers are read as centimetres.
    Centimetres,
    /// A plain number with a number of decimals and an optional unit suffix.
    Number {
        /// Decimals shown.
        decimals: usize,
        /// Unit shown after the field.
        unit: &'static str,
    },
}

fn field_state_id(id: Id) -> Id {
    id.with("kanavos-field-buffer")
}

fn field_error_id(id: Id) -> Id {
    id.with("kanavos-field-error")
}

/// A value field. `value` is `None` for a mixed value. Returns the new value when the user commits a change.
pub fn value_field(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    value: Option<f64>,
    kind: FieldKind,
    locale: Locale,
    width: f32,
) -> Option<f64> {
    let id = ui.make_persistent_id(id_salt);
    let display = match (value, kind) {
        (Some(v), FieldKind::Length) => format_length(Length::m(v), locale),
        (Some(v), FieldKind::Centimetres) => format_number(v * 100.0, 1, locale),
        (Some(v), FieldKind::Number { decimals, .. }) => format_number(v, decimals, locale),
        (None, _) => MIXED.to_owned(),
    };
    // Long values (grid coordinates) widen the field instead of being cut.
    let width = width.max(display.chars().count() as f32 * 8.5 + 14.0);
    let buffer_id = field_state_id(id);
    let error_id = field_error_id(id);
    let editing: Option<String> = ui.data(|d| d.get_temp::<String>(buffer_id));
    let mut text = editing.clone().unwrap_or_else(|| display.clone());
    let had_error = ui.data(|d| d.get_temp::<bool>(error_id)).unwrap_or(false);
    let t = Tokens::of(ui);
    let unit = match kind {
        FieldKind::Length => "m",
        FieldKind::Centimetres => "cm",
        FieldKind::Number { unit, .. } => unit,
    };
    let mut committed = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let mut edit =
            TextEdit::singleline(&mut text).id(id).desired_width(width).horizontal_align(egui::Align::RIGHT);
        if value.is_none() && editing.is_none() {
            edit = edit.text_color(t.text_muted);
        }
        let mut output = edit.show(ui);
        let response = output.response.response.clone();
        if response.gained_focus() {
            // Start editing from the shown value (or empty for a mixed one), all selected.
            let start = if value.is_some() { display.clone() } else { String::new() };
            let n = start.chars().count();
            text = start;
            output.state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(n))));
            output.state.store(ui.ctx(), id);
            ui.data_mut(|d| {
                d.insert_temp(buffer_id, text.clone());
                d.remove::<bool>(error_id);
            });
        } else if response.has_focus() {
            ui.data_mut(|d| d.insert_temp(buffer_id, text.clone()));
        }
        if response.lost_focus() {
            let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
            ui.data_mut(|d| d.remove::<String>(buffer_id));
            if !cancelled && text.trim() != display && !(value.is_none() && text.trim().is_empty()) {
                let parsed = match kind {
                    FieldKind::Length => parse_length_text(&text).map(Length::as_m),
                    FieldKind::Centimetres => parse_cm_text(&text).map(Length::as_m),
                    FieldKind::Number { .. } => parse_number_text(&text),
                };
                match parsed {
                    Some(v) => {
                        let same = value.is_some_and(|old| Tolerance::default().equal_m(old, v));
                        if !same {
                            committed = Some(v);
                        }
                    }
                    None => {
                        ui.data_mut(|d| d.insert_temp(error_id, true));
                    }
                }
            }
        }
        if had_error && !response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.expand(1.0),
                4.0,
                Stroke::new(1.5, t.error),
                egui::StrokeKind::Outside,
            );
        }
        if !unit.is_empty() {
            ui.label(RichText::new(unit).color(t.text_muted).small());
        }
        let hint = if had_error { tr(locale, "ui.field.invalid") } else { tr(locale, "ui.field.hint") };
        response.on_hover_text(hint);
    });
    committed
}

/// A length field in metres. Returns the new length when committed.
pub fn length_field(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    value: Option<Length>,
    locale: Locale,
) -> Option<Length> {
    value_field(ui, id_salt, value.map(Length::as_m), FieldKind::Length, locale, 72.0).map(Length::m)
}

/// A thickness field shown in centimetres. Returns the new length when committed.
pub fn cm_field(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    value: Option<Length>,
    locale: Locale,
) -> Option<Length> {
    value_field(ui, id_salt, value.map(Length::as_m), FieldKind::Centimetres, locale, 56.0).map(Length::m)
}

/// A number field. Returns the new number when committed.
pub fn number_field(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    value: Option<f64>,
    decimals: usize,
    unit: &'static str,
    locale: Locale,
) -> Option<f64> {
    value_field(ui, id_salt, value, FieldKind::Number { decimals, unit }, locale, 72.0)
}

/// A single-line text field that commits on `Enter` or when focus leaves. `value` is `None` for mixed text.
pub fn text_field(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    value: Option<&str>,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(id_salt);
    let buffer_id = field_state_id(id);
    let editing: Option<String> = ui.data(|d| d.get_temp::<String>(buffer_id));
    let shown = value.unwrap_or(MIXED).to_owned();
    let mut text = editing.unwrap_or_else(|| shown.clone());
    let response = TextEdit::singleline(&mut text)
        .id(id)
        .desired_width(width)
        .min_size(Vec2::new(width, 0.0))
        .show(ui)
        .response;
    if response.gained_focus() && value.is_none() {
        text.clear();
    }
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(buffer_id, text.clone()));
    }
    if response.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(buffer_id));
        let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
        if !cancelled && Some(text.as_str()) != value && !(value.is_none() && text.is_empty()) {
            return Some(text);
        }
    }
    None
}

/// A multi-line text field that commits when focus leaves.
pub fn multiline_field(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    value: Option<&str>,
    width: f32,
) -> Option<String> {
    let id = ui.make_persistent_id(id_salt);
    let buffer_id = field_state_id(id);
    let editing: Option<String> = ui.data(|d| d.get_temp::<String>(buffer_id));
    let shown = value.unwrap_or(MIXED).to_owned();
    let mut text = editing.unwrap_or_else(|| shown.clone());
    let response =
        TextEdit::multiline(&mut text).id(id).desired_width(width).desired_rows(3).show(ui).response;
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(buffer_id, text.clone()));
    }
    if response.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(buffer_id));
        if Some(text.as_str()) != value {
            return Some(text);
        }
    }
    None
}

/// Shortens text to at most `max_chars` characters, ending with "…" when cut. Drop-downs use it so that long
/// names never widen a bar or panel (truncation by width does not apply inside scrolling rows).
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out = out.trim_end().to_owned();
    out.push('…');
    out
}

/// Approximate number of body-text characters that fit in `width` points.
pub fn chars_for(width: f32) -> usize {
    ((width / 7.0).floor() as usize).max(6)
}

/// A drop-down choice. `selected` is `None` for a mixed value. Returns the newly chosen option.
pub fn choice<T: PartialEq + Clone>(
    ui: &mut Ui,
    id_salt: impl Hash + Debug,
    selected: Option<&T>,
    options: &[(T, String)],
    width: f32,
) -> Option<T> {
    let current = selected
        .and_then(|s| options.iter().find(|(v, _)| v == s))
        .map(|(_, l)| l.clone())
        .unwrap_or_else(|| MIXED.to_owned());
    let mut picked = None;
    let current = ellipsize(&current, chars_for(width));
    egui::ComboBox::from_id_salt(id_salt).width(width).selected_text(current).truncate().show_ui(ui, |ui| {
        for (v, label) in options {
            let is = selected.is_some_and(|s| s == v);
            if ui.selectable_label(is, label).clicked() && !is {
                picked = Some(v.clone());
            }
        }
    });
    picked
}

/// A check box for a possibly mixed boolean. Returns the new value when clicked.
pub fn tri_check(ui: &mut Ui, value: Option<bool>, label: &str) -> Option<bool> {
    let mut v = value.unwrap_or(false);
    let response = ui.add(egui::Checkbox::new(&mut v, label).indeterminate(value.is_none()));
    if response.clicked() { Some(if value.is_none() { true } else { v }) } else { None }
}

/// A two-column property grid with muted labels on the left.
pub fn grid(ui: &mut Ui, id_salt: impl Hash + Debug, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id_salt)
        .num_columns(2)
        .spacing([10.0, 6.0])
        .min_col_width(140.0)
        .striped(false)
        .show(ui, add);
}

/// A muted label for the left column of a property grid.
pub fn label(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::of(ui);
    ui.label(RichText::new(text).color(t.text_muted))
}

/// A read-only value in a property grid.
pub fn value(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::of(ui);
    ui.label(RichText::new(text).color(t.text))
}

/// A collapsible section with a strong title.
pub fn section(ui: &mut Ui, id_salt: &str, title: &str, default_open: bool, body: impl FnOnce(&mut Ui)) {
    let t = Tokens::of(ui);
    egui::CollapsingHeader::new(RichText::new(title).strong().color(t.text))
        .id_salt(id_salt)
        .default_open(default_open)
        .show(ui, body);
}

/// A thin horizontal rule in the border colour.
pub fn rule(ui: &mut Ui) {
    let t = Tokens::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, t.border));
}

/// A horizontal meter: track, filled part up to `fraction` (clamped to 0..1), and a marker at 100 % when
/// `fraction` exceeds it.
pub fn meter_bar(ui: &mut Ui, fraction: f64, color: Color32, label: &str) -> Response {
    let t = Tokens::of(ui);
    let width = ui.available_width().max(60.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 4.0, t.track);
        let f = if fraction.is_finite() { fraction.clamp(0.0, 1.0) as f32 } else { 1.0 };
        if f > 0.0 {
            let mut fill = rect;
            fill.set_width((rect.width() * f).max(4.0));
            ui.painter().rect_filled(fill, 4.0, color);
        }
    }
    let label = label.to_owned();
    response.widget_info(|| WidgetInfo::labeled(WidgetType::ProgressIndicator, true, label.clone()));
    response
}

/// Converts a model colour to an egui colour.
pub fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Converts an egui colour to a model colour.
pub fn rgba(c: Color32) -> Rgba {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Rgba::new(r, g, b, a)
}

/// A small colour swatch with a hairline border.
pub fn swatch(ui: &mut Ui, color: Rgba, size: f32) -> Response {
    let t = Tokens::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 3.0, color32(color));
        ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
    }
    response
}

/// A colour picker button for a model colour. Returns the new colour when changed.
pub fn color_field(ui: &mut Ui, color: Rgba) -> Option<Rgba> {
    let mut c = color32(color);
    let response = egui::widgets::color_picker::color_edit_button_srgba(
        ui,
        &mut c,
        egui::widgets::color_picker::Alpha::OnlyBlend,
    );
    if response.changed() { Some(rgba(c)) } else { None }
}

/// A muted hint paragraph (empty states, explanations).
pub fn hint(ui: &mut Ui, text: &str) -> Response {
    let t = Tokens::of(ui);
    ui.label(RichText::new(text).color(t.text_muted))
}

/// A small status pill: coloured text on a tinted background.
pub fn pill(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    egui::Frame::new()
        .fill(bg)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| ui.label(RichText::new(text).color(fg).small()))
        .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn lengths_accept_units_arithmetic_and_decimal_commas() {
        assert!(parse_length_text("3,20").is_some_and(|l| close(l.as_m(), 3.2)));
        assert!(parse_length_text("320cm").is_some_and(|l| close(l.as_m(), 3.2)));
        assert!(parse_length_text("3.20+0.25").is_some_and(|l| close(l.as_m(), 3.45)));
        assert!(parse_length_text("2*1,5").is_some_and(|l| close(l.as_m(), 3.0)));
        assert!(parse_length_text(" 250mm ").is_some_and(|l| close(l.as_m(), 0.25)));
        assert!(parse_length_text("(3+1)/2").is_some_and(|l| close(l.as_m(), 2.0)));
    }

    #[test]
    fn lengths_reject_garbage_and_mixed_markers() {
        assert!(parse_length_text("").is_none());
        assert!(parse_length_text(MIXED).is_none());
        assert!(parse_length_text("τρία").is_none());
        assert!(parse_length_text("3,2,1").is_none());
    }

    #[test]
    fn numbers_reject_units_but_accept_degrees_and_percent() {
        assert!(parse_number_text("22,5°").is_some_and(|v| close(v, 22.5)));
        assert!(parse_number_text("60%").is_some_and(|v| close(v, 60.0)));
        assert!(parse_number_text("0,8*2").is_some_and(|v| close(v, 1.6)));
        assert!(parse_number_text("5cm").is_none());
        assert!(parse_number_text("abc").is_none());
    }

    #[test]
    fn thicknesses_default_to_centimetres() {
        assert!(parse_cm_text("8").is_some_and(|l| close(l.as_m(), 0.08)));
        assert!(parse_cm_text("2*9").is_some_and(|l| close(l.as_m(), 0.18)));
        assert!(parse_cm_text("12,5").is_some_and(|l| close(l.as_m(), 0.125)));
        assert!(parse_cm_text("80mm").is_some_and(|l| close(l.as_m(), 0.08)));
        assert!(parse_cm_text("0.08m").is_some_and(|l| close(l.as_m(), 0.08)));
        assert!(parse_cm_text("x").is_none());
    }

    #[test]
    fn lengths_format_with_the_locale_separator() {
        assert_eq!(format_length(Length::m(3.2), Locale::El), "3,20");
        assert_eq!(format_length(Length::m(3.2), Locale::En), "3.20");
        assert_eq!(format_length(Length::m(0.125), Locale::El), "0,125");
        assert_eq!(format_length(Length::m(-0.45), Locale::En), "-0.45");
    }

    #[test]
    fn long_text_is_shortened_with_an_ellipsis() {
        assert_eq!(ellipsize("Τοίχος", 10), "Τοίχος");
        assert_eq!(ellipsize("Εξωτερικός τοίχος 20", 10), "Εξωτερικό…");
        assert_eq!(ellipsize("abc def", 5), "abc…");
    }

    #[test]
    fn colours_round_trip() {
        let c = Rgba::new(10, 20, 30, 255);
        assert_eq!(rgba(color32(c)), c);
    }
}
