//! Rules that turn element parameters into geometry: walls, slabs, openings, roofs, stairs, spaces.
//!
//! Layer L4. See `docs/architecture.md`.
//!
//! Everything here is a pure function of a [`Document`]: given an element id, compute its plan representation at
//! a cut plane ([`plan_parts`]), its 3D solids ([`solids`]), quantities, snap geometry and so on. Views, schedules,
//! exports and tools build on these functions; none of them computes element geometry on its own.
//!
//! Coordinates: plan metres; 3D metres with Z = absolute elevation above project zero (±0.00).
//!
//! Main entry points:
//! - Walls: [`wall_layout`] (junction-clean outline and layer bands), [`StoreyGeometry`] (all walls of a storey
//!   in one pass), [`Junction`] kinds L, T, X, continuation and node.
//! - Roofs: [`roof_planes`], [`roof_lines`].
//! - Stairs: [`stair_layout`], [`stair_checks`] (limits per use: [`stair_limits`]); railings: [`railing_checks`].
//! - Spaces and outlines: [`detect_space`], [`detect_all_spaces`], [`storey_footprint`], [`outline_from_walls`].
//! - Every element: [`plan_parts`], [`solids`], [`quantities`], [`detailed_quantities`], [`footprint`],
//!   [`snap_geometry`], [`validate`].

mod build;
mod checks;
mod junction;
mod quantities;
mod railing;
mod region;
mod roof;
mod space;
mod stair;
mod storey;
mod wall;

use kanavos_geom::{Point2, PolygonWithHoles, Segment2, Vec2};
use kanavos_model::{Document, Element, ElementKind, Id};
use kanavos_solid::Mesh;
use kanavos_units::{Area, Length, Volume};

pub use build::plan_parts;
pub use build::{base_elevation, solids, validate};
pub use checks::{
    AUXILIARY_GOING_MIN, AUXILIARY_RISER_MAX, Check, CheckLevel, GOING_MIN, GOING_MIN_TYPE_I, GUARD_GAP_MAX,
    GUARD_MIN, HEADROOM_MIN, LANDING_MIN_TYPE_I, LANDING_MIN_TYPE_II, MAX_FLIGHT_RISE,
    RESIDENTIAL_NARROW_STOREYS, RISER_MAX, RISER_MAX_TYPE_I, RISER_MIN, SOLID_GUARD_MAX, STEP_RULE,
    StairLimits, WIDTH_MIN_DWELLING, WIDTH_MIN_TYPE_I, WIDTH_MIN_TYPE_II, railing_checks, stair_checks,
    stair_limits,
};
pub use junction::{JUNCTION_TOLERANCE, Junction, JunctionKind, WallJoin};
pub use quantities::{DetailedQuantities, LayerVolume, PlaneArea, detailed_quantities, quantities};
pub use railing::{
    BAR_GAP, BAR_SIZE, BOTTOM_RAIL, GLASS_THICKNESS, PANEL_THICKNESS, POST_SIZE, RAIL_HEIGHT, RAIL_WIDTH,
    railing_length,
};
pub use roof::{RoofLine, RoofLineKind, RoofPlane, roof_lines, roof_planes};
pub use space::{
    OutlineFace, detect_all_spaces, detect_space, outline_from_walls, outline_with_overhang, storey_footprint,
};
pub use stair::{StairFlight, StairLayout, stair_layout};
pub use storey::StoreyGeometry;
pub use wall::{LayerBand, OpeningPlacement, WallLayout, opening_placement, wall_layout};

/// Why geometry could not be computed. Messages are shown to users next to the failing element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeometryError {
    /// The element does not exist.
    NotFound(Id),
    /// The element is not of the kind the function needs (e.g. a wall).
    WrongKind {
        /// Element.
        id: Id,
        /// What was expected.
        expected: &'static str,
    },
    /// A referenced storey, material, build-up or host is missing.
    MissingReference {
        /// Element.
        id: Id,
        /// What is missing.
        what: String,
    },
    /// The parameters describe impossible geometry.
    Invalid {
        /// Element (`None` for elements being validated before creation).
        id: Option<Id>,
        /// Field at fault.
        field: &'static str,
        /// Explanation.
        reason: String,
    },
    /// The operation is not available for this input yet.
    Unsupported(String),
}

impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeometryError::NotFound(id) => write!(f, "element {id} not found"),
            GeometryError::WrongKind { id, expected } => write!(f, "element {id} is not a {expected}"),
            GeometryError::MissingReference { id, what } => write!(f, "element {id}: missing {what}"),
            GeometryError::Invalid { id: Some(id), field, reason } => {
                write!(f, "element {id}: {field} {reason}")
            }
            GeometryError::Invalid { id: None, field, reason } => write!(f, "{field} {reason}"),
            GeometryError::Unsupported(what) => write!(f, "not supported yet: {what}"),
        }
    }
}

impl std::error::Error for GeometryError {}

/// An area cut by a plan's cut plane (or a section plane), filled with its material's hatch.
#[derive(Debug, Clone, PartialEq)]
pub struct CutArea {
    /// The cut region.
    pub region: PolygonWithHoles,
    /// Material, for the hatch and fill.
    pub material: Option<Id>,
}

/// How an element appears in a plan cut at a given elevation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanParts {
    /// Regions sliced by the cut plane: drawn with a heavy outline and the material hatch.
    pub cut: Vec<CutArea>,
    /// Visible edges below the cut plane, seen from above: drawn with thin lines.
    pub below: Vec<Segment2>,
    /// Edges above the cut plane (overhead): drawn dashed.
    pub above: Vec<Segment2>,
    /// Outer contour of the cut regions (the union of `cut`), for the heavy outline; boundaries between layers
    /// inside it are drawn thin.
    pub outline: Vec<PolygonWithHoles>,
}

/// Which part of an element a solid is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SolidPart {
    /// The element body.
    Body,
    /// Layer `i` of a build-up.
    Layer(usize),
    /// A parapet.
    Parapet,
    /// A stair tread or flight.
    Flight,
    /// A stair landing.
    Landing,
    /// A railing post.
    Post,
    /// A railing top rail.
    Rail,
    /// A railing infill panel or bar.
    Infill,
}

/// One solid of an element, in world coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementSolid {
    /// Closed triangle mesh.
    pub mesh: Mesh,
    /// Material for colour and section hatching.
    pub material: Option<Id>,
    /// Which part this is.
    pub part: SolidPart,
}

/// Quantities of one element, for schedules and reports. Fields that do not apply are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Quantities {
    /// Length (walls along the reference line, beams, railings, stair walking line).
    pub length: Option<Length>,
    /// Height (walls, columns).
    pub height: Option<Length>,
    /// Thickness (walls, slabs, roofs).
    pub thickness: Option<Length>,
    /// Net area: wall side area minus openings; slab, roof and space plan area.
    pub area: Option<Area>,
    /// Gross area before openings are subtracted.
    pub gross_area: Option<Area>,
    /// Perimeter (slabs, spaces).
    pub perimeter: Option<Length>,
    /// Volume (walls, slabs, roofs, stairs and railings: sum of their exact solids).
    pub volume: Option<Volume>,
    /// Walls: net area of the left face (after junction clean-up, minus openings).
    pub face_area_left: Option<Area>,
    /// Walls: net area of the right face.
    pub face_area_right: Option<Area>,
}

/// Kind of a snap point offered by an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SnapKind {
    /// End of a line or reference line.
    Endpoint,
    /// Midpoint of a line.
    Midpoint,
    /// Centre of a column, circle or object.
    Centre,
    /// Corner of an outline or face.
    Corner,
    /// Insertion point of an object or text.
    Insertion,
}

/// Points and segments an element offers to snapping and inference.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SnapGeometry {
    /// Snap points.
    pub points: Vec<(Point2, SnapKind)>,
    /// Segments (reference lines, faces, edges) for nearest/perpendicular/intersection snaps.
    pub segments: Vec<Segment2>,
}

/// Looks up an element or fails with [`GeometryError::NotFound`].
pub fn element(doc: &Document, id: Id) -> Result<&Element, GeometryError> {
    doc.element(id).ok_or(GeometryError::NotFound(id))
}

/// Plan footprint of a building part (walls with junctions, slabs, columns, roofs, stairs, objects as their
/// bounding outline). Annotation returns `None`.
pub fn footprint(doc: &Document, id: Id) -> Result<Option<PolygonWithHoles>, GeometryError> {
    let e = element(doc, id)?;
    Ok(match &e.kind {
        ElementKind::Wall(_) => Some(wall_layout(doc, id)?.outline.into()),
        ElementKind::Slab(s) => Some(s.outline.clone()),
        ElementKind::Roof(r) => Some(PolygonWithHoles { outer: r.outline.clone(), holes: r.holes.clone() }),
        ElementKind::Space(s) => Some(s.boundary.clone()),
        ElementKind::Column(c) => Some(build::column_profile(c).into()),
        ElementKind::Beam(b) => build::beam_footprint(b).map(Into::into),
        ElementKind::Stair(_) => {
            let l = stair_layout(doc, id)?;
            l.outline.into_iter().next().map(Into::into)
        }
        _ => None,
    })
}

/// Snap points and segments of an element (reference lines, faces, corners, centres).
pub fn snap_geometry(doc: &Document, id: Id) -> SnapGeometry {
    let Some(e) = doc.element(id) else { return SnapGeometry::default() };
    let mut g = SnapGeometry::default();
    let seg = |g: &mut SnapGeometry, s: Segment2| {
        g.points.push((s.start, SnapKind::Endpoint));
        g.points.push((s.end, SnapKind::Endpoint));
        g.points.push((s.midpoint(), SnapKind::Midpoint));
        g.segments.push(s);
    };
    match &e.kind {
        ElementKind::Wall(w) => {
            seg(&mut g, Segment2::new(w.start, w.end));
            if let Ok(l) = wall_layout(doc, id) {
                for edge in l.outline.edges() {
                    g.points.push((edge.start, SnapKind::Corner));
                    g.segments.push(edge);
                }
            }
        }
        ElementKind::Beam(b) => seg(&mut g, Segment2::new(b.start, b.end)),
        ElementKind::Grid(gl) => seg(&mut g, Segment2::new(gl.start, gl.end)),
        ElementKind::Column(c) => {
            g.points.push((c.position, SnapKind::Centre));
            for edge in build::column_profile(c).edges() {
                g.points.push((edge.start, SnapKind::Corner));
                g.segments.push(edge);
            }
        }
        ElementKind::Slab(s) => {
            for ring in s.outline.rings() {
                for edge in ring.edges() {
                    seg(&mut g, edge);
                }
            }
        }
        ElementKind::Roof(r) => {
            for edge in r.outline.edges().chain(r.holes.iter().flat_map(|h| h.edges().collect::<Vec<_>>())) {
                seg(&mut g, edge);
            }
            if let Ok(lines) = roof_lines(doc, id) {
                for l in lines {
                    if matches!(l.kind, RoofLineKind::Ridge | RoofLineKind::Hip | RoofLineKind::Valley) {
                        seg(&mut g, l.segment);
                    }
                }
            }
        }
        ElementKind::Space(s) => {
            for edge in s.boundary.outer.edges() {
                g.points.push((edge.start, SnapKind::Corner));
                g.segments.push(edge);
            }
        }
        ElementKind::Railing(r) => {
            for w in r.path.windows(2) {
                if let [a, b] = w {
                    seg(&mut g, Segment2::new(*a, *b));
                }
            }
        }
        ElementKind::Object(o) => g.points.push((o.position, SnapKind::Insertion)),
        ElementKind::Text(t) => g.points.push((t.position, SnapKind::Insertion)),
        ElementKind::Level(l) => g.points.push((l.position, SnapKind::Insertion)),
        ElementKind::Stair(s) => {
            g.points.push((s.origin, SnapKind::Insertion));
            if let Ok(l) = stair_layout(doc, id) {
                for o in &l.outline {
                    for edge in o.edges() {
                        g.points.push((edge.start, SnapKind::Corner));
                        g.segments.push(edge);
                    }
                }
            }
        }
        ElementKind::Drafting(d) => match &d.shape {
            kanavos_model::DraftShape::Line { a, b } => seg(&mut g, Segment2::new(*a, *b)),
            kanavos_model::DraftShape::Polyline { points, closed } => {
                for w in points.windows(2) {
                    if let [a, b] = w {
                        seg(&mut g, Segment2::new(*a, *b));
                    }
                }
                if *closed && let (Some(a), Some(b)) = (points.last(), points.first()) {
                    seg(&mut g, Segment2::new(*a, *b));
                }
            }
            kanavos_model::DraftShape::Circle { centre, .. } => g.points.push((*centre, SnapKind::Centre)),
            kanavos_model::DraftShape::Arc { arc } => {
                g.points.push((arc.centre, SnapKind::Centre));
                g.points.push((arc.at(0.0), SnapKind::Endpoint));
                g.points.push((arc.at(1.0), SnapKind::Endpoint));
            }
            kanavos_model::DraftShape::Fill { region } => {
                for edge in region.outer.edges() {
                    seg(&mut g, edge);
                }
            }
        },
        ElementKind::Dimension(_) | ElementKind::Opening(_) | ElementKind::Underlay(_) => {}
    }
    g
}

/// Unit vector along a segment, or +X for degenerate segments.
pub(crate) fn dir(a: Point2, b: Point2) -> Vec2 {
    (b - a).normalized().unwrap_or(Vec2::X)
}
