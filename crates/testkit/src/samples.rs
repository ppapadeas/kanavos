//! Sample drawings for exporter and renderer tests.
//!
//! - [`sample_plan`]: a small ground-floor plan at 1:50 with layered walls (masonry and insulation), a terrace with
//!   tiles and gravel, a column, a door, a window, dimensions, Greek room labels and a title.
//! - [`hatch_swatches`]: every hatch pattern on a paper sheet, labelled.
//! - [`glyph_sheet`]: every character of the built-in stroke font.

use std::f64::consts::FRAC_PI_2;

use kanavos_drawing::fonts::stroke::covered_chars;
use kanavos_drawing::{
    Anchor, Drawing, Fill, HatchPattern, Item, LinePattern, Pt, Rgba, Shape, Stroke, Text, pt, weights,
};

/// A mitred layer of a rectangular building's exterior wall between inward offsets `d0` and `d1`, one quad per side
/// (bottom, right, top, left).
fn ring_layers(w: f64, h: f64, d0: f64, d1: f64) -> [Vec<Pt>; 4] {
    [
        vec![pt(d0, d0), pt(w - d0, d0), pt(w - d1, d1), pt(d1, d1)],
        vec![pt(w - d0, d0), pt(w - d0, h - d0), pt(w - d1, h - d1), pt(w - d1, d1)],
        vec![pt(w - d0, h - d0), pt(d0, h - d0), pt(d1, h - d1), pt(w - d1, h - d1)],
        vec![pt(d0, h - d0), pt(d0, d0), pt(d1, d1), pt(d1, h - d1)],
    ]
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)]
}

fn cut(rings: Vec<Vec<Pt>>, pattern: HatchPattern, background: Option<Rgba>) -> Item {
    Item::polygon(
        rings,
        Some(Fill::hatch(pattern, background)),
        Some(Stroke::solid(weights::THICK)),
        "wall.cut",
    )
}

/// Splits a bottom-wall layer quad at a window opening `[x0, x1]` (square ends at the opening).
fn split_at_opening(q: &[Pt], x0: f64, x1: f64) -> Vec<Vec<Pt>> {
    let (Some(a), Some(b), Some(c), Some(d)) = (q.first(), q.get(1), q.get(2), q.get(3)) else {
        return Vec::new();
    };
    vec![vec![*a, pt(x0, a.y), pt(x0, d.y), *d], vec![pt(x1, b.y), *b, *c, pt(x1, c.y)]]
}

fn dim_tick(p: Pt) -> Item {
    let s = 0.1;
    Item::line(p.offset(-s, -s), p.offset(s, s), Stroke::solid(weights::MEDIUM), "dimension")
}

/// A small house plan at 1:50, in metres.
pub fn sample_plan() -> Drawing {
    let (w, h) = (10.0, 8.0);
    let mut d = Drawing::model(50.0);

    // Plot boundary and overhead roof outline.
    d.push(Item::polyline(
        rect(-2.0, -3.6, 15.0, 10.0),
        true,
        Stroke::patterned(weights::THIN, LinePattern::DashDotDot),
        "site.plot",
    ));
    d.push(Item::polyline(
        rect(-0.6, -0.6, w + 0.6, h + 0.6),
        true,
        Stroke::patterned(weights::FINE, LinePattern::Dashed),
        "roof.above",
    ));

    // Terrace: tiles, gravel strip, a reinforced concrete column.
    d.push(Item::polygon(
        vec![rect(w, 2.0, 13.0, 6.0)],
        Some(Fill::hatch(HatchPattern::Grid, None)),
        Some(Stroke::solid(weights::FINE)),
        "slab.surface",
    ));
    d.push(Item::polygon(
        vec![rect(w, 0.0, 13.0, 2.0)],
        Some(Fill::hatch(HatchPattern::Gravel, None)),
        Some(Stroke::solid(weights::FINE)),
        "site.surface",
    ));
    d.push(Item::polygon(
        vec![rect(12.4, 5.4, 12.7, 5.7)],
        Some(Fill::hatch(HatchPattern::CrossDiagonal, Some(Rgba::WHITE))),
        Some(Stroke::solid(weights::THICK)),
        "column.cut",
    ));

    // Exterior walls: 10 cm masonry, 5 cm insulation, 10 cm masonry. Window in the bottom wall at x 2.0–3.5.
    let (wx0, wx1) = (2.0, 3.5);
    for (d0, d1, pattern, bg) in [
        (0.0, 0.10, HatchPattern::Masonry, Some(Rgba::LIGHT_GREY)),
        (0.10, 0.15, HatchPattern::Insulation, Some(Rgba::WHITE)),
        (0.15, 0.25, HatchPattern::Masonry, Some(Rgba::LIGHT_GREY)),
    ] {
        let [bottom, right, top, left] = ring_layers(w, h, d0, d1);
        for q in split_at_opening(&bottom, wx0, wx1) {
            d.push(cut(vec![q], pattern, bg));
        }
        for q in [right, top, left] {
            d.push(cut(vec![q], pattern, bg));
        }
    }
    // Window: frame lines and glass.
    for y in [0.0, 0.25] {
        d.push(Item::line(pt(wx0, y), pt(wx1, y), Stroke::solid(weights::FINE), "window"));
    }
    d.push(Item::line(pt(wx0, 0.125), pt(wx1, 0.125), Stroke::solid(weights::MEDIUM), "window"));

    // Interior wall with a door opening at y 3.0–3.9.
    d.push(cut(vec![rect(6.0, 0.25, 6.10, 3.0)], HatchPattern::Masonry, Some(Rgba::LIGHT_GREY)));
    d.push(cut(vec![rect(6.0, 3.9, 6.10, h - 0.25)], HatchPattern::Masonry, Some(Rgba::LIGHT_GREY)));
    d.push(Item::line(pt(6.10, 3.0), pt(7.0, 3.0), Stroke::solid(weights::MEDIUM), "door"));
    d.push(Item {
        shape: Shape::Arc { centre: pt(6.10, 3.0), radius: 0.9, start: 0.0, sweep: FRAC_PI_2 },
        stroke: Some(Stroke::solid(weights::FINE)),
        fill: None,
        layer: "door".into(),
        source: None,
    });

    // Section line with labels.
    d.push(Item::line(
        pt(-1.5, 4.5),
        pt(14.0, 4.5),
        Stroke::patterned(weights::MEDIUM, LinePattern::DashDot),
        "section",
    ));
    for x in [-1.5, 14.0] {
        d.push(Item::text(Text::new(pt(x, 4.7), "Α", 5.0).anchored(Anchor::BottomCentre), "section"));
    }

    // Dimension chain below the building.
    let y = -1.2;
    let xs = [0.0, wx0, wx1, w];
    d.push(Item::line(pt(-0.3, y), pt(w + 0.3, y), Stroke::solid(weights::THIN), "dimension"));
    for x in xs {
        d.push(Item::line(pt(x, -0.3), pt(x, y - 0.2), Stroke::solid(weights::FINE), "dimension"));
        d.push(dim_tick(pt(x, y)));
    }
    for pair in xs.windows(2) {
        if let [a, b] = pair {
            let label = format!("{:.2}", b - a).replace('.', ",");
            d.push(Item::text(
                Text::new(pt((a + b) / 2.0, y + 0.08), label, 2.5).anchored(Anchor::BottomCentre),
                "dimension",
            ));
        }
    }

    // Room labels, level mark, title.
    let label = |p: Pt, name: &str, area: &str| {
        vec![
            Item::text(Text::new(p, name, 3.5).anchored(Anchor::BottomCentre), "space.label"),
            Item::text(Text::new(p.offset(0.0, -0.35), area, 2.5).anchored(Anchor::TopCentre), "space.label"),
        ]
    };
    for it in label(pt(3.0, 5.2), "Καθιστικό", "E = 41,25 m²") {
        d.push(it);
    }
    for it in label(pt(8.05, 5.2), "Υπνοδωμάτιο", "E = 28,08 m²") {
        d.push(it);
    }
    d.push(Item::polygon(
        vec![vec![pt(3.0, 2.0), pt(3.15, 2.25), pt(2.85, 2.25)]],
        Some(Fill::Solid(Rgba::BLACK)),
        None,
        "level",
    ));
    d.push(Item::text(Text::new(pt(3.25, 2.25), "±0,00", 2.5), "level"));
    let mut title = Text::new(pt(0.0, -2.6), "ΚΑΤΟΨΗ ΙΣΟΓΕΙΟΥ 1:50", 5.0);
    title.bold = true;
    d.push(Item::text(title, "title"));
    d.push(Item::text(Text::new(pt(0.0, -3.0), "Κάτοψη ισογείου — κλίμακα 1:50 (ΕΓΣΑ87)", 2.5), "title"));
    d
}

/// Every hatch pattern in a labelled 40 × 25 mm swatch on a paper drawing (millimetres).
pub fn hatch_swatches() -> Drawing {
    use HatchPattern as H;
    let patterns = [
        (H::Diagonal, "diagonal / διαγώνια"),
        (H::Masonry, "masonry / τοιχοποιία"),
        (H::CrossDiagonal, "cross-diagonal / οπλισμένο"),
        (H::Concrete, "concrete / σκυρόδεμα"),
        (H::Insulation, "insulation / μόνωση"),
        (H::Wood, "wood / ξύλο"),
        (H::Grid, "grid / πλακίδια"),
        (H::Dots, "dots / επίχρισμα"),
        (H::Earth, "earth / χώμα"),
        (H::Gravel, "gravel / χαλίκι"),
        (H::Horizontal, "horizontal / γυαλί"),
        (H::None, "none / χωρίς"),
    ];
    let mut d = Drawing::paper();
    for (i, (p, name)) in patterns.iter().enumerate() {
        let (col, row) = ((i % 4) as f64, (i / 4) as f64);
        let (x, y) = (10.0 + col * 50.0, 110.0 - row * 45.0);
        // Insulation is shown as a 12 mm band, the way it appears in a build-up.
        let r = if *p == H::Insulation {
            rect(x, y + 6.0, x + 40.0, y + 18.0)
        } else {
            rect(x, y, x + 40.0, y + 25.0)
        };
        d.push(Item::polygon(
            vec![r],
            Some(Fill::hatch(*p, Some(Rgba::rgb(245, 245, 245)))),
            Some(Stroke::solid(weights::THIN)),
            "swatch",
        ));
        d.push(Item::text(Text::new(pt(x, y - 4.0), *name, 2.5), "label"));
    }
    d
}

/// Every character of the built-in stroke font, 32 per line, at 5 mm cap height (paper drawing).
pub fn glyph_sheet() -> Drawing {
    let chars = covered_chars();
    let mut d = Drawing::paper();
    for (i, line) in chars.chunks(32).enumerate() {
        let s: String = line.iter().flat_map(|c| [*c, ' ']).collect();
        d.push(Item::text(Text::new(pt(10.0, 200.0 - i as f64 * 10.0), s, 5.0), "glyphs"));
    }
    d.push(Item::text(
        Text::new(pt(10.0, 20.0), "Κάτοψη ισογείου 1:50 — ΆΈΉΊΌΎΏ ϊϋΐΰ ±0,00 25°C m² m³", 5.0),
        "glyphs",
    ));
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_not_empty() {
        assert!(sample_plan().items.len() > 40);
        assert_eq!(hatch_swatches().items.len(), 24);
        assert!(glyph_sheet().items.len() > 5);
    }
}
