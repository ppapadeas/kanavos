//! Commands that derive elements from walls or from a point: slabs, roofs, spaces, dimension chains, openings.

use std::collections::BTreeMap;

use kanavos_geom::{Angle, Point2, Polygon2, PolygonWithHoles, Segment2};
use kanavos_model::{
    Category, Document, ElementKind, Id, Opening, OpeningKind, Parapet, Phase, Roof, RoofShape, Slab, Space,
    SpaceCategory, Structure,
};
use kanavos_units::{Length, Locale};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::CreateElement;
use crate::geo::{self, DimensionOptions, OutlineFace};
use crate::labels::new_element;
use crate::{Command, CommandError, Outcome};

fn yes() -> bool {
    true
}

fn one_metre() -> Length {
    Length::m(1.0)
}

fn spacing() -> Length {
    Length::m(0.7)
}

fn core_face() -> OutlineFace {
    OutlineFace::Core
}

fn default_pitch() -> f64 {
    25.0
}

fn default_overhang() -> Length {
    Length::m(0.5)
}

fn invalid(field: &'static str, reason: impl Into<String>) -> CommandError {
    CommandError::InvalidArgument { field, reason: reason.into() }
}

/// Applies element creations in order, collecting the created ids.
fn create_all(doc: &Document, items: Vec<CreateElement>) -> Result<(Document, Outcome), CommandError> {
    let mut current = doc.clone();
    let mut out = Outcome::default();
    for c in items {
        let (next, o) = c.apply(&current)?;
        current = next;
        out.created.extend(o.created);
    }
    Ok((current, out))
}

fn storey_exists(doc: &Document, storey: Id) -> Result<(), CommandError> {
    doc.storey(storey).map(|_| ()).ok_or(CommandError::NotFound(storey))
}

fn build_up_or(
    doc: &Document,
    given: Option<Id>,
    default: Option<Id>,
    what: &str,
) -> Result<Id, CommandError> {
    let id = given
        .or(default)
        .ok_or_else(|| invalid("build_up", format!("no default {what} build-up; give one")))?;
    doc.build_up(id).ok_or(CommandError::NotFound(id))?;
    Ok(id)
}

// ----- slab.from_walls -----------------------------------------------------------------------------------------

/// `slab.from_walls` — adds a slab on a storey whose outline follows the outside of the storey's walls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SlabFromWalls {
    /// Storey whose walls give the outline; the slab is placed on it.
    pub storey: Id,
    /// Wall face the outline follows (default `core`: the slab edge sits behind the external insulation).
    #[serde(default = "core_face")]
    pub face: OutlineFace,
    /// Build-up; default: the ground-slab default on the lowest storey, the floor-slab default elsewhere.
    #[serde(default)]
    pub build_up: Option<Id>,
    /// Top surface relative to the storey's floor (default 0).
    #[serde(default)]
    pub top_offset: Length,
    /// Openings through the slab (stair wells, shafts), inside the outline.
    #[serde(default)]
    pub holes: Vec<Polygon2>,
    /// User mark.
    #[serde(default)]
    pub tag: String,
}

impl Command for SlabFromWalls {
    fn name(&self) -> &'static str {
        "slab.from_walls"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        storey_exists(doc, self.storey)?;
        let outer = geo::outline_from_walls(doc, self.storey, self.face)?;
        let lowest = doc.storeys().first().map(|s| s.id) == Some(self.storey);
        let d = doc.defaults();
        let default = if lowest { d.ground_slab.or(d.floor_slab) } else { d.floor_slab };
        let build_up = build_up_or(doc, self.build_up, default, "slab")?;
        let tol = doc.tolerance();
        let mut holes = Vec::with_capacity(self.holes.len());
        for h in &self.holes {
            if !h.is_valid(&tol) {
                return Err(invalid("holes", "each hole needs at least three points enclosing an area"));
            }
            if !h.vertices.iter().any(|p| outer.contains(*p, &tol)) {
                return Err(invalid("holes", "must lie inside the slab outline"));
            }
            holes.push(PolygonWithHoles::from(h.clone()));
        }
        // Holes are subtracted, so a stair well that reaches a wall face becomes a notch in the outline.
        let region = if holes.is_empty() {
            PolygonWithHoles::from(outer)
        } else {
            kanavos_geom::difference(&[PolygonWithHoles::from(outer)], &holes)
                .map_err(|e| CommandError::Geometry(e.to_string()))?
                .into_iter()
                .max_by(|a, b| a.area().total_cmp(&b.area()))
                .ok_or_else(|| invalid("holes", "leave no slab"))?
        };
        let mut c = CreateElement::on(
            self.storey,
            ElementKind::Slab(Slab {
                outline: region,
                structure: Structure::Layered { build_up },
                top_offset: self.top_offset,
            }),
        );
        c.tag = self.tag.clone();
        c.apply(doc)
    }
}

// ----- roof.from_walls -----------------------------------------------------------------------------------------

/// Shape of a roof made by `roof.from_walls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoofKind {
    /// Every eaves edge slopes (τετράριχτη).
    #[default]
    Hip,
    /// The edges parallel to the longest edge slope; the others are gables (δίριχτη).
    Gable,
    /// Only the longest edge (the southernmost of equals) slopes (μονόριχτη).
    Mono,
    /// A flat roof (δώμα).
    Flat,
}

/// `roof.from_walls` — adds a roof over the walls of a storey: the outer faces plus an overhang, flat or pitched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoofFromWalls {
    /// Storey whose walls the roof covers; the roof is placed on it.
    pub storey: Id,
    /// Shape (default hip).
    #[serde(default)]
    pub kind: RoofKind,
    /// Pitch of sloped edges in degrees (default 25°).
    #[serde(default = "default_pitch")]
    pub pitch: f64,
    /// Overhang beyond the outer wall faces (default 0.50 m; 0 for none).
    #[serde(default = "default_overhang")]
    pub overhang: Length,
    /// Build-up; default: the pitched-roof or flat-roof default.
    #[serde(default)]
    pub build_up: Option<Id>,
    /// Height of the roof's top surface at the eaves above the storey's floor; default: resting on the tallest
    /// wall of the storey.
    #[serde(default)]
    pub base: Option<Length>,
    /// Parapet of a flat roof (none by default).
    #[serde(default)]
    pub parapet: Option<Parapet>,
    /// Openings through the roof (light wells, skylights, chimneys, a courtyard), each a polygon inside the roof
    /// outline that does not overlap the others (none by default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Polygon2>,
    /// User mark.
    #[serde(default)]
    pub tag: String,
}

/// Which outline edges slope for a roof kind.
fn slopes(outline: &Polygon2, kind: RoofKind, pitch: Angle) -> Vec<Option<Angle>> {
    let edges: Vec<Segment2> = outline.edges().collect();
    let longest = edges.iter().map(|e| e.length().as_m()).fold(0.0, f64::max);
    let main_dir = edges.iter().find(|e| e.length().as_m() >= longest - 1e-9).and_then(Segment2::direction);
    match kind {
        RoofKind::Hip => vec![Some(pitch); edges.len()],
        RoofKind::Flat => Vec::new(),
        RoofKind::Gable => edges
            .iter()
            .map(|e| match (e.direction(), main_dir) {
                (Some(d), Some(m)) if d.dot(m).abs() > 0.99 => Some(pitch),
                _ => None,
            })
            .collect(),
        RoofKind::Mono => {
            let pick = edges
                .iter()
                .enumerate()
                .filter(|(_, e)| e.length().as_m() >= longest * 0.99)
                .min_by(|(_, a), (_, b)| a.midpoint().y.total_cmp(&b.midpoint().y))
                .map(|(i, _)| i);
            (0..edges.len()).map(|i| if Some(i) == pick { Some(pitch) } else { None }).collect()
        }
    }
}

impl Command for RoofFromWalls {
    fn name(&self) -> &'static str {
        "roof.from_walls"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        storey_exists(doc, self.storey)?;
        let pitched = self.kind != RoofKind::Flat;
        if pitched && !(self.pitch > 0.0 && self.pitch < 89.0) {
            return Err(invalid("pitch", "must be between 0° and 89°"));
        }
        if !(self.overhang.as_m() >= 0.0 && self.overhang.is_finite()) {
            return Err(invalid("overhang", "must be zero or more"));
        }
        let walls = geo::outline_from_walls(doc, self.storey, OutlineFace::Outer)?;
        let outline = if doc.tolerance().is_zero(self.overhang) {
            walls
        } else {
            geo::offset_polygon(&walls, self.overhang.as_m())
                .ok_or_else(|| CommandError::Geometry("the overhang outline is degenerate".into()))?
        };
        let d = doc.defaults();
        let default = if pitched { d.pitched_roof.or(d.flat_roof) } else { d.flat_roof.or(d.pitched_roof) };
        let build_up = build_up_or(doc, self.build_up, default, "roof")?;
        let structure = Structure::Layered { build_up };
        let t = doc.structure_thickness(&structure).unwrap_or(Length::ZERO).as_m();
        let pitch = Angle::deg(self.pitch);
        let wall_top = doc
            .walls_on(self.storey)
            .map(|(_, w)| (w.base_offset + w.height).as_m())
            .fold(f64::MIN, f64::max)
            .max(0.0);
        let base = match self.base {
            Some(b) => b,
            None if pitched => Length::m(wall_top + t / pitch.cos() - self.overhang.as_m() * pitch.tan()),
            None => Length::m(wall_top + t),
        };
        let shape = if pitched {
            RoofShape::Pitched { slopes: slopes(&outline, self.kind, pitch) }
        } else {
            RoofShape::Flat { parapet: self.parapet }
        };
        let holes = self.holes.clone();
        let mut c = CreateElement::on(
            self.storey,
            ElementKind::Roof(Roof { outline, holes, base, structure, shape }),
        );
        c.tag = self.tag.clone();
        c.apply(doc)
    }
}

// ----- railing.along_stair ------------------------------------------------------------------------------------

/// A side of a stair, looking up its flights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StairSide {
    /// The left side.
    #[default]
    Left,
    /// The right side.
    Right,
}

fn guard_height() -> Length {
    Length::m(1.0)
}

fn guard_inset() -> Length {
    Length::cm(5.0)
}

fn post_spacing() -> Length {
    Length::m(1.2)
}

/// `railing.along_stair` — adds a railing along one side of every flight of a stair. Each railing follows the line
/// of the flight's tread nosings, from the first nosing up to the level of the landing or floor the flight
/// arrives at, so its height is measured from the nosings. Landings are not railed; draw them with the railing
/// tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RailingAlongStair {
    /// The stair.
    pub stair: Id,
    /// Which side, looking up the flights (default left).
    #[serde(default)]
    pub side: StairSide,
    /// Height of the top rail above the nosing line (default 1.00 m).
    #[serde(default = "guard_height")]
    pub height: Length,
    /// Distance of the railing's line from the flight's edge, inwards (default 5 cm).
    #[serde(default = "guard_inset")]
    pub inset: Length,
    /// Infill (default bars).
    #[serde(default)]
    pub infill: kanavos_model::RailingInfill,
    /// Material of posts and rails; default: the railing default, else the stair's material.
    #[serde(default)]
    pub material: Option<Id>,
    /// Material of the infill; default: the frame's.
    #[serde(default)]
    pub infill_material: Option<Id>,
    /// Largest distance between posts (default 1.20 m).
    #[serde(default = "post_spacing")]
    pub post_spacing: Length,
    /// User mark.
    #[serde(default)]
    pub tag: String,
}

impl Command for RailingAlongStair {
    fn name(&self) -> &'static str {
        "railing.along_stair"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let e = doc.element(self.stair).ok_or(CommandError::NotFound(self.stair))?;
        let ElementKind::Stair(st) = &e.kind else { return Err(invalid("stair", "is not a stair")) };
        let storey = e.storey.ok_or_else(|| invalid("stair", "is not on a storey"))?;
        let floor = doc.storey(storey).map(|s| s.elevation.as_m()).unwrap_or(0.0);
        let half = st.width.as_m() / 2.0;
        if !(self.inset.as_m() >= 0.0 && self.inset.as_m() < half) {
            return Err(invalid("inset", "must be at least zero and less than half the stair's width"));
        }
        let layout = kanavos_elements::stair_layout(doc, self.stair).map_err(super::geometry)?;
        let (r, g) = (layout.riser_height.as_m(), st.going.as_m());
        let material = self.material.or(doc.defaults().railing_material).unwrap_or(st.material);
        let mut next = doc.clone();
        let mut out = Outcome::default();
        for f in layout.flights.iter().filter(|f| !f.treads.is_empty()) {
            let n = f.treads.len() as f64;
            let left = f.direction.perp();
            let across = match self.side {
                StairSide::Left => left,
                StairSide::Right => -left,
            } * (half - self.inset.as_m());
            let start = f.start + across;
            let end = f.start + f.direction * (g * n) + across;
            // The nosing line: from the first nosing (one riser up) to the level the flight arrives at.
            let z0 = f.start_elevation - floor + r;
            let z1 = z0 + n * r;
            let railing = kanavos_model::Railing {
                path: vec![start, end],
                height: self.height,
                base_offset: Length::m(z0),
                elevations: vec![Length::m(z0), Length::m(z1)],
                post_spacing: self.post_spacing,
                infill: self.infill,
                material,
                infill_material: self.infill_material,
            };
            let mut c = CreateElement::on(storey, ElementKind::Railing(railing));
            c.tag = self.tag.clone();
            let (d, o) = c.apply(&next)?;
            next = d;
            out.created.extend(o.created);
        }
        if out.created.is_empty() {
            return Err(invalid("stair", "has no flight with treads to follow"));
        }
        Ok((next, out))
    }
}

// ----- space.detect_all ----------------------------------------------------------------------------------------

/// `space.detect_all` — creates a space in every room closed by walls on a storey.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DetectAllSpaces {
    /// Storey.
    pub storey: Id,
    /// Category of the new spaces (default main use).
    #[serde(default)]
    pub category: SpaceCategory,
    /// Name of the new spaces; default "Χώρος n" / "Space n".
    #[serde(default)]
    pub name: Option<String>,
    /// Leave rooms that already contain a space alone (default true).
    #[serde(default = "yes")]
    pub skip_existing: bool,
    /// Number the new spaces by storey, e.g. "1.03" on the first floor (default true).
    #[serde(default = "yes")]
    pub numbered: bool,
    /// Language of default names.
    #[serde(default)]
    pub locale: Locale,
}

/// Storey level index for space numbers: 0 for the lowest above-ground storey, negative below ground.
fn level_index(doc: &Document, storey: Id) -> i64 {
    let all = doc.storeys();
    let below = all.iter().filter(|s| s.below_ground).count() as i64;
    all.iter().position(|s| s.id == storey).map(|i| i as i64 - below).unwrap_or(0)
}

impl Command for DetectAllSpaces {
    fn name(&self) -> &'static str {
        "space.detect_all"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        storey_exists(doc, self.storey)?;
        let tol = doc.tolerance();
        let existing: Vec<PolygonWithHoles> =
            doc.elements_on(self.storey).filter_map(|e| e.as_space().map(|s| s.boundary.clone())).collect();
        let mut rooms: Vec<(Point2, PolygonWithHoles)> = geo::detect_all_spaces(doc, self.storey)?
            .into_iter()
            .filter_map(|b| geo::interior_point(&b.outer).map(|p| (p, b)))
            .filter(|(p, _)| !self.skip_existing || !existing.iter().any(|e| e.contains(*p, &tol)))
            .collect();
        if rooms.is_empty() {
            return Err(invalid("storey", "has no closed room without a space"));
        }
        // North to south, then west to east, in half-metre rows.
        rooms.sort_by(|(a, _), (b, _)| {
            let row = |p: &Point2| (-p.y.as_m() * 2.0).round();
            row(a).total_cmp(&row(b)).then(a.x.total_cmp(&b.x))
        });
        let level = level_index(doc, self.storey);
        let first = existing.len() + 1;
        let items = rooms
            .into_iter()
            .enumerate()
            .map(|(i, (stamp, boundary))| {
                let n = first + i;
                let name = match &self.name {
                    Some(name) => name.clone(),
                    None => {
                        kanavos_i18n::tf(self.locale, "engine.space.default_name", &[("n", &n.to_string())])
                    }
                };
                let number = if self.numbered { format!("{level}.{n:02}") } else { String::new() };
                CreateElement::on(
                    self.storey,
                    ElementKind::Space(Space {
                        boundary,
                        name,
                        number,
                        category: self.category,
                        height: None,
                        stamp: Some(stamp),
                        finish: String::new(),
                    }),
                )
            })
            .collect();
        create_all(doc, items)
    }
}

// ----- dimension.auto ------------------------------------------------------------------------------------------

/// `dimension.auto` — adds exterior dimension chains on the four sides of a storey's walls: corners and the
/// jambs of doors and windows, then the overall size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AutoDimensions {
    /// Storey.
    pub storey: Id,
    /// Distance from the outer wall faces to the first chain (default 1.00 m).
    #[serde(default = "one_metre")]
    pub offset: Length,
    /// Distance between chains (default 0.70 m).
    #[serde(default = "spacing")]
    pub spacing: Length,
    /// Include door and window jambs (default true).
    #[serde(default = "yes")]
    pub openings: bool,
    /// Add the overall-size chain (default true).
    #[serde(default = "yes")]
    pub overall: bool,
}

impl Command for AutoDimensions {
    fn name(&self) -> &'static str {
        "dimension.auto"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        storey_exists(doc, self.storey)?;
        if !(self.offset.as_m() > 0.0 && self.spacing.as_m() > 0.0) {
            return Err(invalid("offset", "offset and spacing must be positive"));
        }
        let opts = DimensionOptions {
            offset: self.offset.as_m(),
            spacing: self.spacing.as_m(),
            openings: self.openings,
            overall: self.overall,
        };
        let dims = geo::exterior_dimensions(doc, self.storey, &opts)?;
        let items =
            dims.into_iter().map(|d| CreateElement::on(self.storey, ElementKind::Dimension(d))).collect();
        create_all(doc, items)
    }
}

// ----- opening.place -------------------------------------------------------------------------------------------

/// `opening.place` — places a door, window or void in the wall nearest to a point; the point gives the opening's
/// centre along the wall. Sizes default to the catalogue style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlaceOpening {
    /// Point on or near the host wall; its projection on the wall's reference line is the opening centre.
    pub point: Point2,
    /// Door, window or void.
    pub kind: OpeningKind,
    /// Host wall; default: the nearest wall on `storey` within `max_distance`.
    #[serde(default)]
    pub host: Option<Id>,
    /// Storey to search for the nearest wall (needed when `host` is not given).
    #[serde(default)]
    pub storey: Option<Id>,
    /// Object style from the catalogue, e.g. `"window.casement_2"`; default: the kind's default style.
    #[serde(default)]
    pub style: Option<String>,
    /// Clear width; default from the style.
    #[serde(default)]
    pub width: Option<Length>,
    /// Clear height; default from the style.
    #[serde(default)]
    pub height: Option<Length>,
    /// Sill above the storey floor; default from the style.
    #[serde(default)]
    pub sill: Option<Length>,
    /// How far from a wall's reference line the point may be (default 1.00 m).
    #[serde(default = "one_metre")]
    pub max_distance: Length,
    /// Mirror the opening across the wall. Unset, a window's outer face is the wall's left face and a door opens
    /// towards its right face, which suits exterior walls drawn clockwise.
    #[serde(default)]
    pub flip_side: bool,
    /// Swap the hinge jamb.
    #[serde(default)]
    pub flip_hand: bool,
    /// Style parameters.
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
    /// User mark, e.g. "Π1" or "Θ2".
    #[serde(default)]
    pub tag: String,
    /// Free user properties.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

fn category_of(kind: OpeningKind) -> Category {
    match kind {
        OpeningKind::Door => Category::Door,
        OpeningKind::Window => Category::Window,
        OpeningKind::Void => Category::Opening,
    }
}

/// Distance from a point to a wall's reference line and the length along it to the nearest point.
fn along_wall(w: &kanavos_model::Wall, p: Point2) -> (f64, f64) {
    match w.arc() {
        Some(arc) => {
            let pts = arc.tessellate(0.001, 16);
            let mut best = (f64::MAX, 0.0);
            let mut run = 0.0;
            for pair in pts.windows(2) {
                if let [a, b] = pair {
                    let s = Segment2::new(*a, *b);
                    let t = s.project_param(p).clamp(0.0, 1.0);
                    let d = s.at(t).distance(p).as_m();
                    if d < best.0 {
                        best = (d, run + t * s.length().as_m());
                    }
                    run += s.length().as_m();
                }
            }
            best
        }
        None => {
            let s = w.segment();
            let t = s.project_param(p).clamp(0.0, 1.0);
            (s.at(t).distance(p).as_m(), t * w.length().as_m())
        }
    }
}

impl Command for PlaceOpening {
    fn name(&self) -> &'static str {
        "opening.place"
    }
    fn apply(&self, doc: &Document) -> Result<(Document, Outcome), CommandError> {
        let style =
            self.style.clone().unwrap_or_else(|| kanavos_objects::default_style(self.kind).to_owned());
        let def = kanavos_objects::find(&style)
            .ok_or_else(|| invalid("style", "is not in the object catalogue"))?;
        let expected = match self.kind {
            OpeningKind::Door => kanavos_objects::ObjectCategory::Door,
            OpeningKind::Window => kanavos_objects::ObjectCategory::Window,
            OpeningKind::Void => kanavos_objects::ObjectCategory::Void,
        };
        if def.category != expected {
            return Err(invalid("style", format!("is not a {:?} style", self.kind).to_lowercase()));
        }
        let (host, wall) = match self.host {
            Some(h) => {
                let e = doc.element(h).ok_or(CommandError::NotFound(h))?;
                let w = e.as_wall().ok_or_else(|| invalid("host", "is not a wall"))?;
                (h, w)
            }
            None => {
                let storey =
                    self.storey.ok_or_else(|| invalid("storey", "is needed when no host is given"))?;
                storey_exists(doc, storey)?;
                doc.walls_on(storey)
                    .map(|(id, w)| (along_wall(w, self.point).0, id, w))
                    .filter(|(d, _, _)| *d <= self.max_distance.as_m())
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, id, w)| (id, w))
                    .ok_or_else(|| invalid("point", "is not near a wall"))?
            }
        };
        let width = self.width.unwrap_or(Length::m(def.width));
        let height = self.height.unwrap_or(Length::m(def.height));
        let sill = self.sill.unwrap_or(Length::m(def.sill));
        let len = wall.length().as_m();
        let half = width.as_m() / 2.0;
        if width.as_m() > len + doc.tolerance().length.as_m() {
            return Err(invalid("width", "is wider than the wall"));
        }
        let (_, at) = along_wall(wall, self.point);
        let offset = Length::m(at.clamp(half, (len - half).max(half)));
        let storey = doc.element(host).and_then(|e| e.storey);
        let c = CreateElement {
            storey,
            view: None,
            kind: ElementKind::Opening(Opening {
                host,
                kind: self.kind,
                style,
                offset,
                width,
                height,
                sill,
                flip_side: self.flip_side,
                flip_hand: self.flip_hand,
                params: self.params.clone(),
            }),
            tag: self.tag.clone(),
            phase: Phase::New,
            properties: self.properties.clone(),
        };
        c.apply(doc)
    }
    fn label(&self, locale: Locale) -> String {
        new_element(locale, category_of(self.kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use kanavos_geom::Vec2;
    use kanavos_model::{ReferenceLine, Wall, starter};

    fn edge_dir(p: &Polygon2, i: usize) -> Option<Vec2> {
        p.edge(i).and_then(|e| e.direction())
    }

    fn house() -> (Session, Id) {
        let mut s = Session::new(starter());
        let st = s.document().storeys()[0].id;
        let bu = s.document().defaults().exterior_wall.unwrap();
        let ring = [(0.0, 0.0), (0.0, 8.0), (12.0, 8.0), (12.0, 0.0)];
        for i in 0..4 {
            let (a, b) = (ring[i], ring[(i + 1) % 4]);
            s.run(CreateElement::on(
                st,
                ElementKind::Wall(Wall {
                    start: Point2::new(a.0, a.1),
                    end: Point2::new(b.0, b.1),
                    bulge: 0.0,
                    reference: ReferenceLine::Left,
                    structure: Structure::Layered { build_up: bu },
                    height: Length::m(3.0),
                    base_offset: Length::ZERO,
                }),
            ))
            .unwrap();
        }
        (s, st)
    }

    #[test]
    fn slab_follows_the_core_faces_with_a_hole() {
        let (mut s, st) = house();
        let hole = Polygon2::from_m(&[(2.0, 2.0), (4.0, 2.0), (4.0, 5.0), (2.0, 5.0)]);
        let out = s
            .run(SlabFromWalls {
                storey: st,
                face: OutlineFace::Core,
                build_up: None,
                top_offset: Length::ZERO,
                holes: vec![hole],
                tag: String::new(),
            })
            .unwrap();
        let ElementKind::Slab(slab) = &s.document().element(out.created[0]).unwrap().kind else { panic!() };
        let expected = (12.0 - 0.18) * (8.0 - 0.18) - 6.0;
        assert!((slab.outline.area().as_m2() - expected).abs() < 1e-6);
        // Lowest storey: ground slab build-up.
        assert_eq!(
            slab.structure,
            Structure::Layered { build_up: s.document().defaults().ground_slab.unwrap() }
        );
    }

    #[test]
    fn hip_roof_overhangs_and_slopes_everywhere() {
        let (mut s, st) = house();
        let out = s
            .run(RoofFromWalls {
                storey: st,
                kind: RoofKind::Hip,
                pitch: 30.0,
                overhang: Length::m(0.5),
                build_up: None,
                base: None,
                parapet: None,
                holes: Vec::new(),
                tag: String::new(),
            })
            .unwrap();
        let ElementKind::Roof(r) = &s.document().element(out.created[0]).unwrap().kind else { panic!() };
        assert!((r.outline.area().as_m2() - 13.0 * 9.0).abs() < 1e-6);
        let RoofShape::Pitched { slopes } = &r.shape else { panic!() };
        assert!(slopes.iter().all(Option::is_some));
        assert!(r.base.as_m() > 3.0 - 0.5 * 0.6 && r.base.as_m() < 3.4);
    }

    #[test]
    fn roofs_from_walls_can_have_holes() {
        let (mut s, st) = house();
        let skylight = Polygon2::from_m(&[(2.0, 2.0), (3.0, 2.0), (3.0, 3.0), (2.0, 3.0)]);
        let cmd = RoofFromWalls {
            storey: st,
            kind: RoofKind::Hip,
            pitch: 30.0,
            overhang: Length::m(0.5),
            build_up: None,
            base: None,
            parapet: None,
            holes: vec![skylight.clone()],
            tag: String::new(),
        };
        // The JSON form takes the holes as point lists.
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["holes"][0].as_array().unwrap().len(), 4);
        let out = s.run(cmd.clone()).unwrap();
        let ElementKind::Roof(r) = &s.document().element(out.created[0]).unwrap().kind else { panic!() };
        assert_eq!(r.holes, vec![skylight]);
        let q = kanavos_elements::quantities(s.document(), out.created[0]).unwrap();
        assert!((q.gross_area.unwrap().as_m2() - (13.0 * 9.0 - 1.0)).abs() < 1e-6);
        // A hole outside the roof is refused, naming the field.
        let outside = Polygon2::from_m(&[(20.0, 2.0), (21.0, 2.0), (21.0, 3.0), (20.0, 3.0)]);
        let err = s.run(RoofFromWalls { holes: vec![outside], ..cmd }).unwrap_err();
        assert_eq!(err.field(), Some("holes"), "{err}");
    }

    #[test]
    fn railings_follow_the_flights_of_a_stair() {
        let (mut s, st) = house();
        let concrete = s.document().defaults().structural_material.unwrap();
        let stair = |shape| {
            ElementKind::Stair(kanavos_model::Stair {
                origin: Point2::new(2.0, 1.0),
                direction: Angle::deg(90.0),
                shape,
                width: Length::m(1.0),
                rise: Length::m(3.0),
                risers: 18,
                going: Length::m(0.28),
                first_flight_risers: None,
                gap: Length::m(0.1),
                thickness: Length::m(0.15),
                material: concrete,
                usage: Default::default(),
                base_offset: Length::ZERO,
            })
        };
        let straight =
            s.run(CreateElement::on(st, stair(kanavos_model::StairShape::Straight))).unwrap().created[0];
        let cmd = RailingAlongStair {
            stair: straight,
            side: StairSide::Right,
            height: Length::m(1.0),
            inset: Length::cm(5.0),
            infill: kanavos_model::RailingInfill::Bars,
            material: None,
            infill_material: None,
            post_spacing: Length::m(1.2),
            tag: "Κ1".into(),
        };
        let out = s.run(cmd.clone()).unwrap();
        assert_eq!(out.created.len(), 1);
        let doc = s.document();
        let e = doc.element(out.created[0]).unwrap();
        assert_eq!(e.tag, "Κ1");
        let ElementKind::Railing(r) = &e.kind else { panic!() };
        // Walking north (+Y), the right side is east: x = 2.0 + 0.45.
        let tol = doc.tolerance();
        assert!(r.path[0].coincides(Point2::new(2.45, 1.0), &tol), "{:?}", r.path);
        assert!(r.path[1].coincides(Point2::new(2.45, 1.0 + 17.0 * 0.28), &tol));
        // From the first nosing (one riser up) to the upper floor.
        assert!(tol.equal(r.elevations[0], Length::m(3.0 / 18.0)));
        assert!(tol.equal(r.elevations[1], Length::m(3.0)));
        assert_eq!(r.material, doc.defaults().railing_material.unwrap());
        assert!(kanavos_elements::railing_checks(doc, out.created[0]).unwrap().is_empty());
        // A half-turn stair gets one railing per flight.
        let half =
            s.run(CreateElement::on(st, stair(kanavos_model::StairShape::HalfTurnLeft))).unwrap().created[0];
        let out = s.run(RailingAlongStair { stair: half, ..cmd.clone() }).unwrap();
        assert_eq!(out.created.len(), 2);
        let ElementKind::Railing(second) = &s.document().element(out.created[1]).unwrap().kind else {
            panic!()
        };
        // The second flight starts at the landing (riser 9 of 18) and ends at the floor above.
        assert!(tol.equal(second.elevations[0], Length::m(10.0 * 3.0 / 18.0)));
        assert!(tol.equal(second.elevations[1], Length::m(3.0)));
        // Errors name their argument.
        let wall = s.document().walls_on(st).next().unwrap().0;
        assert_eq!(
            s.run(RailingAlongStair { stair: wall, ..cmd.clone() }).unwrap_err().field(),
            Some("stair")
        );
        let too_far = RailingAlongStair { inset: Length::m(0.6), ..cmd };
        assert_eq!(s.run(too_far).unwrap_err().field(), Some("inset"));
    }

    #[test]
    fn gable_roof_slopes_only_the_long_sides() {
        let (mut s, st) = house();
        let mut cmd = RoofFromWalls {
            storey: st,
            kind: RoofKind::Gable,
            pitch: 30.0,
            overhang: Length::ZERO,
            build_up: None,
            base: None,
            parapet: None,
            holes: Vec::new(),
            tag: String::new(),
        };
        let out = s.run(cmd.clone()).unwrap();
        let ElementKind::Roof(r) = &s.document().element(out.created[0]).unwrap().kind else { panic!() };
        let RoofShape::Pitched { slopes } = &r.shape else { panic!() };
        for (i, sl) in slopes.iter().enumerate() {
            let d = edge_dir(&r.outline, i).unwrap();
            assert_eq!(sl.is_some(), d.x.abs() > 0.9, "edge {i}");
        }
        cmd.pitch = 95.0;
        assert!(s.run(cmd).is_err());
    }

    #[test]
    fn detect_all_numbers_rooms_and_skips_existing() {
        let (mut s, st) = house();
        let cmd = DetectAllSpaces {
            storey: st,
            category: SpaceCategory::MainUse,
            name: None,
            skip_existing: true,
            numbered: true,
            locale: Locale::El,
        };
        let out = s.run(cmd.clone()).unwrap();
        assert_eq!(out.created.len(), 1);
        let sp = s.document().element(out.created[0]).unwrap().as_space().unwrap().clone();
        assert_eq!(sp.number, "0.01");
        assert_eq!(sp.name, "Χώρος 1");
        assert!(s.run(cmd).is_err(), "the only room already has a space");
    }

    #[test]
    fn place_opening_projects_onto_the_nearest_wall() {
        let (mut s, st) = house();
        let out = s
            .run(PlaceOpening {
                point: Point2::new(5.0, 0.4),
                kind: OpeningKind::Window,
                host: None,
                storey: Some(st),
                style: None,
                width: Some(Length::m(1.2)),
                height: None,
                sill: None,
                max_distance: Length::m(1.0),
                flip_side: false,
                flip_hand: false,
                params: BTreeMap::new(),
                tag: "Θ1".into(),
                properties: BTreeMap::new(),
            })
            .unwrap();
        let o = s.document().element(out.created[0]).unwrap().as_opening().unwrap().clone();
        // The south wall runs from (12, 0) to (0, 0): x = 5 is 7 m from its start.
        assert!((o.offset.as_m() - 7.0).abs() < 1e-9);
        assert!((o.sill.as_m() - 0.9).abs() < 1e-9);
        assert_eq!(s.history_labels(Locale::El).last().unwrap(), "Νέο παράθυρο");
    }

    #[test]
    fn placed_openings_face_out_of_a_clockwise_ring_by_default() {
        let (mut s, st) = house();
        let outline = crate::geo::outline_from_walls(s.document(), st, OutlineFace::Outer).unwrap();
        for (kind, x) in [("window", 3.0), ("door", 8.0)] {
            // Arguments as a script would send them: no flip given.
            let json = format!(r#"{{"storey":{},"point":[{x},0.0],"kind":"{kind}"}}"#, st.0);
            let c: PlaceOpening = serde_json::from_str(&json).unwrap();
            assert!(!c.flip_side && !c.flip_hand);
            let id = s.run(c).unwrap().created[0];
            let doc = s.document();
            let o = doc.element(id).unwrap().as_opening().unwrap();
            let pl = kanavos_elements::opening_placement(doc, id).unwrap();
            let ctx = kanavos_objects::OpeningContext {
                opening: o,
                left_face: pl.left_face,
                right_face: pl.right_face,
            };
            let tol = doc.tolerance();
            let inside = |d: f64| outline.contains(pl.centre + pl.across * d, &tol);
            // The south wall runs west, so its left face is outside: the outer face is outside and doors open in.
            assert!(!inside(ctx.outer_face() + 0.05), "{kind}: outer face inside");
            assert!(inside(ctx.inner_face() - 0.05), "{kind}: inner face outside");
        }
    }

    #[test]
    fn place_opening_rejects_wrong_styles_and_far_points() {
        let (s, st) = house();
        let mut c = PlaceOpening {
            point: Point2::new(5.0, 4.0),
            kind: OpeningKind::Door,
            host: None,
            storey: Some(st),
            style: None,
            width: None,
            height: None,
            sill: None,
            max_distance: Length::m(1.0),
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
            tag: String::new(),
            properties: BTreeMap::new(),
        };
        assert!(matches!(c.apply(s.document()), Err(CommandError::InvalidArgument { field: "point", .. })));
        c.point = Point2::new(5.0, 0.0);
        c.style = Some("window.fixed".into());
        assert!(matches!(c.apply(s.document()), Err(CommandError::InvalidArgument { field: "style", .. })));
    }

    #[test]
    fn auto_dimensions_add_chains_with_jambs() {
        let (mut s, st) = house();
        let south = s.document().walls_on(st).find(|(_, w)| w.start.y.as_m() < 0.1 && w.end.y.as_m() < 0.1);
        let host = south.map(|(id, _)| id).unwrap();
        s.run(PlaceOpening {
            point: Point2::new(6.0, 0.0),
            kind: OpeningKind::Door,
            host: Some(host),
            storey: None,
            style: None,
            width: Some(Length::m(1.0)),
            height: None,
            sill: None,
            max_distance: Length::m(1.0),
            flip_side: false,
            flip_hand: false,
            params: BTreeMap::new(),
            tag: String::new(),
            properties: BTreeMap::new(),
        })
        .unwrap();
        let out = s
            .run(AutoDimensions {
                storey: st,
                offset: Length::m(1.0),
                spacing: Length::m(0.7),
                openings: true,
                overall: true,
            })
            .unwrap();
        // South: corners + 2 jambs, then overall; the other three sides: corners only.
        assert_eq!(out.created.len(), 5);
        let first = s.document().element(out.created[0]).unwrap();
        let ElementKind::Dimension(d) = &first.kind else { panic!() };
        assert_eq!(d.points.len(), 4);
    }
}
