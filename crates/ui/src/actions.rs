//! Interface actions: everything a menu item, shortcut, tree click or palette entry can ask the shell to do.
//!
//! Panels push actions; the shell performs them at the end of the frame with access to every part of the
//! application. Actions that change the document still go through engine commands.

use std::path::PathBuf;

use kanavos_model::{Id, ScheduleKind};
use kanavos_tools::ToolId;
use kanavos_units::Locale;
use kanavos_update::Channel;

use crate::exports::ExportKind;
use crate::imports::ImportKind;
use crate::theme::Theme;

/// A side panel or window that can be shown or hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// Project tree.
    Tree,
    /// Inspector.
    Inspector,
    /// History (in the inspector column).
    History,
}

/// An attribute manager or information window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Window {
    /// Materials.
    Materials,
    /// Build-ups.
    BuildUps,
    /// Storeys.
    Storeys,
    /// Project information.
    Project,
    /// Site and building terms.
    Site,
    /// Element defaults.
    Defaults,
    /// Keyboard shortcuts.
    Shortcuts,
    /// About Kanavos.
    About,
}

/// What a tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TabTarget {
    /// A model view (plan, section, elevation, 3D, site plan, coverage diagram, or a schedule view).
    View(Id),
    /// A sheet.
    Sheet(Id),
    /// A schedule computed from the model.
    Schedule(ScheduleKind),
    /// The ΝΟΚ area table.
    AreaTable,
}

/// An interface action.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// New project (asks to save changes first).
    New,
    /// Open a project chosen in a file dialog.
    Open,
    /// Open a given project file.
    OpenPath(PathBuf),
    /// Open the sample house.
    OpenSample,
    /// Save.
    Save,
    /// Save under a new name.
    SaveAs,
    /// Export.
    Export(ExportKind),
    /// Import.
    Import(ImportKind),
    /// Quit (asks to save changes first).
    Exit,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Undo until this many steps remain.
    UndoTo(usize),
    /// Redo this many steps.
    RedoSteps(usize),
    /// Delete the selection.
    Delete,
    /// Select every element of the active storey.
    SelectAll,
    /// Clear the selection.
    SelectNone,
    /// Select one element (and show its storey).
    SelectElement(Id),
    /// Copy the selection to a storey.
    CopyToStorey(Id),
    /// Activate a tool.
    Tool(ToolId),
    /// Switch theme.
    Theme(Theme),
    /// Switch language.
    Language(Locale),
    /// Set the UI scale.
    Scale(f32),
    /// Show or hide both side panels (canvas first).
    TogglePanels,
    /// Show or hide one panel.
    TogglePanel(Panel),
    /// Two views side by side, or one.
    ToggleSplit,
    /// Fit the drawing in the focused view.
    ZoomExtents,
    /// Open a tab (or focus it when open).
    OpenTab(TabTarget),
    /// Make a storey active and show its plan.
    ActivateStorey(Id),
    /// Start drawing a section line.
    NewSection,
    /// Start drawing an elevation line.
    NewElevation,
    /// Add an empty sheet.
    NewSheet,
    /// Generate the permit drawing set.
    PermitSet,
    /// Create spaces in every closed room of the active storey.
    DetectSpaces,
    /// Open an attribute manager or information window.
    OpenWindow(Window),
    /// Open the command palette.
    CommandPalette,
    /// Look for a new version now and show the result.
    CheckForUpdates,
    /// Show the update window (the offer or the download in progress).
    ShowUpdate,
    /// Turn the check for updates at start on or off.
    SetUpdateCheck(bool),
    /// Choose which releases are offered.
    SetUpdateChannel(Channel),
}
