//! Project ↔ national grid (ΕΓΣΑ87, EPSG:2100) coordinates for the plot coordinate table.
//!
//! **Local stand-in** for the conversion that `kanavos-geo` will offer: a rigid transform (rotation about the
//! project origin, then translation), exact for the planar grid. Grid convergence between grid north and true
//! north is not applied; the north arrow therefore shows grid north.

use kanavos_geom::{Point2, Vec2};
use kanavos_model::Georef;

/// Grid coordinates (easting, northing) of a project point, metres.
///
/// `georef.rotation` is the angle from project +Y to grid north, counter-clockwise positive, so a project vector
/// is rotated by −rotation to get grid components.
pub fn to_grid(georef: &Georef, p: Point2) -> (f64, f64) {
    let (s, c) = georef.rotation.as_rad().sin_cos();
    let [x, y] = p.xy();
    (georef.easting + x * c + y * s, georef.northing - x * s + y * c)
}

/// Unit vector of grid north in project coordinates.
pub fn grid_north(georef: &Georef) -> Vec2 {
    let r = georef.rotation.as_rad();
    Vec2::new(-r.sin(), r.cos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_units::Angle;

    #[test]
    fn grid_north_maps_to_plus_northing() {
        let g = Georef {
            epsg: 2100,
            easting: 476_532.12,
            northing: 4_097_345.88,
            rotation: Angle::deg(30.0),
            altitude: 0.0,
        };
        let n = grid_north(&g);
        let (e, nn) = to_grid(&g, Point2::new(n.x * 10.0, n.y * 10.0));
        assert!((e - 476_532.12).abs() < 1e-6);
        assert!((nn - 4_097_355.88).abs() < 1e-6);
        let (e0, n0) = to_grid(&g, Point2::ORIGIN);
        assert!((e0 - 476_532.12).abs() < 1e-9 && (n0 - 4_097_345.88).abs() < 1e-9);
    }
}
