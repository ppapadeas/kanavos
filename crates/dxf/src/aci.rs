//! Colour index and lineweight values used by DXF.
//!
//! DXF identifies colours by an index from 1 to 255 (group code 62). Indices 1–9 have fixed meanings (red, yellow,
//! green, cyan, blue, magenta, black/white, dark grey, light grey) and 250–255 form a grey ramp. Indices 10–249 are
//! 24 hues in 15° steps, each with five brightness levels in a full-saturation and a half-saturation variant. The
//! RGB values here are generated from that structure; they are close to, not necessarily identical with, what a
//! particular CAD program shows, which is enough for export (nearest index) and import (approximate colour).

use kanavos_drawing::Rgba;

/// RGB of a colour index (1–255). Index 7 is returned as black, the way it prints on white paper.
pub fn aci_to_rgb(index: i32) -> Rgba {
    match index {
        1 => Rgba::rgb(255, 0, 0),
        2 => Rgba::rgb(255, 255, 0),
        3 => Rgba::rgb(0, 255, 0),
        4 => Rgba::rgb(0, 255, 255),
        5 => Rgba::rgb(0, 0, 255),
        6 => Rgba::rgb(255, 0, 255),
        8 => Rgba::rgb(128, 128, 128),
        9 => Rgba::rgb(192, 192, 192),
        10..=249 => {
            let i = index - 10;
            let hue = f64::from(i / 10) * 15.0;
            let level = i % 10;
            let value = [1.0, 0.8, 0.6, 0.5, 0.3].get((level / 2) as usize).copied().unwrap_or(1.0);
            let saturation = if level % 2 == 0 { 1.0 } else { 0.5 };
            hsv(hue, saturation, value)
        }
        250..=255 => {
            let v = [51u8, 91, 132, 173, 214, 255].get((index - 250) as usize).copied().unwrap_or(0);
            Rgba::rgb(v, v, v)
        }
        _ => Rgba::BLACK,
    }
}

fn hsv(h: f64, s: f64, v: f64) -> Rgba {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgba::rgb(to(r), to(g), to(b))
}

/// The colour index closest to a colour. Black maps to 7, which is drawn in the contrasting colour of the
/// background, so black lines stay visible on dark screens and print black. White maps to 255, true white, so
/// white fills (masking backgrounds) stay white on paper.
pub fn rgb_to_aci(c: Rgba) -> i32 {
    if (c.r, c.g, c.b) == (0, 0, 0) {
        return 7;
    }
    if (c.r, c.g, c.b) == (255, 255, 255) {
        return 255;
    }
    let dist = |a: Rgba| {
        let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2);
        d(a.r, c.r) + d(a.g, c.g) + d(a.b, c.b)
    };
    (1..=255).filter(|i| *i != 7).min_by_key(|i| dist(aci_to_rgb(*i))).unwrap_or(7)
}

/// Lineweights allowed in DXF, hundredths of a millimetre.
pub const LINEWEIGHTS: [i32; 24] =
    [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

/// The allowed lineweight closest to a width in millimetres.
pub fn lineweight(mm: f64) -> i32 {
    let target = if mm.is_finite() { (mm * 100.0).round() as i32 } else { 25 };
    LINEWEIGHTS.iter().copied().min_by_key(|w| (w - target).abs()).unwrap_or(25)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_colours_round_trip() {
        for i in [1, 2, 3, 4, 5, 6, 8, 9, 250, 254] {
            assert_eq!(rgb_to_aci(aci_to_rgb(i)), i, "index {i}");
        }
        assert_eq!(rgb_to_aci(Rgba::BLACK), 7);
        // White fills must stay white on paper, so not the adaptive index 7.
        assert_eq!(rgb_to_aci(Rgba::WHITE), 255);
        assert_eq!(aci_to_rgb(255), Rgba::WHITE);
        assert_eq!(aci_to_rgb(7), Rgba::BLACK);
        // Index 10 is pure red at full brightness; 30 is pure yellow-ish orange at hue 30°.
        assert_eq!(aci_to_rgb(10), Rgba::rgb(255, 0, 0));
        assert_eq!(aci_to_rgb(11), Rgba::rgb(255, 128, 128));
    }

    #[test]
    fn every_index_maps_back_to_a_close_colour() {
        for i in 10..=249 {
            let c = aci_to_rgb(i);
            let j = rgb_to_aci(c);
            assert_eq!(aci_to_rgb(j), c, "index {i} → {j}");
        }
    }

    #[test]
    fn lineweights_snap_to_the_table() {
        assert_eq!(lineweight(0.13), 13);
        assert_eq!(lineweight(0.7), 70);
        assert_eq!(lineweight(0.33), 35);
        assert_eq!(lineweight(5.0), 211);
        assert_eq!(lineweight(f64::NAN), 25);
    }
}
