//! Project georeferencing and ΕΓΣΑ87 (EPSG:2100) coordinate handling.
//!
//! Layer L2. See `docs/architecture.md`.
//!
//! - [`Ellipsoid`]: GRS80 and WGS84 reference ellipsoids.
//! - [`TransverseMercator`]: forward and inverse Transverse Mercator with Krüger's series in the third flattening
//!   to sixth order, following C. F. F. Karney, *Transverse Mercator with an accuracy of a few nanometers*,
//!   J. Geodesy 85(8):475–485 (2011). Within a few thousand kilometres of the central meridian the error is far
//!   below a millimetre. [`TransverseMercator::tm87`] is the ΕΓΣΑ87 grid (TM87).
//! - [`datum`]: geodetic ↔ geocentric conversion and the published three-parameter shift between the Greek
//!   Geodetic Reference System 1987 (GGRS87, ΕΓΣΑ87) and WGS 84 (EPSG transformation 1272).
//! - [`GridPlacement`]: the map between project coordinates and grid coordinates (origin, rotation, altitude),
//!   with plain parameters so that `kanavos-model` can build it from its georeference.
//!
//! Angles in public functions are **degrees**; lengths are metres.

pub mod datum;
mod ellipsoid;
mod grid;
mod tm;

use std::fmt;

pub use ellipsoid::{Ellipsoid, GRS80, WGS84};
pub use grid::GridPlacement;
pub use tm::TransverseMercator;

/// EPSG code of the ΕΓΣΑ87 / Greek Grid projected coordinate system.
pub const EPSG_EGSA87: u32 = 2100;

/// Error from a coordinate conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoError {
    /// An input is NaN or infinite.
    NotFinite,
    /// A latitude is outside ±90°.
    LatitudeOutOfRange,
}

impl fmt::Display for GeoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeoError::NotFinite => write!(f, "coordinate is not a finite number"),
            GeoError::LatitudeOutOfRange => write!(f, "latitude outside ±90°"),
        }
    }
}

impl std::error::Error for GeoError {}

/// ΕΓΣΑ87 grid coordinates (easting, northing in metres) to WGS 84 latitude and longitude in degrees.
///
/// Heights are not involved: the datum shift is applied at zero ellipsoidal height and the height change is
/// dropped. That moves the horizontal result by a few millimetres at most, far inside the one-metre accuracy of the
/// published shift. Use [`datum::ggrs87_to_wgs84`] with heights when they matter.
pub fn egsa87_to_wgs84(easting: f64, northing: f64) -> Result<(f64, f64), GeoError> {
    let (lat, lon) = TransverseMercator::tm87().inverse(easting, northing)?;
    let (lat, lon, _) = datum::ggrs87_to_wgs84(lat, lon, 0.0)?;
    Ok((lat, lon))
}

/// WGS 84 latitude and longitude in degrees to ΕΓΣΑ87 grid coordinates (easting, northing in metres).
pub fn wgs84_to_egsa87(lat: f64, lon: f64) -> Result<(f64, f64), GeoError> {
    let (lat, lon, _) = datum::wgs84_to_ggrs87(lat, lon, 0.0)?;
    TransverseMercator::tm87().forward(lat, lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egsa87_and_wgs84_round_trip() {
        for (e, n) in [(476_000.0, 4_202_000.0), (400_500.0, 4_500_250.0), (600_000.0, 3_900_000.0)] {
            let (lat, lon) = egsa87_to_wgs84(e, n).unwrap();
            let (e2, n2) = wgs84_to_egsa87(lat, lon).unwrap();
            // Heights are dropped on each side, which costs a couple of millimetres (see the function docs).
            assert!((e - e2).hypot(n - n2) < 5e-3, "{e},{n} → {e2},{n2}");
        }
    }

    #[test]
    fn central_athens_is_where_expected() {
        // Syntagma square is near 37.975° N, 23.735° E: about 24 km west of the central meridian.
        let (e, n) = wgs84_to_egsa87(37.975, 23.735).unwrap();
        assert!((e - 476_700.0).abs() < 1_500.0, "E {e}");
        assert!((n - 4_202_000.0).abs() < 1_500.0, "N {n}");
    }

    #[test]
    fn bad_input_is_an_error() {
        assert_eq!(egsa87_to_wgs84(f64::NAN, 0.0), Err(GeoError::NotFinite));
        assert_eq!(wgs84_to_egsa87(91.0, 23.0), Err(GeoError::LatitudeOutOfRange));
    }
}
