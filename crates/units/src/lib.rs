//! Lengths, angles, areas and volumes with explicit tolerances and locale-aware formatting.
//!
//! Layer L0. See `docs/architecture.md`.
//!
//! All quantities are stored in SI base units (metres, square metres, cubic metres, radians) as `f64`.
//! Comparisons always go through a [`Tolerance`]; never compare raw floats for equality.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

/// A length in metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Length(f64);

impl Length {
    /// Zero length.
    pub const ZERO: Length = Length(0.0);

    /// Creates a length from metres.
    pub const fn m(value: f64) -> Self {
        Length(value)
    }
    /// Creates a length from centimetres.
    pub fn cm(value: f64) -> Self {
        Length(value / 100.0)
    }
    /// Creates a length from millimetres.
    pub fn mm(value: f64) -> Self {
        Length(value / 1000.0)
    }
    /// Value in metres.
    pub const fn as_m(self) -> f64 {
        self.0
    }
    /// Value in millimetres.
    pub fn as_mm(self) -> f64 {
        self.0 * 1000.0
    }
    /// Absolute value.
    pub fn abs(self) -> Self {
        Length(self.0.abs())
    }
    /// Whether the value is finite (not NaN or infinite).
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }
}

impl Add for Length {
    type Output = Length;
    fn add(self, rhs: Length) -> Length {
        Length(self.0 + rhs.0)
    }
}
impl Sub for Length {
    type Output = Length;
    fn sub(self, rhs: Length) -> Length {
        Length(self.0 - rhs.0)
    }
}
impl Neg for Length {
    type Output = Length;
    fn neg(self) -> Length {
        Length(-self.0)
    }
}
impl Mul<f64> for Length {
    type Output = Length;
    fn mul(self, rhs: f64) -> Length {
        Length(self.0 * rhs)
    }
}
impl Mul<Length> for Length {
    type Output = Area;
    fn mul(self, rhs: Length) -> Area {
        Area(self.0 * rhs.0)
    }
}
impl Div<f64> for Length {
    type Output = Length;
    fn div(self, rhs: f64) -> Length {
        Length(self.0 / rhs)
    }
}

/// An area in square metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Area(f64);

impl Area {
    /// Zero area.
    pub const ZERO: Area = Area(0.0);
    /// Creates an area from square metres.
    pub const fn m2(value: f64) -> Self {
        Area(value)
    }
    /// Value in square metres.
    pub const fn as_m2(self) -> f64 {
        self.0
    }
}

impl Add for Area {
    type Output = Area;
    fn add(self, rhs: Area) -> Area {
        Area(self.0 + rhs.0)
    }
}
impl Sub for Area {
    type Output = Area;
    fn sub(self, rhs: Area) -> Area {
        Area(self.0 - rhs.0)
    }
}
impl Mul<Length> for Area {
    type Output = Volume;
    fn mul(self, rhs: Length) -> Volume {
        Volume(self.0 * rhs.0)
    }
}
impl std::iter::Sum for Area {
    fn sum<I: Iterator<Item = Area>>(iter: I) -> Area {
        iter.fold(Area::ZERO, |a, b| a + b)
    }
}

/// A volume in cubic metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Volume(f64);

impl Volume {
    /// Creates a volume from cubic metres.
    pub const fn m3(value: f64) -> Self {
        Volume(value)
    }
    /// Value in cubic metres.
    pub const fn as_m3(self) -> f64 {
        self.0
    }
}

/// An angle in radians.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Angle(f64);

impl Angle {
    /// Creates an angle from radians.
    pub const fn rad(value: f64) -> Self {
        Angle(value)
    }
    /// Creates an angle from degrees.
    pub fn deg(value: f64) -> Self {
        Angle(value.to_radians())
    }
    /// Value in radians.
    pub const fn as_rad(self) -> f64 {
        self.0
    }
    /// Value in degrees.
    pub fn as_deg(self) -> f64 {
        self.0.to_degrees()
    }
}

/// Project-wide geometric tolerance. Default 0.1 mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// Distance below which two lengths or points are considered equal.
    pub length: Length,
}

impl Default for Tolerance {
    fn default() -> Self {
        Tolerance { length: Length::mm(0.1) }
    }
}

impl Tolerance {
    /// Whether two lengths are equal within tolerance.
    pub fn equal(&self, a: Length, b: Length) -> bool {
        (a - b).abs().as_m() <= self.length.as_m()
    }
    /// Whether a length is zero within tolerance.
    pub fn is_zero(&self, a: Length) -> bool {
        a.abs().as_m() <= self.length.as_m()
    }
}

/// User interface locale for number formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    /// Greek: decimal comma.
    #[default]
    El,
    /// English: decimal point.
    En,
}

/// Formats a number with a fixed number of decimals using the locale's decimal separator.
pub fn format_decimal(value: f64, decimals: usize, locale: Locale) -> String {
    let s = format!("{value:.decimals$}");
    match locale {
        Locale::El => s.replace('.', ","),
        Locale::En => s,
    }
}

/// Display wrapper for a length in metres with two decimals, e.g. `3,25 m` in Greek.
pub struct LengthDisplay(pub Length, pub Locale);

impl fmt::Display for LengthDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} m", format_decimal(self.0.as_m(), 2, self.1))
    }
}

/// Display wrapper for an area with two decimals, e.g. `124,50 m²`.
pub struct AreaDisplay(pub Area, pub Locale);

impl fmt::Display for AreaDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} m²", format_decimal(self.0.as_m2(), 2, self.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_round_trip() {
        let t = Tolerance::default();
        assert!(t.equal(Length::cm(325.0), Length::m(3.25)));
        assert!(t.equal(Length::mm(250.0), Length::cm(25.0)));
        assert!((Length::m(1.2).as_mm() - 1200.0).abs() < 1e-9);
    }

    #[test]
    fn tolerance_is_a_tenth_of_a_millimetre() {
        let t = Tolerance::default();
        assert!(t.equal(Length::m(1.0), Length::m(1.000_09)));
        assert!(!t.equal(Length::m(1.0), Length::m(1.000_2)));
    }

    #[test]
    fn length_times_length_is_area() {
        let a = Length::m(4.0) * Length::m(2.5);
        assert!((a.as_m2() - 10.0).abs() < 1e-12);
    }

    #[test]
    fn greek_formatting_uses_decimal_comma() {
        assert_eq!(LengthDisplay(Length::m(3.25), Locale::El).to_string(), "3,25 m");
        assert_eq!(AreaDisplay(Area::m2(124.5), Locale::En).to_string(), "124.50 m²");
    }
}
