//! Reader for the Part 21 clear-text encoding.
//!
//! Lenient where real files deviate harmlessly from ISO 10303-21 (lower-case keywords, UTF-8 bytes inside
//! strings, reals without digits after the point), strict where the structure is ambiguous.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::text::decode_string;
use super::{Entity, Record, StepError, StepFile, Value};

/// Parses a complete exchange structure (`ISO-10303-21;` … `END-ISO-10303-21;`).
pub fn parse(input: &[u8]) -> Result<StepFile, StepError> {
    let mut p = Parser { src: input, pos: 0, names: HashMap::new() };
    // A UTF-8 byte order mark is tolerated.
    if input.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.pos = 3;
    }
    p.skip_ws()?;
    let magic = p.keyword()?;
    if !magic.eq_ignore_ascii_case(b"ISO-10303-21") {
        return Err(p.error_at(0, "not an ISO 10303-21 file (missing ISO-10303-21;)"));
    }
    p.expect(b';')?;
    let mut file = StepFile::default();
    let mut saw_header = false;
    loop {
        p.skip_ws()?;
        if p.peek().is_none() {
            return Err(p.error("unexpected end of file (missing END-ISO-10303-21;)"));
        }
        let start = p.pos;
        let kw = p.keyword()?.to_ascii_uppercase();
        match kw.as_slice() {
            b"HEADER" => {
                p.expect(b';')?;
                file.header = p.header_section()?;
                saw_header = true;
            }
            b"DATA" => {
                p.skip_ws()?;
                if p.peek() == Some(b'(') {
                    // Second-edition data section parameters (name and schema): not needed.
                    p.pos += 1;
                    p.params_after_open()?;
                }
                p.expect(b';')?;
                p.data_section(&mut file.data)?;
            }
            b"ANCHOR" | b"REFERENCE" | b"SIGNATURE" => p.skip_section()?,
            b"END-ISO-10303-21" => {
                p.expect(b';')?;
                break;
            }
            _ => {
                return Err(
                    p.error_at(start, &format!("unexpected keyword {}", String::from_utf8_lossy(&kw)))
                );
            }
        }
    }
    if !saw_header {
        return Err(p.error_at(0, "missing HEADER section"));
    }
    let mut by_type: BTreeMap<Arc<str>, Vec<u64>> = BTreeMap::new();
    for (id, record) in &file.data {
        if let Record::Simple(e) = record {
            by_type.entry(e.name.clone()).or_default().push(*id);
        }
    }
    file.by_type = by_type;
    Ok(file)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    names: HashMap<Vec<u8>, Arc<str>>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn error(&self, message: &str) -> StepError {
        self.error_at(self.pos, message)
    }

    fn error_at(&self, pos: usize, message: &str) -> StepError {
        let before = self.src.get(..pos).unwrap_or(self.src);
        let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
        let line_start = before.iter().rposition(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
        StepError { line, column: pos - line_start + 1, message: message.to_owned() }
    }

    /// Skips white space and `/* … */` comments.
    fn skip_ws(&mut self) -> Result<(), StepError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C) => self.pos += 1,
                Some(b'/') if self.src.get(self.pos + 1) == Some(&b'*') => {
                    let start = self.pos;
                    self.pos += 2;
                    loop {
                        match self.peek() {
                            None => return Err(self.error_at(start, "unterminated comment")),
                            Some(b'*') if self.src.get(self.pos + 1) == Some(&b'/') => {
                                self.pos += 2;
                                break;
                            }
                            Some(_) => self.pos += 1,
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), StepError> {
        self.skip_ws()?;
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            let found = match self.peek() {
                Some(c) => format!("'{}'", c as char),
                None => "end of file".to_owned(),
            };
            Err(self.error(&format!("expected '{}', found {found}", b as char)))
        }
    }

    /// A keyword: letters, digits, `_` and `-`, optionally starting with `!` (user-defined).
    fn keyword(&mut self) -> Result<&'a [u8], StepError> {
        let start = self.pos;
        if self.peek() == Some(b'!') {
            self.pos += 1;
        }
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => {}
            _ => return Err(self.error("expected a keyword")),
        }
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            self.pos += 1;
        }
        Ok(self.src.get(start..self.pos).unwrap_or_default())
    }

    fn intern(&mut self, raw: &[u8]) -> Arc<str> {
        if let Some(n) = self.names.get(raw) {
            return n.clone();
        }
        let name: Arc<str> = Arc::from(String::from_utf8_lossy(raw).to_ascii_uppercase());
        self.names.insert(raw.to_vec(), name.clone());
        name
    }

    fn header_section(&mut self) -> Result<Vec<Entity>, StepError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws()?;
            let kw = self.keyword()?;
            if kw.eq_ignore_ascii_case(b"ENDSEC") {
                self.expect(b';')?;
                return Ok(out);
            }
            let name = self.intern(kw);
            self.expect(b'(')?;
            let params = self.params_after_open()?;
            self.expect(b';')?;
            out.push(Entity { name, params });
        }
    }

    fn data_section(&mut self, data: &mut BTreeMap<u64, Record>) -> Result<(), StepError> {
        loop {
            self.skip_ws()?;
            match self.peek() {
                Some(b'#') => {
                    let start = self.pos;
                    self.pos += 1;
                    let id = self.unsigned()?;
                    self.expect(b'=')?;
                    self.skip_ws()?;
                    let record = if self.peek() == Some(b'(') {
                        self.pos += 1;
                        let mut parts = Vec::new();
                        loop {
                            self.skip_ws()?;
                            if self.peek() == Some(b')') {
                                self.pos += 1;
                                break;
                            }
                            parts.push(self.simple_record()?);
                        }
                        Record::Complex(parts)
                    } else {
                        Record::Simple(self.simple_record()?)
                    };
                    self.expect(b';')?;
                    if data.insert(id, record).is_some() {
                        return Err(self.error_at(start, &format!("instance #{id} is defined twice")));
                    }
                }
                Some(_) => {
                    let kw = self.keyword()?;
                    if kw.eq_ignore_ascii_case(b"ENDSEC") {
                        self.expect(b';')?;
                        return Ok(());
                    }
                    return Err(self.error("expected an instance (#n=...) or ENDSEC"));
                }
                None => return Err(self.error("unexpected end of file in DATA section")),
            }
        }
    }

    fn skip_section(&mut self) -> Result<(), StepError> {
        // Sections we do not use: skip tokens up to ENDSEC;, honouring strings and comments.
        loop {
            self.skip_ws()?;
            match self.peek() {
                None => return Err(self.error("unterminated section")),
                Some(b'\'') => {
                    self.pos += 1;
                    self.string_bytes()?;
                }
                Some(c) if c.is_ascii_alphabetic() => {
                    let kw = self.keyword()?;
                    if kw.eq_ignore_ascii_case(b"ENDSEC") {
                        self.expect(b';')?;
                        return Ok(());
                    }
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn simple_record(&mut self) -> Result<Entity, StepError> {
        let kw = self.keyword()?;
        let name = self.intern(kw);
        self.expect(b'(')?;
        let params = self.params_after_open()?;
        Ok(Entity { name, params })
    }

    fn unsigned(&mut self) -> Result<u64, StepError> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let digits = self.src.get(start..self.pos).unwrap_or_default();
        std::str::from_utf8(digits)
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| self.error_at(start, "expected an instance number"))
    }

    /// Parameters after an opening parenthesis, up to and including the closing one.
    fn params_after_open(&mut self) -> Result<Vec<Value>, StepError> {
        let mut out = Vec::new();
        self.skip_ws()?;
        if self.peek() == Some(b')') {
            self.pos += 1;
            return Ok(out);
        }
        loop {
            out.push(self.value()?);
            self.skip_ws()?;
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b')') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => return Err(self.error("expected ',' or ')' in parameter list")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, StepError> {
        self.skip_ws()?;
        let Some(c) = self.peek() else { return Err(self.error("unexpected end of file in parameters")) };
        match c {
            b'$' => {
                self.pos += 1;
                Ok(Value::Null)
            }
            b'*' => {
                self.pos += 1;
                Ok(Value::Derived)
            }
            b'#' => {
                self.pos += 1;
                Ok(Value::Ref(self.unsigned()?))
            }
            b'@' => {
                // Constant instance or value names are not used by IFC; read and ignore.
                self.pos += 1;
                let _ = self.keyword();
                Ok(Value::Null)
            }
            b'\'' => {
                self.pos += 1;
                let raw = self.string_bytes()?;
                let text = match String::from_utf8(raw) {
                    Ok(s) => s,
                    // Not UTF-8: read the bytes as ISO 8859-1, the Part 21 default.
                    Err(e) => e.into_bytes().iter().map(|&b| char::from(b)).collect(),
                };
                Ok(Value::String(decode_string(&text)))
            }
            b'"' => {
                self.pos += 1;
                let start = self.pos;
                while self.peek().is_some_and(|c| c != b'"') {
                    self.pos += 1;
                }
                let hex =
                    String::from_utf8_lossy(self.src.get(start..self.pos).unwrap_or_default()).into_owned();
                self.expect(b'"')?;
                Ok(Value::Binary(hex))
            }
            b'.' => {
                self.pos += 1;
                let start = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                    self.pos += 1;
                }
                let raw = self.src.get(start..self.pos).unwrap_or_default();
                if self.peek() != Some(b'.') {
                    return Err(self.error("unterminated enumeration value"));
                }
                self.pos += 1;
                Ok(Value::Enum(String::from_utf8_lossy(raw).to_ascii_uppercase()))
            }
            b'(' => {
                self.pos += 1;
                Ok(Value::List(self.params_after_open()?))
            }
            b'0'..=b'9' | b'+' | b'-' => self.number(),
            c if c.is_ascii_alphabetic() || c == b'_' || c == b'!' => {
                let kw = self.keyword()?;
                let name = self.intern(kw);
                self.expect(b'(')?;
                let mut inner = self.params_after_open()?;
                let value =
                    if inner.len() == 1 { inner.pop().unwrap_or(Value::Null) } else { Value::List(inner) };
                Ok(Value::Typed(name, Box::new(value)))
            }
            other => Err(self.error(&format!("unexpected character '{}'", other as char))),
        }
    }

    /// String content after the opening apostrophe, with doubled apostrophes undoubled.
    fn string_bytes(&mut self) -> Result<Vec<u8>, StepError> {
        let start = self.pos;
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.error_at(start, "unterminated string")),
                Some(b'\'') => {
                    if self.src.get(self.pos + 1) == Some(&b'\'') {
                        out.push(b'\'');
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                        return Ok(out);
                    }
                }
                Some(b) => {
                    out.push(b);
                    self.pos += 1;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Value, StepError> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let mut digits = 0;
        let mut real = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => digits += 1,
                b'.' => real = true,
                b'E' | b'e' => {
                    real = true;
                    if matches!(self.src.get(self.pos + 1), Some(b'+' | b'-')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
            self.pos += 1;
        }
        if digits == 0 {
            return Err(self.error_at(start, "malformed number"));
        }
        let raw = String::from_utf8_lossy(self.src.get(start..self.pos).unwrap_or_default()).into_owned();
        if !real && let Ok(i) = raw.parse::<i64>() {
            return Ok(Value::Integer(i));
        }
        // Part 21 allows "1." and "1.E5"; make the mantissa explicit for the standard library parser.
        let normalised = raw.replace(".E", ".0E").replace(".e", ".0e");
        let normalised = if normalised.ends_with('.') { format!("{normalised}0") } else { normalised };
        normalised
            .parse::<f64>()
            .map(Value::Real)
            .map_err(|_| self.error_at(start, &format!("malformed number {raw}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [x]'),'2;1');\n\
        FILE_NAME('a.ifc','2026-10-09T12:00:00',(''),(''),'p','s','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\n\
        DATA;\n/* a comment */\n#1=IFCCARTESIANPOINT((0.,1.5,-2.E-3));\n\
        #2=IFCWALL('2O2Fr$t4X7Zf8NOew3FLOH',$,'W\\X2\\0391\\X0\\',$,$,#1,$,'it''s',.NOTDEFINED.);\n\
        #3=IFCPROPERTYSINGLEVALUE('A',$,IFCLABEL('x'),$);\n#4=(IFCA(1)IFCB(*,#1));\n\
        #5=ifclowercase(.t.,+12,\"0F\",IFCCOMPLEX((1,2)));\nENDSEC;\nEND-ISO-10303-21;\n";

    #[test]
    fn parses_a_small_file() {
        let f = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(f.schemas(), vec!["IFC4".to_owned()]);
        let p = f.get(1).unwrap();
        let coords: Vec<f64> = p.list_at(0).unwrap().iter().filter_map(Value::as_f64).collect();
        assert_eq!(coords.len(), 3);
        assert!((coords[1] - 1.5).abs() < 1e-12 && (coords[2] + 0.002).abs() < 1e-12);
        let w = f.get(2).unwrap();
        assert_eq!(w.name.as_ref(), "IFCWALL");
        assert_eq!(w.str_at(2), Some("WΑ"));
        assert_eq!(w.str_at(7), Some("it's"));
        assert_eq!(w.ref_at(5), Some(1));
        assert!(w.attr(1).unwrap().is_null());
        assert_eq!(w.enum_at(8), Some("NOTDEFINED"));
        let v = f.get(3).unwrap().attr(2).unwrap();
        assert_eq!(v.type_name(), Some("IFCLABEL"));
        assert_eq!(v.as_str(), Some("x"));
        assert_eq!(f.data.get(&4).unwrap().parts().len(), 2);
        assert!(f.get(4).is_none());
        let l = f.get(5).unwrap();
        assert_eq!(l.name.as_ref(), "IFCLOWERCASE");
        assert_eq!(l.attr(0).unwrap().as_bool(), Some(true));
        assert_eq!(l.attr(1).unwrap().as_i64(), Some(12));
        assert_eq!(l.attr(2), Some(&Value::Binary("0F".into())));
        assert_eq!(l.attr(3).unwrap().as_list().map(<[Value]>::len), Some(2));
        assert_eq!(f.of_type("IFCWALL"), &[2]);
        assert!(f.dangling_references().is_empty());
        assert_eq!(f.type_counts().get("IFCA"), Some(&1));
    }

    #[test]
    fn reports_positions_of_errors() {
        let bad = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=IFCX(1,,2);\nENDSEC;\nEND-ISO-10303-21;";
        let e = parse(bad.as_bytes()).unwrap_err();
        assert_eq!(e.line, 5);
        assert!(e.column > 5, "{e}");
        assert!(parse(b"hello").is_err());
        let dup = "ISO-10303-21;HEADER;ENDSEC;DATA;#1=A();#1=B();ENDSEC;END-ISO-10303-21;";
        assert!(parse(dup.as_bytes()).unwrap_err().message.contains("twice"));
        let unterminated = "ISO-10303-21;HEADER;ENDSEC;DATA;#1=A('abc);ENDSEC;END-ISO-10303-21;";
        assert!(parse(unterminated.as_bytes()).is_err());
    }

    #[test]
    fn reads_latin1_and_utf8_bytes_in_strings() {
        let mut bytes = b"ISO-10303-21;HEADER;ENDSEC;DATA;#1=A('caf".to_vec();
        bytes.push(0xE9);
        bytes.extend_from_slice(b"','\xCE\xA9');ENDSEC;END-ISO-10303-21;");
        let f = parse(&bytes).unwrap();
        // A lone 0xE9 is not UTF-8, so the whole string is read as ISO 8859-1.
        assert_eq!(f.get(1).unwrap().str_at(0), Some("café"));
        assert_eq!(f.get(1).unwrap().str_at(1), Some("Ω"));
    }
}
