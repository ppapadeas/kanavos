//! Elements: everything placed in the model, from walls to dimension lines.

use std::collections::BTreeMap;

use kanavos_drawing::{Drawing, HatchPattern, LinePattern, Rgba};
use kanavos_geom::{Angle, Arc2, Length, Point2, Polygon2, PolygonWithHoles, Segment2, Transform2, Vec2};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

/// Construction status of an element, used for alteration drawings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// New construction (default).
    #[default]
    New,
    /// Existing, to remain.
    Existing,
    /// Existing, to be demolished.
    Demolished,
}

/// A coarse classification used for visibility, selection filters, schedules and layer names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Walls.
    Wall,
    /// Floor and roof slabs.
    Slab,
    /// Columns.
    Column,
    /// Beams.
    Beam,
    /// Doors.
    Door,
    /// Windows.
    Window,
    /// Empty openings (voids) in walls.
    Opening,
    /// Stairs.
    Stair,
    /// Roofs.
    Roof,
    /// Railings.
    Railing,
    /// Spaces (rooms).
    Space,
    /// Furniture, fixtures and equipment.
    Object,
    /// Dimensions.
    Dimension,
    /// Text notes.
    Text,
    /// Level marks.
    Level,
    /// 2D drafting lines and fills.
    Drafting,
    /// Grid lines.
    Grid,
    /// Imported survey and reference drawings.
    Underlay,
}

impl Category {
    /// All categories, in display order.
    pub const ALL: [Category; 18] = [
        Category::Wall,
        Category::Slab,
        Category::Column,
        Category::Beam,
        Category::Door,
        Category::Window,
        Category::Opening,
        Category::Stair,
        Category::Roof,
        Category::Railing,
        Category::Space,
        Category::Object,
        Category::Dimension,
        Category::Text,
        Category::Level,
        Category::Drafting,
        Category::Grid,
        Category::Underlay,
    ];

    /// Stable lowercase key, used as the layer-name prefix in drawings (e.g. `"wall"` → `"wall.cut"`).
    pub fn key(self) -> &'static str {
        match self {
            Category::Wall => "wall",
            Category::Slab => "slab",
            Category::Column => "column",
            Category::Beam => "beam",
            Category::Door => "door",
            Category::Window => "window",
            Category::Opening => "opening",
            Category::Stair => "stair",
            Category::Roof => "roof",
            Category::Railing => "railing",
            Category::Space => "space",
            Category::Object => "object",
            Category::Dimension => "dimension",
            Category::Text => "text",
            Category::Level => "level",
            Category::Drafting => "drafting",
            Category::Grid => "grid",
            Category::Underlay => "underlay",
        }
    }

    /// Whether elements of this category are physical building parts (as opposed to annotation).
    pub fn is_building_part(self) -> bool {
        matches!(
            self,
            Category::Wall
                | Category::Slab
                | Category::Column
                | Category::Beam
                | Category::Door
                | Category::Window
                | Category::Opening
                | Category::Stair
                | Category::Roof
                | Category::Railing
                | Category::Object
        )
    }
}

/// What a layered element is made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Structure {
    /// One homogeneous material with a thickness.
    Basic {
        /// Material.
        material: Id,
        /// Thickness.
        thickness: Length,
    },
    /// A build-up of layers; the thickness is the sum of its layers.
    Layered {
        /// Build-up.
        build_up: Id,
    },
}

/// Where a wall's reference line sits relative to its thickness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceLine {
    /// The reference line is the wall's left face (looking from start to end); the thickness extends to the right.
    Left,
    /// Thickness is centred on the line.
    #[default]
    Centre,
    /// The reference line is the wall's right face; the thickness extends to the left.
    Right,
}

/// A wall: straight, or a circular arc when `bulge` is non-zero.
///
/// Build-up layers are listed from the wall's **left** face to its **right** face, looking from start to end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wall {
    /// Start of the reference line.
    pub start: Point2,
    /// End of the reference line.
    pub end: Point2,
    /// Arc bulge, `tan(sweep / 4)`; positive bends to the left (counter-clockwise). Zero is straight.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub bulge: f64,
    /// Position of the reference line across the thickness.
    #[serde(default)]
    pub reference: ReferenceLine,
    /// Material or build-up.
    pub structure: Structure,
    /// Height from the wall base.
    pub height: Length,
    /// Base relative to the storey's floor elevation.
    #[serde(default)]
    pub base_offset: Length,
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

impl Wall {
    /// The reference line as a segment (the chord for arc walls).
    pub fn segment(&self) -> Segment2 {
        Segment2::new(self.start, self.end)
    }
    /// The arc, for curved walls.
    pub fn arc(&self) -> Option<Arc2> {
        Arc2::from_bulge(self.start, self.end, self.bulge)
    }
    /// Length of the reference line (arc length for curved walls).
    pub fn length(&self) -> Length {
        match self.arc() {
            Some(a) => a.length(),
            None => self.start.distance(self.end),
        }
    }
    /// Unit direction from start to end (chord direction for arcs).
    pub fn direction(&self) -> Option<Vec2> {
        (self.end - self.start).normalized()
    }
    /// Offsets of the left and right faces from the reference line, measured to the left (positive) for a
    /// given total thickness. E.g. a centred 0.25 m wall gives `(0.125, -0.125)`.
    pub fn face_offsets(&self, thickness: Length) -> (f64, f64) {
        let t = thickness.as_m();
        match self.reference {
            ReferenceLine::Left => (0.0, -t),
            ReferenceLine::Centre => (t / 2.0, -t / 2.0),
            ReferenceLine::Right => (t, 0.0),
        }
    }
}

/// A floor, ceiling or other horizontal slab. Build-up layers are listed from top to bottom.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Slab {
    /// Plan outline.
    pub outline: PolygonWithHoles,
    /// Material or build-up; the slab extends downwards from its top.
    pub structure: Structure,
    /// Top surface relative to the storey's floor elevation (0 = at floor level).
    #[serde(default)]
    pub top_offset: Length,
}

/// Cross-section of a column or beam.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Profile {
    /// Rectangle. For columns: `width` along local X, `depth` along local Y. For beams: `width` horizontal,
    /// `depth` vertical.
    Rectangle {
        /// Width.
        width: Length,
        /// Depth.
        depth: Length,
    },
    /// Circle.
    Circle {
        /// Diameter.
        diameter: Length,
    },
}

/// A vertical column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Column {
    /// Centre in plan.
    pub position: Point2,
    /// Rotation of the profile.
    #[serde(default)]
    pub rotation: Angle,
    /// Cross-section.
    pub profile: Profile,
    /// Material.
    pub material: Id,
    /// Height from the column base.
    pub height: Length,
    /// Base relative to the storey's floor elevation.
    #[serde(default)]
    pub base_offset: Length,
}

/// A horizontal beam.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Beam {
    /// Start of the beam axis in plan.
    pub start: Point2,
    /// End of the beam axis in plan.
    pub end: Point2,
    /// Cross-section (`width` horizontal, `depth` vertical).
    pub profile: Profile,
    /// Material.
    pub material: Id,
    /// Top of the beam relative to the storey's floor elevation.
    pub top_offset: Length,
}

/// What an opening is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    /// A door.
    #[default]
    Door,
    /// A window.
    Window,
    /// An empty opening.
    Void,
}

/// A door, window or empty opening in a wall.
///
/// Position is measured along the host wall's reference line from its start to the opening's centre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Opening {
    /// Host wall.
    pub host: Id,
    /// Door, window or void.
    pub kind: OpeningKind,
    /// Object style from the catalogue in `kanavos-objects`, e.g. `"door.single_swing"`.
    pub style: String,
    /// Distance along the host's reference line from its start to the opening centre.
    pub offset: Length,
    /// Clear width.
    pub width: Length,
    /// Clear height.
    pub height: Length,
    /// Bottom of the opening above the storey's floor elevation (0 for doors).
    pub sill: Length,
    /// Mirror the opening across its wall. Unset, a window's outer face is the wall's left face and a door opens
    /// towards the right face, as suits an exterior wall drawn clockwise (`specs/objects/orientation.md`).
    #[serde(default)]
    pub flip_side: bool,
    /// Swap the hinge jamb.
    #[serde(default)]
    pub flip_hand: bool,
    /// Style-specific numeric parameters (frame width, number of leaves …); see the object catalogue.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, f64>,
}

impl Opening {
    /// Top of the opening above the storey floor.
    pub fn head(&self) -> Length {
        self.sill + self.height
    }
}

/// Plan shape of a stair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StairShape {
    /// One straight flight.
    #[default]
    Straight,
    /// Two flights with a square landing, turning left by 90°.
    QuarterTurnLeft,
    /// Two flights with a square landing, turning right by 90°.
    QuarterTurnRight,
    /// Two parallel flights with a landing, turning left by 180°.
    HalfTurnLeft,
    /// Two parallel flights with a landing, turning right by 180°.
    HalfTurnRight,
}

/// What a stair serves. It decides which limits of the Greek building regulation apply to it (Κτιριοδομικός
/// Κανονισμός, Υ.Α. ΥΠΕΝ/ΔΑΟΚΑ/66006/2360, ΦΕΚ Β΄ 3985/22.06.2023, άρθρο 11): the clear width of §2 and the stair
/// types I–III of the table in §4 that §4.1–§4.5 allow for each use of the building. The limits themselves are
/// in `kanavos-elements` (`stair_limits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StairUse {
    /// The internal stair of a single dwelling (εσωτερική κλίμακα κατοικίας): clear width from 0.60 m (§2); any
    /// of types I–III (§4.1).
    #[default]
    DwellingInternal,
    /// A stair shared by the dwellings of a residential building (category Α): clear width 1.20 m, or 0.90 m in
    /// buildings of up to three storeys (§2); any of types I–III (§4.1).
    Residential,
    /// Temporary accommodation, correctional, commerce, offices, industry, storage and parking buildings
    /// (categories Β, Ζ, Η, Θ, Ι, Κ, Λ): types I or II (§4.2).
    OtherUses,
    /// Assembly and education buildings (categories Γ, Δ): type I (§4.4).
    AssemblyEducation,
    /// Health and social care buildings (category Ε): type I (§4.3).
    HealthCare,
    /// A stair serving only auxiliary rooms of an industrial building, or only storage rooms of a commerce or
    /// storage building, with the relaxed limits of §4.5.
    Auxiliary,
}

impl StairUse {
    /// Every use, in display order.
    pub const ALL: [StairUse; 6] = [
        StairUse::DwellingInternal,
        StairUse::Residential,
        StairUse::OtherUses,
        StairUse::AssemblyEducation,
        StairUse::HealthCare,
        StairUse::Auxiliary,
    ];

    /// Stable snake_case key, as in the native format.
    pub fn key(self) -> &'static str {
        match self {
            StairUse::DwellingInternal => "dwelling_internal",
            StairUse::Residential => "residential",
            StairUse::OtherUses => "other_uses",
            StairUse::AssemblyEducation => "assembly_education",
            StairUse::HealthCare => "health_care",
            StairUse::Auxiliary => "auxiliary",
        }
    }

    /// Whether this is the default use (left out of saved files).
    pub fn is_default(&self) -> bool {
        *self == StairUse::default()
    }
}

/// A stair.
///
/// `origin` is the midpoint of the front edge of the first step; `direction` is the walking direction up the
/// first flight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stair {
    /// Midpoint of the first riser's front edge.
    pub origin: Point2,
    /// Walking direction up the first flight.
    pub direction: Angle,
    /// Plan shape.
    #[serde(default)]
    pub shape: StairShape,
    /// Clear width of the flights.
    pub width: Length,
    /// Total rise.
    pub rise: Length,
    /// Number of risers.
    pub risers: u32,
    /// Tread going (horizontal depth of each step).
    pub going: Length,
    /// Risers in the first flight for two-flight shapes; `None` splits evenly.
    #[serde(default)]
    pub first_flight_risers: Option<u32>,
    /// Gap between the two flights of a half-turn stair.
    #[serde(default)]
    pub gap: Length,
    /// Waist (structural thickness under the steps).
    pub thickness: Length,
    /// Material.
    pub material: Id,
    /// What the stair serves, for the regulation's checks; default the internal stair of a dwelling.
    #[serde(default, skip_serializing_if = "StairUse::is_default")]
    pub usage: StairUse,
    /// Base relative to the storey's floor elevation.
    #[serde(default)]
    pub base_offset: Length,
}

impl Stair {
    /// Height of one riser.
    pub fn riser_height(&self) -> Length {
        if self.risers == 0 { Length::ZERO } else { self.rise / f64::from(self.risers) }
    }
}

/// A parapet around a flat roof.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Parapet {
    /// Height above the roof top surface.
    pub height: Length,
    /// Thickness.
    pub thickness: Length,
}

/// Geometry rule of a roof.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoofShape {
    /// A flat roof, optionally with a parapet along the outline.
    Flat {
        /// Parapet, if any.
        #[serde(default)]
        parapet: Option<Parapet>,
    },
    /// A pitched roof. One entry per outline edge: `Some(pitch)` makes a roof plane rising inwards from that
    /// edge at that pitch; `None` makes the edge a gable (vertical end). One sloped edge gives a mono-pitch
    /// roof, two opposite edges a gable roof, all edges a hip roof.
    Pitched {
        /// Pitch per outline edge, or `None` for gable edges.
        slopes: Vec<Option<Angle>>,
    },
}

/// A roof over an outline (eaves line, including the overhang).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Roof {
    /// Plan outline at the eaves.
    pub outline: Polygon2,
    /// Openings through the roof in plan (light wells, skylights, chimneys, a courtyard): each a polygon inside the
    /// outline, not overlapping the others. They are cut vertically through the roof planes, which keep the slope
    /// they have over the outline.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Polygon2>,
    /// Height of the roof's top surface at the eaves line, relative to the storey's floor elevation.
    pub base: Length,
    /// Material or build-up (layers top to bottom); thickness is measured perpendicular to the roof plane.
    pub structure: Structure,
    /// Flat or pitched.
    pub shape: RoofShape,
}

/// Infill of a railing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RailingInfill {
    /// Vertical bars.
    #[default]
    Bars,
    /// Glass panels.
    Glass,
    /// Solid panels.
    Solid,
}

/// A railing along a path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Railing {
    /// Plan path (two or more points).
    pub path: Vec<Point2>,
    /// Height of the top rail above the base.
    pub height: Length,
    /// Base relative to the storey's floor elevation.
    #[serde(default)]
    pub base_offset: Length,
    /// Maximum distance between posts.
    pub post_spacing: Length,
    /// Infill.
    #[serde(default)]
    pub infill: RailingInfill,
    /// Material.
    pub material: Id,
}

/// Use category of a space, as needed for the Greek building code (ΝΟΚ, ν. 4067/2012) area calculations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SpaceCategory {
    /// Main use (κύρια χρήση): living rooms, bedrooms, kitchens, bathrooms, offices.
    #[default]
    MainUse,
    /// Auxiliary use (βοηθητική χρήση): storage, boiler room, laundry.
    Auxiliary,
    /// Stairwell (κλιμακοστάσιο).
    Stairwell,
    /// Lift shaft (φρεάτιο ανελκυστήρα).
    LiftShaft,
    /// Semi-open space (ημιυπαίθριος χώρος): covered, open on at least one side.
    SemiOpen,
    /// Balcony (εξώστης): open, uncovered or cantilevered platform.
    Balcony,
    /// Open ground floor on columns (πυλωτή).
    Pilotis,
    /// Enclosed parking (στάθμευση).
    Parking,
    /// Attic space under a pitched roof (σοφίτα).
    Attic,
    /// Plant and technical rooms (μηχανοστάσιο, τεχνικός χώρος).
    Technical,
    /// Light and ventilation shafts (φωταγωγός, αεραγωγός).
    Shaft,
    /// Anything else.
    Other,
}

/// A space (room), bounded by walls or drawn by hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Space {
    /// Plan boundary (inside faces of the bounding elements).
    pub boundary: PolygonWithHoles,
    /// Name, e.g. "Καθιστικό".
    pub name: String,
    /// Number, e.g. "0.01".
    #[serde(default)]
    pub number: String,
    /// Use category.
    #[serde(default)]
    pub category: SpaceCategory,
    /// Clear height; `None` derives it from the storey.
    #[serde(default)]
    pub height: Option<Length>,
    /// Position of the label; `None` places it at the centroid.
    #[serde(default)]
    pub stamp: Option<Point2>,
    /// Floor finish description for schedules.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub finish: String,
}

/// A placed object from the catalogue: furniture, sanitary fixture, kitchen unit, equipment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Object {
    /// Catalogue identifier, e.g. `"furniture.bed_double"`.
    pub definition: String,
    /// Insertion point in plan.
    pub position: Point2,
    /// Rotation about the insertion point.
    #[serde(default)]
    pub rotation: Angle,
    /// Mirrored about the object's local Y axis.
    #[serde(default)]
    pub mirrored: bool,
    /// Elevation of the object's base above the storey's floor.
    #[serde(default)]
    pub elevation: Length,
    /// Parameter values overriding the catalogue defaults (dimensions in metres).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, f64>,
}

/// A linear dimension chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Dimension {
    /// Measured points in order; two points give one dimension, more give a chain.
    pub points: Vec<Point2>,
    /// A point the dimension line passes through.
    pub line_point: Point2,
    /// Direction of the dimension line; distances are measured along it.
    pub direction: Angle,
}

/// Horizontal text alignment of a text note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    /// Left.
    #[default]
    Left,
    /// Centre.
    Centre,
    /// Right.
    Right,
}

/// A text note. Several lines are separated by `\n`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextNote {
    /// Position of the first line's baseline start (or centre/end, by alignment).
    pub position: Point2,
    /// Text.
    pub content: String,
    /// Cap height on paper, millimetres.
    pub size_mm: f64,
    /// Rotation.
    #[serde(default)]
    pub rotation: Angle,
    /// Alignment.
    #[serde(default)]
    pub align: TextAlign,
    /// Bold.
    #[serde(default)]
    pub bold: bool,
}

/// A level mark (στάθμη). In plans it shows a floor level; in sections and elevations a height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelMark {
    /// Position of the mark (in sections and elevations, `y` is the elevation when `value` is `None`).
    pub position: Point2,
    /// Value shown; `None` uses the storey's floor elevation in plans and `position.y` in sections.
    #[serde(default)]
    pub value: Option<Length>,
    /// Optional label, e.g. "Δάπεδο".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// Geometry of a drafting item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DraftShape {
    /// A line.
    Line {
        /// Start.
        a: Point2,
        /// End.
        b: Point2,
    },
    /// A polyline.
    Polyline {
        /// Vertices.
        points: Vec<Point2>,
        /// Closed.
        closed: bool,
    },
    /// An arc.
    Arc {
        /// The arc.
        arc: Arc2,
    },
    /// A circle.
    Circle {
        /// Centre.
        centre: Point2,
        /// Radius.
        radius: Length,
    },
    /// A filled region.
    Fill {
        /// Region.
        region: PolygonWithHoles,
    },
}

/// Appearance of a drafting item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DraftStyle {
    /// Line weight, paper millimetres.
    pub weight_mm: f64,
    /// Line pattern.
    #[serde(default)]
    pub pattern: LinePattern,
    /// Colour.
    pub color: Rgba,
    /// Hatch for fills.
    #[serde(default)]
    pub hatch: HatchPattern,
    /// Background colour for fills.
    #[serde(default)]
    pub background: Option<Rgba>,
}

impl Default for DraftStyle {
    fn default() -> Self {
        DraftStyle {
            weight_mm: 0.25,
            pattern: LinePattern::Solid,
            color: Rgba::BLACK,
            hatch: HatchPattern::None,
            background: None,
        }
    }
}

/// A 2D drafting item (lines, arcs, fills) in a plan or another view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Drafting {
    /// Geometry.
    pub shape: DraftShape,
    /// Appearance.
    #[serde(default)]
    pub style: DraftStyle,
}

/// A grid line (άξονας κανάβου), shown on every plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GridLine {
    /// Label in the bubble, e.g. "A" or "1".
    pub label: String,
    /// Start.
    pub start: Point2,
    /// End.
    pub end: Point2,
}

/// An imported reference drawing (e.g. a DXF survey), in project coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Underlay {
    /// Display name.
    pub name: String,
    /// Original file name.
    #[serde(default)]
    pub source: String,
    /// Geometry in metres, project coordinates.
    pub drawing: Drawing,
}

/// Type-specific data of an element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    /// A wall.
    Wall(Wall),
    /// A slab.
    Slab(Slab),
    /// A column.
    Column(Column),
    /// A beam.
    Beam(Beam),
    /// A door, window or void in a wall.
    Opening(Opening),
    /// A stair.
    Stair(Stair),
    /// A roof.
    Roof(Roof),
    /// A railing.
    Railing(Railing),
    /// A space.
    Space(Space),
    /// A catalogue object.
    Object(Object),
    /// A dimension chain.
    Dimension(Dimension),
    /// A text note.
    Text(TextNote),
    /// A level mark.
    Level(LevelMark),
    /// 2D drafting.
    Drafting(Drafting),
    /// A grid line.
    Grid(GridLine),
    /// An imported reference drawing.
    Underlay(Underlay),
}

impl ElementKind {
    /// Category of the element.
    pub fn category(&self) -> Category {
        match self {
            ElementKind::Wall(_) => Category::Wall,
            ElementKind::Slab(_) => Category::Slab,
            ElementKind::Column(_) => Category::Column,
            ElementKind::Beam(_) => Category::Beam,
            ElementKind::Opening(o) => match o.kind {
                OpeningKind::Door => Category::Door,
                OpeningKind::Window => Category::Window,
                OpeningKind::Void => Category::Opening,
            },
            ElementKind::Stair(_) => Category::Stair,
            ElementKind::Roof(_) => Category::Roof,
            ElementKind::Railing(_) => Category::Railing,
            ElementKind::Space(_) => Category::Space,
            ElementKind::Object(_) => Category::Object,
            ElementKind::Dimension(_) => Category::Dimension,
            ElementKind::Text(_) => Category::Text,
            ElementKind::Level(_) => Category::Level,
            ElementKind::Drafting(_) => Category::Drafting,
            ElementKind::Grid(_) => Category::Grid,
            ElementKind::Underlay(_) => Category::Underlay,
        }
    }

    /// A short stable type name, e.g. `"wall"`.
    pub fn type_name(&self) -> &'static str {
        match self {
            ElementKind::Wall(_) => "wall",
            ElementKind::Slab(_) => "slab",
            ElementKind::Column(_) => "column",
            ElementKind::Beam(_) => "beam",
            ElementKind::Opening(_) => "opening",
            ElementKind::Stair(_) => "stair",
            ElementKind::Roof(_) => "roof",
            ElementKind::Railing(_) => "railing",
            ElementKind::Space(_) => "space",
            ElementKind::Object(_) => "object",
            ElementKind::Dimension(_) => "dimension",
            ElementKind::Text(_) => "text",
            ElementKind::Level(_) => "level",
            ElementKind::Drafting(_) => "drafting",
            ElementKind::Grid(_) => "grid",
            ElementKind::Underlay(_) => "underlay",
        }
    }

    /// The element with its plan geometry mapped through a transform (move, rotate, mirror).
    ///
    /// Openings are positioned relative to their host and are returned unchanged; move the host instead.
    /// Mirroring swaps wall ends so the wall keeps its face order, and flips objects.
    pub fn transformed(&self, t: &Transform2) -> ElementKind {
        let p = |q: Point2| t.apply(q);
        let rot = t.rotation_angle();
        let mirror = t.is_mirroring();
        match self {
            ElementKind::Wall(w) => {
                let mut w = w.clone();
                if mirror {
                    // Swapping ends keeps the build-up's left/right sense and the arc's bend.
                    let (s, e) = (p(w.end), p(w.start));
                    w.start = s;
                    w.end = e;
                } else {
                    w.start = p(w.start);
                    w.end = p(w.end);
                }
                ElementKind::Wall(w)
            }
            ElementKind::Slab(s) => {
                let mut s = s.clone();
                s.outline = s.outline.transformed(t);
                ElementKind::Slab(s)
            }
            ElementKind::Column(c) => {
                let mut c = c.clone();
                c.position = p(c.position);
                c.rotation = if mirror { -c.rotation + rot } else { c.rotation + rot };
                ElementKind::Column(c)
            }
            ElementKind::Beam(b) => {
                let mut b = b.clone();
                b.start = p(b.start);
                b.end = p(b.end);
                ElementKind::Beam(b)
            }
            ElementKind::Opening(o) => ElementKind::Opening(o.clone()),
            ElementKind::Stair(s) => {
                let mut s = s.clone();
                let dir = t.apply_vec(Vec2::from_angle(s.direction));
                s.origin = p(s.origin);
                s.direction = dir.angle();
                if mirror {
                    s.shape = match s.shape {
                        StairShape::QuarterTurnLeft => StairShape::QuarterTurnRight,
                        StairShape::QuarterTurnRight => StairShape::QuarterTurnLeft,
                        StairShape::HalfTurnLeft => StairShape::HalfTurnRight,
                        StairShape::HalfTurnRight => StairShape::HalfTurnLeft,
                        StairShape::Straight => StairShape::Straight,
                    };
                }
                ElementKind::Stair(s)
            }
            ElementKind::Roof(r) => {
                let mut r = r.clone();
                r.outline = Polygon2::new(r.outline.vertices.iter().map(|&q| p(q)).collect());
                r.holes = r.holes.iter().map(|h| h.transformed(t)).collect();
                if mirror {
                    // Reverse the ring and keep each slope attached to the same physical edge.
                    r.outline.vertices.reverse();
                    if let RoofShape::Pitched { slopes } = &mut r.shape {
                        let n = slopes.len();
                        if n > 1 {
                            // Edge i (v_i → v_{i+1}) becomes edge n-2-i after reversal (mod n).
                            let old = slopes.clone();
                            for (i, s) in old.into_iter().enumerate() {
                                let j = (2 * n - 2 - i) % n;
                                if let Some(slot) = slopes.get_mut(j) {
                                    *slot = s;
                                }
                            }
                        }
                    }
                }
                ElementKind::Roof(r)
            }
            ElementKind::Railing(r) => {
                let mut r = r.clone();
                r.path = r.path.iter().map(|&q| p(q)).collect();
                ElementKind::Railing(r)
            }
            ElementKind::Space(s) => {
                let mut s = s.clone();
                s.boundary = s.boundary.transformed(t);
                s.stamp = s.stamp.map(p);
                ElementKind::Space(s)
            }
            ElementKind::Object(o) => {
                let mut o = o.clone();
                o.position = p(o.position);
                if mirror {
                    o.mirrored = !o.mirrored;
                    o.rotation = -o.rotation + rot;
                } else {
                    o.rotation += rot;
                }
                ElementKind::Object(o)
            }
            ElementKind::Dimension(d) => {
                let mut d = d.clone();
                d.points = d.points.iter().map(|&q| p(q)).collect();
                d.line_point = p(d.line_point);
                d.direction = t.apply_vec(Vec2::from_angle(d.direction)).angle();
                ElementKind::Dimension(d)
            }
            ElementKind::Text(x) => {
                let mut x = x.clone();
                x.position = p(x.position);
                x.rotation += rot;
                ElementKind::Text(x)
            }
            ElementKind::Level(l) => {
                let mut l = l.clone();
                l.position = p(l.position);
                ElementKind::Level(l)
            }
            ElementKind::Drafting(d) => {
                let mut d = d.clone();
                d.shape = match &d.shape {
                    DraftShape::Line { a, b } => DraftShape::Line { a: p(*a), b: p(*b) },
                    DraftShape::Polyline { points, closed } => DraftShape::Polyline {
                        points: points.iter().map(|&q| p(q)).collect(),
                        closed: *closed,
                    },
                    DraftShape::Arc { arc } => {
                        let a0 = p(arc.at(0.0));
                        let c = p(arc.centre);
                        let sweep = if mirror { -arc.sweep } else { arc.sweep };
                        DraftShape::Arc {
                            arc: Arc2 { centre: c, radius: arc.radius, start: (a0 - c).angle(), sweep },
                        }
                    }
                    DraftShape::Circle { centre, radius } => {
                        DraftShape::Circle { centre: p(*centre), radius: *radius }
                    }
                    DraftShape::Fill { region } => DraftShape::Fill { region: region.transformed(t) },
                };
                ElementKind::Drafting(d)
            }
            ElementKind::Grid(g) => {
                let mut g = g.clone();
                g.start = p(g.start);
                g.end = p(g.end);
                ElementKind::Grid(g)
            }
            ElementKind::Underlay(u) => ElementKind::Underlay(u.clone()),
        }
    }
}

/// Anything placed in the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Element {
    /// Identifier.
    pub id: Id,
    /// Storey the element belongs to; `None` for project-wide items such as grid lines and for annotations that
    /// belong to a section or elevation view.
    #[serde(default)]
    pub storey: Option<Id>,
    /// For annotations drawn in a section, elevation or other non-plan view: that view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<Id>,
    /// Construction status.
    #[serde(default, skip_serializing_if = "is_new")]
    pub phase: Phase,
    /// User mark shown in schedules and labels, e.g. "Π1".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    /// Free user properties.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
    /// Identifier of this element in the file or system it came from, e.g. the IFC `GlobalId` it was imported
    /// with, so that exports can write it again. Not edited by people; copies of the element do not carry it
    /// (see [`Element::copied_as`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// Type-specific data.
    pub kind: ElementKind,
}

fn is_new(p: &Phase) -> bool {
    *p == Phase::New
}

impl Element {
    /// A new-construction element on a storey, with no tag or properties.
    pub fn on_storey(id: Id, storey: Id, kind: ElementKind) -> Element {
        Element {
            id,
            storey: Some(storey),
            view: None,
            phase: Phase::New,
            tag: String::new(),
            properties: BTreeMap::new(),
            external_id: None,
            kind,
        }
    }

    /// A copy of the element under a new id. The copy is a new object, so it has no external id: two elements
    /// must never claim the same identity in another system.
    pub fn copied_as(&self, id: Id) -> Element {
        Element { id, external_id: None, ..self.clone() }
    }

    /// Category of the element.
    pub fn category(&self) -> Category {
        self.kind.category()
    }

    /// The wall data, if this is a wall.
    pub fn as_wall(&self) -> Option<&Wall> {
        match &self.kind {
            ElementKind::Wall(w) => Some(w),
            _ => None,
        }
    }

    /// The opening data, if this is a door, window or void.
    pub fn as_opening(&self) -> Option<&Opening> {
        match &self.kind {
            ElementKind::Opening(o) => Some(o),
            _ => None,
        }
    }

    /// The space data, if this is a space.
    pub fn as_space(&self) -> Option<&Space> {
        match &self.kind {
            ElementKind::Space(s) => Some(s),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Tolerance;

    fn wall() -> Wall {
        Wall {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(4.0, 0.0),
            bulge: 0.0,
            reference: ReferenceLine::Centre,
            structure: Structure::Basic { material: Id(1), thickness: Length::cm(25.0) },
            height: Length::m(3.0),
            base_offset: Length::ZERO,
        }
    }

    #[test]
    fn face_offsets_follow_reference() {
        let mut w = wall();
        let t = Length::cm(20.0);
        assert_eq!(w.face_offsets(t), (0.1, -0.1));
        w.reference = ReferenceLine::Left;
        assert_eq!(w.face_offsets(t), (0.0, -0.2));
    }

    #[test]
    fn mirroring_a_wall_swaps_its_ends() {
        let tol = Tolerance::default();
        let m = Transform2::mirror(Point2::new(0.0, 0.0), Point2::new(0.0, 1.0));
        let ElementKind::Wall(w) = ElementKind::Wall(wall()).transformed(&m) else { panic!("not a wall") };
        assert!(w.start.coincides(Point2::new(-4.0, 0.0), &tol));
        assert!(w.end.coincides(Point2::new(0.0, 0.0), &tol));
    }

    #[test]
    fn element_json_is_tagged() {
        let e = Element::on_storey(Id(5), Id(1), ElementKind::Wall(wall()));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"]["type"], "wall");
        assert_eq!(v["storey"], 1);
        let back: Element = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn external_ids_are_optional_and_not_copied() {
        let mut e = Element::on_storey(Id(5), Id(1), ElementKind::Wall(wall()));
        assert!(serde_json::to_value(&e).unwrap().get("external_id").is_none());
        e.external_id = Some("2O2Fr$t4X7Zf8NOew3FLOH".into());
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["external_id"], "2O2Fr$t4X7Zf8NOew3FLOH");
        assert_eq!(serde_json::from_value::<Element>(v).unwrap(), e);
        let copy = e.copied_as(Id(9));
        assert_eq!(copy.id, Id(9));
        assert_eq!(copy.external_id, None);
        assert_eq!(copy.kind, e.kind);
    }

    #[test]
    fn roof_mirror_keeps_slopes_on_their_edges() {
        let roof = Roof {
            outline: Polygon2::from_m(&[(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (0.0, 2.0)]),
            holes: vec![Polygon2::from_m(&[(1.0, 0.5), (2.0, 0.5), (2.0, 1.0), (1.0, 1.0)])],
            base: Length::m(3.0),
            structure: Structure::Basic { material: Id(1), thickness: Length::cm(20.0) },
            // Only the south edge (0,0)→(4,0) slopes.
            shape: RoofShape::Pitched { slopes: vec![Some(Angle::deg(30.0)), None, None, None] },
        };
        let m = Transform2::mirror(Point2::new(0.0, 1.0), Point2::new(1.0, 1.0)); // about y = 1
        let ElementKind::Roof(r) = ElementKind::Roof(roof).transformed(&m) else { panic!("not a roof") };
        let RoofShape::Pitched { slopes } = &r.shape else { panic!("not pitched") };
        // The sloped edge is now the north edge y = 2.
        let i = slopes.iter().position(Option::is_some).unwrap();
        let e = r.outline.edge(i).unwrap();
        let tol = Tolerance::default();
        assert!(tol.equal_m(e.start.y.as_m(), 2.0) && tol.equal_m(e.end.y.as_m(), 2.0));
        // Holes are mirrored with the outline: (1..2, 0.5..1) becomes (1..2, 1..1.5).
        let b = r.holes[0].bbox().unwrap();
        assert!(tol.equal_m(b.min.y.as_m(), 1.0) && tol.equal_m(b.max.y.as_m(), 1.5));
    }

    #[test]
    fn roofs_without_holes_read_and_write_without_the_key() {
        let json = r#"{"outline": [[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]], "base": 3.0,
            "structure": {"type": "basic", "material": 1, "thickness": 0.2}, "shape": {"type": "flat"}}"#;
        let r: Roof = serde_json::from_str(json).unwrap();
        assert!(r.holes.is_empty());
        assert!(serde_json::to_value(&r).unwrap().get("holes").is_none());
    }
}
