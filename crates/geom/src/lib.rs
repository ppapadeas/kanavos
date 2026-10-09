//! 2D and 3D geometry primitives, predicates and polygon operations.
//!
//! Layer L1. See `docs/architecture.md`.
//!
//! Conventions:
//! - Plan coordinates are metres, X east, Y north, angles counter-clockwise from +X.
//! - [`Point2`] carries typed [`Length`] coordinates because it lives in the document. Vector maths uses
//!   [`Vec2`] (raw metres) for speed and readability.
//! - 3D coordinates ([`Point3`], [`Vec3`]) are raw metres with Z up.
//! - Every predicate that can be affected by rounding takes a [`Tolerance`].

mod d2;
mod d3;
mod polygon;

pub use d2::{Arc2, Bbox2, Line2, Point2, Segment2, Transform2, Vec2};
pub use d3::{Bbox3, Plane, Point3, Vec3};
pub use polygon::{Containment, Polygon2, PolygonWithHoles};

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
