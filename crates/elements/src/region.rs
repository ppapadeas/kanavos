//! Small region algebra for wall layers: sets of simple polygons clipped by half-planes and discs and cut by
//! strips, with a fast exact path for convex pieces (straight walls) and Boolean operations otherwise (arcs).

use kanavos_geom::{
    Bbox2, HalfPlane, Point2, Polygon2, PolygonWithHoles, Vec2, clip_by_half_plane, clip_convex, difference,
    intersection, split_by_half_plane,
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

/// Splits polygons with holes into simple polygons covering the same area, for builders that need simple pieces
/// (column solids, height clipping).
///
/// A polygon with a hole is cut in two by the vertical line through the middle of its first hole's bounding box;
/// the line crosses that hole, so neither half keeps it as a hole. The halves are split again until no holes are
/// left. The cuts run on the Boolean grid, so the two sides of a cut share their edge exactly. Pieces below
/// [`SLIVER`] are dropped. If a cut fails, the polygon's outer ring is kept (its holes are lost) rather than
/// failing the whole element.
pub(crate) fn simple_pieces(polys: &[PolygonWithHoles]) -> Vec<Polygon2> {
    let mut out = Vec::with_capacity(polys.len());
    let mut todo: Vec<PolygonWithHoles> = polys.to_vec();
    // Each cut removes at least one hole, so this bounds the work even for unexpected input.
    let mut budget = 4 * polys.iter().map(|p| p.holes.len() + 1).sum::<usize>() + 16;
    while let Some(p) = todo.pop() {
        let cut = p.holes.first().and_then(Polygon2::bbox).and_then(|b| {
            let x = (b.min.x.as_m() + b.max.x.as_m()) / 2.0;
            HalfPlane::new(Point2::new(x, 0.0), Vec2::X)
        });
        match cut {
            Some(hp) if budget > 0 => {
                budget -= 1;
                match split_by_half_plane(std::slice::from_ref(&p), &hp) {
                    Ok((left, right)) => {
                        todo.extend(left.into_iter().chain(right).filter(|q| q.area().as_m2() > SLIVER));
                    }
                    Err(_) => out.push(p.outer),
                }
            }
            _ => {
                if p.outer.area().as_m2() > SLIVER {
                    out.push(p.outer);
                }
            }
        }
    }
    out
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
    fn polygons_with_holes_split_into_simple_pieces() {
        let outer = Polygon2::from_m(&[(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)]);
        let a = Polygon2::from_m(&[(2.0, 2.0), (3.0, 2.0), (3.0, 3.0), (2.0, 3.0)]);
        let b = Polygon2::from_m(&[(6.0, 1.0), (8.0, 1.0), (8.0, 4.0), (6.0, 4.0)]);
        let p = PolygonWithHoles { outer, holes: vec![a.to_cw(), b.to_cw()] };
        let pieces = simple_pieces(std::slice::from_ref(&p));
        assert!(pieces.len() >= 3);
        assert!((area(&pieces) - p.area().as_m2()).abs() < 1e-9);
        // The pieces cover none of the holes.
        for q in [Point2::new(2.5, 2.5), Point2::new(7.0, 2.0)] {
            assert!(!pieces.iter().any(|piece| piece.contains(q, &kanavos_geom::Tolerance::default())));
        }
        // Simple polygons pass through unchanged.
        let simple = simple_pieces(&[strip().into()]);
        assert_eq!(simple, vec![strip()]);
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
