//! The sample house shown on first launch (docs/ux.md §5): a two-storey house on a plot, with spaces,
//! openings, furniture, a roof, sections, elevations and the permit sheet set.
//!
//! The house is `kanavos_engine::sample::sample_house()`, the same one the command-line tools and tests use. The
//! smaller house built here with ordinary engine commands is a fallback, used only if the engine's sample cannot
//! be built (which would be a bug; the engine's tests guard against it).

use std::collections::BTreeMap;

use kanavos_engine::commands::{
    CreateElement, CreateView, DetectSpace, PermitSet, SetProjectInfo, SetSite, UpdateStorey,
};
use kanavos_engine::{AnyCommand, Session};
use kanavos_geom::{Angle, Point2, Polygon2, PolygonWithHoles};
use kanavos_model::{
    Dimension, Document, ElementKind, Id, Object, Opening, OpeningKind, ProjectInfo, ReferenceLine, Roof,
    RoofShape, Site, Slab, Space, SpaceCategory, Stair, StairShape, Structure, ViewKind, Wall,
};
use kanavos_units::{Length, Locale};

struct Builder {
    session: Session,
    failures: usize,
}

impl Builder {
    fn run(&mut self, c: impl Into<AnyCommand>) -> Vec<Id> {
        match self.session.run(c) {
            Ok(out) => out.created,
            Err(_) => {
                self.failures += 1;
                Vec::new()
            }
        }
    }

    fn doc(&self) -> &Document {
        self.session.document()
    }

    fn element(&mut self, storey: Id, kind: ElementKind) -> Option<Id> {
        self.run(CreateElement::on(storey, kind)).first().copied()
    }

    fn wall(&mut self, storey: Id, a: (f64, f64), b: (f64, f64), s: &Structure, h: f64) -> Option<Id> {
        self.element(
            storey,
            ElementKind::Wall(Wall {
                start: Point2::new(a.0, a.1),
                end: Point2::new(b.0, b.1),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: s.clone(),
                height: Length::m(h),
                base_offset: Length::ZERO,
            }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn opening(
        &mut self,
        storey: Id,
        host: Option<Id>,
        kind: OpeningKind,
        style: &str,
        offset: f64,
        size: (f64, f64),
        sill: f64,
        tag: &str,
    ) {
        let Some(host) = host else { return };
        let mut c = CreateElement::on(
            storey,
            ElementKind::Opening(Opening {
                host,
                kind,
                style: style.into(),
                offset: Length::m(offset),
                width: Length::m(size.0),
                height: Length::m(size.1),
                sill: Length::m(sill),
                flip_side: false,
                flip_hand: false,
                params: BTreeMap::new(),
            }),
        );
        c.tag = tag.into();
        self.run(c);
    }

    /// A space inside the inner faces `(x0, y0, x1, y1)`: detected from the walls when the engine can,
    /// otherwise drawn from the given faces.
    fn space(
        &mut self,
        storey: Id,
        faces: (f64, f64, f64, f64),
        name: &str,
        number: &str,
        cat: SpaceCategory,
    ) {
        let (x0, y0, x1, y1) = faces;
        let centre = Point2::new((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let detect =
            DetectSpace { storey, point: centre, name: name.into(), number: number.into(), category: cat };
        if self.session.run(detect).is_ok() {
            return;
        }
        self.element(
            storey,
            ElementKind::Space(Space {
                boundary: PolygonWithHoles::from(rect(x0, y0, x1, y1)),
                name: name.into(),
                number: number.into(),
                category: cat,
                height: None,
                stamp: Some(centre),
                finish: String::new(),
            }),
        );
    }

    fn object(&mut self, storey: Id, definition: &str, at: (f64, f64), degrees: f64) {
        self.element(
            storey,
            ElementKind::Object(Object {
                definition: definition.into(),
                position: Point2::new(at.0, at.1),
                rotation: Angle::deg(degrees),
                mirrored: false,
                elevation: Length::ZERO,
                params: BTreeMap::new(),
            }),
        );
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon2 {
    Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// Builds the sample house. Steps that the engine rejects are skipped, so the result is always a valid
/// document; [`sample_failures`] reports how many were skipped (zero in a healthy build).
pub fn sample_house(locale: Locale) -> Document {
    kanavos_engine::sample::sample_house().unwrap_or_else(|_| build(locale).0)
}

/// Number of construction steps the engine rejected while building the sample (for tests and diagnostics).
pub fn sample_failures(locale: Locale) -> usize {
    build(locale).1
}

fn build(locale: Locale) -> (Document, usize) {
    let mut b = Builder { session: Session::new(kanavos_model::starter()), failures: 0 };
    let storeys: Vec<Id> = b.doc().storeys().iter().map(|s| s.id).collect();
    let (Some(&ground), Some(&first)) = (storeys.first(), storeys.get(1)) else {
        return (b.doc().clone(), 1);
    };
    let defaults = b.doc().defaults().clone();
    let concrete = defaults.structural_material.unwrap_or_default();
    let layered = |id: Option<Id>| id.map(|build_up| Structure::Layered { build_up });
    let (Some(ext), Some(int)) = (layered(defaults.exterior_wall), layered(defaults.interior_wall)) else {
        return (b.doc().clone(), 1);
    };

    b.run(SetProjectInfo {
        info: ProjectInfo {
            name: "Διώροφη κατοικία (δείγμα)".into(),
            location: "Οδός Αρχιτεκτόνων 12".into(),
            municipality: "Καλαμάτας".into(),
            owner: "Ιδιοκτήτης δείγματος".into(),
            designer: "Γραφείο μελετών (δείγμα)".into(),
            designer_details: "Αρ. μητρώου ΤΕΕ —".into(),
            date: "Οκτώβριος 2026".into(),
            revision: "0".into(),
            notes: String::new(),
        },
    });
    let mut site = Site { plot: Some(rect(-4.0, -6.0, 17.0, 20.0)), ..Site::default() };
    site.georef.easting = 476_532.12;
    site.georef.northing = 4_097_345.88;
    site.georef.altitude = 12.0;
    b.run(SetSite { site });
    b.run(UpdateStorey { id: first, height: Some(Length::m(3.0)), ..UpdateStorey::default() });

    // Ground floor.
    let h0 = 3.2;
    let south = b.wall(ground, (0.0, 0.0), (12.0, 0.0), &ext, h0);
    let east = b.wall(ground, (12.0, 0.0), (12.0, 9.0), &ext, h0);
    let north = b.wall(ground, (12.0, 9.0), (0.0, 9.0), &ext, h0);
    let west = b.wall(ground, (0.0, 9.0), (0.0, 0.0), &ext, h0);
    let spine = b.wall(ground, (5.0, 0.0), (5.0, 9.0), &int, h0);
    let wc_wall = b.wall(ground, (0.0, 3.0), (5.0, 3.0), &int, h0);
    let study_wall = b.wall(ground, (0.0, 6.0), (5.0, 6.0), &int, h0);
    use OpeningKind::{Door, Window};
    b.opening(ground, west, Door, "door.single_swing", 4.5, (1.0, 2.2), 0.0, "Π1");
    b.opening(ground, spine, Door, "door.double_swing", 4.5, (1.4, 2.1), 0.0, "Π2");
    b.opening(ground, wc_wall, Door, "door.single_swing", 3.6, (0.8, 2.1), 0.0, "Π3");
    b.opening(ground, study_wall, Door, "door.single_swing", 3.6, (0.9, 2.1), 0.0, "Π4");
    b.opening(ground, south, Window, "window.sliding", 7.0, (1.8, 1.4), 0.9, "Α1");
    b.opening(ground, south, Window, "window.sliding", 10.2, (1.8, 1.4), 0.9, "Α2");
    b.opening(ground, south, Window, "window.casement_1", 2.5, (0.8, 0.8), 1.4, "Α3");
    b.opening(ground, east, Window, "window.casement_2", 4.5, (1.2, 1.4), 0.9, "Α4");
    b.opening(ground, north, Window, "window.casement_2", 3.0, (1.2, 1.4), 0.9, "Α5");
    b.opening(ground, north, Window, "window.casement_2", 9.5, (1.2, 1.4), 0.9, "Α6");
    let slab0 = PolygonWithHoles::from(rect(-0.155, -0.155, 12.155, 9.155));
    if let Some(ground_slab) = layered(defaults.ground_slab) {
        b.element(
            ground,
            ElementKind::Slab(Slab { outline: slab0, structure: ground_slab, top_offset: Length::ZERO }),
        );
    }
    b.element(
        ground,
        ElementKind::Stair(Stair {
            origin: Point2::new(1.0, 3.62),
            direction: Angle::ZERO,
            shape: StairShape::HalfTurnLeft,
            width: Length::m(1.0),
            rise: Length::m(h0),
            risers: 18,
            going: Length::cm(26.0),
            first_flight_risers: None,
            gap: Length::cm(10.0),
            thickness: Length::cm(15.0),
            material: concrete,
            base_offset: Length::ZERO,
        }),
    );
    // Inner faces: exterior walls are 31 cm and interior walls 13 cm, both centred on their lines.
    let (o, i) = (0.155, 0.065);
    b.space(ground, (5.0 + i, o, 12.0 - o, 9.0 - o), "Καθιστικό – κουζίνα", "0.01", SpaceCategory::MainUse);
    b.space(ground, (o, o, 5.0 - i, 3.0 - i), "WC", "0.02", SpaceCategory::MainUse);
    b.space(ground, (o, 3.0 + i, 5.0 - i, 6.0 - i), "Χώλ – κλιμακοστάσιο", "0.03", SpaceCategory::Stairwell);
    b.space(ground, (o, 6.0 + i, 5.0 - i, 9.0 - o), "Γραφείο", "0.04", SpaceCategory::MainUse);
    b.object(ground, "furniture.sofa", (7.4, 1.4), 0.0);
    b.object(ground, "furniture.coffee_table", (7.4, 2.8), 0.0);
    b.object(ground, "furniture.armchair", (9.6, 2.2), 90.0);
    b.object(ground, "furniture.table", (8.6, 6.0), 0.0);
    b.object(ground, "kitchen.counter", (10.5, 8.5), 180.0);
    b.object(ground, "kitchen.fridge", (11.4, 7.0), 90.0);
    b.object(ground, "sanitary.wc", (0.6, 0.6), 0.0);
    b.object(ground, "sanitary.basin", (1.6, 2.6), 180.0);
    b.object(ground, "furniture.desk", (2.5, 8.4), 180.0);
    b.object(ground, "furniture.office_chair", (2.5, 7.6), 0.0);
    b.run(CreateElement::on(
        ground,
        ElementKind::Dimension(Dimension {
            points: vec![Point2::new(-0.155, -0.155), Point2::new(5.0, -0.155), Point2::new(12.155, -0.155)],
            line_point: Point2::new(0.0, -1.6),
            direction: Angle::ZERO,
        }),
    ));

    // First floor.
    let h1 = 3.0;
    let south1 = b.wall(first, (0.0, 0.0), (12.0, 0.0), &ext, h1);
    let east1 = b.wall(first, (12.0, 0.0), (12.0, 9.0), &ext, h1);
    let north1 = b.wall(first, (12.0, 9.0), (0.0, 9.0), &ext, h1);
    let west1 = b.wall(first, (0.0, 9.0), (0.0, 0.0), &ext, h1);
    let spine1 = b.wall(first, (5.0, 0.0), (5.0, 9.0), &int, h1);
    let bath1 = b.wall(first, (0.0, 3.0), (5.0, 3.0), &int, h1);
    let bed3 = b.wall(first, (0.0, 6.0), (5.0, 6.0), &int, h1);
    b.wall(first, (5.0, 4.5), (12.0, 4.5), &int, h1);
    b.opening(first, spine1, Door, "door.single_swing", 3.8, (0.9, 2.1), 0.0, "Π5");
    b.opening(first, spine1, Door, "door.single_swing", 5.25, (0.9, 2.1), 0.0, "Π6");
    b.opening(first, bath1, Door, "door.single_swing", 3.6, (0.8, 2.1), 0.0, "Π7");
    b.opening(first, bed3, Door, "door.single_swing", 3.6, (0.9, 2.1), 0.0, "Π8");
    b.opening(first, south1, Window, "window.casement_2", 8.5, (1.4, 1.4), 0.9, "Α7");
    b.opening(first, south1, Window, "window.casement_1", 2.5, (0.8, 0.8), 1.4, "Α8");
    b.opening(first, east1, Window, "window.casement_2", 2.25, (1.2, 1.4), 0.9, "Α9");
    b.opening(first, east1, Window, "window.casement_2", 6.75, (1.2, 1.4), 0.9, "Α10");
    b.opening(first, north1, Window, "window.casement_2", 3.0, (1.4, 1.4), 0.9, "Α11");
    b.opening(first, north1, Window, "window.casement_2", 9.5, (1.2, 1.4), 0.9, "Α12");
    b.opening(first, west1, Window, "window.casement_1", 4.5, (0.8, 1.2), 1.0, "Α13");
    if let Some(floor) = layered(defaults.floor_slab) {
        let mut outline = PolygonWithHoles::from(rect(-0.155, -0.155, 12.155, 9.155));
        outline.holes.push(rect(0.95, 3.55, 3.35, 5.75));
        b.element(first, ElementKind::Slab(Slab { outline, structure: floor, top_offset: Length::ZERO }));
    }
    b.space(first, (5.0 + i, o, 12.0 - o, 4.5 - i), "Υπνοδωμάτιο 1", "1.01", SpaceCategory::MainUse);
    b.space(first, (5.0 + i, 4.5 + i, 12.0 - o, 9.0 - o), "Υπνοδωμάτιο 2", "1.02", SpaceCategory::MainUse);
    b.space(first, (o, 6.0 + i, 5.0 - i, 9.0 - o), "Υπνοδωμάτιο 3", "1.03", SpaceCategory::MainUse);
    b.space(first, (o, o, 5.0 - i, 3.0 - i), "Λουτρό", "1.04", SpaceCategory::MainUse);
    b.space(first, (o, 3.0 + i, 5.0 - i, 6.0 - i), "Διάδρομος", "1.05", SpaceCategory::Stairwell);
    b.object(first, "furniture.bed_double", (8.5, 2.4), 0.0);
    b.object(first, "furniture.bed_single", (7.0, 7.4), 0.0);
    b.object(first, "furniture.bed_single", (2.0, 7.6), 90.0);
    b.object(first, "furniture.wardrobe", (11.3, 2.4), 90.0);
    b.object(first, "sanitary.bath", (1.2, 0.6), 0.0);
    b.object(first, "sanitary.basin", (3.6, 2.6), 180.0);
    if let Some(roof) = layered(defaults.pitched_roof) {
        b.element(
            first,
            ElementKind::Roof(Roof {
                outline: rect(-0.6, -0.6, 12.6, 9.6),
                base: Length::m(h1),
                structure: roof,
                holes: Vec::new(),
                shape: RoofShape::Pitched { slopes: vec![Some(Angle::deg(22.0)); 4] },
            }),
        );
    }

    // Sections and elevations.
    let depth = Length::m(25.0);
    let views = [
        ("Τομή Α-Α", true, (6.2, -4.0), (6.2, 13.0), "Α"),
        ("Τομή Β-Β", true, (-4.0, 4.6), (16.0, 4.6), "Β"),
        ("Νότια όψη", false, (-3.0, -5.0), (15.0, -5.0), "1"),
        ("Ανατολική όψη", false, (16.0, -3.0), (16.0, 12.0), "2"),
        ("Βόρεια όψη", false, (15.0, 14.0), (-3.0, 14.0), "3"),
        ("Δυτική όψη", false, (-3.0, 12.0), (-3.0, -3.0), "4"),
    ];
    for (name, section, a, z, marker) in views {
        let (start, end) = (Point2::new(a.0, a.1), Point2::new(z.0, z.1));
        let kind = if section {
            ViewKind::Section { start, end, depth, flip: false, marker: marker.into() }
        } else {
            ViewKind::Elevation { start, end, depth, flip: false, marker: marker.into() }
        };
        b.run(CreateView { name: name.into(), kind, scale: 100 });
    }
    b.run(PermitSet { locale });
    let failures = b.failures;
    (b.doc().clone(), failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::Category;

    #[test]
    fn the_sample_house_builds_without_rejected_steps() {
        assert_eq!(sample_failures(Locale::El), 0);
    }

    #[test]
    fn the_sample_house_has_the_parts_people_look_for() {
        let doc = sample_house(Locale::El);
        doc.validate().unwrap();
        let count = |c: Category| doc.elements_of(c).count();
        assert!(count(Category::Wall) >= 15);
        assert!(count(Category::Door) >= 8);
        assert!(count(Category::Window) >= 10);
        assert!(count(Category::Space) >= 9);
        assert_eq!(count(Category::Roof), 1);
        assert!(!doc.sheets().is_empty());
        assert!(doc.site().plot.is_some());
    }
}
