---
id: SPEC-elements-002
title: Openings in plan and wall solids
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - General professional conventions of architectural plans (cut plane, elements below seen, elements above dashed)
  - specs/geometry/solids.md
  - specs/elements/wall-junctions.md
---

# Openings in plan and wall solids

## Purpose

Plans must show doors and windows as gaps where the cut plane passes through them, the wall seen below window sills,
and high windows dashed. 3D, sections and quantities need walls with real holes.

## Behaviour

### Plan at the cut plane

- A wall is cut when its base is below and its top above the cut elevation; walls entirely below are drawn as
  outlines seen from above; walls entirely above as dashed outlines.
- An opening whose sill is below and whose head is above the cut leaves a gap across the whole thickness of every
  layer, between its two jambs (square to the wall; radial for arc walls).
- Through such an opening with a sill above the wall base (a window), the wall below the sill is seen: both faces
  across the opening width are drawn as lines below.
- An opening whose sill is at or above the cut (a high window) is drawn dashed: its outline across the wall.
- The cut parts of a wall are its layer bands after junction clean-up, minus the gaps; their union is given as the
  wall's outline at the cut for a heavy line.
- Where a roof on the storey pulls a wall's top below the cut, that part of the wall is seen from above, not cut.

### Wall solids

- One closed solid per build-up layer, following the layer bands after junction clean-up.
- Every opening is a true rectangular hole through all layers between its jambs, sill and head.
- A wall taller than the underside of a roof on its storey is clipped to the underside (perpendicular roof
  thickness converted to vertical): gable walls follow the roof slope. Parts where the underside is below the wall
  base vanish.

### Quantities

- Gross side area = reference length × height; net = gross minus openings; net area of each face after junction
  clean-up; exact volume of each layer from its solid.

## Edge cases

- Openings beyond the wall's ends are clipped to the wall.
- Doors with zero sill: no sill lines.

## Acceptance tests

1. A 6 m wall, 25 cm, with a door, a window spanning the cut and a high window: three cut pieces of
   0.25 × (6 − 0.9 − 1.2) m², two sill lines, four dashed lines.
2. An arc wall with a door at mid-length: every layer is split in two; the gap is width × thickness.
3. A 5 × 3 m, 25 cm wall with a 1.2 × 1.2 m window: one closed solid of 3.39 m³; net area 13.56 m².
4. An exterior wall with a window and a door, joined at a corner and a tee: four closed layer solids whose volumes
   sum to plan area × height minus the openings.
5. A tall wall under a gable roof is clipped to the roof underside: its peak and volume match the underside.

## Open questions

- Reveal depth and frame position inside the opening belong to the opening objects, not to the wall.

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
