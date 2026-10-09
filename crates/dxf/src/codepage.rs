//! Text encodings used in DXF files.
//!
//! Before format version AC1021 a DXF file stores text in the code page named by its `$DWGCODEPAGE` header
//! variable; characters outside it are written as `\U+XXXX` escapes. From AC1021 on, files are UTF-8. Greek surveys
//! commonly use `ANSI_1253` (Windows-1253); older ones `DOS737`.
//!
//! The single-byte tables below follow the Unicode Consortium's published mapping files for these code pages
//! (unicode.org, `MAPPINGS/VENDORS/MICSFT` and `MAPPINGS/ISO8859`). Bytes a code page leaves undefined decode to
//! U+FFFD.

/// A text encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codepage {
    /// UTF-8 (DXF AC1021 and later).
    Utf8,
    /// Windows-1253, Greek (`ANSI_1253`).
    Ansi1253,
    /// Windows-1252, Western European (`ANSI_1252`).
    Ansi1252,
    /// ISO 8859-7, Greek (2003 edition).
    Iso8859_7,
    /// DOS code page 737, Greek.
    Dos737,
}

const UNDEF: u16 = 0xFFFD;

/// Windows-1253, bytes 0x80–0xBF (0xC0–0xFE follow U+0390 + offset, 0xD2 and 0xFF undefined).
const CP1253_80: [u16; 64] = [
    0x20AC, UNDEF, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, UNDEF, 0x2030, UNDEF, 0x2039, UNDEF,
    UNDEF, UNDEF, UNDEF, UNDEF, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, UNDEF, 0x2122, UNDEF,
    0x203A, UNDEF, UNDEF, UNDEF, UNDEF, 0x00A0, 0x0385, 0x0386, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7,
    0x00A8, 0x00A9, UNDEF, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x2015, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x0384,
    0x00B5, 0x00B6, 0x00B7, 0x0388, 0x0389, 0x038A, 0x00BB, 0x038C, 0x00BD, 0x038E, 0x038F,
];

/// ISO 8859-7, bytes 0xA0–0xBF (0x80–0x9F are C1 controls; 0xC0–0xFE as Windows-1253).
const ISO8859_7_A0: [u16; 32] = [
    0x00A0, 0x2018, 0x2019, 0x00A3, 0x20AC, 0x20AF, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x037A, 0x00AB, 0x00AC,
    0x00AD, UNDEF, 0x2015, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x0384, 0x0385, 0x0386, 0x00B7, 0x0388, 0x0389,
    0x038A, 0x00BB, 0x038C, 0x00BD, 0x038E, 0x038F,
];

/// Windows-1252, bytes 0x80–0x9F (0xA0–0xFF are Latin-1).
const CP1252_80: [u16; 32] = [
    0x20AC, UNDEF, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152,
    UNDEF, 0x017D, UNDEF, UNDEF, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122,
    0x0161, 0x203A, 0x0153, UNDEF, 0x017E, 0x0178,
];

/// DOS 737, bytes 0xE0–0xFF (0x80–0xAF are Greek letters, 0xB0–0xDF box drawing).
const CP737_E0: [u16; 32] = [
    0x03C9, 0x03AC, 0x03AD, 0x03AE, 0x03CA, 0x03AF, 0x03CC, 0x03CD, 0x03CB, 0x03CE, 0x0386, 0x0388, 0x0389,
    0x038A, 0x038C, 0x038E, 0x038F, 0x00B1, 0x2265, 0x2264, 0x03AA, 0x03AB, 0x00F7, 0x2248, 0x00B0, 0x2219,
    0x00B7, 0x221A, 0x207F, 0x00B2, 0x25A0, 0x00A0,
];

fn greek_c0(b: u8) -> u16 {
    // 0xC0–0xFE map to U+0390–U+03CE; 0xD2 (U+03A2) is unassigned and 0xFF undefined.
    if b == 0xD2 || b == 0xFF { UNDEF } else { 0x0390 + u16::from(b - 0xC0) }
}

fn cp737_letter(b: u8) -> u16 {
    match b {
        // Capitals Α–Ρ, then Σ–Ω (skipping the unassigned U+03A2).
        0x80..=0x90 => 0x0391 + u16::from(b - 0x80),
        0x91..=0x97 => 0x03A3 + u16::from(b - 0x91),
        // Lowercase α–ρ, σ, ς, τ–ψ.
        0x98..=0xA8 => 0x03B1 + u16::from(b - 0x98),
        0xA9 => 0x03C3,
        0xAA => 0x03C2,
        0xAB..=0xAF => 0x03C4 + u16::from(b - 0xAB),
        _ => UNDEF,
    }
}

impl Codepage {
    /// The code page for a `$DWGCODEPAGE` value such as `ANSI_1253` (case-insensitive), if supported.
    pub fn from_dxf_name(name: &str) -> Option<Codepage> {
        let n = name.trim().to_ascii_uppercase().replace(['-', '_', ' '], "");
        match n.as_str() {
            "ANSI1253" | "CP1253" | "WINDOWS1253" => Some(Codepage::Ansi1253),
            "ANSI1252" | "CP1252" | "WINDOWS1252" => Some(Codepage::Ansi1252),
            "ISO88597" | "ANSI28597" => Some(Codepage::Iso8859_7),
            "DOS737" | "CP737" => Some(Codepage::Dos737),
            "UTF8" => Some(Codepage::Utf8),
            _ => None,
        }
    }

    /// The `$DWGCODEPAGE` value written for this code page.
    pub fn dxf_name(self) -> &'static str {
        match self {
            Codepage::Utf8 => "UTF-8",
            Codepage::Ansi1253 => "ANSI_1253",
            Codepage::Ansi1252 => "ANSI_1252",
            Codepage::Iso8859_7 => "ISO8859-7",
            Codepage::Dos737 => "DOS737",
        }
    }

    /// Unicode scalar for one byte (bytes below 0x80 are ASCII in every supported code page).
    fn decode_byte(self, b: u8) -> u16 {
        if b < 0x80 {
            return u16::from(b);
        }
        let idx = usize::from(b & 0x7F);
        match self {
            Codepage::Utf8 => UNDEF,
            Codepage::Ansi1253 => {
                if b >= 0xC0 {
                    greek_c0(b)
                } else {
                    CP1253_80.get(idx).copied().unwrap_or(UNDEF)
                }
            }
            Codepage::Ansi1252 => {
                if b >= 0xA0 {
                    u16::from(b)
                } else {
                    CP1252_80.get(idx).copied().unwrap_or(UNDEF)
                }
            }
            Codepage::Iso8859_7 => match b {
                0x80..=0x9F => u16::from(b),
                0xA0..=0xBF => ISO8859_7_A0.get(usize::from(b - 0xA0)).copied().unwrap_or(UNDEF),
                _ => greek_c0(b),
            },
            Codepage::Dos737 => match b {
                0x80..=0xAF => cp737_letter(b),
                0xE0..=0xFF => CP737_E0.get(usize::from(b - 0xE0)).copied().unwrap_or(UNDEF),
                // Box-drawing characters have no place in drawing text.
                _ => UNDEF,
            },
        }
    }

    /// Decodes bytes to text (no `\U+` handling; see [`decode_unicode_escapes`]).
    pub fn decode(self, bytes: &[u8]) -> String {
        if self == Codepage::Utf8 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        bytes.iter().map(|b| char::from_u32(u32::from(self.decode_byte(*b))).unwrap_or('\u{FFFD}')).collect()
    }

    /// The byte for a character in this code page, if it has one.
    pub fn encode_char(self, c: char) -> Option<u8> {
        if c.is_ascii() {
            return Some(c as u8);
        }
        if self == Codepage::Utf8 {
            return None;
        }
        let u = u32::from(c);
        (0x80u8..=0xFF).find(|b| u32::from(self.decode_byte(*b)) == u && u != u32::from(UNDEF))
    }
}

/// Replaces DXF `\U+XXXX` escapes with the characters they name. Malformed escapes are kept as written.
pub fn decode_unicode_escapes(s: &str) -> String {
    if !s.contains("\\U+") && !s.contains("\\u+") {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c == '\\'
            && matches!(chars.get(i + 1), Some('U' | 'u'))
            && chars.get(i + 2) == Some(&'+')
            && let Some(hex) = chars.get(i + 3..i + 7)
        {
            let h: String = hex.iter().collect();
            if let Some(ch) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) {
                out.push(ch);
                i += 7;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Encodes text for a DXF file in a code page: characters the code page has are written as bytes, others as
/// `\U+XXXX` escapes (characters beyond U+FFFF become `?`). Control characters are dropped.
pub fn encode_dxf_text(s: &str, cp: Codepage) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            continue;
        }
        if cp == Codepage::Utf8 {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        } else if let Some(b) = cp.encode_char(c) {
            out.push(b);
        } else if (c as u32) <= 0xFFFF {
            out.extend_from_slice(format!("\\U+{:04X}", c as u32).as_bytes());
        } else {
            out.push(b'?');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_1253_greek() {
        // "Οικόπεδο" in Windows-1253.
        let bytes = [0xCF, 0xE9, 0xEA, 0xFC, 0xF0, 0xE5, 0xE4, 0xEF];
        assert_eq!(Codepage::Ansi1253.decode(&bytes), "Οικόπεδο");
        // Accented capitals and symbols.
        assert_eq!(Codepage::Ansi1253.decode(&[0xA2, 0xB8, 0xBF, 0xB0, 0xB1, 0xB2]), "ΆΈΏ°±²");
        assert_eq!(Codepage::Ansi1253.decode(&[0xD2, 0xFF]), "\u{FFFD}\u{FFFD}");
        assert_eq!(Codepage::Ansi1253.decode(&[0xF2, 0xF3, 0xC0, 0xE0]), "ςσΐΰ");
    }

    #[test]
    fn other_greek_code_pages() {
        assert_eq!(Codepage::Iso8859_7.decode(&[0xB6, 0xEB, 0xF6, 0xE1]), "Άλφα");
        // DOS 737: "Κάτοψη".
        assert_eq!(Codepage::Dos737.decode(&[0x89, 0xE1, 0xAB, 0xA6, 0xAF, 0x9E]), "Κάτοψη");
        assert_eq!(Codepage::Dos737.decode(&[0x97, 0xE0, 0xF8]), "Ωω°");
        assert_eq!(Codepage::Ansi1252.decode(&[0xE9, 0x80, 0x8D]), "é€\u{FFFD}");
    }

    #[test]
    fn names_and_round_trip() {
        assert_eq!(Codepage::from_dxf_name("ansi_1253"), Some(Codepage::Ansi1253));
        assert_eq!(Codepage::from_dxf_name("ISO8859-7"), Some(Codepage::Iso8859_7));
        assert_eq!(Codepage::from_dxf_name("DOS737"), Some(Codepage::Dos737));
        assert_eq!(Codepage::from_dxf_name("ANSI_932"), None);
        for cp in [Codepage::Ansi1253, Codepage::Iso8859_7, Codepage::Dos737] {
            let text = "Κάτοψη ΆΈΉ ϊΐ ±°";
            let enc = encode_dxf_text(text, cp);
            assert_eq!(decode_unicode_escapes(&cp.decode(&enc)), text, "{cp:?}");
        }
    }

    #[test]
    fn unicode_escapes() {
        assert_eq!(decode_unicode_escapes("A\\U+0391B"), "AΑB");
        assert_eq!(decode_unicode_escapes("\\U+03a9\\U+zzzz\\U+12"), "Ω\\U+zzzz\\U+12");
        assert_eq!(encode_dxf_text("Ωx\u{1F600}\u{1}", Codepage::Ansi1252), b"\\U+03A9x?".to_vec());
    }
}
