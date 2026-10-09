//! Small region algebra for wall layers: sets of simple polygons clipped by half-planes and discs and cut by
//! strips, with a fast exact path for convex pieces (straight walls) and Boolean operations otherwise (arcs).

use kanavos_geom::{
    Bbox2, HalfPlane, Point2, Polygon2, PolygonWithHoles, clip_by_half_plane, clip_convex, difference,
    intersection,
};

/// Pieces smaller than this (m²) are dropped: a 0.1 mm sliver along a 10 cm edge.
pub(crate) const SLIVER: f64 = 1e-8;

/// A region to keep: one side of a line, or the inside or outside of a circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Clip {
    /// Keep the half-plane.
    Half(HalfPlane),
    /// Keep the inside (`inside == true`) or the outside of a circle.
    Disc {
        /// Centre.
        centre: Point2,
        /// Radius.
        radius: f64,
        /// Which side to keep.
        inside: bool,
    },
}

/// A region to remove.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CutShape {
    /// The strip inside both half-planes (two parallel lines facing each other).
    Strip(HalfPlane, HalfPlane),
    /// A polygon.
    Region(Polygon2),
}

pub(crate) type Region = Vec<Polygon2>;

/// A circle as a polygon with its vertices on the circle and chords at most 0.2 mm inside it.
pub(crate) fn circle_polygon(centre: Point2, radius: f64) -> Polygon2 {
    let r = radius.abs().max(1e-6);
    let dev = 0.0002f64.min(r * 0.01);
    let step = 2.0 * (1.0 - dev / r).clamp(-1.0, 1.0).acos();
    let n = ((std::f64::consts::TAU / step.max(1e-3)).ceil() as usize).clamp(16, 4096);
    let [cx, cy] = centre.xy();
    Polygon2::new(
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Point2::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect(),
    )
}

fn outers(polys: Vec<PolygonWithHoles>) -> Region {
    polys.into_iter().map(|p| p.outer).filter(|p| p.area().as_m2() > SLIVER).collect()
}

fn bbox(region: &[Polygon2]) -> Option<Bbox2> {
    region.iter().filter_map(Polygon2::bbox).reduce(|a, b| a.union(&b))
}

/// Keeps the part of every piece inside the clip.
pub(crate) fn apply_clip(region: Region, clip: &Clip) -> Region {
    match clip {
        Clip::Half(hp) => {
            let mut out = Vec::with_capacity(region.len());
            for p in region {
                if p.is_convex() {
                    if let Some(c) = clip_convex(&p, hp, 1e-9)
                        && c.area().as_m2() > SLIVER
                    {
                        out.push(c);
                    }
                } else if let Ok(r) = clip_by_half_plane(&[p.into()], hp) {
                    out.extend(outers(r));
                }
            }
            out
        }
        Clip::Disc { centre, radius, inside } => {
            if region.is_empty() {
                return region;
            }
            let disc: PolygonWithHoles = circle_polygon(*centre, *radius).into();
            let polys: Vec<PolygonWithHoles> = region.into_iter().map(Into::into).collect();
            let r = if *inside { intersection(&polys, &[disc]) } else { difference(&polys, &[disc]) };
            r.map(outers).unwrap_or_default()
        }
    }
}

/// Keeps the union of the parts inside either of two clips that do not overlap (the two sides of a crossing
/// wall).
pub(crate) fn apply_either(region: Region, sides: &[Clip; 2]) -> Region {
    let mut out = apply_clip(region.clone(), &sides[0]);
    out.extend(apply_clip(region, &sides[1]));
    out
}

/// Removes a cut from every piece.
pub(crate) fn apply_cut(region: Region, cut: &CutShape) -> Region {
    match cut {
        CutShape::Strip(a, b) => {
            let mut out = Vec::with_capacity(region.len() + 1);
            for p in region {
                if p.is_convex() {
                    // piece − (a ∩ b) = (piece ∩ ¬a) ∪ (piece ∩ ¬b); disjoint because the lines face each other.
                    for hp in [a.flipped(), b.flipped()] {
                        if let Some(c) = clip_convex(&p, &hp, 1e-9)
                            && c.area().as_m2() > SLIVER
                        {
                            out.push(c);
                        }
                    }
                } else {
                    let Some(bounds) = p.bbox() else { continue };
                    let strip = clip_convex(&a.as_polygon(&bounds, 1.0), b, 0.0);
                    match strip {
                        Some(s) => {
                            if let Ok(r) = difference(&[p.clone().into()], &[s.into()]) {
                                out.extend(outers(r));
                            } else {
                                out.push(p);
                            }
                        }
                        None => out.push(p),
                    }
                }
            }
            out
        }
        CutShape::Region(shape) => {
            let Some(b) = bbox(&region) else { return region };
            let Some(sb) = shape.bbox() else { return region };
            if !b.intersects(&sb) {
                return region;
            }
            let polys: Vec<PolygonWithHoles> = region.iter().cloned().map(Into::into).collect();
            match difference(&polys, &[shape.clone().into()]) {
                Ok(r) => outers(r),
                Err(_) => region,
            }
        }
    }
}

/// Total area of a region.
#[cfg(test)]
pub(crate) fn area(region: &[Polygon2]) -> f64 {
    region.iter().map(|p| p.area().as_m2()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Vec2;

    fn strip() -> Polygon2 {
        Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 1.0), (0.0, 1.0)])
    }

    #[test]
    fn strip_cut_splits_a_band() {
        let a = HalfPlane::new(Point2::new(4.0, 0.0), Vec2::X).unwrap();
        let b = HalfPlane::new(Point2::new(5.0, 0.0), -Vec2::X).unwrap();
        let r = apply_cut(vec![strip()], &CutShape::Strip(a, b));
        assert_eq!(r.len(), 2);
        assert!((area(&r) - 9.0).abs() < 1e-12);
        // A strip that misses the band leaves it whole and does not duplicate it.
        let a = HalfPlane::new(Point2::new(14.0, 0.0), Vec2::X).unwrap();
        let b = HalfPlane::new(Point2::new(15.0, 0.0), -Vec2::X).unwrap();
        let r = apply_cut(vec![strip()], &CutShape::Strip(a, b));
        assert!((area(&r) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn disc_clip() {
        let inside = apply_clip(
            vec![strip()],
            &Clip::Disc { centre: Point2::new(0.0, 0.0), radius: 5.0, inside: true },
        );
        assert!(area(&inside) > 4.8 && area(&inside) < 5.0);
        let outside = apply_clip(
            vec![strip()],
            &Clip::Disc { centre: Point2::new(0.0, 0.0), radius: 5.0, inside: false },
        );
        assert!((area(&inside) + area(&outside) - 10.0).abs() < 1e-3);
    }
}
