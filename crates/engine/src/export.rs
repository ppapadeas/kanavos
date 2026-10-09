//! Exporting views, sheets, schedules and the model: the single place where exporters are connected.
//!
//! **Adapter.** Formats whose exporter is not merged yet return [`ExportError::NotAvailable`]. To switch one
//! on, replace the body of its function below:
//!
//! | Function | Exporter |
//! | --- | --- |
//! | [`svg`] | `kanavos_drawing::svg::to_svg(drawing, &SvgOptions { title, .. })` |
//! | [`png`] | `kanavos_raster::render_png(drawing, &RasterOptions { size: RasterSize::Dpi(dpi), .. })` |
//! | [`pdf`] | `kanavos_pdf::write_pdf(pages, options)` (one page per drawing, paper size from the sheet) |
//! | [`dxf`] | `kanavos_dxf` drawing export |
//! | [`ifc`] | `kanavos_ifc` model export |
//! | [`glb`] | `kanavos_gltf::export_glb(doc, &GltfOptions::default())` |
//!
//! CSV (schedules and area tables) and JSON (drawings, the native vector form) work now. The control protocol,
//! the MCP server and the CLI all export through [`export`].

use std::fmt;

use kanavos_drawing::Drawing;
use kanavos_drawing::table::Table;
use kanavos_model::{Document, Id, Paper, ScheduleKind, ViewKind};
use kanavos_units::Locale;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An export file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Vector PDF, one page per sheet or view.
    Pdf,
    /// SVG drawing.
    Svg,
    /// DXF drawing.
    Dxf,
    /// PNG image.
    Png,
    /// IFC 4.3 model.
    Ifc,
    /// Binary glTF 2.0 model (`.glb`).
    Gltf,
    /// CSV table (schedules and area tables).
    Csv,
    /// The drawing as Kanavos JSON (vector items in model or paper units).
    Json,
}

impl ExportFormat {
    /// Every format.
    pub const ALL: [ExportFormat; 8] = [
        ExportFormat::Pdf,
        ExportFormat::Svg,
        ExportFormat::Dxf,
        ExportFormat::Png,
        ExportFormat::Ifc,
        ExportFormat::Gltf,
        ExportFormat::Csv,
        ExportFormat::Json,
    ];

    /// Parses a format name (case-insensitive); `glb` is accepted for glTF.
    pub fn parse(s: &str) -> Option<ExportFormat> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "pdf" => ExportFormat::Pdf,
            "svg" => ExportFormat::Svg,
            "dxf" => ExportFormat::Dxf,
            "png" => ExportFormat::Png,
            "ifc" => ExportFormat::Ifc,
            "gltf" | "glb" => ExportFormat::Gltf,
            "csv" => ExportFormat::Csv,
            "json" => ExportFormat::Json,
            _ => return None,
        })
    }

    /// Lower-case name.
    pub fn name(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Svg => "svg",
            ExportFormat::Dxf => "dxf",
            ExportFormat::Png => "png",
            ExportFormat::Ifc => "ifc",
            ExportFormat::Gltf => "gltf",
            ExportFormat::Csv => "csv",
            ExportFormat::Json => "json",
        }
    }

    /// File extension.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Gltf => "glb",
            other => other.name(),
        }
    }

    /// MIME type.
    pub fn mime(self) -> &'static str {
        match self {
            ExportFormat::Pdf => "application/pdf",
            ExportFormat::Svg => "image/svg+xml",
            ExportFormat::Dxf => "image/vnd.dxf",
            ExportFormat::Png => "image/png",
            ExportFormat::Ifc => "application/x-step",
            ExportFormat::Gltf => "model/gltf-binary",
            ExportFormat::Csv => "text/csv",
            ExportFormat::Json => "application/json",
        }
    }

    /// Whether the format exports the whole model rather than a view or sheet.
    pub fn is_model(self) -> bool {
        matches!(self, ExportFormat::Ifc | ExportFormat::Gltf)
    }

    /// Whether the format's exporter is connected in this build.
    pub fn available(self) -> bool {
        matches!(self, ExportFormat::Csv | ExportFormat::Json)
    }
}

/// Why an export failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// The exporter for this format is not part of this build yet.
    NotAvailable(ExportFormat),
    /// The format cannot represent what was asked (e.g. CSV of a plan).
    Unsupported {
        /// Format.
        format: ExportFormat,
        /// What was asked.
        what: String,
    },
    /// The view, sheet or element does not exist.
    NotFound(Id),
    /// Drawing the view or sheet failed.
    Failed(String),
}

impl ExportError {
    /// The message in a language.
    pub fn message(&self, locale: Locale) -> String {
        use kanavos_i18n::tf;
        match self {
            ExportError::NotAvailable(f) => {
                tf(locale, "engine.export.not_available", &[("format", f.name())])
            }
            ExportError::Unsupported { format, what } => {
                tf(locale, "engine.export.unsupported", &[("format", format.name()), ("what", what)])
            }
            ExportError::NotFound(id) => tf(locale, "engine.error.not_found", &[("id", &id.0.to_string())]),
            ExportError::Failed(why) => tf(locale, "engine.export.failed", &[("reason", why)]),
        }
    }
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message(Locale::En))
    }
}

impl std::error::Error for ExportError {}

/// What to export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportTarget {
    /// One view.
    View {
        /// View.
        id: Id,
    },
    /// One sheet.
    Sheet {
        /// Sheet.
        id: Id,
    },
    /// Every sheet, in sheet-number order.
    Sheets,
    /// The whole model (IFC, glTF).
    Model,
}

/// One exported file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    /// Suggested file name stem (view name or sheet number), without extension.
    pub stem: String,
    /// Contents.
    pub bytes: Vec<u8>,
}

/// A 2D view as a drawing (plans, sections, elevations in model metres; tables in paper millimetres).
pub fn view_drawing(doc: &Document, view: Id, locale: Locale) -> Result<Drawing, ExportError> {
    doc.view(view).ok_or(ExportError::NotFound(view))?;
    kanavos_sheets::render_view(doc, view, locale).map_err(|e| ExportError::Failed(e.to_string()))
}

/// A sheet as a drawing in paper millimetres.
pub fn sheet_drawing(doc: &Document, sheet: Id, locale: Locale) -> Result<Drawing, ExportError> {
    doc.sheet(sheet).ok_or(ExportError::NotFound(sheet))?;
    kanavos_sheets::sheet_drawing(doc, sheet, locale).map_err(|e| ExportError::Failed(e.to_string()))
}

/// The table behind a schedule or area-table view, if it is one.
pub fn view_table(doc: &Document, view: Id, locale: Locale) -> Result<Option<Table>, ExportError> {
    let v = doc.view(view).ok_or(ExportError::NotFound(view))?;
    Ok(match &v.kind {
        ViewKind::Schedule { schedule } => Some(kanavos_schedules::schedule(doc, *schedule, locale)),
        ViewKind::AreaTable => Some(kanavos_rules_gr::area_table(doc, locale)),
        _ => None,
    })
}

/// A schedule table.
pub fn schedule_table(doc: &Document, kind: ScheduleKind, locale: Locale) -> Table {
    kanavos_schedules::schedule(doc, kind, locale)
}

/// SVG text of a drawing. Not available until the SVG writer is merged.
pub fn svg(drawing: &Drawing, title: &str) -> Result<String, ExportError> {
    let _ = (drawing, title);
    Err(ExportError::NotAvailable(ExportFormat::Svg))
}

/// PNG bytes of a drawing at a resolution. Not available until the rasteriser is merged.
pub fn png(drawing: &Drawing, dpi: f64) -> Result<Vec<u8>, ExportError> {
    let _ = (drawing, dpi);
    Err(ExportError::NotAvailable(ExportFormat::Png))
}

/// PDF bytes with one page per drawing (with its paper, when known). Not available until the PDF writer is merged.
pub fn pdf(pages: &[(Drawing, Option<Paper>)], title: &str) -> Result<Vec<u8>, ExportError> {
    let _ = (pages, title);
    Err(ExportError::NotAvailable(ExportFormat::Pdf))
}

/// DXF bytes of a drawing. Not available until the DXF writer is merged.
pub fn dxf(drawing: &Drawing) -> Result<Vec<u8>, ExportError> {
    let _ = drawing;
    Err(ExportError::NotAvailable(ExportFormat::Dxf))
}

/// IFC bytes of the model. Not available until the IFC writer is merged.
pub fn ifc(doc: &Document) -> Result<Vec<u8>, ExportError> {
    let _ = doc;
    Err(ExportError::NotAvailable(ExportFormat::Ifc))
}

/// Binary glTF bytes of the model. Not available until the glTF writer is merged.
pub fn glb(doc: &Document) -> Result<Vec<u8>, ExportError> {
    let _ = doc;
    Err(ExportError::NotAvailable(ExportFormat::Gltf))
}

/// CSV text of a table.
pub fn csv(table: &Table, locale: Locale) -> String {
    table.to_csv(locale)
}

/// A drawing as pretty JSON.
pub fn drawing_json(drawing: &Drawing) -> Result<String, ExportError> {
    serde_json::to_string_pretty(drawing).map_err(|e| ExportError::Failed(e.to_string()))
}

fn file_stem(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() { "export".to_owned() } else { trimmed.to_owned() }
}

/// Encodes one drawing in a drawing format.
fn encode(
    drawing: &Drawing,
    format: ExportFormat,
    title: &str,
    paper: Option<Paper>,
) -> Result<Vec<u8>, ExportError> {
    match format {
        ExportFormat::Svg => svg(drawing, title).map(String::into_bytes),
        ExportFormat::Png => png(drawing, 150.0),
        ExportFormat::Dxf => dxf(drawing),
        ExportFormat::Pdf => pdf(&[(drawing.clone(), paper)], title),
        ExportFormat::Json => drawing_json(drawing).map(String::into_bytes),
        other => Err(ExportError::Unsupported { format: other, what: "a drawing".into() }),
    }
}

/// Exports a target in a format. Model formats ignore the target and export the model; drawing formats need a
/// view, a sheet or all sheets (PDF puts all sheets in one file, the others write one file per sheet); CSV needs a
/// schedule or area-table view.
pub fn export(
    doc: &Document,
    target: &ExportTarget,
    format: ExportFormat,
    locale: Locale,
) -> Result<Vec<ExportFile>, ExportError> {
    let title = doc.info().name.clone();
    match format {
        ExportFormat::Ifc => return Ok(vec![ExportFile { stem: file_stem(&title), bytes: ifc(doc)? }]),
        ExportFormat::Gltf => return Ok(vec![ExportFile { stem: file_stem(&title), bytes: glb(doc)? }]),
        _ => {}
    }
    match target {
        ExportTarget::Model => {
            Err(ExportError::Unsupported { format, what: "the whole model (give a view or sheet)".into() })
        }
        ExportTarget::View { id } => {
            let v = doc.view(*id).ok_or(ExportError::NotFound(*id))?;
            if format == ExportFormat::Csv {
                let table = view_table(doc, *id, locale)?
                    .ok_or_else(|| ExportError::Unsupported { format, what: format!("view '{}'", v.name) })?;
                return Ok(vec![ExportFile {
                    stem: file_stem(&v.name),
                    bytes: csv(&table, locale).into_bytes(),
                }]);
            }
            let d = view_drawing(doc, *id, locale)?;
            Ok(vec![ExportFile { stem: file_stem(&v.name), bytes: encode(&d, format, &v.name, None)? }])
        }
        ExportTarget::Sheet { id } => {
            let s = doc.sheet(*id).ok_or(ExportError::NotFound(*id))?;
            if format == ExportFormat::Csv {
                return Err(ExportError::Unsupported { format, what: format!("sheet {}", s.number) });
            }
            let d = sheet_drawing(doc, *id, locale)?;
            let stem = file_stem(&format!("{} {}", s.number, s.name));
            Ok(vec![ExportFile { stem, bytes: encode(&d, format, &s.name, Some(s.paper))? }])
        }
        ExportTarget::Sheets => {
            let sheets = doc.sheets();
            if sheets.is_empty() {
                return Err(ExportError::Unsupported { format, what: "a document without sheets".into() });
            }
            if format == ExportFormat::Csv {
                return Err(ExportError::Unsupported { format, what: "sheets".into() });
            }
            if format == ExportFormat::Pdf {
                let mut pages = Vec::with_capacity(sheets.len());
                for s in &sheets {
                    pages.push((sheet_drawing(doc, s.id, locale)?, Some(s.paper)));
                }
                return Ok(vec![ExportFile { stem: file_stem(&title), bytes: pdf(&pages, &title)? }]);
            }
            let mut out = Vec::with_capacity(sheets.len());
            for s in sheets {
                let d = sheet_drawing(doc, s.id, locale)?;
                let stem = file_stem(&format!("{} {}", s.number, s.name));
                out.push(ExportFile { stem, bytes: encode(&d, format, &s.name, Some(s.paper))? });
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::starter;

    #[test]
    fn formats_parse_and_name_themselves() {
        for f in ExportFormat::ALL {
            assert_eq!(ExportFormat::parse(f.name()), Some(f));
        }
        assert_eq!(ExportFormat::parse("GLB"), Some(ExportFormat::Gltf));
        assert_eq!(ExportFormat::Gltf.extension(), "glb");
        assert_eq!(ExportFormat::parse("docx"), None);
    }

    #[test]
    fn schedules_export_as_csv_and_plans_as_json() {
        let doc = starter();
        let schedule = doc.views().find(|v| matches!(v.kind, ViewKind::Schedule { .. })).unwrap().id;
        let files =
            export(&doc, &ExportTarget::View { id: schedule }, ExportFormat::Csv, Locale::El).unwrap();
        assert_eq!(files.len(), 1);
        assert!(!files[0].bytes.is_empty());
        let plan = doc.views().find(|v| matches!(v.kind, ViewKind::Plan { .. })).unwrap().id;
        let files = export(&doc, &ExportTarget::View { id: plan }, ExportFormat::Json, Locale::El).unwrap();
        let back: Drawing = serde_json::from_slice(&files[0].bytes).unwrap();
        assert!(back.items.is_empty());
        let err = export(&doc, &ExportTarget::View { id: plan }, ExportFormat::Csv, Locale::El).unwrap_err();
        assert!(matches!(err, ExportError::Unsupported { .. }));
    }

    #[test]
    fn missing_exporters_say_so() {
        let doc = starter();
        let err = export(&doc, &ExportTarget::Model, ExportFormat::Ifc, Locale::En).unwrap_err();
        assert_eq!(err, ExportError::NotAvailable(ExportFormat::Ifc));
        assert!(err.message(Locale::El).contains("ifc"));
    }
}
