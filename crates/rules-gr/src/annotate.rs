//! Drawing helpers shared by the coverage diagram and the site plan: edge length labels, vertex letters,
//! dimension lines, the north arrow and paper-space insets placed in model space.
//!
//! Sizes passed as "mm" are paper millimetres and are converted to drawing units with the drawing's scale.

use std::f64::consts::{FRAC_PI_2, PI};

use kanavos_drawing::table::{Table, TableStyle};
use kanavos_drawing::{Affine, Anchor, Drawing, Fill, Item, Pt, Rgba, Shape, Stroke, Text, pt, weights};
use kanavos_geom::{Point2, Polygon2};
use kanavos_i18n::t;
use kanavos_units::Locale;

/// Greek capital letters used to name plot vertices, in order; after Ω the letters repeat with a number.
const LETTERS: [&str; 24] = [
    "Α", "Β", "Γ", "Δ", "Ε", "Ζ", "Η", "Θ", "Ι", "Κ", "Λ", "Μ", "Ν", "Ξ", "Ο", "Π", "Ρ", "Σ", "Τ", "Υ", "Φ",
    "Χ", "Ψ", "Ω",
];

/// Name of the `i`-th plot vertex: Α, Β, … Ω, Α1, Β1, …
pub fn vertex_name(i: usize) -> String {
    let n = LETTERS.len();
    let letter = LETTERS.get(i % n).copied().unwrap_or("Α");
    if i < n { letter.to_owned() } else { format!("{letter}{}", i / n) }
}

/// Converts a plan point to a drawing point.
pub fn p(q: Point2) -> Pt {
    let [x, y] = q.xy();
    pt(x, y)
}

/// Text rotation that keeps text along a direction readable (never upside down).
pub fn upright(angle: f64) -> f64 {
    let a = angle.rem_euclid(2.0 * PI);
    if a > FRAC_PI_2 + 1e-9 && a <= 3.0 * FRAC_PI_2 + 1e-9 { a - PI } else { a }
}

/// Labels an edge with text centred on it, offset `off_mm` to its right (outside a counter-clockwise ring).
pub fn edge_label(d: &mut Drawing, a: Pt, b: Pt, text: String, off_mm: f64, size_mm: f64, layer: &str) {
    let len = a.distance(b);
    if len < 1e-9 {
        return;
    }
    let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
    let off = d.paper_mm(off_mm);
    let m = a.midpoint(b);
    let at = pt(m.x + uy * off, m.y - ux * off);
    let t = Text::new(at, text, size_mm).anchored(Anchor::Centre).rotated(upright(uy.atan2(ux)));
    d.push(Item::text(t, layer));
}

/// Letters at the vertices of a ring, placed outside along the corner bisector.
pub fn vertex_letters(d: &mut Drawing, ring: &Polygon2, layer: &str) {
    let ring = ring.to_ccw();
    let n = ring.vertices.len();
    for (i, v) in ring.vertices.iter().enumerate() {
        let prev = ring.vertices.get((i + n - 1) % n).copied().unwrap_or(*v);
        let next = ring.vertices.get((i + 1) % n).copied().unwrap_or(*v);
        let a = (*v - prev).normalized();
        let b = (next - *v).normalized();
        // Outward direction: sum of the right-hand normals of the two edges.
        let dir = match (a, b) {
            (Some(a), Some(b)) => (-a.perp() - b.perp()).normalized(),
            _ => None,
        };
        let Some(dir) = dir else { continue };
        let off = d.paper_mm(4.0);
        let at = p(*v).offset(dir.x * off, dir.y * off);
        let mut t = Text::new(at, vertex_name(i), 3.0).anchored(Anchor::Centre);
        t.bold = true;
        d.push(Item::text(t, layer));
        d.push(Item {
            shape: Shape::Circle { centre: p(*v), radius: d.paper_mm(0.6) },
            stroke: None,
            fill: Some(Fill::Solid(Rgba::BLACK)),
            layer: layer.to_owned(),
            source: None,
        });
    }
}

/// A dimension line from `a` to `b` with oblique ticks and its text above the line.
pub fn dimension(d: &mut Drawing, a: Pt, b: Pt, text: String, layer: &str) {
    let len = a.distance(b);
    if len < 1e-9 {
        return;
    }
    let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
    let s = Stroke::solid(weights::FINE);
    d.push(Item::line(a, b, s.clone(), layer));
    let k = d.paper_mm(1.2);
    // Ticks at 45° to the line.
    let (tx, ty) = ((ux - uy) * k, (uy + ux) * k);
    for q in [a, b] {
        d.push(Item::line(
            pt(q.x - tx, q.y - ty),
            pt(q.x + tx, q.y + ty),
            Stroke::solid(weights::THIN),
            layer,
        ));
    }
    let off = d.paper_mm(1.2);
    let m = a.midpoint(b);
    let angle = upright(uy.atan2(ux));
    // Offset to the side that is "up" for the upright text.
    let (nx, ny) = (-angle.sin(), angle.cos());
    let t = Text::new(pt(m.x + nx * off, m.y + ny * off), text, 2.0)
        .anchored(Anchor::BottomCentre)
        .rotated(angle);
    d.push(Item::text(t, layer));
}

/// A north arrow centred at `at` with a circle of `radius` drawing units, pointing to grid north, which lies
/// `rotation` radians counter-clockwise from the drawing's +Y. The letter is 3 mm high on paper.
pub fn north_arrow(at: Pt, radius: f64, rotation: f64, locale: Locale) -> Drawing {
    let mut d = Drawing::paper();
    let (s, c) = rotation.sin_cos();
    let rot = |x: f64, y: f64| pt(at.x + (x * c - y * s) * radius, at.y + (x * s + y * c) * radius);
    let layer = "annotation.north";
    d.push(Item {
        shape: Shape::Circle { centre: at, radius },
        stroke: Some(Stroke::solid(weights::THIN)),
        fill: None,
        layer: layer.to_owned(),
        source: None,
    });
    let tip = rot(0.0, 1.25);
    let notch = rot(0.0, -0.45);
    let left = rot(-0.38, -0.85);
    let right = rot(0.38, -0.85);
    d.push(Item::polygon(vec![vec![tip, left, notch]], Some(Fill::Solid(Rgba::BLACK)), None, layer));
    d.push(Item::polygon(vec![vec![tip, notch, right]], None, Some(Stroke::solid(weights::THIN)), layer));
    let mut letter = Text::new(rot(0.0, 1.75), t(locale, "rules.north"), 3.0).anchored(Anchor::Centre);
    letter.bold = true;
    d.push(Item::text(letter, layer));
    d
}

/// Widens table columns so that headers and cells fit their text, measured with the printing fonts (1.5 mm padding
/// each side).
pub fn fit_columns(table: &mut Table, style: &TableStyle, locale: Locale) {
    let measure = |s: &str, bold: bool| kanavos_drawing::text::measure_mm(s, style.text_mm, bold) + 3.0;
    for (i, c) in table.columns.iter_mut().enumerate() {
        let mut w = measure(&c.header, true);
        for (row, bold) in table.rows.iter().map(|r| (r, false)).chain(table.footer.iter().map(|r| (r, true)))
        {
            if let Some(cell) = row.get(i) {
                w = w.max(measure(&cell.display(locale), bold));
            }
        }
        c.width_mm = c.width_mm.max(w);
    }
}

/// A paper-space table placed in a model-space drawing with its top-left corner at `top_left` (drawing units),
/// columns widened to their text. Returns the table's width and height in drawing units.
pub fn inset_table(d: &mut Drawing, table: &Table, top_left: Pt, locale: Locale) -> (f64, f64) {
    let style = TableStyle::default();
    let mut table = table.clone();
    fit_columns(&mut table, &style, locale);
    let paper = table.to_drawing(pt(0.0, 0.0), &style, locale);
    let k = d.paper_mm(1.0);
    let m = Affine::scale_translate(k, top_left.x, top_left.y);
    d.extend(paper.transformed(&m));
    (table.width_mm() * k, table.height_mm(&style) * k)
}

/// The outward (right-hand) unit normal of the edge `a → b` of a counter-clockwise ring.
pub fn outward(a: Point2, b: Point2) -> Option<kanavos_geom::Vec2> {
    (b - a).normalized().map(|u| -u.perp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_names_continue_after_omega() {
        assert_eq!(vertex_name(0), "Α");
        assert_eq!(vertex_name(23), "Ω");
        assert_eq!(vertex_name(24), "Α1");
    }

    #[test]
    fn text_stays_upright() {
        assert!((upright(PI) - 0.0).abs() < 1e-9);
        assert!((upright(-FRAC_PI_2) - FRAC_PI_2).abs() < 1e-9);
        assert!((upright(0.3) - 0.3).abs() < 1e-9);
    }
}
