//! Desktop user interface.
//!
//! Layer L9. See `docs/architecture.md`, `docs/ux.md` and ADR-0004.
//!
//! The interface contains no model or geometry logic: it shows derived drawings, forwards input to tools and runs
//! the commands they produce.
//!
//! Modules:
//! - [`state`]: shared application state (session, selection, active storey, tool settings, language).
//! - [`app`]: the shell — menus, project tree, view tabs, inspector, status bar.
//! - [`canvas`]: the 2D drawing canvas (plans, sections, elevations, sheets) hosting the tools.
//! - [`viewport3d`]: the 3D viewport.
//! - [`fonts`]: system font loading.

pub mod app;
pub mod canvas;
pub mod fonts;
pub mod state;
pub mod viewport3d;

pub use kanavos_model::Document;

/// The document a new session starts with.
pub fn starter_document() -> Document {
    kanavos_model::starter()
}

/// Runs the desktop application on a document.
pub fn run(doc: kanavos_model::Document) -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_title("Kanavos").with_inner_size([1400.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kanavos",
        options,
        Box::new(|cc| {
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::KanavosApp::new(doc)))
        }),
    )
}
