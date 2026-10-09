---
id: SPEC-tools-006
title: Placing columns, objects, beams, stairs and railings
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §3 (drawing is direct)
  - SPEC-tools-001, SPEC-tools-002
  - General drafting and stair design practice (contributors' own professional knowledge)
---

# Placing columns, objects, beams, stairs and railings

## Purpose

Place point-like and linear building parts with the fewest clicks, rotated and sized from the context bar.

## Behaviour

**Columns and objects.** A click places the element at the snapped point with the rotation from the settings. The
rotate key turns the next placements by 90° per press; for objects the flip key mirrors them (for columns it also
turns them by 90°). Pressing and dragging places the element at the press point and takes the rotation from the
drag direction. With "pick rotation" set, a click places it and a second click sets the rotation towards the
pointer. A typed rotation in degrees overrides all of these (Enter places at the pointer, or at the picked point).
Columns take the profile and material from the settings (else the structural material) and the storey height;
objects take the catalogue id from the settings.

**Beams.** A chain like walls: each click after the first adds a beam from the previous point; Backspace removes the
last beam with an undoable delete. The beam top sits at the set offset, else at the storey height.

**Stairs.** Click the midpoint of the first riser, then a point in the walking direction (or type the direction in
degrees and press Enter). Shape, width, going, waist thickness and the gap of half-turn stairs come from the
settings. The stair rises one storey height with the fewest risers that keep each riser at or below the set maximum
(never fewer than two). The prompt reports risers, riser height, going and run while placing; the preview shows the
treads and walking line.

**Railings.** Click the points of the path; Enter or a double click ends it; clicking the first point closes the
path into a loop. Height, infill and post spacing from the settings.

## Edge cases

- Without any material in the document, placement is refused with an explanation.
- Clicking the stair origin twice does not create a stair with no direction.

## Acceptance tests

1. A column click at (1,1) has no rotation; a drag from (4,1) to (4,3) gives 90°; after the rotate key the next
   column is turned 90°. Columns are as tall as the storey (3.20 m).
2. With pick rotation, a second click towards (−1,1) from (0,0) gives 135°; typing 30 and Enter gives 30°.
3. An object after rotate and flip keys is rotated 90° and mirrored, with the catalogue id from the settings.
4. Beams chain; Backspace deletes the last one; beam tops sit at the storey height.
5. A straight stair from (1,1) towards (1,5) faces 90° with 18 risers for a 3.20 m storey and 18 cm maximum riser.
6. A half-turn stair with a typed direction of 0° is created facing east.
7. A railing through three points ends with Enter and keeps the three points.

## Open questions

- Stairs could snap their run to fit between two clicked points by adjusting the going.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
