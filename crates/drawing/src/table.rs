//! Tables (schedules, area tables) as data, with CSV and paper-space drawing output.

use kanavos_units::{Locale, format_decimal};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Anchor, Drawing, Item, Pt, Stroke, Text, pt, weights};

/// Horizontal alignment of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    /// Left.
    #[default]
    Left,
    /// Centre.
    Centre,
    /// Right (numbers).
    Right,
}

/// One table cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Cell {
    /// Text.
    Text(String),
    /// A number shown with a fixed number of decimals in the locale's format.
    Number {
        /// Value.
        value: f64,
        /// Decimals shown.
        decimals: u8,
    },
    /// Nothing.
    Empty,
}

impl Cell {
    /// A text cell.
    pub fn text(s: impl Into<String>) -> Cell {
        Cell::Text(s.into())
    }
    /// A number cell with two decimals.
    pub fn num2(value: f64) -> Cell {
        Cell::Number { value, decimals: 2 }
    }
    /// An integer cell.
    pub fn int(value: i64) -> Cell {
        Cell::Number { value: value as f64, decimals: 0 }
    }
    /// The cell as displayed.
    pub fn display(&self, locale: Locale) -> String {
        match self {
            Cell::Text(s) => s.clone(),
            Cell::Number { value, decimals } => format_decimal(*value, usize::from(*decimals), locale),
            Cell::Empty => String::new(),
        }
    }
}

/// A column definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Column {
    /// Header text.
    pub header: String,
    /// Alignment.
    #[serde(default)]
    pub align: Align,
    /// Printed width in paper millimetres.
    pub width_mm: f64,
}

impl Column {
    /// A column.
    pub fn new(header: impl Into<String>, align: Align, width_mm: f64) -> Column {
        Column { header: header.into(), align, width_mm }
    }
}

/// A table: title, columns, body rows and footer rows (totals).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Table {
    /// Title printed above the table.
    pub title: String,
    /// Columns.
    pub columns: Vec<Column>,
    /// Body rows; each row has one cell per column (missing cells print empty).
    pub rows: Vec<Vec<Cell>>,
    /// Footer rows, printed bold under a rule (totals).
    #[serde(default)]
    pub footer: Vec<Vec<Cell>>,
}

/// Printed sizes of a table, in paper millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableStyle {
    /// Body text height.
    pub text_mm: f64,
    /// Row height.
    pub row_mm: f64,
    /// Title text height.
    pub title_mm: f64,
}

impl Default for TableStyle {
    fn default() -> Self {
        TableStyle { text_mm: 2.0, row_mm: 5.0, title_mm: 3.5 }
    }
}

fn csv_escape(s: &str, sep: char) -> String {
    if s.contains(sep) || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

impl Table {
    /// Total printed width.
    pub fn width_mm(&self) -> f64 {
        self.columns.iter().map(|c| c.width_mm).sum()
    }

    /// Total printed height including the title.
    pub fn height_mm(&self, style: &TableStyle) -> f64 {
        let title = if self.title.is_empty() { 0.0 } else { style.title_mm * 2.0 };
        title + style.row_mm * (1 + self.rows.len() + self.footer.len()) as f64
    }

    /// CSV text. Greek locale uses `;` as separator (decimal comma), English uses `,`. Starts with a UTF-8 BOM
    /// so spreadsheet programs detect the encoding.
    pub fn to_csv(&self, locale: Locale) -> String {
        let sep = match locale {
            Locale::El => ';',
            Locale::En => ',',
        };
        let mut out = String::from('\u{feff}');
        let line = |cells: Vec<String>| {
            cells.iter().map(|c| csv_escape(c, sep)).collect::<Vec<_>>().join(&sep.to_string())
        };
        out.push_str(&line(self.columns.iter().map(|c| c.header.clone()).collect()));
        out.push_str("\r\n");
        for row in self.rows.iter().chain(self.footer.iter()) {
            out.push_str(&line(row.iter().map(|c| c.display(locale)).collect()));
            out.push_str("\r\n");
        }
        out
    }

    /// A paper-space drawing of the table with its top-left corner at `origin` (millimetres, Y up, so the table
    /// grows downwards).
    pub fn to_drawing(&self, origin: Pt, style: &TableStyle, locale: Locale) -> Drawing {
        let mut d = Drawing::paper();
        let mut y = origin.y;
        let width = self.width_mm();
        if !self.title.is_empty() {
            let mut t = Text::new(pt(origin.x, y - style.title_mm * 1.4), self.title.clone(), style.title_mm);
            t.bold = true;
            d.push(Item::text(t, "table.title"));
            y -= style.title_mm * 2.0;
        }
        let thin = Stroke::solid(weights::FINE);
        let thick = Stroke::solid(weights::MEDIUM);
        let row_count = 1 + self.rows.len() + self.footer.len();
        let bottom = y - style.row_mm * row_count as f64;
        // Outer frame and column rules.
        d.push(Item::polyline(
            vec![
                pt(origin.x, y),
                pt(origin.x + width, y),
                pt(origin.x + width, bottom),
                pt(origin.x, bottom),
            ],
            true,
            thick.clone(),
            "table.grid",
        ));
        let mut x = origin.x;
        for c in self.columns.iter().take(self.columns.len().saturating_sub(1)) {
            x += c.width_mm;
            d.push(Item::line(pt(x, y), pt(x, bottom), thin.clone(), "table.grid"));
        }
        let mut row_y = y;
        let draw_row = |d: &mut Drawing, cells: &[Cell], row_y: f64, bold: bool, header: bool| {
            let mut x = origin.x;
            for (i, col) in self.columns.iter().enumerate() {
                let text = if header {
                    col.header.clone()
                } else {
                    cells.get(i).map(|c| c.display(locale)).unwrap_or_default()
                };
                if !text.is_empty() {
                    let pad = 1.0;
                    let (px, anchor) = match col.align {
                        Align::Left => (x + pad, Anchor::MiddleLeft),
                        Align::Centre => (x + col.width_mm / 2.0, Anchor::Centre),
                        Align::Right => (x + col.width_mm - pad, Anchor::MiddleRight),
                    };
                    let mut t =
                        Text::new(pt(px, row_y - style.row_mm / 2.0), text, style.text_mm).anchored(anchor);
                    t.bold = bold;
                    d.push(Item::text(t, if header { "table.header" } else { "table.text" }));
                }
                x += col.width_mm;
            }
        };
        draw_row(&mut d, &[], row_y, true, true);
        row_y -= style.row_mm;
        d.push(Item::line(pt(origin.x, row_y), pt(origin.x + width, row_y), thick.clone(), "table.grid"));
        for row in &self.rows {
            draw_row(&mut d, row, row_y, false, false);
            row_y -= style.row_mm;
            d.push(Item::line(pt(origin.x, row_y), pt(origin.x + width, row_y), thin.clone(), "table.grid"));
        }
        if !self.footer.is_empty() {
            d.push(Item::line(pt(origin.x, row_y), pt(origin.x + width, row_y), thick, "table.grid"));
            for row in &self.footer {
                draw_row(&mut d, row, row_y, true, false);
                row_y -= style.row_mm;
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Table {
        Table {
            title: "Πίνακας χώρων".into(),
            columns: vec![
                Column::new("Χώρος", Align::Left, 40.0),
                Column::new("Εμβαδόν", Align::Right, 20.0),
            ],
            rows: vec![vec![Cell::text("Καθιστικό; κουζίνα"), Cell::num2(32.456)]],
            footer: vec![vec![Cell::text("Σύνολο"), Cell::num2(32.456)]],
        }
    }

    #[test]
    fn csv_uses_locale_separators() {
        let csv = sample().to_csv(Locale::El);
        assert!(csv.contains("\"Καθιστικό; κουζίνα\";32,46"));
        let csv = sample().to_csv(Locale::En);
        assert!(csv.contains("Καθιστικό; κουζίνα,32.46"));
    }

    #[test]
    fn drawing_has_text_for_every_cell() {
        let t = sample();
        let d = t.to_drawing(pt(0.0, 0.0), &TableStyle::default(), Locale::El);
        let texts = d.items.iter().filter(|i| matches!(i.shape, crate::Shape::Text(_))).count();
        assert_eq!(texts, 1 + 2 + 2 + 2);
        assert!((t.width_mm() - 60.0).abs() < 1e-9);
    }
}
