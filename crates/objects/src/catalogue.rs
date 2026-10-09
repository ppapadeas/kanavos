//! The built-in catalogue (ADR-0006 phase 1): every definition, its default size and its parameters.
//!
//! Sizes are common Greek and European practice: interior doors 0.80–0.90 × 2.10 m, windows on 0.90 m sills,
//! sanitary ware and kitchen modules on the 0.60 m grid, beds and sofas at standard mattress and seat sizes.

use crate::{ObjectCategory, ObjectDef, Origin, ParamDef, ParamKind};

macro_rules! param {
    ($key:literal, $kind:ident, $default:expr, $min:expr, $max:expr) => {
        ParamDef {
            key: $key,
            label: concat!("objects.param.", $key),
            kind: ParamKind::$kind,
            default: $default,
            min: $min,
            max: $max,
        }
    };
}

macro_rules! opening {
    ($id:literal, $cat:ident, ($w:expr, $h:expr), $sill:expr, $ifc:literal, $ty:literal, $params:expr) => {
        ObjectDef {
            id: $id,
            name: concat!("objects.", $id),
            category: ObjectCategory::$cat,
            width: $w,
            depth: 0.0,
            height: $h,
            sill: $sill,
            params: $params,
            origin: Origin::FrontCentre,
            ifc_class: $ifc,
            ifc_type: $ty,
            min_size: [0.3, 0.0, 0.3],
            max_size: [8.0, 0.0, 6.0],
        }
    };
}

macro_rules! object {
    ($id:literal, $cat:ident, ($w:expr, $d:expr, $h:expr), $origin:ident, $ifc:literal, $ty:literal, $params:expr) => {
        ObjectDef {
            id: $id,
            name: concat!("objects.", $id),
            category: ObjectCategory::$cat,
            width: $w,
            depth: $d,
            height: $h,
            sill: 0.0,
            params: $params,
            origin: Origin::$origin,
            ifc_class: $ifc,
            ifc_type: $ty,
            min_size: [0.1, 0.01, 0.01],
            max_size: [30.0, 30.0, 30.0],
        }
    };
}

fn frame(default: f64) -> ParamDef {
    param!("frame", Length, default, 0.02, 0.2)
}
fn frame_depth(default: f64) -> ParamDef {
    param!("frame_depth", Length, default, 0.03, 0.6)
}
fn setback(default: f64) -> ParamDef {
    param!("setback", Length, default, 0.0, 0.6)
}
fn leaf(default: f64) -> ParamDef {
    param!("leaf", Length, default, 0.02, 0.1)
}
fn swing() -> ParamDef {
    param!("swing", Angle, 90.0, 15.0, 180.0)
}
fn threshold(on: bool) -> ParamDef {
    param!("threshold", Toggle, if on { 1.0 } else { 0.0 }, 0.0, 1.0)
}
fn glazing(default: f64) -> ParamDef {
    param!("glazing", Count, default, 0.0, 90.0)
}
fn handle() -> ParamDef {
    param!("handle", Toggle, 1.0, 0.0, 1.0)
}
fn shutters() -> ParamDef {
    param!("shutters", Toggle, 0.0, 0.0, 1.0)
}
fn roller() -> ParamDef {
    param!("roller", Toggle, 0.0, 0.0, 1.0)
}
fn roller_box(default: f64) -> ParamDef {
    param!("roller_box", Length, default, 0.12, 0.4)
}

/// Parameters of a hinged door.
fn hinged_door() -> Vec<ParamDef> {
    vec![frame(0.05), frame_depth(0.1), leaf(0.04), swing(), threshold(false), glazing(0.0), handle()]
}

/// Parameters every window shares.
fn window(extra: Vec<ParamDef>) -> Vec<ParamDef> {
    let mut v = vec![
        frame(0.06),
        frame_depth(0.07),
        setback(0.06),
        param!("sill_out", Toggle, 1.0, 0.0, 1.0),
        param!("sill_projection", Length, 0.03, 0.0, 0.1),
        param!("sill_in", Toggle, 1.0, 0.0, 1.0),
        shutters(),
        roller(),
        roller_box(0.22),
        handle(),
    ];
    v.extend(extra);
    v
}

fn transom() -> ParamDef {
    param!("transom", Length, 0.0, 0.0, 1.0)
}

/// Builds the catalogue.
pub(crate) fn build() -> Vec<ObjectDef> {
    vec![
        // Doors.
        opening!("door.single_swing", Door, (0.9, 2.1), 0.0, "IfcDoor", "DOOR", hinged_door()),
        opening!("door.double_swing", Door, (1.4, 2.1), 0.0, "IfcDoor", "DOOR", hinged_door()),
        opening!("door.unequal_double", Door, (1.3, 2.1), 0.0, "IfcDoor", "DOOR", {
            let mut v = hinged_door();
            v.push(param!("main_leaf", Length, 0.85, 0.3, 2.0));
            v
        }),
        opening!(
            "door.sliding_single",
            Door,
            (0.9, 2.1),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![leaf(0.04), glazing(0.0), handle()]
        ),
        opening!(
            "door.sliding",
            Door,
            (1.6, 2.2),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![frame(0.04), frame_depth(0.12), leaf(0.04), glazing(0.0), handle()]
        ),
        opening!(
            "door.pocket",
            Door,
            (0.9, 2.1),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![
                frame(0.03),
                frame_depth(0.1),
                leaf(0.04),
                param!("leaves", Count, 1.0, 1.0, 2.0),
                glazing(0.0),
                handle()
            ]
        ),
        opening!(
            "door.folding",
            Door,
            (1.2, 2.1),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![
                frame(0.04),
                frame_depth(0.1),
                leaf(0.03),
                param!("panels", Count, 4.0, 2.0, 8.0),
                glazing(0.0),
                handle()
            ]
        ),
        opening!(
            "door.entrance",
            Door,
            (1.35, 2.2),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![
                frame(0.07),
                frame_depth(0.12),
                leaf(0.065),
                swing(),
                param!("sidelight", Length, 0.35, 0.0, 1.2),
                threshold(true),
                glazing(0.0),
                handle()
            ]
        ),
        opening!(
            "door.balcony",
            Door,
            (1.4, 2.3),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![
                frame(0.06),
                frame_depth(0.07),
                setback(0.06),
                leaf(0.065),
                param!("leaves", Count, 2.0, 1.0, 2.0),
                swing(),
                glazing(75.0),
                threshold(true),
                shutters(),
                roller(),
                roller_box(0.22),
                handle()
            ]
        ),
        opening!(
            "door.balcony_sliding",
            Door,
            (2.0, 2.3),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![
                frame(0.06),
                frame_depth(0.11),
                setback(0.06),
                param!("panes", Count, 2.0, 2.0, 4.0),
                threshold(true),
                shutters(),
                roller(),
                roller_box(0.25),
                handle()
            ]
        ),
        opening!(
            "door.garage",
            Door,
            (2.5, 2.2),
            0.0,
            "IfcDoor",
            "DOOR",
            vec![leaf(0.045), param!("sections", Count, 5.0, 3.0, 8.0), handle()]
        ),
        // Windows.
        opening!(
            "window.casement_1",
            Window,
            (0.8, 1.2),
            0.9,
            "IfcWindow",
            "WINDOW",
            window(vec![transom()])
        ),
        opening!(
            "window.casement_2",
            Window,
            (1.2, 1.4),
            0.9,
            "IfcWindow",
            "WINDOW",
            window(vec![transom()])
        ),
        opening!(
            "window.casement_3",
            Window,
            (1.8, 1.4),
            0.9,
            "IfcWindow",
            "WINDOW",
            window(vec![transom()])
        ),
        opening!(
            "window.tilt_turn",
            Window,
            (0.9, 1.4),
            0.9,
            "IfcWindow",
            "WINDOW",
            window(vec![param!("leaves", Count, 1.0, 1.0, 2.0), transom()])
        ),
        opening!("window.top_hung", Window, (0.8, 0.6), 1.5, "IfcWindow", "WINDOW", window(vec![])),
        opening!("window.sliding", Window, (1.8, 1.4), 0.9, "IfcWindow", "WINDOW", {
            let mut v = window(vec![param!("panes", Count, 2.0, 2.0, 4.0)]);
            for p in &mut v {
                if p.key == "frame_depth" {
                    p.default = 0.1;
                }
            }
            v
        }),
        opening!(
            "window.fixed",
            Window,
            (1.0, 1.0),
            1.0,
            "IfcWindow",
            "WINDOW",
            window(vec![param!("mullions", Count, 0.0, 0.0, 6.0), transom()])
        ),
        // Voids.
        opening!(
            "void.rectangular",
            Void,
            (1.0, 2.2),
            0.0,
            "IfcOpeningElement",
            "OPENING",
            vec![param!("lintel", Toggle, 1.0, 0.0, 1.0)]
        ),
        opening!(
            "void.lined",
            Void,
            (1.0, 2.2),
            0.0,
            "IfcOpeningElement",
            "OPENING",
            vec![param!("lining", Length, 0.025, 0.01, 0.08), param!("lintel", Toggle, 1.0, 0.0, 1.0)]
        ),
        // Free objects (drawn as boxes until their definitions land).
        object!(
            "furniture.bed_double",
            Furniture,
            (1.6, 2.0, 0.45),
            FrontCentre,
            "IfcFurniture",
            "BED",
            vec![]
        ),
        object!(
            "furniture.bed_single",
            Furniture,
            (0.9, 2.0, 0.45),
            FrontCentre,
            "IfcFurniture",
            "BED",
            vec![]
        ),
        object!("furniture.table", Furniture, (1.6, 0.9, 0.75), FrontCentre, "IfcFurniture", "TABLE", vec![]),
        object!("furniture.chair", Furniture, (0.45, 0.5, 0.9), FrontCentre, "IfcFurniture", "CHAIR", vec![]),
        object!("furniture.sofa", Furniture, (2.2, 0.9, 0.8), FrontCentre, "IfcFurniture", "SOFA", vec![]),
        object!(
            "furniture.wardrobe",
            Furniture,
            (1.2, 0.6, 2.4),
            FrontCentre,
            "IfcFurniture",
            "NOTDEFINED",
            vec![]
        ),
        object!(
            "sanitary.wc",
            Sanitary,
            (0.38, 0.65, 0.4),
            FrontCentre,
            "IfcSanitaryTerminal",
            "TOILETPAN",
            vec![]
        ),
        object!(
            "sanitary.basin",
            Sanitary,
            (0.6, 0.45, 0.85),
            FrontCentre,
            "IfcSanitaryTerminal",
            "WASHHANDBASIN",
            vec![]
        ),
        object!(
            "sanitary.shower",
            Sanitary,
            (0.9, 0.9, 0.05),
            FrontCentre,
            "IfcSanitaryTerminal",
            "SHOWER",
            vec![]
        ),
        object!(
            "sanitary.bath",
            Sanitary,
            (1.7, 0.75, 0.55),
            FrontCentre,
            "IfcSanitaryTerminal",
            "BATH",
            vec![]
        ),
        object!(
            "kitchen.counter",
            Kitchen,
            (2.4, 0.6, 0.9),
            FrontCentre,
            "IfcFurniture",
            "NOTDEFINED",
            vec![]
        ),
        object!("kitchen.sink", Kitchen, (0.8, 0.6, 0.9), FrontCentre, "IfcSanitaryTerminal", "SINK", vec![]),
        object!(
            "kitchen.hob",
            Kitchen,
            (0.6, 0.6, 0.9),
            FrontCentre,
            "IfcElectricAppliance",
            "ELECTRICCOOKER",
            vec![]
        ),
        object!(
            "kitchen.fridge",
            Kitchen,
            (0.6, 0.65, 1.85),
            FrontCentre,
            "IfcElectricAppliance",
            "FRIDGE_FREEZER",
            vec![]
        ),
        object!(
            "vegetation.tree",
            Vegetation,
            (4.0, 4.0, 6.0),
            FrontCentre,
            "IfcGeographicElement",
            "VEGETATION",
            vec![]
        ),
        object!(
            "vehicle.car",
            Vehicle,
            (1.8, 4.5, 1.5),
            FrontCentre,
            "IfcBuildingElementProxy",
            "NOTDEFINED",
            vec![]
        ),
    ]
}
