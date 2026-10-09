//! Localised display names for model enums, tools and catalogue entries.
//!
//! Every name comes from the message catalogue (`crates/i18n/catalogue/ui.tsv`) so that both languages stay
//! complete. Catalogue objects use the key their definition names; until that key exists in the catalogue a
//! fallback key in `ui.tsv` is used, so the interface never shows a raw identifier.

use kanavos_drawing::{HatchPattern, LinePattern};
use kanavos_i18n::{has, t};
use kanavos_model::{
    BuildUpUsage, Category, MaterialCategory, OpeningKind, PaperSize, Phase, RailingInfill, ReferenceLine,
    ScheduleKind, SpaceCategory, StairShape, ViewKind,
};
use kanavos_objects::{ObjectCategory, ObjectDef};
use kanavos_tools::{RoofKind, ToolId};
use kanavos_units::Locale;

/// Translates a key.
pub fn tr(locale: Locale, key: &str) -> String {
    t(locale, key).into_owned()
}

/// Translates a key with `{name}` placeholders.
pub fn trf(locale: Locale, key: &str, args: &[(&str, &str)]) -> String {
    kanavos_i18n::tf(locale, key, args)
}

/// Name of a tool.
pub fn tool(locale: Locale, id: ToolId) -> String {
    tr(locale, &format!("ui.tool.{}", id.key()))
}

/// One-line description of a tool for tooltips.
pub fn tool_hint(locale: Locale, id: ToolId) -> String {
    tr(locale, &format!("ui.tool_hint.{}", id.key()))
}

/// Name of an element category (singular).
pub fn category(locale: Locale, c: Category) -> String {
    tr(locale, &format!("ui.cat.{}", c.key()))
}

/// Name of an element category (plural, for counts and filters).
pub fn category_plural(locale: Locale, c: Category) -> String {
    tr(locale, &format!("ui.cats.{}", c.key()))
}

/// Keys of space categories, in display order.
pub const SPACE_CATEGORIES: [SpaceCategory; 12] = [
    SpaceCategory::MainUse,
    SpaceCategory::Auxiliary,
    SpaceCategory::Stairwell,
    SpaceCategory::LiftShaft,
    SpaceCategory::SemiOpen,
    SpaceCategory::Balcony,
    SpaceCategory::Pilotis,
    SpaceCategory::Parking,
    SpaceCategory::Attic,
    SpaceCategory::Technical,
    SpaceCategory::Shaft,
    SpaceCategory::Other,
];

/// Name of a space category (ΝΟΚ use category).
pub fn space_category(locale: Locale, c: SpaceCategory) -> String {
    let key = match c {
        SpaceCategory::MainUse => "main_use",
        SpaceCategory::Auxiliary => "auxiliary",
        SpaceCategory::Stairwell => "stairwell",
        SpaceCategory::LiftShaft => "lift_shaft",
        SpaceCategory::SemiOpen => "semi_open",
        SpaceCategory::Balcony => "balcony",
        SpaceCategory::Pilotis => "pilotis",
        SpaceCategory::Parking => "parking",
        SpaceCategory::Attic => "attic",
        SpaceCategory::Technical => "technical",
        SpaceCategory::Shaft => "shaft",
        SpaceCategory::Other => "other",
    };
    tr(locale, &format!("ui.space_cat.{key}"))
}

/// Material categories in display order.
pub const MATERIAL_CATEGORIES: [MaterialCategory; 16] = [
    MaterialCategory::Concrete,
    MaterialCategory::Masonry,
    MaterialCategory::Insulation,
    MaterialCategory::Plaster,
    MaterialCategory::Stone,
    MaterialCategory::Tile,
    MaterialCategory::Wood,
    MaterialCategory::Metal,
    MaterialCategory::Glass,
    MaterialCategory::Membrane,
    MaterialCategory::Screed,
    MaterialCategory::Gravel,
    MaterialCategory::Soil,
    MaterialCategory::Air,
    MaterialCategory::Board,
    MaterialCategory::Other,
];

/// Name of a material category.
pub fn material_category(locale: Locale, c: MaterialCategory) -> String {
    let key = match c {
        MaterialCategory::Concrete => "concrete",
        MaterialCategory::Masonry => "masonry",
        MaterialCategory::Insulation => "insulation",
        MaterialCategory::Plaster => "plaster",
        MaterialCategory::Stone => "stone",
        MaterialCategory::Tile => "tile",
        MaterialCategory::Wood => "wood",
        MaterialCategory::Metal => "metal",
        MaterialCategory::Glass => "glass",
        MaterialCategory::Membrane => "membrane",
        MaterialCategory::Screed => "screed",
        MaterialCategory::Gravel => "gravel",
        MaterialCategory::Soil => "soil",
        MaterialCategory::Air => "air",
        MaterialCategory::Board => "board",
        MaterialCategory::Other => "other",
    };
    tr(locale, &format!("ui.mat_cat.{key}"))
}

/// Hatch patterns in display order.
pub const HATCHES: [HatchPattern; 12] = [
    HatchPattern::None,
    HatchPattern::Diagonal,
    HatchPattern::Masonry,
    HatchPattern::CrossDiagonal,
    HatchPattern::Concrete,
    HatchPattern::Insulation,
    HatchPattern::Wood,
    HatchPattern::Grid,
    HatchPattern::Dots,
    HatchPattern::Earth,
    HatchPattern::Gravel,
    HatchPattern::Horizontal,
];

/// Name of a hatch pattern.
pub fn hatch(locale: Locale, h: HatchPattern) -> String {
    let key = match h {
        HatchPattern::None => "none",
        HatchPattern::Diagonal => "diagonal",
        HatchPattern::Masonry => "masonry",
        HatchPattern::CrossDiagonal => "cross_diagonal",
        HatchPattern::Concrete => "concrete",
        HatchPattern::Insulation => "insulation",
        HatchPattern::Wood => "wood",
        HatchPattern::Grid => "grid",
        HatchPattern::Dots => "dots",
        HatchPattern::Earth => "earth",
        HatchPattern::Gravel => "gravel",
        HatchPattern::Horizontal => "horizontal",
    };
    tr(locale, &format!("ui.hatch.{key}"))
}

/// Line patterns offered in pickers.
pub fn line_patterns() -> [LinePattern; 6] {
    [
        LinePattern::Solid,
        LinePattern::Dashed,
        LinePattern::Hidden,
        LinePattern::Dotted,
        LinePattern::DashDot,
        LinePattern::DashDotDot,
    ]
}

/// Name of a line pattern.
pub fn line_pattern(locale: Locale, p: &LinePattern) -> String {
    let key = match p {
        LinePattern::Solid => "solid",
        LinePattern::Dashed => "dashed",
        LinePattern::Hidden => "hidden",
        LinePattern::Dotted => "dotted",
        LinePattern::DashDot => "dash_dot",
        LinePattern::DashDotDot => "dash_dot_dot",
        LinePattern::Custom(_) => "custom",
    };
    tr(locale, &format!("ui.pattern.{key}"))
}

/// Standard line weights in paper millimetres.
pub const LINE_WEIGHTS: [f64; 7] = [0.13, 0.18, 0.25, 0.35, 0.50, 0.70, 1.00];

/// Name of a wall reference line position.
pub fn reference(locale: Locale, r: ReferenceLine) -> String {
    let key = match r {
        ReferenceLine::Left => "left",
        ReferenceLine::Centre => "centre",
        ReferenceLine::Right => "right",
    };
    tr(locale, &format!("ui.ref.{key}"))
}

/// Stair shapes in display order.
pub const STAIR_SHAPES: [StairShape; 5] = [
    StairShape::Straight,
    StairShape::QuarterTurnLeft,
    StairShape::QuarterTurnRight,
    StairShape::HalfTurnLeft,
    StairShape::HalfTurnRight,
];

/// Name of a stair shape.
pub fn stair_shape(locale: Locale, s: StairShape) -> String {
    let key = match s {
        StairShape::Straight => "straight",
        StairShape::QuarterTurnLeft => "quarter_left",
        StairShape::QuarterTurnRight => "quarter_right",
        StairShape::HalfTurnLeft => "half_left",
        StairShape::HalfTurnRight => "half_right",
    };
    tr(locale, &format!("ui.stair.{key}"))
}

/// Railing infills in display order.
pub const INFILLS: [RailingInfill; 3] = [RailingInfill::Bars, RailingInfill::Glass, RailingInfill::Solid];

/// Name of a railing infill.
pub fn infill(locale: Locale, i: RailingInfill) -> String {
    let key = match i {
        RailingInfill::Bars => "bars",
        RailingInfill::Glass => "glass",
        RailingInfill::Solid => "solid",
    };
    tr(locale, &format!("ui.infill.{key}"))
}

/// Phases in display order.
pub const PHASES: [Phase; 3] = [Phase::New, Phase::Existing, Phase::Demolished];

/// Name of a construction phase.
pub fn phase(locale: Locale, p: Phase) -> String {
    let key = match p {
        Phase::New => "new",
        Phase::Existing => "existing",
        Phase::Demolished => "demolished",
    };
    tr(locale, &format!("ui.phase.{key}"))
}

/// Roof kinds of the roof tool, in display order.
pub const ROOF_KINDS: [RoofKind; 4] = [RoofKind::Flat, RoofKind::MonoPitch, RoofKind::Gable, RoofKind::Hip];

/// Name of a roof kind.
pub fn roof_kind(locale: Locale, k: RoofKind) -> String {
    let key = match k {
        RoofKind::Flat => "flat",
        RoofKind::MonoPitch => "mono",
        RoofKind::Gable => "gable",
        RoofKind::Hip => "hip",
    };
    tr(locale, &format!("ui.roof.{key}"))
}

/// Build-up usages in display order.
pub const USAGES: [BuildUpUsage; 4] =
    [BuildUpUsage::Wall, BuildUpUsage::Slab, BuildUpUsage::Roof, BuildUpUsage::Any];

/// Name of a build-up usage.
pub fn usage(locale: Locale, u: BuildUpUsage) -> String {
    let key = match u {
        BuildUpUsage::Wall => "wall",
        BuildUpUsage::Slab => "slab",
        BuildUpUsage::Roof => "roof",
        BuildUpUsage::Any => "any",
    };
    tr(locale, &format!("ui.usage.{key}"))
}

/// Name of an opening kind.
pub fn opening_kind(locale: Locale, k: OpeningKind) -> String {
    let key = match k {
        OpeningKind::Door => "door",
        OpeningKind::Window => "window",
        OpeningKind::Void => "void",
    };
    tr(locale, &format!("ui.opening.{key}"))
}

/// Name of a paper size.
pub fn paper(locale: Locale, p: PaperSize) -> String {
    match p {
        PaperSize::A0 => "A0".into(),
        PaperSize::A1 => "A1".into(),
        PaperSize::A2 => "A2".into(),
        PaperSize::A3 => "A3".into(),
        PaperSize::A4 => "A4".into(),
        PaperSize::Custom { .. } => tr(locale, "ui.paper.custom"),
    }
}

/// Short name of a view kind.
pub fn view_kind(locale: Locale, k: &ViewKind) -> String {
    let key = match k {
        ViewKind::Plan { .. } => "plan",
        ViewKind::Section { .. } => "section",
        ViewKind::Elevation { .. } => "elevation",
        ViewKind::ThreeD { .. } => "three_d",
        ViewKind::SitePlan => "site_plan",
        ViewKind::CoverageDiagram => "coverage",
        ViewKind::AreaTable => "area_table",
        ViewKind::Schedule { .. } => "schedule",
    };
    tr(locale, &format!("ui.view.{key}"))
}

/// Schedules listed in the project tree, in display order.
pub const SCHEDULES: [ScheduleKind; 5] = [
    ScheduleKind::Doors,
    ScheduleKind::Windows,
    ScheduleKind::Spaces,
    ScheduleKind::Walls,
    ScheduleKind::Materials,
];

/// Name of a schedule.
pub fn schedule(locale: Locale, k: ScheduleKind) -> String {
    let key = match k {
        ScheduleKind::Doors => "doors",
        ScheduleKind::Windows => "windows",
        ScheduleKind::Spaces => "spaces",
        ScheduleKind::Walls => "walls",
        ScheduleKind::Slabs => "slabs",
        ScheduleKind::Materials => "materials",
        ScheduleKind::Objects => "objects",
    };
    tr(locale, &format!("ui.schedule.{key}"))
}

/// Object categories in display order.
pub const OBJECT_CATEGORIES: [ObjectCategory; 9] = [
    ObjectCategory::Door,
    ObjectCategory::Window,
    ObjectCategory::Void,
    ObjectCategory::Furniture,
    ObjectCategory::Sanitary,
    ObjectCategory::Kitchen,
    ObjectCategory::Equipment,
    ObjectCategory::Vegetation,
    ObjectCategory::Vehicle,
];

/// Uses `key` when the catalogue has it, else the interface fallback key.
fn with_fallback(locale: Locale, key: &str, fallback: &str) -> String {
    if has(key) { tr(locale, key) } else { tr(locale, fallback) }
}

/// Name of an object category.
pub fn object_category(locale: Locale, c: ObjectCategory) -> String {
    let key = c.name_key();
    let short = key.rsplit('.').next().unwrap_or(key);
    with_fallback(locale, key, &format!("ui.objcat.{short}"))
}

/// Name of a catalogue object.
pub fn object(locale: Locale, def: &ObjectDef) -> String {
    with_fallback(locale, def.name, &format!("ui.obj.{}", def.id))
}

/// Name of a catalogue object by id; the id itself when it is unknown.
pub fn object_by_id(locale: Locale, id: &str) -> String {
    match kanavos_objects::find(id) {
        Some(def) => object(locale, def),
        None => id.to_owned(),
    }
}

/// Label of a catalogue parameter.
pub fn object_param(locale: Locale, label_key: &str, key: &str) -> String {
    with_fallback(locale, label_key, &format!("ui.objparam.{key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_i18n::has;

    #[test]
    fn every_tool_has_a_name_and_hint_in_both_languages() {
        for id in ToolId::ALL {
            assert!(has(&format!("ui.tool.{}", id.key())), "missing ui.tool.{}", id.key());
            assert!(has(&format!("ui.tool_hint.{}", id.key())), "missing ui.tool_hint.{}", id.key());
        }
    }

    #[test]
    fn every_category_has_singular_and_plural_names() {
        for c in Category::ALL {
            assert!(has(&format!("ui.cat.{}", c.key())), "missing ui.cat.{}", c.key());
            assert!(has(&format!("ui.cats.{}", c.key())), "missing ui.cats.{}", c.key());
        }
    }

    #[test]
    fn enum_names_resolve() {
        let el = Locale::El;
        for c in SPACE_CATEGORIES {
            assert!(!space_category(el, c).starts_with("ui."));
        }
        for c in MATERIAL_CATEGORIES {
            assert!(!material_category(el, c).starts_with("ui."));
        }
        for h in HATCHES {
            assert!(!hatch(el, h).starts_with("ui."));
        }
        for p in line_patterns() {
            assert!(!line_pattern(el, &p).starts_with("ui."));
        }
        for s in STAIR_SHAPES {
            assert!(!stair_shape(el, s).starts_with("ui."));
        }
        for k in ROOF_KINDS {
            assert!(!roof_kind(el, k).starts_with("ui."));
        }
        for s in SCHEDULES {
            assert!(!schedule(el, s).starts_with("ui."));
        }
    }

    #[test]
    fn every_catalogue_object_has_a_name() {
        for def in kanavos_objects::catalogue() {
            let el = object(Locale::El, def);
            let en = object(Locale::En, def);
            assert!(!el.starts_with("ui.") && !el.starts_with("objects."), "no Greek name for {}", def.id);
            assert!(!en.starts_with("ui.") && !en.starts_with("objects."), "no English name for {}", def.id);
            for p in &def.params {
                assert!(!object_param(Locale::El, p.label, p.key).starts_with("ui."));
            }
        }
        for c in OBJECT_CATEGORIES {
            assert!(!object_category(Locale::En, c).starts_with("ui."));
        }
    }
}
