---
id: SPEC-elements-001
title: Wall junctions and layer clean-up
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - General professional knowledge of layered wall construction and its representation in plans (1:50, 1:100)
  - specs/geometry/polygon-operations.md
---

# Wall junctions and layer clean-up

## Purpose

Where walls meet, the plan must read as built: corners closed, layers continuous, the stronger material running
through the weaker one. The architect draws walls along reference lines and expects the junctions to clean up by
themselves.

## Behaviour

### Detection (per storey, walls whose height ranges overlap)

- **Node**: wall ends within 2 mm of each other.
- **Tee**: a wall end inside another wall's band (between its faces, within its length, 2 mm tolerance), not at one
  of that wall's ends.
- **Cross**: the reference lines of two walls cross at points inside both walls (more than 2 mm from their ends).
- Ends whose directions differ by less than 10° overlap and are not joined.

### Corner (two ends at a node)

- Directions opposite within 2°: **continuation**; both walls end square at the node.
- Same core priority: every layer runs to the **joint line** through the intersection of the two outer faces and the
  intersection of the two inner faces. Equal build-ups therefore mitre layer by layer; different thicknesses meet
  face to face. If a face intersection lies further than six times the thicker wall from the node, the joint
  follows the bisector of the two walls instead.
- Different core priority: the stronger wall runs to the far face of the weaker one, which abuts it as at a tee.

### Tee

- The abutting wall's outline stops at the near face of the through wall.
- Each layer of the abutting wall continues into the through wall across the through wall's layers, counted from
  the near face, whose junction priority is strictly lower than its own; it stops at the first layer of equal or
  higher priority. The through wall's layers that it crosses are cut where it passes.
- Example: an interior wall (plaster | brick 60 | plaster) against an exterior wall (render 10 | insulation 20 |
  brick 60 | plaster 10): the interior brick runs through the render and the insulation to the exterior brick;
  the interior plasters stop at the face.

### Nodes of three or more ends

- The two strongest walls (core priority, then thickness) decide. If they continue each other they run through
  and the others abut them as tees. If they meet at an angle and are stronger than every other wall at the node,
  they form a corner and each other wall abuts the one of them it meets most squarely.
- Otherwise a collinear pair (the strongest one) runs through and the others abut; without a collinear pair every
  wall ends at the two joint lines it shares with its angular neighbours, each through the node and the
  intersection of the neighbouring faces.

### Cross

- The wall with the higher core priority runs through (then the thicker, then the older). The other is interrupted
  at the through wall's faces, except that each of its layers continues across the through wall's layers of lower
  priority from each side, as at a tee; a layer that out-ranks all of them is not interrupted.
- The crossing wall's outline (its footprint) is not split.

### Columns

- A column standing in a wall cuts the wall layers whose priority is lower than the column material's.

### Results

- The wall outline after clean-up (one polygon), the layer bands (several per layer when a layer is interrupted),
  the face polylines and the list of junctions of the wall.
- Arc walls follow the same rules with circular faces.
- Computing all walls of a storey at once gives exactly the same layouts as computing walls one by one.

## Edge cases

- A wall with both ends at the same node is ignored for junctions.
- Very acute corners fall back to the bisector joint.
- Walls drawn in opposite directions are handled: faces are matched by side, not by name.

## Acceptance tests

1. Two 31 cm exterior walls in an L, centred references at (0,0)–(10,0)–(10,8): corners at (10.155, −0.155) and
   (9.845, 0.155); each outline keeps length × thickness; every layer wraps the corner as one region.
2. 30 cm and 10 cm walls in an L: outer corner (6.05, −0.15), inner corner (5.95, 0.15).
3. A concrete wall and a brick wall in an L: the concrete wall reaches the brick wall's far face.
4. An interior wall at a tee: outline stops at the near face; its brick reaches 9 cm into the exterior wall; the
   exterior render and insulation are cut there, the exterior brick and plaster are not.
5. Two equal interior walls crossing: the newer is interrupted; its brick runs 2 cm into the other's plasters on
   each side.
6. Four ends at a point: a plus shape without overlaps. Three walls at 120°: no overlaps.
7. A closed ring of four exterior walls: one outline of 10.31 × 8.31 m with one hole of 9.69 × 7.69 m.
8. A storey with seven walls: storey-wide and single-wall layouts are identical.
9. A straight wall meeting an arc wall: no overlap between their outlines.

## Open questions

- Junction priorities per layer pair (overriding material priorities) are not modelled.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
