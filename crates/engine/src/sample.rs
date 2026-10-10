//! A complete sample project: a two-storey house on a 500 m² plot, built only through commands on the starter
//! template, so it doubles as an end-to-end test of the command set and as demo data for the interface.
//!
//! The house was designed for Kanavos from general Greek residential practice; names of people and places are
//! fictional.
//!
//! - Plot 20 × 25 m (500 m²), street to the south; terms Σ.Δ. 0,8, κάλυψη 60 %, max height 7,50 m, 2 floors.
//! - Footprint 12 × 10 m. Exterior walls 31 cm (render, 8 cm EPS, 20 cm brick, plaster) with their reference line
//!   on the outer face, drawn clockwise so the insulation is outside; 13 cm brick partitions on centre lines.
//! - Ground floor (±0,00): living room, kitchen, dining room, stairwell, shower room, hall, storage and a covered
//!   semi-open entrance porch (ημιυπαίθριος) in the south-east corner under the upper floor.
//! - First floor (+3,20): three bedrooms, bathroom, WC, study, corridor, stairwell, storage and a south balcony
//!   with a railing.
//! - RC columns 30 × 30 cm at corners and junctions, ground slab, first-floor slab with the stair well, a
//!   half-turn stair (18 risers of 17,8 cm, 28 cm going), a hip roof (22°, 60 cm overhang).
//! - Doors Π1–Π4 and windows Θ1–Θ3 tagged by type, furniture and fixtures, exterior dimension chains, level marks,
//!   two sections, four elevations and the permit sheet set.

use std::collections::BTreeMap;

use kanavos_geom::{Angle, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    BuildingTerms, Column, Dimension, Document, ElementKind, Georef, Id, LevelMark, Object, OpeningKind,
    Profile, ProjectInfo, Railing, RailingInfill, ReferenceLine, Site, Slab, Space, SpaceCategory, Stair,
    StairShape, Structure, TextAlign, TextNote, ViewKind, Wall,
};
use kanavos_units::{Length, Locale};

use crate::commands::{
    AutoDimensions, CreateElement, CreateView, DetectSpace, PermitSet, PlaceOpening, RoofFromWalls, RoofKind,
    SetBuildingTerms, SetPlot, SetProjectInfo, SetSite, SlabFromWalls, UpdateView,
};
use crate::geo::OutlineFace;
use crate::{AnyCommand, CommandError, Outcome, Session};

/// Builds the sample house as a document.
pub fn sample_house() -> Result<Document, CommandError> {
    Ok(sample_session()?.document().clone())
}

/// Builds the sample house as a session whose history holds every command used (one undo step each); its
/// [`Session::log_jsonl`] is a replayable script of the whole project.
pub fn sample_session() -> Result<Session, CommandError> {
    let mut b = Builder::new();
    b.project()?;
    b.ground_floor()?;
    b.first_floor()?;
    b.views_and_sheets()?;
    Ok(b.s)
}

fn missing(what: &str) -> CommandError {
    CommandError::Geometry(format!("the starter template has no {what}"))
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon2 {
    Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

const EXT_T: f64 = 0.31; // exterior wall thickness
const H_GROUND: f64 = 3.2; // ground exterior walls run to the top of the first-floor slab
const H_PART_GROUND: f64 = 2.925; // ground partitions to the underside of the 27,5 cm floor slab
const H_FIRST: f64 = 3.0; // first-floor walls carry the roof

/// Project id of the sample house. Fixed, so that building the sample twice (or replaying its command log on
/// [`sample_start`]) gives the same document.
pub const SAMPLE_PROJECT_ID: kanavos_model::ProjectId =
    kanavos_model::ProjectId::from_u128(0x4b61_6e61_766f_4573_8a6d_706c_6548_6f75);

/// The document the sample house starts from: the starter template with [`SAMPLE_PROJECT_ID`].
pub fn sample_start() -> kanavos_model::Document {
    kanavos_model::starter_with_id(SAMPLE_PROJECT_ID)
}

/// Ids the steps need.
struct Builder {
    s: Session,
    ground: Id,
    first: Id,
    exterior: Id,
    interior: Id,
    concrete: Id,
    steel: Id,
    marble: Id,
}

/// Opening spec: kind, style, tag, centre point on the host's reference line, width, height, sill, and whether
/// it opens to the other side.
type OpeningSpec = (OpeningKind, &'static str, &'static str, (f64, f64), f64, f64, f64, bool);

impl Builder {
    fn new() -> Builder {
        let doc = sample_start();
        let storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
        let ground = storeys.first().copied().unwrap_or_default();
        let first = storeys.get(1).copied().unwrap_or_default();
        let d = doc.defaults().clone();
        let marble = doc.materials().find(|m| m.name == "Μάρμαρο").map(|m| m.id).unwrap_or_default();
        Builder {
            ground,
            first,
            exterior: d.exterior_wall.unwrap_or_default(),
            interior: d.interior_wall.unwrap_or_default(),
            concrete: d.structural_material.unwrap_or_default(),
            steel: d.railing_material.unwrap_or_default(),
            marble,
            s: Session::new(doc),
        }
    }

    fn run(&mut self, c: impl Into<AnyCommand>) -> Result<Outcome, CommandError> {
        self.s.run(c)
    }

    fn create(&mut self, storey: Id, kind: ElementKind) -> Result<Id, CommandError> {
        self.create_tagged(storey, kind, "")
    }

    fn create_tagged(&mut self, storey: Id, kind: ElementKind, tag: &str) -> Result<Id, CommandError> {
        let mut c = CreateElement::on(storey, kind);
        c.tag = tag.to_owned();
        let out = self.run(c)?;
        out.created.first().copied().ok_or_else(|| missing("created element"))
    }

    fn wall(
        &mut self,
        storey: Id,
        a: (f64, f64),
        b: (f64, f64),
        exterior: bool,
        height: f64,
    ) -> Result<Id, CommandError> {
        let (build_up, reference) = if exterior {
            (self.exterior, ReferenceLine::Left)
        } else {
            (self.interior, ReferenceLine::Centre)
        };
        self.create(
            storey,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference,
                structure: Structure::Layered { build_up },
                height: Length::m(height),
                base_offset: Length::ZERO,
            }),
        )
    }

    /// Exterior walls along a clockwise ring of outer-face corners.
    fn ring(&mut self, storey: Id, corners: &[(f64, f64)], height: f64) -> Result<Vec<Id>, CommandError> {
        let mut ids = Vec::with_capacity(corners.len());
        for (i, a) in corners.iter().enumerate() {
            let b = corners.get((i + 1) % corners.len()).copied().unwrap_or(*a);
            ids.push(self.wall(storey, *a, b, true, height)?);
        }
        Ok(ids)
    }

    fn columns(&mut self, storey: Id, at: &[(f64, f64)], height: f64) -> Result<(), CommandError> {
        for (x, y) in at {
            self.create(
                storey,
                ElementKind::Column(Column {
                    position: Point2::new(*x, *y),
                    rotation: Angle::ZERO,
                    profile: Profile::Rectangle { width: Length::cm(30.0), depth: Length::cm(30.0) },
                    material: self.concrete,
                    height: Length::m(height),
                    base_offset: Length::ZERO,
                }),
            )?;
        }
        Ok(())
    }

    fn openings(&mut self, host: Id, specs: &[OpeningSpec]) -> Result<(), CommandError> {
        // Exterior walls run clockwise, so their left face is outside: doors open inwards and windows show their
        // outer face outside when flipped. `flip` reverses that choice.
        let exterior = self
            .s
            .document()
            .element(host)
            .and_then(|e| e.as_wall())
            .is_some_and(|w| w.reference == ReferenceLine::Left);
        for (kind, style, tag, (x, y), w, h, sill, flip) in specs {
            self.run(PlaceOpening {
                point: Point2::new(*x, *y),
                kind: *kind,
                host: Some(host),
                storey: None,
                style: Some((*style).to_owned()),
                width: Some(Length::m(*w)),
                height: Some(Length::m(*h)),
                sill: Some(Length::m(*sill)),
                max_distance: Length::m(0.5),
                flip_side: exterior != *flip,
                flip_hand: false,
                params: BTreeMap::new(),
                tag: (*tag).to_owned(),
                properties: BTreeMap::new(),
            })?;
        }
        Ok(())
    }

    fn space(
        &mut self,
        storey: Id,
        boundary: Polygon2,
        name: &str,
        number: &str,
        category: SpaceCategory,
        finish: &str,
    ) -> Result<Id, CommandError> {
        let stamp = crate::geo::interior_point(&boundary);
        self.create(
            storey,
            ElementKind::Space(Space {
                boundary: boundary.to_ccw().into(),
                name: name.to_owned(),
                number: number.to_owned(),
                category,
                height: None,
                stamp,
                finish: finish.to_owned(),
            }),
        )
    }

    fn object(
        &mut self,
        storey: Id,
        definition: &str,
        at: (f64, f64),
        degrees: f64,
    ) -> Result<Id, CommandError> {
        self.object_sized(storey, definition, at, degrees, &[])
    }

    fn object_sized(
        &mut self,
        storey: Id,
        definition: &str,
        at: (f64, f64),
        degrees: f64,
        params: &[(&str, f64)],
    ) -> Result<Id, CommandError> {
        self.create(
            storey,
            ElementKind::Object(Object {
                definition: definition.to_owned(),
                position: Point2::new(at.0, at.1),
                rotation: Angle::deg(degrees),
                mirrored: false,
                elevation: Length::ZERO,
                params: params.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(),
            }),
        )
    }

    fn level(&mut self, storey: Id, at: (f64, f64), value: Option<f64>) -> Result<Id, CommandError> {
        self.create(
            storey,
            ElementKind::Level(LevelMark {
                position: Point2::new(at.0, at.1),
                value: value.map(Length::m),
                label: String::new(),
            }),
        )
    }

    fn dimension(&mut self, storey: Id, points: &[(f64, f64)], line: (f64, f64)) -> Result<Id, CommandError> {
        self.create(
            storey,
            ElementKind::Dimension(Dimension {
                points: points.iter().map(|(x, y)| Point2::new(*x, *y)).collect(),
                line_point: Point2::new(line.0, line.1),
                direction: Angle::ZERO,
            }),
        )
    }

    fn railing(&mut self, storey: Id, path: &[(f64, f64)], base: f64) -> Result<Id, CommandError> {
        self.create(
            storey,
            ElementKind::Railing(Railing {
                path: path.iter().map(|(x, y)| Point2::new(*x, *y)).collect(),
                height: Length::m(1.0),
                base_offset: Length::m(base),
                post_spacing: Length::m(1.2),
                infill: RailingInfill::Bars,
                material: self.steel,
            }),
        )
    }

    // ----- Project and site ----------------------------------------------------------------------------------

    fn project(&mut self) -> Result<(), CommandError> {
        self.run(SetProjectInfo {
            info: ProjectInfo {
                name: "Νέα διώροφη κατοικία".into(),
                location: "Οδός Λεμονιάς 7, θέση Αμπελάκια".into(),
                municipality: "Δήμος Δειγματούπολης".into(),
                owner: "Μαρίνα Δοκιμάκη".into(),
                designer: "Στέλιος Παραδειγματάκης, αρχιτέκτων μηχανικός".into(),
                designer_details: "Α.Μ. ΤΕΕ 000000 (στοιχεία δείγματος)".into(),
                date: "Οκτώβριος 2026".into(),
                revision: "0".into(),
                notes: "Παράδειγμα έργου του Kanavos με φανταστικά στοιχεία.".into(),
            },
        })?;
        let site = self.s.document().site().clone();
        self.run(SetSite {
            site: Site {
                street_lines: vec![vec![Point2::new(-4.0, -6.0), Point2::new(16.0, -6.0)]],
                building_lines: vec![vec![Point2::new(-4.0, -2.0), Point2::new(16.0, -2.0)]],
                ground_level: Length::m(-0.30),
                georef: Georef {
                    epsg: 2100,
                    easting: 400_000.0,
                    northing: 4_200_000.0,
                    rotation: Angle::ZERO,
                    altitude: 120.0,
                },
                ..site
            },
        })?;
        self.run(SetPlot { plot: Some(rect(-4.0, -6.0, 16.0, 19.0)) })?;
        self.run(SetBuildingTerms {
            terms: BuildingTerms {
                building_coefficient: 0.8,
                coverage_ratio: 0.6,
                max_height: Length::m(7.5),
                max_floors: Some(2),
                volume_coefficient: None,
                setback: None,
            },
        })?;
        Ok(())
    }

    // ----- Ground floor --------------------------------------------------------------------------------------

    fn ground_floor(&mut self) -> Result<(), CommandError> {
        let g = self.ground;
        let ext = self.ring(
            g,
            &[(0.0, 0.0), (0.0, 10.0), (12.0, 10.0), (12.0, 2.4), (8.4, 2.4), (8.4, 0.0)],
            H_GROUND,
        )?;
        let [west, north, east, porch_n, porch_w, south] = ext.as_slice() else {
            return Err(missing("ring"));
        };
        let (west, north, east, porch_n, porch_w, south) = (*west, *north, *east, *porch_n, *porch_w, *south);
        let i = 10.0 - EXT_T; // inner face of the north wall
        let a = self.wall(g, (EXT_T, 5.8), (12.0 - EXT_T, 5.8), false, H_PART_GROUND)?;
        let b = self.wall(g, (4.2, 5.8), (4.2, i), false, H_PART_GROUND)?;
        self.wall(g, (7.6, 5.8), (7.6, i), false, H_PART_GROUND)?;
        self.wall(g, (9.9, 5.8), (9.9, i), false, H_PART_GROUND)?;
        let f = self.wall(g, (6.5, EXT_T), (6.5, 5.8), false, H_PART_GROUND)?;
        let gw = self.wall(g, (6.5, 2.3), (8.4 - EXT_T, 2.3), false, H_PART_GROUND)?;

        self.columns(
            g,
            &[
                (0.24, 0.24),
                (6.5, 0.24),
                (8.16, 0.24),
                (8.16, 2.64),
                (11.76, 0.24),
                (11.76, 2.64),
                (11.76, 5.8),
                (11.76, 9.76),
                (7.6, 9.76),
                (0.24, 9.76),
                (0.24, 5.8),
                (6.5, 5.8),
            ],
            H_GROUND,
        )?;

        use OpeningKind::{Door, Void, Window};
        self.openings(porch_n, &[(Door, "door.single_swing", "Π1", (10.0, 2.4), 1.0, 2.2, 0.0, false)])?;
        self.openings(
            south,
            &[
                (Door, "door.sliding", "Π4", (2.0, 0.0), 1.6, 2.2, 0.0, false),
                (Window, "window.casement_2", "Θ1", (5.0, 0.0), 1.2, 1.4, 0.9, false),
            ],
        )?;
        self.openings(west, &[(Window, "window.casement_2", "Θ1", (0.0, 3.0), 1.2, 1.4, 0.9, false)])?;
        self.openings(
            north,
            &[
                (Window, "window.casement_1", "Θ2", (3.2, 10.0), 0.8, 1.2, 1.05, false),
                (Window, "window.casement_2", "Θ1", (5.9, 10.0), 1.2, 1.4, 0.9, false),
                (Window, "window.casement_1", "Θ2", (8.75, 10.0), 0.8, 1.2, 1.8, false),
            ],
        )?;
        self.openings(
            east,
            &[
                (Window, "window.casement_1", "Θ3", (12.0, 7.8), 0.6, 0.6, 1.6, false),
                (Window, "window.casement_1", "Θ2", (12.0, 4.2), 0.8, 1.2, 0.9, false),
            ],
        )?;
        self.openings(porch_w, &[(Window, "window.casement_1", "Θ3", (8.4, 1.2), 0.6, 0.6, 1.6, false)])?;
        self.openings(f, &[(Door, "door.single_swing", "Π3", (6.5, 4.0), 0.9, 2.1, 0.0, false)])?;
        self.openings(gw, &[(Door, "door.single_swing", "Π2", (7.3, 2.3), 0.8, 2.1, 0.0, true)])?;
        self.openings(
            a,
            &[
                (Void, "void.rectangular", "", (5.35, 5.8), 1.8, 2.4, 0.0, false),
                (Void, "void.rectangular", "", (8.75, 5.8), 1.9, 2.4, 0.0, false),
                (Door, "door.single_swing", "Π2", (10.8, 5.8), 0.8, 2.1, 0.0, false),
            ],
        )?;
        self.openings(b, &[(Door, "door.single_swing", "Π3", (4.2, 7.0), 0.9, 2.1, 0.0, false)])?;

        // Slabs: the ground slab follows the outer faces; the porch is paved separately, 2 cm lower.
        self.run(SlabFromWalls {
            storey: g,
            face: OutlineFace::Outer,
            build_up: None,
            top_offset: Length::ZERO,
            holes: Vec::new(),
            tag: String::new(),
        })?;
        self.create(
            g,
            ElementKind::Slab(Slab {
                outline: PolygonWithHoles::from(rect(8.4, 0.0, 12.0, 2.4)),
                structure: Structure::Basic { material: self.marble, thickness: Length::cm(3.0) },
                top_offset: Length::m(-0.02),
            }),
        )?;

        // Half-turn stair: 18 risers of 17,8 cm, two flights of nine, 28 cm going.
        self.create(
            g,
            ElementKind::Stair(Stair {
                origin: Point2::new(8.2, 6.4),
                direction: Angle::deg(90.0),
                shape: StairShape::HalfTurnRight,
                width: Length::m(1.0),
                rise: Length::m(3.2),
                risers: 18,
                going: Length::m(0.28),
                first_flight_risers: Some(9),
                gap: Length::m(0.1),
                thickness: Length::m(0.15),
                material: self.concrete,
                base_offset: Length::ZERO,
            }),
        )?;

        // Spaces: the first through detection around a point, the rest drawn explicitly.
        let out = self.run(DetectSpace {
            storey: g,
            point: Point2::new(3.0, 3.0),
            name: "Καθιστικό".into(),
            number: "0.01".into(),
            category: SpaceCategory::MainUse,
        })?;
        let living = out.created.first().copied().ok_or_else(|| missing("living room"))?;
        let mut sp = self
            .s
            .document()
            .element(living)
            .and_then(|e| e.as_space())
            .cloned()
            .ok_or_else(|| missing("space"))?;
        sp.finish = "Πλακίδια".into();
        self.run(crate::commands::UpdateElement {
            id: living,
            kind: Some(ElementKind::Space(sp)),
            ..Default::default()
        })?;
        let rooms: [(Polygon2, &str, &str, SpaceCategory, &str); 7] = [
            (rect(0.31, 5.865, 4.135, i), "Κουζίνα", "0.02", SpaceCategory::MainUse, "Πλακίδια"),
            (rect(4.265, 5.865, 7.535, i), "Τραπεζαρία", "0.03", SpaceCategory::MainUse, "Πλακίδια"),
            (rect(7.665, 5.865, 9.835, i), "Κλιμακοστάσιο", "0.04", SpaceCategory::Stairwell, "Μάρμαρο"),
            (rect(9.965, 5.865, 11.69, i), "Λουτρό", "0.05", SpaceCategory::MainUse, "Πλακίδια"),
            (
                Polygon2::from_m(&[
                    (6.565, 2.365),
                    (8.09, 2.365),
                    (8.09, 2.71),
                    (11.69, 2.71),
                    (11.69, 5.735),
                    (6.565, 5.735),
                ]),
                "Διάδρομος",
                "0.06",
                SpaceCategory::MainUse,
                "Μάρμαρο",
            ),
            (rect(6.565, 0.31, 8.09, 2.235), "Αποθήκη", "0.07", SpaceCategory::Auxiliary, "Πλακίδια"),
            (rect(8.4, 0.0, 12.0, 2.4), "Ημιυπαίθριος", "0.08", SpaceCategory::SemiOpen, "Μάρμαρο"),
        ];
        for (poly, name, number, category, finish) in rooms {
            self.space(g, poly, name, number, category, finish)?;
        }

        // Furniture and fixtures.
        // Objects against a wall are placed by the middle of their front edge; tables, trees and cars by their
        // centre (the catalogue's convention).
        self.object(g, "furniture.sofa", (2.5, 4.835), 0.0)?;
        self.object(g, "furniture.coffee_table", (2.5, 3.7), 0.0)?;
        self.object(g, "furniture.table", (5.9, 7.75), 0.0)?;
        for (x, y, deg) in [(5.5, 7.25, 180.0), (6.3, 7.25, 180.0), (5.5, 8.25, 0.0), (6.3, 8.25, 0.0)] {
            self.object(g, "furniture.chair", (x, y), deg)?;
        }
        self.object(g, "kitchen.hob", (0.7, i - 0.6), 0.0)?;
        self.object_sized(g, "kitchen.counter", (1.9, i - 0.6), 0.0, &[("width", 1.8)])?;
        self.object(g, "kitchen.sink", (3.2, i - 0.6), 0.0)?;
        self.object(g, "kitchen.fridge", (0.96, 6.5), 90.0)?;
        self.object(g, "sanitary.wc", (10.3, i - 0.68), 0.0)?;
        self.object(g, "sanitary.basin", (11.24, 7.5), -90.0)?;
        self.object(g, "sanitary.shower", (11.2, 8.75), 0.0)?;
        self.object(g, "vegetation.tree", (-2.0, 15.0), 0.0)?;
        self.object(g, "vegetation.tree", (14.0, 16.0), 0.0)?;
        self.object(g, "vehicle.car", (14.0, -3.25), 0.0)?;

        // Annotation: levels, an interior dimension chain, exterior chains, the street name.
        self.level(g, (3.0, 1.2), None)?;
        self.level(g, (10.6, 1.0), Some(-0.02))?;
        self.dimension(g, &[(EXT_T, 1.5), (6.435, 1.5), (6.565, 1.5), (8.09, 1.5)], (0.0, 1.5))?;
        self.run(AutoDimensions {
            storey: g,
            offset: Length::m(1.2),
            spacing: Length::m(0.7),
            openings: true,
            overall: true,
        })?;
        self.create(
            g,
            ElementKind::Text(TextNote {
                position: Point2::new(6.0, -7.2),
                content: "ΟΔΟΣ ΛΕΜΟΝΙΑΣ".into(),
                size_mm: 3.5,
                rotation: Angle::ZERO,
                align: TextAlign::Centre,
                bold: true,
            }),
        )?;
        Ok(())
    }

    // ----- First floor ---------------------------------------------------------------------------------------

    fn first_floor(&mut self) -> Result<(), CommandError> {
        let g = self.first;
        let ext = self.ring(g, &[(0.0, 0.0), (0.0, 10.0), (12.0, 10.0), (12.0, 0.0)], H_FIRST)?;
        let [west, north, east, south] = ext.as_slice() else { return Err(missing("ring")) };
        let (west, north, east, south) = (*west, *north, *east, *south);
        let i = 10.0 - EXT_T;
        let h = self.wall(g, (EXT_T, 5.0), (12.0 - EXT_T, 5.0), false, H_FIRST)?;
        self.wall(g, (4.6, EXT_T), (4.6, 5.0), false, H_FIRST)?;
        self.wall(g, (8.4, EXT_T), (8.4, 5.0), false, H_FIRST)?;
        let l = self.wall(g, (EXT_T, 6.4), (7.6, 6.4), false, H_FIRST)?;
        self.wall(g, (3.6, 6.4), (3.6, i), false, H_FIRST)?;
        self.wall(g, (5.4, 6.4), (5.4, i), false, H_FIRST)?;
        self.wall(g, (7.6, 6.4), (7.6, i), false, H_FIRST)?;
        let d = self.wall(g, (9.9, 5.0), (9.9, i), false, H_FIRST)?;

        self.columns(
            g,
            &[
                (0.24, 0.24),
                (6.5, 0.24),
                (8.16, 0.24),
                (11.76, 0.24),
                (11.76, 2.64),
                (11.76, 5.8),
                (11.76, 9.76),
                (7.6, 9.76),
                (0.24, 9.76),
                (0.24, 5.8),
            ],
            H_FIRST,
        )?;

        use OpeningKind::{Door, Window};
        self.openings(
            south,
            &[
                (Door, "door.sliding", "Π4", (2.4, 0.0), 1.6, 2.2, 0.0, false),
                (Door, "door.sliding", "Π4", (5.5, 0.0), 1.6, 2.2, 0.0, false),
                (Window, "window.casement_2", "Θ1", (10.1, 0.0), 1.2, 1.4, 0.9, false),
            ],
        )?;
        self.openings(
            east,
            &[
                (Window, "window.casement_1", "Θ2", (12.0, 3.7), 0.8, 1.2, 0.9, false),
                (Window, "window.casement_1", "Θ3", (12.0, 7.3), 0.6, 0.6, 1.6, false),
            ],
        )?;
        self.openings(
            north,
            &[
                (Window, "window.casement_1", "Θ2", (1.9, 10.0), 0.8, 1.2, 1.5, false),
                (Window, "window.casement_1", "Θ3", (4.5, 10.0), 0.6, 0.6, 1.6, false),
                (Window, "window.casement_1", "Θ2", (6.4, 10.0), 0.8, 1.2, 0.9, false),
                (Window, "window.casement_1", "Θ2", (8.75, 10.0), 0.8, 1.2, 0.9, false),
            ],
        )?;
        self.openings(west, &[(Window, "window.casement_2", "Θ1", (0.0, 2.6), 1.2, 1.4, 0.9, false)])?;
        self.openings(
            h,
            &[
                (Door, "door.single_swing", "Π3", (3.8, 5.0), 0.9, 2.1, 0.0, true),
                (Door, "door.single_swing", "Π3", (7.0, 5.0), 0.9, 2.1, 0.0, true),
                (Door, "door.single_swing", "Π3", (9.1, 5.0), 0.9, 2.1, 0.0, true),
            ],
        )?;
        self.openings(
            l,
            &[
                (Door, "door.single_swing", "Π2", (2.5, 6.4), 0.8, 2.1, 0.0, false),
                (Door, "door.single_swing", "Π2", (4.5, 6.4), 0.8, 2.1, 0.0, false),
                (Door, "door.single_swing", "Π2", (6.5, 6.4), 0.8, 2.1, 0.0, false),
            ],
        )?;
        self.openings(d, &[(Door, "door.single_swing", "Π2", (9.9, 5.7), 0.8, 2.1, 0.0, true)])?;

        // First-floor slab at the core faces with the stair well; cantilevered balcony to the south.
        self.run(SlabFromWalls {
            storey: g,
            face: OutlineFace::Core,
            build_up: None,
            top_offset: Length::ZERO,
            holes: vec![rect(7.665, 6.4, 9.835, i)],
            tag: String::new(),
        })?;
        self.create(
            g,
            ElementKind::Slab(Slab {
                outline: PolygonWithHoles::from(rect(0.5, -1.5, 7.5, 0.09)),
                structure: Structure::Basic { material: self.concrete, thickness: Length::cm(20.0) },
                top_offset: Length::m(-0.02),
            }),
        )?;
        self.railing(g, &[(0.55, 0.0), (0.55, -1.45), (7.45, -1.45), (7.45, 0.0)], -0.02)?;
        self.railing(g, &[(7.7, 6.37), (8.75, 6.37)], 0.0)?;

        // Hip roof over the first floor.
        self.run(RoofFromWalls {
            storey: g,
            kind: RoofKind::Hip,
            pitch: 22.0,
            overhang: Length::m(0.6),
            build_up: None,
            base: None,
            parapet: None,
            tag: String::new(),
        })?;

        let rooms: [(Polygon2, &str, &str, SpaceCategory, &str); 10] = [
            (rect(0.31, 0.31, 4.535, 4.935), "Υπνοδωμάτιο 1", "1.01", SpaceCategory::MainUse, "Ξύλο"),
            (rect(4.665, 0.31, 8.335, 4.935), "Υπνοδωμάτιο 2", "1.02", SpaceCategory::MainUse, "Ξύλο"),
            (rect(8.465, 0.31, 11.69, 4.935), "Υπνοδωμάτιο 3", "1.03", SpaceCategory::MainUse, "Ξύλο"),
            (rect(0.31, 5.065, 7.665, 6.335), "Διάδρομος", "1.04", SpaceCategory::MainUse, "Ξύλο"),
            (rect(0.31, 6.465, 3.535, i), "Λουτρό", "1.05", SpaceCategory::MainUse, "Πλακίδια"),
            (rect(3.665, 6.465, 5.335, i), "WC", "1.06", SpaceCategory::MainUse, "Πλακίδια"),
            (rect(5.465, 6.465, 7.535, i), "Γραφείο", "1.07", SpaceCategory::MainUse, "Ξύλο"),
            (rect(7.665, 5.065, 9.835, i), "Κλιμακοστάσιο", "1.08", SpaceCategory::Stairwell, "Μάρμαρο"),
            (rect(9.965, 5.065, 11.69, i), "Αποθήκη", "1.09", SpaceCategory::Auxiliary, "Πλακίδια"),
            (rect(0.5, -1.5, 7.5, 0.0), "Εξώστης", "1.10", SpaceCategory::Balcony, "Πλακίδια"),
        ];
        for (poly, name, number, category, finish) in rooms {
            self.space(g, poly, name, number, category, finish)?;
        }

        self.object(g, "furniture.bed_double", (2.4, 2.935), 0.0)?;
        self.object(g, "furniture.wardrobe", (3.935, 1.5), -90.0)?;
        self.object(g, "furniture.bed_double", (5.6, 2.935), 0.0)?;
        self.object(g, "furniture.wardrobe", (7.735, 1.5), -90.0)?;
        self.object(g, "furniture.bed_single", (9.69, 3.5), -90.0)?;
        self.object(g, "furniture.wardrobe", (9.065, 1.5), 90.0)?;
        self.object(g, "sanitary.bath", (1.2, i - 0.75), 0.0)?;
        self.object(g, "sanitary.wc", (0.99, 7.4), 90.0)?;
        self.object(g, "sanitary.basin", (3.085, 8.0), -90.0)?;
        self.object(g, "sanitary.wc", (4.2, i - 0.68), 0.0)?;
        self.object(g, "sanitary.basin", (4.9, i - 0.45), 0.0)?;
        self.object(g, "furniture.desk", (6.5, i - 0.7), 0.0)?;
        self.object(g, "furniture.chair", (6.5, i - 0.75), 180.0)?;

        self.level(g, (2.4, 1.2), None)?;
        self.level(g, (4.0, -0.8), Some(3.18))?;
        self.dimension(
            g,
            &[(EXT_T, 0.9), (4.535, 0.9), (4.665, 0.9), (8.335, 0.9), (8.465, 0.9), (12.0 - EXT_T, 0.9)],
            (0.0, 0.9),
        )?;
        self.run(AutoDimensions {
            storey: g,
            offset: Length::m(2.4),
            spacing: Length::m(0.7),
            openings: true,
            overall: true,
        })?;
        Ok(())
    }

    // ----- Views and sheets ----------------------------------------------------------------------------------

    fn view(&mut self, name: &str, kind: ViewKind) -> Result<Id, CommandError> {
        let out = self.run(CreateView { name: name.into(), kind, scale: 100 })?;
        let id = out.created.first().copied().ok_or_else(|| missing("view"))?;
        // Permit sections and elevations leave out furniture, trees and cars.
        self.run(UpdateView {
            id,
            name: None,
            kind: None,
            scale: None,
            hidden_layers: Some(std::collections::BTreeSet::from(["object".to_owned()])),
        })?;
        Ok(id)
    }

    fn view_levels(&mut self, view: Id, along: f64) -> Result<(), CommandError> {
        for (z, label) in [(0.0, "Ισόγειο"), (3.2, "1ος όροφος"), (6.2, "Στέγη")] {
            let c = CreateElement {
                storey: None,
                view: Some(view),
                kind: ElementKind::Level(LevelMark {
                    position: Point2::new(along, z),
                    value: None,
                    label: label.into(),
                }),
                tag: String::new(),
                phase: Default::default(),
                properties: BTreeMap::new(),
            };
            self.run(c)?;
        }
        Ok(())
    }

    fn views_and_sheets(&mut self) -> Result<(), CommandError> {
        // Plans at 1:100 for the permit drawings.
        let plans: Vec<Id> = self
            .s
            .document()
            .views()
            .filter(|v| matches!(v.kind, ViewKind::Plan { .. }))
            .map(|v| v.id)
            .collect();
        for id in plans {
            self.run(UpdateView { id, name: None, kind: None, scale: Some(100), hidden_layers: None })?;
        }
        let section = |start: (f64, f64), end: (f64, f64), marker: &str| ViewKind::Section {
            start: Point2::new(start.0, start.1),
            end: Point2::new(end.0, end.1),
            depth: Length::m(20.0),
            flip: false,
            marker: marker.into(),
        };
        let elevation = |start: (f64, f64), end: (f64, f64), marker: &str| ViewKind::Elevation {
            start: Point2::new(start.0, start.1),
            end: Point2::new(end.0, end.1),
            depth: Length::m(30.0),
            flip: false,
            marker: marker.into(),
        };
        let a = self.view("Τομή Α-Α", section((-3.0, 4.2), (15.0, 4.2), "Α"))?;
        let b = self.view("Τομή Β-Β", section((9.3, -3.0), (9.3, 13.0), "Β"))?;
        let south = self.view("Νότια όψη", elevation((-4.0, -8.0), (16.0, -8.0), "1"))?;
        let east = self.view("Ανατολική όψη", elevation((20.0, -6.0), (20.0, 19.0), "2"))?;
        let north = self.view("Βόρεια όψη", elevation((16.0, 21.0), (-4.0, 21.0), "3"))?;
        let west = self.view("Δυτική όψη", elevation((-8.0, 19.0), (-8.0, -6.0), "4"))?;
        // Level marks a metre before each line's building edge (x along the line, y elevation).
        for (v, along) in [(a, 1.5), (b, 1.5), (south, 2.5), (east, 4.0), (north, 2.5), (west, 4.0)] {
            self.view_levels(v, along)?;
        }

        // The permit set lays out its sheets (papers, viewport positions and crops) itself.
        self.run(PermitSet { locale: Locale::El })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::Category;

    fn count(doc: &Document, c: Category) -> usize {
        doc.elements_of(c).count()
    }

    #[test]
    fn sample_house_is_complete_and_consistent() {
        let session = sample_session().unwrap();
        let doc = session.document();
        doc.validate().unwrap();
        for e in doc.elements() {
            kanavos_elements::validate(doc, e).unwrap_or_else(|err| panic!("{}: {err}", e.id));
        }
        assert_eq!(count(doc, Category::Wall), 24);
        assert_eq!(count(doc, Category::Column), 22);
        assert_eq!(count(doc, Category::Door), 15);
        assert_eq!(count(doc, Category::Window), 16);
        assert_eq!(count(doc, Category::Opening), 2);
        assert_eq!(count(doc, Category::Space), 18);
        assert_eq!(count(doc, Category::Slab), 4);
        assert_eq!(count(doc, Category::Stair), 1);
        assert_eq!(count(doc, Category::Roof), 1);
        assert_eq!(count(doc, Category::Railing), 2);
        assert!(count(doc, Category::Object) >= 28);
        assert!(count(doc, Category::Dimension) >= 10);
        assert!(count(doc, Category::Level) >= 20);
        let views = |f: fn(&ViewKind) -> bool| doc.views().filter(|v| f(&v.kind)).count();
        assert_eq!(views(|k| matches!(k, ViewKind::Section { .. })), 2);
        assert_eq!(views(|k| matches!(k, ViewKind::Elevation { .. })), 4);
        assert!(doc.sheets().len() >= 6);
        // Every door and window is tagged; tags follow the type.
        for e in doc.elements() {
            if let ElementKind::Opening(o) = &e.kind
                && o.kind != OpeningKind::Void
            {
                let prefix = if o.kind == OpeningKind::Door { "Π" } else { "Θ" };
                assert!(e.tag.starts_with(prefix), "{} has tag '{}'", e.id, e.tag);
            }
            // Every style and object comes from the catalogue.
            match &e.kind {
                ElementKind::Opening(o) => assert!(kanavos_objects::find(&o.style).is_some(), "{}", o.style),
                ElementKind::Object(o) => {
                    assert!(kanavos_objects::find(&o.definition).is_some(), "{}", o.definition)
                }
                _ => {}
            }
        }
        // Doors in exterior walls open inwards: the exterior walls run clockwise, so they are flipped.
        let entrance = doc.elements().find(|e| e.tag == "Π1").and_then(|e| e.as_opening()).unwrap();
        assert!(entrance.flip_side);
        // Each step is one undo step and the session replays from its log.
        let log = session.log_jsonl();
        let commands = crate::log::parse_jsonl(&log).unwrap();
        let replayed = Session::replay(sample_start(), commands).unwrap();
        assert_eq!(replayed.document(), doc);
    }

    #[test]
    fn sample_house_meets_its_building_terms() {
        let doc = sample_house().unwrap();
        let r = kanavos_rules_gr::nok_report(&doc);
        let plot = r.plot_area.unwrap().as_m2();
        assert!((plot - 500.0).abs() < 1e-6);
        // The footprint is 12 × 10 m; how the code counts its parts is the rules' business, so only the ranges a
        // house of this size must fall in are checked.
        let coverage = r.meters.coverage.used.as_m2();
        assert!(coverage > 100.0 && coverage <= 120.5, "coverage {coverage}");
        assert!(r.meters.coverage.within_limit());
        assert!(r.meters.floor_area.within_limit());
        assert_eq!(r.storeys.len(), 2);
        for s in &r.storeys {
            let gross = s.gross.as_m2();
            assert!(gross > 100.0 && gross < 135.0, "{}: gross {gross}", s.name);
            let counted = s.counted.as_m2();
            assert!(counted > 80.0 && counted < 120.0, "{}: counted {counted}", s.name);
        }
        let (height, max) = r.height;
        assert!(height.as_m() > 6.0 && height.as_m() <= max.as_m(), "height {}", height.as_m());
    }

    #[test]
    fn sample_views_and_sheets_draw() {
        let doc = sample_house().unwrap();
        for v in doc.views() {
            if matches!(v.kind, ViewKind::ThreeD { .. }) {
                continue;
            }
            let d = kanavos_sheets::render_view(&doc, v.id, Locale::El).unwrap();
            if matches!(v.kind, ViewKind::Plan { .. } | ViewKind::Section { .. } | ViewKind::Elevation { .. })
            {
                assert!(!d.items.is_empty(), "view {} is empty", v.name);
            }
        }
        for s in doc.sheets() {
            let d = kanavos_sheets::sheet_drawing(&doc, s.id, Locale::El).unwrap();
            assert!(!d.items.is_empty());
        }
    }

    #[test]
    fn room_detection_finds_the_drawn_rooms() {
        let doc = sample_house().unwrap();
        let ground = doc.storeys()[0].id;
        let rooms = crate::geo::detect_all_spaces(&doc, ground).unwrap();
        // Living, kitchen, dining, stairwell, shower room, hall, storage: the porch is outside.
        assert_eq!(rooms.len(), 7, "{rooms:?}");
        // The clear area between wall faces, less the corners of the concrete columns that reach into the room.
        let largest = rooms[0].area().as_m2();
        assert!(largest < 6.125 * 5.425 && largest > 6.125 * 5.425 - 0.1, "{largest}");
        let first = doc.storeys()[1].id;
        // The corridor and the stairwell form one room when detected; the balcony has no walls.
        assert_eq!(crate::geo::detect_all_spaces(&doc, first).unwrap().len(), 8);
    }
}
