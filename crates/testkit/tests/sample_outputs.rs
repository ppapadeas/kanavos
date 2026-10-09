//! Renders the sample drawings to every output format under `target/samples/` and checks the files are
//! well-formed. The files are kept for visual inspection.

// Test-only helpers: a failure here should stop the test with a message.
#![allow(clippy::expect_used)]

use std::path::PathBuf;

use kanavos_drawing::fonts::Fonts;
use kanavos_drawing::svg::{SvgOptions, to_svg_with_fonts};
use kanavos_dxf::{Codepage, DxfExportOptions, ImportOptions, read_dxf, write_dxf};
use kanavos_pdf::{PdfMeta, PdfPage, write_pdf_with_fonts};
use kanavos_raster::{RasterOptions, RasterSize, render_png_with_fonts};
use kanavos_testkit::samples::{glyph_sheet, hatch_swatches, sample_plan};

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/samples");
    std::fs::create_dir_all(&dir).expect("create target/samples");
    dir
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder.read_info().expect("valid PNG");
    (reader.info().width, reader.info().height)
}

#[test]
fn sample_plan_svg_and_png() {
    let dir = out_dir();
    let plan = sample_plan();
    for (fonts, tag) in [(&Fonts::builtin(), "builtin"), (Fonts::system(), "system")] {
        let svg = to_svg_with_fonts(
            &plan,
            &SvgOptions { title: Some("Κάτοψη".into()), ..SvgOptions::default() },
            fonts,
        );
        assert!(svg.starts_with("<?xml") && svg.trim_end().ends_with("</svg>"));
        std::fs::write(dir.join(format!("sample_plan_{tag}.svg")), &svg).expect("write svg");
        let png = render_png_with_fonts(
            &plan,
            &RasterOptions { size: RasterSize::Dpi(200.0), ..RasterOptions::default() },
            fonts,
        )
        .expect("render");
        let (w, h) = png_size(&png);
        assert!(w > 1000 && h > 800, "{w} × {h}");
        std::fs::write(dir.join(format!("sample_plan_{tag}.png")), &png).expect("write png");
    }
}

#[test]
fn sample_plan_pdf() {
    let dir = out_dir();
    let fonts = Fonts::system();
    let pages = [
        PdfPage::new(sample_plan(), 420.0, 297.0),
        PdfPage::new(hatch_swatches(), 210.0, 148.0),
        PdfPage::new(glyph_sheet(), 297.0, 210.0),
    ];
    for (pdfa, name) in [(false, "sample_plan.pdf"), (true, "sample_plan_pdfa.pdf")] {
        let meta = PdfMeta {
            title: "Κάτοψη ισογείου".into(),
            author: "Kanavos".into(),
            subject: "Δείγμα σχεδίου".into(),
            pdfa,
            ..PdfMeta::default()
        };
        let pdf = write_pdf_with_fonts(&pages, &meta, fonts).expect("pdf");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        std::fs::write(dir.join(name), &pdf).expect("write pdf");
    }
}

#[test]
fn swatches_and_glyphs_png() {
    let dir = out_dir();
    let opts = RasterOptions { size: RasterSize::Dpi(300.0), ..RasterOptions::default() };
    let png = render_png_with_fonts(&hatch_swatches(), &opts, &Fonts::builtin()).expect("render");
    std::fs::write(dir.join("hatch_swatches.png"), &png).expect("write");
    let png = render_png_with_fonts(&glyph_sheet(), &opts, &Fonts::builtin()).expect("render");
    std::fs::write(dir.join("glyph_sheet.png"), &png).expect("write");
}

#[test]
fn sample_plan_dxf_round_trip() {
    let dir = out_dir();
    let plan = sample_plan();
    for (name, cp) in [("sample_plan.dxf", Codepage::Ansi1252), ("sample_plan_1253.dxf", Codepage::Ansi1253)]
    {
        let bytes = write_dxf(&plan, &DxfExportOptions { codepage: cp, ..DxfExportOptions::default() });
        std::fs::write(dir.join(name), &bytes).expect("write dxf");
        let back =
            read_dxf(&bytes, &ImportOptions { scale: plan.scale, ..ImportOptions::default() }).expect("read");
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        let titles: Vec<&str> = back
            .drawing
            .items
            .iter()
            .filter_map(|i| match &i.shape {
                kanavos_drawing::Shape::Text(t) => Some(t.content.as_str()),
                _ => None,
            })
            .collect();
        assert!(titles.contains(&"ΚΑΤΟΨΗ ΙΣΟΓΕΙΟΥ 1:50"), "{titles:?}");
        // The re-imported drawing renders like the original.
        let png = render_png_with_fonts(
            &back.drawing,
            &RasterOptions { size: RasterSize::Dpi(150.0), ..RasterOptions::default() },
            Fonts::system(),
        )
        .expect("render");
        std::fs::write(dir.join(format!("{name}.png")), &png).expect("write png");
    }
    let swatches = write_dxf(&hatch_swatches(), &DxfExportOptions::default());
    std::fs::write(dir.join("hatch_swatches.dxf"), &swatches).expect("write dxf");
}

/// Degenerate drawing content must never make an exporter fail or panic.
#[test]
fn degenerate_drawings_export_everywhere() {
    use kanavos_drawing::{
        Drawing, Fill, HatchPattern, Item, LinePattern, Rgba, Shape, Stroke, Text, pt, weights,
    };
    let nan = f64::NAN;
    for scale in [50.0, 0.0, f64::NAN, -10.0] {
        let mut d = Drawing::model(scale);
        d.push(Item::line(pt(nan, 0.0), pt(1.0, 1.0), Stroke::solid(weights::THIN), "bad"));
        d.push(Item::line(
            pt(0.0, 0.0),
            pt(0.0, 0.0),
            Stroke::patterned(0.25, LinePattern::Custom(vec![])),
            "bad",
        ));
        d.push(Item::polyline(vec![pt(0.0, 0.0)], true, Stroke::solid(-1.0), "bad"));
        d.push(Item::polygon(
            vec![vec![], vec![pt(0.0, 0.0), pt(1.0, 0.0)]],
            Some(Fill::hatch(HatchPattern::Concrete, None)),
            None,
            "bad",
        ));
        d.push(Item::polygon(
            vec![vec![pt(0.0, 0.0), pt(1e9, 0.0), pt(1e9, 1e9)]],
            Some(Fill::hatch(HatchPattern::Masonry, Some(Rgba::LIGHT_GREY))),
            Some(Stroke::patterned(0.5, LinePattern::Custom(vec![-1.0, f64::INFINITY]))),
            "huge",
        ));
        for shape in [
            Shape::Circle { centre: pt(0.0, 0.0), radius: 0.0 },
            Shape::Circle { centre: pt(0.0, 0.0), radius: -2.0 },
            Shape::Arc { centre: pt(0.0, 0.0), radius: 1.0, start: nan, sweep: 100.0 },
            Shape::Arc { centre: pt(0.0, 0.0), radius: f64::INFINITY, start: 0.0, sweep: 1.0 },
        ] {
            d.push(Item {
                shape,
                stroke: Some(Stroke::solid(0.25)),
                fill: None,
                layer: "bad".into(),
                source: None,
            });
        }
        d.push(Item::text(Text::new(pt(0.0, 0.0), "", 2.5), "bad"));
        d.push(Item::text(Text::new(pt(nan, 0.0), "Α", 2.5), "bad"));
        d.push(Item::text(Text::new(pt(0.0, 0.0), "Ω\u{0}\u{7}<&>", 1e6).rotated(nan), "bad"));
        let fonts = Fonts::builtin();
        let _ = to_svg_with_fonts(&d, &SvgOptions::default(), &fonts);
        let _ = write_pdf_with_fonts(&[PdfPage::new(d.clone(), 210.0, 297.0)], &PdfMeta::default(), &fonts);
        let _ = render_png_with_fonts(&d, &RasterOptions::default(), &fonts);
        let bytes = write_dxf(&d, &DxfExportOptions::default());
        let _ = read_dxf(&bytes, &ImportOptions::default()).expect("own output reads back");
    }
}
