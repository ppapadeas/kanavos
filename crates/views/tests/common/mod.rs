//! Shared test fixtures: a two-storey house built with the model API, and PNG output for visual checks.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_arguments
)]

use std::collections::BTreeMap;

use kanavos_geom::{Angle, Length, Point2, Point3, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Camera, Column, Document, Element, ElementKind, Id, LevelMark, Object, Opening, OpeningKind, Parapet,
    Profile, Railing, RailingInfill, ReferenceLine, Roof, RoofShape, Slab, Space, SpaceCategory, Stair,
    StairShape, Structure, View, ViewKind, Wall, starter,
};

/// A fixture house and the ids tests need.
pub struct House {
    pub doc: Document,
    pub ground: Id,
    pub first: Id,
    pub section: Id,
    pub elevation: Id,
    pub axo: Id,
    pub perspective: Id,
}

pub struct Builder {
    pub doc: Document,
}

impl Builder {
    pub fn new(doc: Document) -> Builder {
        Builder { doc }
    }

    pub fn id(&mut self) -> Id {
        let (d, id) = self.doc.with_new_id();
        self.doc = d;
        id
    }

    pub fn add(&mut self, storey: Option<Id>, kind: ElementKind) -> Id {
        let id = self.id();
        let mut e = Element::on_storey(id, Id(0), kind);
        e.storey = storey;
        self.doc = self.doc.with_element(e);
        id
    }

    pub fn material(&self, name: &str) -> Id {
        self.doc.materials().find(|m| m.name == name).unwrap_or_else(|| panic!("material {name}")).id
    }

    pub fn build_up(&self, name: &str) -> Id {
        self.doc.build_ups().find(|b| b.name == name).unwrap_or_else(|| panic!("build-up {name}")).id
    }

    pub fn wall(&mut self, storey: Id, a: (f64, f64), b: (f64, f64), build_up: Id, height: f64) -> Id {
        self.add(
            Some(storey),
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Layered { build_up },
                height: Length::m(height),
                base_offset: Length::ZERO,
            }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn opening(
        &mut self,
        storey: Id,
        host: Id,
        kind: OpeningKind,
        style: &str,
        offset: f64,
        width: f64,
        height: f64,
        sill: f64,
        flip_side: bool,
    ) -> Id {
        self.add(
            Some(storey),
            ElementKind::Opening(Opening {
                host,
                kind,
                style: style.into(),
                offset: Length::m(offset),
                width: Length::m(width),
                height: Length::m(height),
                sill: Length::m(sill),
                flip_side,
                flip_hand: false,
                params: BTreeMap::new(),
            }),
        )
    }

    pub fn column(&mut self, storey: Id, x: f64, y: f64, size: f64, height: f64) -> Id {
        let material = self.material("Οπλισμένο σκυρόδεμα");
        self.add(
            Some(storey),
            ElementKind::Column(Column {
                position: Point2::new(x, y),
                rotation: Angle::ZERO,
                profile: Profile::Rectangle { width: Length::m(size), depth: Length::m(size) },
                material,
                height: Length::m(height),
                base_offset: Length::ZERO,
            }),
        )
    }

    pub fn space(
        &mut self,
        storey: Id,
        name: &str,
        number: &str,
        c: SpaceCategory,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) -> Id {
        self.add(
            Some(storey),
            ElementKind::Space(Space {
                boundary: Polygon2::rectangle(Point2::new(x0, y0), Point2::new(x1, y1)).into(),
                name: name.into(),
                number: number.into(),
                category: c,
                height: None,
                stamp: None,
                finish: String::new(),
            }),
        )
    }

    pub fn object(&mut self, storey: Id, def: &str, x: f64, y: f64, rot_deg: f64) -> Id {
        self.add(
            Some(storey),
            ElementKind::Object(Object {
                definition: def.into(),
                position: Point2::new(x, y),
                rotation: Angle::deg(rot_deg),
                mirrored: false,
                elevation: Length::ZERO,
                params: BTreeMap::new(),
            }),
        )
    }

    /// An object standing with its back against a wall face at `y`, facing south (`north = true`) or north.
    pub fn against(&mut self, storey: Id, def: &str, x: f64, y: f64, north: bool) -> Id {
        let depth = kanavos_objects::find(def).map(|d| d.depth).unwrap_or(0.6);
        if north {
            self.object(storey, def, x, y - depth, 0.0)
        } else {
            self.object(storey, def, x, y + depth, 180.0)
        }
    }

    pub fn view(&mut self, name: &str, kind: ViewKind, scale: u32) -> Id {
        let id = self.id();
        self.doc = self.doc.with_view(View {
            id,
            name: name.into(),
            kind,
            scale,
            hidden_layers: Default::default(),
        });
        id
    }
}

/// Half the exterior wall thickness (31 cm build-up).
const T: f64 = 0.155;

/// A two-storey house of 12 × 9 m (wall axes) with a half-turn stair, a flat roof with parapet and a balcony.
pub fn house() -> House {
    let mut b = Builder::new(starter());
    let storeys: Vec<Id> = b.doc.storeys().iter().map(|s| s.id).collect();
    let (ground, first) = (storeys[0], storeys[1]);
    let ext = b.build_up("Εξωτερικός τοίχος 20 με θερμοπρόσοψη 8");
    let int = b.build_up("Εσωτερικός τοίχος δρομικός 9");
    let ground_slab = b.build_up("Πλάκα επί εδάφους με θερμομόνωση");
    let floor_slab = b.build_up("Πλάκα ορόφου 20 με δάπεδο");
    let flat_roof = b.build_up("Δώμα με ανεστραμμένη μόνωση");
    let concrete = b.material("Οπλισμένο σκυρόδεμα");
    let steel = b.material("Χάλυβας");
    let mut site = b.doc.site().clone();
    site.ground_level = Length::m(-0.45);
    site.georef.rotation = Angle::deg(-12.0);
    b.doc = b.doc.with_site(site);

    for (storey, h) in [(ground, 3.2), (first, 3.0)] {
        // Exterior walls, clockwise so the left face (render and insulation) is outside; extended to the corners.
        // Their doors and windows therefore use flip_side, since symbols put the outer face on the right.
        let w_west = b.wall(storey, (0.0, -T), (0.0, 9.0 + T), ext, h);
        let w_north = b.wall(storey, (-T, 9.0), (12.0 + T, 9.0), ext, h);
        let w_east = b.wall(storey, (12.0, 9.0 + T), (12.0, -T), ext, h);
        let w_south = b.wall(storey, (12.0 + T, 0.0), (-T, 0.0), ext, h);
        for (x, y) in
            [(0.085, 0.085), (11.915, 0.085), (0.085, 8.915), (11.915, 8.915), (6.0, 0.085), (6.0, 8.915)]
        {
            b.column(storey, x, y, 0.30, h);
        }
        // Interior walls.
        let i_mid = b.wall(storey, (7.0, T), (7.0, 9.0 - T), int, h);
        let i_east = b.wall(storey, (7.065, 4.5), (12.0 - T, 4.5), int, h);
        if storey == ground {
            // South: entrance door and two living-room windows.
            b.opening(
                storey,
                w_south,
                OpeningKind::Door,
                "door.single_swing",
                12.0 + T - 8.2,
                1.0,
                2.2,
                0.0,
                true,
            );
            b.opening(
                storey,
                w_south,
                OpeningKind::Window,
                "window.casement_2",
                12.0 + T - 2.0,
                1.2,
                1.4,
                0.9,
                true,
            );
            b.opening(
                storey,
                w_south,
                OpeningKind::Window,
                "window.casement_2",
                12.0 + T - 4.6,
                1.2,
                1.4,
                0.9,
                true,
            );
            // West window.
            b.opening(storey, w_west, OpeningKind::Window, "window.casement_2", T + 4.5, 1.2, 1.4, 0.9, true);
            // North: sliding door to the garden and a high kitchen window above the cut plane.
            b.opening(storey, w_north, OpeningKind::Door, "door.sliding", T + 3.5, 1.6, 2.2, 0.0, true);
            b.opening(storey, w_north, OpeningKind::Window, "window.fixed", T + 9.5, 1.0, 1.0, 1.2, true);
            // East: kitchen window and a small high stair window.
            b.opening(
                storey,
                w_east,
                OpeningKind::Window,
                "window.casement_2",
                9.0 + T - 6.8,
                1.2,
                1.4,
                0.9,
                true,
            );
            b.opening(
                storey,
                w_east,
                OpeningKind::Window,
                "window.fixed",
                9.0 + T - 2.0,
                0.6,
                0.6,
                1.8,
                true,
            );
            // Interior doors.
            b.opening(storey, i_mid, OpeningKind::Door, "door.single_swing", 2.2 - T, 0.9, 2.1, 0.0, false);
            b.opening(storey, i_mid, OpeningKind::Door, "door.single_swing", 6.5 - T, 0.9, 2.1, 0.0, true);
            b.opening(
                storey,
                i_east,
                OpeningKind::Door,
                "door.single_swing",
                8.2 - 7.065,
                0.9,
                2.1,
                0.0,
                false,
            );
            // Spaces.
            b.space(
                storey,
                "Καθιστικό - Τραπεζαρία",
                "0.01",
                SpaceCategory::MainUse,
                T,
                T,
                7.0 - 0.065,
                9.0 - T,
            );
            b.space(storey, "Κουζίνα", "0.02", SpaceCategory::MainUse, 7.065, 4.565, 12.0 - T, 9.0 - T);
            b.space(
                storey,
                "Είσοδος - Κλιμακοστάσιο",
                "0.03",
                SpaceCategory::Stairwell,
                7.065,
                T,
                12.0 - T,
                4.435,
            );
            // Furniture.
            b.object(storey, "furniture.sofa", 3.0, 6.9, 0.0);
            b.object(storey, "furniture.table", 3.5, 2.6, 0.0);
            b.against(storey, "kitchen.counter", 9.6, 9.0 - T, true);
            // Stair: half turn, 18 risers of 17,8 cm, going 28 cm.
            b.add(
                Some(storey),
                ElementKind::Stair(Stair {
                    origin: Point2::new(11.3, 0.5),
                    direction: Angle::deg(90.0),
                    shape: StairShape::HalfTurnLeft,
                    width: Length::m(1.0),
                    rise: Length::m(3.2),
                    risers: 18,
                    going: Length::m(0.28),
                    first_flight_risers: Some(9),
                    gap: Length::m(0.1),
                    thickness: Length::m(0.15),
                    material: concrete,
                    base_offset: Length::ZERO,
                }),
            );
            // Ground slab.
            b.add(
                Some(storey),
                ElementKind::Slab(Slab {
                    outline: Polygon2::rectangle(Point2::new(-T, -T), Point2::new(12.0 + T, 9.0 + T)).into(),
                    structure: Structure::Layered { build_up: ground_slab },
                    top_offset: Length::ZERO,
                }),
            );
            b.add(
                Some(storey),
                ElementKind::Level(LevelMark {
                    position: Point2::new(5.2, 4.0),
                    value: None,
                    label: String::new(),
                }),
            );
        } else {
            let i_west = b.wall(storey, (T, 4.5), (6.935, 4.5), int, h);
            b.opening(
                storey,
                w_south,
                OpeningKind::Window,
                "window.casement_2",
                12.0 + T - 3.0,
                1.2,
                1.4,
                0.9,
                true,
            );
            b.opening(
                storey,
                w_south,
                OpeningKind::Door,
                "door.sliding",
                12.0 + T - 8.8,
                1.6,
                2.2,
                0.0,
                true,
            );
            b.opening(
                storey,
                w_north,
                OpeningKind::Window,
                "window.casement_2",
                T + 3.0,
                1.2,
                1.4,
                0.9,
                true,
            );
            b.opening(storey, w_north, OpeningKind::Window, "window.fixed", T + 9.5, 0.6, 0.6, 1.5, true);
            b.opening(storey, w_west, OpeningKind::Window, "window.casement_2", T + 6.8, 1.2, 1.4, 0.9, true);
            b.opening(storey, i_mid, OpeningKind::Door, "door.single_swing", 3.6 - T, 0.9, 2.1, 0.0, false);
            b.opening(storey, i_mid, OpeningKind::Door, "door.single_swing", 6.2 - T, 0.9, 2.1, 0.0, true);
            b.opening(
                storey,
                i_east,
                OpeningKind::Door,
                "door.single_swing",
                9.4 - 7.065,
                0.8,
                2.1,
                0.0,
                false,
            );
            let _ = i_west;
            b.space(storey, "Υπνοδωμάτιο 1", "1.01", SpaceCategory::MainUse, T, 4.565, 6.935, 9.0 - T);
            b.space(storey, "Υπνοδωμάτιο 2", "1.02", SpaceCategory::MainUse, T, T, 6.935, 4.435);
            b.space(storey, "Λουτρό", "1.03", SpaceCategory::MainUse, 7.065, 4.565, 12.0 - T, 9.0 - T);
            b.space(storey, "Διάδρομος", "1.04", SpaceCategory::Stairwell, 7.065, T, 12.0 - T, 4.435);
            b.against(storey, "furniture.bed_double", 3.5, 9.0 - T, true);
            b.against(storey, "furniture.bed_single", 2.0, T, false);
            b.against(storey, "furniture.wardrobe", 5.6, 4.435, true);
            b.against(storey, "sanitary.wc", 8.0, 9.0 - T, true);
            b.against(storey, "sanitary.basin", 9.0, 9.0 - T, true);
            b.against(storey, "sanitary.bath", 11.0, 9.0 - T, true);
            // First-floor slab with the stair hole and a balcony to the south.
            b.add(
                Some(storey),
                ElementKind::Slab(Slab {
                    outline: PolygonWithHoles {
                        outer: Polygon2::from_m(&[
                            (-T, -T),
                            (6.5, -T),
                            (6.5, -1.5),
                            (11.0, -1.5),
                            (11.0, -T),
                            (12.0 + T, -T),
                            (12.0 + T, 9.0 + T),
                            (-T, 9.0 + T),
                        ]),
                        holes: vec![Polygon2::rectangle(Point2::new(9.6, 0.4), Point2::new(11.85, 3.8))],
                    },
                    structure: Structure::Layered { build_up: floor_slab },
                    top_offset: Length::ZERO,
                }),
            );
            b.add(
                Some(storey),
                ElementKind::Railing(Railing {
                    path: vec![
                        Point2::new(6.55, -T),
                        Point2::new(6.55, -1.45),
                        Point2::new(10.95, -1.45),
                        Point2::new(10.95, -T),
                    ],
                    height: Length::m(1.0),
                    base_offset: Length::ZERO,
                    post_spacing: Length::m(1.2),
                    infill: RailingInfill::Bars,
                    material: steel,
                }),
            );
            // Flat roof with a parapet.
            b.add(
                Some(storey),
                ElementKind::Roof(Roof {
                    outline: Polygon2::rectangle(Point2::new(-T, -T), Point2::new(12.0 + T, 9.0 + T)),
                    base: Length::m(3.2 + 0.2),
                    structure: Structure::Layered { build_up: flat_roof },
                    shape: RoofShape::Flat {
                        parapet: Some(Parapet { height: Length::m(0.8), thickness: Length::m(0.2) }),
                    },
                }),
            );
            b.add(
                Some(storey),
                ElementKind::Level(LevelMark {
                    position: Point2::new(5.2, 3.6),
                    value: None,
                    label: String::new(),
                }),
            );
        }
    }
    // Exterior dimensions of the ground floor, as elements.
    for dim in kanavos_views::auto_dimensions(&b.doc, ground) {
        b.add(Some(ground), ElementKind::Dimension(dim));
    }
    let section = b.view(
        "Τομή Α-Α",
        ViewKind::Section {
            start: Point2::new(-2.5, 2.0),
            end: Point2::new(14.5, 2.0),
            depth: Length::m(12.0),
            flip: false,
            marker: "Α".into(),
        },
        50,
    );
    let elevation = b.view(
        "Νότια όψη",
        ViewKind::Elevation {
            start: Point2::new(-2.5, -4.0),
            end: Point2::new(14.5, -4.0),
            depth: Length::m(20.0),
            flip: false,
            marker: "Β".into(),
        },
        50,
    );
    let axo = b.view(
        "Αξονομετρικό",
        ViewKind::ThreeD {
            camera: Camera {
                eye: Point3::new(-14.0, -18.0, 16.0),
                target: Point3::new(6.0, 4.5, 3.0),
                fov: 20.0,
                parallel: true,
            },
        },
        100,
    );
    let perspective = b.view(
        "Προοπτικό",
        ViewKind::ThreeD {
            camera: Camera {
                eye: Point3::new(-9.0, -16.0, 5.0),
                target: Point3::new(6.0, 4.5, 3.0),
                fov: 40.0,
                parallel: false,
            },
        },
        100,
    );
    House { doc: b.doc, ground, first, section, elevation, axo, perspective }
}

/// Directory for visual test output.
pub fn out_dir() -> std::path::PathBuf {
    let base = std::env::var("CARGO_TARGET_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("target")
    });
    base.join("views-png")
}

/// Renders a drawing to `target/views-png/<name>` at 150 dpi on paper, for people to look at.
pub fn save_png(d: &kanavos_drawing::Drawing, name: &str) {
    let opts = kanavos_raster::RasterOptions {
        size: kanavos_raster::RasterSize::Dpi(150.0),
        margin_mm: 12.0,
        ..kanavos_raster::RasterOptions::default()
    };
    let png = kanavos_raster::render_png(d, &opts).expect("render");
    let dir = out_dir();
    std::fs::create_dir_all(&dir).expect("output directory");
    std::fs::write(dir.join(name), png).expect("write png");
}

/// A larger model: `n` copies of the fixture house's building elements, side by side along X every 14 m (the
/// first copy is the original). Views and annotation are kept from the original.
pub fn houses(n: usize) -> House {
    let h = house();
    let mut b = Builder::new(h.doc.clone());
    let originals: Vec<Element> = h
        .doc
        .elements()
        .filter(|e| e.category().is_building_part() || matches!(e.kind, ElementKind::Space(_)))
        .cloned()
        .collect();
    for k in 1..n {
        let t = kanavos_geom::Transform2::translation(kanavos_geom::Vec2::new(14.0 * k as f64, 0.0));
        let mut map: BTreeMap<Id, Id> = BTreeMap::new();
        for e in &originals {
            let id = b.id();
            map.insert(e.id, id);
        }
        for e in &originals {
            let mut c = e.clone();
            c.id = map[&e.id];
            c.kind = e.kind.transformed(&t);
            if let ElementKind::Opening(o) = &mut c.kind {
                o.host = map[&o.host];
            }
            b.doc = b.doc.with_element(c);
        }
    }
    House { doc: b.doc, ..h }
}
