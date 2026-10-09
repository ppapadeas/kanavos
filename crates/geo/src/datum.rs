//! Geodetic ↔ geocentric conversion and datum shifts.
//!
//! The GGRS87 ↔ WGS 84 shift is the published geocentric translation of EPSG transformation 1272
//! ("GGRS87 to WGS 84 (1)"): tX = −199.87 m, tY = +74.79 m, tZ = +246.62 m, from GGRS87 (GRS80 ellipsoid) to
//! WGS 84. Its stated accuracy is about one metre, which is the accuracy of any result that crosses datums; grid
//! work that stays in ΕΓΣΑ87 is unaffected.
//!
//! Geodetic ↔ geocentric formulas follow EPSG Guidance Note 7-2, §2.2.1.

use crate::{Ellipsoid, GRS80, GeoError, WGS84};

/// Geocentric translation from GGRS87 to WGS 84, metres (EPSG:1272).
pub const GGRS87_TO_WGS84: [f64; 3] = [-199.87, 74.79, 246.62];

fn check(lat: f64, lon: f64, h: f64) -> Result<(), GeoError> {
    if !(lat.is_finite() && lon.is_finite() && h.is_finite()) {
        return Err(GeoError::NotFinite);
    }
    if lat.abs() > 90.0 {
        return Err(GeoError::LatitudeOutOfRange);
    }
    Ok(())
}

/// Geodetic latitude, longitude (degrees) and ellipsoidal height (metres) to geocentric X, Y, Z (metres).
pub fn geodetic_to_geocentric(ell: &Ellipsoid, lat: f64, lon: f64, h: f64) -> Result<[f64; 3], GeoError> {
    check(lat, lon, h)?;
    let (phi, lam) = (lat.to_radians(), lon.to_radians());
    let nu = ell.prime_vertical_radius(phi);
    let e2 = ell.e2();
    Ok([
        (nu + h) * phi.cos() * lam.cos(),
        (nu + h) * phi.cos() * lam.sin(),
        ((1.0 - e2) * nu + h) * phi.sin(),
    ])
}

/// Geocentric X, Y, Z (metres) to geodetic latitude, longitude (degrees) and ellipsoidal height (metres), by
/// fixed-point iteration on the latitude (converges to 1e-14 rad in a few steps for terrestrial points).
pub fn geocentric_to_geodetic(ell: &Ellipsoid, xyz: [f64; 3]) -> Result<(f64, f64, f64), GeoError> {
    let [x, y, z] = xyz;
    if !(x.is_finite() && y.is_finite() && z.is_finite()) {
        return Err(GeoError::NotFinite);
    }
    let e2 = ell.e2();
    let p = x.hypot(y);
    let lam = y.atan2(x);
    if p < 1e-9 {
        // On the polar axis.
        let lat = if z >= 0.0 { 90.0 } else { -90.0 };
        return Ok((lat, lam.to_degrees(), z.abs() - ell.b()));
    }
    let mut phi = z.atan2(p * (1.0 - e2));
    let mut h = 0.0;
    for _ in 0..20 {
        let nu = ell.prime_vertical_radius(phi);
        h = if phi.abs() < std::f64::consts::FRAC_PI_4 {
            p / phi.cos() - nu
        } else {
            z / phi.sin() - nu * (1.0 - e2)
        };
        let next = z.atan2(p * (1.0 - e2 * nu / (nu + h)));
        let done = (next - phi).abs() < 1e-15;
        phi = next;
        if done {
            break;
        }
    }
    Ok((phi.to_degrees(), lam.to_degrees(), h))
}

/// Applies a geocentric translation between two datums.
pub fn translate(
    from: &Ellipsoid,
    to: &Ellipsoid,
    shift: [f64; 3],
    lat: f64,
    lon: f64,
    h: f64,
) -> Result<(f64, f64, f64), GeoError> {
    let [x, y, z] = geodetic_to_geocentric(from, lat, lon, h)?;
    geocentric_to_geodetic(to, [x + shift[0], y + shift[1], z + shift[2]])
}

/// GGRS87 (ΕΓΣΑ87) geodetic coordinates to WGS 84 (degrees, metres).
pub fn ggrs87_to_wgs84(lat: f64, lon: f64, h: f64) -> Result<(f64, f64, f64), GeoError> {
    translate(&GRS80, &WGS84, GGRS87_TO_WGS84, lat, lon, h)
}

/// WGS 84 geodetic coordinates to GGRS87 (ΕΓΣΑ87) (degrees, metres).
pub fn wgs84_to_ggrs87(lat: f64, lon: f64, h: f64) -> Result<(f64, f64, f64), GeoError> {
    let [tx, ty, tz] = GGRS87_TO_WGS84;
    translate(&WGS84, &GRS80, [-tx, -ty, -tz], lat, lon, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EPSG Guidance Note 7-2, §2.2.1 worked example on WGS 84: 53°48'33.82" N, 2°07'46.38" E, h = 73.0 m gives
    /// X = 3 771 793.97 m, Y = 140 253.34 m, Z = 5 124 304.35 m.
    #[test]
    fn epsg_geocentric_example() {
        let lat = 53.0 + 48.0 / 60.0 + 33.82 / 3600.0;
        let lon = 2.0 + 7.0 / 60.0 + 46.38 / 3600.0;
        let [x, y, z] = geodetic_to_geocentric(&WGS84, lat, lon, 73.0).unwrap();
        assert!((x - 3_771_793.97).abs() < 0.01, "X {x}");
        assert!((y - 140_253.34).abs() < 0.01, "Y {y}");
        assert!((z - 5_124_304.35).abs() < 0.01, "Z {z}");
        let (lat2, lon2, h2) = geocentric_to_geodetic(&WGS84, [x, y, z]).unwrap();
        assert!((lat2 - lat).abs() < 1e-11 && (lon2 - lon).abs() < 1e-11 && (h2 - 73.0).abs() < 1e-6);
    }

    #[test]
    fn datum_shift_moves_points_by_the_translation() {
        let (lat, lon, h) = (38.0, 23.7, 150.0);
        let (lat2, lon2, h2) = ggrs87_to_wgs84(lat, lon, h).unwrap();
        // Same physical point expressed in both datums: geocentric vectors differ by exactly the translation.
        let a = geodetic_to_geocentric(&GRS80, lat, lon, h).unwrap();
        let b = geodetic_to_geocentric(&WGS84, lat2, lon2, h2).unwrap();
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        for i in 0..3 {
            assert!((d[i] - GGRS87_TO_WGS84[i]).abs() < 1e-6);
        }
        // The horizontal shift in Greece is a few hundred metres at most, mostly in latitude and longitude.
        assert!((lat2 - lat).abs() < 0.01 && (lon2 - lon).abs() < 0.01);
        let (lat3, lon3, h3) = wgs84_to_ggrs87(lat2, lon2, h2).unwrap();
        assert!((lat3 - lat).abs() < 1e-11 && (lon3 - lon).abs() < 1e-11 && (h3 - h).abs() < 1e-6);
    }

    #[test]
    fn poles_and_bad_input() {
        let p = geodetic_to_geocentric(&GRS80, 90.0, 0.0, 0.0).unwrap();
        let (lat, _, h) = geocentric_to_geodetic(&GRS80, p).unwrap();
        assert!((lat - 90.0).abs() < 1e-9 && h.abs() < 1e-6);
        assert_eq!(geocentric_to_geodetic(&GRS80, [f64::NAN, 0.0, 0.0]), Err(GeoError::NotFinite));
        assert_eq!(geodetic_to_geocentric(&GRS80, 100.0, 0.0, 0.0), Err(GeoError::LatitudeOutOfRange));
    }
}
