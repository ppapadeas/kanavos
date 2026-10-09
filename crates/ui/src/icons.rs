//! Kanavos icons, drawn with egui painter primitives on a 24 × 24 grid (docs/ux.md §6).
//!
//! Every icon is our own: simple drafting marks (a wall band, a door swing, a level triangle …) drawn with one
//! stroke weight. They scale with the button they sit in and take their colour from the theme, so there are no
//! image assets to license or keep in sync.

use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo, WidgetType};
use kanavos_tools::ToolId;

use crate::theme::Tokens;

/// An icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// The icon of a tool.
    Tool(ToolId),
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// New project.
    New,
    /// Open.
    Open,
    /// Save.
    Save,
    /// Search.
    Search,
    /// Close (×).
    Close,
    /// Add (+).
    Plus,
    /// Remove (−).
    Minus,
    /// Move up.
    ArrowUp,
    /// Move down.
    ArrowDown,
    /// Expand (pointing right).
    ChevronRight,
    /// Collapse (pointing down).
    ChevronDown,
    /// Pointing left.
    ChevronLeft,
    /// Two panes side by side.
    Split,
    /// Side panels.
    Panels,
    /// History.
    History,
    /// Properties.
    Properties,
    /// Storeys.
    Storey,
    /// Views.
    Views,
    /// Sheets.
    Sheet,
    /// Schedules and tables.
    Table,
    /// Site and plot.
    Site,
    /// 3D view.
    Cube,
    /// Section view.
    SectionView,
    /// Elevation view.
    ElevationView,
    /// Plan view.
    PlanView,
    /// Coverage diagram.
    Coverage,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Check mark.
    Check,
    /// Information.
    Info,
    /// Duplicate.
    Copy,
    /// Delete.
    Trash,
    /// Snapping on/off.
    Snap,
    /// Grid snapping.
    GridSnap,
    /// Point snapping.
    PointSnap,
    /// Edge snapping.
    EdgeSnap,
    /// Materials.
    Material,
    /// Build-ups.
    Layers,
    /// Settings.
    Gear,
    /// Fit the drawing in the view.
    ZoomExtents,
    /// Export.
    Export,
    /// Import.
    Import,
    /// Reference line on the left face.
    RefLeft,
    /// Reference line on the centre.
    RefCentre,
    /// Reference line on the right face.
    RefRight,
}

/// A pen drawing on the 24-unit grid of a rectangle.
struct Pen<'a> {
    painter: &'a egui::Painter,
    rect: Rect,
    color: Color32,
    width: f32,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        let k = self.rect.width().min(self.rect.height()) / 24.0;
        let o = self.rect.center() - Vec2::splat(12.0 * k);
        Pos2::new(o.x + x * k, o.y + y * k)
    }
    fn k(&self) -> f32 {
        self.rect.width().min(self.rect.height()) / 24.0
    }
    fn stroke(&self) -> Stroke {
        Stroke::new(self.width, self.color)
    }
    fn thin(&self) -> Stroke {
        Stroke::new(self.width * 0.6, self.color)
    }
    fn line(&self, a: (f32, f32), b: (f32, f32)) {
        self.painter.line_segment([self.p(a.0, a.1), self.p(b.0, b.1)], self.stroke());
    }
    fn thin_line(&self, a: (f32, f32), b: (f32, f32)) {
        self.painter.line_segment([self.p(a.0, a.1), self.p(b.0, b.1)], self.thin());
    }
    fn path(&self, pts: &[(f32, f32)], closed: bool) {
        let points: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        if closed {
            self.painter.add(Shape::closed_line(points, self.stroke()));
        } else {
            self.painter.add(Shape::line(points, self.stroke()));
        }
    }
    fn dashed(&self, pts: &[(f32, f32)]) {
        let points: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        let k = self.k();
        self.painter.extend(Shape::dashed_line(&points, self.thin(), 2.2 * k, 1.6 * k));
    }
    fn fill(&self, pts: &[(f32, f32)], color: Color32) {
        let points: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.path(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true);
    }
    fn fill_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color32) {
        self.fill(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], color);
    }
    fn circle(&self, c: (f32, f32), r: f32) {
        self.painter.circle_stroke(self.p(c.0, c.1), r * self.k(), self.stroke());
    }
    fn dot(&self, c: (f32, f32), r: f32) {
        self.painter.circle_filled(self.p(c.0, c.1), r * self.k(), self.color);
    }
    /// Arc from angle `a0` to `a1` (degrees, counter-clockwise on screen means negative y).
    fn arc(&self, c: (f32, f32), r: f32, a0: f32, a1: f32) {
        let n = 16;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                (c.0 + r * a.cos(), c.1 - r * a.sin())
            })
            .collect();
        self.path(&pts, false);
    }
    fn tint(&self, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), alpha)
    }
}

/// Paints an icon into a rectangle (the icon keeps its square proportions, centred).
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let k = rect.width().min(rect.height()) / 24.0;
    let pen = Pen { painter, rect, color, width: (1.5 * k).max(1.0) };
    let soft = pen.tint(60);
    match icon {
        Icon::Tool(tool) => paint_tool(&pen, tool),
        Icon::Undo => {
            pen.arc((12.0, 13.0), 6.5, 160.0, -40.0);
            pen.path(&[(4.0, 8.0), (5.4, 13.5), (10.0, 10.5)], false);
        }
        Icon::Redo => {
            pen.arc((12.0, 13.0), 6.5, 20.0, 220.0);
            pen.path(&[(20.0, 8.0), (18.6, 13.5), (14.0, 10.5)], false);
        }
        Icon::New => {
            pen.path(&[(6.0, 3.5), (14.5, 3.5), (18.5, 7.5), (18.5, 20.5), (6.0, 20.5)], true);
            pen.line((12.25, 9.5), (12.25, 16.5));
            pen.line((8.75, 13.0), (15.75, 13.0));
        }
        Icon::Open => {
            pen.path(&[(3.5, 6.0), (9.0, 6.0), (11.0, 8.0), (20.5, 8.0), (20.5, 18.5), (3.5, 18.5)], true);
            pen.line((3.5, 11.0), (20.5, 11.0));
        }
        Icon::Save => {
            pen.path(&[(4.5, 4.5), (17.0, 4.5), (19.5, 7.0), (19.5, 19.5), (4.5, 19.5)], true);
            pen.rect(8.0, 4.5, 15.0, 9.0);
            pen.rect(7.5, 13.0, 16.5, 19.5);
        }
        Icon::Search => {
            pen.circle((10.5, 10.5), 5.5);
            pen.line((14.6, 14.6), (19.5, 19.5));
        }
        Icon::Close => {
            pen.line((7.0, 7.0), (17.0, 17.0));
            pen.line((17.0, 7.0), (7.0, 17.0));
        }
        Icon::Plus => {
            pen.line((12.0, 6.0), (12.0, 18.0));
            pen.line((6.0, 12.0), (18.0, 12.0));
        }
        Icon::Minus => pen.line((6.0, 12.0), (18.0, 12.0)),
        Icon::ArrowUp => {
            pen.line((12.0, 18.0), (12.0, 6.0));
            pen.path(&[(7.0, 11.0), (12.0, 6.0), (17.0, 11.0)], false);
        }
        Icon::ArrowDown => {
            pen.line((12.0, 6.0), (12.0, 18.0));
            pen.path(&[(7.0, 13.0), (12.0, 18.0), (17.0, 13.0)], false);
        }
        Icon::ChevronRight => pen.path(&[(10.0, 7.0), (15.0, 12.0), (10.0, 17.0)], false),
        Icon::ChevronDown => pen.path(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)], false),
        Icon::ChevronLeft => pen.path(&[(14.0, 7.0), (9.0, 12.0), (14.0, 17.0)], false),
        Icon::Split => {
            pen.rect(3.5, 5.5, 20.5, 18.5);
            pen.line((12.0, 5.5), (12.0, 18.5));
        }
        Icon::Panels => {
            pen.rect(3.5, 5.5, 20.5, 18.5);
            pen.fill_rect(3.5, 5.5, 8.0, 18.5, soft);
            pen.fill_rect(16.0, 5.5, 20.5, 18.5, soft);
            pen.line((8.0, 5.5), (8.0, 18.5));
            pen.line((16.0, 5.5), (16.0, 18.5));
        }
        Icon::History => {
            pen.circle((12.0, 12.0), 8.0);
            pen.path(&[(12.0, 7.5), (12.0, 12.0), (15.5, 14.0)], false);
        }
        Icon::Properties => {
            for (y, x) in [(7.0, 15.0), (12.0, 8.5), (17.0, 13.0)] {
                pen.line((4.0, y), (20.0, y));
                pen.painter.circle_filled(pen.p(x, y), 2.2 * pen.k(), color);
            }
        }
        Icon::Storey => {
            pen.line((4.0, 6.5), (20.0, 6.5));
            pen.line((4.0, 12.0), (20.0, 12.0));
            pen.line((4.0, 17.5), (20.0, 17.5));
            pen.fill(&[(4.0, 12.0), (7.0, 9.5), (7.0, 14.5)], color);
        }
        Icon::Views | Icon::PlanView => {
            pen.rect(4.0, 4.0, 20.0, 20.0);
            pen.line((11.0, 4.0), (11.0, 13.0));
            pen.line((4.0, 13.0), (14.0, 13.0));
            pen.thin_line((14.0, 13.0), (20.0, 13.0));
        }
        Icon::Sheet => {
            pen.path(&[(5.0, 3.5), (15.0, 3.5), (19.0, 7.5), (19.0, 20.5), (5.0, 20.5)], true);
            pen.path(&[(15.0, 3.5), (15.0, 7.5), (19.0, 7.5)], false);
            pen.rect(11.0, 15.5, 19.0, 20.5);
        }
        Icon::Table => {
            pen.rect(3.5, 5.0, 20.5, 19.0);
            pen.line((3.5, 9.5), (20.5, 9.5));
            pen.thin_line((3.5, 14.25), (20.5, 14.25));
            pen.line((10.0, 5.0), (10.0, 19.0));
        }
        Icon::Site => {
            pen.dashed(&[(3.5, 8.0), (14.0, 4.0), (20.5, 10.0), (17.0, 20.0), (5.0, 18.0), (3.5, 8.0)]);
            pen.fill_rect(8.5, 9.5, 15.0, 15.5, soft);
            pen.rect(8.5, 9.5, 15.0, 15.5);
        }
        Icon::Cube => {
            pen.path(&[(12.0, 3.5), (19.5, 7.5), (19.5, 16.5), (12.0, 20.5), (4.5, 16.5), (4.5, 7.5)], true);
            pen.path(&[(4.5, 7.5), (12.0, 11.5), (19.5, 7.5)], false);
            pen.line((12.0, 11.5), (12.0, 20.5));
        }
        Icon::SectionView => {
            pen.line((4.0, 18.0), (20.0, 18.0));
            pen.path(&[(6.0, 18.0), (6.0, 10.0), (12.0, 5.0), (18.0, 10.0), (18.0, 18.0)], false);
            pen.fill_rect(6.0, 13.0, 18.0, 14.5, soft);
            pen.thin_line((6.0, 13.0), (18.0, 13.0));
        }
        Icon::ElevationView => {
            pen.line((3.5, 19.0), (20.5, 19.0));
            pen.path(&[(6.0, 19.0), (6.0, 10.0), (12.0, 5.0), (18.0, 10.0), (18.0, 19.0)], false);
            pen.rect(8.5, 12.0, 11.0, 15.0);
            pen.rect(13.0, 12.0, 15.5, 15.0);
        }
        Icon::Coverage => {
            pen.dashed(&[(3.5, 4.0), (20.5, 4.0), (20.5, 20.0), (3.5, 20.0), (3.5, 4.0)]);
            pen.fill_rect(7.0, 8.0, 15.5, 16.0, soft);
            pen.rect(7.0, 8.0, 15.5, 16.0);
            pen.thin_line((7.0, 12.0), (11.0, 8.0));
            pen.thin_line((7.0, 16.0), (15.0, 8.0));
            pen.thin_line((11.5, 16.0), (15.5, 12.0));
        }
        Icon::Warning => {
            pen.path(&[(12.0, 3.5), (21.0, 19.5), (3.0, 19.5)], true);
            pen.line((12.0, 9.0), (12.0, 14.0));
            pen.dot((12.0, 16.8), 1.1);
        }
        Icon::Error => {
            pen.circle((12.0, 12.0), 8.5);
            pen.line((12.0, 7.5), (12.0, 13.0));
            pen.dot((12.0, 16.0), 1.1);
        }
        Icon::Check => pen.path(&[(5.0, 12.5), (10.0, 17.0), (19.0, 7.0)], false),
        Icon::Info => {
            pen.circle((12.0, 12.0), 8.5);
            pen.line((12.0, 11.0), (12.0, 16.5));
            pen.dot((12.0, 8.0), 1.1);
        }
        Icon::Copy => {
            pen.rect(8.0, 8.0, 19.0, 19.0);
            pen.path(&[(5.0, 15.5), (5.0, 5.0), (15.5, 5.0)], false);
        }
        Icon::Trash => {
            pen.line((4.5, 6.5), (19.5, 6.5));
            pen.path(&[(9.5, 6.5), (9.5, 4.0), (14.5, 4.0), (14.5, 6.5)], false);
            pen.path(&[(6.5, 6.5), (7.5, 20.0), (16.5, 20.0), (17.5, 6.5)], false);
            pen.thin_line((10.5, 10.0), (10.5, 17.0));
            pen.thin_line((13.5, 10.0), (13.5, 17.0));
        }
        Icon::Snap => {
            pen.path(&[(6.0, 4.0), (6.0, 12.0)], false);
            pen.path(&[(18.0, 4.0), (18.0, 12.0)], false);
            pen.arc((12.0, 12.0), 6.0, 180.0, 360.0);
            pen.line((3.5, 7.0), (8.5, 7.0));
            pen.line((15.5, 7.0), (20.5, 7.0));
        }
        Icon::GridSnap => {
            for v in [5.0, 12.0, 19.0] {
                pen.thin_line((v, 4.0), (v, 20.0));
                pen.thin_line((4.0, v), (20.0, v));
            }
            pen.dot((12.0, 12.0), 2.2);
        }
        Icon::PointSnap => {
            pen.line((4.0, 18.0), (16.0, 6.0));
            pen.rect(13.0, 3.0, 19.0, 9.0);
            pen.dot((4.0, 18.0), 1.6);
        }
        Icon::EdgeSnap => {
            pen.line((3.5, 17.0), (20.5, 7.0));
            pen.path(&[(10.0, 9.5), (13.0, 15.0), (16.0, 9.5)], true);
        }
        Icon::Material => {
            pen.rect(4.0, 4.0, 20.0, 20.0);
            pen.thin_line((4.0, 12.0), (12.0, 4.0));
            pen.thin_line((4.0, 20.0), (20.0, 4.0));
            pen.thin_line((12.0, 20.0), (20.0, 12.0));
        }
        Icon::Layers => {
            pen.fill_rect(4.0, 4.0, 8.0, 20.0, soft);
            pen.rect(4.0, 4.0, 20.0, 20.0);
            pen.line((8.0, 4.0), (8.0, 20.0));
            pen.line((15.0, 4.0), (15.0, 20.0));
            pen.thin_line((15.0, 9.0), (20.0, 4.0));
            pen.thin_line((15.0, 15.0), (20.0, 10.0));
            pen.thin_line((15.0, 20.0), (20.0, 15.0));
        }
        Icon::Gear => {
            pen.circle((12.0, 12.0), 3.0);
            for i in 0..8 {
                let a = (i as f32 * 45.0).to_radians();
                pen.line(
                    (12.0 + 6.0 * a.cos(), 12.0 + 6.0 * a.sin()),
                    (12.0 + 8.5 * a.cos(), 12.0 + 8.5 * a.sin()),
                );
            }
            pen.circle((12.0, 12.0), 6.0);
        }
        Icon::ZoomExtents => {
            pen.path(&[(4.0, 9.0), (4.0, 4.0), (9.0, 4.0)], false);
            pen.path(&[(15.0, 4.0), (20.0, 4.0), (20.0, 9.0)], false);
            pen.path(&[(20.0, 15.0), (20.0, 20.0), (15.0, 20.0)], false);
            pen.path(&[(9.0, 20.0), (4.0, 20.0), (4.0, 15.0)], false);
            pen.rect(9.0, 9.0, 15.0, 15.0);
        }
        Icon::Export => {
            pen.path(&[(9.0, 6.0), (5.0, 6.0), (5.0, 19.0), (18.0, 19.0), (18.0, 15.0)], false);
            pen.line((11.0, 13.0), (19.5, 4.5));
            pen.path(&[(14.0, 4.5), (19.5, 4.5), (19.5, 10.0)], false);
        }
        Icon::Import => {
            pen.path(&[(9.0, 6.0), (5.0, 6.0), (5.0, 19.0), (18.0, 19.0), (18.0, 15.0)], false);
            pen.line((19.5, 4.5), (11.0, 13.0));
            pen.path(&[(11.0, 7.5), (11.0, 13.0), (16.5, 13.0)], false);
        }
        Icon::RefLeft | Icon::RefCentre | Icon::RefRight => {
            pen.fill_rect(4.0, 7.0, 20.0, 17.0, soft);
            pen.thin_line((4.0, 7.0), (20.0, 7.0));
            pen.thin_line((4.0, 17.0), (20.0, 17.0));
            let y = match icon {
                Icon::RefLeft => 7.0,
                Icon::RefCentre => 12.0,
                _ => 17.0,
            };
            pen.line((2.5, y), (21.5, y));
            pen.dot((2.5, y), 1.4);
        }
    }
}

/// Our own tool marks.
fn paint_tool(pen: &Pen<'_>, tool: ToolId) {
    let soft = pen.tint(70);
    match tool {
        ToolId::Select => {
            pen.fill(&[(6.0, 3.5), (6.0, 18.5), (10.0, 14.5)], soft);
            pen.path(&[(6.0, 3.5), (6.0, 18.5), (10.0, 14.5), (16.5, 14.5)], true);
            pen.line((10.0, 14.5), (13.0, 20.5));
        }
        ToolId::Wall => {
            pen.fill_rect(2.5, 8.5, 21.5, 15.5, soft);
            pen.line((2.5, 8.5), (21.5, 8.5));
            pen.line((2.5, 15.5), (21.5, 15.5));
            for x in [6.0, 11.0, 16.0] {
                pen.thin_line((x, 15.5), (x + 5.0, 8.5));
            }
        }
        ToolId::Slab => {
            pen.fill(&[(7.0, 8.0), (21.0, 8.0), (17.0, 15.0), (3.0, 15.0)], soft);
            pen.path(&[(7.0, 8.0), (21.0, 8.0), (17.0, 15.0), (3.0, 15.0)], true);
            pen.path(&[(3.0, 15.0), (3.0, 17.5), (17.0, 17.5), (21.0, 10.5), (21.0, 8.0)], false);
            pen.line((17.0, 15.0), (17.0, 17.5));
        }
        ToolId::Column => {
            pen.fill_rect(7.0, 7.0, 17.0, 17.0, soft);
            pen.rect(7.0, 7.0, 17.0, 17.0);
            pen.thin_line((7.0, 7.0), (17.0, 17.0));
            pen.thin_line((17.0, 7.0), (7.0, 17.0));
        }
        ToolId::Beam => {
            pen.dashed(&[(3.0, 10.0), (21.0, 10.0)]);
            pen.dashed(&[(3.0, 14.0), (21.0, 14.0)]);
            pen.fill_rect(2.5, 7.5, 6.5, 16.5, soft);
            pen.rect(2.5, 7.5, 6.5, 16.5);
            pen.fill_rect(17.5, 7.5, 21.5, 16.5, soft);
            pen.rect(17.5, 7.5, 21.5, 16.5);
        }
        ToolId::Door => {
            pen.fill_rect(2.0, 15.0, 6.0, 19.0, soft);
            pen.fill_rect(18.0, 15.0, 22.0, 19.0, soft);
            pen.line((2.0, 15.0), (6.0, 15.0));
            pen.line((18.0, 15.0), (22.0, 15.0));
            pen.line((6.0, 15.0), (6.0, 4.0));
            pen.arc((6.0, 15.0), 11.0, 0.0, 90.0);
        }
        ToolId::Window => {
            pen.fill_rect(2.0, 9.0, 6.0, 15.0, soft);
            pen.fill_rect(18.0, 9.0, 22.0, 15.0, soft);
            pen.rect(2.0, 9.0, 22.0, 15.0);
            pen.line((6.0, 9.0), (6.0, 15.0));
            pen.line((18.0, 9.0), (18.0, 15.0));
            pen.thin_line((6.0, 11.3), (18.0, 11.3));
            pen.thin_line((6.0, 12.7), (18.0, 12.7));
        }
        ToolId::Void => {
            pen.fill_rect(2.0, 9.0, 7.0, 15.0, soft);
            pen.fill_rect(17.0, 9.0, 22.0, 15.0, soft);
            pen.path(&[(2.0, 9.0), (7.0, 9.0), (7.0, 15.0), (2.0, 15.0)], false);
            pen.path(&[(22.0, 9.0), (17.0, 9.0), (17.0, 15.0), (22.0, 15.0)], false);
            pen.dashed(&[(7.0, 9.0), (17.0, 15.0)]);
            pen.dashed(&[(7.0, 15.0), (17.0, 9.0)]);
        }
        ToolId::Stair => {
            pen.path(
                &[(3.0, 20.0), (3.0, 16.0), (8.0, 16.0), (8.0, 12.0), (13.0, 12.0), (13.0, 8.0), (18.0, 8.0)],
                false,
            );
            pen.path(&[(18.0, 8.0), (18.0, 4.0), (21.0, 4.0)], false);
            pen.thin_line((3.0, 20.0), (21.0, 20.0));
        }
        ToolId::Roof => {
            pen.fill(&[(2.5, 14.0), (12.0, 5.0), (21.5, 14.0)], soft);
            pen.path(&[(2.5, 14.0), (12.0, 5.0), (21.5, 14.0)], false);
            pen.path(&[(5.5, 11.2), (5.5, 19.5), (18.5, 19.5), (18.5, 11.2)], false);
        }
        ToolId::Railing => {
            pen.line((3.0, 6.0), (21.0, 6.0));
            pen.line((3.0, 19.0), (21.0, 19.0));
            for x in [4.0, 8.0, 12.0, 16.0, 20.0] {
                pen.thin_line((x, 6.0), (x, 19.0));
            }
        }
        ToolId::Space => {
            pen.fill_rect(4.0, 5.0, 20.0, 19.0, soft);
            pen.rect(4.0, 5.0, 20.0, 19.0);
            pen.thin_line((8.5, 11.0), (15.5, 11.0));
            pen.thin_line((9.5, 14.0), (14.5, 14.0));
        }
        ToolId::Object => {
            pen.fill_rect(6.0, 8.0, 18.0, 16.0, soft);
            pen.rect(6.0, 8.0, 18.0, 16.0);
            pen.rect(9.0, 3.5, 15.0, 6.0);
            pen.rect(9.0, 18.0, 15.0, 20.5);
        }
        ToolId::Dimension => {
            pen.line((3.0, 13.0), (21.0, 13.0));
            pen.thin_line((4.0, 7.0), (4.0, 19.0));
            pen.thin_line((20.0, 7.0), (20.0, 19.0));
            pen.line((2.5, 14.5), (5.5, 11.5));
            pen.line((18.5, 14.5), (21.5, 11.5));
        }
        ToolId::Text => {
            pen.line((5.0, 5.0), (19.0, 5.0));
            pen.line((12.0, 5.0), (12.0, 20.0));
            pen.line((9.0, 20.0), (15.0, 20.0));
        }
        ToolId::Level => {
            pen.line((3.0, 15.0), (21.0, 15.0));
            pen.fill(&[(6.0, 9.0), (12.0, 9.0), (9.0, 15.0)], soft);
            pen.path(&[(6.0, 9.0), (12.0, 9.0), (9.0, 15.0)], true);
            pen.thin_line((12.0, 9.0), (20.0, 9.0));
        }
        ToolId::Line => {
            pen.line((5.0, 19.0), (19.0, 5.0));
            pen.dot((5.0, 19.0), 1.8);
            pen.dot((19.0, 5.0), 1.8);
        }
        ToolId::Polyline => {
            pen.path(&[(3.5, 18.0), (9.0, 7.0), (15.0, 15.0), (20.5, 5.0)], false);
            for p in [(3.5, 18.0), (9.0, 7.0), (15.0, 15.0), (20.5, 5.0)] {
                pen.dot(p, 1.6);
            }
        }
        ToolId::Arc => {
            pen.arc((12.0, 18.0), 9.0, 15.0, 165.0);
            pen.dot((20.7, 15.7), 1.6);
            pen.dot((3.3, 15.7), 1.6);
        }
        ToolId::Circle => {
            pen.circle((12.0, 12.0), 8.0);
            pen.dot((12.0, 12.0), 1.5);
        }
        ToolId::Fill => {
            pen.fill(&[(4.0, 18.0), (7.0, 6.0), (19.0, 4.0), (20.0, 18.0)], soft);
            pen.path(&[(4.0, 18.0), (7.0, 6.0), (19.0, 4.0), (20.0, 18.0)], true);
            pen.thin_line((6.0, 14.0), (13.0, 5.5));
            pen.thin_line((8.5, 18.0), (19.4, 7.0));
        }
        ToolId::Grid => {
            pen.circle((12.0, 6.5), 4.0);
            pen.dashed(&[(12.0, 10.5), (12.0, 21.5)]);
            pen.thin_line((10.5, 8.0), (12.0, 4.5));
            pen.thin_line((13.5, 8.0), (12.0, 4.5));
        }
        ToolId::Section => {
            pen.dashed(&[(12.0, 3.0), (12.0, 21.0)]);
            pen.line((12.0, 3.0), (12.0, 6.0));
            pen.line((12.0, 18.0), (12.0, 21.0));
            pen.fill(&[(12.0, 3.0), (18.0, 3.0), (12.0, 7.5)], pen.color);
            pen.fill(&[(12.0, 21.0), (18.0, 21.0), (12.0, 16.5)], pen.color);
        }
        ToolId::Elevation => {
            pen.circle((10.0, 12.0), 5.5);
            pen.fill(&[(15.5, 12.0), (20.5, 12.0), (15.5, 7.0)], pen.color);
            pen.fill(&[(15.5, 12.0), (20.5, 12.0), (15.5, 17.0)], pen.color);
            pen.thin_line((4.5, 12.0), (15.5, 12.0));
        }
        ToolId::Measure => {
            pen.path(&[(2.5, 15.0), (15.0, 2.5), (21.5, 9.0), (9.0, 21.5)], true);
            for i in 0..4 {
                let o = i as f32 * 3.0;
                pen.thin_line((6.0 + o, 11.5 - o), (8.0 + o, 13.5 - o));
            }
        }
        ToolId::Plot => {
            let pts = [(3.5, 7.0), (15.0, 3.5), (20.5, 12.0), (14.0, 20.5), (4.5, 17.0)];
            let mut ring: Vec<(f32, f32)> = pts.to_vec();
            ring.push((3.5, 7.0));
            pen.dashed(&ring);
            for p in pts {
                pen.dot(p, 1.6);
            }
        }
    }
}

/// A square icon button: hover background, accent tint when `selected`, tooltip and an AccessKit label.
pub fn button(ui: &mut Ui, icon: Icon, selected: bool, size: f32, label: &str) -> Response {
    button_enabled(ui, icon, selected, true, size, label)
}

/// [`button`] that can be disabled.
pub fn button_enabled(
    ui: &mut Ui,
    icon: Icon,
    selected: bool,
    enabled: bool,
    size: f32,
    label: &str,
) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), sense);
    let t = Tokens::of(ui);
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            Some(t.accent_weak)
        } else if enabled && response.hovered() {
            Some(t.hover)
        } else {
            None
        };
        if let Some(bg) = bg {
            ui.painter().rect_filled(rect, 4.0, bg);
        }
        if selected {
            ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        }
        let color = if !enabled {
            t.text_faint
        } else if selected {
            t.accent
        } else {
            t.text
        };
        let inset = (size * 0.18).round();
        paint(ui.painter(), rect.shrink(inset), icon, color);
    }
    let label_owned = label.to_owned();
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, selected, label_owned.clone()));
    if label.is_empty() { response } else { response.on_hover_text(label) }
}

/// Paints a small inline icon at the start of a row and returns the space it used.
pub fn inline(ui: &mut Ui, icon: Icon, color: Color32) -> Response {
    let size = ui.text_style_height(&egui::TextStyle::Body) + 2.0;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint(ui.painter(), rect.shrink(1.0), icon, color);
    }
    response
}
