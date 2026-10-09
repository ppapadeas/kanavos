//! Debug hook for visual checks: when `KANAVOS_CANVAS_SHOT=<file.png>` is set, the window is captured after
//! `KANAVOS_CANVAS_SHOT_FRAMES` frames (default 30), written as a PNG, and the application closes.
//!
//! `KANAVOS_CANVAS_SHOT_HOUSE=<n>` additionally builds `n` sample houses (see `demo`) on the active storey on
//! the first frame, so that a fresh starter document has something to show.
//!
//! The PNG writer is a minimal, dependency-free encoder (stored deflate blocks), adequate for screenshots.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::state::AppState;

/// Configuration read from the environment once.
#[derive(Debug, Clone)]
pub struct ShotConfig {
    /// Output file.
    pub path: PathBuf,
    /// Frames to wait before capturing.
    pub frames: u64,
    /// Sample houses to build first.
    pub houses: usize,
}

/// The configuration, if the hook is enabled.
pub fn config() -> Option<&'static ShotConfig> {
    static CONFIG: OnceLock<Option<ShotConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let path = std::env::var_os("KANAVOS_CANVAS_SHOT")?;
            let frames = std::env::var("KANAVOS_CANVAS_SHOT_FRAMES")
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(30);
            let houses = std::env::var("KANAVOS_CANVAS_SHOT_HOUSE")
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            Some(ShotConfig { path: PathBuf::from(path), frames, houses })
        })
        .as_ref()
}

#[derive(Debug, Clone, Copy, Default)]
struct ShotState {
    last_pass: Option<u64>,
    frames: u64,
    requested: bool,
    built: bool,
    done: bool,
}

/// Advances the hook by one frame (idempotent within a frame). Builds the sample houses when asked, requests the
/// screenshot when due, and writes it and closes the window when it arrives.
pub fn tick(ctx: &egui::Context, state: Option<&mut AppState>) {
    let Some(cfg) = config() else { return };
    let id = egui::Id::new("kanavos.canvas_shot");
    let mut st: ShotState = ctx.data_mut(|d| d.get_temp(id)).unwrap_or_default();
    if !st.built {
        st.built = true;
        if cfg.houses > 0
            && let Some(state) = state
        {
            let storey = state.active_storey;
            match super::demo::houses_commands(state.doc(), storey, cfg.houses) {
                Ok(cmds) => {
                    let _ = state.run_all(cmds, "sample".into());
                }
                Err(e) => eprintln!("kanavos: sample house failed: {e}"),
            }
        }
    }
    let pass = ctx.cumulative_pass_nr();
    if st.last_pass == Some(pass) || st.done {
        ctx.data_mut(|d| d.insert_temp(id, st));
        return;
    }
    st.last_pass = Some(pass);
    let shots: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
        i.raw
            .events
            .iter()
            .chain(i.events.iter())
            .filter_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
            .collect()
    });
    if let Some(image) = shots.first() {
        match write_png(&cfg.path, image) {
            Ok(()) => eprintln!("kanavos: screenshot written to {}", cfg.path.display()),
            Err(e) => eprintln!("kanavos: cannot write {}: {e}", cfg.path.display()),
        }
        st.done = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    } else {
        st.frames += 1;
        if !st.requested && st.frames >= cfg.frames {
            st.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
    ctx.request_repaint();
}

/// Writes an image as an 8-bit RGBA PNG.
pub fn write_png(path: &std::path::Path, image: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = image.size;
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for row in image.pixels.chunks(w.max(1)).take(h) {
        raw.push(0u8); // filter: none
        for c in row {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            raw.extend_from_slice(&[r, g, b, a]);
        }
    }
    std::fs::write(path, encode_png(w as u32, h as u32, &raw))
}

/// Encodes filtered scanlines (`raw`: one filter byte then RGBA per row) as a PNG file.
pub fn encode_png(width: u32, height: u32, raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65_535 * 5 + 128);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA, deflate, adaptive filter, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib_stored(raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(out.get(start..).unwrap_or(&[]));
    out.extend_from_slice(&crc.to_be_bytes());
}

/// A zlib stream of stored (uncompressed) deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65_535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut blocks = data.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
    }
    while let Some(block) = blocks.next() {
        let last = blocks.peek().is_none();
        out.push(u8::from(last));
        let len = block.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// CRC-32 (ISO 3309, as used by PNG).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 (RFC 1950).
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn png_has_signature_and_chunks() {
        let raw = vec![0u8, 255, 0, 0, 255];
        let png = encode_png(1, 1, &raw);
        assert_eq!(png.get(..8), Some(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'][..]));
        assert_eq!(png.get(12..16), Some(&b"IHDR"[..]));
        assert_eq!(png.get(png.len() - 8..png.len() - 4), Some(&b"IEND"[..]));
    }
}
