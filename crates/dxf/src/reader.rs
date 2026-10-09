//! Reading the structure of an ASCII DXF file: group code / value pairs, sections, table records, blocks and
//! entities.

use std::collections::HashMap;

use crate::DxfError;
use crate::codepage::{Codepage, decode_unicode_escapes};

/// One group code / value pair. The value is kept as raw bytes until its type is known.
#[derive(Debug, Clone, Copy)]
pub struct Pair<'a> {
    /// Group code.
    pub code: i32,
    /// Raw value bytes (line terminator removed).
    pub raw: &'a [u8],
    /// Line number of the code (1-based), for messages.
    pub line: usize,
}

impl Pair<'_> {
    /// The value as a real number.
    pub fn f64(&self) -> Option<f64> {
        std::str::from_utf8(self.raw).ok()?.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    }
    /// The value as an integer (accepting a real written for an integer code).
    pub fn i64(&self) -> Option<i64> {
        let s = std::str::from_utf8(self.raw).ok()?.trim();
        s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v as i64))
    }
    /// The value as an ASCII keyword (trimmed, upper case).
    pub fn keyword(&self) -> String {
        String::from_utf8_lossy(self.raw).trim().to_ascii_uppercase()
    }
}

/// Splits a file into group code / value pairs. Stops at `0 EOF`.
pub fn tokenize(data: &[u8]) -> Result<Vec<Pair<'_>>, DxfError> {
    let head = data.get(..data.len().min(32)).unwrap_or(data);
    if head.windows(10).any(|w| w == b"Binary DXF") {
        return Err(DxfError::Binary);
    }
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    let mut lines = data.split(|b| *b == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)).enumerate();
    let mut pairs = Vec::new();
    while let Some((n, code_line)) = lines.next() {
        let code_text = String::from_utf8_lossy(code_line);
        let code_text = code_text.trim();
        if code_text.is_empty() && pairs.is_empty() {
            continue; // leading blank lines
        }
        let Some((_, value)) = lines.next() else { break };
        let code: i32 = match code_text.parse() {
            Ok(c) => c,
            Err(_) if code_text.is_empty() => break, // trailing blank lines
            Err(_) => {
                return Err(DxfError::Malformed {
                    line: n + 1,
                    reason: format!("expected a group code, found {code_text:?}"),
                });
            }
        };
        let pair = Pair { code, raw: value, line: n + 1 };
        if code == 0 && pair.keyword() == "EOF" {
            break;
        }
        pairs.push(pair);
    }
    if pairs.is_empty() {
        return Err(DxfError::Empty);
    }
    Ok(pairs)
}

/// Decodes text values according to the file's version and code page.
#[derive(Debug, Clone, Copy)]
pub struct Decoder {
    /// Files from AC1021 on are UTF-8.
    pub utf8: bool,
    /// Code page for older files.
    pub codepage: Codepage,
}

impl Decoder {
    /// A text value, with `\U+XXXX` escapes resolved. Bytes that form valid UTF-8 with non-ASCII characters are
    /// read as UTF-8 even in older files (some writers ignore the code page); Greek in Windows-1253 is never valid
    /// UTF-8, so this does not misread it.
    pub fn text(&self, raw: &[u8]) -> String {
        let s = if self.utf8 || raw.is_ascii() {
            String::from_utf8_lossy(raw).into_owned()
        } else {
            match std::str::from_utf8(raw) {
                Ok(s) => s.to_owned(),
                Err(_) => self.codepage.decode(raw),
            }
        };
        decode_unicode_escapes(&s)
    }
}

/// A section: its name and its pairs (between `2 NAME` and `0 ENDSEC`).
pub struct Section<'a> {
    /// Upper-case section name.
    pub name: String,
    /// Pairs of the section body.
    pub pairs: &'a [Pair<'a>],
}

/// Splits pairs into sections. Pairs outside sections are ignored.
pub fn sections<'a>(pairs: &'a [Pair<'a>]) -> Vec<Section<'a>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = pairs.get(i) {
        if p.code == 0 && p.keyword() == "SECTION" {
            let name = pairs.get(i + 1).filter(|q| q.code == 2).map(|q| q.keyword()).unwrap_or_default();
            let start = i + 2;
            let mut end = start;
            while let Some(q) = pairs.get(end) {
                if q.code == 0 && matches!(q.keyword().as_str(), "ENDSEC" | "SECTION") {
                    break;
                }
                end += 1;
            }
            out.push(Section { name, pairs: pairs.get(start.min(end)..end).unwrap_or(&[]) });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Header variables: name (upper case, with `$`) → pairs of its value.
pub fn header<'a>(pairs: &'a [Pair<'a>]) -> HashMap<String, Vec<Pair<'a>>> {
    let mut vars: HashMap<String, Vec<Pair<'a>>> = HashMap::new();
    let mut current: Option<String> = None;
    for p in pairs {
        if p.code == 9 {
            let name = p.keyword();
            vars.entry(name.clone()).or_default();
            current = Some(name);
        } else if let Some(v) = current.as_ref().and_then(|n| vars.get_mut(n)) {
            v.push(*p);
        }
    }
    vars
}

/// An object: a type name (code 0) and the pairs up to the next code 0, plus attached sub-entities (vertices,
/// attributes).
#[derive(Debug, Clone)]
pub struct Obj<'a> {
    /// Upper-case type name.
    pub kind: String,
    /// Pairs after the type name.
    pub pairs: &'a [Pair<'a>],
    /// Vertices of a POLYLINE, attributes of an INSERT.
    pub children: Vec<Obj<'a>>,
    /// Line of the type name.
    pub line: usize,
}

impl<'a> Obj<'a> {
    /// First value with a group code.
    pub fn get(&self, code: i32) -> Option<&Pair<'a>> {
        self.pairs.iter().find(|p| p.code == code)
    }
    /// First real value with a group code.
    pub fn f(&self, code: i32) -> Option<f64> {
        self.get(code).and_then(Pair::f64)
    }
    /// First real value with a group code, or a default.
    pub fn f_or(&self, code: i32, default: f64) -> f64 {
        self.f(code).unwrap_or(default)
    }
    /// First integer value with a group code.
    pub fn i(&self, code: i32) -> Option<i64> {
        self.get(code).and_then(Pair::i64)
    }
    /// First text value with a group code.
    pub fn s(&self, code: i32, dec: &Decoder) -> Option<String> {
        self.get(code).map(|p| dec.text(p.raw))
    }
    /// A point from codes `c`, `c + 10`, `c + 20` (missing coordinates are 0).
    pub fn point(&self, c: i32) -> Option<[f64; 3]> {
        let x = self.f(c)?;
        Some([x, self.f(c + 10).unwrap_or(0.0), self.f(c + 20).unwrap_or(0.0)])
    }
    /// Extrusion direction (codes 210/220/230), default +Z.
    pub fn extrusion(&self) -> [f64; 3] {
        [self.f_or(210, 0.0), self.f_or(220, 0.0), self.f_or(230, 1.0)]
    }
}

/// Splits pairs into objects at every code 0.
pub fn objects<'a>(pairs: &'a [Pair<'a>]) -> Vec<Obj<'a>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = pairs.get(i) {
        if p.code != 0 {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while pairs.get(end).is_some_and(|q| q.code != 0) {
            end += 1;
        }
        out.push(Obj {
            kind: p.keyword(),
            pairs: pairs.get(i + 1..end).unwrap_or(&[]),
            children: Vec::new(),
            line: p.line,
        });
        i = end;
    }
    out
}

/// Attaches VERTEX objects to their POLYLINE and ATTRIB objects to their INSERT, dropping SEQEND.
pub fn group_entities(objs: Vec<Obj<'_>>) -> Vec<Obj<'_>> {
    let mut out: Vec<Obj<'_>> = Vec::with_capacity(objs.len());
    let mut open = false;
    for o in objs {
        let attach = match o.kind.as_str() {
            "VERTEX" => out.last().is_some_and(|l| l.kind == "POLYLINE") && open,
            "ATTRIB" => out.last().is_some_and(|l| matches!(l.kind.as_str(), "INSERT" | "MINSERT")) && open,
            "SEQEND" => {
                open = false;
                continue;
            }
            _ => false,
        };
        if attach {
            if let Some(last) = out.last_mut() {
                last.children.push(o);
            }
            continue;
        }
        open = matches!(o.kind.as_str(), "POLYLINE" | "INSERT" | "MINSERT");
        out.push(o);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_with_mixed_line_endings_and_padding() {
        let data =
            b"  0\r\nSECTION\n  2\r\nENTITIES\r\n0\nLINE\n 10\n1.5\n  0\nENDSEC\n  0\nEOF\ntrailing garbage";
        let p = tokenize(data).unwrap();
        assert_eq!(p.len(), 5);
        assert_eq!(p[3].code, 10);
        assert_eq!(p[3].f64(), Some(1.5));
        let s = sections(&p);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].name, "ENTITIES");
        assert_eq!(objects(s[0].pairs)[0].kind, "LINE");
    }

    #[test]
    fn rejects_binary_and_garbage() {
        // Binary files start with a sentinel line that contains "Binary DXF".
        assert!(matches!(tokenize(b"Program Binary DXF\r\n\x1a\0rest"), Err(DxfError::Binary)));
        assert!(matches!(tokenize(b"hello\nworld\n"), Err(DxfError::Malformed { line: 1, .. })));
        assert!(matches!(tokenize(b"\n\n"), Err(DxfError::Empty)));
    }

    #[test]
    fn decoder_prefers_valid_utf8_then_code_page() {
        let d = Decoder { utf8: false, codepage: Codepage::Ansi1253 };
        assert_eq!(d.text(&[0xC1, 0xE2]), "Αβ");
        assert_eq!(d.text("Αβ".as_bytes()), "Αβ");
        assert_eq!(d.text(b"\\U+0393"), "Γ");
    }

    #[test]
    fn polylines_collect_vertices_and_inserts_attributes() {
        let data = b"0\nPOLYLINE\n66\n1\n0\nVERTEX\n10\n0\n0\nVERTEX\n10\n1\n0\nSEQEND\n0\nINSERT\n2\nB\n66\n1\n0\nATTRIB\n1\nx\n0\nSEQEND\n0\nLINE\n";
        let p = tokenize(data).unwrap();
        let g = group_entities(objects(&p));
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].children.len(), 2);
        assert_eq!(g[1].children.len(), 1);
        assert!(g[2].children.is_empty());
    }
}
