//! Over-the-air updates in the desktop interface (`docs/updates.md`, `specs/updates/over-the-air.md`): the
//! check at start and on request, the notice in the status bar, the update window, the download with progress,
//! and the hand-over to installation and restart, which the shell performs after the unsaved-changes question.
//!
//! Network and file work runs on worker threads; [`Updates::poll`] collects their results every frame, so the
//! interface never waits. Nothing here contacts the network unless the shell asks: the shell skips the
//! automatic check in the screenshot hook, when the preference is off, when `KANAVOS_NO_UPDATE_CHECK` is set, and
//! in builds without a release key.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use egui::{Color32, RichText};
use kanavos_units::Locale;
use kanavos_update::{Available, Channel, Config, Downloaded, Installed, UpdateError};

use crate::names::{tr, trf};
use crate::theme::Tokens;
use crate::widgets;

/// What a worker reports.
enum Msg {
    Checked(Result<Option<Available>, UpdateError>),
    Progress(u64, u64),
    Downloaded(Result<Downloaded, UpdateError>),
    Installed(Result<Installed, UpdateError>),
}

/// Where the update stands.
pub enum Stage {
    /// Nothing happening.
    Idle,
    /// Asking for the release list; `manual` when the person asked.
    Checking {
        /// Asked for from the menu (results are always shown).
        manual: bool,
    },
    /// A check found nothing newer (shown after a manual check).
    UpToDate,
    /// A newer release is offered ([`Updates::available`]).
    Offer,
    /// Downloading the package.
    Downloading {
        /// Bytes received.
        done: u64,
        /// Bytes expected (0 when unknown).
        total: u64,
        /// Set to stop the download.
        cancel: Arc<AtomicBool>,
    },
    /// Downloaded and verified, waiting for the go-ahead (unsaved changes).
    Ready(Box<Downloaded>),
    /// Replacing the files.
    Installing,
    /// Something went wrong.
    Failed(UpdateError),
}

/// What the person chose in the update window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateEvent {
    /// Download (or install, when downloaded) and restart.
    Install,
    /// Close the window and keep the offer.
    Later,
    /// Do not offer this version again.
    Skip,
    /// Stop the download.
    Cancel,
    /// Try the failed step again.
    Retry,
    /// Close the window.
    Close,
}

/// A finished background step the shell must act on.
pub enum Done {
    /// The package is downloaded and verified: ask about unsaved changes, then install.
    Downloaded,
    /// The new version is installed: restart once nothing is being written.
    Installed(Installed),
}

/// Update state of the application.
pub struct Updates {
    /// Current step.
    pub stage: Stage,
    /// The release on offer, if any.
    pub available: Option<Available>,
    /// The update window is shown.
    pub open: bool,
    /// Whether network checks may run at all (off in the screenshot hook).
    enabled: bool,
    rx: Option<Receiver<Msg>>,
}

impl Updates {
    /// Update state; `enabled` is false where no network access may happen (visual verification).
    pub fn new(enabled: bool) -> Updates {
        Updates { stage: Stage::Idle, available: None, open: false, enabled, rx: None }
    }

    /// Whether a check, download or installation is running.
    pub fn busy(&self) -> bool {
        matches!(self.stage, Stage::Checking { .. } | Stage::Downloading { .. } | Stage::Installing)
    }

    /// Whether the package is downloaded and waiting to be installed.
    pub fn ready(&self) -> bool {
        matches!(self.stage, Stage::Ready(_))
    }

    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce(&Sender<Msg>) + Send + 'static) {
        let (tx, rx) = channel();
        let wake = ctx.clone();
        // When the thread cannot start, the sender is dropped and `poll` reports the step as stopped.
        let _ = std::thread::Builder::new().name("kanavos-update".into()).spawn(move || {
            job(&tx);
            wake.request_repaint();
        });
        self.rx = Some(rx);
    }

    /// At start: removes what an earlier update left behind (retrying while the previous program closes) and,
    /// when `check` is set, looks for a new version in the background.
    pub fn startup(&mut self, ctx: &egui::Context, check: bool, channel: Channel) {
        if !self.enabled {
            return;
        }
        let _ = std::thread::Builder::new().name("kanavos-update-cleanup".into()).spawn(|| {
            for _ in 0..12 {
                match kanavos_update::cleanup_old() {
                    Ok(c) if c.pending == 0 && !c.busy => return,
                    Ok(_) => std::thread::sleep(Duration::from_secs(5)),
                    Err(_) => return,
                }
            }
        });
        if check && kanavos_update::available_here().is_ok() {
            self.check(ctx, channel, false);
        }
    }

    /// Looks for a new version. A manual check opens the window and always reports its result; an automatic
    /// one stays silent unless a new version is found.
    pub fn check(&mut self, ctx: &egui::Context, channel: Channel, manual: bool) {
        if matches!(self.stage, Stage::Downloading { .. } | Stage::Installing | Stage::Ready(_)) {
            self.open |= manual;
            return;
        }
        if manual {
            self.open = true;
        }
        if !self.enabled {
            self.stage = Stage::Failed(UpdateError::Disabled);
            return;
        }
        if let Err(e) = kanavos_update::available_here() {
            self.stage = if manual { Stage::Failed(e) } else { Stage::Idle };
            return;
        }
        self.stage = Stage::Checking { manual };
        self.spawn(ctx, move |tx| {
            let _ = tx.send(Msg::Checked(kanavos_update::check(&Config::new(channel))));
        });
    }

    /// Starts downloading the release on offer, after checking that this installation can update itself.
    pub fn download(&mut self, ctx: &egui::Context) {
        let Some(available) = self.available.clone() else { return };
        if self.busy() {
            return;
        }
        self.open = true;
        if let Err(e) = kanavos_update::preflight() {
            self.stage = Stage::Failed(e);
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.stage = Stage::Downloading { done: 0, total: available.archive.size, cancel: cancel.clone() };
        let wake = ctx.clone();
        self.spawn(ctx, move |tx| {
            let mut sent = 0u64;
            let progress = |done: u64, total: u64| {
                if done == 0 || done >= sent.saturating_add(256 * 1024) || done == total {
                    sent = done;
                    let _ = tx.send(Msg::Progress(done, total));
                    wake.request_repaint();
                }
            };
            let result = kanavos_update::download(&available, progress, &cancel);
            let _ = tx.send(Msg::Downloaded(result));
        });
    }

    /// Stops the download; the partial file is deleted by the worker.
    pub fn cancel(&mut self) {
        if let Stage::Downloading { cancel, .. } = &self.stage {
            cancel.store(true, Ordering::Relaxed);
            // The worker's last message goes nowhere; it deletes its temporary folder itself.
            self.rx = None;
            self.stage = if self.available.is_some() { Stage::Offer } else { Stage::Idle };
        }
    }

    /// Installs the downloaded package in the background. The shell has asked about unsaved changes and written
    /// a recovery copy first.
    pub fn install(&mut self, ctx: &egui::Context) {
        let Stage::Ready(downloaded) = std::mem::replace(&mut self.stage, Stage::Installing) else {
            return;
        };
        self.open = true;
        self.spawn(ctx, move |tx| {
            let result = kanavos_update::install(&downloaded);
            drop(downloaded);
            let _ = tx.send(Msg::Installed(result));
        });
    }

    /// Reports a failure in the window.
    pub fn fail(&mut self, error: UpdateError) {
        self.stage = Stage::Failed(error);
        self.open = true;
    }

    /// Collects worker results. `skipped` is the version the person chose to skip: an automatic check does not
    /// offer it again.
    pub fn poll(&mut self, skipped: Option<&str>) -> Option<Done> {
        let mut done = None;
        while let Some(rx) = &self.rx {
            let msg = match rx.try_recv() {
                Ok(m) => m,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    if self.busy() {
                        self.fail(UpdateError::Stopped);
                    }
                    break;
                }
            };
            match msg {
                Msg::Checked(result) => {
                    self.rx = None;
                    let manual = matches!(self.stage, Stage::Checking { manual: true });
                    self.stage = match result {
                        Ok(Some(a)) if !manual && skipped == Some(a.version.to_string().as_str()) => {
                            self.available = None;
                            Stage::Idle
                        }
                        Ok(Some(a)) => {
                            self.available = Some(a);
                            Stage::Offer
                        }
                        Ok(None) => {
                            self.available = None;
                            if manual { Stage::UpToDate } else { Stage::Idle }
                        }
                        Err(e) if manual => Stage::Failed(e),
                        Err(e) => {
                            eprintln!("kanavos: update check: {e}");
                            Stage::Idle
                        }
                    };
                }
                Msg::Progress(d, t) => {
                    if let Stage::Downloading { done, total, .. } = &mut self.stage {
                        *done = d;
                        *total = t;
                    }
                }
                Msg::Downloaded(result) => {
                    self.rx = None;
                    match result {
                        Ok(d) => {
                            self.stage = Stage::Ready(Box::new(d));
                            done = Some(Done::Downloaded);
                        }
                        Err(UpdateError::Cancelled) => self.stage = Stage::Offer,
                        Err(e) => self.fail(e),
                    }
                }
                Msg::Installed(result) => {
                    self.rx = None;
                    match result {
                        // The stage stays "installing" until the shell restarts.
                        Ok(installed) => done = Some(Done::Installed(installed)),
                        Err(e) => self.fail(e),
                    }
                }
            }
        }
        done
    }

    /// Text for the status bar while an update is on offer or under way.
    pub fn status_text(&self, locale: Locale) -> Option<String> {
        let version = self.available.as_ref().map(|a| a.version.to_string()).unwrap_or_default();
        match &self.stage {
            Stage::Downloading { done, total, .. } => {
                let percent = done.saturating_mul(100).checked_div(*total).unwrap_or(0);
                Some(trf(locale, "ui.update.status_downloading", &[("percent", &percent.to_string())]))
            }
            Stage::Installing => Some(tr(locale, "ui.update.status_installing")),
            Stage::Offer | Stage::Ready(_) if self.available.is_some() => {
                Some(trf(locale, "ui.update.status_available", &[("version", &version)]))
            }
            _ => None,
        }
    }

    /// The update window; returns what the person chose.
    pub fn window(&mut self, ctx: &egui::Context, locale: Locale) -> Option<UpdateEvent> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut event = None;
        let centre = ctx.content_rect().center();
        egui::Window::new(RichText::new(tr(locale, "ui.update.title")).family(crate::fonts::strong()))
            .id(egui::Id::new("kanavos-update-window"))
            .default_pos(centre)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_size([500.0, 420.0])
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| event = self.body(ui, locale));
        if !open {
            self.open = false;
            if event.is_none() {
                event = Some(UpdateEvent::Close);
            }
        }
        event
    }

    fn body(&self, ui: &mut egui::Ui, locale: Locale) -> Option<UpdateEvent> {
        let t = Tokens::of(ui);
        let mut event = None;
        let version = self.available.as_ref().map(|a| a.version.to_string()).unwrap_or_default();
        match &self.stage {
            Stage::Idle => {
                if self.available.is_some() {
                    event = self.offer(ui, locale);
                } else {
                    ui.label(trf(
                        locale,
                        "ui.update.latest",
                        &[("version", kanavos_update::CURRENT_VERSION)],
                    ));
                    event = close_button(ui, locale);
                }
            }
            Stage::Checking { .. } => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(tr(locale, "ui.update.checking"));
                });
            }
            Stage::UpToDate => {
                ui.horizontal(|ui| {
                    crate::icons::inline(ui, crate::icons::Icon::Info, t.ok);
                    ui.label(trf(
                        locale,
                        "ui.update.latest",
                        &[("version", kanavos_update::CURRENT_VERSION)],
                    ));
                });
                ui.add_space(4.0);
                widgets::hint(ui, &tr(locale, "ui.update.privacy"));
                event = close_button(ui, locale);
            }
            Stage::Offer => event = self.offer(ui, locale),
            Stage::Downloading { done, total, .. } => {
                ui.label(trf(locale, "ui.update.downloading", &[("version", &version)]));
                ui.add_space(6.0);
                let mb = |b: u64| format!("{:.1}", b as f64 / 1_048_576.0);
                let (fraction, text) = if *total > 0 {
                    let text = if done >= total {
                        tr(locale, "ui.update.verifying")
                    } else {
                        trf(locale, "ui.update.progress", &[("done", &mb(*done)), ("total", &mb(*total))])
                    };
                    (*done as f32 / *total as f32, text)
                } else {
                    (0.0, trf(locale, "ui.update.progress_unknown", &[("done", &mb(*done))]))
                };
                ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).text(text));
                ui.add_space(8.0);
                if ui.button(tr(locale, "common.cancel")).clicked() {
                    event = Some(UpdateEvent::Cancel);
                }
            }
            Stage::Ready(_) => {
                ui.label(trf(locale, "ui.update.ready", &[("version", &version)]));
                ui.add_space(4.0);
                widgets::hint(ui, &tr(locale, "ui.update.restart_note"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if accent_button(ui, &tr(locale, "ui.update.install")).clicked() {
                        event = Some(UpdateEvent::Install);
                    }
                    if ui.button(tr(locale, "ui.update.later")).clicked() {
                        event = Some(UpdateEvent::Later);
                    }
                });
            }
            Stage::Installing => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(tr(locale, "ui.update.installing"));
                });
            }
            Stage::Failed(e) => {
                ui.horizontal(|ui| {
                    crate::icons::inline(ui, crate::icons::Icon::Error, t.error);
                    ui.label(RichText::new(tr(locale, "ui.update.failed")).family(crate::fonts::strong()));
                });
                ui.add_space(4.0);
                ui.add(egui::Label::new(e.message(locale)).wrap());
                let manual = matches!(
                    e,
                    UpdateError::NotWritable(_)
                        | UpdateError::NotPackaged(_)
                        | UpdateError::InstallFailed { .. }
                );
                if let Some(a) = self.available.as_ref().filter(|a| manual && !a.page.is_empty()) {
                    ui.add_space(6.0);
                    widgets::hint(ui, &tr(locale, "ui.update.manual_download"));
                    ui.add(egui::Label::new(RichText::new(&a.page).monospace().small()).selectable(true));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if !e.is_unavailable() && ui.button(tr(locale, "ui.update.retry")).clicked() {
                        event = Some(UpdateEvent::Retry);
                    }
                    if ui.button(tr(locale, "common.close")).clicked() {
                        event = Some(UpdateEvent::Close);
                    }
                });
            }
        }
        event
    }

    /// The offer: version, date, notes and the three choices.
    fn offer(&self, ui: &mut egui::Ui, locale: Locale) -> Option<UpdateEvent> {
        let a = self.available.as_ref()?;
        let t = Tokens::of(ui);
        let mut event = None;
        ui.horizontal_wrapped(|ui| {
            let heading = trf(locale, "ui.update.available", &[("version", &a.version.to_string())]);
            ui.label(RichText::new(heading).family(crate::fonts::strong()).size(16.0));
            if a.prerelease {
                widgets::pill(ui, &tr(locale, "ui.update.test_build"), t.warning, t.warning_weak);
            }
        });
        ui.add_space(4.0);
        widgets::grid(ui, "update-facts", |ui| {
            widgets::label(ui, &tr(locale, "ui.update.installed_version"));
            widgets::value(ui, kanavos_update::CURRENT_VERSION);
            ui.end_row();
            if let Some(date) = a.date() {
                widgets::label(ui, &tr(locale, "ui.update.published"));
                widgets::value(ui, date);
                ui.end_row();
            }
            if a.title != a.tag && !a.title.is_empty() {
                widgets::label(ui, &tr(locale, "ui.update.release"));
                widgets::value(ui, &a.title);
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        ui.label(RichText::new(tr(locale, "ui.update.notes")).family(crate::fonts::strong()));
        egui::Frame::new().fill(t.sunken).corner_radius(4.0).inner_margin(8.0).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("update-notes").max_height(200.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if a.notes.trim().is_empty() {
                    widgets::hint(ui, &tr(locale, "ui.update.no_notes"));
                } else {
                    // Plain text: release notes are shown exactly as written, never interpreted.
                    ui.add(egui::Label::new(a.notes.trim()).selectable(true).wrap());
                }
            });
        });
        ui.add_space(6.0);
        widgets::hint(ui, &tr(locale, "ui.update.how"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if accent_button(ui, &tr(locale, "ui.update.install")).clicked() {
                event = Some(UpdateEvent::Install);
            }
            if ui.button(tr(locale, "ui.update.later")).clicked() {
                event = Some(UpdateEvent::Later);
            }
            if ui.button(tr(locale, "ui.update.skip")).clicked() {
                event = Some(UpdateEvent::Skip);
            }
        });
        event
    }
}

fn accent_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = Tokens::of(ui);
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(t.accent))
}

fn close_button(ui: &mut egui::Ui, locale: Locale) -> Option<UpdateEvent> {
    ui.add_space(8.0);
    ui.button(tr(locale, "common.close")).clicked().then_some(UpdateEvent::Close)
}

/// A sample offer for the screenshot hook (`KANAVOS_SCENE=update`); never used for a real update.
pub fn sample_offer() -> Available {
    let asset = |name: &str| kanavos_update::Asset { name: name.into(), url: String::new(), size: 0 };
    Available {
        version: kanavos_update::Version::new(9, 9, 9),
        tag: "v9.9.9".into(),
        title: "v9.9.9".into(),
        published_at: Some("2026-10-10T09:00:00Z".into()),
        notes: "- Sample release notes for visual checks.\n- Δείγμα σημειώσεων έκδοσης.".into(),
        prerelease: false,
        page: String::new(),
        archive: asset("sample.zip"),
        checksum: asset("sample.zip.sha256"),
        signature: asset("sample.zip.sig"),
        package: "sample".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> Updates {
        let mut u = Updates::new(false);
        u.available = Some(sample_offer());
        u.stage = Stage::Offer;
        u
    }

    #[test]
    fn the_status_bar_names_the_offer_and_the_progress() {
        let mut u = offer();
        assert_eq!(u.status_text(Locale::En).as_deref(), Some("New version 9.9.9"));
        u.stage = Stage::Downloading { done: 50, total: 200, cancel: Arc::new(AtomicBool::new(false)) };
        assert_eq!(u.status_text(Locale::En).as_deref(), Some("Downloading update 25 %"));
        u.stage = Stage::Idle;
        assert_eq!(u.status_text(Locale::En), None);
    }

    #[test]
    fn checks_are_refused_without_network_permission() {
        let ctx = egui::Context::default();
        let mut u = Updates::new(false);
        u.check(&ctx, Channel::Stable, true);
        assert!(u.open);
        assert!(matches!(u.stage, Stage::Failed(UpdateError::Disabled)));
        // An automatic check never opens the window.
        let mut u = Updates::new(false);
        u.startup(&ctx, true, Channel::Stable);
        assert!(!u.open);
        assert!(matches!(u.stage, Stage::Idle));
    }

    #[test]
    fn cancelling_returns_to_the_offer() {
        let mut u = offer();
        let flag = Arc::new(AtomicBool::new(false));
        u.stage = Stage::Downloading { done: 1, total: 2, cancel: flag.clone() };
        u.cancel();
        assert!(flag.load(Ordering::Relaxed));
        assert!(matches!(u.stage, Stage::Offer));
    }

    #[test]
    fn a_skipped_version_is_not_offered_by_the_automatic_check() {
        let (tx, rx) = channel();
        let mut u = Updates::new(false);
        u.stage = Stage::Checking { manual: false };
        u.rx = Some(rx);
        tx.send(Msg::Checked(Ok(Some(sample_offer())))).unwrap();
        assert!(u.poll(Some("9.9.9")).is_none());
        assert!(u.available.is_none());
        // The same result from a manual check is shown.
        let (tx, rx) = channel();
        u.stage = Stage::Checking { manual: true };
        u.rx = Some(rx);
        tx.send(Msg::Checked(Ok(Some(sample_offer())))).unwrap();
        u.poll(Some("9.9.9"));
        assert!(matches!(u.stage, Stage::Offer));
        assert!(u.available.is_some());
    }

    #[test]
    fn a_worker_that_stops_is_reported() {
        let (tx, rx) = channel::<Msg>();
        let mut u = offer();
        u.stage = Stage::Installing;
        u.rx = Some(rx);
        drop(tx);
        assert!(u.poll(None).is_none());
        assert!(matches!(u.stage, Stage::Failed(UpdateError::Stopped)));
        assert!(u.open);
    }
}
