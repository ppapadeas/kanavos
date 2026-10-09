//! 2D and 3D geometry primitives, predicates and polygon operations.
//!
//! Layer L1. See `docs/architecture.md`.
//!
//! Conventions:
//! - Plan coordinates are metres, X east, Y north, angles counter-clockwise from +X.
//! - [`Point2`] carries typed [`Length`] coordinates because it lives in the document. Vector maths uses
//!   [`Vec2`] (raw metres) for speed and readability.
//! - 3D coordinates ([`Point3`], [`Vec3`]) are raw metres with Z up.
//! - Every predicate that can be affected by rounding takes a [`Tolerance`] or is exact ([`orient2d`]).
//!
//! Operations:
//! - Boolean operations on sets of polygons with holes ([`boolean`], [`union_all`] …).
//! - Offsets of polygons and polylines with miter, round, square or bevel joins ([`offset_polygons`],
//!   [`offset_polyline`], [`stroke_polyline`]).
//! - Segment, line and circle intersections ([`segment_intersection`], [`line_circle`], [`circle_circle`]).
//! - Half-plane clipping ([`HalfPlane`], [`clip_convex`], [`clip_by_half_plane`]) and convex hulls.
//! - The weighted straight skeleton ([`weighted_straight_skeleton`]), the basis of hip and valley roofs.

mod adjacency;
mod boolean;
mod clip;
mod d2;
mod d3;
mod hull;
mod intersect;
mod offset;
mod polygon;
mod predicates;
mod skeleton;

pub use adjacency::{SharedEdge, shared_boundaries};
pub use boolean::{
    BooleanOp, MIN_AREA, bbox_of, boolean, clip_by_half_plane, difference, intersection, resolve_positive,
    split_by_half_plane, total_area, union, union_all, xor,
};
pub use clip::{HalfPlane, clip_convex, clip_convex_all};
pub use d2::{Arc2, Bbox2, Line2, Point2, Segment2, Transform2, Vec2};
pub use d3::{Bbox3, Plane, Point3, Vec3};
pub use hull::convex_hull;
pub use intersect::{SegmentIntersection, circle_circle, line_circle, segment_intersection};
pub use offset::{CapStyle, JoinStyle, offset_polygon, offset_polygons, offset_polyline, stroke_polyline};
pub use polygon::{Containment, Polygon2, PolygonWithHoles};
pub use predicates::{cross3, orient2d};
pub use skeleton::{Skeleton, SkeletonArc, SkeletonFace, straight_skeleton, weighted_straight_skeleton};

pub use kanavos_units::{Angle, Area, Length, Tolerance};

/// Error from a geometric operation. The message is suitable for users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeomError {
    /// The input is degenerate (zero length, zero area, coincident points).
    Degenerate(&'static str),
    /// The input is invalid in another way.
    Invalid(String),
}

impl std::fmt::Display for GeomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeomError::Degenerate(what) => write!(f, "degenerate geometry: {what}"),
            GeomError::Invalid(why) => write!(f, "invalid geometry: {why}"),
        }
    }
}

impl std::error::Error for GeomError {}
