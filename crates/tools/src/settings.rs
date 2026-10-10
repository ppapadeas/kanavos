//! Tool parameters edited in the context bar.
//!
//! Every tool reads its parameters from [`ToolSettings`] on each event, so a change in the context bar applies to
//! the very next click. Fields that are `Option` fall back to the document's defaults (see the `*_for` helpers).

use kanavos_geom::Angle;
use kanavos_model::{
    Document, DraftStyle, Id, Parapet, Profile, RailingInfill, ReferenceLine, SpaceCategory, StairShape,
    StairUse, Structure, TextAlign,
};
use kanavos_units::Length;

/// How the wall tool turns clicks into walls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallMode {
    /// Each click adds a straight wall to a chain.
    #[default]
    Chain,
    /// Two opposite corners make four walls.
    Rectangle,
    /// Start, end and a point on the arc make a curved wall; the chain continues from the end.
    Arc,
}

/// How outline tools (slab, roof, space, fill) get their outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutlineMode {
    /// Click the vertices; click the first one again, double-click or press Enter to close.
    #[default]
    Polygon,
    /// Two opposite corners of an axis-aligned rectangle.
    Rectangle,
    /// Click inside an area enclosed by walls.
    PickInside,
}

/// Orientation of a dimension line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DimensionOrientation {
    /// Along the line from the first to the last measured point.
    #[default]
    Aligned,
    /// Horizontal (measures X distances).
    Horizontal,
    /// Vertical (measures Y distances).
    Vertical,
}

/// Wall tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct WallSettings {
    /// Material or build-up; `None` uses the document's default exterior wall.
    pub structure: Option<Structure>,
    /// Height; `None` uses the storey height.
    pub height: Option<Length>,
    /// Reference line position.
    pub reference: ReferenceLine,
    /// Base offset from the storey floor.
    pub base_offset: Length,
    /// Chain, rectangle or arc.
    pub mode: WallMode,
}

/// Slab tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SlabSettings {
    /// Material or build-up; `None` uses the default floor slab (the ground slab on the lowest storey).
    pub structure: Option<Structure>,
    /// Top relative to the storey floor.
    pub top_offset: Length,
    /// How the outline is drawn.
    pub mode: OutlineMode,
}

/// Column tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSettings {
    /// Cross-section.
    pub profile: Profile,
    /// Material; `None` uses the default structural material.
    pub material: Option<Id>,
    /// Height; `None` uses the storey height.
    pub height: Option<Length>,
    /// Rotation of the profile for a plain click.
    pub rotation: Angle,
    /// When set, a second click sets the rotation.
    pub pick_rotation: bool,
}

/// Beam tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct BeamSettings {
    /// Cross-section.
    pub profile: Profile,
    /// Material; `None` uses the default structural material.
    pub material: Option<Id>,
    /// Top relative to the storey floor; `None` uses the storey height.
    pub top_offset: Option<Length>,
}

/// Door, window and void tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct OpeningSettings {
    /// Catalogue style for doors.
    pub door_style: String,
    /// Catalogue style for windows.
    pub window_style: String,
    /// Door width and height.
    pub door_size: (Length, Length),
    /// Window width and height.
    pub window_size: (Length, Length),
    /// Window sill above the floor.
    pub window_sill: Length,
    /// Void width and height.
    pub void_size: (Length, Length),
    /// Step for the distance between the nearest wall end and the opening's jamb.
    pub step: Length,
}

/// Stair tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct StairSettings {
    /// Shape.
    pub shape: StairShape,
    /// Flight width.
    pub width: Length,
    /// Tread going.
    pub going: Length,
    /// Maximum riser height (the number of risers follows from the storey height).
    pub max_riser: Length,
    /// Waist thickness under the steps.
    pub thickness: Length,
    /// Gap between the two flights of a half-turn stair.
    pub gap: Length,
    /// What the stair serves (decides the regulation's checks).
    pub usage: StairUse,
}

/// Roof shape choice in the roof tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoofKind {
    /// Flat roof with a parapet.
    Flat,
    /// One sloped edge.
    MonoPitch,
    /// Two opposite sloped edges.
    Gable,
    /// All edges sloped.
    Hip,
}

/// Roof tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct RoofSettings {
    /// Shape.
    pub kind: RoofKind,
    /// Pitch of sloped edges.
    pub pitch: Angle,
    /// Overhang beyond the drawn outline.
    pub overhang: Length,
    /// Material or build-up; `None` uses the default for the kind.
    pub structure: Option<Structure>,
    /// How the outline is drawn.
    pub mode: OutlineMode,
    /// Height of the roof's top surface at the eaves above the storey floor; `None` uses the storey height.
    pub base: Option<Length>,
    /// Parapet of flat roofs; `None` for none.
    pub parapet: Option<Parapet>,
}

/// Railing tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct RailingSettings {
    /// Height.
    pub height: Length,
    /// Infill.
    pub infill: RailingInfill,
    /// Maximum distance between posts.
    pub post_spacing: Length,
}

/// Space tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceSettings {
    /// Name for the next space.
    pub name: String,
    /// Category for the next space.
    pub category: SpaceCategory,
    /// Click inside walls (with polygon drawing as the fallback), or draw a polygon or rectangle.
    pub mode: OutlineMode,
    /// Number new spaces `<storey>.<nn>` (e.g. `0.03`), continuing from the highest number on the storey.
    pub auto_number: bool,
}

/// Text tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSettings {
    /// Size on paper, mm.
    pub size_mm: f64,
    /// Alignment.
    pub align: TextAlign,
    /// Bold.
    pub bold: bool,
}

/// Object tool parameters beyond the catalogue id.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectPlacement {
    /// Rotation for a plain click; the rotate key turns it in 90° steps.
    pub rotation: Angle,
    /// When set, a second click sets the rotation.
    pub pick_rotation: bool,
    /// Place mirrored (the flip key toggles it).
    pub mirrored: bool,
}

/// Level mark parameters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LevelSettings {
    /// Label shown before the value, e.g. "Δάπεδο".
    pub label: String,
}

/// Grid line parameters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GridSettings {
    /// Label for the next grid line; `None` continues the sequence (A, B, C… or 1, 2, 3…).
    pub label: Option<String>,
}

/// Section and elevation tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionSettings {
    /// How far beyond the line the view sees.
    pub depth: Length,
    /// Scale denominator of the new view.
    pub scale: u32,
}

/// Dimension tool parameters.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DimensionSettings {
    /// Orientation of the dimension line (the flip key cycles it while placing).
    pub orientation: DimensionOrientation,
}

/// All tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSettings {
    /// Walls.
    pub wall: WallSettings,
    /// Slabs.
    pub slab: SlabSettings,
    /// Columns.
    pub column: ColumnSettings,
    /// Beams.
    pub beam: BeamSettings,
    /// Openings.
    pub opening: OpeningSettings,
    /// Stairs.
    pub stair: StairSettings,
    /// Roofs.
    pub roof: RoofSettings,
    /// Railings.
    pub railing: RailingSettings,
    /// Spaces.
    pub space: SpaceSettings,
    /// Object catalogue id for the object tool.
    pub object: String,
    /// Text.
    pub text: TextSettings,
    /// Drafting lines and fills.
    pub draft: DraftStyle,
    /// Object rotation and mirroring.
    pub object_placement: ObjectPlacement,
    /// Level marks.
    pub level: LevelSettings,
    /// Grid lines.
    pub grid: GridSettings,
    /// Sections and elevations.
    pub section: SectionSettings,
    /// Dimensions.
    pub dimension: DimensionSettings,
    /// How the fill tool gets its outline.
    pub fill_mode: OutlineMode,
}

impl Default for ToolSettings {
    fn default() -> Self {
        ToolSettings {
            wall: WallSettings {
                structure: None,
                height: None,
                reference: ReferenceLine::Centre,
                base_offset: Length::ZERO,
                mode: WallMode::Chain,
            },
            slab: SlabSettings { structure: None, top_offset: Length::ZERO, mode: OutlineMode::Polygon },
            column: ColumnSettings {
                profile: Profile::Rectangle { width: Length::cm(40.0), depth: Length::cm(40.0) },
                material: None,
                height: None,
                rotation: Angle::ZERO,
                pick_rotation: false,
            },
            beam: BeamSettings {
                profile: Profile::Rectangle { width: Length::cm(25.0), depth: Length::cm(50.0) },
                material: None,
                top_offset: None,
            },
            opening: OpeningSettings {
                door_style: "door.single_swing".into(),
                window_style: "window.casement_2".into(),
                door_size: (Length::m(0.9), Length::m(2.1)),
                window_size: (Length::m(1.2), Length::m(1.4)),
                window_sill: Length::m(0.9),
                void_size: (Length::m(1.0), Length::m(2.2)),
                step: Length::cm(5.0),
            },
            stair: StairSettings {
                shape: StairShape::Straight,
                width: Length::m(1.0),
                going: Length::cm(28.0),
                max_riser: Length::cm(18.0),
                thickness: Length::cm(15.0),
                gap: Length::cm(10.0),
                usage: StairUse::DwellingInternal,
            },
            roof: RoofSettings {
                kind: RoofKind::Hip,
                pitch: Angle::deg(22.0),
                overhang: Length::cm(50.0),
                structure: None,
                mode: OutlineMode::Polygon,
                base: None,
                parapet: Some(Parapet { height: Length::m(0.6), thickness: Length::cm(15.0) }),
            },
            railing: RailingSettings {
                height: Length::m(1.0),
                infill: RailingInfill::Bars,
                post_spacing: Length::m(1.2),
            },
            space: SpaceSettings {
                name: "Χώρος".into(),
                category: SpaceCategory::MainUse,
                mode: OutlineMode::PickInside,
                auto_number: true,
            },
            object: "furniture.table".into(),
            text: TextSettings { size_mm: 2.5, align: TextAlign::Left, bold: false },
            draft: DraftStyle::default(),
            object_placement: ObjectPlacement {
                rotation: Angle::ZERO,
                pick_rotation: false,
                mirrored: false,
            },
            level: LevelSettings::default(),
            grid: GridSettings::default(),
            section: SectionSettings { depth: Length::m(30.0), scale: 50 },
            dimension: DimensionSettings::default(),
            fill_mode: OutlineMode::Polygon,
        }
    }
}

impl ToolSettings {
    /// The wall structure to use: the setting, else the document's default exterior wall, else 25 cm of the first
    /// material.
    pub fn wall_structure(&self, doc: &Document) -> Option<Structure> {
        self.wall
            .structure
            .clone()
            .or_else(|| doc.defaults().exterior_wall.map(|b| Structure::Layered { build_up: b }))
            .or_else(|| {
                first_material(doc).map(|m| Structure::Basic { material: m, thickness: Length::cm(25.0) })
            })
    }

    /// The slab structure to use on a storey: the setting, else the default ground slab on the lowest storey and
    /// the default floor slab elsewhere, else 20 cm of the structural material.
    pub fn slab_structure(&self, doc: &Document, storey: Id) -> Option<Structure> {
        let lowest = doc.storeys().first().map(|s| s.id) == Some(storey);
        let d = doc.defaults();
        let default = if lowest { d.ground_slab.or(d.floor_slab) } else { d.floor_slab.or(d.ground_slab) };
        self.slab.structure.clone().or_else(|| default.map(|b| Structure::Layered { build_up: b })).or_else(
            || {
                structural_material(doc)
                    .map(|m| Structure::Basic { material: m, thickness: Length::cm(20.0) })
            },
        )
    }

    /// The roof structure to use: the setting, else the default flat or pitched roof build-up, else 20 cm of the
    /// structural material.
    pub fn roof_structure(&self, doc: &Document) -> Option<Structure> {
        let d = doc.defaults();
        let default = match self.roof.kind {
            RoofKind::Flat => d.flat_roof.or(d.pitched_roof),
            _ => d.pitched_roof.or(d.flat_roof),
        };
        self.roof.structure.clone().or_else(|| default.map(|b| Structure::Layered { build_up: b })).or_else(
            || {
                structural_material(doc)
                    .map(|m| Structure::Basic { material: m, thickness: Length::cm(20.0) })
            },
        )
    }

    /// Material of new columns: the setting, else the structural material.
    pub fn column_material(&self, doc: &Document) -> Option<Id> {
        self.column.material.or_else(|| structural_material(doc))
    }

    /// Material of new beams: the setting, else the structural material.
    pub fn beam_material(&self, doc: &Document) -> Option<Id> {
        self.beam.material.or_else(|| structural_material(doc))
    }
}

/// The first material of a document.
pub(crate) fn first_material(doc: &Document) -> Option<Id> {
    doc.materials().next().map(|m| m.id)
}

/// The default structural material, else the first material.
pub(crate) fn structural_material(doc: &Document) -> Option<Id> {
    doc.defaults().structural_material.or_else(|| first_material(doc))
}

/// The default railing material, else the structural material.
pub(crate) fn railing_material(doc: &Document) -> Option<Id> {
    doc.defaults().railing_material.or_else(|| structural_material(doc))
}

/// Height of a storey, or 3 m when it is missing.
pub(crate) fn storey_height(doc: &Document, storey: Id) -> Length {
    doc.storey(storey).map(|s| s.height).unwrap_or(Length::m(3.0))
}
