//! The title block (υπόμνημα) of a sheet, laid out as Greek permit drawings expect: project, location, owner,
//! designer with a signature field, drawing title, scale, date, revision and sheet number.
//!
//! The block is [`TITLE_BLOCK_WIDTH`] mm wide so that it stays visible when a large sheet is folded to A4, and
//! sits in the lower-right corner of the drawing frame.

use std::collections::BTreeSet;

use kanavos_drawing::{Anchor, Drawing, Item, Stroke, Text, pt, weights};
use kanavos_i18n::t;
use kanavos_model::{Document, Sheet, ViewKind};
use kanavos_units::Locale;

use crate::FRAME_MARGIN;

/// Width of the title block, mm.
pub const TITLE_BLOCK_WIDTH: f64 = 180.0;
/// Height of the title block, mm.
pub const TITLE_BLOCK_HEIGHT: f64 = 76.0;

/// Average advance of a bold glyph as a fraction of the cap height, for fitting text (a little wider than the
/// estimate of [`Text::approx_width_mm`], because Greek capitals and bold faces run wide).
const GLYPH: f64 = 0.75;
/// Width of the label column of the title block, mm.
const LABEL_W: f64 = 24.0;

/// Text height that makes `text` fit `width` mm, between `max` and `min` mm. Returns the text, shortened with an
/// ellipsis when it does not fit at `min`.
pub(crate) fn fit(text: &str, width: f64, max: f64, min: f64) -> (String, f64) {
    let n = text.chars().count().max(1) as f64;
    let h = (width / (n * GLYPH)).min(max);
    if h >= min {
        return (text.to_owned(), h);
    }
    let keep = ((width / (min * GLYPH)).floor() as usize).saturating_sub(1);
    let short: String = text.chars().take(keep).collect();
    (format!("{short}…"), min)
}

/// Scale shown in the title block: the single scale of the sheet's model views, "as shown" when they differ,
/// or a dash when the sheet holds only tables.
pub fn sheet_scale_label(doc: &Document, s: &Sheet, locale: Locale) -> String {
    let scales: BTreeSet<u32> = s
        .viewports
        .iter()
        .filter_map(|vp| {
            let v = doc.view(vp.view)?;
            match v.kind {
                ViewKind::AreaTable | ViewKind::Schedule { .. } => None,
                _ => Some(vp.scale.unwrap_or(v.scale)),
            }
        })
        .collect();
    match scales.len() {
        0 => "—".to_owned(),
        1 => scales.first().map(|s| format!("1:{s}")).unwrap_or_default(),
        _ => t(locale, "sheets.tb.as_shown").into_owned(),
    }
}

struct Block {
    d: Drawing,
    x0: f64,
    x1: f64,
}

impl Block {
    fn line(&mut self, a: (f64, f64), b: (f64, f64), w: f64) {
        self.d.push(Item::line(pt(a.0, a.1), pt(b.0, b.1), Stroke::solid(w), "sheet.title_block"));
    }
    fn label(&mut self, x: f64, y_top: f64, text: &str) {
        let t = Text::new(pt(x + 1.5, y_top - 1.5), text, 1.8).anchored(Anchor::TopLeft);
        self.d.push(Item::text(t, "sheet.title_block"));
    }
    fn value(&mut self, x: f64, y: f64, width: f64, text: &str, max: f64, anchor: Anchor) {
        if text.is_empty() {
            return;
        }
        let (text, h) = fit(text, width - 3.0, max, 1.8);
        let mut t = Text::new(pt(x, y), text, h).anchored(anchor);
        t.bold = true;
        self.d.push(Item::text(t, "sheet.title_block"));
    }
    /// A full-width field: label in the label column, value beside it, both centred on the row.
    fn field(&mut self, y_top: f64, h: f64, label: &str, value: &str, size: f64) {
        let (x0, x1) = (self.x0, self.x1);
        let y = y_top - h / 2.0;
        let (label, lh) = fit(label, LABEL_W - 3.0, 2.0, 1.5);
        self.d.push(Item::text(
            Text::new(pt(x0 + 1.5, y), label, lh).anchored(Anchor::MiddleLeft),
            "sheet.title_block",
        ));
        self.value(x0 + LABEL_W + 2.0, y, x1 - x0 - LABEL_W - 2.0, value, size, Anchor::MiddleLeft);
        self.line((x0 + LABEL_W, y_top), (x0 + LABEL_W, y_top - h), weights::FINE);
        self.line((x0, y_top - h), (x1, y_top - h), weights::FINE);
    }
}

/// The title block in the lower-right corner of a sheet, in sheet millimetres.
pub fn title_block(doc: &Document, s: &Sheet, locale: Locale) -> Drawing {
    let (w, _) = s.paper.dimensions_mm();
    let info = doc.info();
    let x1 = w - FRAME_MARGIN;
    let x0 = x1 - TITLE_BLOCK_WIDTH;
    let y0 = FRAME_MARGIN;
    let top = y0 + TITLE_BLOCK_HEIGHT;
    let mut b = Block { d: Drawing::paper(), x0, x1 };
    b.d.push(Item::polyline(
        vec![pt(x0, y0), pt(x1, y0), pt(x1, top), pt(x0, top)],
        true,
        Stroke::solid(weights::THICK),
        "sheet.title_block",
    ));

    // Discipline header.
    let mut y = top;
    let mut head = Text::new(pt((x0 + x1) / 2.0, y - 4.0), t(locale, "sheets.tb.discipline"), 3.5)
        .anchored(Anchor::Centre);
    head.bold = true;
    b.d.push(Item::text(head, "sheet.title_block"));
    y -= 8.0;
    b.line((x0, y), (x1, y), weights::MEDIUM);

    b.field(y, 12.0, &t(locale, "sheets.tb.project"), &info.name, 3.5);
    y -= 12.0;
    let location = match (info.location.is_empty(), info.municipality.is_empty()) {
        (false, false) => format!("{}, {}", info.location, info.municipality),
        (false, true) => info.location.clone(),
        (true, _) => info.municipality.clone(),
    };
    b.field(y, 8.0, &t(locale, "sheets.tb.location"), &location, 2.8);
    y -= 8.0;
    b.field(y, 8.0, &t(locale, "sheets.tb.owner"), &info.owner, 2.8);
    y -= 8.0;

    // Designer with details beside the label column, signature field on the right.
    let split = x0 + 118.0;
    let h = 16.0;
    let (label, lh) = fit(&t(locale, "sheets.tb.designer"), LABEL_W - 3.0, 2.0, 1.5);
    b.d.push(Item::text(
        Text::new(pt(x0 + 1.5, y - 4.5), label, lh).anchored(Anchor::MiddleLeft),
        "sheet.title_block",
    ));
    let vx = x0 + LABEL_W + 2.0;
    b.value(vx, y - 4.5, split - vx, &info.designer, 3.0, Anchor::MiddleLeft);
    for (i, line) in info.designer_details.lines().take(2).enumerate() {
        let (text, size) = fit(line, split - vx - 2.0, 2.0, 1.5);
        let t = Text::new(pt(vx, y - 9.5 - 3.5 * i as f64), text, size).anchored(Anchor::MiddleLeft);
        b.d.push(Item::text(t, "sheet.title_block"));
    }
    b.line((x0 + LABEL_W, y), (x0 + LABEL_W, y - h), weights::FINE);
    b.label(split, y, &t(locale, "sheets.tb.signature"));
    b.line((split, y), (split, y - h), weights::FINE);
    y -= h;
    b.line((x0, y), (x1, y), weights::FINE);

    b.field(y, 10.0, &t(locale, "sheets.tb.drawing"), &s.name, 3.5);
    y -= 10.0;
    b.line((x0, y), (x1, y), weights::MEDIUM);

    // Bottom row: scale, date, revision, sheet number.
    let cells: [(f64, String, String, f64); 4] = [
        (30.0, t(locale, "sheets.tb.scale").into_owned(), sheet_scale_label(doc, s, locale), 3.0),
        (55.0, t(locale, "sheets.tb.date").into_owned(), info.date.clone(), 2.8),
        (30.0, t(locale, "sheets.tb.revision").into_owned(), info.revision.clone(), 3.0),
        (65.0, t(locale, "sheets.tb.number").into_owned(), s.number.clone(), 5.0),
    ];
    let mut x = x0;
    for (i, (cw, label, value, size)) in cells.iter().enumerate() {
        b.label(x, y, label);
        b.value(x + cw / 2.0, y0 + 3.0, *cw, value, *size, Anchor::BottomCentre);
        x += cw;
        if i + 1 < cells.len() {
            b.line((x, y), (x, y0), weights::FINE);
        }
    }
    b.d
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::{Paper, PaperSize, starter};

    #[test]
    fn long_values_shrink_then_shorten() {
        let (s, h) = fit("Νέα κατοικία", 100.0, 3.5, 1.8);
        assert_eq!(s, "Νέα κατοικία");
        assert!((h - 3.5).abs() < 1e-9);
        let long = "Α".repeat(200);
        let (s, h) = fit(&long, 100.0, 3.5, 1.8);
        assert!(s.ends_with('…') && (h - 1.8).abs() < 1e-9);
        assert!(s.chars().count() as f64 * 1.8 * 0.6 <= 100.0 + 1e-9);
    }

    #[test]
    fn block_sits_inside_the_frame_corner() {
        let doc = starter();
        let sheet = Sheet {
            id: kanavos_model::Id(1),
            number: "Α-01".into(),
            name: "Κάτοψη".into(),
            paper: Paper { size: PaperSize::A3, landscape: true },
            viewports: vec![],
            title_block: true,
        };
        let d = title_block(&doc, &sheet, Locale::El);
        let b = d.bounds().unwrap();
        assert!(b.max.x <= 420.0 - FRAME_MARGIN + 1e-9 && b.min.y >= FRAME_MARGIN - 1e-9);
        assert!((b.width() - TITLE_BLOCK_WIDTH).abs() < 1e-9);
        assert!((b.height() - TITLE_BLOCK_HEIGHT).abs() < 1e-9);
        assert_eq!(sheet_scale_label(&doc, &sheet, Locale::El), "—");
    }
}
