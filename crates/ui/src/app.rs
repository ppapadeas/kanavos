//! The application shell: menus, project tree, view tabs, inspector, status bar.
//!
//! First version: storey tabs, a tool strip, a plan canvas, a 3D tab and undo/redo. The full workspace described
//! in docs/ux.md §2 is to be built here.

use kanavos_i18n::t;
use kanavos_model::ViewKind;
use kanavos_tools::ToolId;

use crate::canvas::{Canvas, CanvasKind};
use crate::state::AppState;
use crate::viewport3d::Viewport3d;

/// The open tab.
enum Tab {
    Canvas(Box<Canvas>),
    ThreeD(Viewport3d),
}

/// The application.
pub struct KanavosApp {
    state: AppState,
    tabs: Vec<(String, Tab)>,
    active: usize,
}

impl KanavosApp {
    /// Starts on a document.
    pub fn new(doc: kanavos_model::Document) -> KanavosApp {
        let state = AppState::new(doc);
        let mut tabs = Vec::new();
        let doc = state.doc().clone();
        for s in doc.storeys() {
            if let Some(v) = doc.plan_view_of(s.id) {
                tabs.push((
                    v.name.clone(),
                    Tab::Canvas(Box::new(Canvas::new(CanvasKind::Plan { view: v.id }))),
                ));
            }
        }
        for v in doc.views() {
            if matches!(v.kind, ViewKind::ThreeD { .. }) {
                tabs.push((v.name.clone(), Tab::ThreeD(Viewport3d::new())));
            }
        }
        KanavosApp { state, tabs, active: 0 }
    }
}

impl eframe::App for KanavosApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let locale = self.state.locale;
        egui::Panel::top("tools").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("↶").clicked() {
                    self.state.undo();
                }
                if ui.button("↷").clicked() {
                    self.state.redo();
                }
                ui.separator();
                for id in [ToolId::Select, ToolId::Wall] {
                    let label = t(locale, &format!("tools.{}", id.key()));
                    if ui.selectable_label(self.state.tool == id, label).clicked() {
                        self.state.tool = id;
                    }
                }
                ui.separator();
                for (i, (name, _)) in self.tabs.iter().enumerate() {
                    if ui.selectable_label(self.active == i, name).clicked() {
                        self.active = i;
                    }
                }
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            let text = self.state.notice.as_ref().map(|n| n.text.clone()).unwrap_or_default();
            ui.label(text);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some((_, tab)) = self.tabs.get_mut(self.active) {
                match tab {
                    Tab::Canvas(c) => {
                        if let CanvasKind::Plan { view } = c.kind
                            && let Some(ViewKind::Plan { storey }) =
                                self.state.doc().view(view).map(|v| v.kind.clone())
                        {
                            self.state.active_storey = storey;
                        }
                        c.ui(ui, &mut self.state);
                    }
                    Tab::ThreeD(v) => v.ui(ui, frame, &mut self.state),
                }
            }
        });
    }
}
