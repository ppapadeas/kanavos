//! Automatic sheet layout: pack viewports in rows and pick the smallest A-series sheet that holds them.
//!
//! Every view occupies a box on paper: its drawing's extent at the viewport scale plus a band underneath for the
//! title and scale ([`LABEL_BAND`]) and a margin around it ([`GAP`]). Boxes are placed in rows from the top of
//! the frame, left to right in the given order; rows are centred in the width available to them. The title
//! block occupies the lower-right corner of the frame, so a row that reaches down beside it is narrower.

use kanavos_drawing::DrawingUnits;
use kanavos_model::{Document, Id, Paper, PaperSize, Viewport};
use kanavos_units::Locale;

use crate::{BINDING_MARGIN, FRAME_MARGIN, SheetError, TITLE_BLOCK_HEIGHT, TITLE_BLOCK_WIDTH, render_view};

/// Height of the band under a viewport that holds its title and scale, mm.
pub const LABEL_BAND: f64 = 16.0;
/// Clear space around each viewport box, mm.
const GAP: f64 = 8.0;
/// Extra space around a drawing's visible extent, mm.
const PAD: f64 = 3.0;

/// Sheets tried by [`auto_layout`], smallest first.
pub const PAPER_CANDIDATES: [Paper; 9] = [
    Paper { size: PaperSize::A4, landscape: false },
    Paper { size: PaperSize::A3, landscape: true },
    Paper { size: PaperSize::A3, landscape: false },
    Paper { size: PaperSize::A2, landscape: true },
    Paper { size: PaperSize::A2, landscape: false },
    Paper { size: PaperSize::A1, landscape: true },
    Paper { size: PaperSize::A1, landscape: false },
    Paper { size: PaperSize::A0, landscape: true },
    Paper { size: PaperSize::A0, landscape: false },
];

/// Size of a view on paper (width, height in mm) at its own scale, without the label band; `None` when the
/// view draws nothing. Views that cannot be drawn yet take the size of their placeholder.
pub fn view_paper_size(doc: &Document, view: Id, locale: Locale) -> Result<Option<(f64, f64)>, SheetError> {
    let v = doc.view(view).ok_or(SheetError::NotFound(view))?;
    let d = match render_view(doc, view, locale) {
        Ok(d) => d,
        Err(SheetError::Unsupported(_)) => return Ok(Some((120.0, 80.0))),
        Err(e) => return Err(e),
    };
    let Some(b) = crate::extent(&d) else { return Ok(None) };
    let k = match d.units {
        DrawingUnits::Metres => 1000.0 / f64::from(v.scale.max(1)),
        DrawingUnits::Millimetres => 1.0,
    };
    Ok(Some((b.width() * k + 2.0 * PAD, b.height() * k + 2.0 * PAD)))
}

/// The drawing frame of a paper: (left, bottom, right, top) in mm.
pub fn paper_extent(paper: &Paper) -> (f64, f64, f64, f64) {
    let (w, h) = paper.dimensions_mm();
    (BINDING_MARGIN, FRAME_MARGIN, w - FRAME_MARGIN, h - FRAME_MARGIN)
}

/// Places boxes (width, height in mm, label band included) on a paper. Returns the centre of each box, or `None`
/// when they do not fit.
fn pack(boxes: &[(f64, f64)], paper: &Paper) -> Option<Vec<[f64; 2]>> {
    let (left, bottom, right, top) = paper_extent(paper);
    let tb_left = right - TITLE_BLOCK_WIDTH;
    let tb_top = bottom + TITLE_BLOCK_HEIGHT;
    // Width available to a row whose lowest point is `y`.
    let span = |y: f64| -> (f64, f64) {
        if y < tb_top + GAP { (left + GAP, tb_left - GAP) } else { (left + GAP, right - GAP) }
    };
    struct Row {
        top: f64,
        height: f64,
        items: Vec<(usize, f64, f64)>,
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut y = top - GAP;
    for (i, &(w, h)) in boxes.iter().enumerate() {
        let placed = rows.last_mut().is_some_and(|row| {
            let height = row.height.max(h);
            let (x0, x1) = span(row.top - height);
            let used: f64 = row.items.iter().map(|(_, w, _)| w + GAP).sum();
            if used + w <= x1 - x0 + 1e-9 {
                row.height = height;
                row.items.push((i, w, h));
                true
            } else {
                false
            }
        });
        if !placed {
            if let Some(row) = rows.last() {
                y = row.top - row.height - GAP;
            }
            let (x0, x1) = span(y - h);
            if w > x1 - x0 + 1e-9 {
                return None;
            }
            rows.push(Row { top: y, height: h, items: vec![(i, w, h)] });
        }
    }
    // Every row must stay inside the frame and clear of the title block.
    let ok = |shift: f64| {
        rows.iter().all(|r| {
            let low = r.top - r.height - shift;
            let (x0, x1) = span(low);
            let used: f64 =
                r.items.iter().map(|(_, w, _)| w).sum::<f64>() + GAP * r.items.len().saturating_sub(1) as f64;
            low >= bottom + GAP - 1e-9 && used <= x1 - x0 + 1e-9
        })
    };
    if !ok(0.0) {
        return None;
    }
    // Centre the rows vertically when that keeps them clear of the title block.
    let lowest = rows.last().map(|r| r.top - r.height).unwrap_or(top);
    let free = (lowest - (bottom + GAP)).max(0.0);
    let shift = if ok(free / 2.0) { free / 2.0 } else { 0.0 };
    let mut centres = vec![[0.0, 0.0]; boxes.len()];
    for r in &rows {
        let (x0, x1) = span(r.top - r.height - shift);
        let used: f64 =
            r.items.iter().map(|(_, w, _)| w).sum::<f64>() + GAP * r.items.len().saturating_sub(1) as f64;
        let mut x = x0 + ((x1 - x0) - used) / 2.0;
        for &(i, w, h) in &r.items {
            if let Some(c) = centres.get_mut(i) {
                *c = [x + w / 2.0, r.top - shift - h / 2.0];
            }
            x += w + GAP;
        }
    }
    Some(centres)
}

/// Whether boxes of these sizes (drawing only; the label band is added) fit on a paper.
pub fn fits(sizes: &[(f64, f64)], paper: &Paper) -> bool {
    let boxes: Vec<(f64, f64)> = sizes.iter().map(|(w, h)| (*w, h + LABEL_BAND)).collect();
    pack(&boxes, paper).is_some()
}

/// The smallest paper among `candidates` that holds drawings of these sizes, with the centre of each drawing
/// (above its label band) in sheet millimetres.
pub(crate) fn place(sizes: &[(f64, f64)], candidates: &[Paper]) -> Option<(Paper, Vec<[f64; 2]>)> {
    let boxes: Vec<(f64, f64)> = sizes.iter().map(|(w, h)| (*w, h + LABEL_BAND)).collect();
    candidates.iter().find_map(|p| {
        pack(&boxes, p).map(|c| (*p, c.into_iter().map(|[x, y]| [x, y + LABEL_BAND / 2.0]).collect()))
    })
}

/// Places drawings in one row from the top-left of a large sheet, for content that fits nowhere.
pub(crate) fn place_overflowing(sizes: &[(f64, f64)]) -> (Paper, Vec<[f64; 2]>) {
    let paper = Paper { size: PaperSize::A0, landscape: true };
    let (left, _, _, top) = paper_extent(&paper);
    let mut x = left + GAP;
    let centres = sizes
        .iter()
        .map(|(w, h)| {
            let c = [x + w / 2.0, top - GAP - h / 2.0];
            x += w + GAP;
            c
        })
        .collect();
    (paper, centres)
}

/// Viewports at the views' own scales with the given centres.
pub(crate) fn viewports(views: &[Id], centres: Vec<[f64; 2]>) -> Vec<Viewport> {
    views
        .iter()
        .zip(centres)
        .map(|(v, centre)| Viewport { view: *v, centre, scale: None, title: None, crop: None })
        .collect()
}

/// Paper sizes of views, with a small default for views that draw nothing.
pub(crate) fn sizes_of(doc: &Document, views: &[Id], locale: Locale) -> Result<Vec<(f64, f64)>, SheetError> {
    views.iter().map(|v| Ok(view_paper_size(doc, *v, locale)?.unwrap_or((40.0, 20.0)))).collect()
}

/// Lays out views on the smallest paper in [`PAPER_CANDIDATES`] that holds them all, in order. When even the
/// largest does not, the views go in one row on the largest landscape sheet anyway (they overrun the frame).
///
/// Returns the paper and one viewport per view (at the view's scale; the drawing sits above its label band).
pub fn auto_layout(
    doc: &Document,
    views: &[Id],
    locale: Locale,
) -> Result<(Paper, Vec<Viewport>), SheetError> {
    let sizes = sizes_of(doc, views, locale)?;
    let (paper, centres) = place(&sizes, &PAPER_CANDIDATES).unwrap_or_else(|| place_overflowing(&sizes));
    Ok((paper, viewports(views, centres)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_view_fits_a4_and_a_large_one_needs_a_bigger_sheet() {
        let a4 = Paper { size: PaperSize::A4, landscape: false };
        assert!(fits(&[(150.0, 120.0)], &a4));
        assert!(!fits(&[(300.0, 200.0)], &a4));
        assert!(fits(&[(300.0, 150.0)], &Paper { size: PaperSize::A3, landscape: true }));
        // Too tall to clear the title block at that width.
        assert!(!fits(&[(300.0, 200.0)], &Paper { size: PaperSize::A3, landscape: true }));
    }

    #[test]
    fn rows_avoid_the_title_block() {
        let paper = Paper { size: PaperSize::A3, landscape: true };
        let boxes = [(180.0, 100.0), (180.0, 100.0), (180.0, 100.0)];
        let centres = pack(&boxes, &paper).unwrap();
        let (_, bottom, right, _) = paper_extent(&paper);
        for (c, (w, h)) in centres.iter().zip(boxes) {
            let (x1, y0) = (c[0] + w / 2.0, c[1] - h / 2.0);
            let clear = x1 <= right - TITLE_BLOCK_WIDTH || y0 >= bottom + TITLE_BLOCK_HEIGHT;
            assert!(clear, "box at {c:?} overlaps the title block");
            assert!(y0 >= bottom - 1e-9);
        }
    }
}
