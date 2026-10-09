//! Writer for the Part 21 clear-text encoding.

use std::borrow::Cow;
use std::fmt::Write as _;

use super::text::encode_string;

/// A parameter to write.
#[derive(Debug, Clone, PartialEq)]
pub enum P {
    /// `$`
    Null,
    /// `*`
    Derived,
    /// An integer.
    Int(i64),
    /// A real number (written with [`format_real`]).
    Real(f64),
    /// A string (escaped with [`encode_string`]).
    Str(String),
    /// An enumeration value, without dots.
    Enum(Cow<'static, str>),
    /// `.T.` or `.F.`
    Bool(bool),
    /// `#n`
    Ref(u64),
    /// `( … )`
    List(Vec<P>),
    /// A typed parameter, `NAME(value)`.
    Typed(&'static str, Box<P>),
}

impl P {
    /// A string parameter.
    pub fn s(text: impl Into<String>) -> P {
        P::Str(text.into())
    }
    /// A string parameter, or `$` when the text is empty.
    pub fn opt_s(text: &str) -> P {
        if text.is_empty() { P::Null } else { P::Str(text.to_owned()) }
    }
    /// An enumeration parameter.
    pub fn e(value: &'static str) -> P {
        P::Enum(Cow::Borrowed(value))
    }
    /// A list of references.
    pub fn refs(ids: impl IntoIterator<Item = u64>) -> P {
        P::List(ids.into_iter().map(P::Ref).collect())
    }
    /// A list of reals.
    pub fn reals(values: impl IntoIterator<Item = f64>) -> P {
        P::List(values.into_iter().map(P::Real).collect())
    }
    /// A typed parameter.
    pub fn typed(name: &'static str, value: P) -> P {
        P::Typed(name, Box::new(value))
    }

    fn write(&self, out: &mut String) {
        match self {
            P::Null => out.push('$'),
            P::Derived => out.push('*'),
            P::Int(i) => {
                let _ = write!(out, "{i}");
            }
            P::Real(v) => out.push_str(&format_real(*v)),
            P::Str(s) => out.push_str(&encode_string(s)),
            P::Enum(e) => {
                out.push('.');
                out.push_str(e);
                out.push('.');
            }
            P::Bool(b) => out.push_str(if *b { ".T." } else { ".F." }),
            P::Ref(id) => {
                let _ = write!(out, "#{id}");
            }
            P::List(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(')');
            }
            P::Typed(name, inner) => {
                out.push_str(name);
                out.push('(');
                inner.write(out);
                out.push(')');
            }
        }
    }
}

/// Formats a real as Part 21 requires: always a decimal point, exponent with `E`.
///
/// Values are written with up to 12 significant digits when that changes them by no more than 1e-11 of their
/// magnitude (so `2.9999999999999996` is written `3.`), otherwise in the shortest form that reads back exactly.
/// Magnitudes below 1e-12 are written as `0.`. Non-finite values, which Part 21 cannot represent, are written as
/// `0.`; callers check their data first.
pub fn format_real(v: f64) -> String {
    if !v.is_finite() || v.abs() < 1e-12 {
        return "0.".to_owned();
    }
    let rounded: f64 = format!("{v:.11e}").parse().unwrap_or(v);
    let v = if (rounded - v).abs() <= 1e-11 * v.abs().max(1.0) { rounded } else { v };
    let a = v.abs();
    let text = if (1e-4..1e15).contains(&a) { format!("{v}") } else { format!("{v:E}") };
    let (mantissa, exponent) = match text.split_once('E') {
        Some((m, e)) => (m.to_owned(), Some(e.to_owned())),
        None => (text, None),
    };
    let mut out = mantissa;
    if !out.contains('.') {
        out.push('.');
    }
    if let Some(e) = exponent {
        out.push('E');
        out.push_str(&e);
    }
    out
}

/// Accumulates entity instances with consecutive ids.
#[derive(Debug, Default)]
pub struct StepWriter {
    data: String,
    count: u64,
}

impl StepWriter {
    /// An empty writer; the first instance gets id 1.
    pub fn new() -> StepWriter {
        StepWriter::default()
    }

    /// Appends an instance and returns its id.
    pub fn add(&mut self, name: &str, params: Vec<P>) -> u64 {
        self.count += 1;
        let id = self.count;
        let _ = write!(self.data, "#{id}={name}(");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.data.push(',');
            }
            p.write(&mut self.data);
        }
        self.data.push_str(");\n");
        id
    }

    /// Number of instances written.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// The complete exchange structure with the given header entities.
    pub fn finish(self, header: &[(&str, Vec<P>)]) -> String {
        let mut out = String::with_capacity(self.data.len() + 512);
        out.push_str("ISO-10303-21;\nHEADER;\n");
        for (name, params) in header {
            out.push_str(name);
            out.push('(');
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                p.write(&mut out);
            }
            out.push_str(");\n");
        }
        out.push_str("ENDSEC;\nDATA;\n");
        out.push_str(&self.data);
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::{Value, parse};

    #[test]
    fn reals_always_have_a_point() {
        assert_eq!(format_real(0.0), "0.");
        assert_eq!(format_real(-0.0), "0.");
        assert_eq!(format_real(3.0), "3.");
        assert_eq!(format_real(-2.5), "-2.5");
        assert_eq!(format_real(0.1 + 0.2), "0.3");
        assert_eq!(format_real(2.9999999999999996), "3.");
        assert_eq!(format_real(1e-5), "1.E-5");
        assert_eq!(format_real(1.5e20), "1.5E20");
        assert_eq!(format_real(1e-13), "0.");
        assert_eq!(format_real(4_500_000.123), "4500000.123");
        assert_eq!(format_real(f64::NAN), "0.");
    }

    #[test]
    fn written_files_parse_back() {
        let mut w = StepWriter::new();
        let p = w.add("IFCCARTESIANPOINT", vec![P::reals([0.0, 1.25, -3.0])]);
        let q = w.add(
            "IFCTHING",
            vec![
                P::s("Δοκιμή 'x'"),
                P::Null,
                P::Derived,
                P::Int(-4),
                P::e("ELEMENT"),
                P::Bool(false),
                P::Ref(p),
                P::typed("IFCLABEL", P::s("a")),
                P::List(vec![]),
            ],
        );
        let text = w.finish(&[("FILE_SCHEMA", vec![P::List(vec![P::s("IFC4X3_ADD2")])])]);
        assert!(text.is_ascii());
        let f = parse(text.as_bytes()).unwrap();
        let e = f.get(q).unwrap();
        assert_eq!(e.str_at(0), Some("Δοκιμή 'x'"));
        assert_eq!(e.attr(2), Some(&Value::Derived));
        assert_eq!(e.attr(3).and_then(Value::as_i64), Some(-4));
        assert_eq!(e.enum_at(4), Some("ELEMENT"));
        assert_eq!(e.attr(5).and_then(Value::as_bool), Some(false));
        assert_eq!(e.ref_at(6), Some(p));
        assert_eq!(e.attr(7).and_then(Value::type_name), Some("IFCLABEL"));
        assert_eq!(e.list_at(8).map(<[Value]>::len), Some(0));
        assert_eq!(f.schemas(), vec!["IFC4X3_ADD2".to_owned()]);
    }

    #[test]
    fn reals_round_trip_within_tolerance() {
        for v in [0.123456789012345, 12345.6789, -1e-9, 6.02214076e23, 1.0 / 3.0] {
            let back: f64 = format_real(v).replace(".E", ".0E").parse().unwrap();
            assert!((back - v).abs() <= 1e-11 * v.abs().max(1.0), "{v} -> {}", format_real(v));
        }
    }
}
