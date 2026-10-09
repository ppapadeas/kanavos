//! String encoding of ISO 10303-21 (clause 6.4.3 and the control directives `\S\`, `\P?\`, `\X\`, `\X2\`,
//! `\X4\`).

/// Encodes text as a Part 21 string literal, including the surrounding apostrophes.
///
/// Printable ASCII is written as is (apostrophes doubled, backslashes doubled). Every other character is written
/// with `\X2\…\X0\` (characters in the Basic Multilingual Plane, as four hexadecimal digits each) or
/// `\X4\…\X0\` (other characters, eight digits each), so the output is pure ASCII.
pub fn encode_string(s: &str) -> String {
    #[derive(PartialEq, Clone, Copy)]
    enum Run {
        None,
        X2,
        X4,
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    let mut run = Run::None;
    for c in s.chars() {
        let wanted = match c {
            ' '..='~' => Run::None,
            c if (c as u32) <= 0xFFFF => Run::X2,
            _ => Run::X4,
        };
        if wanted != run {
            if run != Run::None {
                out.push_str("\\X0\\");
            }
            match wanted {
                Run::X2 => out.push_str("\\X2\\"),
                Run::X4 => out.push_str("\\X4\\"),
                Run::None => {}
            }
            run = wanted;
        }
        match wanted {
            Run::None => match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            },
            Run::X2 => out.push_str(&format!("{:04X}", c as u32)),
            Run::X4 => out.push_str(&format!("{:08X}", c as u32)),
        }
    }
    if run != Run::None {
        out.push_str("\\X0\\");
    }
    out.push('\'');
    out
}

/// Code pages selectable with `\P?\`; `\S\` adds 128 to the next character in the current page.
#[derive(Clone, Copy, PartialEq)]
enum CodePage {
    /// ISO 8859-1 (`\PA\`, the default).
    Latin1,
    /// ISO 8859-7 (`\PG\`), Greek.
    Greek,
}

/// Maps an upper-half byte (0x80–0xFF) of a code page to a character.
fn upper_half(page: CodePage, byte: u32) -> char {
    match page {
        CodePage::Latin1 => char::from_u32(byte).unwrap_or('\u{FFFD}'),
        CodePage::Greek => greek(byte),
    }
}

/// ISO 8859-7 (2003 edition), upper half.
fn greek(byte: u32) -> char {
    let code = match byte {
        0xA1 => 0x2018,
        0xA2 => 0x2019,
        0xA4 => 0x20AC,
        0xA5 => 0x20AF,
        0xAA => 0x037A,
        0xAF => 0x2015,
        0xAE | 0xD2 | 0xFF => 0xFFFD,
        0xA0 | 0xA3 | 0xA6..=0xA9 | 0xAB..=0xAD | 0xB0..=0xB3 | 0xB7 | 0xBB | 0xBD => byte,
        0xB4..=0xFE => byte + 0x2D0,
        _ => byte,
    };
    char::from_u32(code).unwrap_or('\u{FFFD}')
}

fn hex_at(chars: &[char], at: usize, digits: usize) -> Option<u32> {
    let mut v = 0u32;
    for k in 0..digits {
        let d = chars.get(at + k)?.to_digit(16)?;
        v = v.checked_mul(16)?.checked_add(d)?;
    }
    Some(v)
}

fn is_at(chars: &[char], at: usize, pat: &str) -> bool {
    pat.chars().enumerate().all(|(k, p)| chars.get(at + k) == Some(&p))
}

/// Decodes the control directives in the content of a Part 21 string (apostrophes already undoubled).
///
/// Unknown or malformed directives are kept literally rather than rejected, because real files contain them.
pub fn decode_string(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut page = CodePage::Latin1;
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        if is_at(&chars, i, "\\\\") {
            out.push('\\');
            i += 2;
        } else if is_at(&chars, i, "\\S\\") && chars.get(i + 3).is_some() {
            let base = chars.get(i + 3).map(|&c| c as u32).unwrap_or(0);
            out.push(upper_half(page, (base & 0x7F) + 0x80));
            i += 4;
        } else if chars.get(i + 1) == Some(&'P')
            && chars.get(i + 3) == Some(&'\\')
            && chars.get(i + 2).is_some_and(|p| ('A'..='I').contains(p))
        {
            page = if chars.get(i + 2) == Some(&'G') { CodePage::Greek } else { CodePage::Latin1 };
            i += 4;
        } else if is_at(&chars, i, "\\X2\\") {
            let mut j = i + 4;
            let mut units = Vec::new();
            while let Some(u) = hex_at(&chars, j, 4) {
                units.push(u as u16);
                j += 4;
            }
            if is_at(&chars, j, "\\X0\\") {
                out.extend(char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')));
                i = j + 4;
            } else {
                out.push('\\');
                i += 1;
            }
        } else if is_at(&chars, i, "\\X4\\") {
            let mut j = i + 4;
            let mut decoded = String::new();
            while let Some(u) = hex_at(&chars, j, 8) {
                decoded.push(char::from_u32(u).unwrap_or('\u{FFFD}'));
                j += 8;
            }
            if is_at(&chars, j, "\\X0\\") {
                out.push_str(&decoded);
                i = j + 4;
            } else {
                out.push('\\');
                i += 1;
            }
        } else if is_at(&chars, i, "\\X\\")
            && let Some(byte) = hex_at(&chars, i + 3, 2)
        {
            out.push(char::from_u32(byte).unwrap_or('\u{FFFD}'));
            i += 5;
        } else {
            out.push('\\');
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_kept_with_doubled_quotes_and_backslashes() {
        assert_eq!(encode_string("it's a\\b"), "'it''s a\\\\b'");
    }

    #[test]
    fn greek_uses_x2() {
        assert_eq!(encode_string("Τοίχος 1"), "'\\X2\\03A403BF03AF03C703BF03C2\\X0\\ 1'");
    }

    #[test]
    fn non_bmp_and_controls() {
        assert_eq!(encode_string("a\n😀"), "'a\\X2\\000A\\X0\\\\X4\\0001F600\\X0\\'");
    }

    #[test]
    fn round_trips() {
        for s in ["", "plain", "Καθιστικό 0.01", "a'b\\c", "line\nbreak\ttab", "mixed Ωmega😀end", "½ «»"]
        {
            let enc = encode_string(s);
            assert!(enc.is_ascii(), "{enc}");
            let inner = &enc[1..enc.len() - 1];
            let unquoted = inner.replace("''", "'");
            assert_eq!(decode_string(&unquoted), s);
        }
    }

    #[test]
    fn decodes_legacy_directives() {
        // \S\ adds 128: 'D' (0x44) → 0xC4 'Ä' in ISO 8859-1.
        assert_eq!(decode_string("\\S\\Dpfel"), "Äpfel");
        // \PG\ switches to ISO 8859-7: 'A' (0x41) → 0xC1 'Α'.
        assert_eq!(decode_string("\\PG\\\\S\\A\\S\\B"), "ΑΒ");
        assert_eq!(decode_string("caf\\X\\E9"), "café");
        assert_eq!(decode_string("\\X2\\03A9\\X0\\"), "Ω");
        // Surrogate pair inside \X2\ (seen in practice).
        assert_eq!(decode_string("\\X2\\D83DDE00\\X0\\"), "😀");
        // Malformed directives stay literal.
        assert_eq!(decode_string("a\\X2\\12"), "a\\X2\\12");
        assert_eq!(decode_string("C:\\\\dir"), "C:\\dir");
    }
}
