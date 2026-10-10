---
id: SPEC-elements-003
title: Roof planes, roof lines and roof solids
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - specs/geometry/straight-skeleton.md (Aichholzer et al. 1995; Eppstein & Erickson 1999)
  - General professional knowledge of pitched and flat roof construction
---

# Roof planes, roof lines and roof solids

## Purpose

A roof is drawn as its eaves outline with a pitch (or a gable) per edge; Kanavos derives the planes, the ridge, hip
and valley lines, the plan representation, the 3D solid and the true roof area.

## Behaviour

### Planes

- Flat roof: one horizontal plane over the outline at the roof's base height.
- Pitched roof: each sloped edge carries a plane through the edge at the base height, rising inwards at the edge's
  pitch; gable edges (no pitch) carry no plane.
- Convex outlines: each plane covers the part of the outline where it is the lowest (lower envelope), so the roof is
  exact and needs no event processing. Collinear edges with the same pitch share one plane.
- Other outlines: the weighted straight skeleton with speed cot(pitch) (gables at speed zero) gives the planes;
  hips and valleys follow automatically, weighted by the pitches.
- Each plane reports its edges, plan region, plane equation, pitch and downhill direction.

### Holes

- A roof may have holes: polygons in plan for light wells, skylights, chimneys or a courtyard. Each must enclose an
  area, must not cross itself, must lie inside the outline (touching it is allowed), must not overlap another hole
  (touching is allowed), and some roof must remain. On a flat roof with a parapet, holes stay clear of the
  parapet band. Overlaps up to 1 mm² count as touching.
- The planes are computed for the outline as if there were no holes; each hole is then cut vertically through
  them. The planes keep their slope, so a hole's sides are plumb and its edges on the top surface follow the
  planes. A courtyard whose planes slope down towards it (with its own eaves) is a different roof form and is not
  modelled.
- Planes left without area by a hole are dropped.

### Lines

- Between two planes: ridge (horizontal), hip (convex fold), valley (concave fold), step (planes of different pitch
  meeting at a vertical step).
- On the outline: eave (the horizontal lower edge of a plane) and verge (a gable edge, split where the planes above
  it change).
- Around holes: opening lines along every hole edge on the top surface, split where the planes under them
  change. A hole across a ridge, hip or valley interrupts that line.

### Plan

- Lines whose height is below the cut plane are seen (drawn), above it dashed; a line crossing the cut is split.
- Where the roof slab crosses the cut plane (bottom below, top above), the band is cut and hatched with the core
  material.

### Solids and quantities

- Thickness is measured perpendicular to each plane (vertical thickness = thickness / cos pitch). One closed solid
  per build-up layer; plumb cuts at eaves (fascia) and gables; steps where neighbouring planes of different pitch
  have different undersides.
- Flat roofs: stacked layers; a parapet is the outline minus its inward offset by the parapet thickness, from the
  roof top up by the parapet height.
- True area per plane = plan area / cos pitch; total true area; plan area; volume of each layer. Holes are
  subtracted from all of them; the roof's perimeter adds the hole edges (where flashings run) to the outline.
- Walls under a hole are not capped by the roof there.

## Edge cases

- A set of gables that leaves part of the outline unreachable by any plane is reported as an error.
- Very small planes (below 10⁻⁹ m²) are dropped.

## Acceptance tests

1. Mono-pitch 10 × 6 at 20°: one plane of 60 m², height 3 + 6 tan 20° at the high side, one eave.
2. Gable 10 × 6 at 30°: two planes of 30 m², one ridge of 10 m at 3 + 3 tan 30°, two eaves, four verges.
3. Hip 10 × 6 at 35°: four planes, four hips, one ridge, four eaves; true area 60 / cos 35°.
4. L-shaped hip roof (75 m²): six planes, one valley, two ridges at 3 + 2.5 tan 30°, five hips.
5. The same L with gables at the arm ends: four planes, one valley, one hip.
6. Pitches 45° and 26.57° on a gabled 10 × 6 box: the ridge 2 m from the steeper eave.
7. A hip roof with the 28 cm pitched build-up: five closed layer solids, total volume 60 × 0.28 / cos 30°.
8. A flat 10 × 6 roof with a 0.6 × 0.2 m parapet: parapet volume (60 − 9.6 × 5.6) × 0.6.
9. A 2 × 1 m skylight inside the south plane of the 10 × 6 hip roof at 30°: four planes of 58 m² together, four
   opening lines (the lower one at 3 + 0.5 tan 30°), plan area 58, true area 58 / cos 30°, perimeter 32 + 6 m,
   five closed layers of total volume 58 × t / cos 30°, footprint with the hole; a cut above everything shows
   4 eaves + 4 hips + 1 ridge + 4 opening lines.
10. A 2 × 1 m hole across the ridge of the 10 × 6 gable roof: two planes of 29 m², two ridge pieces of 4 m, six
    opening lines, closed layers.
11. A 4 × 2 m courtyard in a flat 10 × 6 roof with a 0.6 × 0.2 m parapet: layers of volume 52 × t; the parapet
    unchanged.
12. A 0.3 m wall 8 m high crossing a 2 × 2 m hole in a flat roof: 2 m of it stops at the roof's underside, 2 m
    rises to 8 m through the hole; the solid is closed.
13. Refused: a hole partly outside the outline, two overlapping holes, fewer than three distinct points, a ring
    that crosses itself, a hole covering the whole roof, a hole in a flat roof's parapet band. Two holes that touch
    are accepted.

## Open questions

- Dormers are not covered. Courtyards whose roof planes slope down towards the courtyard (eaves around the hole)
  are not covered; holes are cut vertically through planes computed for the outline.

---

## Review (completed by the reviewer)

- [x] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [x] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [x] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [x] No screenshots or images of the reference product.
- [x] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [x] Describes behaviour, not implementation details of the reference product.

Reviewed by: Pierros Papadeas, 2026-10-10 — Result: approved

Revised on 2026-10-10 after that review: holes (section Holes, the opening lines, quantities and wall caps), acceptance tests 9–13. The revision was approved by Pierros Papadeas on 2026-10-10.

Note: the reviewer is also the author of record. Clean-room protocol §2 asks for a second reviewer; add one when a second contributor joins, before the first public release.
