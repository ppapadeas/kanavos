//! Project information, the plot and its building terms, and georeferencing.

use kanavos_geom::{Angle, Length, Point2, Point3, Polygon2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Project information printed in title blocks and reports.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ProjectInfo {
    /// Project title (τίτλος έργου), e.g. "Νέα διώροφη κατοικία".
    pub name: String,
    /// Location or address (θέση).
    pub location: String,
    /// Municipality (δήμος).
    pub municipality: String,
    /// Owner or client (ιδιοκτήτης / κύριος του έργου).
    pub owner: String,
    /// Designer (μελετητής).
    pub designer: String,
    /// Designer details: registration number, address, contact.
    pub designer_details: String,
    /// Issue date as shown on sheets, e.g. "Οκτώβριος 2026".
    pub date: String,
    /// Revision label.
    pub revision: String,
    /// Free notes.
    pub notes: String,
}

/// Building terms of the plot (όροι δόμησης).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BuildingTerms {
    /// Building coefficient (συντελεστής δόμησης, Σ.Δ.).
    pub building_coefficient: f64,
    /// Maximum coverage ratio (ποσοστό κάλυψης), 0.0–1.0.
    pub coverage_ratio: f64,
    /// Maximum building height (μέγιστο επιτρεπόμενο ύψος).
    pub max_height: Length,
    /// Maximum number of floors, if set by the terms.
    #[serde(default)]
    pub max_floors: Option<u32>,
    /// Volume coefficient (συντελεστής όγκου), if set.
    #[serde(default)]
    pub volume_coefficient: Option<f64>,
    /// Fixed setback from the plot boundaries (απόσταση Δ); `None` applies the code's height-based rule.
    #[serde(default)]
    pub setback: Option<Length>,
}

impl Default for BuildingTerms {
    fn default() -> Self {
        // Typical terms outside a town plan in Greece; users set their own.
        BuildingTerms {
            building_coefficient: 0.8,
            coverage_ratio: 0.6,
            max_height: Length::m(11.0),
            max_floors: Some(3),
            volume_coefficient: None,
            setback: None,
        }
    }
}

/// Link between project coordinates and a national grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Georef {
    /// EPSG code of the grid (2100 = ΕΓΣΑ87).
    pub epsg: u32,
    /// Grid easting of the project origin, metres.
    pub easting: f64,
    /// Grid northing of the project origin, metres.
    pub northing: f64,
    /// Rotation from project +Y to grid north (counter-clockwise positive).
    pub rotation: Angle,
    /// Absolute altitude of the project's ±0.00, metres.
    pub altitude: f64,
}

impl Default for Georef {
    fn default() -> Self {
        Georef { epsg: 2100, easting: 0.0, northing: 0.0, rotation: Angle::ZERO, altitude: 0.0 }
    }
}

/// The plot and its context.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Site {
    /// Plot boundary (όρια οικοπέδου), project coordinates.
    pub plot: Option<Polygon2>,
    /// Street lines (ρυμοτομικές γραμμές) as polylines.
    pub street_lines: Vec<Vec<Point2>>,
    /// Building lines (οικοδομικές γραμμές) as polylines.
    pub building_lines: Vec<Vec<Point2>>,
    /// Building terms.
    pub terms: BuildingTerms,
    /// Georeferencing.
    pub georef: Georef,
    /// Surveyed terrain points (x, y, altitude relative to ±0.00), metres.
    pub terrain: Vec<Point3>,
    /// Natural ground level at the building relative to ±0.00 (used for height checks), metres.
    pub ground_level: Length,
}
