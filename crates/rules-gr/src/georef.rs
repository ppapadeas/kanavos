//! Project ↔ national grid (ΕΓΣΑ87, EPSG:2100) coordinates for the plot and building coordinate tables, through
//! `kanavos-geo`.
//!
//! Grid convergence between grid north and true north is not applied; the north arrow shows grid north.

use kanavos_geo::GridPlacement;
use kanavos_geom::{Point2, Vec2};
use kanavos_model::Georef;

fn placement(georef: &Georef) -> GridPlacement {
    GridPlacement {
        easting: georef.easting,
        northing: georef.northing,
        rotation: georef.rotation.as_rad(),
        altitude: georef.altitude,
    }
}

/// Grid coordinates (easting, northing) of a project point, metres.
///
/// `georef.rotation` is the angle from project +Y to grid north, counter-clockwise positive, so a project vector
/// is rotated by −rotation to get grid components.
pub fn to_grid(georef: &Georef, p: Point2) -> (f64, f64) {
    let [x, y] = p.xy();
    let [e, n] = placement(georef).project_to_grid(x, y);
    (e, n)
}

/// Unit vector of grid north in project coordinates.
pub fn grid_north(georef: &Georef) -> Vec2 {
    let [x, y] = placement(georef).grid_north();
    Vec2::new(x, y)
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
