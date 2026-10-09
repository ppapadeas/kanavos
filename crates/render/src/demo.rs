//! A procedurally built sample house for examples, tests and first-run screenshots.
//!
//! One storey, 10 × 7 m, rendered plaster walls with window and door openings, a concrete floor slab, a gable
//! roof, framed windows with glass, a door, an interior partition, and a pergola with round columns over a
//! paved terrace, standing on a site plane. It sits at projected coordinates typical of ΕΓΣΑ87 so that the
//! large-coordinate path (scene origin) is exercised.

use kanavos_drawing::Rgba;
use kanavos_geom::{Point2, Point3, Polygon2, PolygonWithHoles, Vec3};
use kanavos_solid::{Mesh, cuboid, cylinder, extrude, extrude_between};

use crate::scene::Scene;

/// World position of the house's south-west corner (and the scene origin).
pub const HOUSE_ORIGIN: Point3 = Point3::new(360_250.0, 4_099_870.0, 12.0);

/// Object ids in the sample house.
pub mod ids {
    /// Site plane.
    pub const SITE: u64 = 1;
    /// Ground floor slab.
    pub const SLAB: u64 = 10;
    /// South wall (door and two windows).
    pub const SOUTH_WALL: u64 = 20;
    /// North wall (one window).
    pub const NORTH_WALL: u64 = 21;
    /// West wall with gable (one window).
    pub const WEST_WALL: u64 = 22;
    /// East wall with gable.
    pub const EAST_WALL: u64 = 23;
    /// Interior partition with a doorway.
    pub const PARTITION: u64 = 24;
    /// South roof pitch.
    pub const ROOF_SOUTH: u64 = 30;
    /// North roof pitch.
    pub const ROOF_NORTH: u64 = 31;
    /// South-west window (frame and glass).
    pub const WINDOW_SOUTH_WEST: u64 = 40;
    /// South-east window.
    pub const WINDOW_SOUTH_EAST: u64 = 41;
    /// North window.
    pub const WINDOW_NORTH: u64 = 42;
    /// West window.
    pub const WINDOW_WEST: u64 = 43;
    /// Entrance door.
    pub const DOOR: u64 = 50;
    /// Terrace paving.
    pub const TERRACE: u64 = 60;
    /// Pergola column, west.
    pub const COLUMN_WEST: u64 = 61;
    /// Pergola column, east.
    pub const COLUMN_EAST: u64 = 62;
    /// Pergola beams.
    pub const PERGOLA: u64 = 63;
}

/// Categories used in the sample house.
pub mod categories {
    /// Site and terrain.
    pub const SITE: u32 = 0;
    /// Slabs and paving.
    pub const SLAB: u32 = 1;
    /// Walls.
    pub const WALL: u32 = 2;
    /// Roofs.
    pub const ROOF: u32 = 3;
    /// Doors and windows.
    pub const OPENING: u32 = 4;
    /// Columns and beams.
    pub const STRUCTURE: u32 = 5;
}

const PLASTER: Rgba = Rgba::rgb(236, 231, 222);
const CONCRETE: Rgba = Rgba::rgb(184, 180, 172);
const ROOF: Rgba = Rgba::rgb(170, 104, 82);
const FRAME: Rgba = Rgba::rgb(74, 78, 84);
const GLASS: Rgba = Rgba::new(150, 192, 208, 80);
const TIMBER: Rgba = Rgba::rgb(150, 110, 76);
const PAVING: Rgba = Rgba::rgb(204, 196, 182);
const SITE_GREEN: Rgba = Rgba::rgb(196, 204, 182);

/// Wall height at the eaves, metres.
pub const WALL_HEIGHT: f64 = 3.0;
const THICKNESS: f64 = 0.25;
const LENGTH: f64 = 10.0;
const DEPTH: f64 = 7.0;
const PITCH_DEG: f64 = 30.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon2 {
    Polygon2::from_m(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// Extrudes a profile drawn in a vertical plane. Profile coordinates are (u, h): `u` runs along `along`, `h` is
/// height above `base`. The extrusion runs `thickness` metres along `across`.
fn vertical(profile: &PolygonWithHoles, base: Point3, along: Vec3, across: Vec3, thickness: f64) -> Mesh {
    let Ok(m) = extrude(profile, 0.0, thickness) else { return Mesh::new() };
    let mapped = m.map_positions(|p| base + along * p.x + Vec3::Z * p.y + across * p.z);
    // The map (u, h, t) → (along, Z, across) mirrors when its determinant is negative.
    if along.dot(Vec3::Z.cross(across)) < 0.0 { mapped.flipped() } else { mapped }
}

fn hole(u0: f64, h0: f64, u1: f64, h1: f64) -> Polygon2 {
    rect(u0, h0, u1, h1)
}

fn ridge_height() -> f64 {
    WALL_HEIGHT + 0.5 * DEPTH * PITCH_DEG.to_radians().tan()
}

/// Gable line height above floor at plan position `y`.
fn gable(y: f64) -> f64 {
    WALL_HEIGHT + (0.5 * DEPTH - (y - 0.5 * DEPTH).abs()) * PITCH_DEG.to_radians().tan()
}

/// A window (frame ring and glass) in a wall plane.
fn window(base: Point3, along: Vec3, across: Vec3, u0: f64, h0: f64, u1: f64, h1: f64) -> Vec<(Mesh, Rgba)> {
    let f = 0.06;
    let ring =
        PolygonWithHoles { outer: rect(u0, h0, u1, h1), holes: vec![rect(u0 + f, h0 + f, u1 - f, h1 - f)] };
    let frame = vertical(&ring, base + across * 0.06, along, across, 0.08);
    let glass = vertical(
        &PolygonWithHoles::from(rect(u0 + f, h0 + f, u1 - f, h1 - f)),
        base + across * 0.09,
        along,
        across,
        0.02,
    );
    vec![(frame, FRAME), (glass, GLASS)]
}

/// Builds the sample house scene.
pub fn sample_house() -> Scene {
    let o = HOUSE_ORIGIN;
    let at = |x: f64, y: f64, z: f64| Point3::new(o.x + x, o.y + y, o.z + z);
    let mut scene = Scene::new(o);
    let mut add =
        |id: u64, mesh: &Mesh, color: Rgba, category: u32| scene.push_solid(id, mesh, color, category);

    // Site plane: an open surface (single-sided terrain).
    let mut site = Mesh::new();
    site.push_quad(
        at(-12.0, -10.0, -0.3),
        at(22.0, -10.0, -0.3),
        at(22.0, 16.0, -0.3),
        at(-12.0, 16.0, -0.3),
    );
    add(ids::SITE, &site, SITE_GREEN, categories::SITE);

    add(
        ids::SLAB,
        &cuboid(at(-0.2, -0.2, -0.3), at(LENGTH + 0.2, DEPTH + 0.2, 0.0)),
        CONCRETE,
        categories::SLAB,
    );

    // South wall: door notch and two window holes. Along +X, thickness towards +Y.
    let south = PolygonWithHoles {
        outer: Polygon2::from_m(&[
            (0.0, 0.0),
            (4.2, 0.0),
            (4.2, 2.2),
            (5.2, 2.2),
            (5.2, 0.0),
            (LENGTH, 0.0),
            (LENGTH, WALL_HEIGHT),
            (0.0, WALL_HEIGHT),
        ]),
        holes: vec![hole(1.4, 0.9, 3.0, 2.2), hole(6.6, 0.9, 8.6, 2.2)],
    };
    add(
        ids::SOUTH_WALL,
        &vertical(&south, at(0.0, 0.0, 0.0), Vec3::X, Vec3::Y, THICKNESS),
        PLASTER,
        categories::WALL,
    );

    // North wall: one window.
    let north = PolygonWithHoles {
        outer: rect(0.0, 0.0, LENGTH, WALL_HEIGHT),
        holes: vec![hole(3.5, 1.0, 6.5, 2.2)],
    };
    add(
        ids::NORTH_WALL,
        &vertical(&north, at(0.0, DEPTH - THICKNESS, 0.0), Vec3::X, Vec3::Y, THICKNESS),
        PLASTER,
        categories::WALL,
    );

    // Gable walls between the long walls, along +Y, thickness towards +X.
    let (y0, y1) = (THICKNESS, DEPTH - THICKNESS);
    let gable_outer = Polygon2::from_m(&[
        (y0, 0.0),
        (y1, 0.0),
        (y1, gable(y1)),
        (0.5 * DEPTH, ridge_height()),
        (y0, gable(y0)),
    ]);
    let west = PolygonWithHoles { outer: gable_outer.clone(), holes: vec![hole(2.5, 1.0, 4.5, 2.2)] };
    add(
        ids::WEST_WALL,
        &vertical(&west, at(0.0, 0.0, 0.0), Vec3::Y, Vec3::X, THICKNESS),
        PLASTER,
        categories::WALL,
    );
    let east = PolygonWithHoles::from(gable_outer);
    add(
        ids::EAST_WALL,
        &vertical(&east, at(LENGTH - THICKNESS, 0.0, 0.0), Vec3::Y, Vec3::X, THICKNESS),
        PLASTER,
        categories::WALL,
    );

    // Partition with a doorway.
    let partition = PolygonWithHoles {
        outer: Polygon2::from_m(&[
            (y0, 0.0),
            (3.0, 0.0),
            (3.0, 2.1),
            (3.9, 2.1),
            (3.9, 0.0),
            (y1, 0.0),
            (y1, WALL_HEIGHT),
            (y0, WALL_HEIGHT),
        ]),
        holes: vec![],
    };
    add(
        ids::PARTITION,
        &vertical(&partition, at(5.6, 0.0, 0.0), Vec3::Y, Vec3::X, 0.12),
        PLASTER,
        categories::WALL,
    );

    // Roof pitches: slanted slabs with eaves and verge overhangs.
    let tan = PITCH_DEG.to_radians().tan();
    let thick = 0.2 / PITCH_DEG.to_radians().cos();
    let over = 0.45;
    let south_roof = PolygonWithHoles::from(Polygon2::from_m(&[
        (o.x - over, o.y - over),
        (o.x + LENGTH + over, o.y - over),
        (o.x + LENGTH + over, o.y + 0.5 * DEPTH),
        (o.x - over, o.y + 0.5 * DEPTH),
    ]));
    let north_roof = PolygonWithHoles::from(Polygon2::from_m(&[
        (o.x - over, o.y + 0.5 * DEPTH),
        (o.x + LENGTH + over, o.y + 0.5 * DEPTH),
        (o.x + LENGTH + over, o.y + DEPTH + over),
        (o.x - over, o.y + DEPTH + over),
    ]));
    let south_bottom = move |p: Point2| o.z + WALL_HEIGHT + (p.xy()[1] - o.y) * tan;
    let north_bottom = move |p: Point2| o.z + WALL_HEIGHT + (o.y + DEPTH - p.xy()[1]) * tan;
    if let Ok(m) = extrude_between(&south_roof, south_bottom, move |p| south_bottom(p) + thick) {
        add(ids::ROOF_SOUTH, &m, ROOF, categories::ROOF);
    }
    if let Ok(m) = extrude_between(&north_roof, north_bottom, move |p| north_bottom(p) + thick) {
        add(ids::ROOF_NORTH, &m, ROOF, categories::ROOF);
    }

    // Windows: frame and glass share the window's id.
    let windows = [
        (ids::WINDOW_SOUTH_WEST, at(0.0, 0.0, 0.0), Vec3::X, Vec3::Y, (1.4, 0.9, 3.0, 2.2)),
        (ids::WINDOW_SOUTH_EAST, at(0.0, 0.0, 0.0), Vec3::X, Vec3::Y, (6.6, 0.9, 8.6, 2.2)),
        (ids::WINDOW_NORTH, at(0.0, DEPTH - THICKNESS, 0.0), Vec3::X, Vec3::Y, (3.5, 1.0, 6.5, 2.2)),
        (ids::WINDOW_WEST, at(0.0, 0.0, 0.0), Vec3::Y, Vec3::X, (2.5, 1.0, 4.5, 2.2)),
    ];
    for (id, base, along, across, (u0, h0, u1, h1)) in windows {
        for (mesh, color) in window(base, along, across, u0, h0, u1, h1) {
            add(id, &mesh, color, categories::OPENING);
        }
    }

    // Door leaf, set back in the opening.
    add(ids::DOOR, &cuboid(at(4.22, 0.14, 0.0), at(5.18, 0.18, 2.18)), TIMBER, categories::OPENING);

    // Terrace with a pergola on two round columns.
    add(ids::TERRACE, &cuboid(at(2.6, -3.0, -0.3), at(7.4, -0.2, -0.15)), PAVING, categories::SLAB);
    let column = |x: f64, y: f64| {
        cylinder(Point2::new(o.x + x, o.y + y), 0.14, o.z - 0.15, o.z + 2.6, 24).unwrap_or_default()
    };
    add(ids::COLUMN_WEST, &column(3.0, -2.6), TIMBER, categories::STRUCTURE);
    add(ids::COLUMN_EAST, &column(7.0, -2.6), TIMBER, categories::STRUCTURE);
    let mut pergola = cuboid(at(2.6, -2.75, 2.6), at(7.4, -2.45, 2.85));
    for x in [3.0, 4.35, 5.65, 7.0] {
        pergola.append(&cuboid(at(x - 0.06, -2.95, 2.85), at(x + 0.06, 0.0, 3.05)));
    }
    add(ids::PERGOLA, &pergola, TIMBER, categories::STRUCTURE);

    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_house_is_complete() {
        let s = sample_house();
        let has = |id| s.meshes.iter().any(|m| m.id == id && m.triangle_count() > 0);
        for id in [
            ids::SITE,
            ids::SLAB,
            ids::SOUTH_WALL,
            ids::NORTH_WALL,
            ids::WEST_WALL,
            ids::EAST_WALL,
            ids::PARTITION,
            ids::ROOF_SOUTH,
            ids::ROOF_NORTH,
            ids::WINDOW_SOUTH_WEST,
            ids::DOOR,
            ids::COLUMN_WEST,
            ids::PERGOLA,
        ] {
            assert!(has(id), "missing {id}");
        }
        let glass = s.meshes.iter().filter(|m| m.is_transparent()).count();
        assert_eq!(glass, 4);
        let b = s.bounds().unwrap();
        assert!((b.max.z - (HOUSE_ORIGIN.z + ridge_height())).abs() < 0.5);
        // Relative coordinates stay small.
        assert!(s.meshes.iter().all(|m| m.positions.iter().all(|p| p.iter().all(|c| c.abs() < 100.0))));
    }

    #[test]
    fn walls_are_closed_solids_with_positive_volume() {
        // Near the origin: the divergence-theorem volume loses precision at large coordinates.
        let o = Point3::new(1.0, 2.0, 0.0);
        let south = PolygonWithHoles { outer: rect(0.0, 0.0, 2.0, 3.0), holes: vec![] };
        let m = vertical(&south, o, Vec3::X, Vec3::Y, 0.25);
        assert!((m.volume() - 1.5).abs() < 1e-6, "outward orientation, volume {}", m.volume());
        let m = vertical(&south, o, Vec3::Y, Vec3::X, 0.25);
        assert!((m.volume() - 1.5).abs() < 1e-6, "mirrored map is flipped back, volume {}", m.volume());
    }
}
