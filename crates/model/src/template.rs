//! The starter template: a new project with materials, build-ups, two storeys and the usual views.
//!
//! All content here is authored for Kanavos from general construction practice in Greece (typical masonry,
//! concrete and insulation build-ups and conductivities in the range given by ΤΟΤΕΕ 20701-2). It is not taken
//! from any other product's templates.

use kanavos_drawing::{HatchPattern, Rgba};
use kanavos_geom::Length;

use crate::{
    BuildUp, BuildUpLayer, BuildUpUsage, Camera, Defaults, Document, Id, Material, MaterialCategory,
    ProjectId, ProjectInfo, ScheduleKind, Storey, View, ViewKind,
};

struct MaterialSpec {
    name: &'static str,
    category: MaterialCategory,
    color: Rgba,
    pattern: HatchPattern,
    background: Option<Rgba>,
    priority: u8,
    conductivity: f64,
    density: f64,
}

const MATERIALS: &[MaterialSpec] = &[
    MaterialSpec {
        name: "Οπλισμένο σκυρόδεμα",
        category: MaterialCategory::Concrete,
        color: Rgba::rgb(186, 186, 180),
        pattern: HatchPattern::CrossDiagonal,
        background: Some(Rgba::rgb(200, 200, 200)),
        priority: 90,
        conductivity: 2.5,
        density: 2400.0,
    },
    MaterialSpec {
        name: "Οπτοπλινθοδομή",
        category: MaterialCategory::Masonry,
        color: Rgba::rgb(196, 122, 92),
        pattern: HatchPattern::Masonry,
        background: None,
        priority: 60,
        conductivity: 0.45,
        density: 1300.0,
    },
    MaterialSpec {
        name: "Θερμομόνωση EPS",
        category: MaterialCategory::Insulation,
        color: Rgba::rgb(240, 238, 225),
        pattern: HatchPattern::Insulation,
        background: None,
        priority: 20,
        conductivity: 0.035,
        density: 20.0,
    },
    MaterialSpec {
        name: "Θερμομόνωση XPS",
        category: MaterialCategory::Insulation,
        color: Rgba::rgb(190, 215, 235),
        pattern: HatchPattern::Insulation,
        background: None,
        priority: 20,
        conductivity: 0.034,
        density: 32.0,
    },
    MaterialSpec {
        name: "Επίχρισμα",
        category: MaterialCategory::Plaster,
        color: Rgba::rgb(236, 232, 222),
        pattern: HatchPattern::Dots,
        background: None,
        priority: 10,
        conductivity: 0.87,
        density: 1800.0,
    },
    MaterialSpec {
        name: "Γυψοσανίδα",
        category: MaterialCategory::Board,
        color: Rgba::rgb(242, 242, 242),
        pattern: HatchPattern::Dots,
        background: None,
        priority: 15,
        conductivity: 0.25,
        density: 900.0,
    },
    MaterialSpec {
        name: "Τσιμεντοκονία",
        category: MaterialCategory::Screed,
        color: Rgba::rgb(170, 170, 165),
        pattern: HatchPattern::Dots,
        background: None,
        priority: 30,
        conductivity: 1.4,
        density: 2000.0,
    },
    MaterialSpec {
        name: "Ελαφροσκυρόδεμα κλίσεων",
        category: MaterialCategory::Screed,
        color: Rgba::rgb(185, 180, 170),
        pattern: HatchPattern::Concrete,
        background: None,
        priority: 30,
        conductivity: 0.5,
        density: 1000.0,
    },
    MaterialSpec {
        name: "Κεραμικά πλακίδια",
        category: MaterialCategory::Tile,
        color: Rgba::rgb(214, 200, 180),
        pattern: HatchPattern::Grid,
        background: None,
        priority: 25,
        conductivity: 1.3,
        density: 2300.0,
    },
    MaterialSpec {
        name: "Μάρμαρο",
        category: MaterialCategory::Stone,
        color: Rgba::rgb(235, 232, 226),
        pattern: HatchPattern::Diagonal,
        background: None,
        priority: 40,
        conductivity: 3.5,
        density: 2800.0,
    },
    MaterialSpec {
        name: "Ξύλο",
        category: MaterialCategory::Wood,
        color: Rgba::rgb(170, 125, 80),
        pattern: HatchPattern::Wood,
        background: None,
        priority: 35,
        conductivity: 0.13,
        density: 500.0,
    },
    MaterialSpec {
        name: "Αλουμίνιο",
        category: MaterialCategory::Metal,
        color: Rgba::rgb(160, 165, 170),
        pattern: HatchPattern::Diagonal,
        background: Some(Rgba::rgb(120, 120, 120)),
        priority: 80,
        conductivity: 160.0,
        density: 2700.0,
    },
    MaterialSpec {
        name: "Χάλυβας",
        category: MaterialCategory::Metal,
        color: Rgba::rgb(90, 95, 100),
        pattern: HatchPattern::Diagonal,
        background: Some(Rgba::rgb(60, 60, 60)),
        priority: 95,
        conductivity: 50.0,
        density: 7850.0,
    },
    MaterialSpec {
        name: "Γυαλί",
        category: MaterialCategory::Glass,
        color: Rgba::new(170, 205, 225, 120),
        pattern: HatchPattern::Horizontal,
        background: None,
        priority: 5,
        conductivity: 1.0,
        density: 2500.0,
    },
    MaterialSpec {
        name: "Στεγανωτική μεμβράνη",
        category: MaterialCategory::Membrane,
        color: Rgba::rgb(60, 60, 60),
        pattern: HatchPattern::None,
        background: Some(Rgba::rgb(40, 40, 40)),
        priority: 5,
        conductivity: 0.23,
        density: 1100.0,
    },
    MaterialSpec {
        name: "Κεραμίδια",
        category: MaterialCategory::Tile,
        color: Rgba::rgb(178, 84, 54),
        pattern: HatchPattern::Diagonal,
        background: None,
        priority: 25,
        conductivity: 1.0,
        density: 1900.0,
    },
    MaterialSpec {
        name: "Χαλίκι",
        category: MaterialCategory::Gravel,
        color: Rgba::rgb(160, 155, 145),
        pattern: HatchPattern::Gravel,
        background: None,
        priority: 5,
        conductivity: 2.0,
        density: 1800.0,
    },
    MaterialSpec {
        name: "Έδαφος",
        category: MaterialCategory::Soil,
        color: Rgba::rgb(130, 105, 75),
        pattern: HatchPattern::Earth,
        background: None,
        priority: 1,
        conductivity: 1.5,
        density: 1800.0,
    },
];

/// Layer spec: material index in [`MATERIALS`], thickness in cm, core flag.
type LayerSpec = (usize, f64, bool);

struct BuildUpSpec {
    name: &'static str,
    usage: BuildUpUsage,
    layers: &'static [LayerSpec],
}

// Material indices.
const CONCRETE: usize = 0;
const BRICK: usize = 1;
const EPS: usize = 2;
const XPS: usize = 3;
const PLASTER: usize = 4;
const BOARD: usize = 5;
const SCREED: usize = 6;
const LIGHT_SCREED: usize = 7;
const TILES: usize = 8;
const MEMBRANE: usize = 14;
const ROOF_TILES: usize = 15;
const GRAVEL: usize = 16;
const STEEL: usize = 12;

const BUILD_UPS: &[BuildUpSpec] = &[
    BuildUpSpec {
        name: "Εξωτερικός τοίχος 20 με θερμοπρόσοψη 8",
        usage: BuildUpUsage::Wall,
        layers: &[(PLASTER, 1.0, false), (EPS, 8.0, false), (BRICK, 20.0, true), (PLASTER, 2.0, false)],
    },
    BuildUpSpec {
        name: "Διπλή δρομική 2×9 με θερμομόνωση 8",
        usage: BuildUpUsage::Wall,
        layers: &[
            (PLASTER, 2.0, false),
            (BRICK, 9.0, true),
            (XPS, 8.0, false),
            (BRICK, 9.0, true),
            (PLASTER, 2.0, false),
        ],
    },
    BuildUpSpec {
        name: "Εσωτερικός τοίχος δρομικός 9",
        usage: BuildUpUsage::Wall,
        layers: &[(PLASTER, 2.0, false), (BRICK, 9.0, true), (PLASTER, 2.0, false)],
    },
    BuildUpSpec {
        name: "Γυψοσανίδα 10 με μόνωση",
        usage: BuildUpUsage::Wall,
        layers: &[(BOARD, 1.25, false), (EPS, 7.5, true), (BOARD, 1.25, false)],
    },
    BuildUpSpec {
        name: "Τοίχωμα σκυροδέματος 25",
        usage: BuildUpUsage::Wall,
        layers: &[(PLASTER, 2.0, false), (CONCRETE, 25.0, true), (PLASTER, 2.0, false)],
    },
    BuildUpSpec {
        name: "Πλάκα ορόφου 20 με δάπεδο",
        usage: BuildUpUsage::Slab,
        layers: &[(TILES, 1.0, false), (SCREED, 5.0, false), (CONCRETE, 20.0, true), (PLASTER, 1.5, false)],
    },
    BuildUpSpec {
        name: "Πλάκα επί εδάφους με θερμομόνωση",
        usage: BuildUpUsage::Slab,
        layers: &[
            (TILES, 1.0, false),
            (SCREED, 5.0, false),
            (XPS, 5.0, false),
            (MEMBRANE, 0.5, false),
            (CONCRETE, 15.0, true),
            (GRAVEL, 15.0, false),
        ],
    },
    BuildUpSpec {
        name: "Δώμα με ανεστραμμένη μόνωση",
        usage: BuildUpUsage::Roof,
        layers: &[
            (GRAVEL, 5.0, false),
            (XPS, 8.0, false),
            (MEMBRANE, 0.5, false),
            (LIGHT_SCREED, 6.0, false),
            (CONCRETE, 20.0, true),
            (PLASTER, 1.5, false),
        ],
    },
    BuildUpSpec {
        name: "Κεκλιμένη στέγη με κεραμίδια",
        usage: BuildUpUsage::Roof,
        layers: &[
            (ROOF_TILES, 3.0, false),
            (MEMBRANE, 0.5, false),
            (XPS, 8.0, false),
            (CONCRETE, 15.0, true),
            (PLASTER, 1.5, false),
        ],
    },
];

/// A new project: materials, build-ups, a ground floor and a first floor, their plans, a 3D view, a site plan,
/// a coverage diagram, an area table and the main schedules. Every call gives the project a new random id
/// ([`ProjectId::generate`]); callers that need the same document every time use [`starter_with_id`].
pub fn starter() -> Document {
    starter_with_id(ProjectId::generate())
}

/// The starter project (see [`starter`]) with a given project id, for deterministic callers such as tests, the
/// sample project and replays of a command log.
pub fn starter_with_id(id: ProjectId) -> Document {
    let mut doc = Document::new().with_project_id(id).with_info(ProjectInfo {
        name: "Νέο έργο".into(),
        revision: "0".into(),
        ..ProjectInfo::default()
    });

    let mut material_ids = Vec::with_capacity(MATERIALS.len());
    for m in MATERIALS {
        let (d, id) = doc.with_new_id();
        doc = d.with_material(Material {
            id,
            name: m.name.into(),
            category: m.category,
            color: m.color,
            cut_pattern: m.pattern,
            cut_background: m.background,
            priority: m.priority,
            conductivity: Some(m.conductivity),
            density: Some(m.density),
        });
        material_ids.push(id);
    }
    let mat = |i: usize| material_ids.get(i).copied().unwrap_or(Id(0));

    let mut build_up_ids = Vec::with_capacity(BUILD_UPS.len());
    for b in BUILD_UPS {
        let (d, id) = doc.with_new_id();
        doc = d.with_build_up(BuildUp {
            id,
            name: b.name.into(),
            usage: b.usage,
            layers: b
                .layers
                .iter()
                .map(|&(m, cm, core)| BuildUpLayer { material: mat(m), thickness: Length::cm(cm), core })
                .collect(),
        });
        build_up_ids.push(id);
    }
    let bu = |i: usize| build_up_ids.get(i).copied();

    doc = doc.with_defaults(Defaults {
        exterior_wall: bu(0),
        interior_wall: bu(2),
        floor_slab: bu(5),
        ground_slab: bu(6),
        flat_roof: bu(7),
        pitched_roof: bu(8),
        structural_material: Some(mat(CONCRETE)),
        railing_material: Some(mat(STEEL)),
    });

    let storeys = [("Ισόγειο", 0.0, "Κάτοψη ισογείου"), ("1ος όροφος", 3.2, "Κάτοψη 1ου ορόφου")];
    for (name, elevation, plan_name) in storeys {
        let (d, sid) = doc.with_new_id();
        doc = d.with_storey(Storey::new(sid, name, Length::m(elevation), Length::m(3.2)));
        let (d, vid) = doc.with_new_id();
        doc = d.with_view(View {
            id: vid,
            name: plan_name.into(),
            kind: ViewKind::Plan { storey: sid },
            scale: 50,
            hidden_layers: Default::default(),
        });
    }

    let extra_views = [
        ("3D", ViewKind::ThreeD { camera: Camera::default() }, 100),
        ("Τοπογραφικό διάγραμμα", ViewKind::SitePlan, 200),
        ("Διάγραμμα κάλυψης", ViewKind::CoverageDiagram, 200),
        ("Πίνακας εμβαδών", ViewKind::AreaTable, 100),
        ("Πίνακας θυρών", ViewKind::Schedule { schedule: ScheduleKind::Doors }, 100),
        ("Πίνακας παραθύρων", ViewKind::Schedule { schedule: ScheduleKind::Windows }, 100),
        ("Πίνακας χώρων", ViewKind::Schedule { schedule: ScheduleKind::Spaces }, 100),
    ];
    for (name, kind, scale) in extra_views {
        let (d, vid) = doc.with_new_id();
        doc =
            d.with_view(View { id: vid, name: name.into(), kind, scale, hidden_layers: Default::default() });
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_build_ups_have_expected_thicknesses() {
        let doc = starter();
        let ext = doc.defaults().exterior_wall.and_then(|id| doc.build_up(id)).unwrap();
        assert!(doc.tolerance().equal(ext.thickness(), Length::cm(31.0)));
        // A reasonable U-value for an insulated wall: below the 0.40 W/m²K of the strictest Greek climate zone.
        let u = ext.u_value(|m| doc.material(m).and_then(|m| m.conductivity)).unwrap();
        assert!(u > 0.2 && u < 0.45, "U = {u}");
    }

    #[test]
    fn every_starter_project_gets_its_own_id() {
        let (a, b) = (starter(), starter());
        assert!(!a.project_id().is_nil());
        assert_ne!(a.project_id(), b.project_id());
        // Apart from the id, the template is the same.
        assert_eq!(a.with_project_id(ProjectId::NIL), b.with_project_id(ProjectId::NIL));
        let fixed = ProjectId::from_u128(42);
        assert_eq!(starter_with_id(fixed), starter_with_id(fixed));
        assert_eq!(starter_with_id(fixed).project_id(), fixed);
    }

    #[test]
    fn starter_has_two_storeys_with_plans() {
        let doc = starter();
        let storeys = doc.storeys();
        assert_eq!(storeys.len(), 2);
        for s in storeys {
            assert!(doc.plan_view_of(s.id).is_some());
        }
    }
}
