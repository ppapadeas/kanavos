---
id: SPEC-elements-005
title: Space detection, storey footprint and building outlines
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ν. 4067/2012 (ΝΟΚ), άρθρο 2 (ορισμός κάλυψης) — footprint used for coverage
  - specs/elements/wall-junctions.md
  - specs/geometry/polygon-operations.md
---

# Space detection, storey footprint and building outlines

## Purpose

Spaces are created by clicking inside a room; area tables, coverage diagrams, slabs and roofs start from what the
walls enclose.

## Behaviour

- **Bounding elements**: walls and columns of the storey that stand at its floor (base below the cut plane, top
  above the floor). Whole wall outlines after junction clean-up are used, so doors and windows do not break rooms.
- **Rooms**: every region enclosed by the union of the bounding elements; columns and free-standing walls inside a
  room become holes (islands).
- **Space at a point**: the smallest room containing the point. A point inside a wall or column, or in a region not
  closed by walls, is reported as an error.
- **Storey footprint** (for coverage): walls, columns and slabs of the storey unioned by their outer faces. Rooms
  enclosed by walls are filled. When the storey has slabs, openings in them (light wells, courtyards) remain holes,
  except where walls stand in them.
- **Building outline** from walls: the outer boundary of the union of the walls (outer face) or of their core
  layers (core face); free-standing walls inside are ignored; separate buildings give separate outlines. An outline
  can be grown by an overhang with mitred corners for eaves and slab edges.

## Edge cases

- Walls that do not quite meet (gap above the 2 mm junction tolerance) leave the room open.
- Nested rooms (a closet inside a room) are separate rooms; the closet's walls are an island of the outer room.

## Acceptance tests

1. A 10 × 8 m house of 31 cm exterior walls with a 13 cm wall at x = 4 and a 30 cm column in the right room: the
   left room is 3.78 × 7.69 m²; the right room 5.78 × 7.69 − 0.09 m² with one hole; two rooms in all; points in a
   wall or outside fail.
2. Its footprint and outer outline are 10.31 × 8.31 m²; grown by 0.5 m, 11.31 × 9.31 m².
3. With a slab over the house that has a 1 × 1 m light well, the footprint loses 1 m².

## Open questions

- Room-bounding lines (virtual separations) are not modelled yet.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
