//! Element quantities for schedules.

use kanavos_model::{Document, ElementKind, Id, Profile, RoofShape};
use kanavos_units::{Area, Length, Volume};

use crate::{GeometryError, Quantities, element, wall_layout};

/// Quantities of an element.
pub fn quantities(doc: &Document, id: Id) -> Result<Quantities, GeometryError> {
    let e = element(doc, id)?;
    let mut q = Quantities::default();
    match &e.kind {
        ElementKind::Wall(w) => {
            let layout = wall_layout(doc, id)?;
            let length = w.length();
            let gross = length * w.height;
            let openings: Area = doc.openings_in(id).map(|(_, o)| o.width * o.height).sum();
            q.length = Some(length);
            q.height = Some(w.height);
            q.thickness = Some(layout.thickness);
            q.gross_area = Some(gross);
            q.area = Some(gross - openings);
            q.volume = Some((gross - openings) * layout.thickness);
        }
        ElementKind::Slab(s) => {
            let t = doc.structure_thickness(&s.structure).unwrap_or(Length::ZERO);
            let a = s.outline.area();
            q.area = Some(a);
            q.gross_area = Some(s.outline.outer.area());
            q.thickness = Some(t);
            q.perimeter = Some(s.outline.perimeter());
            q.volume = Some(a * t);
        }
        ElementKind::Roof(r) => {
            let t = doc.structure_thickness(&r.structure).unwrap_or(Length::ZERO);
            let plan = r.outline.area();
            // Sloped area ≈ plan area / cos(pitch) for single-pitch-like roofs; exact planes come from the roof
            // geometry once it is computed per plane.
            let area = match &r.shape {
                RoofShape::Flat { .. } => plan,
                RoofShape::Pitched { slopes } => {
                    let max_cos = slopes.iter().flatten().map(|a| a.cos()).fold(1.0f64, f64::min);
                    if max_cos > 1e-6 { plan / max_cos } else { plan }
                }
            };
            q.area = Some(area);
            q.gross_area = Some(plan);
            q.thickness = Some(t);
            q.perimeter = Some(r.outline.perimeter());
            q.volume = Some(area * t);
        }
        ElementKind::Column(c) => {
            let a = match c.profile {
                Profile::Rectangle { width, depth } => width * depth,
                Profile::Circle { diameter } => {
                    Area::m2(std::f64::consts::PI * (diameter.as_m() / 2.0).powi(2))
                }
            };
            q.height = Some(c.height);
            q.area = Some(a);
            q.volume = Some(a * c.height);
        }
        ElementKind::Beam(b) => {
            let length = b.start.distance(b.end);
            let a = match b.profile {
                Profile::Rectangle { width, depth } => width * depth,
                Profile::Circle { diameter } => {
                    Area::m2(std::f64::consts::PI * (diameter.as_m() / 2.0).powi(2))
                }
            };
            q.length = Some(length);
            q.area = Some(a);
            q.volume = Some(a * length);
        }
        ElementKind::Space(s) => {
            q.area = Some(s.boundary.area());
            q.gross_area = Some(s.boundary.outer.area());
            q.perimeter = Some(s.boundary.perimeter());
            if let Some(h) = s.height {
                q.height = Some(h);
                q.volume = Some(s.boundary.area() * h);
            }
        }
        ElementKind::Opening(o) => {
            q.area = Some(o.width * o.height);
            q.height = Some(o.height);
        }
        ElementKind::Railing(r) => {
            let length: Length = r
                .path
                .windows(2)
                .filter_map(|w| match w {
                    [a, b] => Some(a.distance(*b)),
                    _ => None,
                })
                .sum();
            q.length = Some(length);
            q.height = Some(r.height);
        }
        ElementKind::Stair(s) => {
            q.height = Some(s.rise);
            q.length = Some(s.going * f64::from(s.risers.saturating_sub(1)));
            q.volume = Some(Volume::ZERO);
        }
        _ => {}
    }
    Ok(q)
}
