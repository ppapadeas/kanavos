---
id: SPEC-tools-003
title: Drawing walls
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §1 and §3 (drawing is direct, typed input)
  - SPEC-tools-001, SPEC-tools-002
  - General drafting practice (contributors' own professional knowledge)
---

# Drawing walls

## Purpose

Draw the walls of a building quickly and exactly: as a chain, as a rectangle, or curved, with the build-up, height
and reference line chosen in the context bar.

## Behaviour

**Chain mode.** The first click sets the start. Every further click (or typed point and Enter) creates a straight
wall from the previous point and continues the chain from the new point. A click within the snap radius of the
chain's first point (after at least two points) closes the chain exactly on that point and ends it. Enter with
nothing typed, a double click or Escape ends the chain. Each wall is created as soon as it is drawn, so it joins its
neighbours and appears in every view at once; each is its own undo step.

**Backspace** with nothing typed removes the chain's last wall with an undoable delete and continues from that
wall's start; at the start of the chain it ends the chain.

**Rectangle mode.** Two opposite corners (or the first corner and typed Δx and Δy, towards the pointer's quadrant)
create four walls as one undo step. The walls run clockwise starting with the side at the smaller x, so the
build-up's first layer — its left face, which the starter build-ups use for the outside — faces out. With the
reference line on the left face, the clicked rectangle is the outside of the walls.

**Arc mode.** Click the start, the end, then a point the wall passes through, or type the rise of the arc above its
chord (on the pointer's side). Arcs of more than a half circle are refused with an explanation. The chain continues
from the end.

**Reference side.** The flip key swaps a left reference line for a right one (and back) for the walls that follow.

**Defaults.** Without a build-up in the settings, the document's default exterior wall is used; without a height,
the storey height.

**Preview.** The wall being drawn is shown as it will be drawn in the plan (layers and hatches, joined with nearby
walls), with its length written along it and the fields showing length, angle and offset. A wall that cannot be
built is drawn in the warning colour with the reason.

## Edge cases

- A click on the previous point adds nothing.
- Changing the mode in the context bar abandons the operation in progress.
- Typed polar or relative input works for every point, including the closing point.

## Acceptance tests

1. Clicks at (0,0), (5,0), (5,4), (0,4), (0,0) create four walls and end the chain; each was its own step.
2. Enter, double click and Escape end the chain.
3. Backspace after two walls removes the second and the chain continues from (4,0).
4. Rectangle from (0,0) to (6,4): one batch of four walls, the first from (0,0) to (0,4), clockwise.
5. Rectangle with typed 8 and 6 towards the upper left of (1,1) reaches x = −7 and y = 7.
6. Arc from (0,0) to (4,0) through (2,−1) gives a positive bulge passing through (2,−1); a typed rise of 0.5 gives
   an arc through the point 0.5 m off the chord.
7. Flip turns a left reference line into a right one.

## Open questions

- Should the chain close automatically when the pointer snaps to the first point while Enter is pressed?

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
