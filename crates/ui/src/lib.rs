//! Desktop user interface.
//!
//! Layer L9. See `docs/architecture.md`, `docs/ux.md` and ADR-0004.
//!
//! The interface contains no model or geometry logic: it shows derived drawings, forwards input to tools and runs
//! the commands they produce.
//!
//! Modules:
//! - [`state`]: shared application state (session, selection, active storey, tool settings, language, pointer).
//! - [`app`]: the shell, tying the panels together and performing [`actions`].
//! - [`menu`], [`ctxbar`], [`palette`], [`tree`], [`tabs`], [`inspector`], [`status`], [`search`], [`mgr`],
//!   [`table`]: the parts of the workspace (docs/ux.md §2).
//! - [`canvas`]: the 2D drawing canvas (plans, sections, elevations, sheets) hosting the tools.
//! - [`viewport3d`]: the 3D viewport.
//! - [`theme`], [`icons`], [`widgets`], [`fonts`], [`names`]: our own visual identity and shared widgets.
//! - Adapters to switch when other crates land: [`persist`] (native file format), [`exports`], [`imports`],
//!   [`sample`] (first-launch project), [`jobs`] (background computation).
//! - [`prefs`], [`dialogs`], [`fuzzy`], [`screenshot`]: preferences, file dialogs and confirmations, search
//!   matching, and the visual-verification hook.
//! - [`update`]: over-the-air updates (check, download, window), over `kanavos-update`.

pub mod actions;
pub mod app;
pub mod canvas;
pub mod ctxbar;
pub mod dialogs;
pub mod exports;
pub mod fonts;
pub mod fuzzy;
pub mod icons;
pub mod imports;
pub mod inspector;
pub mod jobs;
pub mod menu;
pub mod mgr;
pub mod names;
pub mod palette;
pub mod persist;
pub mod prefs;
pub mod sample;
pub mod screenshot;
pub mod search;
pub mod state;
pub mod status;
pub mod table;
pub mod tabs;
pub mod theme;
pub mod tree;
pub mod update;
pub mod viewport3d;
pub mod widgets;

pub use app::Startup;
pub use kanavos_model::Document;

/// The document a new session starts with.
pub fn starter_document() -> Document {
    kanavos_model::starter()
}

/// Runs the desktop application.
pub fn run(startup: Startup) -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Kanavos")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Kanavos",
        options,
        Box::new(move |cc| {
            if fonts::install(&cc.egui_ctx).is_none() {
                eprintln!("kanavos: no system font with Greek coverage was found; set KANAVOS_FONT");
            }
            Ok(Box::new(app::KanavosApp::new(startup)))
        }),
    )
}
