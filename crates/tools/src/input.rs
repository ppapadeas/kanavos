//! Typed input while drawing (docs/ux.md §3): lengths with arithmetic and units, polar and relative offsets,
//! absolute project coordinates and absolute ΕΓΣΑ87 coordinates.
//!
//! Forms:
//! - `3.20`, `3,20`, `320cm`, `3.20+0.25`, `2*1.5` — a length (metres by default)
//! - `@2.5<90` — polar: length and angle in degrees from the previous point
//! - `@1.2,0.8` — relative offset from the previous point (Greek users may write `@1,2;0,8`)
//! - `3.5,4.0` — absolute project coordinates (`;` separator with decimal commas: `3,5;4`)
//! - `#476532.12,4097345.88` — absolute ΕΓΣΑ87 easting and northing, converted with the site's georeference
//!
//! A bare `3,4` is the length 3.4 m (decimal comma), never a coordinate pair: pairs need a `;`, or a `.` decimal
//! point next to the `,` separator.

use kanavos_geom::{Angle, Point2, Vec2};
use kanavos_model::Georef;
use kanavos_units::Length;

/// A parsed typed input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Typed {
    /// A length (or a bare number in metres).
    Length(Length),
    /// Length and direction (degrees) from the previous point.
    Polar {
        /// Distance.
        length: Length,
        /// Direction in degrees, counter-clockwise from +X.
        degrees: f64,
    },
    /// Offset from the previous point.
    Relative {
        /// X offset.
        dx: Length,
        /// Y offset.
        dy: Length,
    },
    /// Absolute project coordinates.
    Absolute {
        /// X.
        x: Length,
        /// Y.
        y: Length,
    },
    /// Absolute ΕΓΣΑ87 grid coordinates.
    Grid {
        /// Easting, metres.
        easting: f64,
        /// Northing, metres.
        northing: f64,
    },
}

impl Typed {
    /// Whether the input names a point by itself (everything except a bare length).
    pub fn is_point(&self) -> bool {
        !matches!(self, Typed::Length(_))
    }

    /// The point this input names. Polar and relative inputs are measured from `from` (the project origin when
    /// there is none); absolute ΕΓΣΑ87 coordinates are converted with `georef`. A bare length has no point.
    pub fn to_point(&self, from: Option<Point2>, georef: &Georef) -> Option<Point2> {
        let base = from.unwrap_or(Point2::ORIGIN);
        match *self {
            Typed::Length(_) => None,
            Typed::Polar { length, degrees } => {
                Some(base + Vec2::from_angle(Angle::deg(degrees)) * length.as_m())
            }
            Typed::Relative { dx, dy } => Some(base + Vec2::new(dx.as_m(), dy.as_m())),
            Typed::Absolute { x, y } => Some(Point2::from_lengths(x, y)),
            Typed::Grid { easting, northing } => Some(grid_to_project(georef, easting, northing)),
        }
    }
}

/// Why typed input could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputError(pub String);

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cannot read '{}'", self.0)
    }
}

impl std::error::Error for InputError {}

/// Evaluates a length expression: numbers with optional unit suffixes (m, cm, mm), `+ - * /` and parentheses.
pub fn eval_length(s: &str) -> Result<Length, InputError> {
    eval_number(s).map(Length::m)
}

/// Evaluates an angle in degrees: a number expression with an optional `°` sign, e.g. `90`, `22,5°`, `180-30`.
pub fn eval_angle(s: &str) -> Result<f64, InputError> {
    let cleaned = s.trim().trim_end_matches('°').trim();
    eval_number(cleaned).map_err(|_| InputError(s.to_owned()))
}

fn eval_number(s: &str) -> Result<f64, InputError> {
    let toks = tokenize(s).ok_or_else(|| InputError(s.to_owned()))?;
    if toks.is_empty() {
        return Err(InputError(s.to_owned()));
    }
    let mut pos = 0;
    let v = expr(&toks, &mut pos).ok_or_else(|| InputError(s.to_owned()))?;
    if pos != toks.len() || !v.is_finite() {
        return Err(InputError(s.to_owned()));
    }
    Ok(v)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let s = s.replace(',', ".");
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while chars.get(i).is_some_and(|c| c.is_ascii_digit() || *c == '.') {
                i += 1;
            }
            let num: String = chars.get(start..i)?.iter().collect();
            let mut v: f64 = num.parse().ok()?;
            let rest: String = chars.get(i..)?.iter().take(2).collect::<String>().to_lowercase();
            if rest.starts_with("mm") {
                v /= 1000.0;
                i += 2;
            } else if rest.starts_with("cm") {
                v /= 100.0;
                i += 2;
            } else if rest.starts_with('m') {
                i += 1;
            }
            out.push(Tok::Num(v));
        } else if "+-*/()".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

fn expr(t: &[Tok], pos: &mut usize) -> Option<f64> {
    let mut v = term(t, pos)?;
    while let Some(Tok::Op(op @ ('+' | '-'))) = t.get(*pos).copied() {
        *pos += 1;
        let r = term(t, pos)?;
        v = if op == '+' { v + r } else { v - r };
    }
    Some(v)
}

fn term(t: &[Tok], pos: &mut usize) -> Option<f64> {
    let mut v = factor(t, pos)?;
    while let Some(Tok::Op(op @ ('*' | '/'))) = t.get(*pos).copied() {
        *pos += 1;
        let r = factor(t, pos)?;
        v = if op == '*' { v * r } else { v / r };
    }
    Some(v)
}

fn factor(t: &[Tok], pos: &mut usize) -> Option<f64> {
    match t.get(*pos).copied()? {
        Tok::Num(v) => {
            *pos += 1;
            Some(v)
        }
        Tok::Op('-') => {
            *pos += 1;
            factor(t, pos).map(|v| -v)
        }
        Tok::Op('+') => {
            *pos += 1;
            factor(t, pos)
        }
        Tok::Op('(') => {
            *pos += 1;
            let v = expr(t, pos)?;
            if t.get(*pos) == Some(&Tok::Op(')')) {
                *pos += 1;
                Some(v)
            } else {
                None
            }
        }
        Tok::Op(_) => None,
    }
}

fn split_pair(s: &str) -> Option<(&str, &str)> {
    // `;` separates when decimal commas are used; otherwise `,` separates.
    if let Some(p) = s.split_once(';') {
        return Some(p);
    }
    s.split_once(',')
}

/// Parses typed input.
pub fn parse(input: &str) -> Result<Typed, InputError> {
    let s = input.trim();
    let err = || InputError(input.to_owned());
    if let Some(rest) = s.strip_prefix('#') {
        let (e, n) = split_pair(rest).ok_or_else(err)?;
        let easting: f64 = e.trim().replace(',', ".").parse().map_err(|_| err())?;
        let northing: f64 = n.trim().replace(',', ".").parse().map_err(|_| err())?;
        if !easting.is_finite() || !northing.is_finite() {
            return Err(err());
        }
        return Ok(Typed::Grid { easting, northing });
    }
    if let Some(rest) = s.strip_prefix('@') {
        if let Some((l, a)) = rest.split_once('<') {
            let degrees = eval_angle(a).map_err(|_| err())?;
            return Ok(Typed::Polar { length: eval_length(l)?, degrees });
        }
        let (x, y) = split_pair(rest).ok_or_else(err)?;
        return Ok(Typed::Relative { dx: eval_length(x)?, dy: eval_length(y)? });
    }
    if s.contains(';') || (s.contains(',') && s.contains('.')) {
        let (x, y) = split_pair(s).ok_or_else(err)?;
        return Ok(Typed::Absolute { x: eval_length(x)?, y: eval_length(y)? });
    }
    Ok(Typed::Length(eval_length(s)?))
}

/// Converts national-grid coordinates (ΕΓΣΑ87 easting and northing, metres) to project coordinates.
///
/// The georeference places the project origin at (`easting`, `northing`) and gives the angle from project +Y to
/// grid north, counter-clockwise positive. Grid north in project coordinates is therefore +Y rotated by that
/// angle, and `project = R(rotation) · (grid − origin)`. The conversion is a rigid motion: no scale factor is
/// applied (the ΕΓΣΑ87 scale factor is below 1 part in 1,000 over a building plot and surveys are drawn at grid
/// scale).
pub fn grid_to_project(georef: &Georef, easting: f64, northing: f64) -> Point2 {
    let d = Vec2::new(easting - georef.easting, northing - georef.northing).rotated(georef.rotation);
    Point2::new(d.x, d.y)
}

/// Converts project coordinates to national-grid coordinates (easting, northing); the inverse of
/// [`grid_to_project`].
pub fn project_to_grid(georef: &Georef, p: Point2) -> (f64, f64) {
    let d = p.to_vec().rotated(-georef.rotation);
    (georef.easting + d.x, georef.northing + d.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_units::Tolerance;

    fn len(s: &str) -> f64 {
        match parse(s).unwrap() {
            Typed::Length(l) => l.as_m(),
            other => panic!("not a length: {other:?}"),
        }
    }

    #[test]
    fn lengths_with_units_and_arithmetic() {
        let tol = Tolerance::default();
        assert!(tol.equal_m(len("3.20+0.25"), 3.45));
        assert!(tol.equal_m(len("320cm"), 3.2));
        assert!(tol.equal_m(len("3,20"), 3.2));
        assert!(tol.equal_m(len("2*(1.5+0.5)"), 4.0));
        assert!(tol.equal_m(len("250mm"), 0.25));
        assert!(tol.equal_m(len("4-25cm"), 3.75));
        assert!(parse("").is_err());
        assert!(parse("3+").is_err());
    }

    #[test]
    fn polar_relative_absolute_and_grid() {
        assert!(
            matches!(parse("@2.5<90").unwrap(), Typed::Polar { degrees, .. } if (degrees - 90.0).abs() < 1e-9)
        );
        assert!(matches!(parse("@1.2,0.8").unwrap(), Typed::Relative { .. }));
        assert!(matches!(parse("@1,2;0,8").unwrap(), Typed::Relative { .. }));
        assert!(matches!(parse("3.5,4.0").unwrap(), Typed::Absolute { .. }));
        assert!(matches!(parse("3,5;4").unwrap(), Typed::Absolute { .. }));
        assert!(matches!(parse("#476532.12,4097345.88").unwrap(), Typed::Grid { .. }));
        assert!(matches!(parse("3,4").unwrap(), Typed::Length(_)));
        assert!(parse("abc").is_err());
    }

    #[test]
    fn angles_accept_degree_signs_and_commas() {
        assert!((eval_angle("22,5°").unwrap() - 22.5).abs() < 1e-12);
        assert!((eval_angle("180-30").unwrap() - 150.0).abs() < 1e-12);
        assert!(eval_angle("north").is_err());
    }

    #[test]
    fn typed_points_resolve_from_the_previous_point() {
        let tol = Tolerance::default();
        let g = Georef::default();
        let from = Some(Point2::new(1.0, 1.0));
        let p = parse("@2<90").unwrap().to_point(from, &g).unwrap();
        assert!(p.coincides(Point2::new(1.0, 3.0), &tol));
        let p = parse("@1;-1").unwrap().to_point(from, &g).unwrap();
        assert!(p.coincides(Point2::new(2.0, 0.0), &tol));
        let p = parse("5.5,2.0").unwrap().to_point(from, &g).unwrap();
        assert!(p.coincides(Point2::new(5.5, 2.0), &tol));
        assert!(parse("3").unwrap().to_point(from, &g).is_none());
    }

    #[test]
    fn grid_coordinates_follow_the_georeference() {
        let tol = Tolerance::default();
        let mut g = Georef {
            epsg: 2100,
            easting: 476_500.0,
            northing: 4_097_300.0,
            rotation: Angle::ZERO,
            altitude: 0.0,
        };
        let p = grid_to_project(&g, 476_532.12, 4_097_345.88);
        assert!(p.coincides(Point2::new(32.12, 45.88), &tol));
        // Grid north is project +Y turned 90° counter-clockwise (project −X): a point 10 m north of the origin
        // lies at project (−10, 0).
        g.rotation = Angle::deg(90.0);
        let p = grid_to_project(&g, 476_500.0, 4_097_310.0);
        assert!(p.coincides(Point2::new(-10.0, 0.0), &tol));
        let (e, n) = project_to_grid(&g, p);
        assert!(tol.equal_m(e, 476_500.0) && tol.equal_m(n, 4_097_310.0));
        let q = parse("#476532.12;4097345.88").unwrap().to_point(None, &g).unwrap();
        let (e, n) = project_to_grid(&g, q);
        assert!(tol.equal_m(e, 476_532.12) && tol.equal_m(n, 4_097_345.88));
    }
}
