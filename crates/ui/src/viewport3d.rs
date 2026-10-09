//! The 3D viewport: shows the model rendered by `kanavos-render`, with orbit, pan, zoom and picking.
//!
//! First version: a placeholder that reports the scene size. The renderer integration lands here.

use crate::state::AppState;

/// A 3D viewport.
#[derive(Default)]
pub struct Viewport3d {
    cache: Option<(u64, usize)>,
}

impl Viewport3d {
    /// A new viewport.
    pub fn new() -> Viewport3d {
        Viewport3d::default()
    }

    /// Shows the viewport.
    pub fn ui(&mut self, ui: &mut egui::Ui, _frame: &eframe::Frame, state: &mut AppState) {
        let rev = state.session.revision();
        let count = match self.cache {
            Some((r, n)) if r == rev => n,
            _ => {
                let n =
                    kanavos_views::scene(state.doc(), &kanavos_views::SceneOptions::default()).items.len();
                self.cache = Some((rev, n));
                n
            }
        };
        ui.centered_and_justified(|ui| {
            ui.label(format!("3D: {count} meshes"));
        });
    }
}
