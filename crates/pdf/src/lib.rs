//! Vector PDF and PDF/A export.
//!
//! Layer L6. See `docs/architecture.md` and `docs/formats/pdf.md`.
//!
//! [`write_pdf`] turns pages of drawings into a PDF 1.4 file:
//!
//! - one page per [`PdfPage`], with MediaBox, CropBox and TrimBox equal to the paper size;
//! - strokes with their true widths (paper millimetres), dash arrays, caps and colours; arcs and circles as exact
//!   cubic Bézier curves;
//! - solid fills and hatch backgrounds with the even-odd rule, hatch lines expanded by
//!   [`kanavos_drawing::hatch`];
//! - text as filled glyph outlines from the system font, or single-stroke lettering from the built-in font, so
//!   Greek always prints and no font needs embedding;
//! - document information (title, author, subject, keywords, creator, producer, dates) and a file identifier;
//! - optional PDF/A-1b output ([`PdfMeta::pdfa`]): XMP metadata matching the document information, an sRGB output
//!   intent with a generated ICC profile, and no transparency (colours with alpha are blended over white).
//!
//! The objects are written directly (no PDF library): text is drawn as outlines, so the file needs only a catalogue,
//! a page tree, pages, content streams and metadata. Content streams are Flate-compressed with `miniz_oxide`.

mod content;
mod icc;

use std::fmt;
use std::fmt::Write as _;

use kanavos_drawing::fonts::Fonts;
use kanavos_drawing::hatch::HatchOptions;
use kanavos_drawing::page::PageFrame;
use kanavos_drawing::{Drawing, DrawingUnits};

pub use content::PT_PER_MM;
pub use icc::srgb_profile;

/// Error from PDF export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// No pages were given.
    NoPages,
    /// A page has a zero, negative or non-finite size, or one larger than PDF allows (5080 mm).
    InvalidPageSize {
        /// Index of the page.
        page: usize,
    },
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfError::NoPages => write!(f, "nothing to export: no pages"),
            PdfError::InvalidPageSize { page } => write!(f, "page {} has an invalid size", page + 1),
        }
    }
}

impl std::error::Error for PdfError {}

/// Largest page side PDF readers are required to support (200 inches).
pub const MAX_PAGE_MM: f64 = 5080.0;

/// One page: a drawing and the paper size.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfPage {
    /// The drawing. A paper-space drawing (millimetres) is placed with its origin at the lower-left corner of the
    /// page; a model-space drawing is centred on the page at its scale.
    pub drawing: Drawing,
    /// Page width, millimetres.
    pub width_mm: f64,
    /// Page height, millimetres.
    pub height_mm: f64,
}

impl PdfPage {
    /// A page of the given paper size.
    pub fn new(drawing: Drawing, width_mm: f64, height_mm: f64) -> PdfPage {
        PdfPage { drawing, width_mm, height_mm }
    }

    /// A model-space drawing at its scale on a page just large enough for it plus a margin.
    pub fn fitted(drawing: &Drawing, margin_mm: f64, fonts: &Fonts) -> PdfPage {
        let frame = PageFrame::fit(drawing, fonts, margin_mm);
        PdfPage {
            drawing: frame.to_paper_drawing(drawing),
            width_mm: frame.width_mm,
            height_mm: frame.height_mm,
        }
    }

    /// The drawing in page millimetres.
    fn paper(&self, fonts: &Fonts) -> Drawing {
        match self.drawing.units {
            DrawingUnits::Millimetres => self.drawing.clone(),
            DrawingUnits::Metres => PageFrame::centred(&self.drawing, fonts, self.width_mm, self.height_mm)
                .to_paper_drawing(&self.drawing),
        }
    }
}

/// A date and time in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDate {
    /// Year.
    pub year: u16,
    /// Month, 1–12.
    pub month: u8,
    /// Day, 1–31.
    pub day: u8,
    /// Hour, 0–23.
    pub hour: u8,
    /// Minute, 0–59.
    pub minute: u8,
    /// Second, 0–59.
    pub second: u8,
}

impl PdfDate {
    /// The current time from the system clock (UTC); the Unix epoch if the clock is before it.
    pub fn now() -> PdfDate {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        PdfDate::from_unix(secs)
    }

    /// The date of a Unix timestamp (seconds since 1970-01-01 UTC).
    pub fn from_unix(secs: u64) -> PdfDate {
        let days = (secs / 86_400) as i64;
        let rem = secs % 86_400;
        // Civil date from a day count (proleptic Gregorian calendar, eras of 400 years).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        PdfDate {
            year: year.clamp(0, 9999) as u16,
            month: month as u8,
            day: day as u8,
            hour: (rem / 3600) as u8,
            minute: (rem % 3600 / 60) as u8,
            second: (rem % 60) as u8,
        }
    }

    /// PDF date string, e.g. `D:20261009120000+00'00'`.
    fn pdf(&self) -> String {
        format!(
            "D:{:04}{:02}{:02}{:02}{:02}{:02}+00'00'",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// XMP (ISO 8601) date string, e.g. `2026-10-09T12:00:00+00:00`.
    fn xmp(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Document information and output options.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfMeta {
    /// Title.
    pub title: String,
    /// Author.
    pub author: String,
    /// Subject.
    pub subject: String,
    /// Keywords.
    pub keywords: String,
    /// Application that created the content.
    pub creator: String,
    /// Creation (and modification) date; `None` uses the current time.
    pub creation_date: Option<PdfDate>,
    /// Write PDF/A-1b: XMP metadata, sRGB output intent, no transparency.
    pub pdfa: bool,
    /// Flate-compress content streams.
    pub compress: bool,
    /// Limits for hatch expansion. Areas whose hatch is too dense get their background only.
    pub hatch: HatchOptions,
}

impl Default for PdfMeta {
    fn default() -> Self {
        PdfMeta {
            title: String::new(),
            author: String::new(),
            subject: String::new(),
            keywords: String::new(),
            creator: "Kanavos".to_owned(),
            creation_date: None,
            pdfa: false,
            compress: true,
            hatch: HatchOptions::default(),
        }
    }
}

/// Producer string written in the document information.
pub const PRODUCER: &str = concat!("Kanavos PDF writer ", env!("CARGO_PKG_VERSION"));

/// Writes pages to a PDF file in memory, drawing text with the system fonts.
pub fn write_pdf(pages: &[PdfPage], meta: &PdfMeta) -> Result<Vec<u8>, PdfError> {
    write_pdf_with_fonts(pages, meta, Fonts::system())
}

/// Writes pages to a PDF file in memory with explicit fonts. Pass [`Fonts::builtin`] for output identical on every
/// machine.
pub fn write_pdf_with_fonts(pages: &[PdfPage], meta: &PdfMeta, fonts: &Fonts) -> Result<Vec<u8>, PdfError> {
    if pages.is_empty() {
        return Err(PdfError::NoPages);
    }
    for (i, p) in pages.iter().enumerate() {
        let ok = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_PAGE_MM;
        if !(ok(p.width_mm) && ok(p.height_mm)) {
            return Err(PdfError::InvalidPageSize { page: i });
        }
    }
    let date = meta.creation_date.unwrap_or_else(PdfDate::now);
    let mut w = Writer::default();
    w.out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    // Object numbers: 1 catalogue, 2 page tree, 3 information, then PDF/A objects, then two per page.
    let (catalog, pages_id, info) = (1, 2, 3);
    let (metadata, icc) = (4, 5);
    let first_page = if meta.pdfa { 6 } else { 4 };
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| first_page + 2 * i).collect();

    let mut cat = format!("<< /Type /Catalog /Pages {pages_id} 0 R");
    if meta.pdfa {
        let _ = write!(
            cat,
            " /Metadata {metadata} 0 R /OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1 \
             /OutputConditionIdentifier (sRGB IEC61966-2.1) /Info (sRGB IEC61966-2.1) \
             /DestOutputProfile {icc} 0 R >>]"
        );
    }
    cat.push_str(" >>");
    w.object(catalog, cat.as_bytes());

    let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
    w.object(
        pages_id,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes(),
    );

    let mut info_dict = String::from("<<");
    for (key, value) in [
        ("Title", &meta.title),
        ("Author", &meta.author),
        ("Subject", &meta.subject),
        ("Keywords", &meta.keywords),
        ("Creator", &meta.creator),
    ] {
        if !value.is_empty() {
            let _ = write!(info_dict, " /{key} {}", text_string(value));
        }
    }
    let _ = write!(
        info_dict,
        " /Producer {} /CreationDate ({}) /ModDate ({}) >>",
        text_string(PRODUCER),
        date.pdf(),
        date.pdf()
    );
    w.object(info, info_dict.as_bytes());

    if meta.pdfa {
        let xmp = xmp_packet(meta, &date);
        let mut obj =
            format!("<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n", xmp.len()).into_bytes();
        obj.extend_from_slice(xmp.as_bytes());
        obj.extend_from_slice(b"\nendstream");
        w.object(metadata, &obj);
        let profile = srgb_profile();
        w.stream(icc, "/N 3 /Alternate /DeviceRGB", &profile, meta.compress);
    }

    for (page, id) in pages.iter().zip(&page_ids) {
        let paper = page.paper(fonts);
        let mut c = content::Content::new(fonts, !meta.pdfa);
        for item in &paper.items {
            c.item(item, &paper, &meta.hatch);
        }
        let (stream, states) = c.finish();
        let (wpt, hpt) = (content::num(page.width_mm * PT_PER_MM), content::num(page.height_mm * PT_PER_MM));
        let mut resources = String::from("<<");
        if !states.is_empty() {
            resources.push_str(" /ExtGState <<");
            for (name, (stroke, a)) in &states {
                let key = if *stroke { "CA" } else { "ca" };
                let _ = write!(resources, " /{name} << /Type /ExtGState /{key} {} >>", content::num(*a));
            }
            resources.push_str(" >>");
        }
        resources.push_str(" >>");
        let content_id = id + 1;
        w.object(
            *id,
            format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {wpt} {hpt}] /CropBox [0 0 {wpt} {hpt}] \
                 /TrimBox [0 0 {wpt} {hpt}] /Resources {resources} /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        w.stream(content_id, "", stream.as_bytes(), meta.compress);
    }

    let size = page_ids.last().map(|id| id + 2).unwrap_or(first_page);
    let xref = w.out.len();
    let _ = write!(w.text, "xref\n0 {size}\n0000000000 65535 f\r\n");
    for n in 1..size {
        let off = w.offsets.get(&n).copied().unwrap_or(0);
        let _ = write!(w.text, "{off:010} 00000 n\r\n");
    }
    let id = file_id(&w.out);
    let _ = write!(
        w.text,
        "trailer\n<< /Size {size} /Root {catalog} 0 R /Info {info} 0 R /ID [<{id}> <{id}>] >>\nstartxref\n{xref}\n%%EOF\n"
    );
    let tail = std::mem::take(&mut w.text);
    w.out.extend_from_slice(tail.as_bytes());
    Ok(w.out)
}

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
    offsets: std::collections::BTreeMap<usize, usize>,
    text: String,
}

impl Writer {
    fn object(&mut self, id: usize, body: &[u8]) {
        self.offsets.insert(id, self.out.len());
        self.out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        self.out.extend_from_slice(body);
        self.out.extend_from_slice(b"\nendobj\n");
    }

    fn stream(&mut self, id: usize, extra: &str, data: &[u8], compress: bool) {
        let (bytes, filter) = if compress {
            (miniz_oxide::deflate::compress_to_vec_zlib(data, 6), " /Filter /FlateDecode")
        } else {
            (data.to_vec(), "")
        };
        let sep = if extra.is_empty() { "" } else { " " };
        let mut obj = format!("<< /Length {}{filter}{sep}{extra} >>\nstream\n", bytes.len()).into_bytes();
        obj.extend_from_slice(&bytes);
        obj.extend_from_slice(b"\nendstream");
        self.object(id, &obj);
    }
}

/// A PDF text string: a literal string for printable ASCII, otherwise UTF-16BE with a byte-order mark in hex.
fn text_string(s: &str) -> String {
    if s.chars().all(|c| (' '..='~').contains(&c)) {
        let mut out = String::from("(");
        for c in s.chars() {
            if matches!(c, '(' | ')' | '\\') {
                out.push('\\');
            }
            out.push(c);
        }
        out.push(')');
        out
    } else {
        let mut out = String::from("<FEFF");
        for unit in s.encode_utf16() {
            let _ = write!(out, "{unit:04X}");
        }
        out.push('>');
        out
    }
}

/// A 16-byte file identifier: two FNV-1a 64-bit hashes of the file body with different offsets.
fn file_id(data: &[u8]) -> String {
    let hash =
        |seed: u64| data.iter().fold(seed, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01B3));
    format!("{:016X}{:016X}", hash(0xCBF2_9CE4_8422_2325), hash(0x6C62_272E_07BB_0142))
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// XMP metadata (ISO 16684-1) mirroring the document information, with the PDF/A-1b identification schema.
fn xmp_packet(meta: &PdfMeta, date: &PdfDate) -> String {
    let mut x = String::new();
    x.push_str("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n");
    x.push_str("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n");
    x.push_str("<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n");
    x.push_str(
        "<rdf:Description rdf:about=\"\" xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\">\
         <pdfaid:part>1</pdfaid:part><pdfaid:conformance>B</pdfaid:conformance></rdf:Description>\n",
    );
    x.push_str("<rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">");
    x.push_str("<dc:format>application/pdf</dc:format>");
    if !meta.title.is_empty() {
        let _ = write!(
            x,
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:title>",
            xml_escape(&meta.title)
        );
    }
    if !meta.author.is_empty() {
        let _ = write!(
            x,
            "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
            xml_escape(&meta.author)
        );
    }
    if !meta.subject.is_empty() {
        let _ = write!(
            x,
            "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt></dc:description>",
            xml_escape(&meta.subject)
        );
    }
    x.push_str("</rdf:Description>\n");
    let _ = writeln!(
        x,
        "<rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\
         <xmp:CreateDate>{d}</xmp:CreateDate><xmp:ModifyDate>{d}</xmp:ModifyDate>{tool}</rdf:Description>",
        d = date.xmp(),
        tool = if meta.creator.is_empty() {
            String::new()
        } else {
            format!("<xmp:CreatorTool>{}</xmp:CreatorTool>", xml_escape(&meta.creator))
        }
    );
    let _ = write!(
        x,
        "<rdf:Description rdf:about=\"\" xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"><pdf:Producer>{}</pdf:Producer>",
        xml_escape(PRODUCER)
    );
    if !meta.keywords.is_empty() {
        let _ = write!(x, "<pdf:Keywords>{}</pdf:Keywords>", xml_escape(&meta.keywords));
    }
    x.push_str("</rdf:Description>\n</rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>");
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_drawing::{Fill, HatchPattern, Item, LinePattern, Rgba, Stroke, Text, pt, weights};

    fn sample() -> Drawing {
        let mut d = Drawing::model(50.0);
        d.push(Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(4.0, 0.0), pt(4.0, 0.25), pt(0.0, 0.25)]],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            Some(Stroke::solid(weights::EXTRA_THICK)),
            "wall.cut",
        ));
        d.push(Item::line(
            pt(0.0, 1.0),
            pt(4.0, 1.0),
            Stroke::patterned(weights::THIN, LinePattern::Dashed),
            "x",
        ));
        d.push(Item::text(Text::new(pt(0.0, 1.5), "Κάτοψη ισογείου 1:50", 3.5), "text"));
        d
    }

    fn meta() -> PdfMeta {
        PdfMeta {
            title: "Κάτοψη (ισόγειο)".into(),
            author: "Test".into(),
            creation_date: Some(PdfDate { year: 2026, month: 10, day: 9, hour: 12, minute: 0, second: 0 }),
            ..PdfMeta::default()
        }
    }

    /// Minimal structural reader: checks the cross-reference table against the object positions and returns the
    /// trailer and the object bodies.
    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    fn check_structure(pdf: &[u8]) -> (String, Vec<String>) {
        assert!(pdf.starts_with(b"%PDF-1.4\n%"));
        // The cross-reference table and trailer are ASCII.
        let sx = pdf.windows(10).rposition(|w| w == b"startxref\n").unwrap();
        let tail = std::str::from_utf8(&pdf[sx..]).unwrap();
        assert!(tail.trim_end().ends_with("%%EOF"));
        let xref_off: usize = tail[10..].lines().next().unwrap().trim().parse().unwrap();
        assert!(pdf[xref_off..].starts_with(b"xref\n"), "startxref points at the xref table");
        let text = std::str::from_utf8(&pdf[xref_off..]).unwrap();
        let mut lines = text.split('\n');
        lines.next();
        let header: Vec<usize> = lines.next().unwrap().split(' ').map(|v| v.parse().unwrap()).collect();
        let size = header[1];
        let mut bodies = Vec::new();
        for n in 0..size {
            let entry = lines.next().unwrap();
            assert!(
                entry.len() + 1 == 20 && entry.as_bytes().last() == Some(&13),
                "xref entries are 20 bytes"
            );
            if n == 0 {
                assert!(entry.trim_end().ends_with('f'));
                continue;
            }
            let off: usize = entry[..10].parse().unwrap();
            let head = format!("{n} 0 obj\n");
            assert!(pdf[off..].starts_with(head.as_bytes()), "object {n} at offset {off}");
            let end = find(&pdf[off..], b"endobj").unwrap();
            bodies.push(String::from_utf8_lossy(&pdf[off + head.len()..off + end]).into_owned());
        }
        let trailer = text[text.rfind("trailer").unwrap()..].to_owned();
        assert!(trailer.contains(&format!("/Size {size}")));
        (trailer, bodies)
    }

    /// Decompressed data of the content stream object `id`.
    fn page_content(pdf: &[u8], id: usize) -> String {
        let head = format!("\n{id} 0 obj\n<< /Length ");
        let base = find(pdf, head.as_bytes()).unwrap() + head.len();
        let len_end = base + find(&pdf[base..], b" ").unwrap();
        let len: usize = std::str::from_utf8(&pdf[base..len_end]).unwrap().parse().unwrap();
        let start = base + find(&pdf[base..], b"stream\n").unwrap() + 7;
        let data = &pdf[start..start + len];
        String::from_utf8(miniz_oxide::inflate::decompress_to_vec_zlib(data).unwrap()).unwrap()
    }

    #[test]
    fn single_page_structure_and_content() {
        let page = PdfPage::new(sample(), 297.0, 210.0);
        let pdf = write_pdf_with_fonts(&[page], &meta(), &Fonts::builtin()).unwrap();
        let (trailer, bodies) = check_structure(&pdf);
        assert!(trailer.contains("/Root 1 0 R") && trailer.contains("/ID ["));
        assert!(bodies[0].contains("/Type /Catalog"));
        assert!(bodies[1].contains("/Count 1"));
        // Title with non-ASCII characters is UTF-16BE with a byte-order mark.
        assert!(bodies[2].contains("/Title <FEFF039A"));
        assert!(bodies[2].contains("/CreationDate (D:20261009120000+00'00')"));
        let page = &bodies[3];
        assert!(page.contains("/MediaBox [0 0 841.8898 595.2756]"), "{page}");
        let content = page_content(&pdf, 5);
        assert!(content.starts_with("2.8346 0 0 2.8346 0 0 cm"));
        assert!(content.contains("f*"), "fills");
        assert!(content.contains("0.7 w"), "wall outline weight");
        assert!(content.contains("[3 1.5] 0 d"), "dash array");
        assert!(content.contains("0.13 w 0 J"), "hatch lines");
        assert!(content.matches(" S\n").count() + content.matches("\nS\n").count() > 3);
    }

    #[test]
    fn multiple_pages_and_uncompressed_output() {
        let mut paper = Drawing::paper();
        paper.push(Item::line(pt(10.0, 10.0), pt(200.0, 10.0), Stroke::solid(weights::HEAVY), "border"));
        let pages = [PdfPage::new(paper, 420.0, 297.0), PdfPage::fitted(&sample(), 10.0, &Fonts::builtin())];
        let m = PdfMeta { compress: false, ..meta() };
        let pdf = write_pdf_with_fonts(&pages, &m, &Fonts::builtin()).unwrap();
        let (_, bodies) = check_structure(&pdf);
        assert!(bodies[1].contains("/Kids [4 0 R 6 0 R] /Count 2"));
        assert!(bodies[4].contains("10 10 m"), "{}", &bodies[4]);
        // The fitted page is the content at 1:50 plus margins: 4 m → 80 mm + 20 mm.
        assert!(bodies[5].contains("/MediaBox [0 0 283.4646 "), "{}", &bodies[5]);
    }

    #[test]
    fn pdfa_adds_metadata_output_intent_and_no_transparency() {
        let mut d = sample();
        d.push(Item::polygon(
            vec![vec![pt(0.0, 2.0), pt(1.0, 2.0), pt(1.0, 3.0)]],
            Some(Fill::Solid(Rgba::new(255, 0, 0, 128))),
            None,
            "x",
        ));
        let m = PdfMeta { pdfa: true, keywords: "άδεια".into(), ..meta() };
        let pdf =
            write_pdf_with_fonts(&[PdfPage::new(d.clone(), 297.0, 210.0)], &m, &Fonts::builtin()).unwrap();
        let (_, bodies) = check_structure(&pdf);
        assert!(bodies[0].contains("/OutputIntents [<< /Type /OutputIntent /S /GTS_PDFA1"));
        assert!(bodies[3].contains("/Subtype /XML") && !bodies[3].contains("/Filter"));
        assert!(bodies[3].contains("<pdfaid:part>1</pdfaid:part>"));
        assert!(bodies[3].contains("Κάτοψη (ισόγειο)") && bodies[3].contains("2026-10-09T12:00:00+00:00"));
        assert!(bodies[4].contains("/N 3"));
        assert!(!bodies[5].contains("/ExtGState"));
        // The same drawing without PDF/A uses a transparency state.
        let pdf = write_pdf_with_fonts(&[PdfPage::new(d, 297.0, 210.0)], &meta(), &Fonts::builtin()).unwrap();
        let (_, bodies) = check_structure(&pdf);
        assert!(bodies[3].contains("/GSF128 << /Type /ExtGState /ca 0.502 >>"));
    }

    #[test]
    fn errors_and_strings() {
        assert_eq!(write_pdf_with_fonts(&[], &meta(), &Fonts::builtin()), Err(PdfError::NoPages));
        let bad = PdfPage::new(Drawing::paper(), 0.0, 100.0);
        assert_eq!(
            write_pdf_with_fonts(&[bad], &meta(), &Fonts::builtin()),
            Err(PdfError::InvalidPageSize { page: 0 })
        );
        assert_eq!(text_string("a(b)\\"), "(a\\(b\\)\\\\)");
        assert_eq!(text_string("Ά"), "<FEFF0386>");
        let a = write_pdf_with_fonts(&[PdfPage::new(sample(), 297.0, 210.0)], &meta(), &Fonts::builtin())
            .unwrap();
        let b = write_pdf_with_fonts(&[PdfPage::new(sample(), 297.0, 210.0)], &meta(), &Fonts::builtin())
            .unwrap();
        assert_eq!(a, b, "output is deterministic for a fixed date");
    }

    #[test]
    fn dates() {
        let d = PdfDate::from_unix(0);
        assert_eq!((d.year, d.month, d.day), (1970, 1, 1));
        let d = PdfDate::from_unix(1_791_547_200); // 2026-10-09 12:00:00 UTC
        assert_eq!(d, PdfDate { year: 2026, month: 10, day: 9, hour: 12, minute: 0, second: 0 });
        let d = PdfDate::from_unix(951_782_400); // 2000-02-29
        assert_eq!((d.year, d.month, d.day), (2000, 2, 29));
    }
}
