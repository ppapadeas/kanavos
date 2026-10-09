//! Table view for schedules and the area table: sortable columns, totals, CSV export.
//!
//! The table is recomputed from the model whenever the document revision or the language changes (tables are
//! live views of the model, docs/ux.md §1.3). Sorting is interface state only.

use std::cmp::Ordering;

use egui::{Align2, FontId, Rect, RichText, ScrollArea, Sense, Stroke, Ui, Vec2};
use kanavos_drawing::table::{Align, Cell, Table};
use kanavos_model::ScheduleKind;
use kanavos_units::Locale;

use crate::actions::Action;
use crate::exports::ExportKind;
use crate::fuzzy::fold;
use crate::icons::{self, Icon};
use crate::names::{tr, trf};
use crate::state::AppState;
use crate::theme::Tokens;

/// What a table view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSource {
    /// A schedule.
    Schedule(ScheduleKind),
    /// The ΝΟΚ area table.
    AreaTable,
}

/// A live, sortable table of the model.
pub struct TableView {
    /// What is shown.
    pub source: TableSource,
    cache: Option<(u64, Locale, Table)>,
    /// Sort column and direction (`true` = ascending).
    sort: Option<(usize, bool)>,
}

/// Compares two cells for sorting: numbers numerically, text accent-insensitively, empty cells last.
pub fn compare_cells(a: Option<&Cell>, b: Option<&Cell>) -> Ordering {
    match (a, b) {
        (Some(Cell::Number { value: x, .. }), Some(Cell::Number { value: y, .. })) => x.total_cmp(y),
        (Some(Cell::Text(x)), Some(Cell::Text(y))) => fold(x).cmp(&fold(y)),
        (Some(Cell::Number { .. }), Some(Cell::Text(_))) => Ordering::Less,
        (Some(Cell::Text(_)), Some(Cell::Number { .. })) => Ordering::Greater,
        (None | Some(Cell::Empty), None | Some(Cell::Empty)) => Ordering::Equal,
        (None | Some(Cell::Empty), _) => Ordering::Greater,
        (_, None | Some(Cell::Empty)) => Ordering::Less,
    }
}

/// Row order for a sort column and direction.
pub fn sorted_rows(table: &Table, sort: Option<(usize, bool)>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..table.rows.len()).collect();
    if let Some((col, ascending)) = sort {
        order.sort_by(|&i, &j| {
            let a = table.rows.get(i).and_then(|r| r.get(col));
            let b = table.rows.get(j).and_then(|r| r.get(col));
            let o = compare_cells(a, b);
            if ascending { o } else { o.reverse() }
        });
    }
    order
}

impl TableView {
    /// A table view.
    pub fn new(source: TableSource) -> TableView {
        TableView { source, cache: None, sort: None }
    }

    /// The table for the current document and language (computed when needed).
    pub fn table(&mut self, state: &AppState) -> &Table {
        let rev = state.session.revision();
        let stale = !matches!(&self.cache, Some((r, l, _)) if *r == rev && *l == state.locale);
        if stale {
            let t = match self.source {
                TableSource::Schedule(kind) => kanavos_schedules::schedule(state.doc(), kind, state.locale),
                TableSource::AreaTable => kanavos_rules_gr::area_table(state.doc(), state.locale),
            };
            self.cache = Some((rev, state.locale, t));
        }
        match &self.cache {
            Some((_, _, t)) => t,
            None => &EMPTY,
        }
    }

    /// Shows the table with its toolbar.
    pub fn ui(&mut self, ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
        let locale = state.locale;
        let t = Tokens::of(ui);
        let sort = self.sort;
        let table = self.table(state).clone();
        let mut new_sort = sort;
        egui::Frame::new().fill(t.raised).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.horizontal(|ui| {
                icons::inline(ui, Icon::Table, t.text_muted);
                ui.label(RichText::new(&table.title).heading().color(t.text));
                ui.label(
                    RichText::new(trf(locale, "ui.table.rows", &[("n", &table.rows.len().to_string())]))
                        .color(t.text_muted)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = tr(locale, "ui.table.export_csv");
                    if ui
                        .add(egui::Button::new(label.as_str()))
                        .on_hover_text(tr(locale, "ui.table.export_csv_hint"))
                        .clicked()
                    {
                        actions.push(Action::Export(ExportKind::Csv));
                    }
                });
            });
            ui.add_space(8.0);
            if table.columns.is_empty() || (table.rows.is_empty() && table.footer.is_empty()) {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    let key =
                        if table.columns.is_empty() { "ui.table.not_available" } else { "ui.table.empty" };
                    ui.label(RichText::new(tr(locale, key)).color(t.text_muted));
                });
                return;
            }
            if let Some(s) = draw_table(ui, &table, sort, locale) {
                new_sort = Some(s);
            }
        });
        if new_sort != sort {
            self.sort = new_sort;
        }
    }
}

static EMPTY: Table =
    Table { title: String::new(), columns: Vec::new(), rows: Vec::new(), footer: Vec::new() };

/// Draws the table; returns a new sort state when a header was clicked.
fn draw_table(
    ui: &mut Ui,
    table: &Table,
    sort: Option<(usize, bool)>,
    locale: Locale,
) -> Option<(usize, bool)> {
    let t = Tokens::of(ui);
    let font = FontId::proportional(crate::theme::size::BODY);
    let row_h = 26.0;
    let pad = 10.0;
    let painter = ui.painter().clone();
    let text_width = |s: &str| painter.layout_no_wrap(s.to_owned(), font.clone(), t.text).size().x;
    let widths: Vec<f32> = table
        .columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let cells = table.rows.iter().chain(table.footer.iter()).filter_map(|r| r.get(i));
            let w =
                cells.map(|c| text_width(&c.display(locale))).fold(text_width(&c.header) + 16.0, f32::max);
            w + 2.0 * pad
        })
        .collect();
    let total_w: f32 = widths.iter().sum();
    let order = sorted_rows(table, sort);
    let mut clicked = None;
    ScrollArea::both()
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .show(ui, |ui| {
            let rows = 1 + table.rows.len() + table.footer.len();
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(total_w, row_h * rows as f32 + 2.0), Sense::hover());
            let p = ui.painter();
            // Header.
            let header = Rect::from_min_size(rect.min, Vec2::new(total_w, row_h));
            p.rect_filled(header, 0.0, t.panel);
            let mut x = rect.min.x;
            for (i, (col, w)) in table.columns.iter().zip(widths.iter()).enumerate() {
                let cell = Rect::from_min_size(egui::pos2(x, rect.min.y), Vec2::new(*w, row_h));
                let id = ui.id().with(("table-header", i));
                let response = ui.interact(cell, id, Sense::click());
                if response.hovered() {
                    p.rect_filled(cell, 0.0, t.hover);
                }
                let arrow = match sort {
                    Some((c, true)) if c == i => " ↑",
                    Some((c, false)) if c == i => " ↓",
                    _ => "",
                };
                let text = format!("{}{arrow}", col.header);
                let (pos, align) = cell_anchor(cell, col.align, pad);
                p.text(pos, align, text, FontId::proportional(crate::theme::size::BODY), t.text);
                let header_label = col.header.clone();
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, header_label.clone())
                });
                if response.clicked() {
                    clicked = Some(match sort {
                        Some((c, asc)) if c == i => (i, !asc),
                        _ => (i, true),
                    });
                }
                x += w;
            }
            let mut y = rect.min.y + row_h;
            p.hline(rect.x_range(), y, Stroke::new(1.0, t.border_strong));
            // Body.
            for (n, &ri) in order.iter().enumerate() {
                let Some(row) = table.rows.get(ri) else { continue };
                let r = Rect::from_min_size(egui::pos2(rect.min.x, y), Vec2::new(total_w, row_h));
                if n % 2 == 1 {
                    p.rect_filled(r, 0.0, t.panel);
                }
                draw_row(p, table, row, &widths, r, pad, locale, t.text, false);
                y += row_h;
            }
            // Footer (totals).
            if !table.footer.is_empty() {
                p.hline(rect.x_range(), y, Stroke::new(1.0, t.border_strong));
                for row in &table.footer {
                    let r = Rect::from_min_size(egui::pos2(rect.min.x, y), Vec2::new(total_w, row_h));
                    draw_row(p, table, row, &widths, r, pad, locale, t.text, true);
                    y += row_h;
                }
            }
            p.rect_stroke(
                Rect::from_min_max(rect.min, egui::pos2(rect.min.x + total_w, y)),
                2.0,
                Stroke::new(1.0, t.border),
                egui::StrokeKind::Inside,
            );
        });
    clicked
}

fn cell_anchor(cell: Rect, align: Align, pad: f32) -> (egui::Pos2, Align2) {
    match align {
        Align::Left => (egui::pos2(cell.min.x + pad, cell.center().y), Align2::LEFT_CENTER),
        Align::Centre => (cell.center(), Align2::CENTER_CENTER),
        Align::Right => (egui::pos2(cell.max.x - pad, cell.center().y), Align2::RIGHT_CENTER),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_row(
    p: &egui::Painter,
    table: &Table,
    row: &[Cell],
    widths: &[f32],
    r: Rect,
    pad: f32,
    locale: Locale,
    color: egui::Color32,
    bold: bool,
) {
    let mut x = r.min.x;
    for (i, (col, w)) in table.columns.iter().zip(widths.iter()).enumerate() {
        let cell = Rect::from_min_size(egui::pos2(x, r.min.y), Vec2::new(*w, r.height()));
        if let Some(c) = row.get(i) {
            let text = c.display(locale);
            if !text.is_empty() {
                let (pos, align) = cell_anchor(cell, col.align, pad);
                let size = crate::theme::size::BODY;
                let font = if bold {
                    FontId::new(size, egui::FontFamily::Name("strong".into()))
                } else {
                    FontId::proportional(size)
                };
                p.text(pos, align, text, font, color);
            }
        }
        x += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::table::Column;

    fn table() -> Table {
        Table {
            title: "t".into(),
            columns: vec![
                Column::new("Όνομα", Align::Left, 10.0),
                Column::new("Εμβαδόν", Align::Right, 10.0),
            ],
            rows: vec![
                vec![Cell::text("Ύπνος"), Cell::num2(12.0)],
                vec![Cell::text("αποθήκη"), Cell::num2(4.5)],
                vec![Cell::text("Καθιστικό"), Cell::Empty],
            ],
            footer: vec![],
        }
    }

    #[test]
    fn numbers_sort_numerically_with_empty_cells_last() {
        let t = table();
        assert_eq!(sorted_rows(&t, Some((1, true))), vec![1, 0, 2]);
        assert_eq!(sorted_rows(&t, Some((1, false))), vec![2, 0, 1]);
    }

    #[test]
    fn text_sorts_accent_and_case_insensitively() {
        let t = table();
        // αποθήκη < Καθιστικό < Ύπνος
        assert_eq!(sorted_rows(&t, Some((0, true))), vec![1, 2, 0]);
    }

    #[test]
    fn unsorted_keeps_model_order() {
        assert_eq!(sorted_rows(&table(), None), vec![0, 1, 2]);
    }
}
