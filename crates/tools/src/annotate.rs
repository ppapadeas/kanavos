//! Annotation tools: dimensions, text, level marks, and section and elevation lines
//! (`specs/tools/annotation.md`).

use kanavos_engine::commands::{CreateElement, CreateView};
use kanavos_geom::{Angle, Point2, Vec2};
use kanavos_i18n::tf;
use kanavos_model::{Dimension, ElementKind, Id, LevelMark, TextNote, View, ViewKind};

use crate::common::{Field, Fields, Pointer, base_output, length_text, segment_live, tr, typed_target};
use crate::preview::{self, BLUE};
use crate::settings::DimensionOrientation;
use crate::{Key, MouseButton, TextEntry, Tool, ToolContext, ToolEvent, ToolId, ToolOutput, hit_test};

/// Dimension tool: click the points to measure, then press Enter or double-click and place the dimension line
/// with a click (or a typed offset). Clicking a wall away from its snap points dimensions that wall's face on the
/// clicked side, with the jambs of its openings. The flip key cycles aligned, horizontal and vertical lines.
#[derive(Debug, Clone, Default)]
pub struct DimensionTool {
    pointer: Pointer,
    fields: Fields,
    points: Vec<Point2>,
    placing: bool,
    orientation: Option<DimensionOrientation>,
}

impl DimensionTool {
    fn orientation(&self, ctx: &ToolContext<'_>) -> DimensionOrientation {
        self.orientation.unwrap_or(ctx.settings.dimension.orientation)
    }

    fn direction(&self, ctx: &ToolContext<'_>, pts: &[Point2]) -> Angle {
        match self.orientation(ctx) {
            DimensionOrientation::Horizontal => Angle::ZERO,
            DimensionOrientation::Vertical => Angle::deg(90.0),
            DimensionOrientation::Aligned => match (pts.first(), pts.last()) {
                (Some(a), Some(b)) => (*b - *a).normalized().map(|v| v.angle()).unwrap_or(Angle::ZERO),
                _ => Angle::ZERO,
            },
        }
    }

    fn line_point(&self, ctx: &ToolContext<'_>, pts: &[Point2], cursor: Point2) -> Point2 {
        let Some(first) = pts.first().copied() else { return cursor };
        let u = Vec2::from_angle(self.direction(ctx, pts));
        let n = u.perp();
        match self.fields.length(Field::Offset) {
            Some(off) => {
                let side = if n.dot(cursor - first) >= 0.0 { 1.0 } else { -1.0 };
                // Offset from the measured point farthest towards the pointer's side.
                let base = pts
                    .iter()
                    .copied()
                    .max_by(|a, b| (side * n.dot(*a - first)).total_cmp(&(side * n.dot(*b - first))))
                    .unwrap_or(first);
                base + n * (side * off.as_m())
            }
            None => cursor,
        }
    }

    fn dimension(&self, ctx: &ToolContext<'_>, pts: &[Point2], cursor: Point2) -> Dimension {
        Dimension {
            points: pts.to_vec(),
            line_point: self.line_point(ctx, pts, cursor),
            direction: self.direction(ctx, pts),
        }
    }

    fn reset(&mut self) {
        self.points.clear();
        self.placing = false;
        self.fields.clear();
    }

    /// Points of a wall's face on the clicked side and the jambs of its openings, in order along the wall.
    fn wall_points(ctx: &ToolContext<'_>, wall: Id, click: Point2) -> Option<Vec<Point2>> {
        let w = ctx.doc.element(wall)?.as_wall()?;
        let layout = kanavos_elements::wall_layout(ctx.doc, wall).ok()?;
        let dir = w.direction()?;
        let left = dir.cross(click - w.start) >= 0.0;
        let face = if left { &layout.left_face } else { &layout.right_face };
        let mut pts = vec![*face.first()?, *face.last()?];
        if w.bulge.abs() < 1e-9 {
            let (l, r) = w.face_offsets(layout.thickness);
            let off = if left { l } else { r };
            for (_, o) in ctx.doc.openings_in(wall) {
                let c = w.start + dir * o.offset.as_m() + dir.perp() * off;
                let h = o.width.as_m() / 2.0;
                pts.push(c - dir * h);
                pts.push(c + dir * h);
            }
        }
        pts.sort_by(|a, b| (*a - w.start).dot(dir).total_cmp(&(*b - w.start).dot(dir)));
        pts.dedup_by(|a, b| a.coincides(*b, &ctx.doc.tolerance()));
        Some(pts)
    }

    fn commit(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput, cursor: Point2) {
        let d = self.dimension(ctx, &self.points, cursor);
        out.commands.push(CreateElement::on(ctx.storey, ElementKind::Dimension(d)).into());
        out.finished = true;
        self.reset();
    }
}

impl Tool for DimensionTool {
    fn id(&self) -> ToolId {
        ToolId::Dimension
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        self.fields.set(if self.placing { &[Field::Offset] } else { &[] });
        let mut out = base_output(&self.pointer);
        let tol = ctx.doc.tolerance();
        match event {
            ToolEvent::Move { at, mods } => {
                let from = if self.placing { None } else { self.points.last().copied() };
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let from = if self.placing { None } else { self.points.last().copied() };
                let s = self.pointer.update(ctx, *at, *mods, from);
                if self.placing {
                    self.commit(ctx, &mut out, s.point);
                } else if self.points.is_empty() && !s.hint.is_point() {
                    let r = ctx.snap.radius_px.max(1.0) * ctx.pixel_size;
                    let wall = hit_test(ctx.doc, ctx.storey, *at, r)
                        .into_iter()
                        .find(|id| ctx.doc.element(*id).and_then(|e| e.as_wall()).is_some());
                    match wall.and_then(|w| Self::wall_points(ctx, w, *at)) {
                        Some(pts) if pts.len() >= 2 => {
                            self.points = pts;
                            self.placing = true;
                        }
                        _ => self.points.push(s.point),
                    }
                } else if self.points.last().is_some_and(|l| l.coincides(s.point, &tol)) {
                    if self.points.len() >= 2 {
                        self.placing = true;
                    }
                } else {
                    self.points.push(s.point);
                }
            }
            ToolEvent::DoubleClick { .. } => {
                if !self.placing && self.points.len() >= 2 {
                    self.placing = true;
                }
            }
            ToolEvent::Text(s) => {
                if self.placing {
                    self.fields.type_text(s);
                }
            }
            ToolEvent::Key { key, .. } => match key {
                Key::Flip => {
                    self.orientation = Some(match self.orientation(ctx) {
                        DimensionOrientation::Aligned => DimensionOrientation::Horizontal,
                        DimensionOrientation::Horizontal => DimensionOrientation::Vertical,
                        DimensionOrientation::Vertical => DimensionOrientation::Aligned,
                    });
                }
                Key::Enter => {
                    if self.placing {
                        if let Some(c) = self.pointer.point() {
                            self.commit(ctx, &mut out, c);
                        }
                    } else if self.points.len() >= 2 {
                        self.placing = true;
                    }
                }
                Key::Backspace => {
                    if !self.fields.backspace() {
                        if self.placing {
                            self.placing = false;
                        } else {
                            self.points.pop();
                        }
                    }
                }
                Key::Escape => {
                    if !self.fields.is_empty() {
                        self.fields.clear();
                    } else {
                        self.reset();
                        out.finished = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.fields.set(if self.placing { &[Field::Offset] } else { &[] });
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        out.prompt = tr(
            ctx,
            match (self.placing, self.points.len()) {
                (true, _) => "tools.dimension.place",
                (false, 0) => "tools.dimension.start",
                (false, 1) => "tools.dimension.next",
                _ => "tools.dimension.next_or_place",
            },
        );
        let mut d = preview::drawing(ctx);
        for p in &self.points {
            preview::mark(&mut d, *p);
        }
        if let Some(c) = self.pointer.point() {
            let mut pts = self.points.clone();
            let line_at = if self.placing {
                c
            } else {
                if pts.last().is_none_or(|l| !l.coincides(c, &tol)) {
                    pts.push(c);
                }
                pts.first().copied().unwrap_or(c)
            };
            if pts.len() >= 2 {
                let dim = self.dimension(ctx, &pts, line_at);
                let e = preview::new_element(ctx, 0, Some(ctx.storey), ElementKind::Dimension(dim.clone()));
                d.extend(preview::ghost(ctx, &[e], &[], &[]).drawing);
                if self.placing {
                    let u = Vec2::from_angle(dim.direction);
                    let first = pts.first().copied().unwrap_or(c);
                    let off = (dim.line_point - first).dot(u.perp()).abs();
                    out.fields = self
                        .fields
                        .view(&[length_text(kanavos_units::Length::m(off), ctx.locale)], ctx.locale);
                }
            }
        }
        out.preview = d;
        out.typing = self.placing;
        out
    }

    fn cancel(&mut self) {
        self.reset();
    }

    fn is_idle(&self) -> bool {
        self.points.is_empty()
    }
}

/// Text tool: click to open a text box, type, and press Enter (Shift+Enter for a new line). Clicking elsewhere
/// commits the current text and opens a new box there; Escape discards it.
#[derive(Debug, Clone, Default)]
pub struct TextTool {
    pointer: Pointer,
    entry: Option<(Point2, String)>,
}

impl TextTool {
    fn note(ctx: &ToolContext<'_>, at: Point2, text: &str) -> TextNote {
        let s = &ctx.settings.text;
        TextNote {
            position: at,
            content: text.to_owned(),
            size_mm: s.size_mm,
            rotation: Angle::ZERO,
            align: s.align,
            bold: s.bold,
        }
    }

    fn commit(&mut self, ctx: &ToolContext<'_>, out: &mut ToolOutput) {
        if let Some((at, text)) = self.entry.take()
            && !text.trim().is_empty()
        {
            let n = Self::note(ctx, at, text.trim_end());
            out.commands.push(CreateElement::on(ctx.storey, ElementKind::Text(n)).into());
            out.finished = true;
        }
    }
}

impl Tool for TextTool {
    fn id(&self) -> ToolId {
        ToolId::Text
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let mut out = base_output(&self.pointer);
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, None);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let s = self.pointer.update(ctx, *at, *mods, None);
                self.commit(ctx, &mut out);
                self.entry = Some((s.point, String::new()));
            }
            ToolEvent::Text(s) => {
                if let Some((_, text)) = &mut self.entry {
                    text.push_str(s);
                }
            }
            ToolEvent::Key { key, mods } => match key {
                Key::Enter if mods.shift => {
                    if let Some((_, text)) = &mut self.entry {
                        text.push('\n');
                    }
                }
                Key::Enter => self.commit(ctx, &mut out),
                Key::Backspace => {
                    if let Some((_, text)) = &mut self.entry {
                        text.pop();
                    }
                }
                Key::Escape if self.entry.take().is_none() => out.finished = true,
                _ => {}
            },
            _ => {}
        }
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        out.prompt = tr(ctx, if self.entry.is_some() { "tools.text.type" } else { "tools.text.place" });
        if let Some((at, text)) = &self.entry {
            out.text_entry = Some(TextEntry {
                at: *at,
                text: text.clone(),
                size_mm: ctx.settings.text.size_mm,
                rotation: 0.0,
            });
            out.typing = true;
            if !text.trim().is_empty() {
                let e = preview::new_element(
                    ctx,
                    0,
                    Some(ctx.storey),
                    ElementKind::Text(Self::note(ctx, *at, text)),
                );
                out.preview = preview::ghost(ctx, &[e], &[], &[]).drawing;
            }
        }
        out
    }

    fn cancel(&mut self) {
        self.entry = None;
    }

    fn is_idle(&self) -> bool {
        self.entry.is_none()
    }
}

/// Level mark tool: each click places a level mark showing the storey's floor level, or a typed value.
#[derive(Debug, Clone, Default)]
pub struct LevelTool {
    pointer: Pointer,
    fields: Fields,
}

impl LevelTool {
    fn mark(&self, ctx: &ToolContext<'_>, at: Point2) -> LevelMark {
        LevelMark {
            position: at,
            value: self.fields.length(Field::Level),
            label: ctx.settings.level.label.clone(),
        }
    }
}

impl Tool for LevelTool {
    fn id(&self) -> ToolId {
        ToolId::Level
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        self.fields.set(&[Field::Level]);
        let mut out = base_output(&self.pointer);
        let mut place = None;
        match event {
            ToolEvent::Move { at, mods } => {
                self.pointer.update(ctx, *at, *mods, None);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                place = Some(self.pointer.update(ctx, *at, *mods, None).point);
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Backspace => {
                    self.fields.backspace();
                }
                Key::Enter => place = self.pointer.point(),
                Key::Escape => {
                    if self.fields.is_empty() {
                        out.finished = true;
                    } else {
                        self.fields.clear();
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if let Some(at) = place {
            out.commands.push(CreateElement::on(ctx.storey, ElementKind::Level(self.mark(ctx, at))).into());
            out.finished = true;
        }
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        out.prompt = tr(ctx, "tools.level.place");
        if let Some(c) = self.pointer.point() {
            let e = preview::new_element(ctx, 0, Some(ctx.storey), ElementKind::Level(self.mark(ctx, c)));
            out.preview = preview::ghost(ctx, &[e], &[], &[]).drawing;
        }
        let floor = ctx.doc.storey(ctx.storey).map(|s| s.elevation).unwrap_or_default();
        out.fields = self.fields.view(&[kanavos_units::format_level(floor, ctx.locale)], ctx.locale);
        out.typing = true;
        out
    }

    fn cancel(&mut self) {
        self.fields.clear();
    }

    fn is_idle(&self) -> bool {
        true
    }
}

/// Greek capital letters used for section and elevation markers.
const MARKERS: [&str; 24] = [
    "Α", "Β", "Γ", "Δ", "Ε", "Ζ", "Η", "Θ", "Ι", "Κ", "Λ", "Μ", "Ν", "Ξ", "Ο", "Π", "Ρ", "Σ", "Τ", "Υ", "Φ",
    "Χ", "Ψ", "Ω",
];

/// The first marker letter not used by any section or elevation: Α, Β, Γ…, then Α1, Β1….
pub(crate) fn next_marker(doc: &kanavos_model::Document) -> String {
    let used: Vec<String> = doc
        .views()
        .filter_map(|v| match &v.kind {
            ViewKind::Section { marker, .. } | ViewKind::Elevation { marker, .. } => Some(marker.clone()),
            _ => None,
        })
        .collect();
    for round in 0..100u32 {
        for m in MARKERS {
            let candidate = if round == 0 { m.to_owned() } else { format!("{m}{round}") };
            if !used.contains(&candidate) {
                return candidate;
            }
        }
    }
    "Α".to_owned()
}

/// Section and elevation tool: click the two ends of the line, then click on the side the view looks to.
#[derive(Debug, Clone)]
pub struct MarkerTool {
    section: bool,
    pointer: Pointer,
    fields: Fields,
    points: Vec<Point2>,
}

impl MarkerTool {
    /// A section or elevation tool.
    pub fn new(id: ToolId) -> MarkerTool {
        MarkerTool {
            section: id == ToolId::Section,
            pointer: Pointer::default(),
            fields: Fields::default(),
            points: Vec::new(),
        }
    }

    fn view(&self, ctx: &ToolContext<'_>, start: Point2, end: Point2, side: Point2) -> CreateView {
        let flip = (end - start).cross(side - start) < 0.0;
        let marker = next_marker(ctx.doc);
        let s = &ctx.settings.section;
        let (kind, name) = if self.section {
            (
                ViewKind::Section { start, end, depth: s.depth, flip, marker: marker.clone() },
                tf(ctx.locale, "tools.section.name", &[("m", &marker)]),
            )
        } else {
            (
                ViewKind::Elevation { start, end, depth: s.depth, flip, marker: marker.clone() },
                tf(ctx.locale, "tools.elevation.name", &[("m", &marker)]),
            )
        };
        CreateView { name, kind, scale: s.scale.max(1) }
    }

    fn target(&self, ctx: &ToolContext<'_>) -> Option<Point2> {
        let from = self.points.last().copied();
        if self.points.len() >= 2 {
            return self.pointer.point();
        }
        typed_target(&self.fields, from, self.pointer.point(), ctx.doc).or(self.pointer.point())
    }
}

impl Tool for MarkerTool {
    fn id(&self) -> ToolId {
        if self.section { ToolId::Section } else { ToolId::Elevation }
    }

    fn handle(&mut self, ctx: &ToolContext<'_>, event: &ToolEvent) -> ToolOutput {
        let kinds: &[Field] = match self.points.len() {
            0 => &[Field::Point],
            1 => &[Field::Length, Field::Angle],
            _ => &[],
        };
        self.fields.set(kinds);
        let mut out = base_output(&self.pointer);
        match event {
            ToolEvent::Move { at, mods } => {
                let from = self.points.last().copied().filter(|_| self.points.len() < 2);
                self.pointer.update(ctx, *at, *mods, from);
            }
            ToolEvent::Press { at, button: MouseButton::Primary, mods } => {
                let from = self.points.last().copied().filter(|_| self.points.len() < 2);
                self.pointer.update(ctx, *at, *mods, from);
                if let Some(p) = self.target(ctx) {
                    match self.points.as_slice() {
                        [a, b] => {
                            let v = self.view(ctx, *a, *b, p);
                            out.commands.push(v.into());
                            out.finished = true;
                            self.points.clear();
                        }
                        [a] if a.coincides(p, &ctx.doc.tolerance()) => {}
                        _ => self.points.push(p),
                    }
                }
                self.fields.clear();
            }
            ToolEvent::Text(s) => self.fields.type_text(s),
            ToolEvent::Key { key, .. } => match key {
                Key::Tab => self.fields.tab(),
                Key::Backspace => {
                    if !self.fields.backspace() {
                        self.points.pop();
                    }
                }
                Key::Enter => {
                    if !self.fields.is_empty()
                        && let Some(p) = self.target(ctx)
                        && self.points.len() < 2
                    {
                        self.points.push(p);
                    }
                    self.fields.clear();
                }
                Key::Escape => {
                    if !self.fields.is_empty() {
                        self.fields.clear();
                    } else {
                        self.points.clear();
                        out.finished = true;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        let kinds: &[Field] = match self.points.len() {
            0 => &[Field::Point],
            1 => &[Field::Length, Field::Angle],
            _ => &[],
        };
        self.fields.set(kinds);
        out.snap = self.pointer.snap.clone();
        out.tracked = self.pointer.tracked();
        let key = match (self.section, self.points.len()) {
            (true, 0) => "tools.section.start",
            (false, 0) => "tools.elevation.start",
            (_, 1) => "tools.marker.end",
            _ => "tools.marker.side",
        };
        out.prompt = tf(ctx.locale, key, &[("m", &next_marker(ctx.doc))]);
        let mut d = preview::drawing(ctx);
        let mut live = Vec::new();
        if let Some(c) = self.target(ctx) {
            match self.points.as_slice() {
                [a] => {
                    preview::line(&mut d, *a, c, BLUE);
                    preview::length_label(&mut d, *a, c, ctx.locale);
                    live = segment_live(*a, c, ctx.locale);
                    live.truncate(2);
                }
                [a, b] => {
                    let v = self.view(ctx, *a, *b, c);
                    let view = View {
                        id: Id(ctx.doc.peek_next_id().0),
                        name: v.name,
                        kind: v.kind,
                        scale: v.scale,
                        hidden_layers: Default::default(),
                    };
                    d.extend(preview::ghost(ctx, &[], &[], &[view]).drawing);
                    preview::line(&mut d, *a, *b, BLUE);
                }
                _ => {}
            }
        }
        out.preview = d;
        out.fields = self.fields.view(&live, ctx.locale);
        out.typing = self.points.len() < 2;
        out
    }

    fn cancel(&mut self) {
        self.points.clear();
        self.fields.clear();
    }

    fn is_idle(&self) -> bool {
        self.points.is_empty()
    }
}
