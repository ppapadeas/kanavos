//! Exports: the adapter between the File ▸ Export menu and the exporter crates.
//!
//! - PDF of sheets (`kanavos_pdf::write_pdf`), one page per sheet at its paper size; a 2D view gives one page
//!   fitted to the drawing.
//! - DXF (`kanavos_dxf::write_dxf`), SVG (`kanavos_drawing::svg::to_svg`) and PNG
//!   (`kanavos_raster::render_png`) of the focused view or sheet.
//! - IFC 4.3 (`kanavos_ifc::export_to_file`) and glTF binary (`kanavos_gltf::export_glb`) of the model.
//! - CSV of a schedule or the area table (`Table::to_csv`).
//!
//! Exports run on a worker thread ([`spawn`]); the interface keeps working and reports the result.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use kanavos_drawing::Drawing;
use kanavos_drawing::page::PageFrame;
use kanavos_drawing::table::Table;
use kanavos_fonts::Fonts;
use kanavos_model::{Document, Id};
use kanavos_units::Locale;

/// What to export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// All sheets (or the active sheet) as one PDF.
    SheetsPdf,
    /// The active view or sheet as DXF.
    Dxf,
    /// The active view or sheet as SVG.
    Svg,
    /// The active view or sheet as PNG.
    Png,
    /// The model as IFC 4.3.
    Ifc,
    /// The model as glTF.
    Gltf,
    /// The active schedule or area table as CSV.
    Csv,
}

impl ExportKind {
    /// Every export, in menu order.
    pub const ALL: [ExportKind; 7] = [
        ExportKind::SheetsPdf,
        ExportKind::Dxf,
        ExportKind::Svg,
        ExportKind::Png,
        ExportKind::Ifc,
        ExportKind::Gltf,
        ExportKind::Csv,
    ];

    /// File extension.
    pub fn extension(self) -> &'static str {
        match self {
            ExportKind::SheetsPdf => "pdf",
            ExportKind::Dxf => "dxf",
            ExportKind::Svg => "svg",
            ExportKind::Png => "png",
            ExportKind::Ifc => "ifc",
            ExportKind::Gltf => "glb",
            ExportKind::Csv => "csv",
        }
    }

    /// Catalogue key of the menu label.
    pub fn label_key(self) -> &'static str {
        match self {
            ExportKind::SheetsPdf => "ui.export.pdf",
            ExportKind::Dxf => "ui.export.dxf",
            ExportKind::Svg => "ui.export.svg",
            ExportKind::Png => "ui.export.png",
            ExportKind::Ifc => "ui.export.ifc",
            ExportKind::Gltf => "ui.export.gltf",
            ExportKind::Csv => "ui.export.csv",
        }
    }

    /// Whether this build can perform the export.
    pub fn available(self) -> bool {
        true
    }
}

/// What the export reads.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportSource {
    /// Sheets, in order.
    Sheets(Vec<Id>),
    /// A 2D view.
    View(Id),
    /// One sheet.
    Sheet(Id),
    /// The whole model.
    Model,
    /// A computed table (schedule, area table).
    Table(Table),
}

/// Why an export failed.
#[derive(Debug)]
pub enum ExportError {
    /// The exporter is not part of this build.
    NotAvailable,
    /// The export needs a different source (a sheet or 2D view, a table …).
    WrongSource,
    /// The exporter refused the content.
    Failed(String),
    /// Writing the file failed.
    Io(std::io::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::NotAvailable => f.write_str("not available yet"),
            ExportError::WrongSource => f.write_str("nothing suitable to export"),
            ExportError::Failed(why) => f.write_str(why),
            ExportError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// What an export produced, for the notice.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportSummary {
    /// Pages, drawings or products written.
    pub items: usize,
    /// Warnings from the exporter (elements left out, simplifications).
    pub warnings: usize,
}

/// A 2D drawing of a source with the page it belongs on (sheets have their paper; views fit their content).
fn drawing_of(
    doc: &Document,
    source: &ExportSource,
    locale: Locale,
) -> Result<(Drawing, Option<PageFrame>), ExportError> {
    let failed = |e: kanavos_sheets::SheetError| match e {
        kanavos_sheets::SheetError::Unsupported(_) => ExportError::WrongSource,
        other => ExportError::Failed(other.to_string()),
    };
    match source {
        ExportSource::View(id) => Ok((kanavos_sheets::render_view(doc, *id, locale).map_err(failed)?, None)),
        ExportSource::Sheet(id) => {
            let sheet = doc.sheet(*id).ok_or(ExportError::WrongSource)?;
            let (w, h) = sheet.paper.dimensions_mm();
            let drawing = kanavos_sheets::sheet_drawing(doc, *id, locale).map_err(failed)?;
            Ok((drawing, Some(PageFrame::paper(w, h))))
        }
        ExportSource::Sheets(ids) => match ids.first() {
            Some(id) => drawing_of(doc, &ExportSource::Sheet(*id), locale),
            None => Err(ExportError::WrongSource),
        },
        ExportSource::Model | ExportSource::Table(_) => Err(ExportError::WrongSource),
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    std::fs::write(path, bytes).map_err(ExportError::Io)
}

/// The IFC header timestamp (`YYYY-MM-DDThh:mm:ss`).
fn ifc_timestamp() -> String {
    let d = kanavos_pdf::PdfDate::now();
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", d.year, d.month, d.day, d.hour, d.minute, d.second)
}

/// Exports `source` from `doc` to `path` (blocking).
pub fn export(
    kind: ExportKind,
    doc: &Document,
    source: &ExportSource,
    locale: Locale,
    path: &Path,
) -> Result<ExportSummary, ExportError> {
    let info = doc.info();
    let title = if info.name.is_empty() { "Kanavos".to_owned() } else { info.name.clone() };
    match kind {
        ExportKind::Csv => match source {
            ExportSource::Table(table) => {
                write(path, table.to_csv(locale).as_bytes())?;
                Ok(ExportSummary { items: table.rows.len(), warnings: 0 })
            }
            _ => Err(ExportError::WrongSource),
        },
        ExportKind::SheetsPdf => {
            let pages: Vec<kanavos_pdf::PdfPage> = match source {
                ExportSource::Sheets(ids) => ids
                    .iter()
                    .filter_map(|id| {
                        let sheet = doc.sheet(*id)?;
                        let (w, h) = sheet.paper.dimensions_mm();
                        let drawing = kanavos_sheets::sheet_drawing(doc, *id, locale).ok()?;
                        Some(kanavos_pdf::PdfPage::new(drawing, w, h))
                    })
                    .collect(),
                ExportSource::Sheet(_) | ExportSource::View(_) => {
                    let (drawing, frame) = drawing_of(doc, source, locale)?;
                    match frame {
                        Some(f) => vec![kanavos_pdf::PdfPage::new(drawing, f.width_mm, f.height_mm)],
                        None => vec![kanavos_pdf::PdfPage::fitted(&drawing, 10.0, Fonts::system())],
                    }
                }
                ExportSource::Model | ExportSource::Table(_) => return Err(ExportError::WrongSource),
            };
            if pages.is_empty() {
                return Err(ExportError::WrongSource);
            }
            let meta = kanavos_pdf::PdfMeta {
                title,
                author: info.designer.clone(),
                subject: info.location.clone(),
                ..kanavos_pdf::PdfMeta::default()
            };
            let bytes =
                kanavos_pdf::write_pdf(&pages, &meta).map_err(|e| ExportError::Failed(e.to_string()))?;
            write(path, &bytes)?;
            Ok(ExportSummary { items: pages.len(), warnings: 0 })
        }
        ExportKind::Dxf => {
            let (drawing, _) = drawing_of(doc, source, locale)?;
            let bytes = kanavos_dxf::write_dxf(&drawing, &kanavos_dxf::DxfExportOptions::default());
            write(path, &bytes)?;
            Ok(ExportSummary { items: drawing.items.len(), warnings: 0 })
        }
        ExportKind::Svg => {
            let (drawing, frame) = drawing_of(doc, source, locale)?;
            let opts = kanavos_drawing::svg::SvgOptions {
                frame,
                background: Some(kanavos_drawing::Rgba::WHITE),
                title: Some(title),
                ..kanavos_drawing::svg::SvgOptions::default()
            };
            write(path, kanavos_drawing::svg::to_svg(&drawing, &opts).as_bytes())?;
            Ok(ExportSummary { items: drawing.items.len(), warnings: 0 })
        }
        ExportKind::Png => {
            let (drawing, frame) = drawing_of(doc, source, locale)?;
            let opts = kanavos_raster::RasterOptions { frame, ..kanavos_raster::RasterOptions::default() };
            let bytes = kanavos_raster::render_png(&drawing, &opts)
                .map_err(|e| ExportError::Failed(e.to_string()))?;
            write(path, &bytes)?;
            Ok(ExportSummary { items: 1, warnings: 0 })
        }
        ExportKind::Ifc => {
            let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let opts = kanavos_ifc::ExportOptions {
                timestamp: ifc_timestamp(),
                author: info.designer.clone(),
                organization: info.designer_details.clone(),
                file_name,
                ..kanavos_ifc::ExportOptions::default()
            };
            let report = kanavos_ifc::export_to_file(path, doc, &opts)
                .map_err(|e| ExportError::Failed(e.to_string()))?;
            let items = report.products.values().sum();
            Ok(ExportSummary { items, warnings: report.warnings.len() })
        }
        ExportKind::Gltf => {
            let bytes = kanavos_gltf::export_glb(doc, &kanavos_gltf::GltfOptions::default())
                .map_err(|e| ExportError::Failed(e.to_string()))?;
            write(path, &bytes)?;
            Ok(ExportSummary { items: doc.element_count(), warnings: 0 })
        }
    }
}

/// Runs an export on a worker thread; the result arrives on the returned channel with the file path.
pub fn spawn(
    kind: ExportKind,
    doc: Document,
    source: ExportSource,
    locale: Locale,
    path: PathBuf,
    wake: impl FnOnce() + Send + 'static,
) -> Receiver<(PathBuf, Result<ExportSummary, String>)> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new().name("kanavos-export".into()).spawn(move || {
        let r = export(kind, &doc, &source, locale, &path).map_err(|e| e.to_string());
        if tx.send((path, r)).is_ok() {
            wake();
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::table::{Align, Cell, Column};

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kanavos-export-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn csv_of_a_table_is_written_with_locale_separators() {
        let table = Table {
            title: "T".into(),
            columns: vec![Column::new("Α", Align::Left, 10.0), Column::new("Β", Align::Right, 10.0)],
            rows: vec![vec![Cell::text("x"), Cell::num2(1.5)]],
            footer: vec![],
        };
        let dir = temp("csv");
        let path = dir.join("t.csv");
        export(ExportKind::Csv, &Document::new(), &ExportSource::Table(table), Locale::El, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("x;1,50"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_format_exports_the_sample_house() {
        let doc = crate::sample::sample_house(Locale::El);
        let dir = temp("all");
        let sheets: Vec<Id> = doc.sheets().iter().map(|s| s.id).collect();
        let plan = doc.storeys().first().and_then(|s| doc.plan_view_of(s.id)).map(|v| v.id).unwrap();
        let cases = [
            (ExportKind::SheetsPdf, ExportSource::Sheets(sheets.clone()), "%PDF"),
            (ExportKind::Dxf, ExportSource::View(plan), "0"),
            (ExportKind::Svg, ExportSource::Sheet(sheets[0]), "<"),
            (ExportKind::Png, ExportSource::View(plan), "\u{89}PNG"),
            (ExportKind::Ifc, ExportSource::Model, "ISO-10303-21"),
            (ExportKind::Gltf, ExportSource::Model, "glTF"),
        ];
        for (kind, source, start) in cases {
            let path = dir.join(format!("out.{}", kind.extension()));
            let summary = export(kind, &doc, &source, Locale::El, &path).unwrap();
            assert!(summary.items > 0, "{kind:?} wrote nothing");
            let bytes = std::fs::read(&path).unwrap();
            let head: String = bytes.iter().take(64).map(|b| char::from(*b)).collect();
            assert!(head.trim_start().starts_with(start), "{kind:?} starts with {head:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn model_exports_need_no_drawing_and_drawings_need_a_view() {
        let doc = kanavos_model::starter();
        let path = std::env::temp_dir().join("never-written.dxf");
        assert!(matches!(
            export(ExportKind::Dxf, &doc, &ExportSource::Model, Locale::En, &path),
            Err(ExportError::WrongSource)
        ));
        // A 3D view exports as its hidden-line drawing, so SVG of a 3D view is allowed; a table is not a drawing.
        assert!(matches!(
            export(ExportKind::Dxf, &doc, &ExportSource::Table(Table::default()), Locale::En, &path),
            Err(ExportError::WrongSource)
        ));
    }
}
