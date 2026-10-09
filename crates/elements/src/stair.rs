//! Stair layout: treads, landings and the walking line in plan.

use kanavos_geom::{Point2, Polygon2, Vec2};
use kanavos_model::{Document, ElementKind, Id, StairShape};
use kanavos_units::Length;

use crate::{GeometryError, base_elevation, element};

/// One flight of a stair.
#[derive(Debug, Clone, PartialEq)]
pub struct StairFlight {
    /// Tread outlines in walking order.
    pub treads: Vec<Polygon2>,
    /// Index (0-based) of the first riser of this flight in the whole stair.
    pub first_riser: usize,
}

/// Plan layout of a stair.
#[derive(Debug, Clone, PartialEq)]
pub struct StairLayout {
    /// Flights in walking order.
    pub flights: Vec<StairFlight>,
    /// Landings.
    pub landings: Vec<Polygon2>,
    /// Absolute elevation of the top of each landing.
    pub landing_elevations: Vec<f64>,
    /// Walking line from the first riser to the top.
    pub walking_line: Vec<Point2>,
    /// Outline(s) of the whole stair in plan.
    pub outline: Vec<Polygon2>,
    /// Height of one riser.
    pub riser_height: Length,
    /// Absolute elevation of the top of tread `i` (tread `i` is reached after riser `i + 1`).
    pub tread_elevations: Vec<f64>,
}

fn rect(origin: Point2, along: Vec2, across: Vec2, length: f64, width: f64) -> Polygon2 {
    let hw = across * (width / 2.0);
    let end = origin + along * length;
    Polygon2::new(vec![origin - hw, end - hw, end + hw, origin + hw])
}

/// Computes the plan layout of a stair.
pub fn stair_layout(doc: &Document, id: Id) -> Result<StairLayout, GeometryError> {
    let e = element(doc, id)?;
    let ElementKind::Stair(s) = &e.kind else {
        return Err(GeometryError::WrongKind { id, expected: "stair" });
    };
    if s.risers < 2 {
        return Err(GeometryError::Invalid {
            id: Some(id),
            field: "risers",
            reason: "must be at least 2".into(),
        });
    }
    let base = base_elevation(doc, e)?.as_m();
    let riser = s.riser_height();
    let going = s.going.as_m();
    let width = s.width.as_m();
    let along = Vec2::from_angle(s.direction);
    let across = along.perp();
    // Treads: one fewer than risers (the last riser arrives at the upper floor).
    let treads_total = (s.risers - 1) as usize;
    let tread_elevations: Vec<f64> = (1..=treads_total).map(|i| base + riser.as_m() * i as f64).collect();
    let mut flights = Vec::new();
    let mut landings = Vec::new();
    let mut landing_elevations = Vec::new();
    let mut walking = vec![s.origin];
    let mut outline = Vec::new();

    let flight = |start: Point2, dir: Vec2, count: usize, first: usize| StairFlight {
        treads: (0..count)
            .map(|i| rect(start + dir * (going * i as f64), dir, dir.perp(), going, width))
            .collect(),
        first_riser: first,
    };

    match s.shape {
        StairShape::Straight => {
            flights.push(flight(s.origin, along, treads_total, 0));
            let end = s.origin + along * (going * treads_total as f64);
            walking.push(end);
            outline.push(rect(s.origin, along, across, going * treads_total as f64, width));
        }
        shape => {
            let first = s.first_flight_risers.unwrap_or(s.risers / 2).clamp(1, s.risers - 1) as usize;
            // Flight 1 has `first - 1` treads, then the landing takes the place of tread `first`.
            let t1 = first.saturating_sub(1);
            let t2 = treads_total.saturating_sub(first);
            flights.push(flight(s.origin, along, t1, 0));
            let land_start = s.origin + along * (going * t1 as f64);
            let landing = rect(land_start, along, across, width, width);
            landings.push(landing.clone());
            landing_elevations.push(base + riser.as_m() * first as f64);
            let land_centre = land_start + along * (width / 2.0);
            walking.push(land_centre);
            let left = matches!(shape, StairShape::QuarterTurnLeft | StairShape::HalfTurnLeft);
            let side = if left { across } else { -across };
            let (start2, dir2) = match shape {
                StairShape::QuarterTurnLeft | StairShape::QuarterTurnRight => {
                    (land_centre + side * (width / 2.0), side)
                }
                _ => {
                    // Second flight runs back, beside the first, separated by the gap.
                    let offset = width + s.gap.as_m();
                    let landing2 = rect(land_start + side * offset, along, across, width, width);
                    landings.push(landing2);
                    landing_elevations.push(base + riser.as_m() * first as f64);
                    (land_start + side * offset, -along)
                }
            };
            flights.push(flight(start2, dir2, t2, first));
            walking.push(start2 + dir2 * (going * t2 as f64));
            outline.push(rect(s.origin, along, across, going * t1 as f64 + width, width));
            outline.push(rect(start2, dir2, dir2.perp(), going * t2 as f64, width));
        }
    }
    Ok(StairLayout {
        flights,
        landings,
        landing_elevations,
        walking_line: walking,
        outline,
        riser_height: riser,
        tread_elevations,
    })
}
