//! Lengths, angles, areas and volumes with explicit tolerances and locale-aware formatting.
//!
//! Layer L0. See `docs/architecture.md`.
//!
//! All quantities are stored in SI base units (metres, square metres, cubic metres, radians) as `f64`.
//! Comparisons always go through a [`Tolerance`]; never compare raw floats for equality.
//!
//! Every quantity serialises as a bare JSON number in its base unit (a [`Length`] of 3.2 m is `3.2`).

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! scalar_ops {
    ($t:ident) => {
        impl Add for $t {
            type Output = $t;
            fn add(self, rhs: $t) -> $t {
                $t(self.0 + rhs.0)
            }
        }
        impl AddAssign for $t {
            fn add_assign(&mut self, rhs: $t) {
                self.0 += rhs.0;
            }
        }
        impl Sub for $t {
            type Output = $t;
            fn sub(self, rhs: $t) -> $t {
                $t(self.0 - rhs.0)
            }
        }
        impl SubAssign for $t {
            fn sub_assign(&mut self, rhs: $t) {
                self.0 -= rhs.0;
            }
        }
        impl Neg for $t {
            type Output = $t;
            fn neg(self) -> $t {
                $t(-self.0)
            }
        }
        impl Mul<f64> for $t {
            type Output = $t;
            fn mul(self, rhs: f64) -> $t {
                $t(self.0 * rhs)
            }
        }
        impl Mul<$t> for f64 {
            type Output = $t;
            fn mul(self, rhs: $t) -> $t {
                $t(self * rhs.0)
            }
        }
        impl Div<f64> for $t {
            type Output = $t;
            fn div(self, rhs: f64) -> $t {
                $t(self.0 / rhs)
            }
        }
        /// Ratio of two quantities of the same kind.
        impl Div<$t> for $t {
            type Output = f64;
            fn div(self, rhs: $t) -> f64 {
                self.0 / rhs.0
            }
        }
        impl Sum for $t {
            fn sum<I: Iterator<Item = $t>>(iter: I) -> $t {
                iter.fold($t(0.0), |a, b| a + b)
            }
        }
        impl<'a> Sum<&'a $t> for $t {
            fn sum<I: Iterator<Item = &'a $t>>(iter: I) -> $t {
                iter.fold($t(0.0), |a, b| a + *b)
            }
        }
        impl $t {
            /// Absolute value.
            pub fn abs(self) -> Self {
                $t(self.0.abs())
            }
            /// Whether the value is finite (not NaN or infinite).
            pub fn is_finite(self) -> bool {
                self.0.is_finite()
            }
            /// The smaller of two values (NaN-safe: a NaN operand yields the other one).
            pub fn min(self, other: Self) -> Self {
                $t(self.0.min(other.0))
            }
            /// The larger of two values (NaN-safe: a NaN operand yields the other one).
            pub fn max(self, other: Self) -> Self {
                $t(self.0.max(other.0))
            }
            /// Total ordering, suitable for sorting.
            pub fn total_cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0.total_cmp(&other.0)
            }
        }
    };
}

/// A length in metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Length(f64);

scalar_ops!(Length);

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
    /// Value in centimetres.
    pub fn as_cm(self) -> f64 {
        self.0 * 100.0
    }
    /// Value in millimetres.
    pub fn as_mm(self) -> f64 {
        self.0 * 1000.0
    }
    /// Whether the length is strictly positive beyond the tolerance and finite.
    pub fn is_positive(self, tol: &Tolerance) -> bool {
        self.0.is_finite() && self.0 > tol.length.0
    }
}

impl Mul<Length> for Length {
    type Output = Area;
    fn mul(self, rhs: Length) -> Area {
        Area(self.0 * rhs.0)
    }
}

/// An area in square metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Area(f64);

scalar_ops!(Area);

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

impl Mul<Length> for Area {
    type Output = Volume;
    fn mul(self, rhs: Length) -> Volume {
        Volume(self.0 * rhs.0)
    }
}

impl Div<Length> for Area {
    type Output = Length;
    fn div(self, rhs: Length) -> Length {
        Length(self.0 / rhs.0)
    }
}

/// A volume in cubic metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Volume(f64);

scalar_ops!(Volume);

impl Volume {
    /// Zero volume.
    pub const ZERO: Volume = Volume(0.0);
    /// Creates a volume from cubic metres.
    pub const fn m3(value: f64) -> Self {
        Volume(value)
    }
    /// Value in cubic metres.
    pub const fn as_m3(self) -> f64 {
        self.0
    }
}

/// An angle in radians. Counter-clockwise is positive; zero points along +X (east in plan).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Angle(f64);

scalar_ops!(Angle);

impl Angle {
    /// Zero angle.
    pub const ZERO: Angle = Angle(0.0);
    /// A quarter turn.
    pub const RIGHT: Angle = Angle(std::f64::consts::FRAC_PI_2);
    /// A half turn.
    pub const STRAIGHT: Angle = Angle(std::f64::consts::PI);

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
    /// Sine.
    pub fn sin(self) -> f64 {
        self.0.sin()
    }
    /// Cosine.
    pub fn cos(self) -> f64 {
        self.0.cos()
    }
    /// Tangent.
    pub fn tan(self) -> f64 {
        self.0.tan()
    }
    /// The same direction expressed in `[0, 2π)`.
    pub fn normalized(self) -> Angle {
        Angle(self.0.rem_euclid(std::f64::consts::TAU))
    }
    /// The same direction expressed in `(-π, π]`.
    pub fn normalized_signed(self) -> Angle {
        let a = self.normalized().0;
        if a > std::f64::consts::PI { Angle(a - std::f64::consts::TAU) } else { Angle(a) }
    }
    /// Slope angle from a rise over a run, e.g. a roof pitch.
    pub fn from_slope(rise: Length, run: Length) -> Angle {
        Angle(rise.0.atan2(run.0))
    }
}

/// Project-wide geometric tolerance. Default 0.1 mm and 0.001°.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tolerance {
    /// Distance below which two lengths or points are considered equal.
    pub length: Length,
    /// Angle below which two directions are considered equal.
    #[serde(default = "default_angle_tolerance")]
    pub angle: Angle,
}

fn default_angle_tolerance() -> Angle {
    Angle(1e-5)
}

impl Default for Tolerance {
    fn default() -> Self {
        Tolerance { length: Length::mm(0.1), angle: default_angle_tolerance() }
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
    /// Whether two raw metre values are equal within tolerance.
    pub fn equal_m(&self, a: f64, b: f64) -> bool {
        (a - b).abs() <= self.length.as_m()
    }
    /// Whether two angles describe the same direction within tolerance.
    pub fn same_direction(&self, a: Angle, b: Angle) -> bool {
        (a - b).normalized_signed().abs().as_rad() <= self.angle.as_rad()
    }
}

/// User interface locale for number formatting and text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    /// Greek: decimal comma.
    #[default]
    El,
    /// English: decimal point.
    En,
}

impl Locale {
    /// The decimal separator.
    pub fn decimal_separator(self) -> char {
        match self {
            Locale::El => ',',
            Locale::En => '.',
        }
    }
    /// ISO 639-1 code.
    pub fn code(self) -> &'static str {
        match self {
            Locale::El => "el",
            Locale::En => "en",
        }
    }
}

/// Formats a number with a fixed number of decimals using the locale's decimal separator.
pub fn format_decimal(value: f64, decimals: usize, locale: Locale) -> String {
    // Avoid "-0,00".
    let value = if value.abs() < 0.5 * 10f64.powi(-(decimals as i32)) { 0.0 } else { value };
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

/// Display wrapper for a volume with two decimals, e.g. `412,80 m³`.
pub struct VolumeDisplay(pub Volume, pub Locale);

impl fmt::Display for VolumeDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} m³", format_decimal(self.0.as_m3(), 2, self.1))
    }
}

/// Display wrapper for an angle in degrees with two decimals, e.g. `30,00°`.
pub struct AngleDisplay(pub Angle, pub Locale);

impl fmt::Display for AngleDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}°", format_decimal(self.0.as_deg(), 2, self.1))
    }
}

/// Formats a level (στάθμη) with an explicit sign, e.g. `+3,20` or `-0,45`; zero is `±0,00`.
pub fn format_level(level: Length, locale: Locale) -> String {
    let v = level.as_m();
    if v.abs() < 0.005 {
        format!("±{}", format_decimal(0.0, 2, locale))
    } else if v > 0.0 {
        format!("+{}", format_decimal(v, 2, locale))
    } else {
        format_decimal(v, 2, locale)
    }
}

/// Error from [`parse_length`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseLengthError(pub String);

impl fmt::Display for ParseLengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a length: {}", self.0)
    }
}

impl std::error::Error for ParseLengthError {}

/// Parses a single length such as `3.20`, `3,20`, `320cm`, `25 mm` or `1.5m`. A bare number is in metres.
///
/// Both decimal separators are accepted regardless of locale. Expressions (`3.20+0.25`) are handled by the
/// typed-input parser in `kanavos-tools`, not here.
pub fn parse_length(input: &str) -> Result<Length, ParseLengthError> {
    let s = input.trim().replace(',', ".");
    let lower = s.to_lowercase();
    let (number, factor) = if let Some(n) = lower.strip_suffix("mm") {
        (n, 0.001)
    } else if let Some(n) = lower.strip_suffix("cm") {
        (n, 0.01)
    } else if let Some(n) = lower.strip_suffix('m') {
        (n, 1.0)
    } else {
        (lower.as_str(), 1.0)
    };
    let v: f64 = number.trim().parse().map_err(|_| ParseLengthError(input.to_owned()))?;
    if v.is_finite() { Ok(Length(v * factor)) } else { Err(ParseLengthError(input.to_owned())) }
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
        let v = a * Length::m(3.0);
        assert!((v.as_m3() - 30.0).abs() < 1e-12);
    }

    #[test]
    fn greek_formatting_uses_decimal_comma() {
        assert_eq!(LengthDisplay(Length::m(3.25), Locale::El).to_string(), "3,25 m");
        assert_eq!(AreaDisplay(Area::m2(124.5), Locale::En).to_string(), "124.50 m²");
        assert_eq!(format_decimal(-0.001, 2, Locale::El), "0,00");
    }

    #[test]
    fn levels_carry_a_sign() {
        assert_eq!(format_level(Length::ZERO, Locale::El), "±0,00");
        assert_eq!(format_level(Length::m(3.2), Locale::El), "+3,20");
        assert_eq!(format_level(Length::m(-0.45), Locale::En), "-0.45");
    }

    #[test]
    fn parses_lengths_with_units() {
        let t = Tolerance::default();
        assert!(t.equal(parse_length("320cm").unwrap(), Length::m(3.2)));
        assert!(t.equal(parse_length("3,20").unwrap(), Length::m(3.2)));
        assert!(t.equal(parse_length(" 25 mm ").unwrap(), Length::cm(2.5)));
        assert!(t.equal(parse_length("1.5m").unwrap(), Length::m(1.5)));
        assert!(parse_length("abc").is_err());
    }

    #[test]
    fn angles_normalise() {
        let t = Tolerance::default();
        assert!(t.same_direction(Angle::deg(370.0), Angle::deg(10.0)));
        assert!((Angle::deg(270.0).normalized_signed().as_deg() + 90.0).abs() < 1e-9);
    }

    #[test]
    fn quantities_serialise_as_bare_numbers() {
        assert_eq!(serde_json::to_string(&Length::m(3.2)).unwrap(), "3.2");
        let l: Length = serde_json::from_str("0.25").unwrap();
        assert!(Tolerance::default().equal(l, Length::cm(25.0)));
    }
}
