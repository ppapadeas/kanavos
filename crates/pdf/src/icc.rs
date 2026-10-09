//! A minimal sRGB ICC profile (version 2.1, display class), generated for the PDF/A output intent.
//!
//! The colorants are computed from the sRGB primaries and D65 white point of IEC 61966-2-1 and adapted to the
//! ICC D50 connection-space illuminant with the Bradford transform (ICC.1:2001-04, annex E). The tone curve is the
//! sRGB transfer function sampled at 1024 points. Structure per ICC.1:2001-04: 128-byte header, tag table, and
//! `desc`, `cprt`, `wtpt`, `rXYZ`/`gXYZ`/`bXYZ` and `rTRC`/`gTRC`/`bTRC` tags (the three curves share one data
//! block, which the format allows).

type M3 = [[f64; 3]; 3];

fn dot([a, b, c]: [f64; 3], [x, y, z]: [f64; 3]) -> f64 {
    a * x + b * y + c * z
}

fn transpose(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    [[a, d, g], [b, e, h], [c, f, i]]
}

fn mul_v(m: &M3, v: [f64; 3]) -> [f64; 3] {
    m.map(|row| dot(row, v))
}

fn mul(a: &M3, b: &M3) -> M3 {
    let [c0, c1, c2] = transpose(b);
    a.map(|row| [dot(row, c0), dot(row, c1), dot(row, c2)])
}

fn inverse(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [(e * i - f * h) / det, (c * h - b * i) / det, (b * f - c * e) / det],
        [(f * g - d * i) / det, (a * i - c * g) / det, (c * d - a * f) / det],
        [(d * h - e * g) / det, (b * g - a * h) / det, (a * e - b * d) / det],
    ]
}

/// ICC profile connection space illuminant (D50).
pub const D50: [f64; 3] = [0.9642, 1.0, 0.8249];

fn xy_to_xyz(x: f64, y: f64) -> [f64; 3] {
    [x / y, 1.0, (1.0 - x - y) / y]
}

/// sRGB red, green and blue colorants adapted to D50 (columns of the RGB → XYZ matrix).
pub fn srgb_colorants_d50() -> [[f64; 3]; 3] {
    let primaries: M3 = [xy_to_xyz(0.64, 0.33), xy_to_xyz(0.30, 0.60), xy_to_xyz(0.15, 0.06)];
    let white = xy_to_xyz(0.3127, 0.3290);
    // Columns of P are the primaries; scale them so that R = G = B = 1 gives the white point.
    let p = transpose(&primaries);
    let [s0, s1, s2] = mul_v(&inverse(&p), white);
    let m: M3 = p.map(|[x, y, z]| [x * s0, y * s1, z * s2]);
    let bradford: M3 = [[0.8951, 0.2664, -0.1614], [-0.7502, 1.7135, 0.0367], [0.0389, -0.0685, 1.0296]];
    let [s_r, s_g, s_b] = mul_v(&bradford, white);
    let [d_r, d_g, d_b] = mul_v(&bradford, D50);
    let scale: M3 = [[d_r / s_r, 0.0, 0.0], [0.0, d_g / s_g, 0.0], [0.0, 0.0, d_b / s_b]];
    let adapt = mul(&inverse(&bradford), &mul(&scale, &bradford));
    transpose(&mul(&adapt, &m))
}

fn s15f16(v: f64) -> [u8; 4] {
    ((v * 65536.0).round() as i32).to_be_bytes()
}

fn xyz_tag(v: [f64; 3]) -> Vec<u8> {
    let mut t = b"XYZ \0\0\0\0".to_vec();
    for c in v {
        t.extend_from_slice(&s15f16(c));
    }
    t
}

fn pad4(v: &mut Vec<u8>) {
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
}

/// The sRGB profile bytes.
pub fn srgb_profile() -> Vec<u8> {
    let text = "sRGB IEC61966-2.1 (Kanavos)";
    let mut desc = b"desc\0\0\0\0".to_vec();
    desc.extend_from_slice(&((text.len() + 1) as u32).to_be_bytes());
    desc.extend_from_slice(text.as_bytes());
    desc.push(0);
    desc.extend_from_slice(&[0; 4]); // Unicode language code
    desc.extend_from_slice(&[0; 4]); // Unicode count
    desc.extend_from_slice(&[0; 2]); // ScriptCode code
    desc.push(0); // ScriptCode count
    desc.extend_from_slice(&[0; 67]); // ScriptCode string
    let mut cprt = b"text\0\0\0\0".to_vec();
    cprt.extend_from_slice(b"No copyright, use freely");
    cprt.push(0);
    let mut curv = b"curv\0\0\0\0".to_vec();
    let n = 1024u32;
    curv.extend_from_slice(&n.to_be_bytes());
    for i in 0..n {
        let v = f64::from(i) / f64::from(n - 1);
        let lin = if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        curv.extend_from_slice(&((lin * 65535.0).round().clamp(0.0, 65535.0) as u16).to_be_bytes());
    }
    let [r, g, b] = srgb_colorants_d50();
    let blocks: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"cprt", cprt),
        (b"wtpt", xyz_tag(D50)),
        (b"rXYZ", xyz_tag(r)),
        (b"gXYZ", xyz_tag(g)),
        (b"bXYZ", xyz_tag(b)),
        (b"rTRC", curv),
    ];
    let tag_count = blocks.len() + 2; // gTRC and bTRC share rTRC's data
    let table_len = 4 + 12 * tag_count;
    let mut data = Vec::new();
    let mut table = Vec::new();
    table.extend_from_slice(&(tag_count as u32).to_be_bytes());
    let mut trc = (0u32, 0u32);
    for (sig, mut bytes) in blocks {
        let offset = (128 + table_len + data.len()) as u32;
        let size = bytes.len() as u32;
        table.extend_from_slice(sig);
        table.extend_from_slice(&offset.to_be_bytes());
        table.extend_from_slice(&size.to_be_bytes());
        if sig == b"rTRC" {
            trc = (offset, size);
        }
        pad4(&mut bytes);
        data.extend_from_slice(&bytes);
    }
    for sig in [b"gTRC", b"bTRC"] {
        table.extend_from_slice(sig);
        table.extend_from_slice(&trc.0.to_be_bytes());
        table.extend_from_slice(&trc.1.to_be_bytes());
    }
    let total = (128 + table.len() + data.len()) as u32;
    let mut h = Vec::with_capacity(total as usize);
    h.extend_from_slice(&total.to_be_bytes());
    h.extend_from_slice(&[0; 4]); // preferred CMM
    h.extend_from_slice(&[0x02, 0x10, 0, 0]); // version 2.1.0
    h.extend_from_slice(b"mntr");
    h.extend_from_slice(b"RGB ");
    h.extend_from_slice(b"XYZ ");
    for v in [2026u16, 1, 1, 0, 0, 0] {
        h.extend_from_slice(&v.to_be_bytes());
    }
    h.extend_from_slice(b"acsp");
    h.extend_from_slice(&[0; 4]); // platform
    h.extend_from_slice(&[0; 4]); // flags
    h.extend_from_slice(&[0; 4]); // manufacturer
    h.extend_from_slice(&[0; 4]); // model
    h.extend_from_slice(&[0; 8]); // attributes
    h.extend_from_slice(&[0; 4]); // rendering intent: perceptual
    for c in D50 {
        h.extend_from_slice(&s15f16(c));
    }
    h.extend_from_slice(&[0; 4]); // creator
    h.resize(128, 0);
    h.extend_from_slice(&table);
    h.extend_from_slice(&data);
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorants_match_the_usual_srgb_d50_values() {
        let [r, g, b] = srgb_colorants_d50();
        let expect = [[0.4361, 0.2225, 0.0139], [0.3851, 0.7169, 0.0971], [0.1431, 0.0606, 0.7141]];
        for (got, want) in [r, g, b].iter().zip(expect) {
            for i in 0..3 {
                assert!((got[i] - want[i]).abs() < 2e-3, "{got:?} vs {want:?}");
            }
        }
        // The colorants sum to the D50 white.
        for i in 0..3 {
            assert!((r[i] + g[i] + b[i] - D50[i]).abs() < 1e-3);
        }
    }

    #[test]
    fn profile_structure_is_consistent() {
        let p = srgb_profile();
        assert_eq!(u32::from_be_bytes([p[0], p[1], p[2], p[3]]) as usize, p.len());
        assert_eq!(&p[36..40], b"acsp");
        assert_eq!(&p[12..20], b"mntrRGB ");
        let count = u32::from_be_bytes([p[128], p[129], p[130], p[131]]) as usize;
        assert_eq!(count, 9);
        for i in 0..count {
            let e = 132 + 12 * i;
            let off = u32::from_be_bytes([p[e + 4], p[e + 5], p[e + 6], p[e + 7]]) as usize;
            let size = u32::from_be_bytes([p[e + 8], p[e + 9], p[e + 10], p[e + 11]]) as usize;
            assert!(off.is_multiple_of(4) && off + size <= p.len());
        }
    }
}
