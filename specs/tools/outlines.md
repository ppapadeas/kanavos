---
id: SPEC-tools-004
title: Slabs, roofs, spaces, fills, the plot, and measuring
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §3 and §4 (drawing, regulation-aware spaces)
  - ν. 4067/2012 (ΝΟΚ) for space categories used in area tables
  - SPEC-tools-001, SPEC-tools-002
  - General drafting practice (contributors' own professional knowledge)
---

# Slabs, roofs, spaces, fills, the plot, and measuring

## Purpose

Create elements defined by a closed outline — slabs, roofs, spaces, filled regions and the plot boundary — with as
few clicks as possible, and measure lengths and areas without creating anything.

## Behaviour

**Getting the outline** (chosen per tool in the context bar):

- *Polygon*: click the vertices (typed input works for each). Clicking within the snap radius of the first vertex
  (after three vertices), a double click, or Enter closes the outline. Backspace removes the last vertex; Escape
  abandons the outline.
- *Rectangle*: two opposite corners, or the first corner and typed Δx and Δy.
- *Pick inside*: click inside an area enclosed by walls. The enclosed region is previewed while hovering. Spaces use
  the walls' inside faces; slabs and roofs grow each outline edge that lies on a wall face by that wall's thickness,
  to reach the outside faces. When the walls do not enclose the point, or the enclosure cannot be found, the tool
  says so in the prompt and the click starts a polygon instead.

**Slab.** The outline (counter-clockwise, simplified) with the slab build-up from the settings, else the document's
ground slab on the lowest storey and its floor slab elsewhere; top at the storey floor plus the set offset.

**Roof.** The drawn outline grows by the overhang (each edge moved out, corners mitred). Kind:

- *Flat*: a flat roof with the parapet from the settings.
- *Mono-pitch*: one sloped edge — the edge nearest the first click (between equally near edges, the longer one).
- *Gable*: two sloped edges — for four edges, the opposite pair with the larger total length; otherwise the two
  longest edges.
- *Hip*: every edge sloped.

All sloped edges take the set pitch. The flip key moves the sloped edge of a mono-pitch roof to the next edge, and
switches a gable roof to the other pair. The eaves sit at the set height above the storey floor, else at the storey
height. The build-up is the setting, else the document's flat or pitched roof default.

**Space.** Name and category from the settings. With automatic numbering, numbers are `<storey>.<nn>`: the storey
index counts from the first storey above ground (0, 1, 2; below ground −1, …) and `nn` continues from the highest
number with that prefix on the storey. Picked spaces keep the click point as the position of their label.

**Fill.** A filled region with the drafting style from the settings (hatch, colour, background).

**Plot.** Sets the site's plot boundary; every other site field (terms, georeference, terrain) is kept.

**Measure.** Clicks add points; the fields show the current segment, its angle, the total length and, from three
points on, the enclosed area. Clicking the first point (or Enter, or a double click) closes the polygon and the
prompt reports area and perimeter. Escape starts again. Nothing is created.

**Preview.** The element is drawn as the plan will draw it (spaces with their label and area), with the area of
slabs, roofs, fills and the plot written at the centroid; invalid outlines are drawn in the warning colour with the
reason.

## Edge cases

- Outlines with fewer than three points or no area are refused with an explanation.
- Consecutive duplicate points are ignored.

## Acceptance tests

1. A 5 × 4 polygon closed on its first vertex creates a 20 m² counter-clockwise slab.
2. Rectangle and Enter-closed polygons create slabs; on the lowest storey the slab uses the ground slab build-up.
3. Pick inside without detectable enclosure explains why and continues as a polygon.
4. A 10 × 6 hip roof with 0.5 m overhang has an 11 × 7 outline and four slopes of the set pitch at the storey
   height.
5. A 10 × 6 gable roof slopes the two long edges; after Flip, the two short edges.
6. A mono-pitch roof drawn from (0,0) slopes the longer edge at that corner (the south edge).
7. A flat roof gets the parapet and the flat-roof build-up.
8. Two spaces on the ground floor are numbered 0.01 and 0.02; the first on the floor above is 1.01.
9. The plot tool keeps the building coefficient when it sets a 250 m² plot.
10. Measuring a 3-4-5 triangle reports a 6.00 m² area and a 12.00 m perimeter and creates nothing.

## Open questions

- Slab edges could optionally stop at the wall core instead of the outside face.

---

## Review (completed by the reviewer)

- [x] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [x] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [x] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [x] No screenshots or images of the reference product.
- [x] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [x] Describes behaviour, not implementation details of the reference product.

Reviewed by: Pierros Papadeas, 2026-10-10 — Result: approved

Note: the reviewer is also the author of record. Clean-room protocol §2 asks for a second reviewer; add one when a second contributor joins, before the first public release.
