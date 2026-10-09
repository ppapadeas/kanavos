---
id: SPEC-geometry-001
title: Polygon Boolean operations, offsets, clipping and intersections
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - M. de Berg, O. Cheong, M. van Kreveld, M. Overmars, "Computational Geometry", 3rd ed., Springer 2008, §1.1 (convex hull), §2 (segment intersection)
  - J. R. Shewchuk, "Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates", DCG 18 (1997)
  - X. Chen, S. McMains, "Polygon Offsetting by Computing Winding Numbers", ASME IDETC 2005
  - B. R. Vatti, "A Generic Solution to Polygon Clipping", CACM 35(7) 1992
  - i_overlay 9.x (MIT OR Apache-2.0), integer-grid Boolean operations
---

# Polygon Boolean operations, offsets, clipping and intersections

## Purpose

Element geometry (wall junctions, slab edges, roof overhangs, space boundaries, the building footprint used for
coverage) needs robust 2D operations on polygons with holes that never crash on the coincident edges and touching
corners that are normal in buildings.

## Behaviour

### Boolean operations

- Inputs are sets of polygons with holes. Orientation of the input does not matter: every polygon is normalised
  (outer ring counter-clockwise, holes clockwise) and the non-zero fill rule is applied, so overlapping polygons in
  the same set are merged.
- Operations: union, intersection, difference (subject minus clip) and exclusive or; self-union of one set.
- Output: polygons with holes, outer rings counter-clockwise, holes clockwise, collinear vertices removed, contours
  smaller than 10⁻¹⁰ m² removed.
- Coordinates are snapped to an integer grid fitted to the bounding box of the operands (about 2⁻²⁹ of its
  extent); for a 100 m building that is below 0.2 µm, far below the project tolerance of 0.1 mm.
- Non-finite coordinates or coordinates beyond 10¹² m are rejected with an error; nothing panics.

### Offsets

- Polygons: positive distance grows the material, negative shrinks it; holes move the opposite way to the outer
  ring. Joins where offset edges separate: miter (with a limit as a multiple of the distance; beyond it the corner
  is cut square at that distance), round (chord deviation given in metres), square (cut perpendicular to the
  bisector at the offset distance) or bevel.
- The raw offset curve is cleaned with the positive winding rule; a polygon shrunk beyond its inradius vanishes.
- Open polylines: a parallel polyline at a signed distance (positive to the left of travel), trimmed at inside
  corners; and the stroke region of a given half width with butt, square or round caps.

### Clipping and intersections

- Half-plane clipping of convex polygons (exact for convex input) and of arbitrary polygons (through the Boolean
  operations).
- Segment–segment classification: none, a single point (crossing or touching within tolerance) or a collinear
  overlap, decided with the exact orientation predicate.
- Line–circle and circle–circle intersections for arc walls.
- Convex hull (monotone chain, exact predicate, collinear points dropped).
- Shared boundaries between regions: collinear overlapping edges of different regions, used for ridge, hip and
  valley lines between roof planes.
- Faces of a planar arrangement of segments: the segments are split at crossings and overlap ends, joined into a
  planar graph and every closed region is returned, with enclosed components as holes; dangling segments close
  nothing.

## Edge cases

- Empty operands: intersection or difference with an empty subject is empty; union with an empty set is a
  normalised copy.
- Walls whose bands share a face exactly merge into one polygon without slivers.
- A ring of walls gives one polygon with one hole (the room).
- Degenerate rings (fewer than three vertices) are ignored.

## Acceptance tests

1. Given two 2 × 2 squares overlapping by 1 × 1, union area is 7, intersection 1, difference 3, exclusive or 6.
2. Given a 10 × 10 square minus a 2 × 2 square inside it, the result has one hole and area 96.
3. Given four wall bands forming a closed ring, the union has one hole whose area equals the clear room area.
4. Given a 4 × 4 square grown by 1 with round joins, the area is 32 + π within 0.01; with bevel joins 34; with
   miter joins 36; shrunk by 2.5 it vanishes.
5. Given an L-shaped polygon grown by 0.5 with miter joins, the result has six vertices and the exact area.
6. Given a U-shaped polygon clipped by a horizontal half-plane through both arms, two pieces result.
7. Two horizontal and two vertical overshooting lines give one 6 m² cell; a rectangle split by a middle line with a
   dangling stub gives two faces of 8 m² in total; a square drawn inside another becomes its hole.

## Open questions

- None.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
