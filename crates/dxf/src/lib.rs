//! DXF import (survey underlays) and export (drawings).
//!
//! Layer L6. See `docs/architecture.md` and `docs/formats/dxf.md`.
//!
//! - [`write_dxf`] writes a drawing as an ASCII DXF file, format version AC1015 (R2000), with layers, line types,
//!   lineweights, solid hatches and expanded hatch lines.
//! - [`read_dxf`] reads ASCII DXF files from R12 to current versions into a [`kanavos_drawing::Drawing`] in metres,
//!   with blocks expanded, Greek text decoded from the file's code page, survey points with elevations, closed
//!   polylines (candidate plot boundaries) and layers. [`DxfImport`] moves the result from ΕΓΣΑ87 grid coordinates
//!   into project coordinates and builds the model's underlay and plot polygon.
//!
//! All behaviour follows the published DXF reference; nothing here depends on any CAD program's internals.

mod aci;
pub mod codepage;
mod export;
mod import;
mod mtext;
mod reader;
mod survey;

use std::fmt;

pub use aci::{aci_to_rgb, lineweight, rgb_to_aci};
pub use codepage::Codepage;
pub use export::{DxfExportOptions, DxfUnits, hatch_layer_name, layer_name, write_dxf};
pub use import::{ImportOptions, insunits_to_metres, parse_elevation, read_dxf};
pub use mtext::{decode_percent_codes, mtext_lines};
pub use survey::{ClosedPolyline, DxfImport, DxfLayer, PointSource, SurveyPoint, placement_from_georef};

/// Error reading a DXF file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxfError {
    /// The file is binary DXF, which is not supported (save it as ASCII DXF).
    Binary,
    /// The file has no content.
    Empty,
    /// The file is not valid DXF.
    Malformed {
        /// Line number where reading failed (1-based).
        line: usize,
        /// What was wrong.
        reason: String,
    },
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DxfError::Binary => write!(f, "binary DXF files are not supported; save the file as ASCII DXF"),
            DxfError::Empty => write!(f, "the file is empty"),
            DxfError::Malformed { line, reason } => write!(f, "not a valid DXF file (line {line}): {reason}"),
        }
    }
}

impl std::error::Error for DxfError {}
