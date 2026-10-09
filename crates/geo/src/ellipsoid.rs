//! Reference ellipsoids.

/// A reference ellipsoid of revolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipsoid {
    /// Semi-major axis, metres.
    pub a: f64,
    /// Inverse flattening `1 / f`.
    pub inv_f: f64,
}

/// GRS 1980 (EPSG:7019), the ellipsoid of GGRS87 / ΕΓΣΑ87.
pub const GRS80: Ellipsoid = Ellipsoid { a: 6_378_137.0, inv_f: 298.257_222_101 };

/// WGS 84 (EPSG:7030).
pub const WGS84: Ellipsoid = Ellipsoid { a: 6_378_137.0, inv_f: 298.257_223_563 };

impl Ellipsoid {
    /// Flattening `f`.
    pub fn f(&self) -> f64 {
        1.0 / self.inv_f
    }
    /// First eccentricity squared `e² = f (2 − f)`.
    pub fn e2(&self) -> f64 {
        let f = self.f();
        f * (2.0 - f)
    }
    /// Semi-minor axis `b = a (1 − f)`.
    pub fn b(&self) -> f64 {
        self.a * (1.0 - self.f())
    }
    /// Third flattening `n = f / (2 − f)`.
    pub fn n(&self) -> f64 {
        let f = self.f();
        f / (2.0 - f)
    }
    /// Radius of curvature in the prime vertical at a latitude (radians).
    pub fn prime_vertical_radius(&self, lat: f64) -> f64 {
        self.a / (1.0 - self.e2() * lat.sin().powi(2)).sqrt()
    }
    /// Radius of curvature in the meridian at a latitude (radians).
    pub fn meridian_radius(&self, lat: f64) -> f64 {
        self.a * (1.0 - self.e2()) / (1.0 - self.e2() * lat.sin().powi(2)).powf(1.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_constants() {
        assert!((GRS80.b() - 6_356_752.314_140).abs() < 1e-3);
        assert!((WGS84.b() - 6_356_752.314_245).abs() < 1e-3);
        assert!((GRS80.e2() - 0.006_694_380_022_90).abs() < 1e-14);
    }
}
