//! The command palette (`Ctrl+K`, docs/ux.md §3): one search box for tools, commands, views, sheets,
//! schedules, storeys, settings and elements, in Greek and English, with fuzzy accent-insensitive matching.
//!
//! Every entry carries its text in both languages, so "wall" finds the wall tool in a Greek interface and
//! "τοιχ πάχ" finds the wall thickness setting in an English one.

use egui::{Align2, Key, RichText, Ui, Vec2};
use kanavos_model::{Category, Document, ElementKind, ViewKind};
use kanavos_tools::ToolId;
use kanavos_units::Locale;

use crate::actions::{Action, Panel, TabTarget, Window};
use crate::exports::ExportKind;
use crate::fuzzy;
use crate::icons::{self, Icon};
use crate::imports::ImportKind;
use crate::names::{self, tr};
use crate::theme::{Theme, Tokens};

/// Maximum number of results shown.
pub const MAX_RESULTS: usize = 60;

/// One searchable entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// What happens when it is chosen.
    pub action: Action,
    /// Title in the interface language.
    pub title: String,
    /// Kind of entry (tool, command, view …) in the interface language.
    pub kind: String,
    /// Shortcut shown on the right, if any.
    pub shortcut: String,
    /// Icon.
    pub icon: Icon,
    /// Text matched against the query (both languages and extra keywords).
    pub haystack: String,
}

fn both(key: &str) -> String {
    format!("{} {}", tr(Locale::El, key), tr(Locale::En, key))
}

/// Builds every entry for a document.
pub fn entries(doc: &Document, locale: Locale) -> Vec<Entry> {
    let mut v = Vec::new();
    let kind_tool = tr(locale, "ui.palette.kind.tool");
    let kind_cmd = tr(locale, "ui.palette.kind.command");
    let kind_view = tr(locale, "ui.palette.kind.view");
    let kind_sheet = tr(locale, "ui.palette.kind.sheet");
    let kind_storey = tr(locale, "ui.palette.kind.storey");
    let kind_setting = tr(locale, "ui.palette.kind.setting");
    let kind_element = tr(locale, "ui.palette.kind.element");
    let mut push = |action: Action, title: String, kind: &str, shortcut: &str, icon: Icon, extra: String| {
        let haystack = format!("{title} {extra}");
        v.push(Entry { action, title, kind: kind.to_owned(), shortcut: shortcut.to_owned(), icon, haystack });
    };

    for id in ToolId::ALL {
        let key = format!("ui.tool.{}", id.key());
        let shortcut = id.shortcut().map(|c| c.to_string()).unwrap_or_default();
        push(Action::Tool(id), names::tool(locale, id), &kind_tool, &shortcut, Icon::Tool(id), both(&key));
    }
    // Tool settings people look for by name ("τοιχ πάχ").
    let settings: [(ToolId, &str); 7] = [
        (ToolId::Wall, "ui.palette.set.wall_thickness"),
        (ToolId::Wall, "ui.palette.set.wall_height"),
        (ToolId::Door, "ui.palette.set.door_size"),
        (ToolId::Window, "ui.palette.set.window_sill"),
        (ToolId::Stair, "ui.palette.set.stair_riser"),
        (ToolId::Roof, "ui.palette.set.roof_pitch"),
        (ToolId::Text, "ui.palette.set.text_size"),
    ];
    for (tool, key) in settings {
        push(Action::Tool(tool), tr(locale, key), &kind_setting, "", Icon::Tool(tool), both(key));
    }

    let commands: Vec<(Action, &str, &str, Icon)> = vec![
        (Action::New, "ui.menu.new", "Ctrl+N", Icon::New),
        (Action::Open, "ui.menu.open", "Ctrl+O", Icon::Open),
        (Action::Save, "ui.menu.save", "Ctrl+S", Icon::Save),
        (Action::SaveAs, "ui.menu.save_as", "Ctrl+Shift+S", Icon::Save),
        (Action::OpenSample, "ui.menu.open_sample", "", Icon::Open),
        (Action::Undo, "ui.menu.undo", "Ctrl+Z", Icon::Undo),
        (Action::Redo, "ui.menu.redo", "Ctrl+Y", Icon::Redo),
        (Action::Delete, "ui.menu.delete", "Del", Icon::Trash),
        (Action::SelectAll, "ui.menu.select_all", "Ctrl+A", Icon::Tool(ToolId::Select)),
        (Action::SelectNone, "ui.menu.select_none", "Esc", Icon::Close),
        (Action::TogglePanels, "ui.menu.toggle_panels", "Tab", Icon::Panels),
        (Action::TogglePanel(Panel::History), "ui.menu.history", "", Icon::History),
        (Action::ToggleSplit, "ui.menu.split", "", Icon::Split),
        (Action::ZoomExtents, "ui.menu.zoom_extents", "Home", Icon::ZoomExtents),
        (Action::NewSection, "ui.menu.new_section", "", Icon::Tool(ToolId::Section)),
        (Action::NewElevation, "ui.menu.new_elevation", "", Icon::Tool(ToolId::Elevation)),
        (Action::NewSheet, "ui.menu.new_sheet", "", Icon::Sheet),
        (Action::PermitSet, "ui.menu.permit_set", "", Icon::Layers),
        (Action::DetectSpaces, "ui.menu.detect_spaces", "", Icon::Tool(ToolId::Space)),
        (Action::OpenWindow(Window::Shortcuts), "ui.menu.shortcuts", "F1", Icon::Info),
        (Action::OpenWindow(Window::About), "ui.menu.about", "", Icon::Info),
    ];
    for (action, key, shortcut, icon) in commands {
        push(action, tr(locale, key), &kind_cmd, shortcut, icon, both(key));
    }
    for k in ExportKind::ALL {
        push(
            Action::Export(k),
            tr(locale, k.label_key()),
            &kind_cmd,
            "",
            Icon::Export,
            format!("{} {}", both(k.label_key()), both("ui.menu.export")),
        );
    }
    for k in ImportKind::ALL {
        push(
            Action::Import(k),
            tr(locale, k.label_key()),
            &kind_cmd,
            "",
            Icon::Import,
            format!("{} {}", both(k.label_key()), both("ui.menu.import")),
        );
    }
    let windows: [(Window, &str, Icon); 6] = [
        (Window::Materials, "ui.window.materials", Icon::Material),
        (Window::BuildUps, "ui.window.buildups", Icon::Layers),
        (Window::Storeys, "ui.window.storeys", Icon::Storey),
        (Window::Project, "ui.window.project", Icon::Info),
        (Window::Site, "ui.window.site", Icon::Site),
        (Window::Defaults, "ui.window.defaults", Icon::Gear),
    ];
    for (w, key, icon) in windows {
        push(Action::OpenWindow(w), tr(locale, key), &kind_setting, "", icon, both(key));
    }
    push(
        Action::Theme(Theme::Light),
        tr(locale, "ui.menu.theme_light"),
        &kind_setting,
        "",
        Icon::Gear,
        both("ui.menu.theme_light"),
    );
    push(
        Action::Theme(Theme::Dark),
        tr(locale, "ui.menu.theme_dark"),
        &kind_setting,
        "",
        Icon::Gear,
        both("ui.menu.theme_dark"),
    );
    push(
        Action::Language(Locale::El),
        "Ελληνικά".into(),
        &kind_setting,
        "",
        Icon::Gear,
        "greek language γλώσσα".into(),
    );
    push(
        Action::Language(Locale::En),
        "English".into(),
        &kind_setting,
        "",
        Icon::Gear,
        "english language γλώσσα αγγλικά".into(),
    );

    for s in doc.storeys().into_iter().rev() {
        push(
            Action::ActivateStorey(s.id),
            s.name.clone(),
            &kind_storey,
            "",
            Icon::Storey,
            both("ui.tree.storeys"),
        );
    }
    for view in doc.views() {
        let icon = match view.kind {
            ViewKind::Plan { .. } => Icon::PlanView,
            ViewKind::Section { .. } => Icon::SectionView,
            ViewKind::Elevation { .. } => Icon::ElevationView,
            ViewKind::ThreeD { .. } => Icon::Cube,
            ViewKind::SitePlan => Icon::Site,
            ViewKind::CoverageDiagram => Icon::Coverage,
            ViewKind::AreaTable | ViewKind::Schedule { .. } => Icon::Table,
        };
        let extra = format!(
            "{} {}",
            names::view_kind(Locale::El, &view.kind),
            names::view_kind(Locale::En, &view.kind)
        );
        push(Action::OpenTab(TabTarget::View(view.id)), view.name.clone(), &kind_view, "", icon, extra);
    }
    for kind in names::SCHEDULES {
        let el = names::schedule(Locale::El, kind);
        let en = names::schedule(Locale::En, kind);
        push(
            Action::OpenTab(TabTarget::Schedule(kind)),
            names::schedule(locale, kind),
            &kind_view,
            "",
            Icon::Table,
            format!("{el} {en}"),
        );
    }
    push(
        Action::OpenTab(TabTarget::AreaTable),
        tr(locale, "ui.tree.area_table"),
        &kind_view,
        "",
        Icon::Table,
        both("ui.tree.area_table"),
    );
    for s in doc.sheets() {
        push(
            Action::OpenTab(TabTarget::Sheet(s.id)),
            format!("{} {}", s.number, s.name),
            &kind_sheet,
            "",
            Icon::Sheet,
            both("ui.tree.sheets"),
        );
    }
    for e in doc.elements() {
        let category = e.category();
        if matches!(category, Category::Underlay) && e.storey.is_none() {
            continue;
        }
        let storey = e.storey.and_then(|s| doc.storey(s)).map(|s| s.name.clone()).unwrap_or_default();
        let name = match &e.kind {
            ElementKind::Space(s) => format!("{} {}", s.number, s.name).trim().to_owned(),
            _ => String::new(),
        };
        let tag = if e.tag.is_empty() { e.id.to_string() } else { format!("{} {}", e.tag, e.id) };
        let title = format!("{} {} {}", names::category(locale, category), tag, name).trim().to_owned();
        let extra = format!(
            "{} {} {} {}",
            names::category(Locale::El, category),
            names::category(Locale::En, category),
            names::category_plural(Locale::El, category),
            storey
        );
        let detail =
            if storey.is_empty() { kind_element.clone() } else { format!("{kind_element} · {storey}") };
        let icon = crate::inspector::category_icon(category);
        push(Action::SelectElement(e.id), title, &detail, "", icon, extra);
    }
    v
}

/// Ranks entries for a query, best first, at most [`MAX_RESULTS`].
pub fn search(entries: &[Entry], query: &str) -> Vec<usize> {
    let ranked = fuzzy::rank(query, entries.iter().enumerate().map(|(i, e)| (i, e.haystack.clone())));
    ranked.into_iter().map(|(i, _)| i).take(MAX_RESULTS).collect()
}

/// The palette window.
#[derive(Default)]
pub struct CommandPalette {
    /// Whether it is shown.
    pub open: bool,
    query: String,
    selected: usize,
    focus: bool,
    cache: Option<(u64, Locale, Vec<Entry>)>,
}

impl CommandPalette {
    /// Opens the palette, optionally with a query.
    pub fn show(&mut self, query: &str) {
        self.open = true;
        self.query = query.to_owned();
        self.selected = 0;
        self.focus = true;
    }

    /// Shows the palette when open; returns the chosen action.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        doc: &Document,
        revision: u64,
        locale: Locale,
    ) -> Option<Action> {
        if !self.open {
            return None;
        }
        let stale = !matches!(&self.cache, Some((r, l, _)) if *r == revision && *l == locale);
        if stale {
            self.cache = Some((revision, locale, entries(doc, locale)));
        }
        let all: &[Entry] = match &self.cache {
            Some((_, _, e)) => e,
            None => &[],
        };
        let results = search(all, &self.query);
        if self.selected >= results.len() {
            self.selected = results.len().saturating_sub(1);
        }
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            self.open = false;
            return None;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        if down && self.selected + 1 < results.len() {
            self.selected += 1;
        }
        let mut chosen = None;
        if enter {
            chosen = results.get(self.selected).and_then(|&i| all.get(i)).map(|e| e.action.clone());
        }
        let screen = ctx.content_rect();
        let width = 600.0_f32.min(screen.width() - 40.0);
        let area = egui::Area::new(egui::Id::new("command-palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 70.0))
            .show(ctx, |ui| {
                let t = Tokens::of(ui);
                egui::Frame::popup(ui.style()).fill(t.raised).corner_radius(10.0).inner_margin(10.0).show(
                    ui,
                    |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            icons::inline(ui, Icon::Search, t.text_muted);
                            let edit = egui::TextEdit::singleline(&mut self.query)
                                .hint_text(tr(locale, "ui.palette.placeholder"))
                                .desired_width(f32::INFINITY)
                                .frame(egui::Frame::NONE);
                            let r = ui.add(edit);
                            if self.focus {
                                r.request_focus();
                                self.focus = false;
                            }
                            if r.changed() {
                                self.selected = 0;
                            }
                        });
                        crate::widgets::rule(ui);
                        ui.add_space(4.0);
                        if results.is_empty() {
                            ui.label(RichText::new(tr(locale, "ui.palette.no_results")).color(t.text_muted));
                            return;
                        }
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            for (n, &i) in results.iter().enumerate() {
                                let Some(e) = all.get(i) else { continue };
                                if result_row(ui, e, n == self.selected, n == self.selected && (up || down))
                                    .clicked()
                                {
                                    chosen = Some(e.action.clone());
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.label(RichText::new(tr(locale, "ui.palette.footer")).small().color(t.text_faint));
                    },
                );
            });
        let clicked_outside = ctx.input(|i| i.pointer.any_pressed())
            && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if clicked_outside {
            self.open = false;
        }
        if chosen.is_some() {
            self.open = false;
        }
        chosen
    }
}

fn result_row(ui: &mut Ui, e: &Entry, selected: bool, scroll_to: bool) -> egui::Response {
    let t = Tokens::of(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 6.0, t.accent_weak);
        } else if response.hovered() {
            p.rect_filled(rect, 6.0, t.hover);
        }
        let icon_rect =
            egui::Rect::from_center_size(rect.left_center() + Vec2::new(16.0, 0.0), Vec2::splat(18.0));
        icons::paint(p, icon_rect, e.icon, if selected { t.accent } else { t.text_muted });
        p.text(
            rect.left_center() + Vec2::new(34.0, 0.0),
            Align2::LEFT_CENTER,
            &e.title,
            egui::FontId::proportional(crate::theme::size::BODY),
            t.text,
        );
        let right =
            if e.shortcut.is_empty() { e.kind.clone() } else { format!("{}   {}", e.kind, e.shortcut) };
        p.text(
            rect.right_center() - Vec2::new(10.0, 0.0),
            Align2::RIGHT_CENTER,
            right,
            egui::FontId::proportional(crate::theme::size::SMALL),
            t.text_muted,
        );
    }
    if scroll_to {
        response.scroll_to_me(None);
    }
    let label = e.title.clone();
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label.clone()));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
        search(entries, query).into_iter().filter_map(|i| entries.get(i)).collect()
    }

    #[test]
    fn tools_are_found_in_both_languages() {
        let doc = kanavos_model::starter();
        let e = entries(&doc, Locale::El);
        assert_eq!(find(&e, "τοιχ").first().map(|x| &x.action), Some(&Action::Tool(ToolId::Wall)));
        assert_eq!(find(&e, "wall").first().map(|x| &x.action), Some(&Action::Tool(ToolId::Wall)));
    }

    #[test]
    fn wall_thickness_is_found_by_partial_greek_words() {
        let doc = kanavos_model::starter();
        let e = entries(&doc, Locale::En);
        let hits = find(&e, "τοιχ πάχ");
        assert!(
            hits.iter().any(|x| x.action == Action::Tool(ToolId::Wall)
                && x.kind != tr(Locale::En, "ui.palette.kind.tool"))
        );
    }

    #[test]
    fn views_sheets_and_storeys_are_searchable() {
        let doc = crate::sample::sample_house(Locale::El);
        let e = entries(&doc, Locale::El);
        assert!(find(&e, "τομη α").iter().any(|x| matches!(x.action, Action::OpenTab(TabTarget::View(_)))));
        assert!(find(&e, "ισογειο").iter().any(|x| matches!(x.action, Action::ActivateStorey(_))));
        assert!(find(&e, "παραθυρ").iter().any(|x| matches!(x.action, Action::SelectElement(_))));
        assert!(
            find(&e, "door schedule")
                .iter()
                .any(|x| matches!(x.action, Action::OpenTab(TabTarget::Schedule(_))))
        );
    }

    #[test]
    fn results_are_bounded() {
        let doc = crate::sample::sample_house(Locale::El);
        let e = entries(&doc, Locale::El);
        assert!(search(&e, "").len() <= MAX_RESULTS);
    }
}
