//! Sheets, title blocks, viewports and sheet sets.
//!
//! Layer L6. See `docs/architecture.md`.
//!
//! [`render_view`] is the single entry point that turns any view into a drawing (plans, sections, elevations and
//! 3D line drawings from `kanavos-views`, regulation views from `kanavos-rules-gr`, tables from
//! `kanavos-schedules`). [`sheet_drawing`] composes a sheet in paper millimetres: frame with a binding margin,
//! viewports at scale with titles, and the title block. [`auto_layout`] packs views on the smallest A-series
//! sheet that holds them, and [`permit_set`] builds the whole permit drawing set. Behaviour is specified in
//! `specs/sheets/`.

mod crop;
mod layout;
mod permit;
mod title_block;

use kanavos_drawing::table::{Table, TableStyle};
use kanavos_drawing::{
    Affine, Anchor, Drawing, DrawingUnits, Item, LinePattern, Rect, Stroke, Text, pt, weights,
};
use kanavos_i18n::t;
use kanavos_model::{Document, Id, View, ViewKind, Viewport};
use kanavos_units::Locale;
use kanavos_views::ViewError;

pub use crop::crop;
pub use layout::{LABEL_BAND, PAPER_CANDIDATES, auto_layout, fits, paper_extent, view_paper_size};
pub use permit::{PermitViews, ensure_permit_views, permit_set};
pub use title_block::{TITLE_BLOCK_HEIGHT, TITLE_BLOCK_WIDTH, sheet_scale_label, title_block};

/// Margin of the drawing frame from the paper edge on the top, right and bottom, mm.
pub const FRAME_MARGIN: f64 = 10.0;
/// Margin of the drawing frame from the left paper edge, leaving room for binding, mm.
pub const BINDING_MARGIN: f64 = 20.0;

/// Why a sheet or view could not be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetError {
    /// The sheet or view does not exist.
    NotFound(Id),
    /// The view cannot be drawn in 2D (e.g. a 3D view without a line rendering).
    Unsupported(Id),
    /// Deriving the view failed.
    View(ViewError),
}

impl std::fmt::Display for SheetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SheetError::NotFound(id) => write!(f, "sheet or view {id} not found"),
            SheetError::Unsupported(id) => write!(f, "view {id} cannot be placed on a sheet yet"),
            SheetError::View(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SheetError {}

impl From<ViewError> for SheetError {
    fn from(e: ViewError) -> Self {
        SheetError::View(e)
    }
}

/// Visible extent of a drawing in its own coordinates, with text measured by its full box in the fonts it will be
/// printed with (`kanavos_drawing::page::content_bounds`).
pub(crate) fn extent(d: &Drawing) -> Option<Rect> {
    kanavos_drawing::page::content_bounds(d, kanavos_drawing::fonts::Fonts::system())
}

/// Widens table columns so that headers and cells fit their text (measured, with 1.5 mm padding each side).
pub fn fit_columns(table: &mut Table, style: &TableStyle, locale: Locale) {
    let measure = |s: &str, bold: bool| kanavos_drawing::text::measure_mm(s, style.text_mm, bold) + 3.0;
    for (i, c) in table.columns.iter_mut().enumerate() {
        let mut w = measure(&c.header, true);
        for row in &table.rows {
            if let Some(cell) = row.get(i) {
                w = w.max(measure(&cell.display(locale), false));
            }
        }
        for row in &table.footer {
            if let Some(cell) = row.get(i) {
                w = w.max(measure(&cell.display(locale), true));
            }
        }
        c.width_mm = c.width_mm.max(w);
    }
}

/// Stacks paper-space tables vertically with a gap, top-left at the origin, columns widened to their text.
fn stack_tables(tables: &[Table], locale: Locale) -> Drawing {
    let style = TableStyle::default();
    let mut d = Drawing::paper();
    let mut y = 0.0;
    for table in tables {
        let mut table = table.clone();
        fit_columns(&mut table, &style, locale);
        d.extend(table.to_drawing(pt(0.0, y), &style, locale));
        y -= table.height_mm(&style) + 8.0;
    }
    d
}

/// Draws any view. Model-space views come back in metres at the view's scale; tables come back in paper
/// millimetres (check `Drawing::units`).
///
/// 3D views are drawn by `kanavos_views::view_drawing` as line drawings; while that is not available the call
/// fails with [`SheetError::Unsupported`], and [`sheet_drawing`] shows a framed placeholder instead.
pub fn render_view(doc: &Document, view: Id, locale: Locale) -> Result<Drawing, SheetError> {
    let v = doc.view(view).ok_or(SheetError::NotFound(view))?;
    let scale = f64::from(v.scale.max(1));
    Ok(match &v.kind {
        ViewKind::Plan { .. } | ViewKind::Section { .. } | ViewKind::Elevation { .. } => {
            kanavos_views::view_drawing(doc, view, locale)?
        }
        ViewKind::ThreeD { .. } => match kanavos_views::view_drawing(doc, view, locale) {
            Ok(d) => d,
            Err(ViewError::WrongKind(_)) => return Err(SheetError::Unsupported(view)),
            Err(e) => return Err(e.into()),
        },
        ViewKind::CoverageDiagram => {
            let mut d = kanavos_rules_gr::coverage_diagram(doc, locale, scale);
            kanavos_views::filter_layers(&mut d, &v.hidden_layers);
            d
        }
        ViewKind::SitePlan => {
            let mut d = kanavos_rules_gr::site_plan(doc, locale, scale);
            kanavos_views::filter_layers(&mut d, &v.hidden_layers);
            d
        }
        ViewKind::AreaTable => {
            let report = kanavos_rules_gr::nok_report(doc);
            stack_tables(
                &[
                    kanavos_rules_gr::terms_table_of(doc, &report, locale),
                    kanavos_rules_gr::area_table_of(doc, &report, locale),
                ],
                locale,
            )
        }
        ViewKind::Schedule { schedule } => {
            stack_tables(&[kanavos_schedules::schedule(doc, *schedule, locale)], locale)
        }
    })
}

/// The view's drawing as placed on a sheet: cropped to the viewport's crop rectangle (model views only).
pub fn viewport_drawing(doc: &Document, vp: &Viewport, locale: Locale) -> Result<Drawing, SheetError> {
    let d = render_view(doc, vp.view, locale)?;
    Ok(match (vp.crop, d.units) {
        (Some([a, b]), DrawingUnits::Metres) => {
            let ([ax, ay], [bx, by]) = (a.xy(), b.xy());
            crop(&d, pt(ax, ay), pt(bx, by))
        }
        _ => d,
    })
}

/// Map from a view's drawing coordinates to sheet millimetres for a viewport: the centre of the drawing's
/// bounds goes to the viewport centre, at the viewport's scale (or the view's).
pub fn viewport_map(drawing: &Drawing, vp: &Viewport, view: &View) -> Affine {
    let bounds = extent(drawing);
    match drawing.units {
        DrawingUnits::Metres => {
            let scale = f64::from(vp.scale.unwrap_or(view.scale)).max(1.0);
            let k = 1000.0 / scale;
            let c = bounds.map(|b| b.centre()).unwrap_or_default();
            Affine::scale_translate(k, vp.centre[0] - c.x * k, vp.centre[1] - c.y * k)
        }
        DrawingUnits::Millimetres => {
            let c = bounds.map(|b| b.centre()).unwrap_or_default();
            Affine::scale_translate(1.0, vp.centre[0] - c.x, vp.centre[1] - c.y)
        }
    }
}

/// Title and scale label under a viewport whose drawing ends at `bottom` (sheet mm).
fn viewport_label(d: &mut Drawing, x: f64, bottom: f64, title: &str, scale: Option<u32>, locale: Locale) {
    let mut text = Text::new(pt(x, bottom - 4.0), title, 3.5).anchored(Anchor::TopCentre);
    text.bold = true;
    let half = (text.width_mm() / 2.0 + 1.0).max(10.0);
    d.push(Item::text(text, "sheet.viewport_title"));
    let y = bottom - 9.0;
    d.push(Item::line(
        pt(x - half, y),
        pt(x + half, y),
        Stroke::solid(weights::MEDIUM),
        "sheet.viewport_title",
    ));
    if let Some(s) = scale {
        let label = format!("{} 1:{s}", t(locale, "sheets.scale"));
        let text = Text::new(pt(x, y - 1.5), label, 2.5).anchored(Anchor::TopCentre);
        d.push(Item::text(text, "sheet.viewport_title"));
    }
}

/// A framed placeholder for a view that cannot be drawn yet (e.g. 3D before line drawings exist).
fn placeholder(d: &mut Drawing, centre: [f64; 2], locale: Locale) -> f64 {
    let (w, h) = (120.0, 80.0);
    let [cx, cy] = centre;
    d.push(Item::polyline(
        vec![
            pt(cx - w / 2.0, cy - h / 2.0),
            pt(cx + w / 2.0, cy - h / 2.0),
            pt(cx + w / 2.0, cy + h / 2.0),
            pt(cx - w / 2.0, cy + h / 2.0),
        ],
        true,
        Stroke::patterned(weights::THIN, LinePattern::Dashed),
        "sheet.placeholder",
    ));
    d.push(Item::text(
        Text::new(pt(cx, cy), t(locale, "sheets.view_unavailable"), 3.0).anchored(Anchor::Centre),
        "sheet.placeholder",
    ));
    cy - h / 2.0
}

/// North arrow beside a plan viewport, rotated to grid north.
fn plan_north_arrow(doc: &Document, d: &mut Drawing, b: &Rect, locale: Locale) {
    let rotation = doc.site().georef.rotation.as_rad();
    let at = pt(b.max.x + 12.0, b.max.y - 12.0);
    d.extend(kanavos_rules_gr::north_arrow(at, 8.0, rotation, locale));
}

/// Composes a sheet: frame, every viewport with its title and scale, and the title block, in paper millimetres.
pub fn sheet_drawing(doc: &Document, sheet: Id, locale: Locale) -> Result<Drawing, SheetError> {
    let s = doc.sheet(sheet).ok_or(SheetError::NotFound(sheet))?;
    let (w, h) = s.paper.dimensions_mm();
    let mut d = Drawing::paper();
    d.push(Item::polyline(
        vec![
            pt(BINDING_MARGIN, FRAME_MARGIN),
            pt(w - FRAME_MARGIN, FRAME_MARGIN),
            pt(w - FRAME_MARGIN, h - FRAME_MARGIN),
            pt(BINDING_MARGIN, h - FRAME_MARGIN),
        ],
        true,
        Stroke::solid(weights::EXTRA_THICK),
        "sheet.border",
    ));
    for vp in &s.viewports {
        let Some(view) = doc.view(vp.view) else { continue };
        let title = vp.title.clone().unwrap_or_else(|| view.name.clone());
        let drawing = match viewport_drawing(doc, vp, locale) {
            Ok(drawing) => drawing,
            Err(_) => {
                let bottom = placeholder(&mut d, vp.centre, locale);
                viewport_label(&mut d, vp.centre[0], bottom, &title, None, locale);
                continue;
            }
        };
        let m = viewport_map(&drawing, vp, view);
        let placed = drawing.transformed(&m);
        let bounds = extent(&placed);
        let bottom = bounds.map(|b| b.min.y).unwrap_or(vp.centre[1]);
        let is_model = drawing.units == DrawingUnits::Metres;
        d.items.extend(placed.items);
        if let (ViewKind::Plan { .. }, Some(b)) = (&view.kind, bounds) {
            plan_north_arrow(doc, &mut d, &b, locale);
        }
        // Tables carry their own title; model views get a title and scale underneath.
        if is_model {
            let scale = vp.scale.unwrap_or(view.scale);
            viewport_label(&mut d, vp.centre[0], bottom, &title, Some(scale), locale);
        }
    }
    if s.title_block {
        d.extend(title_block(doc, s, locale));
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::{Paper, PaperSize, Sheet, starter};

    #[test]
    fn every_view_kind_renders_or_degrades() {
        let doc = starter();
        for v in doc.views() {
            match render_view(&doc, v.id, Locale::El) {
                Ok(_) => {}
                Err(SheetError::Unsupported(id)) => {
                    assert!(matches!(doc.view(id).map(|v| &v.kind), Some(ViewKind::ThreeD { .. })))
                }
                Err(e) => panic!("{} failed: {e}", v.name),
            }
        }
    }

    #[test]
    fn three_d_viewport_draws_the_model() {
        let doc = starter();
        let three_d = doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD { .. })).map(|v| v.id).unwrap();
        let storey = doc.storeys()[0].id;
        let (doc, wall) = doc.with_new_id();
        let structure = kanavos_model::Structure::Layered { build_up: doc.defaults().exterior_wall.unwrap() };
        let doc = doc.with_element(kanavos_model::Element::on_storey(
            wall,
            storey,
            kanavos_model::ElementKind::Wall(kanavos_model::Wall {
                start: kanavos_geom::Point2::new(0.0, 0.0),
                end: kanavos_geom::Point2::new(6.0, 0.0),
                bulge: 0.0,
                reference: kanavos_model::ReferenceLine::Centre,
                structure,
                height: kanavos_units::Length::m(3.0),
                base_offset: kanavos_units::Length::ZERO,
            }),
        ));
        let (doc, id) = doc.with_new_id();
        let doc = doc.with_sheet(Sheet {
            id,
            number: "Α-09".into(),
            name: "3D".into(),
            paper: Paper { size: PaperSize::A3, landscape: true },
            viewports: vec![Viewport {
                view: three_d,
                centre: [200.0, 180.0],
                scale: None,
                title: None,
                crop: None,
            }],
            title_block: true,
        });
        let d = sheet_drawing(&doc, id, Locale::El).unwrap();
        assert_eq!(d.layer_items("sheet.placeholder").count(), 0);
        assert!(d.items.iter().any(|i| i.source == Some(wall.0)), "the wall is drawn in the 3D viewport");
    }
}
