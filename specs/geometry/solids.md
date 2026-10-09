---
id: SPEC-geometry-003
title: Solid builders, mesh sections and clipping
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - D. Eberly, "Triangulation by Ear Clipping", Geometric Tools, 2002–2015
  - H. Edelsbrunner, E. P. Mücke, "Simulation of Simplicity", ACM TOG 9(1), 1990 (symbolic perturbation)
  - M. Mäntylä, "An Introduction to Solid Modeling", Computer Science Press, 1988 (boundary representation, Euler characteristic of closed shells)
---

# Solid builders, mesh sections and clipping

## Purpose

Element solids feed the 3D view, sections, elevations, quantities and IFC. They must be closed, consistently
oriented meshes so that volumes are exact and a section plane always gives closed outlines. Walls need true door
and window holes and tops that follow a roof slope, without a general solid Boolean kernel.

## Behaviour

### Column solids (walls with holes and sloped tops)

- Input: plan pieces tiling a region, each with its solid vertical intervals; interval bounds are planes
  `z = z0 + gx·x + gy·y`.
- Output: one closed mesh. Each interval contributes its bottom and top faces; outer plan edges get vertical side
  faces; edges shared by two pieces get faces only where one side is solid and the other is not (window jambs,
  steps between different tops).
- Plan T-junctions (a corner of one piece on the edge of another) and vertical T-junctions (a height where a
  neighbouring face has a corner) are resolved, so the mesh is watertight.
- Convenience: a convex prism with rectangular holes running through it perpendicular to a direction (a straight
  wall with doors and windows).

### Other builders

- Sloped slabs: a prism between two planes over a plan profile with holes.
- Sweeps: a closed profile (x to the left of travel, y up) along an open polyline, mitred at joints, capped at
  the ends; joints turning back more than about 170° are rejected.

### Mesh operations

- Welding merges vertices within a distance and removes degenerate triangles; a mesh is closed when every edge is
  used exactly twice in opposite directions.
- Smooth vertex normals (area-weighted), and splitting along creases sharper than a given angle for flat shading.
- Section by a plane: polygons with holes in a 2D frame on the plane. For non-horizontal planes the frame's `v`
  axis points up and `u` is horizontal, so a vertical section reads as an elevation. Vertices within the
  tolerance of the plane are treated as just in front of it; a section exactly through a face shows the solid just
  behind it. Outer rings are counter-clockwise seen from the front.
- Clipping by a plane: the part behind the plane, closed with a cap on the plane.

## Edge cases

- Several overlapping closed shells in one mesh give overlapping section polygons; callers merge them if needed.
- Open meshes leave section chains that do not close; they are dropped.

## Acceptance tests

1. A 5 × 0.3 m wall, 3 m high, with a 1.2 × 1.2 m window is closed and has volume 4.5 − 0.432 m³.
2. The same wall with a door to the floor and two windows is closed and has the net volume.
3. A wall whose top rises from 3 to 4 m along its length is closed with volume 0.3 × 5 × 3.5 m³.
4. Pieces with different tops and a T-junction form a closed mesh with the sum of their volumes.
5. A longitudinal vertical section through the windowed wall is one polygon with one hole of 1.44 m²; a horizontal
   section at 1.5 m gives two pieces.
6. Clipping a 4 × 3 × 2 box at 0.5 m keeps a closed 6 m³ solid; clipping by a slanted plane through its centre
   keeps half.
7. A rail swept along an L path keeps area × centre-line length.

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
