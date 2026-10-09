//! Sheets, title blocks, viewports and sheet sets.
//!
//! Layer L6. See `docs/architecture.md`.
//!
//! [`render_view`] is the single entry point that turns any 2D view into a drawing (plans, sections and
//! elevations from `kanavos-views`, regulation views from `kanavos-rules-gr`, tables from `kanavos-schedules`).
//! [`sheet_drawing`] composes a sheet in paper millimetres: border, title block and viewports at scale.

use kanavos_drawing::table::TableStyle;
use kanavos_drawing::{Affine, Anchor, Drawing, DrawingUnits, Item, Stroke, Text, pt, weights};
use kanavos_i18n::t;
use kanavos_model::{Document, Id, Paper, PaperSize, Sheet, View, ViewKind, Viewport};
use kanavos_units::Locale;
use kanavos_views::ViewError;

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

/// Draws any 2D view. Model-space views come back in metres at the view's scale; tables come back in paper
/// millimetres (check `Drawing::units`).
pub fn render_view(doc: &Document, view: Id, locale: Locale) -> Result<Drawing, SheetError> {
    let v = doc.view(view).ok_or(SheetError::NotFound(view))?;
    let scale = f64::from(v.scale);
    Ok(match &v.kind {
        ViewKind::Plan { .. } | ViewKind::Section { .. } | ViewKind::Elevation { .. } => {
            kanavos_views::view_drawing(doc, view, locale)?
        }
        ViewKind::CoverageDiagram | ViewKind::SitePlan => {
            kanavos_rules_gr::coverage_diagram(doc, locale, scale)
        }
        ViewKind::AreaTable => {
            kanavos_rules_gr::area_table(doc, locale).to_drawing(pt(0.0, 0.0), &TableStyle::default(), locale)
        }
        ViewKind::Schedule { schedule } => kanavos_schedules::schedule(doc, *schedule, locale).to_drawing(
            pt(0.0, 0.0),
            &TableStyle::default(),
            locale,
        ),
        ViewKind::ThreeD { .. } => return Err(SheetError::Unsupported(view)),
    })
}

/// Map from a view's drawing coordinates to sheet millimetres for a viewport.
pub fn viewport_map(drawing: &Drawing, vp: &Viewport, view: &View) -> Affine {
    let bounds = drawing.bounds();
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

/// Composes a sheet: border, title block and every viewport, in paper millimetres.
pub fn sheet_drawing(doc: &Document, sheet: Id, locale: Locale) -> Result<Drawing, SheetError> {
    let s = doc.sheet(sheet).ok_or(SheetError::NotFound(sheet))?;
    let (w, h) = s.paper.dimensions_mm();
    let mut d = Drawing::paper();
    // Border with a 20 mm binding margin on the left and 10 mm elsewhere.
    d.push(Item::polyline(
        vec![pt(20.0, 10.0), pt(w - 10.0, 10.0), pt(w - 10.0, h - 10.0), pt(20.0, h - 10.0)],
        true,
        Stroke::solid(weights::EXTRA_THICK),
        "sheet.border",
    ));
    for vp in &s.viewports {
        let Some(view) = doc.view(vp.view) else { continue };
        let Ok(drawing) = render_view(doc, vp.view, locale) else { continue };
        let m = viewport_map(&drawing, vp, view);
        let placed = drawing.transformed(&m);
        let bottom = placed.bounds().map(|b| b.min.y).unwrap_or(vp.centre[1]);
        d.items.extend(placed.items);
        let title = vp.title.clone().unwrap_or_else(|| view.name.clone());
        let scale_label = match drawing.units {
            DrawingUnits::Metres => format!("{title}  1:{}", vp.scale.unwrap_or(view.scale)),
            DrawingUnits::Millimetres => title,
        };
        let mut text =
            Text::new(pt(vp.centre[0], bottom - 8.0), scale_label, 3.5).anchored(Anchor::TopCentre);
        text.bold = true;
        d.push(Item::text(text, "sheet.viewport_title"));
    }
    if s.title_block {
        d.extend(title_block(doc, s, locale));
    }
    Ok(d)
}

/// The title block in the lower-right corner of a sheet.
pub fn title_block(doc: &Document, s: &Sheet, locale: Locale) -> Drawing {
    let (w, _) = s.paper.dimensions_mm();
    let info = doc.info();
    let mut d = Drawing::paper();
    let (x1, y0) = (w - 10.0, 10.0);
    let x0 = x1 - 180.0;
    let rows: Vec<(String, String)> = vec![
        (t(locale, "sheets.tb.project").into_owned(), info.name.clone()),
        (t(locale, "sheets.tb.location").into_owned(), info.location.clone()),
        (t(locale, "sheets.tb.owner").into_owned(), info.owner.clone()),
        (t(locale, "sheets.tb.designer").into_owned(), info.designer.clone()),
        (t(locale, "sheets.tb.drawing").into_owned(), s.name.clone()),
        (t(locale, "sheets.tb.number").into_owned(), format!("{}    {}", s.number, info.date)),
    ];
    let row_h = 8.0;
    let height = row_h * rows.len() as f64;
    d.push(Item::polyline(
        vec![pt(x0, y0), pt(x1, y0), pt(x1, y0 + height), pt(x0, y0 + height)],
        true,
        Stroke::solid(weights::THICK),
        "sheet.title_block",
    ));
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = y0 + height - row_h * (i as f64 + 1.0);
        d.push(Item::line(pt(x0, y), pt(x1, y), Stroke::solid(weights::FINE), "sheet.title_block"));
        d.push(Item::text(Text::new(pt(x0 + 2.0, y + 2.5), label.clone(), 2.0), "sheet.title_block"));
        let mut v = Text::new(pt(x0 + 45.0, y + 2.5), value.clone(), 2.8);
        v.bold = true;
        d.push(Item::text(v, "sheet.title_block"));
    }
    d.push(Item::line(
        pt(x0 + 43.0, y0),
        pt(x0 + 43.0, y0 + height),
        Stroke::solid(weights::FINE),
        "sheet.title_block",
    ));
    d
}

/// Adds the standard permit drawing set (διάγραμμα κάλυψης, plans, sections, elevations, area table) as sheets,
/// creating a section through the middle of the building if none exists. Returns the new document.
pub fn permit_set(doc: &Document, locale: Locale) -> Result<Document, SheetError> {
    let mut doc = doc.clone();
    let mut number = 1;
    let mut add_sheet = |doc: &mut Document, name: String, views: Vec<Id>| {
        let (d, id) = doc.with_new_id();
        let paper = Paper { size: PaperSize::A3, landscape: true };
        let (w, h) = paper.dimensions_mm();
        let viewports = views
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let n = views.len().max(1) as f64;
                Viewport {
                    view: *v,
                    centre: [20.0 + (w - 30.0) * (i as f64 + 0.5) / n, h / 2.0 + 20.0],
                    scale: None,
                    title: None,
                    crop: None,
                }
            })
            .collect();
        *doc = d.with_sheet(Sheet {
            id,
            number: format!("Α-{number:02}"),
            name,
            paper,
            viewports,
            title_block: true,
        });
        number += 1;
    };
    let coverage: Vec<Id> =
        doc.views().filter(|v| matches!(v.kind, ViewKind::CoverageDiagram)).map(|v| v.id).collect();
    if !coverage.is_empty() {
        add_sheet(&mut doc, t(locale, "sheets.coverage").into_owned(), coverage);
    }
    let plans: Vec<(String, Id)> =
        doc.storeys().iter().filter_map(|s| doc.plan_view_of(s.id).map(|v| (v.name.clone(), v.id))).collect();
    for (name, v) in plans {
        add_sheet(&mut doc, name, vec![v]);
    }
    let sections: Vec<Id> =
        doc.views().filter(|v| matches!(v.kind, ViewKind::Section { .. })).map(|v| v.id).collect();
    if !sections.is_empty() {
        add_sheet(&mut doc, t(locale, "sheets.sections").into_owned(), sections);
    }
    let elevations: Vec<Id> =
        doc.views().filter(|v| matches!(v.kind, ViewKind::Elevation { .. })).map(|v| v.id).collect();
    if !elevations.is_empty() {
        add_sheet(&mut doc, t(locale, "sheets.elevations").into_owned(), elevations);
    }
    let tables: Vec<Id> =
        doc.views().filter(|v| matches!(v.kind, ViewKind::AreaTable)).map(|v| v.id).collect();
    if !tables.is_empty() {
        add_sheet(&mut doc, t(locale, "sheets.areas").into_owned(), tables);
    }
    Ok(doc)
}
