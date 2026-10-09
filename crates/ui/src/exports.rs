//! Exports: the adapter between the File ▸ Export menu and the exporter crates.
//!
//! Available now: CSV of schedules and the area table (through `kanavos_drawing::table::Table::to_csv`).
//! **Pending merges** — each arm of [`export`] that returns [`ExportError::NotAvailable`] is the one place to
//! call the exporter once it lands:
//! - PDF of sheets: `kanavos_pdf::write_pdf`
//! - SVG of the active view or sheet: `kanavos_drawing::svg`
//! - DXF of the active view or sheet: `kanavos_dxf` export
//! - PNG of the active view or sheet: raster export from the drawing engineer's crate
//! - IFC of the model: `kanavos_ifc`
//! - glTF of the model: `kanavos-render` / `kanavos-views` scene export

use std::fmt;
use std::path::Path;

use kanavos_drawing::table::Table;
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
        matches!(self, ExportKind::Csv)
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
    /// The exporter is not part of this build yet.
    NotAvailable,
    /// The export needs a different source (e.g. CSV needs a schedule tab).
    WrongSource,
    /// Writing the file failed.
    Io(std::io::Error),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::NotAvailable => f.write_str("not available yet"),
            ExportError::WrongSource => f.write_str("nothing suitable to export"),
            ExportError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// Exports `source` from `doc` to `path`.
pub fn export(
    kind: ExportKind,
    doc: &Document,
    source: &ExportSource,
    locale: Locale,
    path: &Path,
) -> Result<(), ExportError> {
    let _ = doc;
    match (kind, source) {
        (ExportKind::Csv, ExportSource::Table(table)) => {
            std::fs::write(path, table.to_csv(locale)).map_err(ExportError::Io)
        }
        (ExportKind::Csv, _) => Err(ExportError::WrongSource),
        // Pending merges: see the module documentation.
        (ExportKind::SheetsPdf, _)
        | (ExportKind::Dxf, _)
        | (ExportKind::Svg, _)
        | (ExportKind::Png, _)
        | (ExportKind::Ifc, _)
        | (ExportKind::Gltf, _) => Err(ExportError::NotAvailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::table::{Align, Cell, Column};

    #[test]
    fn csv_of_a_table_is_written_with_locale_separators() {
        let table = Table {
            title: "T".into(),
            columns: vec![Column::new("Α", Align::Left, 10.0), Column::new("Β", Align::Right, 10.0)],
            rows: vec![vec![Cell::text("x"), Cell::num2(1.5)]],
            footer: vec![],
        };
        let path = std::env::temp_dir().join(format!("kanavos-export-{}.csv", std::process::id()));
        export(ExportKind::Csv, &Document::new(), &ExportSource::Table(table), Locale::El, &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("x;1,50"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pending_exporters_report_not_available() {
        let path = std::env::temp_dir().join("never-written.pdf");
        let r = export(ExportKind::SheetsPdf, &Document::new(), &ExportSource::Model, Locale::En, &path);
        assert!(matches!(r, Err(ExportError::NotAvailable)));
        assert!(!ExportKind::Ifc.available());
        assert!(ExportKind::Csv.available());
    }
}
