//! View tabs and the split view (docs/ux.md §2).
//!
//! Tabs are kept in one ordered list. The centre shows one pane, or two side by side when split; each pane
//! shows one tab, and the focused pane (the one last clicked) receives tabs opened from the tree, the palette
//! and the menus. Plans, sections, elevations, site plans, coverage diagrams and sheets are shown by a
//! [`Canvas`]; 3D views by a [`Viewport3d`]; schedules and the area table by a [`TableView`].

use egui::{Align, Color32, Layout, Rect, RichText, Sense, Stroke, Ui, UiBuilder, Vec2};
use kanavos_model::{Document, ViewKind};

use crate::actions::{Action, TabTarget};
use crate::canvas::{Canvas, CanvasKind};
use crate::icons::{self, Icon};
use crate::names::{schedule, tr};
use crate::state::AppState;
use crate::table::{TableSource, TableView};
use crate::theme::Tokens;
use crate::viewport3d::Viewport3d;

/// One of the two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// Left (or only) pane.
    Left,
    /// Right pane of the split view.
    Right,
}

/// What a tab hosts.
pub enum TabBody {
    /// A 2D canvas.
    Canvas(Box<Canvas>),
    /// A 3D viewport.
    ThreeD(Box<Viewport3d>),
    /// A table.
    Table(TableView),
}

/// An open tab.
pub struct Tab {
    /// What it shows.
    pub target: TabTarget,
    /// The widget showing it.
    pub body: TabBody,
}

impl Tab {
    /// A tab for a target, or `None` when the target no longer exists.
    pub fn new(doc: &Document, target: TabTarget) -> Option<Tab> {
        let body = match target {
            TabTarget::View(id) => match &doc.view(id)?.kind {
                ViewKind::Plan { .. } => {
                    TabBody::Canvas(Box::new(Canvas::new(CanvasKind::Plan { view: id })))
                }
                ViewKind::Section { .. }
                | ViewKind::Elevation { .. }
                | ViewKind::SitePlan
                | ViewKind::CoverageDiagram => {
                    TabBody::Canvas(Box::new(Canvas::new(CanvasKind::Section { view: id })))
                }
                ViewKind::ThreeD { .. } => TabBody::ThreeD(Box::new(Viewport3d::new())),
                ViewKind::AreaTable => TabBody::Table(TableView::new(TableSource::AreaTable)),
                ViewKind::Schedule { schedule } => {
                    TabBody::Table(TableView::new(TableSource::Schedule(*schedule)))
                }
            },
            TabTarget::Sheet(id) => {
                doc.sheet(id)?;
                TabBody::Canvas(Box::new(Canvas::new(CanvasKind::Sheet { sheet: id })))
            }
            TabTarget::Schedule(kind) => TabBody::Table(TableView::new(TableSource::Schedule(kind))),
            TabTarget::AreaTable => TabBody::Table(TableView::new(TableSource::AreaTable)),
        };
        Some(Tab { target, body })
    }

    /// Whether the target still exists in a document.
    pub fn exists(&self, doc: &Document) -> bool {
        match self.target {
            TabTarget::View(id) => doc.view(id).is_some(),
            TabTarget::Sheet(id) => doc.sheet(id).is_some(),
            TabTarget::Schedule(_) | TabTarget::AreaTable => true,
        }
    }
}

/// Title of a tab target.
pub fn title(state: &AppState, target: TabTarget) -> String {
    let doc = state.doc();
    match target {
        TabTarget::View(id) => doc.view(id).map(|v| v.name.clone()).unwrap_or_else(|| "—".into()),
        TabTarget::Sheet(id) => {
            doc.sheet(id).map(|s| format!("{} {}", s.number, s.name)).unwrap_or_else(|| "—".into())
        }
        TabTarget::Schedule(kind) => schedule(state.locale, kind),
        TabTarget::AreaTable => tr(state.locale, "ui.tree.area_table"),
    }
}

/// Icon of a tab target.
pub fn icon(doc: &Document, target: TabTarget) -> Icon {
    match target {
        TabTarget::View(id) => match doc.view(id).map(|v| &v.kind) {
            Some(ViewKind::Plan { .. }) => Icon::PlanView,
            Some(ViewKind::Section { .. }) => Icon::SectionView,
            Some(ViewKind::Elevation { .. }) => Icon::ElevationView,
            Some(ViewKind::ThreeD { .. }) => Icon::Cube,
            Some(ViewKind::SitePlan) => Icon::Site,
            Some(ViewKind::CoverageDiagram) => Icon::Coverage,
            Some(ViewKind::AreaTable | ViewKind::Schedule { .. }) => Icon::Table,
            None => Icon::Views,
        },
        TabTarget::Sheet(_) => Icon::Sheet,
        TabTarget::Schedule(_) | TabTarget::AreaTable => Icon::Table,
    }
}

/// The open tabs and the panes showing them.
pub struct Workspace {
    /// Open tabs, in bar order.
    pub tabs: Vec<Tab>,
    /// Tab shown in the left (or only) pane.
    pub left: Option<usize>,
    /// Tab shown in the right pane when split.
    pub right: Option<usize>,
    /// Two panes side by side.
    pub split: bool,
    /// Pane that receives newly opened tabs.
    pub focus: Pane,
}

impl Workspace {
    /// An empty workspace.
    pub fn new(split: bool) -> Workspace {
        Workspace { tabs: Vec::new(), left: None, right: None, split, focus: Pane::Left }
    }

    /// The default tabs: every storey plan, the 3D view, plan left and 3D right (docs/ux.md §2).
    pub fn with_defaults(doc: &Document, active_storey: kanavos_model::Id, split: bool) -> Workspace {
        let mut w = Workspace::new(split);
        for s in doc.storeys() {
            if let Some(v) = doc.plan_view_of(s.id) {
                w.push(doc, TabTarget::View(v.id));
            }
        }
        let three_d = doc.views().find(|v| matches!(v.kind, ViewKind::ThreeD { .. })).map(|v| v.id);
        if let Some(v) = three_d {
            w.push(doc, TabTarget::View(v));
        }
        let plan = doc.plan_view_of(active_storey).map(|v| TabTarget::View(v.id));
        w.left = plan.and_then(|p| w.index_of(p)).or(if w.tabs.is_empty() { None } else { Some(0) });
        w.right = three_d.and_then(|v| w.index_of(TabTarget::View(v)));
        w
    }

    fn push(&mut self, doc: &Document, target: TabTarget) -> Option<usize> {
        let tab = Tab::new(doc, target)?;
        self.tabs.push(tab);
        Some(self.tabs.len() - 1)
    }

    /// Index of an open tab.
    pub fn index_of(&self, target: TabTarget) -> Option<usize> {
        self.tabs.iter().position(|t| t.target == target)
    }

    /// The tab index shown in a pane.
    pub fn shown(&self, pane: Pane) -> Option<usize> {
        match pane {
            Pane::Left => self.left,
            Pane::Right if self.split => self.right,
            Pane::Right => None,
        }
    }

    fn set_shown(&mut self, pane: Pane, i: Option<usize>) {
        match pane {
            Pane::Left => self.left = i,
            Pane::Right => self.right = i,
        }
    }

    /// The focused pane, falling back to the left one when the right pane is hidden.
    pub fn focused_pane(&self) -> Pane {
        if self.split { self.focus } else { Pane::Left }
    }

    /// The focused tab.
    pub fn focused(&self) -> Option<&Tab> {
        self.shown(self.focused_pane()).and_then(|i| self.tabs.get(i))
    }

    /// The focused tab, mutably.
    pub fn focused_mut(&mut self) -> Option<&mut Tab> {
        let i = self.shown(self.focused_pane())?;
        self.tabs.get_mut(i)
    }

    /// Opens a tab (or shows the open one) in the focused pane. Returns `false` when the target is gone.
    pub fn open(&mut self, doc: &Document, target: TabTarget) -> bool {
        let pane = self.focused_pane();
        let i = match self.index_of(target) {
            Some(i) => i,
            None => match self.push(doc, target) {
                Some(i) => i,
                None => return false,
            },
        };
        self.show_in(pane, i);
        true
    }

    /// Shows tab `i` in a pane; when the other pane shows it, the panes swap.
    pub fn show_in(&mut self, pane: Pane, i: usize) {
        let other = match pane {
            Pane::Left => Pane::Right,
            Pane::Right => Pane::Left,
        };
        if self.split && self.shown(other) == Some(i) {
            let mine = self.shown(pane);
            self.set_shown(other, mine);
        }
        self.set_shown(pane, Some(i));
        self.focus = pane;
    }

    /// Closes tab `i`, keeping both panes on valid tabs.
    pub fn close(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        let fix = |slot: Option<usize>, len: usize| -> Option<usize> {
            match slot {
                Some(s) if s == i => {
                    if len == 0 {
                        None
                    } else {
                        Some(i.min(len - 1))
                    }
                }
                Some(s) if s > i => Some(s - 1),
                other => other,
            }
        };
        let len = self.tabs.len();
        self.left = fix(self.left, len);
        self.right = fix(self.right, len);
        if self.split && self.left.is_some() && self.left == self.right {
            // Both panes would show the same tab: give the right one another tab when there is one.
            self.right = (0..len).find(|&k| Some(k) != self.left);
        }
    }

    /// Moves tab `from` to position `to` (bar order), keeping panes on the same tabs.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        let map = |slot: Option<usize>| -> Option<usize> {
            slot.map(|s| {
                if s == from {
                    to
                } else if from < s && s <= to {
                    s - 1
                } else if to <= s && s < from {
                    s + 1
                } else {
                    s
                }
            })
        };
        self.left = map(self.left);
        self.right = map(self.right);
    }

    /// Closes tabs whose view or sheet was removed (after undo or delete).
    pub fn prune(&mut self, doc: &Document) {
        let mut i = 0;
        while i < self.tabs.len() {
            if self.tabs.get(i).is_some_and(|t| !t.exists(doc)) {
                self.close(i);
            } else {
                i += 1;
            }
        }
        if self.left.is_none() && !self.tabs.is_empty() {
            self.left = Some(0);
        }
    }

    /// Turns the split view on or off. Turning it on shows another tab on the right (3D when open).
    pub fn set_split(&mut self, doc: &Document, split: bool) {
        self.split = split;
        if !split {
            self.focus = Pane::Left;
            return;
        }
        if self.right.is_none() || self.right == self.left {
            let three_d = self.tabs.iter().position(|t| {
                matches!(t.target, TabTarget::View(id) if matches!(doc.view(id).map(|v| &v.kind), Some(ViewKind::ThreeD { .. })))
            });
            self.right = three_d
                .filter(|i| Some(*i) != self.left)
                .or_else(|| (0..self.tabs.len()).find(|&k| Some(k) != self.left));
        }
    }

    /// Asks the canvas of the focused pane to fit its drawing.
    pub fn zoom_extents(&mut self) {
        if let Some(Tab { body: TabBody::Canvas(c), .. }) = self.focused_mut() {
            c.zoom_extents();
        }
    }

    /// The tab bar.
    pub fn bar(&mut self, ui: &mut Ui, state: &AppState, actions: &mut Vec<Action>) {
        let locale = state.locale;
        let mut close = None;
        let mut select = None;
        let mut reorder = None;
        let focused = self.shown(self.focused_pane());
        let other = match self.focused_pane() {
            Pane::Left => self.shown(Pane::Right),
            Pane::Right => self.shown(Pane::Left),
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let bar_right = ui.max_rect().right() - 40.0;
            egui::ScrollArea::horizontal().max_width(bar_right - ui.cursor().left()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let name = title(state, tab.target);
                        let ic = icon(state.doc(), tab.target);
                        let is_focused = focused == Some(i);
                        let is_other = other == Some(i);
                        let r = chip(ui, i, &name, ic, is_focused, is_other, locale);
                        if r.select {
                            select = Some(i);
                        }
                        if r.close {
                            close = Some(i);
                        }
                        if let Some(from) = r.dropped_from {
                            reorder = Some((from, i));
                        }
                    }
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = tr(locale, "ui.tabs.split");
                if icons::button(ui, Icon::Split, self.split, 26.0, &label).clicked() {
                    actions.push(Action::ToggleSplit);
                }
            });
        });
        if let Some(i) = select {
            let pane = self.focused_pane();
            self.show_in(pane, i);
        }
        if let Some((from, to)) = reorder {
            self.reorder(from, to);
        }
        if let Some(i) = close {
            self.close(i);
        }
    }
}

struct ChipResult {
    select: bool,
    close: bool,
    dropped_from: Option<usize>,
}

#[derive(Clone, Copy)]
struct DraggedTab(usize);

fn chip(
    ui: &mut Ui,
    index: usize,
    name: &str,
    icon: Icon,
    focused: bool,
    other: bool,
    locale: kanavos_units::Locale,
) -> ChipResult {
    let t = Tokens::of(ui);
    let font = egui::FontId::proportional(crate::theme::size::BODY);
    let text_color = if focused { t.text } else { t.text_muted };
    let galley = ui.painter().layout_no_wrap(name.to_owned(), font, text_color);
    let height = 30.0;
    let width = galley.size().x + 22.0 + 16.0 + 22.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let close_rect =
        Rect::from_center_size(egui::pos2(rect.right() - 13.0, rect.center().y), Vec2::splat(18.0));
    let close_resp = ui.interact(close_rect, ui.id().with(("tab-close", index)), Sense::click());
    let mut result = ChipResult { select: false, close: false, dropped_from: None };
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if focused {
            t.raised
        } else if response.hovered() {
            t.hover
        } else {
            Color32::TRANSPARENT
        };
        p.rect_filled(
            rect.shrink2(Vec2::new(0.0, 1.0)),
            egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 },
            bg,
        );
        if focused || other {
            let c = if focused { t.accent } else { t.border_strong };
            let y = rect.bottom() - 1.5;
            p.line_segment(
                [egui::pos2(rect.left() + 4.0, y), egui::pos2(rect.right() - 4.0, y)],
                Stroke::new(2.0, c),
            );
        }
        let icon_rect =
            Rect::from_center_size(egui::pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(16.0));
        icons::paint(p, icon_rect, icon, if focused { t.accent } else { t.text_muted });
        p.galley(egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0), galley, text_color);
        if close_resp.hovered() {
            p.rect_filled(close_rect, 4.0, t.hover);
        }
        if focused || response.hovered() || close_resp.hovered() {
            icons::paint(p, close_rect.shrink(3.0), Icon::Close, t.text_muted);
        }
    }
    let close_label = tr(locale, "ui.tabs.close");
    let close_resp = close_resp.on_hover_text(&close_label);
    close_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, close_label.clone()));
    let label = name.to_owned();
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, focused, label.clone()));
    if close_resp.clicked() || response.middle_clicked() {
        result.close = true;
    } else if response.clicked() {
        result.select = true;
    }
    if response.drag_started() {
        response.dnd_set_drag_payload(DraggedTab(index));
    }
    if let Some(p) = response.dnd_hover_payload::<DraggedTab>()
        && p.0 != index
    {
        let x = if p.0 > index { rect.left() } else { rect.right() };
        ui.painter().line_segment(
            [egui::pos2(x, rect.top() + 4.0), egui::pos2(x, rect.bottom() - 4.0)],
            Stroke::new(2.0, t.accent),
        );
    }
    if let Some(p) = response.dnd_release_payload::<DraggedTab>() {
        result.dropped_from = Some(p.0);
    }
    response.context_menu(|ui| {
        if ui.button(tr(locale, "ui.tabs.close")).clicked() {
            result.close = true;
            ui.close();
        }
    });
    result
}

/// Shows a pane: a slim header with the tab's title and scale, then the tab body.
pub fn pane_ui(
    ui: &mut Ui,
    tab: &mut Tab,
    focused: bool,
    split: bool,
    state: &mut AppState,
    frame: &mut eframe::Frame,
    actions: &mut Vec<Action>,
) {
    let t = Tokens::of(ui);
    if split {
        let header = ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 22.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.add_space(8.0);
                let name = title(state, tab.target);
                let c = if focused { t.accent } else { t.text_muted };
                icons::inline(ui, icon(state.doc(), tab.target), c);
                ui.label(RichText::new(name).small().color(if focused { t.text } else { t.text_muted }));
            },
        );
        let r = header.response.rect;
        ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, t.border));
    }
    let rect = ui.available_rect_before_wrap();
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
    match &mut tab.body {
        TabBody::Canvas(c) => c.ui(&mut child, state),
        TabBody::ThreeD(v) => v.ui(&mut child, frame, state),
        TabBody::Table(tv) => tv.ui(&mut child, state, actions),
    }
    ui.advance_cursor_after_rect(rect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_model::ScheduleKind;

    fn ws() -> (Document, Workspace) {
        let doc = kanavos_model::starter();
        let ground = doc.storeys().first().map(|s| s.id).unwrap_or_default();
        let w = Workspace::with_defaults(&doc, ground, true);
        (doc, w)
    }

    #[test]
    fn defaults_show_the_plan_left_and_3d_right() {
        let (doc, w) = ws();
        let left = w.left.and_then(|i| w.tabs.get(i)).map(|t| t.target);
        let right = w.right.and_then(|i| w.tabs.get(i)).map(|t| t.target);
        let ground =
            doc.storeys().first().and_then(|s| doc.plan_view_of(s.id)).map(|v| TabTarget::View(v.id));
        assert_eq!(left, ground);
        assert!(
            matches!(right, Some(TabTarget::View(id)) if matches!(doc.view(id).map(|v| &v.kind), Some(ViewKind::ThreeD { .. })))
        );
    }

    #[test]
    fn opening_an_open_tab_focuses_it_without_duplicates() {
        let (doc, mut w) = ws();
        let n = w.tabs.len();
        assert!(w.open(&doc, TabTarget::Schedule(ScheduleKind::Doors)));
        assert!(w.open(&doc, TabTarget::Schedule(ScheduleKind::Doors)));
        assert_eq!(w.tabs.len(), n + 1);
        assert_eq!(w.focused().map(|t| t.target), Some(TabTarget::Schedule(ScheduleKind::Doors)));
    }

    #[test]
    fn showing_the_other_panes_tab_swaps_panes() {
        let (_, mut w) = ws();
        let (l, r) = (w.left, w.right);
        if let Some(r) = r {
            w.show_in(Pane::Left, r);
        }
        assert_eq!(w.left, r);
        assert_eq!(w.right, l);
    }

    #[test]
    fn closing_keeps_panes_valid() {
        let (_, mut w) = ws();
        while !w.tabs.is_empty() {
            w.close(0);
            for i in [w.left, w.right].into_iter().flatten() {
                assert!(i < w.tabs.len());
            }
        }
        assert_eq!(w.left, None);
    }

    #[test]
    fn reordering_follows_the_shown_tabs() {
        let (_, mut w) = ws();
        let left_target = w.left.and_then(|i| w.tabs.get(i)).map(|t| t.target);
        let last = w.tabs.len() - 1;
        w.reorder(0, last);
        assert_eq!(w.left.and_then(|i| w.tabs.get(i)).map(|t| t.target), left_target);
    }

    #[test]
    fn tabs_of_removed_views_are_closed() {
        let (doc, mut w) = ws();
        let empty = Document::new();
        w.prune(&empty);
        // Only model-independent tabs could survive; the defaults are all views.
        assert!(w.tabs.is_empty());
        w.open(&doc, TabTarget::AreaTable);
        w.prune(&empty);
        assert_eq!(w.tabs.len(), 1);
    }
}
