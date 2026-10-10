//! Shared fixtures for the element integration tests.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

pub mod raster;

use kanavos_geom::{Angle, Length, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Column, Document, Element, ElementKind, Id, Opening, OpeningKind, Profile, RailingInfill, ReferenceLine,
    Roof, RoofShape, Slab, Stair, StairShape, Structure, Wall, starter,
};

/// A document built from the starter template, with handles to its storeys, build-ups and materials.
pub struct Fx {
    pub doc: Document,
    pub ground: Id,
    pub first: Id,
    /// Exterior wall build-up: plaster 1 | EPS 8 | brick 20 (core) | plaster 2 = 31 cm.
    pub ext: Id,
    /// Interior wall build-up: plaster 2 | brick 9 (core) | plaster 2 = 13 cm.
    pub int: Id,
    /// Concrete wall build-up: plaster 2 | concrete 25 | plaster 2 = 29 cm.
    pub concrete_wall: Id,
    pub concrete: Id,
    pub brick: Id,
    pub roof_build_up: Id,
    pub slab_build_up: Id,
}

pub fn fx() -> Fx {
    let doc = starter();
    let storeys: Vec<Id> = doc.storeys().iter().map(|s| s.id).collect();
    let by_name = |n: &str| doc.build_ups().find(|b| b.name.starts_with(n)).map(|b| b.id).unwrap();
    let mat = |n: &str| doc.materials().find(|m| m.name == n).map(|m| m.id).unwrap();
    Fx {
        ground: storeys[0],
        first: storeys[1],
        ext: doc.defaults().exterior_wall.unwrap(),
        int: doc.defaults().interior_wall.unwrap(),
        concrete_wall: by_name("Τοίχωμα σκυροδέματος"),
        concrete: mat("Οπλισμένο σκυρόδεμα"),
        brick: mat("Οπτοπλινθοδομή"),
        roof_build_up: doc.defaults().pitched_roof.unwrap(),
        slab_build_up: doc.defaults().floor_slab.unwrap(),
        doc,
    }
}

impl Fx {
    pub fn add(&mut self, storey: Id, kind: ElementKind) -> Id {
        let (d, id) = self.doc.with_new_id();
        self.doc = d.with_element(Element::on_storey(id, storey, kind));
        id
    }

    pub fn wall_with(&mut self, a: (f64, f64), b: (f64, f64), structure: Structure, height: f64) -> Id {
        let ground = self.ground;
        self.add(
            ground,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure,
                height: Length::m(height),
                base_offset: Length::ZERO,
            }),
        )
    }

    pub fn ext_wall(&mut self, a: (f64, f64), b: (f64, f64)) -> Id {
        let s = Structure::Layered { build_up: self.ext };
        self.wall_with(a, b, s, 3.0)
    }

    pub fn int_wall(&mut self, a: (f64, f64), b: (f64, f64)) -> Id {
        let s = Structure::Layered { build_up: self.int };
        self.wall_with(a, b, s, 3.0)
    }

    pub fn basic_wall(&mut self, a: (f64, f64), b: (f64, f64), thickness: f64, height: f64) -> Id {
        let s = Structure::Basic { material: self.brick, thickness: Length::m(thickness) };
        self.wall_with(a, b, s, height)
    }

    pub fn arc_wall(&mut self, a: (f64, f64), b: (f64, f64), bulge: f64) -> Id {
        let ground = self.ground;
        let s = Structure::Layered { build_up: self.ext };
        self.add(
            ground,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge,
                reference: ReferenceLine::Centre,
                structure: s,
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        )
    }

    pub fn opening(
        &mut self,
        host: Id,
        kind: OpeningKind,
        offset: f64,
        width: f64,
        sill: f64,
        height: f64,
    ) -> Id {
        let ground = self.ground;
        self.add(
            ground,
            ElementKind::Opening(Opening {
                host,
                kind,
                style: "test".into(),
                offset: Length::m(offset),
                width: Length::m(width),
                height: Length::m(height),
                sill: Length::m(sill),
                flip_side: false,
                flip_hand: false,
                params: Default::default(),
            }),
        )
    }

    pub fn column(&mut self, at: (f64, f64), size: f64) -> Id {
        let ground = self.ground;
        let material = self.concrete;
        self.add(
            ground,
            ElementKind::Column(Column {
                position: Point2::new(at.0, at.1),
                rotation: Angle::ZERO,
                profile: Profile::Rectangle { width: Length::m(size), depth: Length::m(size) },
                material,
                height: Length::m(3.0),
                base_offset: Length::ZERO,
            }),
        )
    }

    pub fn roof(&mut self, outline: &[(f64, f64)], slopes: Vec<Option<f64>>, base: f64) -> Id {
        let ground = self.ground;
        let structure = Structure::Layered { build_up: self.roof_build_up };
        self.add(
            ground,
            ElementKind::Roof(Roof {
                outline: Polygon2::from_m(outline),
                base: Length::m(base),
                structure,
                holes: Vec::new(),
                shape: RoofShape::Pitched { slopes: slopes.into_iter().map(|s| s.map(Angle::deg)).collect() },
            }),
        )
    }

    pub fn slab(&mut self, outline: &[(f64, f64)], top: f64) -> Id {
        let ground = self.ground;
        let structure = Structure::Layered { build_up: self.slab_build_up };
        self.add(
            ground,
            ElementKind::Slab(Slab {
                outline: PolygonWithHoles::from(Polygon2::from_m(outline)),
                structure,
                top_offset: Length::m(top),
            }),
        )
    }

    pub fn stair(&mut self, shape: StairShape, risers: u32, rise: f64, going: f64, width: f64) -> Id {
        let ground = self.ground;
        let material = self.concrete;
        self.add(
            ground,
            ElementKind::Stair(Stair {
                origin: Point2::new(0.0, 0.0),
                direction: Angle::ZERO,
                shape,
                width: Length::m(width),
                rise: Length::m(rise),
                risers,
                going: Length::m(going),
                first_flight_risers: None,
                gap: Length::m(0.1),
                thickness: Length::m(0.15),
                material,
                usage: Default::default(),
                base_offset: Length::ZERO,
            }),
        )
    }

    pub fn railing(&mut self, path: &[(f64, f64)], height: f64, infill: RailingInfill) -> Id {
        let ground = self.ground;
        let material = self.brick;
        self.add(
            ground,
            ElementKind::Railing(kanavos_model::Railing {
                path: path.iter().map(|&(x, y)| Point2::new(x, y)).collect(),
                height: Length::m(height),
                base_offset: Length::ZERO,
                post_spacing: Length::m(1.2),
                infill,
                elevations: Vec::new(),
                material,
                infill_material: None,
            }),
        )
    }
}

pub fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

pub fn area(polys: &[PolygonWithHoles]) -> f64 {
    polys.iter().map(|p| p.area().as_m2()).sum()
}
