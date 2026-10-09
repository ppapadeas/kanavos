//! Plain text from DXF text values: `%%` control codes (TEXT, ATTRIB) and MTEXT inline formatting.

/// Resolves `%%` control codes: `%%d` degree, `%%p` plus/minus, `%%c` diameter, `%%%` percent, `%%nnn` a character
/// by its code; `%%u`, `%%o` and `%%k` (underline, overline, strike toggles) are removed.
pub fn decode_percent_codes(s: &str) -> String {
    if !s.contains("%%") {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c == '%' && chars.get(i + 1) == Some(&'%') {
            match chars.get(i + 2).copied() {
                Some('d' | 'D') => out.push('°'),
                Some('p' | 'P') => out.push('±'),
                Some('c' | 'C') => out.push('⌀'),
                Some('%') => out.push('%'),
                Some('u' | 'U' | 'o' | 'O' | 'k' | 'K') => {}
                Some(d) if d.is_ascii_digit() => {
                    let digits: String =
                        chars.iter().skip(i + 2).take(3).take_while(|c| c.is_ascii_digit()).collect();
                    if let Some(ch) = digits.parse::<u32>().ok().and_then(char::from_u32) {
                        out.push(ch);
                    }
                    i += 2 + digits.chars().count();
                    continue;
                }
                _ => {
                    out.push_str("%%");
                    i += 2;
                    continue;
                }
            }
            i += 3;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// Stacked text `upper^lower`, `upper/lower` or `upper#lower` as one line: an empty lower part is a superscript
/// (`2^` is `²`, `3^` is `³`, others stay as written), an empty upper part a subscript, otherwise `upper/lower`.
fn stacked_text(s: &str) -> String {
    let Some(pos) = s.find(['^', '/', '#']) else { return s.trim().to_owned() };
    let (upper, lower) = (s.get(..pos).unwrap_or("").trim(), s.get(pos + 1..).unwrap_or("").trim());
    match (upper, lower) {
        ("2", "") => "²".to_owned(),
        ("3", "") => "³".to_owned(),
        (u, "") => u.to_owned(),
        ("", l) => l.to_owned(),
        (u, l) => format!("{u}/{l}"),
    }
}

/// The lines of an MTEXT value with formatting removed: paragraph breaks (`\P`), column breaks (`\N`) and caret
/// line feeds end lines; font, height, colour, width, tracking, oblique and alignment codes are dropped; stacked
/// fractions become `a/b`; grouping braces are removed; `\~` is a space; `\\`, `\{` and `\}` are literal.
pub fn mtext_lines(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut lines = vec![String::new()];
    let push = |lines: &mut Vec<String>, c: char| {
        if let Some(l) = lines.last_mut() {
            l.push(c);
        }
    };
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        match c {
            '\\' => {
                let next = chars.get(i + 1).copied();
                i += 2;
                match next {
                    Some('P' | 'N' | 'X') => lines.push(String::new()),
                    Some('~') => push(&mut lines, ' '),
                    Some(c @ ('\\' | '{' | '}')) => push(&mut lines, c),
                    Some('L' | 'l' | 'O' | 'o' | 'K' | 'k') => {}
                    Some('A' | 'C' | 'c' | 'F' | 'f' | 'H' | 'Q' | 'T' | 'W' | 'p') => {
                        while chars.get(i).is_some_and(|c| *c != ';') {
                            i += 1;
                        }
                        i += 1;
                    }
                    Some('S') => {
                        let mut stacked = String::new();
                        while let Some(&c) = chars.get(i) {
                            i += 1;
                            if c == ';' {
                                break;
                            }
                            stacked.push(c);
                        }
                        let text = stacked_text(&stacked);
                        for c in text.chars() {
                            push(&mut lines, c);
                        }
                    }
                    Some(c) => push(&mut lines, c),
                    None => {}
                }
            }
            '{' | '}' => i += 1,
            '^' => {
                match chars.get(i + 1) {
                    Some('I') => push(&mut lines, ' '),
                    Some('J') => lines.push(String::new()),
                    Some(' ') => push(&mut lines, '^'),
                    Some(_) => {}
                    None => push(&mut lines, '^'),
                }
                i += 2;
            }
            c => {
                push(&mut lines, c);
                i += 1;
            }
        }
    }
    let lines: Vec<String> = lines.into_iter().map(|l| decode_percent_codes(&l)).collect();
    // Drop trailing empty lines.
    let keep = lines.iter().rposition(|l| !l.trim().is_empty()).map(|p| p + 1).unwrap_or(0);
    lines.into_iter().take(keep).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_codes() {
        assert_eq!(decode_percent_codes("25%%dC %%p0,05 %%c12"), "25°C ±0,05 ⌀12");
        assert_eq!(decode_percent_codes("%%uΟΙΚΟΠΕΔΟ%%u 100%%%"), "ΟΙΚΟΠΕΔΟ 100%");
        assert_eq!(decode_percent_codes("%%065B %%z"), "AB %%z");
    }

    #[test]
    fn mtext_formatting_is_removed() {
        let s =
            "{\\fSans|b1|i0|c161|p34;\\H2.5;Οικόπεδο}\\PΕμβαδόν: 512,30 m\\S2^;\\P\\C1;\\Lκλίση\\l 1 \\S1/2;";
        assert_eq!(mtext_lines(s), vec!["Οικόπεδο", "Εμβαδόν: 512,30 m²", "κλίση 1 1/2"]);
        assert_eq!(mtext_lines("a\\~b\\\\c\\{d\\}^Ie"), vec!["a b\\c{d} e"]);
        assert_eq!(mtext_lines("x\\P\\P"), vec!["x"]);
        assert!(mtext_lines("").is_empty());
    }
}
