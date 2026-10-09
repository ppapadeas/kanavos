//! Views (plans, sections, elevations, 3D, schedules) and sheets.

use std::collections::BTreeSet;

use kanavos_geom::{Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

/// A 3D camera.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Camera {
    /// Eye position.
    pub eye: Point3,
    /// Point looked at.
    pub target: Point3,
    /// Vertical field of view in degrees (perspective), or the visible height in metres (parallel).
    pub fov: f64,
    /// Parallel (axonometric) projection instead of perspective.
    #[serde(default)]
    pub parallel: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            eye: Point3::new(-15.0, -20.0, 14.0),
            target: Point3::new(5.0, 4.0, 2.0),
            fov: 40.0,
            parallel: false,
        }
    }
}

/// Which schedule a schedule view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleKind {
    /// Doors.
    Doors,
    /// Windows.
    Windows,
    /// Spaces with areas.
    Spaces,
    /// Walls with lengths, areas and volumes.
    Walls,
    /// Slabs with areas and volumes.
    Slabs,
    /// Material quantities.
    Materials,
    /// Placed objects.
    Objects,
}

/// What a view shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewKind {
    /// Floor plan of a storey, cut at the storey's cut height.
    Plan {
        /// Storey.
        storey: Id,
    },
    /// Vertical section along a line. It looks to the left of `start → end` (to the right when `flip`).
    Section {
        /// Start of the section line.
        start: Point2,
        /// End of the section line.
        end: Point2,
        /// How far beyond the line the view sees.
        depth: Length,
        /// Look to the right instead of the left.
        #[serde(default)]
        flip: bool,
        /// Marker label shown at both ends in plans, e.g. "Α".
        #[serde(default)]
        marker: String,
    },
    /// Elevation: like a section, but nothing is cut; the line is usually outside the building.
    Elevation {
        /// Start of the elevation line.
        start: Point2,
        /// End of the elevation line.
        end: Point2,
        /// How far beyond the line the view sees.
        depth: Length,
        /// Look to the right instead of the left.
        #[serde(default)]
        flip: bool,
        /// Marker label shown in plans.
        #[serde(default)]
        marker: String,
    },
    /// 3D view.
    ThreeD {
        /// Camera.
        camera: Camera,
    },
    /// Site plan: plot, setbacks and building footprint.
    SitePlan,
    /// Coverage diagram (διάγραμμα κάλυψης) per ΝΟΚ.
    CoverageDiagram,
    /// Area table (πίνακας εμβαδών) per storey and category.
    AreaTable,
    /// A schedule.
    Schedule {
        /// Which schedule.
        schedule: ScheduleKind,
    },
}

/// A view: what to show and at which scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct View {
    /// Identifier.
    pub id: Id,
    /// Display name, e.g. "Κάτοψη ισογείου".
    pub name: String,
    /// What it shows.
    pub kind: ViewKind,
    /// Scale denominator (100 means 1:100).
    pub scale: u32,
    /// Drawing layer prefixes hidden in this view (visibility set), e.g. `"furniture"` or `"space.fill"`.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_layers: BTreeSet<String>,
}

/// ISO 216 A-series paper sizes, or a custom size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaperSize {
    /// 841 × 1189 mm.
    A0,
    /// 594 × 841 mm.
    A1,
    /// 420 × 594 mm.
    A2,
    /// 297 × 420 mm.
    A3,
    /// 210 × 297 mm.
    A4,
    /// Custom size in millimetres (width × height as given).
    Custom {
        /// Width, mm.
        width: f64,
        /// Height, mm.
        height: f64,
    },
}

/// A sheet's paper.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Paper {
    /// Size.
    pub size: PaperSize,
    /// Landscape orientation (long side horizontal). Ignored for custom sizes.
    #[serde(default)]
    pub landscape: bool,
}

impl Paper {
    /// Width and height in millimetres, with orientation applied.
    pub fn dimensions_mm(&self) -> (f64, f64) {
        let (short, long) = match self.size {
            PaperSize::A0 => (841.0, 1189.0),
            PaperSize::A1 => (594.0, 841.0),
            PaperSize::A2 => (420.0, 594.0),
            PaperSize::A3 => (297.0, 420.0),
            PaperSize::A4 => (210.0, 297.0),
            PaperSize::Custom { width, height } => return (width, height),
        };
        if self.landscape { (long, short) } else { (short, long) }
    }
}

/// A view placed on a sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Viewport {
    /// The view.
    pub view: Id,
    /// Centre of the viewport on the sheet, millimetres from the lower-left corner.
    pub centre: [f64; 2],
    /// Scale override; `None` uses the view's scale.
    #[serde(default)]
    pub scale: Option<u32>,
    /// Title under the viewport; `None` uses the view name.
    #[serde(default)]
    pub title: Option<String>,
    /// Model-space crop rectangle (two opposite corners); `None` shows everything.
    #[serde(default)]
    pub crop: Option<[Point2; 2]>,
}

/// A printable sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Sheet {
    /// Identifier.
    pub id: Id,
    /// Sheet number, e.g. "Α-01".
    pub number: String,
    /// Sheet title, e.g. "Κάτοψη ισογείου".
    pub name: String,
    /// Paper.
    pub paper: Paper,
    /// Placed views.
    #[serde(default)]
    pub viewports: Vec<Viewport>,
    /// Whether the title block is drawn.
    #[serde(default = "yes")]
    pub title_block: bool,
}

fn yes() -> bool {
    true
}
