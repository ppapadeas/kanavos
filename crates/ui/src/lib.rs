//! Desktop user interface.
//!
//! Layer L9. See `docs/architecture.md`, `docs/ux.md` and ADR-0004.
//!
//! The interface contains no model or geometry logic: it shows derived drawings, forwards input to tools and runs
//! the commands they produce.
//!
//! Modules:
//! - [`state`]: shared application state (session, selection, active storey, tool settings, language, pointer).
//! - [`app`]: the shell — menus, project tree, view tabs, inspector, status bar.
//! - [`canvas`]: the 2D drawing canvas (plans, sections, elevations, sheets) hosting the tools.
//! - [`viewport3d`]: the 3D viewport.
//! - [`theme`], [`icons`], [`widgets`], [`fonts`], [`names`]: our own visual identity and shared widgets.
//! - Adapters to switch when other crates land: [`persist`] (native file format), [`exports`], [`imports`],
//!   [`sample`] (first-launch project), [`jobs`] (background computation).
//! - [`prefs`], [`fuzzy`], [`screenshot`]: preferences, search matching and the visual-verification hook.

pub mod app;
pub mod canvas;
pub mod exports;
pub mod fonts;
pub mod fuzzy;
pub mod icons;
pub mod imports;
pub mod jobs;
pub mod names;
pub mod persist;
pub mod prefs;
pub mod sample;
pub mod screenshot;
pub mod state;
pub mod theme;
pub mod viewport3d;
pub mod widgets;

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
