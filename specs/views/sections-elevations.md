---
id: SPEC-views-004
title: Sections and elevations with hidden-line removal
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 128-3:2020 Technical product documentation — Views, sections and cuts
  - ISO 128-2:2020 Basic conventions for lines
  - ISO 7519:1991 Construction drawings — General principles of presentation
  - A. Appel, "The notion of quantitative invisibility and the machine rendering of solids", Proc. ACM National Conference, 1967
  - I. E. Sutherland, G. W. Hodgman, "Reentrant polygon clipping", Communications of the ACM 17(1), 1974
  - J. Amanatides, A. Woo, "A fast voxel traversal algorithm for ray tracing", Eurographics 1987
  - J. D. Foley, A. van Dam, S. K. Feiner, J. F. Hughes, Computer Graphics: Principles and Practice, 2nd ed., ch. 6 and 15
---

# Sections and elevations with hidden-line removal

## Purpose

Sections and elevations are derived from the 3D model, exactly: cut parts are filled and outlined, everything
beyond is drawn with hidden lines removed, the ground and the levels are marked, as in a permit drawing.

## Behaviour

### Frame

A section or elevation is defined by a line in plan, a depth and a side. The viewer stands on the side opposite
the viewing direction and looks across the line. Drawing `x` runs along the line so that the drawing is never
mirrored (for a flipped view, `x = 0` is the line's end); drawing `y` is the absolute elevation. Only what lies
between the line and the depth, and between the line's ends, is drawn.

### Cut (sections only)

1. Every solid crossing the plane is sectioned: the loops where its faces cross the plane become regions with
   holes (even-odd rule). Vertices exactly on the plane count as behind it, so faces lying in the plane are not
   cut.
2. Cut regions follow the rules of SPEC-views-001 §Cut areas: material fills, merging of equal materials,
   priority trimming, the white rule for finishes thinner than 0,5 mm on paper, one heavy outline.

### Lines beyond

1. For each element, its solids are merged into one surface; faces shared by two touching solids of the element
   (e.g. two layers of a wall) are removed. The candidate edges are: creases where faces meet at more than 15°
   (when at least one face faces the viewer), silhouettes (a face towards the viewer meets a face away from it)
   and open boundaries. Edges where the surface continues flat (seams of the triangulation) are not candidates.
2. A candidate edge can change visibility only where its projection crosses another edge's projection, at the
   ends of collinear overlapping edges, where it pierces a face, and where it crosses the outline of a cut
   region. It is split at all these points; each piece is visible if no face nearer to the viewer covers its
   middle. Points on the boundary of a nearer face count as covered, so the nearer contour wins. A point inside a
   face lying in the same plane is covered as well (an edge buried flush in another element's face is not drawn).
   Depth differences below 1 µm never hide.
3. Collinear visible pieces of the same element are merged.
4. Line weight by distance from the nearest visible line: up to 1,5 m the beyond pen, up to 6 m the far pen,
   further the finest pen (0,13 mm). This can be switched off.
5. In elevations, a line that separates the building from the empty background (tested 0,4 mm to either side)
   is a **profile line**, drawn with the profile pen.

### Ground

- A heavy ground line at the site's natural ground level, extending 12 mm beyond the drawing on both sides.
- In sections, a 5 mm band of earth hatch under the ground line, outside the building's cut extent; the ground
  line is not drawn across the building.
- Lines below ground are removed, except, in sections, inside the building's cut extent (basements).

### Level marks

On the right of the drawing, 14 mm away: a level mark (SPEC-views-003) for every storey floor, for the top of the
building and for the natural ground (labelled «Φ.Ε.»), each with a thin line from the building.

### Annotation owned by the view

Dimensions, texts, level marks and drafting items that belong to the view are drawn in view coordinates
(`x` along the line, `y` elevation).

### Layers

`<kind>.cut`, `<kind>.layer`, `<kind>.beyond`, `ground.line`, `ground.earth`, `level.*`, `dimension.*`,
`text`, `drafting`.

## Performance

A section of a house of about 300 elements and 100 000 triangles must be derived in less than 300 ms on a
current desktop in a release build. Element meshes are reused between document versions when the element and
everything its geometry depends on are unchanged.

## Edge cases

- Interpenetrating solids: the line where one solid's face enters another is not computed (it appears once
  junctions are resolved in element geometry); visibility is still correct, because pierce points split edges.
- Open meshes are drawn with their boundaries; their cut loops that do not close are ignored.

## Acceptance tests

1. Given a line behind a square face, then it is split at the square's contour and only the outer parts remain.
2. Given a line that pierces a face, then it is visible only on the near side of the pierce point.
3. Given the fixture house, when its section is drawn, then walls, slabs and the stair are cut, the level marks
   `±0,00` and `+3,20` and the ground line appear, and window frames beyond are drawn.
4. Given the fixture house's south elevation, then nothing is cut and window frames are drawn.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending — Result: pending
