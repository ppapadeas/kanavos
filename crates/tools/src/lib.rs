//! Interactive tools, snapping and inference, typed input.
//!
//! Layer L8. See `docs/architecture.md`, `docs/ux.md` §3 and the behaviour specs in `specs/tools/`.
//!
//! A tool is a small state machine. The canvas feeds it [`ToolEvent`]s in model coordinates; the tool answers with
//! a [`ToolOutput`]: preview geometry, commands to run, the snap it used, a prompt for the status bar, typed
//! input fields to show next to the cursor, handles of the selection and a text box when it needs text. Tools
//! never change the document themselves (ADR-0005): everything they do is a command.
//!
//! Snapping uses a spatial index shared by all tools ([`Snapper::cached`]), so pointer moves stay fast on large
//! storeys.

mod common;
pub mod input;
mod measure;
mod outline;
mod preview;
mod select;
mod settings;
pub mod snap;
mod wall;

use std::collections::BTreeSet;

use kanavos_drawing::Drawing;
use kanavos_engine::AnyCommand;
use kanavos_geom::Point2;
use kanavos_model::{Document, ElementKind, Id};
use kanavos_units::Locale;

pub use measure::MeasureTool;
pub use outline::OutlineTool;
pub use select::SelectTool;
pub use settings::{
    BeamSettings, ColumnSettings, DimensionOrientation, DimensionSettings, GridSettings, LevelSettings,
    ObjectPlacement, OpeningSettings, OutlineMode, RailingSettings, RoofKind, RoofSettings, SectionSettings,
    SlabSettings, SpaceSettings, StairSettings, TextSettings, ToolSettings, WallMode, WallSettings,
};
pub use snap::{SnapHint, SnapQuery, SnapResult, SnapSettings, Snapper};
pub use wall::WallTool;

/// Every tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ToolId {
    /// Select, move and edit with handles.
    Select,
    /// Walls (chained).
    Wall,
    /// Slabs by outline.
    Slab,
    /// Columns.
    Column,
    /// Beams.
    Beam,
    /// Doors in walls.
    Door,
    /// Windows in walls.
    Window,
    /// Empty openings in walls.
    Void,
    /// Stairs.
    Stair,
    /// Roofs by outline.
    Roof,
    /// Railings along a path.
    Railing,
    /// Spaces by clicking inside walls or drawing an outline.
    Space,
    /// Furniture and fixtures.
    Object,
    /// Dimensions.
    Dimension,
    /// Text notes.
    Text,
    /// Level marks.
    Level,
    /// Lines.
    Line,
    /// Polylines.
    Polyline,
    /// Arcs.
    Arc,
    /// Circles.
    Circle,
    /// Filled regions.
    Fill,
    /// Grid lines.
    Grid,
    /// Section lines (create a section view).
    Section,
    /// Elevation lines (create an elevation view).
    Elevation,
    /// Measure distances and areas without creating anything.
    Measure,
    /// Plot boundary.
    Plot,
}

impl ToolId {
    /// All tools, in toolbar order.
    pub const ALL: [ToolId; 26] = [
        ToolId::Select,
        ToolId::Wall,
        ToolId::Slab,
        ToolId::Column,
        ToolId::Beam,
        ToolId::Door,
        ToolId::Window,
        ToolId::Void,
        ToolId::Stair,
        ToolId::Roof,
        ToolId::Railing,
        ToolId::Space,
        ToolId::Object,
        ToolId::Dimension,
        ToolId::Text,
        ToolId::Level,
        ToolId::Line,
        ToolId::Polyline,
        ToolId::Arc,
        ToolId::Circle,
        ToolId::Fill,
        ToolId::Grid,
        ToolId::Section,
        ToolId::Elevation,
        ToolId::Measure,
        ToolId::Plot,
    ];

    /// Stable key, used for catalogue entries (`tools.<key>`) and keymaps.
    pub fn key(self) -> &'static str {
        match self {
            ToolId::Select => "select",
            ToolId::Wall => "wall",
            ToolId::Slab => "slab",
            ToolId::Column => "column",
            ToolId::Beam => "beam",
            ToolId::Door => "door",
            ToolId::Window => "window",
            ToolId::Void => "void",
            ToolId::Stair => "stair",
            ToolId::Roof => "roof",
            ToolId::Railing => "railing",
            ToolId::Space => "space",
            ToolId::Object => "object",
            ToolId::Dimension => "dimension",
            ToolId::Text => "text",
            ToolId::Level => "level",
            ToolId::Line => "line",
            ToolId::Polyline => "polyline",
            ToolId::Arc => "arc",
            ToolId::Circle => "circle",
            ToolId::Fill => "fill",
            ToolId::Grid => "grid",
            ToolId::Section => "section",
            ToolId::Elevation => "elevation",
            ToolId::Measure => "measure",
            ToolId::Plot => "plot",
        }
    }

    /// Default single-key shortcut, if any (our own keymap, see docs/ux.md §3).
    pub fn shortcut(self) -> Option<char> {
        match self {
            ToolId::Select => Some('V'),
            ToolId::Wall => Some('W'),
            ToolId::Slab => Some('F'),
            ToolId::Column => Some('C'),
            ToolId::Door => Some('D'),
            ToolId::Window => Some('N'),
            ToolId::Stair => Some('S'),
            ToolId::Roof => Some('R'),
            ToolId::Space => Some('A'),
            ToolId::Dimension => Some('M'),
            ToolId::Text => Some('T'),
            ToolId::Line => Some('L'),
            ToolId::Measure => Some('Q'),
            _ => None,
        }
    }
}

/// Mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary.
    Primary,
    /// Secondary (context).
    Secondary,
    /// Middle (pan, handled by the canvas).
    Middle,
}

/// Modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift: lock direction / add to selection.
    pub shift: bool,
    /// Ctrl (Cmd on macOS): toggle selection / copy while moving.
    pub ctrl: bool,
    /// Alt: disable snapping temporarily.
    pub alt: bool,
}

/// Keys tools react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Commit typed input or finish a chain (with Shift in the text box: a new line).
    Enter,
    /// Back out one step.
    Escape,
    /// Delete the last typed character or the last point.
    Backspace,
    /// Next typed-input field.
    Tab,
    /// Delete the selection.
    Delete,
    /// Flip side (door swing hand, wall reference side, roof slope edge, dimension orientation, object mirror).
    Flip,
    /// Rotate: turns an object or column being placed by 90°; starts rotating the selection in the select tool.
    Rotate,
    /// Mirror the selection about an axis (select tool).
    Mirror,
}

/// Input delivered to a tool. Positions are model coordinates (metres), not yet snapped.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolEvent {
    /// The pointer moved.
    Move {
        /// Position.
        at: Point2,
        /// Modifiers.
        mods: Modifiers,
    },
    /// A button went down.
    Press {
        /// Position.
        at: Point2,
        /// Button.
        button: MouseButton,
        /// Modifiers.
        mods: Modifiers,
    },
    /// A button went up (ends drags).
    Release {
        /// Position.
        at: Point2,
        /// Button.
        button: MouseButton,
        /// Modifiers.
        mods: Modifiers,
    },
    /// Double click.
    DoubleClick {
        /// Position.
        at: Point2,
    },
    /// A key.
    Key {
        /// Key.
        key: Key,
        /// Modifiers.
        mods: Modifiers,
    },
    /// Characters typed while the canvas has focus (numbers go to typed input, text to the text box).
    Text(String),
}

/// A typed-input field shown next to the cursor.
#[derive(Debug, Clone, PartialEq)]
pub struct InputField {
    /// Label, e.g. "Μήκος".
    pub label: String,
    /// Current text (typed or the live value).
    pub value: String,
    /// Whether this field receives typing.
    pub active: bool,
    /// Whether the value was typed (locked) rather than following the pointer.
    pub locked: bool,
}

/// Cursor shape requested by a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorHint {
    /// Normal arrow.
    #[default]
    Arrow,
    /// Precise crosshair for drawing.
    Crosshair,
    /// Move.
    Move,
    /// Over a handle that can be dragged.
    Grab,
}

/// What dragging a handle does; the canvas picks the handle's glyph from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleRole {
    /// The end of a wall, beam, line or grid line (stretches it; connected wall ends follow).
    End,
    /// A vertex of an outline or path.
    Vertex,
    /// The midpoint of an outline edge: dragging inserts a new vertex there.
    InsertVertex,
    /// Moves the element (a column, object, text, level mark, stair, dimension line, opening along its wall, or
    /// a wall sideways).
    Move,
    /// A jamb of a door, window or void: changes the width, keeping the other jamb in place.
    Width,
}

/// An on-canvas handle of a selected element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handle {
    /// Position in model coordinates.
    pub at: Point2,
    /// What dragging it does.
    pub role: HandleRole,
    /// The element it edits.
    pub element: Id,
    /// Under the pointer (draw it highlighted).
    pub hot: bool,
}

/// A text box the canvas shows while the text tool collects text. Typed characters arrive as
/// [`ToolEvent::Text`]; Enter commits, Shift+Enter starts a new line, Escape cancels.
#[derive(Debug, Clone, PartialEq)]
pub struct TextEntry {
    /// Anchor of the first line's baseline, model coordinates.
    pub at: Point2,
    /// Text typed so far (lines separated by `\n`).
    pub text: String,
    /// Text height on paper, millimetres.
    pub size_mm: f64,
    /// Rotation, radians counter-clockwise.
    pub rotation: f64,
}

/// What a tool returns for each event.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolOutput {
    /// Rubber-band and ghost geometry in model metres, at the plan's scale (text and symbols sized for it).
    pub preview: Drawing,
    /// Commands to run now. Several commands that form one operation arrive as a single labelled `batch`, so
    /// each operation is one undo step.
    pub commands: Vec<AnyCommand>,
    /// A new selection, when the tool changes it.
    pub selection: Option<BTreeSet<Id>>,
    /// The snap used for the current point.
    pub snap: Option<SnapResult>,
    /// Hint for the status bar.
    pub prompt: String,
    /// Typed-input fields.
    pub fields: Vec<InputField>,
    /// The tool finished an operation and is idle again.
    pub finished: bool,
    /// Cursor shape.
    pub cursor: CursorHint,
    /// Handles of the selection (select tool).
    pub handles: Vec<Handle>,
    /// Elements to pre-highlight (the element under the pointer, or what a click would act on).
    pub highlight: Vec<Id>,
    /// A text box to show (text tool).
    pub text_entry: Option<TextEntry>,
    /// Why the previewed operation cannot be done as it stands (e.g. an opening wider than its wall). The
    /// preview is drawn in the warning colour and a click does nothing until the problem goes away.
    pub warning: Option<String>,
    /// Recently hovered points the pointer aligns with (draw a small cross on each).
    pub tracked: Vec<Point2>,
    /// The tool takes typed characters now: route character keys to [`ToolEvent::Text`] instead of shortcuts.
    pub typing: bool,
}

/// Everything a tool may read.
pub struct ToolContext<'a> {
    /// The document.
    pub doc: &'a Document,
    /// Active storey.
    pub storey: Id,
    /// Current selection.
    pub selection: &'a BTreeSet<Id>,
    /// Size of one screen pixel in metres (for snap radii and handle sizes).
    pub pixel_size: f64,
    /// Snap settings.
    pub snap: &'a SnapSettings,
    /// Tool parameters (from the context bar).
    pub settings: &'a ToolSettings,
    /// Language of prompts and labels.
    pub locale: Locale,
}

/// An interactive tool.
pub trait Tool {
    /// Which tool this is.
    fn id(&self) -> ToolId;
    /// Handles one event.
    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput;
    /// Abandons the current operation.
    fn cancel(&mut self);
    /// Whether the tool is between operations (no points placed).
    fn is_idle(&self) -> bool;
}

/// Creates a tool. Tools not implemented yet fall back to selection.
pub fn make_tool(id: ToolId) -> Box<dyn Tool> {
    match id {
        ToolId::Select => Box::new(SelectTool::default()),
        ToolId::Wall => Box::new(WallTool::default()),
        ToolId::Slab | ToolId::Roof | ToolId::Space | ToolId::Fill | ToolId::Plot => {
            Box::new(OutlineTool::new(id))
        }
        ToolId::Measure => Box::new(MeasureTool::default()),
        _ => Box::new(SelectTool::default()),
    }
}

/// Elements on a storey under a point, nearest first. `radius` is the pick tolerance in metres.
///
/// Small things win over the spaces, slabs and roofs they sit in; openings win over their walls. Uses the shared
/// spatial index, so it is cheap enough to call on every pointer move.
pub fn hit_test(doc: &Document, storey: Id, at: Point2, radius: f64) -> Vec<Id> {
    let index = Snapper::cached(doc, storey);
    let tol = doc.tolerance();
    let mut hits: Vec<(f64, u8, Id)> = Vec::new();
    for id in index.elements_near(at, radius) {
        let Some(e) = doc.element(id) else { continue };
        let rank = match e.kind {
            ElementKind::Opening(_) => 0,
            ElementKind::Space(_) => 4,
            ElementKind::Slab(_) | ElementKind::Roof(_) => 3,
            ElementKind::Underlay(_) => 5,
            _ => 1,
        };
        let d = match &e.kind {
            ElementKind::Opening(_) => kanavos_elements::opening_placement(doc, id).ok().and_then(|pl| {
                let v = at - pl.centre;
                let along = v.dot(pl.along).abs() - pl.width.as_m() / 2.0;
                let across = v.dot(pl.across);
                let inside_across = across <= pl.left_face.max(pl.right_face) + radius
                    && across >= pl.left_face.min(pl.right_face) - radius;
                (along <= radius && inside_across).then_some(along.max(0.0))
            }),
            ElementKind::Text(_) | ElementKind::Level(_) | ElementKind::Object(_) => {
                index.element_box(id).filter(|b| b.contains(at)).map(|_| 0.0)
            }
            _ => {
                if index.outline(id).is_some_and(|o| o.contains(at, &tol)) {
                    Some(0.0)
                } else {
                    index.distance_to(id, at).filter(|d| *d <= radius).or_else(|| match &e.kind {
                        ElementKind::Dimension(dim) => dimension_distance(dim, at).filter(|d| *d <= radius),
                        ElementKind::Grid(_) => {
                            index.element_box(id).filter(|b| b.contains(at)).map(|_| radius)
                        }
                        _ => None,
                    })
                }
            }
        };
        if let Some(d) = d {
            hits.push((d, rank, id));
        }
    }
    hits.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.total_cmp(&b.0)).then(a.2.cmp(&b.2)));
    hits.into_iter().map(|(_, _, id)| id).collect()
}

/// Distance from a point to a dimension's line and extension lines.
fn dimension_distance(d: &kanavos_model::Dimension, at: Point2) -> Option<f64> {
    let u = kanavos_geom::Vec2::from_angle(d.direction);
    let proj = |q: Point2| d.line_point + u * (q - d.line_point).dot(u);
    let ts: Vec<f64> = d.points.iter().map(|q| (*q - d.line_point).dot(u)).collect();
    let lo = ts.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    let line = kanavos_geom::Segment2::new(d.line_point + u * lo, d.line_point + u * hi);
    let mut best = line.distance_to(at).as_m();
    for q in &d.points {
        best = best.min(kanavos_geom::Segment2::new(*q, proj(*q)).distance_to(at).as_m());
    }
    Some(best)
}
