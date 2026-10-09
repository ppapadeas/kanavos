//! DXF import tests on hand-written files, and export → import round trips.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kanavos_drawing::{
    Anchor, Drawing, Fill, HatchPattern, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt, weights,
};
use kanavos_dxf::{
    Codepage, DxfError, DxfExportOptions, DxfImport, ImportOptions, PointSource, placement_from_georef,
    read_dxf, write_dxf,
};
use kanavos_geo::GridPlacement;

/// A DXF value: text or raw bytes (for code-page encoded text).
enum V<'a> {
    S(&'a str),
    B(&'a [u8]),
}

fn dxf(pairs: &[(i32, V<'_>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (code, v) in pairs {
        out.extend_from_slice(format!("{code:>3}\r\n").as_bytes());
        match v {
            V::S(s) => out.extend_from_slice(s.as_bytes()),
            V::B(b) => out.extend_from_slice(b),
        }
        out.extend_from_slice(b"\r\n");
    }
    out
}

use V::{B, S};

// Windows-1253 encodings of the Greek strings used below.
const ORIA: &[u8] = &[0xCF, 0xD1, 0xC9, 0xC1]; // ΟΡΙΑ
const OIKOPEDO: &[u8] = &[0xCF, 0xE9, 0xEA, 0xFC, 0xF0, 0xE5, 0xE4, 0xEF]; // Οικόπεδο
const AR: &[u8] = &[0xC1, 0xD1]; // ΑΡ
const SIGMA12: &[u8] = &[0xD3, 0x31, 0x32]; // Σ12
const MTEXT: &[u8] = &[
    b'{', b'\\', b'f', b'S', b'a', b'n', b's', b';', 0xD0, 0xEF, 0xEB, 0xEB, 0xDD, 0xF2, b'}', b'\\', b'P',
    0xE3, 0xF1, 0xE1, 0xEC, 0xEC, 0xDD, 0xF2,
]; // {\fSans;Πολλές}\Pγραμμές

fn survey_file() -> Vec<u8> {
    dxf(&[
        (0, S("SECTION")),
        (2, S("HEADER")),
        (9, S("$ACADVER")),
        (1, S("AC1009")),
        (9, S("$DWGCODEPAGE")),
        (3, S("ANSI_1253")),
        (9, S("$INSUNITS")),
        (70, S("6")),
        (0, S("ENDSEC")),
        (0, S("SECTION")),
        (2, S("TABLES")),
        (0, S("TABLE")),
        (2, S("LTYPE")),
        (0, S("LTYPE")),
        (2, S("DASHED")),
        (70, S("0")),
        (3, S("__ __")),
        (72, S("65")),
        (73, S("2")),
        (40, S("0.75")),
        (49, S("0.5")),
        (49, S("-0.25")),
        (0, S("ENDTAB")),
        (0, S("TABLE")),
        (2, S("LAYER")),
        (0, S("LAYER")),
        (2, B(ORIA)),
        (70, S("0")),
        (62, S("1")),
        (6, S("CONTINUOUS")),
        (0, S("LAYER")),
        (2, S("POINTS")),
        (70, S("0")),
        (62, S("3")),
        (6, S("CONTINUOUS")),
        (0, S("LAYER")),
        (2, S("FENCE")),
        (70, S("0")),
        (62, S("-5")),
        (6, S("DASHED")),
        (0, S("ENDTAB")),
        (0, S("ENDSEC")),
        (0, S("SECTION")),
        (2, S("BLOCKS")),
        (0, S("BLOCK")),
        (8, S("0")),
        (2, S("PT")),
        (70, S("2")),
        (10, S("0.0")),
        (20, S("0.0")),
        (30, S("0.0")),
        (0, S("CIRCLE")),
        (8, S("0")),
        (62, S("0")),
        (10, S("0.0")),
        (20, S("0.0")),
        (40, S("0.2")),
        (0, S("ATTDEF")),
        (8, S("0")),
        (10, S("0.3")),
        (20, S("0.0")),
        (40, S("0.5")),
        (1, S("")),
        (2, S("H")),
        (3, S("Elevation")),
        (0, S("ENDBLK")),
        (0, S("BLOCK")),
        (8, S("0")),
        (2, S("NEST")),
        (70, S("0")),
        (10, S("0.0")),
        (20, S("0.0")),
        (0, S("INSERT")),
        (8, S("0")),
        (2, S("PT")),
        (10, S("1.0")),
        (20, S("0.0")),
        (0, S("LINE")),
        (8, S("0")),
        (10, S("0.0")),
        (20, S("0.0")),
        (11, S("1.0")),
        (21, S("0.0")),
        (0, S("ENDBLK")),
        (0, S("BLOCK")),
        (8, S("0")),
        (2, S("*D1")),
        (70, S("1")),
        (10, S("0.0")),
        (20, S("0.0")),
        (0, S("LINE")),
        (8, S("0")),
        (10, S("480000.0")),
        (20, S("4199995.0")),
        (11, S("480002.0")),
        (21, S("4199995.0")),
        (0, S("TEXT")),
        (8, S("0")),
        (10, S("480001.0")),
        (20, S("4199995.2")),
        (40, S("0.35")),
        (1, S("2,00")),
        (0, S("ENDBLK")),
        (0, S("ENDSEC")),
        (0, S("SECTION")),
        (2, S("ENTITIES")),
        // Plot boundary with one bulged edge.
        (0, S("LWPOLYLINE")),
        (8, B(ORIA)),
        (90, S("4")),
        (70, S("1")),
        (10, S("480000.0")),
        (20, S("4200000.0")),
        (10, S("480030.0")),
        (20, S("4200000.0")),
        (42, S("0.2")),
        (10, S("480030.0")),
        (20, S("4200020.0")),
        (10, S("480000.0")),
        (20, S("4200020.0")),
        // Survey point with elevation.
        (0, S("POINT")),
        (8, S("POINTS")),
        (10, S("480010.0")),
        (20, S("4200005.0")),
        (30, S("125.34")),
        // 2D point with an elevation text next to it.
        (0, S("POINT")),
        (8, S("POINTS")),
        (10, S("480020.0")),
        (20, S("4200005.0")),
        (30, S("0.0")),
        (0, S("TEXT")),
        (8, S("POINTS")),
        (10, S("480020.5")),
        (20, S("4200005.3")),
        (40, S("0.5")),
        (1, S("+126,10")),
        // Greek text, centred.
        (0, S("TEXT")),
        (8, B(ORIA)),
        (10, S("480014.0")),
        (20, S("4200009.5")),
        (40, S("1.0")),
        (1, B(OIKOPEDO)),
        (72, S("1")),
        (11, S("480015.0")),
        (21, S("4200010.0")),
        (73, S("2")),
        // Text written with Unicode escapes.
        (0, S("TEXT")),
        (8, S("0")),
        (10, S("480001.0")),
        (20, S("4200001.0")),
        (40, S("0.5")),
        (1, S("\\U+0391\\U+0392 %%p0.05")),
        // Survey point block with label and elevation attributes.
        (0, S("INSERT")),
        (8, S("POINTS")),
        (66, S("1")),
        (2, S("PT")),
        (10, S("480005.0")),
        (20, S("4200015.0")),
        (0, S("ATTRIB")),
        (8, S("POINTS")),
        (10, S("480005.3")),
        (20, S("4200015.0")),
        (40, S("0.5")),
        (1, S("124.80")),
        (2, S("H")),
        (0, S("ATTRIB")),
        (8, S("POINTS")),
        (10, S("480005.3")),
        (20, S("4200015.6")),
        (40, S("0.5")),
        (1, B(SIGMA12)),
        (2, B(AR)),
        (0, S("SEQEND")),
        // Multi-line text.
        (0, S("MTEXT")),
        (8, S("0")),
        (10, S("480002.0")),
        (20, S("4200018.0")),
        (40, S("0.5")),
        (71, S("1")),
        (1, B(MTEXT)),
        // A nested block, scaled 2 and rotated 90°.
        (0, S("INSERT")),
        (8, S("FENCE")),
        (2, S("NEST")),
        (10, S("480050.0")),
        (20, S("4200050.0")),
        (41, S("2.0")),
        (42, S("2.0")),
        (50, S("90.0")),
        // A dimension whose text must not become an elevation.
        (0, S("DIMENSION")),
        (8, S("0")),
        (2, S("*D1")),
        // Mirrored circle (extrusion −Z): OCS x is world −x.
        (0, S("CIRCLE")),
        (8, S("0")),
        (10, S("-480040.0")),
        (20, S("4200030.0")),
        (40, S("1.5")),
        (210, S("0.0")),
        (220, S("0.0")),
        (230, S("-1.0")),
        // Ellipse, spline and arc.
        (0, S("ELLIPSE")),
        (8, S("0")),
        (10, S("480060.0")),
        (20, S("4200000.0")),
        (11, S("2.0")),
        (21, S("0.0")),
        (40, S("0.5")),
        (41, S("0.0")),
        (42, S("6.283185307179586")),
        (0, S("SPLINE")),
        (8, S("0")),
        (70, S("8")),
        (71, S("2")),
        (72, S("6")),
        (73, S("3")),
        (40, S("0.0")),
        (40, S("0.0")),
        (40, S("0.0")),
        (40, S("1.0")),
        (40, S("1.0")),
        (40, S("1.0")),
        (10, S("480070.0")),
        (20, S("4200000.0")),
        (10, S("480072.0")),
        (20, S("4200004.0")),
        (10, S("480074.0")),
        (20, S("4200000.0")),
        (0, S("ARC")),
        (8, S("FENCE")),
        (10, S("480080.0")),
        (20, S("4200000.0")),
        (40, S("1.0")),
        (50, S("0.0")),
        (51, S("90.0")),
        (0, S("ENDSEC")),
        (0, S("EOF")),
    ])
}

fn import() -> DxfImport {
    read_dxf(&survey_file(), &ImportOptions::default()).expect("valid file")
}

fn texts(d: &Drawing) -> Vec<&Text> {
    d.items
        .iter()
        .filter_map(|i| match &i.shape {
            Shape::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn header_layers_and_code_page() {
    let imp = import();
    assert_eq!(imp.version, "AC1009");
    assert_eq!(imp.codepage, Codepage::Ansi1253);
    assert!((imp.metres_per_unit - 1.0).abs() < 1e-12);
    let oria = imp.layers.iter().find(|l| l.name == "ΟΡΙΑ").expect("Greek layer name decoded");
    assert_eq!(oria.color, Rgba::rgb(255, 0, 0));
    let fence = imp.layers.iter().find(|l| l.name == "FENCE").unwrap();
    assert!(!fence.visible, "negative colour means the layer is off");
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
}

#[test]
fn greek_and_escaped_text() {
    let imp = import();
    let t = texts(&imp.drawing);
    let oik = t.iter().find(|t| t.content == "Οικόπεδο").expect("Windows-1253 text decoded");
    assert_eq!(oik.anchor, Anchor::Centre);
    assert!(oik.position.distance(pt(480015.0, 4200010.0)) < 1e-9);
    // 1 m high text at the default 1:200 is 5 mm on paper.
    assert!((oik.height_mm - 5.0).abs() < 1e-9);
    assert!(t.iter().any(|t| t.content == "ΑΒ ±0.05"), "escapes and %% codes");
    assert!(
        t.iter().any(|t| t.content == "Πολλές") && t.iter().any(|t| t.content == "γραμμές"),
        "MTEXT lines"
    );
    assert!(t.iter().any(|t| t.content == "Σ12"), "attribute text");
}

#[test]
fn survey_points_and_elevations() {
    let imp = import();
    let p = &imp.points;
    let z = |x: f64, y: f64| {
        p.iter()
            .find(|q| (q.position[0] - x).abs() < 1e-6 && (q.position[1] - y).abs() < 1e-6)
            .expect("point")
    };
    let a = z(480010.0, 4200005.0);
    assert!(a.has_elevation && (a.position[2] - 125.34).abs() < 1e-9 && a.source == PointSource::Point);
    let b = z(480020.0, 4200005.0);
    assert!((b.position[2] - 126.10).abs() < 1e-9 && b.source == PointSource::TextNearPoint);
    let c = z(480005.0, 4200015.0);
    assert!((c.position[2] - 124.80).abs() < 1e-9 && c.source == PointSource::Insert);
    assert_eq!(c.label.as_deref(), Some("Σ12"));
    // The attribute text "124.80" labels the insert's own elevation; the dimension text "2,00" is not an elevation.
    assert_eq!(p.len(), 3, "{p:?}");
}

#[test]
fn plot_boundary_with_bulge() {
    let imp = import();
    assert_eq!(imp.closed_polylines.len(), 1, "only top-level closed polylines");
    let c = &imp.closed_polylines[0];
    assert_eq!(c.layer, "ΟΡΙΑ");
    // 30 × 20 rectangle plus the circular segment of the bulged edge (bulge 0.2 on a 20 m chord: ≈ 26.8 m²).
    assert!(c.area > 620.0 && c.area < 632.0, "area {}", c.area);
    assert!(c.points.len() > 8, "the arc is flattened");
}

#[test]
fn blocks_nest_scale_rotate_and_inherit() {
    let imp = import();
    // NEST at (480050, 4200050), ×2, 90°: its PT insert at (1, 0) lands at (480050, 4200052), radius 0.4.
    let circle = imp
        .drawing
        .items
        .iter()
        .find_map(|i| match i.shape {
            Shape::Circle { centre, radius } if centre.distance(pt(480050.0, 4200052.0)) < 1e-6 => {
                Some((i, radius))
            }
            _ => None,
        })
        .expect("nested circle");
    assert!((circle.1 - 0.4).abs() < 1e-9);
    // Layer 0 inside the block takes the insert's layer; BYBLOCK colour takes the insert's colour (layer FENCE, 5).
    assert_eq!(circle.0.layer, "FENCE");
    assert_eq!(circle.0.stroke.as_ref().unwrap().color, Rgba::rgb(0, 0, 255));
    // The block's line, rotated: from (480050, 4200050) to (480050, 4200052), dashed by layer.
    let line = imp
        .drawing
        .items
        .iter()
        .find_map(|i| match i.shape {
            Shape::Line { a, b } if a.distance(pt(480050.0, 4200050.0)) < 1e-6 => Some((i, b)),
            _ => None,
        })
        .expect("block line");
    assert!(line.1.distance(pt(480050.0, 4200052.0)) < 1e-6);
    // DASHED 0.5 / 0.25 m at 1:200 is 2.5 / 1.25 mm.
    assert_eq!(line.0.stroke.as_ref().unwrap().pattern, LinePattern::Custom(vec![2.5, 1.25]));
}

#[test]
fn ocs_curves_and_splines() {
    let imp = import();
    let d = &imp.drawing;
    assert!(d.items.iter().any(|i| matches!(i.shape,
        Shape::Circle { centre, radius } if centre.distance(pt(480040.0, 4200030.0)) < 1e-6 && (radius - 1.5).abs() < 1e-9)));
    let ellipse = d
        .items
        .iter()
        .find_map(|i| match &i.shape {
            Shape::Polyline { points, closed: true }
                if points.iter().all(|p| p.x > 480057.9 && p.x < 480062.1) =>
            {
                Some(points)
            }
            _ => None,
        })
        .expect("ellipse");
    let top = ellipse.iter().map(|p| p.y).fold(f64::MIN, f64::max);
    assert!((top - 4200001.0).abs() < 1e-3, "minor radius 1: {top}");
    let spline = d
        .items
        .iter()
        .find_map(|i| match &i.shape {
            Shape::Polyline { points, closed: false }
                if points.first().is_some_and(|p| p.distance(pt(480070.0, 4200000.0)) < 1e-9) =>
            {
                Some(points)
            }
            _ => None,
        })
        .expect("spline");
    // Quadratic Bézier apex: (480072, 4200002).
    let apex = spline.iter().map(|p| p.y).fold(f64::MIN, f64::max);
    assert!((apex - 4200002.0).abs() < 1e-3);
    assert!(d.items.iter().any(|i| matches!(i.shape, Shape::Arc { radius, sweep, .. }
        if (radius - 1.0).abs() < 1e-12 && (sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-12)));
    assert_eq!(imp.entity_counts.get("SPLINE"), Some(&1));
}

#[test]
fn project_coordinates_underlay_and_plot() {
    let imp = import();
    let g = GridPlacement { easting: 480000.0, northing: 4200000.0, rotation: 0.0, altitude: 120.0 };
    let local = imp.to_project(&g);
    let p = local.points.iter().find(|p| p.source == PointSource::Point && p.has_elevation).unwrap();
    assert!((p.position[0] - 10.0).abs() < 1e-9 && (p.position[1] - 5.0).abs() < 1e-9);
    assert!((p.position[2] - 5.34).abs() < 1e-9);
    let e = local.extents.unwrap();
    assert!(e.min.x > -10.0 && e.max.x < 100.0, "{e:?}");
    let plot = local.plot_polygon(0).unwrap();
    assert!(plot.is_ccw() && plot.len() > 8);
    assert!((plot.area().as_m2() - local.closed_polylines[0].area).abs() < 1e-6);
    let u = local.to_underlay("Τοπογραφικό", "survey.dxf");
    assert_eq!(u.drawing.items.len(), imp.drawing.items.len());
    assert_eq!(local.terrain_points().len(), 3);
    // Rotation: grid north 90° counter-clockwise from project +Y sends grid east to project +Y.
    let rotated = imp.to_project(&GridPlacement { rotation: std::f64::consts::FRAC_PI_2, ..g });
    let q = rotated.points.iter().find(|p| p.source == PointSource::Point && p.has_elevation).unwrap();
    assert!((q.position[0] + 5.0).abs() < 1e-9 && (q.position[1] - 10.0).abs() < 1e-9);
    // Model georeference helper.
    let geo = kanavos_model::Georef {
        epsg: 2100,
        easting: 480000.0,
        northing: 4200000.0,
        rotation: kanavos_geom::Angle::ZERO,
        altitude: 120.0,
    };
    assert_eq!(placement_from_georef(&geo), g);
    let t = imp.translated(imp.suggested_origin().unwrap());
    assert!(t.extents.unwrap().min.x >= 0.0 && t.extents.unwrap().min.x < 100.0);
}

#[test]
fn units_and_errors() {
    let mm = dxf(&[
        (0, S("SECTION")),
        (2, S("HEADER")),
        (9, S("$INSUNITS")),
        (70, S("4")),
        (0, S("ENDSEC")),
        (0, S("SECTION")),
        (2, S("ENTITIES")),
        (0, S("LINE")),
        (8, S("0")),
        (10, S("0")),
        (20, S("0")),
        (11, S("1000")),
        (21, S("0")),
        (0, S("ENDSEC")),
        (0, S("EOF")),
    ]);
    let imp = read_dxf(&mm, &ImportOptions::default()).unwrap();
    assert!((imp.metres_per_unit - 0.001).abs() < 1e-15);
    assert!(matches!(imp.drawing.items[0].shape, Shape::Line { b, .. } if (b.x - 1.0).abs() < 1e-12));
    assert!(matches!(read_dxf(b"", &ImportOptions::default()), Err(DxfError::Empty)));
    assert!(matches!(read_dxf(b"0\nLINE\n", &ImportOptions::default()), Err(DxfError::Malformed { .. })));
    assert!(matches!(
        read_dxf(b"x\ny\n", &ImportOptions::default()),
        Err(DxfError::Malformed { line: 1, .. })
    ));
    // Truncated files and unknown entities do not fail.
    let mut cut = survey_file();
    cut.truncate(cut.len() * 2 / 3);
    assert!(read_dxf(&cut, &ImportOptions::default()).is_ok());
}

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
        "roof",
    ));
    d.push(Item::polyline(
        vec![pt(0.0, 2.0), pt(1.0, 2.5), pt(2.0, 2.0)],
        false,
        Stroke::solid(0.35),
        "detail",
    ));
    d.push(Item {
        shape: Shape::Arc {
            centre: pt(1.0, 0.25),
            radius: 0.9,
            start: 0.0,
            sweep: std::f64::consts::FRAC_PI_2,
        },
        stroke: Some(Stroke::solid(weights::FINE).with_color(Rgba::rgb(255, 0, 0))),
        fill: None,
        layer: "door".into(),
        source: None,
    });
    d.push(Item {
        shape: Shape::Circle { centre: pt(3.0, 3.0), radius: 0.15 },
        stroke: Some(Stroke::solid(weights::THIN)),
        fill: Some(Fill::Solid(Rgba::BLACK)),
        layer: "column.cut".into(),
        source: None,
    });
    // An insulation layer on a white (masking) background.
    d.push(Item::polygon(
        vec![vec![pt(0.0, 5.0), pt(4.0, 5.0), pt(4.0, 5.05), pt(0.0, 5.05)]],
        Some(Fill::hatch(HatchPattern::Insulation, Some(Rgba::WHITE))),
        Some(Stroke::solid(weights::THICK)),
        "insulation",
    ));
    for (i, anchor) in
        [Anchor::BottomLeft, Anchor::BottomCentre, Anchor::Centre, Anchor::TopRight, Anchor::MiddleLeft]
            .into_iter()
            .enumerate()
    {
        let mut t =
            Text::new(pt(1.0 + i as f64, 4.0), format!("Κάτοψη {i} 50%"), 2.5).anchored(anchor).rotated(0.3);
        t.bold = i == 1;
        d.push(Item::text(t, "text"));
    }
    d
}

fn shapes<'a>(d: &'a Drawing, f: impl Fn(&Shape) -> bool + 'a) -> Vec<&'a Item> {
    d.items.iter().filter(|i| f(&i.shape)).collect()
}

#[test]
fn round_trip_preserves_geometry_text_and_styles() {
    let src = sample();
    for cp in [Codepage::Ansi1252, Codepage::Ansi1253] {
        let bytes = write_dxf(&src, &DxfExportOptions { codepage: cp, ..DxfExportOptions::default() });
        let imp = read_dxf(&bytes, &ImportOptions { scale: 50.0, ..ImportOptions::default() }).unwrap();
        assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
        let d = &imp.drawing;
        // Wall outline as a closed polyline, its background as a solid fill, its hatch lines on their own layer.
        let outlines = shapes(d, |s| matches!(s, Shape::Polyline { closed: true, .. }));
        assert!(
            outlines
                .iter()
                .any(|i| i.layer == "wall.cut" && (i.stroke.as_ref().unwrap().weight_mm - 0.7).abs() < 1e-12)
        );
        assert!(d.items.iter().any(|i| i.layer == "wall.cut" && matches!(i.fill, Some(Fill::Solid(_)))));
        let hatch = d.items.iter().filter(|i| i.layer == "wall.cut.hatch").count();
        assert!(hatch > 50, "{hatch} hatch lines");
        // The dashed line keeps its pattern in paper millimetres.
        let roof = d.items.iter().find(|i| i.layer == "roof").unwrap();
        assert_eq!(roof.stroke.as_ref().unwrap().pattern.dashes_mm(), vec![3.0, 1.5]);
        assert!(
            matches!(roof.shape, Shape::Line { a, b } if a.distance(pt(0.0, 1.0)) < 1e-9 && b.distance(pt(4.0, 1.0)) < 1e-9)
        );
        // Open polyline, arc with colour, filled circle.
        assert!(d.items.iter().any(|i| i.layer == "detail"
            && matches!(&i.shape, Shape::Polyline { points, closed: false } if points.len() == 3)));
        let arc = d.items.iter().find(|i| i.layer == "door").unwrap();
        assert_eq!(arc.stroke.as_ref().unwrap().color, Rgba::rgb(255, 0, 0));
        assert!(matches!(arc.shape, Shape::Arc { radius, start, sweep, .. }
            if (radius - 0.9).abs() < 1e-9 && start.abs() < 1e-9 && (sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-9));
        assert!(d.items.iter().any(|i| i.layer == "column.cut"
            && matches!(i.shape, Shape::Circle { radius, .. } if (radius - 0.15).abs() < 1e-9)));
        assert!(
            d.items
                .iter()
                .any(|i| i.layer == "column.cut" && matches!(i.fill, Some(Fill::Solid(Rgba::BLACK))))
        );
        // A white background stays white (not the adaptive black/white index).
        assert!(
            d.items
                .iter()
                .any(|i| i.layer == "insulation" && matches!(i.fill, Some(Fill::Solid(Rgba::WHITE))))
        );
        assert!(d.items.iter().filter(|i| i.layer == "insulation.hatch").count() > 10);
        // Texts: content (Greek and a literal percent), height, rotation, anchor and weight survive.
        let src_texts = texts(&src);
        let got = texts(d);
        assert_eq!(got.len(), src_texts.len());
        for (a, b) in src_texts.iter().zip(&got) {
            assert_eq!(a.content, b.content, "{cp:?}");
            assert_eq!(a.anchor, b.anchor);
            assert!(a.position.distance(b.position) < 1e-9);
            assert!((a.height_mm - b.height_mm).abs() < 1e-9);
            assert!((a.rotation - b.rotation).abs() < 1e-9);
            assert_eq!(a.bold, b.bold);
        }
    }
}

#[test]
fn round_trip_in_millimetres() {
    let src = sample();
    let bytes = write_dxf(
        &src,
        &DxfExportOptions { units: Some(kanavos_dxf::DxfUnits::Millimetres), ..DxfExportOptions::default() },
    );
    let imp = read_dxf(&bytes, &ImportOptions { scale: 50.0, ..ImportOptions::default() }).unwrap();
    assert!((imp.metres_per_unit - 0.001).abs() < 1e-15);
    let roof = imp.drawing.items.iter().find(|i| i.layer == "roof").unwrap();
    assert!(matches!(roof.shape, Shape::Line { b, .. } if b.distance(pt(4.0, 1.0)) < 1e-9));
    let t = texts(&imp.drawing);
    assert!((t[0].height_mm - 2.5).abs() < 1e-9);
    let roof_pattern = roof.stroke.as_ref().unwrap().pattern.dashes_mm();
    assert!(roof_pattern.iter().zip([3.0, 1.5]).all(|(a, b)| (a - b).abs() < 1e-9));
}

/// Every closed ring of the exported HATCH boundaries round-trips (paper drawing in millimetres).
#[test]
fn paper_drawings_round_trip() {
    let mut d = Drawing::paper();
    d.push(Item::polyline(
        vec![pt(10.0, 10.0), pt(410.0, 10.0), pt(410.0, 287.0), pt(10.0, 287.0)],
        true,
        Stroke::solid(1.0),
        "border",
    ));
    d.push(Item::text(Text::new(pt(20.0, 20.0), "ΦΥΛΛΟ 1", 5.0), "title"));
    let bytes = write_dxf(&d, &DxfExportOptions::default());
    assert!(String::from_utf8_lossy(&bytes).contains("$INSUNITS\r\n 70\r\n4\r\n"));
    let imp = read_dxf(&bytes, &ImportOptions { scale: 1.0, ..ImportOptions::default() }).unwrap();
    let border: Vec<Pt> = imp
        .drawing
        .items
        .iter()
        .find_map(|i| match &i.shape {
            Shape::Polyline { points, closed: true } => Some(points.clone()),
            _ => None,
        })
        .unwrap();
    // Millimetres in the file, metres after import.
    assert!(border[1].distance(pt(0.41, 0.01)) < 1e-12);
    assert_eq!(imp.closed_polylines.len(), 1);
    assert!((imp.closed_polylines[0].area - 0.4 * 0.277).abs() < 1e-9);
}

/// A small deterministic generator for mutations (64-bit linear congruential, Knuth's constants).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Corrupted files must never panic, hang or allocate without bound: they import partially or fail with an error.
#[test]
fn corrupted_files_never_panic() {
    let seeds = [survey_file(), write_dxf(&sample(), &DxfExportOptions::default())];
    let bytes: &[u8] = b"0123456789-.eE\n\r \x00\xFF9999999999999999999-1e308ABC";
    let mut rng = Lcg(42);
    let opts = ImportOptions { max_items: 50_000, ..ImportOptions::default() };
    for seed in &seeds {
        for _ in 0..300 {
            let mut data = seed.clone();
            for _ in 0..1 + rng.below(8) {
                let pos = rng.below(data.len());
                let b = bytes[rng.below(bytes.len())];
                match rng.below(3) {
                    0 => data[pos] = b,
                    1 => data.insert(pos, b),
                    _ => {
                        data.remove(pos);
                    }
                }
            }
            let _ = read_dxf(&data, &opts);
        }
        for cut in (0..seed.len()).step_by(seed.len() / 200 + 1) {
            let _ = read_dxf(&seed[..cut], &opts);
        }
    }
}

/// Huge counts in a hatch boundary or a mesh must not allocate or loop on them.
#[test]
fn hostile_counts_are_bounded() {
    let data = dxf(&[
        (0, S("SECTION")),
        (2, S("ENTITIES")),
        (0, S("HATCH")),
        (8, S("0")),
        (10, S("0")),
        (20, S("0")),
        (30, S("0")),
        (2, S("SOLID")),
        (70, S("1")),
        (91, S("9223372036854775807")),
        (92, S("2")),
        (72, S("0")),
        (73, S("1")),
        (93, S("1000000000000000")),
        (10, S("0")),
        (20, S("0")),
        (97, S("99999999999999999")),
        (0, S("POLYLINE")),
        (8, S("0")),
        (70, S("16")),
        (71, S("4294967296")),
        (72, S("4294967296")),
        (0, S("VERTEX")),
        (10, S("0")),
        (20, S("0")),
        (0, S("SEQEND")),
        (0, S("ENDSEC")),
        (0, S("EOF")),
    ]);
    let start = std::time::Instant::now();
    assert!(read_dxf(&data, &ImportOptions::default()).is_ok());
    assert!(start.elapsed().as_secs() < 5);
}

/// Blocks nested ten deep with twenty inserts each would expand to 20¹⁰ entities: import stops with a warning.
#[test]
fn exploding_block_nesting_is_cut_short() {
    let names: Vec<String> = (0..=10).map(|i| format!("B{i}")).collect();
    let mut pairs: Vec<(i32, V<'_>)> = vec![(0, S("SECTION")), (2, S("BLOCKS"))];
    pairs.extend([(0, S("BLOCK")), (2, S(&names[0])), (10, S("0")), (20, S("0"))]);
    pairs.extend([(0, S("POINT")), (10, S("0")), (20, S("0")), (0, S("ENDBLK"))]);
    for level in 1..=10 {
        pairs.extend([(0, S("BLOCK")), (2, S(&names[level])), (10, S("0")), (20, S("0"))]);
        for _ in 0..20 {
            pairs.extend([(0, S("INSERT")), (2, S(&names[level - 1])), (10, S("0")), (20, S("0"))]);
        }
        pairs.push((0, S("ENDBLK")));
    }
    pairs.extend([(0, S("ENDSEC")), (0, S("SECTION")), (2, S("ENTITIES"))]);
    pairs.extend([(0, S("INSERT")), (2, S("B10")), (10, S("0")), (20, S("0"))]);
    pairs.extend([(0, S("ENDSEC")), (0, S("EOF"))]);
    let start = std::time::Instant::now();
    let imp =
        read_dxf(&dxf(&pairs), &ImportOptions { max_items: 10_000, ..ImportOptions::default() }).unwrap();
    assert!(start.elapsed().as_secs() < 10);
    assert!(imp.drawing.items.len() <= 10_000);
    assert!(!imp.warnings.is_empty());
}
