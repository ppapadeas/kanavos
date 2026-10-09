//! TrueType / OpenType outline fonts, read with `ttf-parser`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ttf_parser::{Face, OutlineBuilder};

use crate::PathEl;

/// Error loading a font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontError {
    /// The file could not be read.
    Io(String),
    /// The data is not a usable TrueType or OpenType font.
    Parse(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Io(e) => write!(f, "cannot read font: {e}"),
            FontError::Parse(e) => write!(f, "not a usable font: {e}"),
        }
    }
}

impl std::error::Error for FontError {}

/// A loaded outline font face.
///
/// The font bytes are shared (`Arc`), so cloning a face is cheap. Metrics are read once at load time.
#[derive(Clone)]
pub struct FontFace {
    data: Arc<[u8]>,
    index: u32,
    path: Option<PathBuf>,
    family: String,
    units_per_em: f64,
    cap_height: f64,
    descender: f64,
}

impl fmt::Debug for FontFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontFace")
            .field("family", &self.family)
            .field("path", &self.path)
            .field("index", &self.index)
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl FontFace {
    /// Loads a face from font bytes (`index` selects a face in a collection; use 0 for single fonts).
    pub fn from_bytes(data: impl Into<Arc<[u8]>>, index: u32) -> Result<FontFace, FontError> {
        let data: Arc<[u8]> = data.into();
        let face = Face::parse(&data, index).map_err(|e| FontError::Parse(e.to_string()))?;
        let units_per_em = f64::from(face.units_per_em());
        if units_per_em <= 0.0 {
            return Err(FontError::Parse("units per em is zero".into()));
        }
        let cap_height = face
            .capital_height()
            .filter(|h| *h > 0)
            .map(f64::from)
            .or_else(|| {
                let gid = face.glyph_index('H')?;
                let bbox = face.glyph_bounding_box(gid)?;
                (bbox.y_max > 0).then(|| f64::from(bbox.y_max))
            })
            .unwrap_or(0.7 * units_per_em);
        let descender = f64::from(face.descender()).abs();
        let family = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .find_map(|n| n.to_string())
            .unwrap_or_default();
        Ok(FontFace { data, index, path: None, family, units_per_em, cap_height, descender })
    }

    /// Loads the first face of a font file.
    pub fn from_file(path: &Path) -> Result<FontFace, FontError> {
        let bytes = std::fs::read(path).map_err(|e| FontError::Io(format!("{}: {e}", path.display())))?;
        let mut face = FontFace::from_bytes(bytes, 0)?;
        face.path = Some(path.to_path_buf());
        Ok(face)
    }

    /// The font bytes.
    pub fn data(&self) -> &Arc<[u8]> {
        &self.data
    }

    /// Index of the face in its file (0 unless the file is a collection).
    pub fn index(&self) -> u32 {
        self.index
    }

    /// The file the face was loaded from, if any.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Family name from the font's naming table (may be empty).
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Cap height as a fraction of the em.
    pub fn cap_height_em(&self) -> f64 {
        self.cap_height / self.units_per_em
    }

    /// Descender depth as a fraction of the em (positive).
    pub fn descent_em(&self) -> f64 {
        self.descender / self.units_per_em
    }

    /// Parses the face for a series of lookups. Returns `None` only if the bytes stopped parsing, which cannot
    /// happen for a face built by [`FontFace::from_bytes`].
    pub(crate) fn parsed(&self) -> Option<Face<'_>> {
        Face::parse(&self.data, self.index).ok()
    }

    /// Whether the font has a glyph for a character.
    pub fn has_glyph(&self, c: char) -> bool {
        self.parsed().is_some_and(|f| f.glyph_index(c).is_some_and(|g| g.0 != 0))
    }

    /// Whether the font has glyphs for every non-whitespace character of a string.
    pub fn covers(&self, text: &str) -> bool {
        let Some(f) = self.parsed() else { return false };
        text.chars().filter(|c| !c.is_whitespace()).all(|c| f.glyph_index(c).is_some_and(|g| g.0 != 0))
    }

    /// Scale from font units to output units for a cap height.
    fn scale(&self, cap_height: f64) -> f64 {
        cap_height / self.cap_height
    }

    /// Advance width of a character at a cap height, or `None` if the font has no glyph for it.
    pub fn advance(&self, c: char, cap_height: f64) -> Option<f64> {
        let f = self.parsed()?;
        self.advance_with(&f, c, cap_height)
    }

    pub(crate) fn advance_with(&self, f: &Face<'_>, c: char, cap_height: f64) -> Option<f64> {
        let gid = f.glyph_index(c).filter(|g| g.0 != 0)?;
        let adv = f.glyph_hor_advance(gid).map(f64::from).unwrap_or(0.0);
        Some(adv * self.scale(cap_height))
    }

    /// Outline of one character at a cap height with its origin at `(x, 0)`, appended to `out`.
    /// Returns the advance width, or `None` if the font has no glyph for the character.
    pub(crate) fn outline_into(
        &self,
        f: &Face<'_>,
        c: char,
        cap_height: f64,
        x: f64,
        out: &mut Vec<PathEl>,
    ) -> Option<f64> {
        let gid = f.glyph_index(c).filter(|g| g.0 != 0)?;
        let k = self.scale(cap_height);
        let mut b = Builder { out, k, dx: x, open: false };
        // Glyphs without an outline (space) are fine: they only advance.
        let _ = f.outline_glyph(gid, &mut b);
        if b.open {
            b.out.push(PathEl::Close);
        }
        let adv = f.glyph_hor_advance(gid).map(f64::from).unwrap_or(0.0);
        Some(adv * k)
    }

    /// Outline of a whole line of text at a cap height, pen at the origin. Characters missing from the font are
    /// skipped (use [`crate::Fonts::layout`] for fallback). Returns the outline and the advance width.
    pub fn outline_text(&self, text: &str, cap_height: f64) -> (Vec<PathEl>, f64) {
        let mut out = Vec::new();
        let Some(f) = self.parsed() else { return (out, 0.0) };
        let mut x = 0.0;
        for c in text.chars() {
            if let Some(adv) = self.outline_into(&f, c, cap_height, x, &mut out) {
                x += adv;
            }
        }
        (out, x)
    }
}

/// Collects `ttf-parser` outline callbacks as [`PathEl`]s, scaled and shifted.
struct Builder<'a> {
    out: &'a mut Vec<PathEl>,
    k: f64,
    dx: f64,
    open: bool,
}

impl Builder<'_> {
    fn p(&self, x: f32, y: f32) -> [f64; 2] {
        [f64::from(x) * self.k + self.dx, f64::from(y) * self.k]
    }
}

impl OutlineBuilder for Builder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.open {
            self.out.push(PathEl::Close);
        }
        let p = self.p(x, y);
        self.out.push(PathEl::MoveTo(p));
        self.open = true;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.out.push(PathEl::LineTo(p));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.p(x1, y1), self.p(x, y));
        self.out.push(PathEl::QuadTo(c, p));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.out.push(PathEl::CubicTo(c1, c2, p));
    }
    fn close(&mut self) {
        if self.open {
            self.out.push(PathEl::Close);
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_rejected() {
        assert!(matches!(FontFace::from_bytes(vec![0u8; 64], 0), Err(FontError::Parse(_))));
        assert!(matches!(FontFace::from_file(Path::new("/definitely/not/here.ttf")), Err(FontError::Io(_))));
    }

    #[test]
    fn installed_font_outlines_scale_to_cap_height() {
        // Only meaningful where a system font exists; otherwise there is nothing to check.
        let Some(c) = crate::discover_system_font() else { return };
        let face = c.regular;
        assert!(face.covers("ΑΩαωάΆ"));
        let (paths, adv) = face.outline_text("H", 10.0);
        assert!(adv > 5.0 && adv < 12.0, "advance {adv}");
        let rings = crate::flatten_paths(&paths, 0.01);
        let top = rings.iter().flatten().map(|p| p[1]).fold(f64::MIN, f64::max);
        assert!((top - 10.0).abs() < 0.3, "cap height {top}");
    }
}
