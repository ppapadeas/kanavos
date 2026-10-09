//! The built-in single-stroke lettering font.
//!
//! An original design for Kanavos, drawn on a grid where capitals are 6 units tall, lowercase x-height is 4 units
//! and descenders reach 2 units below the baseline. Proportions follow general technical-lettering practice:
//! capitals about two thirds as wide as they are tall, a pen width of one tenth of the cap height, and tabular
//! (equal-width) digits so columns of numbers line up.
//!
//! Glyphs are written in a small notation, parsed once on first use:
//!
//! - `x,y` is a point; consecutive points are joined by straight lines;
//! - `[cx,cy,rx,ry,a0,a1]` is an elliptic arc around `(cx, cy)` from angle `a0` to `a1` in degrees (counter-clockwise
//!   when `a1 > a0`), joined to the points before and after it;
//! - `|` starts a new stroke.
//!
//! Accented letters (Greek tonos and dialytika, common Latin accents) and superscripts are composed from the base
//! letters in code. Characters the font does not cover are drawn as an empty box, so text is never lost silently.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::P;

/// Height of capitals in grid units.
const CAP: f64 = 6.0;
/// Gap added after each glyph, in grid units.
const SPACING: f64 = 1.4;
/// Width of the cell that digits are centred in, in grid units.
const DIGIT_CELL: f64 = 3.4;
/// Advance of a space, in grid units.
const SPACE: f64 = 3.0;
/// Depth of descenders below the baseline, in grid units.
const DESCENT: f64 = 2.0;

/// Cap height as a fraction of the font size (em). The stroke font's em is 9 grid units: 6 for capitals, 2 for
/// descenders and 1 of line gap.
pub const CAP_HEIGHT_EM: f64 = CAP / 9.0;

/// Pen width for stroke-font text: one tenth of the cap height, or one seventh for bold.
pub fn pen_width(cap_height: f64, bold: bool) -> f64 {
    if bold { cap_height / 7.0 } else { cap_height / 10.0 }
}

/// Depth of descenders below the baseline at a cap height.
pub fn descent(cap_height: f64) -> f64 {
    cap_height * DESCENT / CAP
}

/// A glyph in grid units (capitals 6 units tall), with its left edge at x = 0.
#[derive(Debug, Clone, PartialEq)]
pub struct Glyph {
    /// Polylines drawn with the pen.
    pub strokes: Vec<Vec<P>>,
    /// Advance width.
    pub advance: f64,
}

/// Base glyph definitions in the notation described in the module documentation.
const BASE: &[(char, &str)] = &[
    // Latin capitals.
    ('A', "0,0 2,6 4,0 | 0.7,2.1 3.3,2.1"),
    ('B', "0,0 0,6 2.5,6 [2.5,4.5,1.5,1.5,90,-90] | 0,3 2.6,3 [2.6,1.5,1.6,1.5,90,-90] 0,0"),
    ('C', "[2.1,3,2.1,3,45,315]"),
    ('D', "0,0 0,6 1.6,6 [1.6,3,2.4,3,90,-90] 0,0"),
    ('E', "3.8,6 0,6 0,0 3.8,0 | 0,3 3,3"),
    ('F', "3.8,6 0,6 0,0 | 0,3 3,3"),
    ('G', "[2.1,3,2.1,3,45,360] 2.6,3"),
    ('H', "0,0 0,6 | 4,0 4,6 | 0,3 4,3"),
    ('I', "0,0 0,6"),
    ('J', "3,6 3,1.6 [1.5,1.6,1.5,1.6,0,-165]"),
    ('K', "0,0 0,6 | 4,6 0,2 | 1.4,3.4 4,0"),
    ('L', "0,6 0,0 3.6,0"),
    ('M', "0,0 0,6 2.5,1.6 5,6 5,0"),
    ('N', "0,0 0,6 4,0 4,6"),
    ('O', "[2.1,3,2.1,3,0,360]"),
    ('P', "0,0 0,6 2.5,6 [2.5,4.5,1.5,1.5,90,-90] 0,3"),
    ('Q', "[2.1,3,2.1,3,0,360] | 2.7,1.3 4.4,-0.3"),
    ('R', "0,0 0,6 2.5,6 [2.5,4.5,1.5,1.5,90,-90] 0,3 | 2.3,3 4,0"),
    ('S', "[2,4.5,1.9,1.5,25,270] [2,1.5,2,1.5,90,-155]"),
    ('T', "0,6 4,6 | 2,6 2,0"),
    ('U', "0,6 0,2 [2,2,2,2,180,360] 4,6"),
    ('V', "0,6 2,0 4,6"),
    ('W', "0,6 1.3,0 2.6,4.6 3.9,0 5.2,6"),
    ('X', "0,0 4,6 | 0,6 4,0"),
    ('Y', "0,6 2,3 4,6 | 2,3 2,0"),
    ('Z', "0,6 4,6 0,0 4,0"),
    // Latin lowercase.
    ('a', "3,4 3,0 | [1.5,2,1.5,2,0,360]"),
    ('b', "0,6 0,0 | [1.5,2,1.5,2,0,360]"),
    ('c', "[1.6,2,1.6,2,45,315]"),
    ('d', "3,6 3,0 | [1.5,2,1.5,2,0,360]"),
    ('e', "0,2 3,2 [1.5,2,1.5,2,0,315]"),
    ('f', "2.6,6 1.8,6 [1.8,5,1,1,90,180] 0.8,0 | 0,4 2.4,4"),
    ('g', "3,4 3,-0.6 [1.5,-0.6,1.5,1.4,0,-165] | [1.5,2,1.5,2,0,360]"),
    ('h', "0,6 0,0 | 0,2.5 [1.5,2.5,1.5,1.5,180,0] 3,0"),
    ('i', "0,0 0,4 | 0,5.4 0,5.7"),
    ('j', "1,4 1,-1 [0,-1,1,1,0,-90] | 1,5.4 1,5.7"),
    ('k', "0,6 0,0 | 3,4 0,1.5 | 1,2.33 3,0"),
    ('l', "0,6 0,0"),
    ('m', "0,0 0,4 | 0,3 [1.25,3,1.25,1,180,0] 2.5,0 | 2.5,3 [3.75,3,1.25,1,180,0] 5,0"),
    ('n', "0,0 0,4 | 0,2.5 [1.5,2.5,1.5,1.5,180,0] 3,0"),
    ('o', "[1.5,2,1.5,2,0,360]"),
    ('p', "0,4 0,-2 | [1.5,2,1.5,2,0,360]"),
    ('q', "3,4 3,-2 | [1.5,2,1.5,2,0,360]"),
    ('r', "0,0 0,4 | 0,2.4 [1.7,2.4,1.7,1.6,180,40]"),
    ('s', "[1.4,3,1.3,1,20,270] [1.4,1,1.4,1,90,-155]"),
    ('t', "1,5.5 1,1 [2,1,1,1,180,270] 2.6,0 | 0,4 2.6,4"),
    ('u', "0,4 0,2 [1.5,2,1.5,2,180,360] 3,4 | 3,4 3,0"),
    ('v', "0,4 1.5,0 3,4"),
    ('w', "0,4 1.1,0 2.2,3 3.3,0 4.4,4"),
    ('x', "0,0 3,4 | 0,4 3,0"),
    ('y', "0,4 1.5,0 | 3,4 0.8,-2"),
    ('z', "0,4 3,4 0,0 3,0"),
    // Digits (centred in equal cells).
    ('0', "[1.6,3,1.6,3,0,360]"),
    ('1', "0.5,4.8 1.8,6 1.8,0"),
    ('2', "[1.6,4.4,1.5,1.6,160,-25] 0,0 3.2,0"),
    ('3', "[1.5,4.6,1.4,1.4,150,-90] [1.5,1.6,1.7,1.6,90,-150]"),
    ('4', "2.4,0 2.4,6 0,1.8 3.3,1.8"),
    ('5', "3,6 0.4,6 0.3,3.4 [1.5,2,1.7,2,135,-150]"),
    ('6', "[1.6,3,1.6,3,55,180] 0,1.8 | [1.6,1.8,1.6,1.8,0,360]"),
    ('7', "0,6 3.2,6 1,0"),
    ('8', "[1.6,4.6,1.35,1.4,0,360] | [1.6,1.6,1.6,1.6,0,360]"),
    ('9', "[1.6,4.2,1.6,1.8,0,360] | 3.2,4.2 [1.6,3,1.6,3,0,-125]"),
    // Punctuation and symbols.
    (' ', ""),
    ('.', "0,0 0,0.3"),
    (',', "0.4,0.4 0.4,0 0,-1"),
    (':', "0,0 0,0.3 | 0,3.7 0,4"),
    (';', "0.4,3.7 0.4,4 | 0.4,0.4 0.4,0 0,-1"),
    ('!', "0,6 0,1.8 | 0,0 0,0.3"),
    ('?', "[1.5,4.5,1.5,1.5,160,-80] 1.5,1.8 | 1.5,0 1.5,0.3"),
    ('-', "0,2.6 2.6,2.6"),
    ('+', "0,2.8 3.6,2.8 | 1.8,1 1.8,4.6"),
    ('=', "0,1.9 3.6,1.9 | 0,3.7 3.6,3.7"),
    ('/', "0,-0.5 3,6.5"),
    ('\\', "0,6.5 3,-0.5"),
    ('(', "[2.2,3,2.2,4.2,125,235]"),
    (')', "[-1.26,3,2.2,4.2,55,-55]"),
    ('[', "1.2,6.5 0,6.5 0,-0.5 1.2,-0.5"),
    (']', "0,6.5 1.2,6.5 1.2,-0.5 0,-0.5"),
    ('{', "1.5,6.5 0.8,6 0.8,3.6 0,3 0.8,2.4 0.8,0 1.5,-0.5"),
    ('}', "0,6.5 0.7,6 0.7,3.6 1.5,3 0.7,2.4 0.7,0 0,-0.5"),
    ('\'', "0,6 0,4.4"),
    ('"', "0,6 0,4.4 | 1.2,6 1.2,4.4"),
    ('`', "0,5.9 0.8,4.8"),
    ('%', "0,0 3.6,6 | [0.7,5,0.7,1,0,360] | [2.9,1,0.7,1,0,360]"),
    ('&', "3.6,0 0.9,4 [1.6,5,0.85,1,213,-30] 0.3,2.1 [1.4,1.2,1.2,1.2,145,330] 3.4,2.4"),
    ('*', "1.5,2.4 1.5,5.6 | 0.1,4.8 2.9,3.2 | 0.1,3.2 2.9,4.8"),
    ('#', "1,0 1.6,6 | 2.6,0 3.2,6 | 0,2 3.8,2 | 0.2,4 4,4"),
    ('_', "0,-1 4,-1"),
    ('<', "3.2,5 0,2.8 3.2,0.6"),
    ('>', "0,5 3.2,2.8 0,0.6"),
    ('|', "0,-1 0,7"),
    ('^', "0,4 1.5,6 3,4"),
    ('~', "0,2.4 0.8,3 1.6,2.7 2.4,2.4 3.2,3"),
    ('$', "[2,4.5,1.9,1.5,25,270] [2,1.5,2,1.5,90,-155] | 2,-0.8 2,6.8"),
    ('@', "[2.6,3,1,1,0,360] | 3.6,4 3.6,2.2 [4.2,2.2,0.6,0.6,180,360] [2.6,3,2.2,3,0,300]"),
    ('°', "[0.8,5.2,0.8,0.8,0,360]"),
    ('±', "0,3.6 3.6,3.6 | 1.8,1.8 1.8,5.4 | 0,0.6 3.6,0.6"),
    ('×', "0,1 3,4 | 0,4 3,1"),
    ('÷', "0,2.6 3.6,2.6 | 1.8,0.8 1.8,1.1 | 1.8,4.1 1.8,4.4"),
    ('·', "0,2.9 0,3.2"),
    ('€', "[2.6,3,2.4,3,50,310] | 0,3.6 2.8,3.6 | 0,2.4 2.6,2.4"),
    ('–', "0,2.6 4,2.6"),
    ('—', "0,2.6 6,2.6"),
    ('…', "0,0 0,0.3 | 1.5,0 1.5,0.3 | 3,0 3,0.3"),
    ('Ø', "[2.1,3,2.1,3,0,360] | -0.3,-0.3 4.5,6.3"),
    ('ø', "[1.5,2,1.5,2,0,360] | -0.2,-0.2 3.2,4.2"),
    ('⌀', "[1.8,2.8,1.8,1.8,0,360] | 0,0.5 3.6,5.1"),
    ('≤', "3.2,5.6 0,3.9 3.2,2.2 | 0,1 3.2,1"),
    ('≥', "0,5.6 3.2,3.9 0,2.2 | 0,1 3.2,1"),
    ('≈', "0,1.9 0.9,2.4 1.8,1.9 2.7,1.4 3.6,1.9 | 0,3.5 0.9,4 1.8,3.5 2.7,3 3.6,3.5"),
    ('≠', "0,1.9 3.6,1.9 | 0,3.7 3.6,3.7 | 0.8,0.4 2.8,5.2"),
    ('→', "0,2.8 4,2.8 | 2.8,4 4,2.8 2.8,1.6"),
    ('←', "4,2.8 0,2.8 | 1.2,4 0,2.8 1.2,1.6"),
    ('↑', "1.4,0 1.4,5.6 | 0,4.2 1.4,5.6 2.8,4.2"),
    ('↓', "1.4,5.6 1.4,0 | 0,1.4 1.4,0 2.8,1.4"),
    ('΄', "0,4.8 0.6,6"),
    ('΅', "0,5.2 0,5.5 | 1.6,5.2 1.6,5.5 | 0.5,5 1.1,6.1"),
    ('¨', "0,5.2 0,5.5 | 1.4,5.2 1.4,5.5"),
    ('´', "0,4.8 0.8,5.9"),
    // Greek capitals that differ from Latin ones.
    ('Γ', "3.6,6 0,6 0,0"),
    ('Δ', "0,0 2,6 4,0 0,0"),
    ('Θ', "[2.1,3,2.1,3,0,360] | 1,3 3.2,3"),
    ('Λ', "0,0 2,6 4,0"),
    ('Ξ', "0,6 4,6 | 0.6,3 3.4,3 | 0,0 4,0"),
    ('Π', "0,0 0,6 4,6 4,0"),
    ('Σ', "4,6 0,6 2.2,3 0,0 4,0"),
    ('Φ', "[2.3,3.1,2.3,1.8,0,360] | 2.3,0 2.3,6"),
    ('Ψ', "0,6 0,4.2 [2,4.2,2,2,180,360] 4,6 | 2,6 2,0"),
    ('Ω', "0,0 1.3,0 [2,3.4,2,2.6,240,-60] 2.7,0 4,0"),
    // Greek lowercase.
    ('α', "3,4 3,0.7 3.6,0 | [1.5,2,1.5,2,0,360]"),
    ('β', "0,-2 0,4.8 [1.3,4.8,1.3,1.2,180,-90] 0.7,3.6 | 0.7,3.6 1.4,3.6 [1.4,1.8,1.6,1.8,90,-140] 0,0.6"),
    ('γ', "0,4 1.5,0 | 3,4 1.5,0 1.5,-2"),
    ('δ', "[1.5,1.6,1.5,1.6,0,360] | 1.8,3.2 0.6,4.4 0.8,5.5 1.8,6 3,5.6"),
    ('ε', "[1.6,3,1.4,1,30,270] [1.6,1,1.6,1,90,330]"),
    ('ζ', "0.6,6 2.8,6 0.6,3.4 0.1,2 0.4,0.8 1.6,0.2 2.4,-0.4 2.2,-1.4 1.6,-2"),
    ('η', "0,0 0,4 | 0,2.5 [1.5,2.5,1.5,1.5,180,0] 3,-2"),
    ('θ', "[1.5,3,1.5,3,0,360] | 0,3 3,3"),
    ('ι', "0,4 0,0.8 [0.8,0.8,0.8,0.8,180,270]"),
    ('κ', "0,4 0,0 | 3,4 0,1.5 | 1,2.33 3,0"),
    ('λ', "0.5,6 3.2,0 | 1.7,3.3 0,0"),
    ('μ', "0,-2 0,4 | 0,1.6 [1.5,1.6,1.5,1.6,180,360] 3,4 | 3,4 3,0"),
    ('ν', "0,4 1.5,0 3,4"),
    ('ξ', "0.8,6 2.8,6 | 2.6,6 1,5.4 0.8,4.4 2.2,3.6 | 2.2,3.6 0.6,3 0.2,1.6 0.6,0.4 2,0 2.4,-0.6 2,-2"),
    ('π', "0,4 3.6,4 | 0.9,4 0.9,0 | 2.7,4 2.7,0.3 3.2,0"),
    ('ρ', "[1.5,2,1.5,2,0,360] | 0,2 0,-2"),
    ('σ', "3.6,4 1.5,4 [1.5,2,1.5,2,90,450]"),
    ('ς', "2.8,3.8 [1.6,2.4,1.5,1.6,55,200] 0.6,1.2 2,0.6 2.4,-0.4 1.8,-1.4"),
    ('τ', "0,4 3,4 | 1.5,4 1.5,0.8 [2.3,0.8,0.8,0.8,180,270]"),
    ('υ', "0,4 0,2 [1.5,2,1.5,2,180,360] 3,4"),
    ('φ', "[1.75,2,1.75,2,0,360] | 1.75,4.8 1.75,-2"),
    ('χ', "0,4 3,-2 | 3,4 0,-2"),
    ('ψ', "0,4 0,2.2 [1.75,2.2,1.75,2,180,360] 3.5,4 | 1.75,5 1.75,-2"),
    (
        'ω',
        "0.6,4 0,2 0,1.6 [1.3,1.6,1.3,1.6,180,360] 2.6,2.6 | 2.6,1.6 [3.9,1.6,1.3,1.6,180,360] 5.2,2 4.6,4",
    ),
];

/// Characters drawn exactly like another one.
const ALIASES: &[(char, char)] = &[
    ('Α', 'A'),
    ('Β', 'B'),
    ('Ε', 'E'),
    ('Ζ', 'Z'),
    ('Η', 'H'),
    ('Ι', 'I'),
    ('Κ', 'K'),
    ('Μ', 'M'),
    ('Ν', 'N'),
    ('Ο', 'O'),
    ('Ρ', 'P'),
    ('Τ', 'T'),
    ('Υ', 'Y'),
    ('Χ', 'X'),
    ('ο', 'o'),
    ('µ', 'μ'),
    ('\u{37e}', ';'),
    ('\u{387}', '·'),
    ('\u{2018}', '\''),
    ('\u{2019}', '\''),
    ('\u{201a}', ','),
    ('\u{201c}', '"'),
    ('\u{201d}', '"'),
    ('\u{201e}', '"'),
    ('\u{2032}', '\''),
    ('\u{2033}', '"'),
    ('\u{2212}', '-'),
    ('\u{2010}', '-'),
    ('\u{2011}', '-'),
    ('\u{2022}', '·'),
    ('\u{2205}', '⌀'),
    ('«', '<'),
    ('»', '>'),
];

/// Marks placed over or beside a base letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// Greek tonos over a lowercase letter.
    Tonos,
    /// Two dots over a lowercase letter (Greek dialytika, Latin diaeresis).
    Dots,
    /// Dialytika with tonos between the dots, lowercase.
    DotsTonos,
    /// Greek tonos before a capital (the capital moves right).
    CapTonos,
    /// Two dots over a capital.
    CapDots,
    /// Latin acute over a lowercase letter.
    Acute,
    /// Latin grave over a lowercase letter.
    Grave,
    /// Latin circumflex over a lowercase letter.
    Circumflex,
    /// Latin tilde over a lowercase letter.
    Tilde,
    /// Latin cedilla under a letter.
    Cedilla,
    /// Latin acute over a capital.
    CapAcute,
    /// Latin grave over a capital.
    CapGrave,
}

const COMPOSITES: &[(char, char, Mark)] = &[
    ('ά', 'α', Mark::Tonos),
    ('έ', 'ε', Mark::Tonos),
    ('ή', 'η', Mark::Tonos),
    ('ί', 'ι', Mark::Tonos),
    ('ό', 'ο', Mark::Tonos),
    ('ύ', 'υ', Mark::Tonos),
    ('ώ', 'ω', Mark::Tonos),
    ('ϊ', 'ι', Mark::Dots),
    ('ϋ', 'υ', Mark::Dots),
    ('ΐ', 'ι', Mark::DotsTonos),
    ('ΰ', 'υ', Mark::DotsTonos),
    ('Ά', 'Α', Mark::CapTonos),
    ('Έ', 'Ε', Mark::CapTonos),
    ('Ή', 'Η', Mark::CapTonos),
    ('Ί', 'Ι', Mark::CapTonos),
    ('Ό', 'Ο', Mark::CapTonos),
    ('Ύ', 'Υ', Mark::CapTonos),
    ('Ώ', 'Ω', Mark::CapTonos),
    ('Ϊ', 'Ι', Mark::CapDots),
    ('Ϋ', 'Υ', Mark::CapDots),
    ('á', 'a', Mark::Acute),
    ('é', 'e', Mark::Acute),
    ('í', 'ı', Mark::Acute),
    ('ó', 'o', Mark::Acute),
    ('ú', 'u', Mark::Acute),
    ('ý', 'y', Mark::Acute),
    ('à', 'a', Mark::Grave),
    ('è', 'e', Mark::Grave),
    ('ì', 'ı', Mark::Grave),
    ('ò', 'o', Mark::Grave),
    ('ù', 'u', Mark::Grave),
    ('â', 'a', Mark::Circumflex),
    ('ê', 'e', Mark::Circumflex),
    ('î', 'ı', Mark::Circumflex),
    ('ô', 'o', Mark::Circumflex),
    ('û', 'u', Mark::Circumflex),
    ('ä', 'a', Mark::Dots),
    ('ë', 'e', Mark::Dots),
    ('ï', 'ı', Mark::Dots),
    ('ö', 'o', Mark::Dots),
    ('ü', 'u', Mark::Dots),
    ('ÿ', 'y', Mark::Dots),
    ('ñ', 'n', Mark::Tilde),
    ('ã', 'a', Mark::Tilde),
    ('õ', 'o', Mark::Tilde),
    ('ç', 'c', Mark::Cedilla),
    ('Ç', 'C', Mark::Cedilla),
    ('Á', 'A', Mark::CapAcute),
    ('É', 'E', Mark::CapAcute),
    ('Í', 'I', Mark::CapAcute),
    ('Ó', 'O', Mark::CapAcute),
    ('Ú', 'U', Mark::CapAcute),
    ('À', 'A', Mark::CapGrave),
    ('È', 'E', Mark::CapGrave),
    ('Ò', 'O', Mark::CapGrave),
    ('Ù', 'U', Mark::CapGrave),
    ('Ä', 'A', Mark::CapDots),
    ('Ë', 'E', Mark::CapDots),
    ('Ï', 'I', Mark::CapDots),
    ('Ö', 'O', Mark::CapDots),
    ('Ü', 'U', Mark::CapDots),
];

/// Superscript digits: (character, base digit).
const SUPERSCRIPTS: &[(char, char)] = &[('¹', '1'), ('²', '2'), ('³', '3')];

/// Parses one glyph definition into strokes. Returns an error naming the bad token.
fn parse(def: &str) -> Result<Vec<Vec<P>>, String> {
    let mut strokes = Vec::new();
    for part in def.split('|') {
        let mut stroke: Vec<P> = Vec::new();
        for tok in part.split_whitespace() {
            if let Some(inner) = tok.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
                let v = numbers(inner)?;
                let [cx, cy, rx, ry, a0, a1] = v.as_slice() else {
                    return Err(format!("arc needs 6 numbers: {tok}"));
                };
                let n = ((a1 - a0).abs() / 12.0).ceil().max(2.0) as usize;
                for i in 0..=n {
                    let a = (a0 + (a1 - a0) * i as f64 / n as f64).to_radians();
                    push_point(&mut stroke, [cx + rx * a.cos(), cy + ry * a.sin()]);
                }
            } else {
                let v = numbers(tok)?;
                let [x, y] = v.as_slice() else { return Err(format!("point needs 2 numbers: {tok}")) };
                push_point(&mut stroke, [*x, *y]);
            }
        }
        match stroke.len() {
            0 => {}
            1 => return Err(format!("stroke with a single point: {part}")),
            _ => strokes.push(stroke),
        }
    }
    Ok(strokes)
}

fn numbers(s: &str) -> Result<Vec<f64>, String> {
    s.split(',').map(|n| n.trim().parse::<f64>().map_err(|_| format!("not a number: {n}"))).collect()
}

fn push_point(stroke: &mut Vec<P>, p: P) {
    if let Some(last) = stroke.last()
        && (last[0] - p[0]).abs() < 1e-9
        && (last[1] - p[1]).abs() < 1e-9
    {
        return;
    }
    stroke.push(p);
}

fn x_range(strokes: &[Vec<P>]) -> Option<(f64, f64)> {
    let mut it = strokes.iter().flatten().map(|p| p[0]);
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), x| (lo.min(x), hi.max(x))))
}

fn shifted(strokes: &[Vec<P>], dx: f64, dy: f64, k: f64) -> Vec<Vec<P>> {
    strokes.iter().map(|s| s.iter().map(|p| [p[0] * k + dx, p[1] * k + dy]).collect()).collect()
}

/// Builds a glyph: moves the strokes so the leftmost point is at x = 0 and sets the advance from the width.
fn finish(strokes: Vec<Vec<P>>, tabular: bool) -> Glyph {
    let Some((lo, hi)) = x_range(&strokes) else { return Glyph { strokes, advance: SPACE } };
    let width = hi - lo;
    if tabular {
        let dx = (DIGIT_CELL - width) / 2.0 - lo;
        Glyph { strokes: shifted(&strokes, dx, 0.0, 1.0), advance: DIGIT_CELL + SPACING }
    } else {
        Glyph { strokes: shifted(&strokes, -lo, 0.0, 1.0), advance: width + SPACING }
    }
}

fn mark_strokes(mark: Mark, cx: f64) -> Vec<Vec<P>> {
    let dot = |x: f64, y: f64| vec![[x, y], [x, y + 0.3]];
    match mark {
        Mark::Tonos | Mark::Acute => vec![vec![[cx - 0.2, 4.8], [cx + 0.6, 5.9]]],
        Mark::Grave => vec![vec![[cx - 0.6, 5.9], [cx + 0.2, 4.8]]],
        Mark::Circumflex => vec![vec![[cx - 0.8, 4.8], [cx, 5.7], [cx + 0.8, 4.8]]],
        Mark::Tilde => vec![vec![[cx - 1.0, 5.0], [cx - 0.4, 5.5], [cx + 0.4, 5.0], [cx + 1.0, 5.5]]],
        Mark::Dots => vec![dot(cx - 0.8, 5.1), dot(cx + 0.8, 5.1)],
        Mark::DotsTonos => {
            vec![dot(cx - 1.0, 5.1), dot(cx + 1.0, 5.1), vec![[cx - 0.3, 4.9], [cx + 0.3, 6.1]]]
        }
        Mark::CapDots => vec![dot(cx - 0.9, 6.7), dot(cx + 0.9, 6.7)],
        Mark::CapAcute => vec![vec![[cx - 0.3, 6.6], [cx + 0.5, 7.4]]],
        Mark::CapGrave => vec![vec![[cx - 0.5, 7.4], [cx + 0.3, 6.6]]],
        Mark::Cedilla => vec![vec![[cx, 0.0], [cx, -0.6], [cx + 0.6, -1.0], [cx - 0.4, -1.6]]],
        Mark::CapTonos => vec![vec![[0.0, 4.8], [0.6, 6.0]]],
    }
}

fn compose(base: &Glyph, mark: Mark) -> Glyph {
    let (lo, hi) = x_range(&base.strokes).unwrap_or((0.0, 0.0));
    let cx = (lo + hi) / 2.0;
    let mut strokes;
    if mark == Mark::CapTonos {
        strokes = shifted(&base.strokes, 1.4, 0.0, 1.0);
        strokes.extend(mark_strokes(mark, cx));
    } else {
        strokes = base.strokes.clone();
        strokes.extend(mark_strokes(mark, cx));
    }
    finish(strokes, false)
}

/// The glyph table, parsed on first use.
fn table() -> &'static HashMap<char, Glyph> {
    static TABLE: OnceLock<HashMap<char, Glyph>> = OnceLock::new();
    TABLE.get_or_init(build_table)
}

fn build_table() -> HashMap<char, Glyph> {
    let mut map = HashMap::new();
    for (c, def) in BASE {
        // Definitions are checked by the tests; a bad one would only lose that character.
        if let Ok(strokes) = parse(def) {
            let g = if *c == ' ' {
                Glyph { strokes, advance: SPACE }
            } else {
                finish(strokes, c.is_ascii_digit())
            };
            map.insert(*c, g);
        }
    }
    // Dotless i for accented Latin i.
    if let Ok(s) = parse("0,0 0,4") {
        map.insert('ı', finish(s, false));
    }
    for (alias, target) in ALIASES {
        if let Some(g) = map.get(target).cloned() {
            map.insert(*alias, g);
        }
    }
    for (c, base, mark) in COMPOSITES {
        if let Some(g) = map.get(base).map(|b| compose(b, *mark)) {
            map.insert(*c, g);
        }
    }
    for (c, digit) in SUPERSCRIPTS {
        if let Some(g) = map.get(digit) {
            let strokes = shifted(&g.strokes, 0.0, 3.2, 0.5);
            map.insert(*c, finish(strokes, false));
        }
    }
    map
}

/// The "missing character" box.
fn missing() -> &'static Glyph {
    static BOX: OnceLock<Glyph> = OnceLock::new();
    BOX.get_or_init(|| Glyph {
        strokes: vec![vec![[0.0, 0.0], [3.0, 0.0], [3.0, 6.0], [0.0, 6.0], [0.0, 0.0]]],
        advance: 3.0 + SPACING,
    })
}

/// The glyph for a character, if the font covers it.
pub fn glyph(c: char) -> Option<&'static Glyph> {
    table().get(&c)
}

/// Whether the font covers a character (otherwise it is drawn as a box).
pub fn has_glyph(c: char) -> bool {
    table().contains_key(&c)
}

/// All characters the font covers, sorted.
pub fn covered_chars() -> Vec<char> {
    let mut v: Vec<char> = table().keys().copied().collect();
    v.sort_unstable();
    v
}

/// Advance width of a character at a cap height.
pub fn advance(c: char, cap_height: f64) -> f64 {
    glyph(c).unwrap_or_else(missing).advance * cap_height / CAP
}

/// Appends the strokes of a character at a cap height, with its origin at `(x, 0)`, and returns its advance.
pub fn glyph_into(c: char, cap_height: f64, x: f64, out: &mut Vec<Vec<P>>) -> f64 {
    let g = glyph(c).unwrap_or_else(missing);
    let k = cap_height / CAP;
    out.extend(g.strokes.iter().map(|s| s.iter().map(|p| [x + p[0] * k, p[1] * k]).collect()));
    g.advance * k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_definition_parses() {
        for (c, def) in BASE {
            let s = parse(def).unwrap_or_else(|e| panic!("glyph {c:?}: {e}"));
            assert!(*c == ' ' || !s.is_empty(), "glyph {c:?} has no strokes");
        }
    }

    #[test]
    fn bad_definitions_are_rejected() {
        assert!(parse("0,0 x,1").is_err());
        assert!(parse("[1,2,3]").is_err());
        assert!(parse("0,0").is_err());
    }

    #[test]
    fn required_repertoire_is_covered() {
        let mut required: Vec<char> = Vec::new();
        required.extend('0'..='9');
        required.extend('A'..='Z');
        required.extend('a'..='z');
        required.extend(('Α'..='Ω').filter(|c| *c != '\u{3a2}'));
        required.extend('α'..='ω');
        required.extend("άέήίόύώϊϋΐΰΆΈΉΊΌΎΏΪΫ".chars());
        required.extend(".,:;!?-+=/()[]'\"%&*#_<> °²³±×·–—€".chars());
        let missing: Vec<char> = required.into_iter().filter(|c| !has_glyph(*c)).collect();
        assert!(missing.is_empty(), "missing glyphs: {missing:?}");
    }

    #[test]
    fn glyphs_start_at_zero_and_fit_the_grid() {
        for c in covered_chars() {
            let Some(g) = glyph(c) else { continue };
            if let Some((lo, hi)) = x_range(&g.strokes) {
                if c.is_ascii_digit() {
                    assert!(lo >= -1e-9 && hi <= DIGIT_CELL + 1e-9, "{c:?}");
                } else {
                    assert!(lo.abs() < 1e-9, "{c:?} starts at {lo}");
                }
                assert!(g.advance > hi, "{c:?} advance");
            }
            for p in g.strokes.iter().flatten() {
                assert!(p[1] >= -2.0 - 1e-9 && p[1] <= 7.5, "{c:?} point {p:?}");
            }
        }
    }

    #[test]
    fn digits_are_tabular() {
        let a = advance('1', 6.0);
        for d in '0'..='9' {
            assert!((advance(d, 6.0) - a).abs() < 1e-12, "{d}");
        }
    }

    #[test]
    fn accented_letters_differ_from_their_base() {
        assert_ne!(glyph('ά'), glyph('α'));
        assert_ne!(glyph('Ά'), glyph('Α'));
        // Capital tonos sits before the letter, so the letter is wider.
        assert!(advance('Ά', 6.0) > advance('Α', 6.0));
        assert_eq!(glyph('Α'), glyph('A'));
    }

    #[test]
    fn unknown_characters_draw_a_box() {
        let mut out = Vec::new();
        let adv = glyph_into('\u{4e2d}', 6.0, 10.0, &mut out);
        assert_eq!(out.len(), 1);
        assert!((adv - 4.4).abs() < 1e-9);
        assert!(out[0].iter().all(|p| p[0] >= 10.0));
    }
}
