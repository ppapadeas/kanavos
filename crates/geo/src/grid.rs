//! Project coordinates ↔ grid coordinates.
//!
//! A project is modelled in a local frame (metres, X right, Y up in plan) near the origin, which keeps numbers small
//! and the model independent of where the building is. Its georeference places that frame on the national grid:
//! the grid coordinates of the project origin, a rotation, and the absolute altitude of the project's ±0.00.
//!
//! **Rotation convention** (the one `kanavos-model` documents for its georeference): `rotation` is the angle from
//! project +Y to grid north, counter-clockwise positive. Grid north, seen in project coordinates, points along
//! project +Y turned counter-clockwise by `rotation`. Hence
//!
//! ```text
//! grid    = origin + R(−rotation) · project
//! project = R(rotation) · (grid − origin)
//! ```

/// Placement of a project's local frame on a grid.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GridPlacement {
    /// Grid easting of the project origin, metres.
    pub easting: f64,
    /// Grid northing of the project origin, metres.
    pub northing: f64,
    /// Angle from project +Y to grid north, radians, counter-clockwise positive.
    pub rotation: f64,
    /// Absolute altitude of the project's ±0.00, metres.
    pub altitude: f64,
}

impl GridPlacement {
    /// A placement with no rotation.
    pub fn at(easting: f64, northing: f64, altitude: f64) -> GridPlacement {
        GridPlacement { easting, northing, rotation: 0.0, altitude }
    }

    /// Project plan coordinates (metres) to grid easting and northing.
    pub fn project_to_grid(&self, x: f64, y: f64) -> [f64; 2] {
        let (s, c) = (-self.rotation).sin_cos();
        [self.easting + c * x - s * y, self.northing + s * x + c * y]
    }

    /// Grid easting and northing to project plan coordinates (metres).
    pub fn grid_to_project(&self, easting: f64, northing: f64) -> [f64; 2] {
        let (dx, dy) = (easting - self.easting, northing - self.northing);
        let (s, c) = self.rotation.sin_cos();
        [c * dx - s * dy, s * dx + c * dy]
    }

    /// A project height (relative to ±0.00) to an absolute altitude.
    pub fn project_to_altitude(&self, z: f64) -> f64 {
        self.altitude + z
    }

    /// An absolute altitude to a project height relative to ±0.00.
    pub fn altitude_to_project(&self, altitude: f64) -> f64 {
        altitude - self.altitude
    }

    /// Direction of grid north in project coordinates (unit vector).
    pub fn grid_north(&self) -> [f64; 2] {
        let (s, c) = self.rotation.sin_cos();
        [-s, c]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrotated_placement_is_a_translation() {
        let g = GridPlacement::at(476_000.0, 4_202_000.0, 85.0);
        let [e, n] = g.project_to_grid(10.0, 20.0);
        assert!((e - 476_010.0).abs() < 1e-9 && (n - 4_202_020.0).abs() < 1e-9);
        let [x, y] = g.grid_to_project(476_010.0, 4_202_020.0);
        assert!((x - 10.0).abs() < 1e-9 && (y - 20.0).abs() < 1e-9);
        assert!((g.project_to_altitude(3.2) - 88.2).abs() < 1e-12);
        assert!((g.altitude_to_project(84.55) + 0.45).abs() < 1e-12);
    }

    #[test]
    fn rotation_follows_the_documented_convention() {
        // Grid north 90° counter-clockwise from project +Y: grid north points along project −X.
        let g = GridPlacement {
            easting: 1000.0,
            northing: 2000.0,
            rotation: std::f64::consts::FRAC_PI_2,
            altitude: 0.0,
        };
        let n = g.grid_north();
        assert!((n[0] + 1.0).abs() < 1e-12 && n[1].abs() < 1e-12);
        // One metre along project −X is one metre grid north of the origin.
        let p = g.project_to_grid(-1.0, 0.0);
        assert!((p[0] - 1000.0).abs() < 1e-9 && (p[1] - 2001.0).abs() < 1e-9);
        // Round trip.
        let g = GridPlacement { rotation: 0.3, ..g };
        let [e, n] = g.project_to_grid(12.5, -7.25);
        let [x, y] = g.grid_to_project(e, n);
        assert!((x - 12.5).abs() < 1e-9 && (y + 7.25).abs() < 1e-9);
    }
}
