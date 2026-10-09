//! Native file dialogs and the few confirmations the shell needs: unsaved changes and crash recovery.
//!
//! File dialogs use the platform's own dialogs through `rfd` (native on Windows and macOS, the desktop portal
//! on Linux).

use std::path::PathBuf;

use egui::{Color32, RichText};
use kanavos_units::Locale;

use crate::exports::ExportKind;
use crate::imports::ImportKind;
use crate::names::{tr, trf};
use crate::persist::{EXTENSION, Recovery};
use crate::theme::Tokens;

/// Asks for a project to open.
pub fn pick_open(locale: Locale) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(tr(locale, "ui.dialog.open_title"))
        .add_filter(tr(locale, "ui.dialog.project_filter"), &[EXTENSION])
        .add_filter(tr(locale, "ui.dialog.all_files"), &["*"])
        .pick_file()
}

/// Asks where to save the project.
pub fn pick_save(locale: Locale, suggested: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(tr(locale, "ui.dialog.save_title"))
        .set_file_name(format!("{suggested}.{EXTENSION}"))
        .add_filter(tr(locale, "ui.dialog.project_filter"), &[EXTENSION])
        .save_file()
        .map(crate::persist::with_extension)
}

/// Asks where to write an export.
pub fn pick_export(locale: Locale, kind: ExportKind, suggested: &str) -> Option<PathBuf> {
    let ext = kind.extension();
    rfd::FileDialog::new()
        .set_title(tr(locale, kind.label_key()))
        .set_file_name(format!("{suggested}.{ext}"))
        .add_filter(ext.to_uppercase(), &[ext])
        .save_file()
}

/// Asks for a file to import.
pub fn pick_import(locale: Locale, kind: ImportKind) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(tr(locale, kind.label_key()))
        .add_filter(kind.extensions().join(", ").to_uppercase(), kind.extensions())
        .pick_file()
}

/// What to do once the unsaved-changes question is answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// Start a new project.
    New,
    /// Open a project (asking for the file when `None`).
    Open(Option<PathBuf>),
    /// Open the sample house.
    OpenSample,
    /// Quit.
    Exit,
}

/// The answer to the unsaved-changes question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Save first, then continue.
    Save,
    /// Continue without saving.
    Discard,
    /// Do nothing.
    Cancel,
}

/// Dialogs waiting for an answer.
#[derive(Default)]
pub struct Dialogs {
    /// Unsaved changes: what was asked for.
    pub unsaved: Option<Pending>,
    /// A recovery copy found at start-up.
    pub recovery: Option<Recovery>,
}

impl Dialogs {
    /// Whether a dialog is showing (global shortcuts pause).
    pub fn active(&self) -> bool {
        self.unsaved.is_some() || self.recovery.is_some()
    }

    /// Shows the unsaved-changes dialog; returns the answer with the pending action.
    pub fn unsaved_ui(
        &mut self,
        ctx: &egui::Context,
        locale: Locale,
        project: &str,
    ) -> Option<(Answer, Pending)> {
        let pending = self.unsaved.clone()?;
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::new("unsaved-changes")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            let t = Tokens::of(ui);
            ui.horizontal(|ui| {
                crate::icons::inline(ui, crate::icons::Icon::Warning, t.warning);
                ui.label(
                    RichText::new(tr(locale, "ui.dialog.unsaved_title"))
                        .family(crate::fonts::strong())
                        .size(15.0),
                );
            });
            ui.add_space(6.0);
            ui.label(trf(locale, "ui.dialog.unsaved_body", &[("name", project)]));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let save =
                    egui::Button::new(RichText::new(tr(locale, "ui.dialog.save")).color(Color32::WHITE))
                        .fill(t.accent);
                if ui.add(save).clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.button(tr(locale, "ui.dialog.discard")).clicked() {
                    answer = Some(Answer::Discard);
                }
                if ui.button(tr(locale, "common.cancel")).clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(Answer::Cancel);
        }
        let a = answer?;
        self.unsaved = None;
        Some((a, pending))
    }

    /// Shows the recovery dialog; returns `Some(true)` to recover, `Some(false)` to discard.
    pub fn recovery_ui(&mut self, ctx: &egui::Context, locale: Locale) -> Option<(bool, Recovery)> {
        let rec = self.recovery.as_ref()?;
        let mut answer = None;
        egui::Modal::new(egui::Id::new("recovery")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            let t = Tokens::of(ui);
            ui.horizontal(|ui| {
                crate::icons::inline(ui, crate::icons::Icon::History, t.accent);
                ui.label(
                    RichText::new(tr(locale, "ui.dialog.recovery_title"))
                        .family(crate::fonts::strong())
                        .size(15.0),
                );
            });
            ui.add_space(6.0);
            let name = rec
                .original
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| rec.document.info().name.clone());
            ui.label(trf(
                locale,
                "ui.dialog.recovery_body",
                &[("name", &name), ("age", &age(rec.written, locale))],
            ));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let recover =
                    egui::Button::new(RichText::new(tr(locale, "ui.dialog.recover")).color(Color32::WHITE))
                        .fill(t.accent);
                if ui.add(recover).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr(locale, "ui.dialog.discard_recovery")).clicked() {
                    answer = Some(false);
                }
            });
        });
        let a = answer?;
        let rec = self.recovery.take()?;
        Some((a, rec))
    }
}

/// How long ago a time (seconds since 1970) was, in words.
fn age(written: u64, locale: Locale) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(written);
    let mins = now.saturating_sub(written) / 60;
    if written == 0 {
        tr(locale, "ui.dialog.age_unknown")
    } else if mins < 1 {
        tr(locale, "ui.dialog.age_now")
    } else if mins < 120 {
        trf(locale, "ui.dialog.age_minutes", &[("n", &mins.to_string())])
    } else {
        trf(locale, "ui.dialog.age_hours", &[("n", &(mins / 60).to_string())])
    }
}
