//! Imports: the adapter between the File ▸ Import menu and the importer crates.
//!
//! - DXF survey (τοπογραφικό), through `kanavos_dxf::read_dxf`: the drawing becomes an underlay on the active
//!   storey. Grid (ΕΓΣΑ87) coordinates are moved into project coordinates with the project's georeference; when
//!   the project has none yet, a round origin near the survey becomes the georeference. The largest closed
//!   polyline becomes the plot when none is drawn, and points with elevations become terrain points.
//! - IFC 2x3/4/4.3, through `kanavos_ifc::import_file`: a new document that replaces the open project.

use std::fmt;
use std::path::Path;

use kanavos_engine::AnyCommand;
use kanavos_engine::commands::{CreateElement, SetSite};
use kanavos_model::{Document, ElementKind, Id, OpeningKind};

/// What to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// A DXF survey drawing, placed as an underlay.
    DxfSurvey,
    /// An IFC model, opened as a new project.
    Ifc,
}

impl ImportKind {
    /// Every import, in menu order.
    pub const ALL: [ImportKind; 2] = [ImportKind::DxfSurvey, ImportKind::Ifc];

    /// File extensions accepted.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            ImportKind::DxfSurvey => &["dxf"],
            ImportKind::Ifc => &["ifc"],
        }
    }

    /// Catalogue key of the menu label.
    pub fn label_key(self) -> &'static str {
        match self {
            ImportKind::DxfSurvey => "ui.import.dxf",
            ImportKind::Ifc => "ui.import.ifc",
        }
    }

    /// Whether this build can perform the import.
    pub fn available(self) -> bool {
        true
    }
}

/// Counts reported after an import.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    /// Items imported (drawing items for DXF, elements for IFC).
    pub items: usize,
    /// Items left out.
    pub skipped: usize,
    /// Whether the plot outline was taken from the file.
    pub plot: bool,
    /// Terrain points taken from the file.
    pub terrain_points: usize,
}

/// The result of an import.
#[derive(Debug, Clone, PartialEq)]
pub enum Imported {
    /// Commands that add the imported content to the open project (one undo step).
    Commands(Vec<AnyCommand>, ImportSummary),
    /// A new document that replaces the open project.
    Document(Box<Document>, ImportSummary),
}

/// Why an import failed.
#[derive(Debug)]
pub enum ImportError {
    /// The importer is not part of this build.
    NotAvailable,
    /// Reading the file failed.
    Io(std::io::Error),
    /// The file could not be understood.
    Format(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::NotAvailable => f.write_str("not available yet"),
            ImportError::Io(e) => write!(f, "{e}"),
            ImportError::Format(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for ImportError {}

/// Coordinates beyond this distance from the origin (metres) are taken as grid coordinates.
const GRID_THRESHOLD_M: f64 = 50_000.0;

/// Imports a DXF survey into a document (pure: returns the commands to run).
pub fn survey_from_bytes(
    bytes: &[u8],
    name: &str,
    doc: &Document,
    storey: Id,
) -> Result<Imported, ImportError> {
    let raw = kanavos_dxf::read_dxf(bytes, &kanavos_dxf::ImportOptions::default())
        .map_err(|e| ImportError::Format(e.to_string()))?;
    let mut site = doc.site().clone();
    let georef = &site.georef;
    let has_georef = georef.easting.abs() > 1.0 || georef.northing.abs() > 1.0;
    let far = raw.extents.is_some_and(|e| {
        e.min.x.abs().max(e.max.x.abs()) > GRID_THRESHOLD_M
            || e.min.y.abs().max(e.max.y.abs()) > GRID_THRESHOLD_M
    });
    let survey = if has_georef {
        raw.to_project(&kanavos_dxf::placement_from_georef(georef))
    } else if far {
        match raw.suggested_origin() {
            Some([e, n]) => {
                site.georef.easting = e;
                site.georef.northing = n;
                raw.translated([e, n])
            }
            None => raw,
        }
    } else {
        raw
    };
    let mut summary = ImportSummary { items: survey.drawing.items.len(), ..ImportSummary::default() };
    if site.plot.is_none()
        && let Some(plot) = survey.largest_closed_polyline().and_then(|i| survey.plot_polygon(i))
    {
        site.plot = Some(plot);
        summary.plot = true;
    }
    if site.terrain.is_empty() {
        site.terrain = survey.terrain_points();
        summary.terrain_points = site.terrain.len();
    }
    let mut commands: Vec<AnyCommand> = Vec::new();
    if &site != doc.site() {
        commands.push(SetSite { site }.into());
    }
    let underlay = survey.to_underlay(name, name);
    commands.push(CreateElement::on(storey, ElementKind::Underlay(underlay)).into());
    Ok(Imported::Commands(commands, summary))
}

/// Imports a file into the open project (`doc`, active storey `storey`).
pub fn import(kind: ImportKind, path: &Path, doc: &Document, storey: Id) -> Result<Imported, ImportError> {
    match kind {
        ImportKind::DxfSurvey => {
            let bytes = std::fs::read(path).map_err(ImportError::Io)?;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            survey_from_bytes(&bytes, &name, doc, storey)
        }
        ImportKind::Ifc => {
            let options = kanavos_ifc::ImportOptions {
                door_style: kanavos_objects::default_style(OpeningKind::Door).to_owned(),
                window_style: kanavos_objects::default_style(OpeningKind::Window).to_owned(),
                void_style: kanavos_objects::default_style(OpeningKind::Void).to_owned(),
                ..kanavos_ifc::ImportOptions::default()
            };
            let imported =
                kanavos_ifc::import_file(path, &options).map_err(|e| ImportError::Format(e.to_string()))?;
            let report = &imported.report;
            let summary = ImportSummary {
                items: report.mapped.values().sum(),
                skipped: report.skipped.values().map(|m| m.values().sum::<usize>()).sum(),
                ..ImportSummary::default()
            };
            Ok(Imported::Document(Box::new(imported.document), summary))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny ASCII DXF with a closed square plot in grid coordinates and one surveyed point.
    fn survey_dxf() -> String {
        let mut s = String::new();
        let mut push = |code: &str, value: &str| {
            s.push_str(code);
            s.push('\n');
            s.push_str(value);
            s.push('\n');
        };
        push("0", "SECTION");
        push("2", "ENTITIES");
        push("0", "LWPOLYLINE");
        push("8", "OIKOPEDO");
        push("90", "4");
        push("70", "1");
        for (x, y) in [
            (476_510.0, 4_097_320.0),
            (476_540.0, 4_097_320.0),
            (476_540.0, 4_097_350.0),
            (476_510.0, 4_097_350.0),
        ] {
            push("10", &format!("{x}"));
            push("20", &format!("{y}"));
        }
        push("0", "POINT");
        push("8", "POINTS");
        push("10", "476520.0");
        push("20", "4097330.0");
        push("30", "12.5");
        push("0", "ENDSEC");
        push("0", "EOF");
        s
    }

    #[test]
    fn a_grid_survey_becomes_underlay_plot_and_georeference() {
        let doc = kanavos_model::starter();
        let storey = doc.storeys().first().map(|s| s.id).unwrap();
        let Imported::Commands(commands, summary) =
            survey_from_bytes(survey_dxf().as_bytes(), "survey.dxf", &doc, storey).unwrap()
        else {
            panic!("expected commands")
        };
        assert!(summary.plot);
        assert_eq!(commands.len(), 2);
        let mut session = kanavos_engine::Session::new(doc);
        for c in commands {
            session.run(c).unwrap();
        }
        let doc = session.document();
        let plot = doc.site().plot.as_ref().unwrap();
        assert!((plot.area().as_m2() - 900.0).abs() < 1e-6);
        // The project origin moved near the survey, so plan coordinates are small.
        assert!(doc.site().georef.easting > 400_000.0);
        assert!(plot.vertices.iter().all(|p| p.x.as_m().abs() < 1_000.0 && p.y.as_m().abs() < 1_000.0));
        assert_eq!(doc.elements_of(kanavos_model::Category::Underlay).count(), 1);
    }

    #[test]
    fn garbage_is_reported_not_imported() {
        let doc = kanavos_model::starter();
        assert!(matches!(survey_from_bytes(b"", "x.dxf", &doc, Id(1)), Err(ImportError::Format(_))));
    }
}
