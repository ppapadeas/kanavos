//! Typed input while drawing (docs/ux.md §3): lengths with arithmetic and units, polar and relative offsets,
//! absolute project coordinates and absolute ΕΓΣΑ87 coordinates.
//!
//! Forms:
//! - `3.20`, `3,20`, `320cm`, `3.20+0.25`, `2*1.5` — a length (metres by default)
//! - `@2.5<90` — polar: length and angle in degrees from the previous point
//! - `@1.2,0.8` — relative offset from the previous point (Greek users may write `@1,2;0,8`)
//! - `3.5,4` — absolute project coordinates (`;` separator with decimal commas)
//! - `#476532.12,4097345.88` — absolute ΕΓΣΑ87 easting and northing

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
    let toks = tokenize(s).ok_or_else(|| InputError(s.to_owned()))?;
    let mut pos = 0;
    let v = expr(&toks, &mut pos).ok_or_else(|| InputError(s.to_owned()))?;
    if pos != toks.len() || !v.is_finite() {
        return Err(InputError(s.to_owned()));
    }
    Ok(Length::m(v))
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
    if s.matches(',').count() == 1 && !s.contains('.') {
        // Ambiguous "1,2": treat as a pair only in coordinate contexts (the caller decides); here, a pair.
        return s.split_once(',');
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
        return Ok(Typed::Grid { easting, northing });
    }
    if let Some(rest) = s.strip_prefix('@') {
        if let Some((l, a)) = rest.split_once('<') {
            let degrees: f64 = a.trim().replace(',', ".").parse().map_err(|_| err())?;
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
    }

    #[test]
    fn polar_relative_absolute_and_grid() {
        assert!(
            matches!(parse("@2.5<90").unwrap(), Typed::Polar { degrees, .. } if (degrees - 90.0).abs() < 1e-9)
        );
        assert!(matches!(parse("@1.2,0.8").unwrap(), Typed::Relative { .. }));
        assert!(matches!(parse("@1,2;0,8").unwrap(), Typed::Relative { .. }));
        assert!(matches!(parse("3.5,4.0").unwrap(), Typed::Absolute { .. }));
        assert!(matches!(parse("#476532.12,4097345.88").unwrap(), Typed::Grid { .. }));
        assert!(parse("abc").is_err());
    }
}
