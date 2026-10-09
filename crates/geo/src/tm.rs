//! Transverse Mercator projection, Krüger's series to sixth order in the third flattening.
//!
//! Formulas and coefficients: C. F. F. Karney, *Transverse Mercator with an accuracy of a few nanometers*,
//! J. Geodesy 85(8):475–485 (2011), equations (7)–(36), which restate L. Krüger (1912). The forward map goes from
//! geodetic latitude to conformal latitude, then through the series in α; the inverse uses the series in β and a
//! Newton iteration back to geodetic latitude.

use crate::{Ellipsoid, GRS80, GeoError};

/// A Transverse Mercator projection on an ellipsoid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransverseMercator {
    /// Ellipsoid.
    pub ellipsoid: Ellipsoid,
    /// Latitude of natural origin, degrees.
    pub lat0: f64,
    /// Longitude of the central meridian, degrees.
    pub lon0: f64,
    /// Scale factor on the central meridian.
    pub k0: f64,
    /// False easting, metres.
    pub false_easting: f64,
    /// False northing, metres.
    pub false_northing: f64,
    e: f64,
    /// Rectifying radius `A`.
    a_r: f64,
    alpha: [f64; 6],
    beta: [f64; 6],
    /// Northing of the natural origin before the false northing (`k0 · A · ξ(lat0)`).
    y0: f64,
}

/// Forward series coefficients α₁…α₆ (Karney 2011, eq. 35).
fn alpha(n: f64) -> [f64; 6] {
    let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
    [
        n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0 - 127.0 * n5 / 288.0
            + 7891.0 * n6 / 37800.0,
        13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0 + 281.0 * n5 / 630.0
            - 1_983_433.0 * n6 / 1_935_360.0,
        61.0 * n3 / 240.0 - 103.0 * n4 / 140.0 + 15061.0 * n5 / 26880.0 + 167_603.0 * n6 / 181_440.0,
        49561.0 * n4 / 161_280.0 - 179.0 * n5 / 168.0 + 6_601_661.0 * n6 / 7_257_600.0,
        34729.0 * n5 / 80640.0 - 3_418_889.0 * n6 / 1_995_840.0,
        212_378_941.0 * n6 / 319_334_400.0,
    ]
}

/// Inverse series coefficients β₁…β₆ (Karney 2011, eq. 36).
fn beta(n: f64) -> [f64; 6] {
    let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
    [
        n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0 - n4 / 360.0 - 81.0 * n5 / 512.0
            + 96199.0 * n6 / 604_800.0,
        n2 / 48.0 + n3 / 15.0 - 437.0 * n4 / 1440.0 + 46.0 * n5 / 105.0 - 1_118_711.0 * n6 / 3_870_720.0,
        17.0 * n3 / 480.0 - 37.0 * n4 / 840.0 - 209.0 * n5 / 4480.0 + 5569.0 * n6 / 90720.0,
        4397.0 * n4 / 161_280.0 - 11.0 * n5 / 504.0 - 830_251.0 * n6 / 7_257_600.0,
        4583.0 * n5 / 161_280.0 - 108_847.0 * n6 / 3_991_680.0,
        20_648_693.0 * n6 / 638_668_800.0,
    ]
}

/// Tangent of the conformal latitude from the tangent of the geodetic latitude (Karney 2011, eqs. 7–9).
fn tau_prime(tau: f64, e: f64) -> f64 {
    let s = (1.0 + tau * tau).sqrt();
    let sigma = (e * (e * tau / s).atanh()).sinh();
    tau * (1.0 + sigma * sigma).sqrt() - sigma * s
}

impl TransverseMercator {
    /// A projection from its defining parameters (angles in degrees).
    pub fn new(
        ellipsoid: Ellipsoid,
        lat0: f64,
        lon0: f64,
        k0: f64,
        false_easting: f64,
        false_northing: f64,
    ) -> TransverseMercator {
        let n = ellipsoid.n();
        let a_r = ellipsoid.a / (1.0 + n) * (1.0 + n * n / 4.0 + n.powi(4) / 64.0 + n.powi(6) / 256.0);
        let mut tm = TransverseMercator {
            ellipsoid,
            lat0,
            lon0,
            k0,
            false_easting,
            false_northing,
            e: ellipsoid.e2().sqrt(),
            a_r,
            alpha: alpha(n),
            beta: beta(n),
            y0: 0.0,
        };
        // Northing of the origin: on the central meridian ξ' is the conformal latitude.
        let chi = tau_prime(lat0.to_radians().tan(), tm.e).atan();
        let xi = chi
            + tm.alpha.iter().enumerate().map(|(j, a)| a * (2.0 * (j + 1) as f64 * chi).sin()).sum::<f64>();
        tm.y0 = k0 * a_r * xi;
        tm
    }

    /// The ΕΓΣΑ87 / Greek Grid projection (TM87, EPSG:2100): GRS80, central meridian 24° E, scale 0.9996,
    /// false easting 500 000 m, false northing 0, latitude of origin 0.
    pub fn tm87() -> TransverseMercator {
        TransverseMercator::new(GRS80, 0.0, 24.0, 0.9996, 500_000.0, 0.0)
    }

    /// Geodetic latitude and longitude (degrees) to easting and northing (metres).
    pub fn forward(&self, lat: f64, lon: f64) -> Result<(f64, f64), GeoError> {
        if !(lat.is_finite() && lon.is_finite()) {
            return Err(GeoError::NotFinite);
        }
        if lat.abs() > 90.0 {
            return Err(GeoError::LatitudeOutOfRange);
        }
        let (xi_p, eta_p, _) = self.conformal(lat, lon);
        let (mut xi, mut eta) = (xi_p, eta_p);
        for (j, a) in self.alpha.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            xi += a * (k * xi_p).sin() * (k * eta_p).cosh();
            eta += a * (k * xi_p).cos() * (k * eta_p).sinh();
        }
        let e = self.false_easting + self.k0 * self.a_r * eta;
        let n = self.false_northing + self.k0 * self.a_r * xi - self.y0;
        Ok((e, n))
    }

    /// Conformal coordinates ξ', η' of a point and τ' (tangent of its conformal latitude).
    fn conformal(&self, lat: f64, lon: f64) -> (f64, f64, f64) {
        let phi = lat.to_radians();
        let lam = (lon - self.lon0).to_radians();
        let lam = (lam + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        let tau = phi.tan();
        let tp = tau_prime(tau, self.e);
        let xi_p = tp.atan2(lam.cos());
        let eta_p = (lam.sin() / (tp * tp + lam.cos().powi(2)).sqrt()).asinh();
        (xi_p, eta_p, tp)
    }

    /// Easting and northing (metres) to geodetic latitude and longitude (degrees).
    pub fn inverse(&self, easting: f64, northing: f64) -> Result<(f64, f64), GeoError> {
        if !(easting.is_finite() && northing.is_finite()) {
            return Err(GeoError::NotFinite);
        }
        let xi = (northing - self.false_northing + self.y0) / (self.k0 * self.a_r);
        let eta = (easting - self.false_easting) / (self.k0 * self.a_r);
        let (mut xi_p, mut eta_p) = (xi, eta);
        for (j, b) in self.beta.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            xi_p -= b * (k * xi).sin() * (k * eta).cosh();
            eta_p -= b * (k * xi).cos() * (k * eta).sinh();
        }
        let tp = xi_p.sin() / (eta_p.sinh().powi(2) + xi_p.cos().powi(2)).sqrt();
        let lam = eta_p.sinh().atan2(xi_p.cos());
        // Newton iteration for τ from τ' (Karney 2011, eqs. 19–21).
        let e2 = self.e * self.e;
        let mut tau = tp;
        for _ in 0..10 {
            let t = tau_prime(tau, self.e);
            let d = (tp - t) / (1.0 + t * t).sqrt() * (1.0 + (1.0 - e2) * tau * tau)
                / ((1.0 - e2) * (1.0 + tau * tau).sqrt());
            tau += d;
            if !d.is_finite() || d.abs() <= 1e-14 * tau.abs().max(1.0) {
                break;
            }
        }
        if !tau.is_finite() {
            return Err(GeoError::NotFinite);
        }
        Ok((tau.atan().to_degrees(), self.lon0 + lam.to_degrees()))
    }

    /// Meridian convergence γ (degrees) and point scale factor k at a point.
    ///
    /// γ is the angle from true north to grid north, clockwise positive: true north has a grid bearing of −γ.
    /// It is positive east of the central meridian in the northern hemisphere (about Δλ · sin φ).
    /// Following Karney 2011, eqs. 26–28: `γ = γ' + γ''` and `k = k0 · k' · k''`.
    pub fn convergence_and_scale(&self, lat: f64, lon: f64) -> Result<(f64, f64), GeoError> {
        if !(lat.is_finite() && lon.is_finite()) {
            return Err(GeoError::NotFinite);
        }
        if lat.abs() > 90.0 {
            return Err(GeoError::LatitudeOutOfRange);
        }
        let (xi_p, eta_p, tp) = self.conformal(lat, lon);
        let lam = (lon - self.lon0).to_radians();
        let (mut p, mut q) = (1.0, 0.0);
        for (j, a) in self.alpha.iter().enumerate() {
            let k = 2.0 * (j + 1) as f64;
            p += k * a * (k * xi_p).cos() * (k * eta_p).cosh();
            q += k * a * (k * xi_p).sin() * (k * eta_p).sinh();
        }
        let gamma1 = (tp * lam.tan() / (1.0 + tp * tp).sqrt()).atan();
        let gamma2 = q.atan2(p);
        let phi = lat.to_radians();
        let e2 = self.e * self.e;
        let k1 = (1.0 - e2 * phi.sin().powi(2)).sqrt() * (1.0 + phi.tan().powi(2)).sqrt()
            / (tp * tp + lam.cos().powi(2)).sqrt();
        let k2 = self.a_r / self.ellipsoid.a * (p * p + q * q).sqrt();
        Ok(((gamma1 + gamma2).to_degrees(), self.k0 * k1 * k2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Meridian arc length from the equator by Simpson integration of the meridian radius (independent check).
    fn meridian_arc(ell: &Ellipsoid, lat_deg: f64) -> f64 {
        let n = 20_000;
        let h = lat_deg.to_radians() / n as f64;
        let f = |phi: f64| ell.meridian_radius(phi);
        let mut s = f(0.0) + f(lat_deg.to_radians());
        for i in 1..n {
            s += f(i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        s * h / 3.0
    }

    #[test]
    fn central_meridian_is_scaled_meridian_arc() {
        let tm = TransverseMercator::tm87();
        for lat in [34.0, 38.0, 41.5] {
            let (e, n) = tm.forward(lat, 24.0).unwrap();
            assert!((e - 500_000.0).abs() < 1e-9);
            let expected = 0.9996 * meridian_arc(&GRS80, lat);
            assert!((n - expected).abs() < 1e-4, "lat {lat}: {n} vs {expected}");
        }
    }

    /// EPSG Guidance Note 7-2 worked example for Transverse Mercator: OSGB 1936 / British National Grid
    /// (Airy 1830, origin 49° N 2° W, k0 0.9996012717, FE 400 000, FN −100 000). The point 50°30' N, 0°30' E
    /// projects to E 577 274.99, N 69 740.50.
    #[test]
    fn epsg_guidance_note_example() {
        let airy = Ellipsoid { a: 6_377_563.396, inv_f: 299.324_964_6 };
        let tm = TransverseMercator::new(airy, 49.0, -2.0, 0.999_601_271_7, 400_000.0, -100_000.0);
        let (e, n) = tm.forward(50.5, 0.5).unwrap();
        assert!((e - 577_274.99).abs() < 0.01, "E {e}");
        assert!((n - 69_740.50).abs() < 0.01, "N {n}");
        let (lat, lon) = tm.inverse(577_274.99, 69_740.50).unwrap();
        assert!((lat - 50.5).abs() < 1e-7 && (lon - 0.5).abs() < 1e-7, "{lat} {lon}");
    }

    #[test]
    fn forward_inverse_round_trip_is_sub_millimetre() {
        let tm = TransverseMercator::tm87();
        let mut worst = 0.0_f64;
        for i in 0..=40 {
            for j in 0..=40 {
                let lat = 34.0 + 8.0 * i as f64 / 40.0;
                let lon = 19.0 + 11.0 * j as f64 / 40.0;
                let (e, n) = tm.forward(lat, lon).unwrap();
                let (lat2, lon2) = tm.inverse(e, n).unwrap();
                let (e2, n2) = tm.forward(lat2, lon2).unwrap();
                worst = worst.max((e - e2).hypot(n - n2));
                assert!((lat - lat2).abs() < 1e-10 && (lon - lon2).abs() < 1e-10);
            }
        }
        assert!(worst < 1e-6, "worst round trip {worst} m");
    }

    #[test]
    fn scale_and_convergence_match_finite_differences() {
        let tm = TransverseMercator::tm87();
        let (lat, lon) = (38.2, 21.7);
        let (gamma, k) = tm.convergence_and_scale(lat, lon).unwrap();
        // Scale: grid distance over ellipsoid distance for a small step east.
        let d = 1e-5;
        let (e0, n0) = tm.forward(lat, lon).unwrap();
        let (e1, n1) = tm.forward(lat, lon + d).unwrap();
        let ground = GRS80.prime_vertical_radius(lat.to_radians()) * lat.to_radians().cos() * d.to_radians();
        let k_fd = (e1 - e0).hypot(n1 - n0) / ground;
        assert!((k - k_fd).abs() < 1e-7, "{k} vs {k_fd}");
        // Convergence: grid bearing of the meridian (a small step north).
        let (e2, n2) = tm.forward(lat + d, lon).unwrap();
        let bearing = (e2 - e0).atan2(n2 - n0).to_degrees();
        assert!((gamma + bearing).abs() < 1e-6, "γ {gamma} vs meridian bearing {bearing}");
        // West of the central meridian in the northern hemisphere, meridians lean towards it (γ < 0).
        assert!(gamma < 0.0 && (gamma - (lon - 24.0) * lat.to_radians().sin()).abs() < 0.01);
        // On the central meridian the scale is k0 and the convergence zero.
        let (g0, k0) = tm.convergence_and_scale(38.0, 24.0).unwrap();
        assert!(g0.abs() < 1e-12 && (k0 - 0.9996).abs() < 1e-12);
    }

    #[test]
    fn rejects_bad_input() {
        let tm = TransverseMercator::tm87();
        assert_eq!(tm.forward(f64::NAN, 24.0), Err(GeoError::NotFinite));
        assert_eq!(tm.forward(90.5, 24.0), Err(GeoError::LatitudeOutOfRange));
        assert_eq!(tm.inverse(f64::INFINITY, 0.0), Err(GeoError::NotFinite));
        assert!(tm.forward(90.0, 24.0).is_ok());
    }
}
