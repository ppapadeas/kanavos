//! Debug hook for visual verification: render a few frames, save a PNG of the window, exit.
//!
//! Environment:
//! - `KANAVOS_SCREENSHOT=<path>`: where to write the PNG (enables the hook).
//! - `KANAVOS_SCREENSHOT_FRAMES=<n>`: frames to render before capturing (default 40).
//! - `KANAVOS_SCENE=<items>`: comma-separated set-up for the capture, applied on the first frame:
//!   `lang=el|en`, `theme=light|dark`, `select=wall|door|window|space|slab|roof|stair|object|walls`,
//!   `open=materials|buildups|storeys|project|site|defaults|about|shortcuts`, `tab=doors|windows|spaces|walls|
//!   areas|3d|section|sheet`, `split=on|off`, `palette=<query>`, `history`, `tool=<tool key>`, `scale=<factor>`,
//!   `panels=off`.
//!
//! The hook never saves preferences or recovery files, and closes without the unsaved-changes prompt.

use std::path::{Path, PathBuf};

use egui::{ColorImage, Event, UserData, ViewportCommand};

/// One `key=value` (or bare `key`) item of `KANAVOS_SCENE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneItem {
    /// Key.
    pub key: String,
    /// Value (empty for bare keys).
    pub value: String,
}

/// Parses a scene description.
pub fn parse_scene(text: &str) -> Vec<SceneItem> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| match s.split_once('=') {
            Some((k, v)) => SceneItem { key: k.trim().to_lowercase(), value: v.trim().to_owned() },
            None => SceneItem { key: s.to_lowercase(), value: String::new() },
        })
        .collect()
}

/// The screenshot hook.
pub struct ScreenshotHook {
    path: PathBuf,
    frames: u32,
    frame: u32,
    requested: bool,
    /// Scene set-up, consumed by the application on the first frame.
    pub scene: Vec<SceneItem>,
}

impl ScreenshotHook {
    /// Reads the hook configuration from the environment; `None` when `KANAVOS_SCREENSHOT` is not set.
    pub fn from_env() -> Option<ScreenshotHook> {
        let path = std::env::var_os("KANAVOS_SCREENSHOT").filter(|p| !p.is_empty()).map(PathBuf::from)?;
        let frames = std::env::var("KANAVOS_SCREENSHOT_FRAMES")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(40u32)
            .max(2);
        let scene = std::env::var("KANAVOS_SCENE").map(|s| parse_scene(&s)).unwrap_or_default();
        Some(ScreenshotHook { path, frames, frame: 0, requested: false, scene })
    }

    /// Value of a scene key, if present.
    pub fn scene_value(&self, key: &str) -> Option<&str> {
        self.scene.iter().find(|i| i.key == key).map(|i| i.value.as_str())
    }

    /// Advances one frame: keeps frames coming, requests the capture after the configured number of frames,
    /// and saves the image and closes the window when it arrives.
    pub fn frame(&mut self, ctx: &egui::Context) {
        self.frame += 1;
        ctx.request_repaint();
        if !self.requested && self.frame >= self.frames {
            self.requested = true;
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            if let Err(e) = save_png(&self.path, &image) {
                eprintln!("kanavos: screenshot not saved: {e}");
            } else {
                eprintln!("kanavos: screenshot saved to {}", self.path.display());
            }
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

/// Writes an image as an 8-bit RGBA PNG.
pub fn save_png(path: &Path, image: &ColorImage) -> Result<(), String> {
    let [w, h] = image.size;
    let mut bytes = Vec::with_capacity(w * h * 4);
    for c in &image.pixels {
        bytes.extend_from_slice(&c.to_srgba_unmultiplied());
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let width = u32::try_from(w).map_err(|e| e.to_string())?;
    let height = u32::try_from(h).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&bytes).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_items_parse_with_and_without_values() {
        let s = parse_scene(" lang=en, theme = dark ,history,,palette=τοιχ πάχ ");
        assert_eq!(s.len(), 4);
        assert_eq!(s.first(), Some(&SceneItem { key: "lang".into(), value: "en".into() }));
        assert_eq!(s.get(2), Some(&SceneItem { key: "history".into(), value: String::new() }));
        assert_eq!(s.get(3).map(|i| i.value.as_str()), Some("τοιχ πάχ"));
    }

    #[test]
    fn png_files_are_written() {
        let image = ColorImage::new([3, 2], vec![egui::Color32::from_rgb(10, 20, 30); 6]);
        let path = std::env::temp_dir().join(format!("kanavos-shot-{}.png", std::process::id()));
        save_png(&path, &image).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        let _ = std::fs::remove_file(&path);
    }
}
