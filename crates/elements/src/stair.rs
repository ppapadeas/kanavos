//! Stairs: plan layout (treads, landings, walking line), 3D solids (sloped waist under each flight with treads
//! and risers, landing slabs) and plan parts.

use kanavos_geom::{Plane, Point2, Point3, Polygon2, PolygonWithHoles, Segment2, Vec2};
use kanavos_model::{Document, Element, ElementKind, Id, Stair, StairShape};
use kanavos_solid::{Mesh, PlaneFrame, extrude, extrude_polygon, section};
use kanavos_units::Length;

use crate::{CutArea, ElementSolid, GeometryError, PlanParts, SolidPart, base_elevation, element};

/// One flight of a stair.
#[derive(Debug, Clone, PartialEq)]
pub struct StairFlight {
    /// Tread outlines in walking order.
    pub treads: Vec<Polygon2>,
    /// Index (0-based) of the first riser of this flight in the whole stair.
    pub first_riser: usize,
    /// Midpoint of the flight's first riser (front edge of its first step).
    pub start: Point2,
    /// Unit walking direction of the flight.
    pub direction: Vec2,
    /// Absolute elevation of the floor or landing the flight starts from.
    pub start_elevation: f64,
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
    /// Absolute elevation of the top of each tread (tread `i` is reached after riser `i + 1`).
    pub tread_elevations: Vec<f64>,
    /// Absolute elevation of the stair's base.
    pub base: f64,
    /// Absolute elevation of the top (arrival floor).
    pub top: f64,
}

impl StairLayout {
    /// Length of the walking line.
    pub fn walking_length(&self) -> Length {
        Length::m(
            self.walking_line
                .windows(2)
                .map(|w| if let [a, b] = w { a.distance(*b).as_m() } else { 0.0 })
                .sum(),
        )
    }
}

fn rect(origin: Point2, along: Vec2, across: Vec2, length: f64, width: f64) -> Polygon2 {
    let hw = across * (width / 2.0);
    let end = origin + along * length;
    Polygon2::new(vec![origin - hw, end - hw, end + hw, origin + hw]).to_ccw()
}

fn stair_of(e: &Element) -> Result<&Stair, GeometryError> {
    match &e.kind {
        ElementKind::Stair(s) => Ok(s),
        _ => Err(GeometryError::WrongKind { id: e.id, expected: "stair" }),
    }
}

/// Risers of the first flight of a two-flight stair (the landing is reached by the riser with this number).
pub(crate) fn first_flight_risers(s: &Stair) -> u32 {
    s.first_flight_risers.unwrap_or(s.risers / 2).clamp(1, s.risers.saturating_sub(1).max(1))
}

/// Computes the plan layout of a stair.
pub fn stair_layout(doc: &Document, id: Id) -> Result<StairLayout, GeometryError> {
    let e = element(doc, id)?;
    let s = stair_of(e)?;
    if s.risers < 2 {
        return Err(GeometryError::Invalid {
            id: Some(id),
            field: "risers",
            reason: "must be at least 2".into(),
        });
    }
    let base = base_elevation(doc, e)?.as_m();
    let riser = s.riser_height();
    let r = riser.as_m();
    let going = s.going.as_m();
    let width = s.width.as_m();
    let along = Vec2::from_angle(s.direction);
    let across = along.perp();
    // Treads: one fewer than risers (the last riser arrives at the upper floor).
    let treads_total = (s.risers - 1) as usize;
    let tread_elevations: Vec<f64> = (1..=treads_total).map(|i| base + r * i as f64).collect();
    let mut flights = Vec::new();
    let mut landings = Vec::new();
    let mut landing_elevations = Vec::new();
    let mut walking = vec![s.origin];
    let mut outline = Vec::new();

    let flight = |start: Point2, dir: Vec2, count: usize, first: usize, z: f64| StairFlight {
        treads: (0..count)
            .map(|i| rect(start + dir * (going * i as f64), dir, dir.perp(), going, width))
            .collect(),
        first_riser: first,
        start,
        direction: dir,
        start_elevation: z,
    };

    match s.shape {
        StairShape::Straight => {
            flights.push(flight(s.origin, along, treads_total, 0, base));
            let end = s.origin + along * (going * treads_total as f64);
            walking.push(end);
            outline.push(rect(s.origin, along, across, going * treads_total as f64, width));
        }
        shape => {
            let first = first_flight_risers(s) as usize;
            // Flight 1 has `first - 1` treads; the landing takes the place of tread `first`.
            let t1 = first.saturating_sub(1);
            let t2 = treads_total.saturating_sub(first);
            let landing_z = base + r * first as f64;
            flights.push(flight(s.origin, along, t1, 0, base));
            let land_start = s.origin + along * (going * t1 as f64);
            let left = matches!(shape, StairShape::QuarterTurnLeft | StairShape::HalfTurnLeft);
            let side = if left { across } else { -across };
            let land_centre = land_start + along * (width / 2.0);
            let (start2, dir2) = match shape {
                StairShape::QuarterTurnLeft | StairShape::QuarterTurnRight => {
                    landings.push(rect(land_start, along, across, width, width));
                    walking.push(land_centre);
                    (land_centre + side * (width / 2.0), side)
                }
                _ => {
                    // The second flight runs back beside the first, separated by the gap; one landing spans
                    // both flights and the gap.
                    let offset = width + s.gap.as_m();
                    let mid = land_start + side * (offset / 2.0);
                    landings.push(rect(mid, along, across, width, 2.0 * width + s.gap.as_m()));
                    walking.push(land_centre);
                    walking.push(land_centre + side * offset);
                    (land_start + side * offset, -along)
                }
            };
            landing_elevations.push(landing_z);
            flights.push(flight(start2, dir2, t2, first, landing_z));
            walking.push(start2 + dir2 * (going * t2 as f64));
            outline.push(rect(s.origin, along, across, going * t1 as f64 + width, width));
            if t2 > 0 {
                outline.push(rect(start2, dir2, dir2.perp(), going * t2 as f64, width));
            }
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
        base,
        top: base + s.rise.as_m(),
    })
}

/// The closed solid of one flight: steps on top of a sloped waist of perpendicular thickness `waist`.
///
/// The flight is built in its own vertical plane (s along the flight, z up) and extruded across its width. Its
/// lower end rests on the starting floor; its upper end stops at the front of the landing or upper floor, below
/// the last tread.
pub(crate) fn flight_mesh(
    f: &StairFlight,
    count: usize,
    going: f64,
    riser: f64,
    width: f64,
    waist: f64,
) -> Result<Mesh, kanavos_solid::SolidError> {
    if count == 0 {
        return Ok(Mesh::new());
    }
    let z0 = f.start_elevation;
    let len = going * count as f64;
    let hyp = (going * going + riser * riser).sqrt();
    let w = waist * hyp / going.max(1e-9); // vertical waist thickness
    let soffit = |s: f64| z0 + riser + riser / going.max(1e-9) * s - w;
    let last_tread = z0 + riser * count as f64;
    let mut pts: Vec<Point2> = Vec::with_capacity(2 * count + 4);
    // Bottom: on the floor, then along the soffit.
    pts.push(Point2::new(0.0, z0));
    let s_floor = (w - riser) * going / riser.max(1e-9);
    if s_floor > 1e-6 && s_floor < len {
        pts.push(Point2::new(s_floor, z0));
    }
    let end_low = soffit(len).min(last_tread - 0.01).max(z0);
    pts.push(Point2::new(len, end_low));
    // Top: the sawtooth, from the last tread back to the first riser.
    for i in (1..=count).rev() {
        let z = z0 + riser * i as f64;
        pts.push(Point2::new(going * i as f64, z));
        pts.push(Point2::new(going * (i - 1) as f64, z));
    }
    let mut profile = Polygon2::new(pts);
    profile = profile.simplified(&kanavos_units::Tolerance::default());
    let local = extrude(&PolygonWithHoles::from(profile), -width / 2.0, width / 2.0)?;
    let across = f.direction.perp();
    let mesh = local.map_positions(|q| {
        let p = f.start + f.direction * q.x + across * q.z;
        Point3::from_plan(p, q.y)
    });
    // (s, z, a) → (direction, up, left) reverses orientation.
    Ok(mesh.flipped())
}

/// The 3D solids of a stair: one per flight and one per landing.
pub(crate) fn stair_solids(doc: &Document, e: &Element) -> Result<Vec<ElementSolid>, GeometryError> {
    let s = stair_of(e)?;
    let layout = stair_layout(doc, e.id)?;
    let err = |x: kanavos_solid::SolidError| GeometryError::Invalid {
        id: Some(e.id),
        field: "geometry",
        reason: x.to_string(),
    };
    let mut out = Vec::new();
    for f in &layout.flights {
        let mesh = flight_mesh(
            f,
            f.treads.len(),
            s.going.as_m(),
            layout.riser_height.as_m(),
            s.width.as_m(),
            s.thickness.as_m(),
        )
        .map_err(err)?;
        if !mesh.is_empty() {
            out.push(ElementSolid { mesh, material: Some(s.material), part: SolidPart::Flight });
        }
    }
    for (l, z) in layout.landings.iter().zip(&layout.landing_elevations) {
        out.push(ElementSolid {
            mesh: extrude_polygon(l, z - s.thickness.as_m(), *z).map_err(err)?,
            material: Some(s.material),
            part: SolidPart::Landing,
        });
    }
    Ok(out)
}

fn ring_edges(p: &Polygon2) -> Vec<Segment2> {
    p.edges().collect()
}

/// How a stair appears in a plan cut at absolute elevation `cut`: treads and landings below the cut drawn,
/// those above dashed, and the flights' cut section filled.
pub(crate) fn stair_plan_parts(doc: &Document, e: &Element, cut: f64) -> Result<PlanParts, GeometryError> {
    let s = stair_of(e)?;
    let layout = stair_layout(doc, e.id)?;
    let mut parts = PlanParts::default();
    for f in &layout.flights {
        for (i, tread) in f.treads.iter().enumerate() {
            let z = layout.tread_elevations.get(f.first_riser + i).copied().unwrap_or(layout.base);
            let target = if z <= cut { &mut parts.below } else { &mut parts.above };
            target.extend(ring_edges(tread));
        }
        if let Ok(mesh) = flight_mesh(
            f,
            f.treads.len(),
            s.going.as_m(),
            layout.riser_height.as_m(),
            s.width.as_m(),
            s.thickness.as_m(),
        ) {
            let frame = PlaneFrame::new(&Plane::horizontal(cut));
            for region in section(&mesh, &frame, 1e-9) {
                parts.cut.push(CutArea { region, material: Some(s.material) });
            }
        }
    }
    for (l, z) in layout.landings.iter().zip(&layout.landing_elevations) {
        if *z - s.thickness.as_m() < cut && cut < *z {
            parts.cut.push(CutArea { region: l.clone().into(), material: Some(s.material) });
        } else if *z <= cut {
            parts.below.extend(ring_edges(l));
        } else {
            parts.above.extend(ring_edges(l));
        }
    }
    let polys: Vec<PolygonWithHoles> = parts.cut.iter().map(|c| c.region.clone()).collect();
    parts.outline = kanavos_geom::union_all(&polys).unwrap_or(polys);
    Ok(parts)
}
