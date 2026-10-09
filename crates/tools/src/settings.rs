//! Tool parameters edited in the context bar.

use kanavos_geom::Angle;
use kanavos_model::{
    Document, DraftStyle, Id, Profile, RailingInfill, ReferenceLine, SpaceCategory, StairShape, Structure,
};
use kanavos_units::Length;

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
}

/// Slab tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SlabSettings {
    /// Material or build-up; `None` uses the default floor slab.
    pub structure: Option<Structure>,
    /// Top relative to the storey floor.
    pub top_offset: Length,
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
}

/// Railing tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct RailingSettings {
    /// Height.
    pub height: Length,
    /// Infill.
    pub infill: RailingInfill,
}

/// Space tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct SpaceSettings {
    /// Name for the next space.
    pub name: String,
    /// Category for the next space.
    pub category: SpaceCategory,
}

/// Text tool parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSettings {
    /// Size on paper, mm.
    pub size_mm: f64,
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
}

impl Default for ToolSettings {
    fn default() -> Self {
        ToolSettings {
            wall: WallSettings {
                structure: None,
                height: None,
                reference: ReferenceLine::Centre,
                base_offset: Length::ZERO,
            },
            slab: SlabSettings { structure: None, top_offset: Length::ZERO },
            column: ColumnSettings {
                profile: Profile::Rectangle { width: Length::cm(40.0), depth: Length::cm(40.0) },
                material: None,
                height: None,
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
            },
            stair: StairSettings {
                shape: StairShape::Straight,
                width: Length::m(1.0),
                going: Length::cm(28.0),
                max_riser: Length::cm(18.0),
            },
            roof: RoofSettings {
                kind: RoofKind::Hip,
                pitch: Angle::deg(22.0),
                overhang: Length::cm(50.0),
                structure: None,
            },
            railing: RailingSettings { height: Length::m(1.0), infill: RailingInfill::Bars },
            space: SpaceSettings { name: "Χώρος".into(), category: SpaceCategory::MainUse },
            object: "furniture.table".into(),
            text: TextSettings { size_mm: 2.5 },
            draft: DraftStyle::default(),
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
                doc.materials()
                    .next()
                    .map(|m| Structure::Basic { material: m.id, thickness: Length::cm(25.0) })
            })
    }
}
