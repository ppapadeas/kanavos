//! Exporting views, sheets, schedules and the model: the single place where exporters are connected.
//!
//! | Format | Writer |
//! | --- | --- |
//! | SVG | `kanavos_drawing::svg::to_svg` |
//! | PNG | `kanavos_raster::render_png` |
//! | PDF | `kanavos_pdf::write_pdf`, sheets on their paper, views on a fitted page |
//! | DXF | `kanavos_dxf::write_dxf` |
//! | IFC 4.3 | `kanavos_ifc::export`, with roof, stair and object bodies from the 3D scene |
//! | glTF (GLB) | `kanavos_gltf::export_glb` |
//! | CSV | schedules and area tables |
//! | JSON | drawings in their native vector form |
//!
//! The control protocol, the MCP server and the CLI all export through [`export`].

use std::fmt;

use kanavos_drawing::Drawing;
use kanavos_drawing::fonts::Fonts;
use kanavos_drawing::page::PageFrame;
use kanavos_drawing::svg::SvgOptions;
use kanavos_drawing::table::Table;
use kanavos_dxf::DxfExportOptions;
use kanavos_model::{Category, Document, Id, Paper, ScheduleKind, ViewKind};
use kanavos_pdf::{PdfMeta, PdfPage};
use kanavos_raster::{RasterOptions, RasterSize};
use kanavos_solid::Mesh;
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
        true
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

/// Page frame for a drawing: the sheet's paper when known, otherwise fitted to the content.
fn frame(paper: Option<Paper>) -> Option<PageFrame> {
    paper.map(|p| {
        let (w, h) = p.dimensions_mm();
        PageFrame::paper(w, h)
    })
}

/// SVG text of a drawing, on the sheet's paper when given, otherwise on a page fitted to the content.
pub fn svg(drawing: &Drawing, title: &str, paper: Option<Paper>) -> Result<String, ExportError> {
    let opts = SvgOptions {
        frame: frame(paper),
        title: (!title.is_empty()).then(|| title.to_owned()),
        ..SvgOptions::default()
    };
    Ok(kanavos_drawing::svg::to_svg(drawing, &opts))
}

/// PNG bytes of a drawing at a resolution (dots per inch), on a white background.
pub fn png(drawing: &Drawing, dpi: f64, paper: Option<Paper>) -> Result<Vec<u8>, ExportError> {
    let opts = RasterOptions { size: RasterSize::Dpi(dpi), frame: frame(paper), ..RasterOptions::default() };
    kanavos_raster::render_png(drawing, &opts).map_err(|e| ExportError::Failed(e.to_string()))
}

/// PDF bytes with one page per drawing: on the sheet's paper when given, otherwise a page fitted to the drawing at
/// its scale.
pub fn pdf(pages: &[(Drawing, Option<Paper>)], doc: &Document) -> Result<Vec<u8>, ExportError> {
    let fonts = Fonts::system();
    let pages: Vec<PdfPage> = pages
        .iter()
        .map(|(d, paper)| match paper {
            Some(p) => {
                let (w, h) = p.dimensions_mm();
                PdfPage::new(d.clone(), w, h)
            }
            None => PdfPage::fitted(d, 10.0, fonts),
        })
        .collect();
    let info = doc.info();
    let meta = PdfMeta {
        title: info.name.clone(),
        author: info.designer.clone(),
        subject: info.location.clone(),
        ..PdfMeta::default()
    };
    kanavos_pdf::write_pdf(&pages, &meta).map_err(|e| ExportError::Failed(e.to_string()))
}

/// DXF bytes of a drawing (model drawings in metres, sheets in millimetres).
pub fn dxf(drawing: &Drawing) -> Result<Vec<u8>, ExportError> {
    Ok(kanavos_dxf::write_dxf(drawing, &DxfExportOptions::default()))
}

/// The 3D body of a roof, stair or object as one mesh, from the views' scene, for exporters whose own geometry
/// for these kinds is simpler.
pub fn scene_mesh(doc: &Document, id: Id) -> Option<Mesh> {
    let e = doc.element(id)?;
    if !matches!(e.category(), Category::Roof | Category::Stair | Category::Object) {
        return None;
    }
    let mut mesh = Mesh::new();
    for item in kanavos_views::element_scene_items(doc, id) {
        mesh.append(&item.mesh);
    }
    (!mesh.is_empty()).then_some(mesh)
}

/// IFC 4.3 text of the model (bodies of roofs, stairs and objects from the 3D scene).
pub fn ifc(doc: &Document) -> Result<Vec<u8>, ExportError> {
    let geometry = |id: Id| scene_mesh(doc, id);
    let stamp = crate::io::now_rfc3339();
    let info = doc.info();
    let opts = kanavos_ifc::ExportOptions {
        timestamp: stamp.trim_end_matches('Z').to_owned(),
        author: info.designer.clone(),
        file_name: format!("{}.ifc", file_stem(&info.name)),
        geometry: Some(&geometry),
        ..kanavos_ifc::ExportOptions::default()
    };
    Ok(kanavos_ifc::export(doc, &opts).text.into_bytes())
}

/// Binary glTF bytes of the model.
pub fn glb(doc: &Document) -> Result<Vec<u8>, ExportError> {
    kanavos_gltf::export_glb(doc, &kanavos_gltf::GltfOptions::default())
        .map_err(|e| ExportError::Failed(e.to_string()))
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
    doc: &Document,
    drawing: &Drawing,
    format: ExportFormat,
    title: &str,
    paper: Option<Paper>,
) -> Result<Vec<u8>, ExportError> {
    match format {
        ExportFormat::Svg => svg(drawing, title, paper).map(String::into_bytes),
        ExportFormat::Png => png(drawing, 150.0, paper),
        ExportFormat::Dxf => dxf(drawing),
        ExportFormat::Pdf => pdf(&[(drawing.clone(), paper)], doc),
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
            Ok(vec![ExportFile { stem: file_stem(&v.name), bytes: encode(doc, &d, format, &v.name, None)? }])
        }
        ExportTarget::Sheet { id } => {
            let s = doc.sheet(*id).ok_or(ExportError::NotFound(*id))?;
            if format == ExportFormat::Csv {
                return Err(ExportError::Unsupported { format, what: format!("sheet {}", s.number) });
            }
            let d = sheet_drawing(doc, *id, locale)?;
            let stem = file_stem(&format!("{} {}", s.number, s.name));
            Ok(vec![ExportFile { stem, bytes: encode(doc, &d, format, &s.name, Some(s.paper))? }])
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
                return Ok(vec![ExportFile { stem: file_stem(&title), bytes: pdf(&pages, doc)? }]);
            }
            let mut out = Vec::with_capacity(sheets.len());
            for s in sheets {
                let d = sheet_drawing(doc, s.id, locale)?;
                let stem = file_stem(&format!("{} {}", s.number, s.name));
                out.push(ExportFile { stem, bytes: encode(doc, &d, format, &s.name, Some(s.paper))? });
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
    fn every_format_writes_the_sample() {
        let doc = crate::sample::sample_house().unwrap();
        let plan = doc.views().find(|v| matches!(v.kind, ViewKind::Plan { .. })).unwrap().id;
        let view = ExportTarget::View { id: plan };
        let svg = export(&doc, &view, ExportFormat::Svg, Locale::El).unwrap();
        assert!(String::from_utf8_lossy(&svg[0].bytes).contains("<svg"));
        let png = export(&doc, &view, ExportFormat::Png, Locale::El).unwrap();
        assert!(png[0].bytes.starts_with(b"\x89PNG"));
        let dxf = export(&doc, &view, ExportFormat::Dxf, Locale::El).unwrap();
        assert!(String::from_utf8_lossy(&dxf[0].bytes).contains("SECTION"));
        let pdf = export(&doc, &ExportTarget::Sheets, ExportFormat::Pdf, Locale::El).unwrap();
        assert_eq!(pdf.len(), 1);
        assert!(pdf[0].bytes.starts_with(b"%PDF"));
        let sheets = export(&doc, &ExportTarget::Sheets, ExportFormat::Svg, Locale::El).unwrap();
        assert_eq!(sheets.len(), doc.sheets().len());
        let ifc = export(&doc, &ExportTarget::Model, ExportFormat::Ifc, Locale::En).unwrap();
        let text = String::from_utf8_lossy(&ifc[0].bytes);
        assert!(text.starts_with("ISO-10303-21") && text.contains("IFCWALL") && text.contains("IFCROOF"));
        let glb = export(&doc, &ExportTarget::Model, ExportFormat::Gltf, Locale::En).unwrap();
        assert!(glb[0].bytes.starts_with(b"glTF"));
    }

    #[test]
    fn roofs_stairs_and_objects_take_their_bodies_from_the_scene() {
        let doc = crate::sample::sample_house().unwrap();
        let roof = doc.elements_of(Category::Roof).next().unwrap().id;
        assert!(scene_mesh(&doc, roof).is_some());
        let wall = doc.elements_of(Category::Wall).next().unwrap().id;
        assert!(scene_mesh(&doc, wall).is_none());
    }
}
