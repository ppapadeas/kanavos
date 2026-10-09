---
id: SPEC-tools-008
title: Selecting, moving, rotating, mirroring and editing with handles
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §2 and §3 (on-canvas editing, nothing is modal)
  - SPEC-tools-001, SPEC-tools-002
  - General drafting practice (contributors' own professional knowledge)
---

# Selecting, moving, rotating, mirroring and editing with handles

## Purpose

Pick elements and change them directly on the canvas: drag them, stretch them by their handles, rotate and mirror
them, with snapping and typed values, every change one undo step.

## Behaviour

**Picking.** The element under the pointer is pre-highlighted. Openings win over their walls; small elements win
over the slabs, roofs and spaces they sit in. A click selects it (replacing the selection); Shift adds; Ctrl removes
an already selected element (on release, so a Ctrl-drag can copy instead); a click on empty space clears the
selection. Clicking one element of a multiple selection, without dragging, selects only that element.

**Rectangles.** Dragging from empty space draws a rectangle. Dragged left to right it selects what lies fully inside
(window); right to left also what it touches (crossing). Shift adds to the selection, Ctrl toggles.

**Handles** are shown for up to 30 selected elements:

- Walls: both ends (stretch; walls whose ends meet that end follow it) and the midpoint (moves the wall sideways;
  walls meeting its ends follow). Openings keep their distance from the end that does not move.
- Doors, windows and voids: the centre (slides along the wall) and both jambs (change the width, keeping the other
  jamb).
- Slabs, roofs, spaces, fills, polylines and railings: every vertex (drag) and every edge midpoint (drag to insert a
  vertex; a roof's new edge keeps the slope of the edge it splits).
- Columns, objects, stairs, text, level marks, space labels: their position. Objects are picked anywhere inside
  the box around the plan symbol the object library draws for them (which follows the definition's insertion point
  and includes parts such as chairs or door swings). Beams, grid lines and lines: both ends.
  Circles: centre and radius. Arcs: ends and middle. Dimensions: the line and each measured point.

Dragging a handle snaps (ignoring the elements being changed); a typed length sets the distance from the fixed end
(for a wall end: the wall's new length). The handle under the pointer is highlighted. With Shift or Ctrl held, a
press acts on the selection instead of the handle.

**Moving.** Dragging a selected element moves the selection by the vector from the grabbed point (snapped) to the
snapped pointer, so pressing near an element's end and releasing on another element's end joins them exactly.
Typing while dragging sets the distance along the drag direction (or a relative `@dx;dy`); after release the move
waits for Enter. Ctrl at release keeps the original and selects the copy.

**Rotating** (rotate key): click the centre, then a reference direction, then the new direction; or type degrees
after the centre and press Enter. **Mirroring** (mirror key): click two points of the axis. Ctrl at the last click
keeps the original.

**Other keys.** Delete removes the selection. The flip key flips selected doors and windows to the other side of
their wall; with Shift, to the other hand. Escape backs out of the current operation, then clears the selection.

**Previews and refusals.** Moves, copies, rotations, mirrors and handle edits preview the result as the plan will
draw it. A change the model refuses (e.g. a wall shortened past one of its openings) is not committed; the reason
is shown.

## Edge cases

- Very large selections (over 150 elements) preview as outlines.
- An opening selected without its wall slides along the wall when moved.

## Acceptance tests

1. Hover pre-highlights; click, Shift-click and Ctrl-click (even on a handle) change the selection; clicking empty
   space clears it.
2. A left-to-right rectangle selects only the wall fully inside; right-to-left selects both walls it touches.
3. Pressing 8 px from a wall's end and releasing near another wall's end moves the wall exactly onto that end.
4. Dragging upwards, typing 2.5, releasing and pressing Enter moves the wall by 2.5 m; Ctrl-dragging copies and
   selects the copy.
5. Rotating about (0,0) from (4,0) to (0,4) turns a wall by 90°; typed −90 turns it back.
6. Mirroring with Ctrl about x = 5 keeps the original and adds the mirrored wall from (6,0) to (10,0).
7. Delete removes the selection; Escape clears it.
8. Dragging a wall's end from (4,0) to (5,0) moves the connected wall's start too; a typed 3 makes the wall 3 m
   long.
9. Dragging a wall's midpoint up by 1 m moves it and the connected wall's end.
10. Moving a wall's start keeps its door in place; shortening the wall past the door is refused.
11. A window's jamb handle widens it from 0.90 m to 1.20 m keeping the other jamb; its centre handle slides it.
12. A slab vertex drag and an edge-midpoint drag give a five-vertex outline.
13. A column's position handle moves it.

## Open questions

- A quick-action strip (docs/ux.md §3) will offer split, join, offset and mirror next to the selection.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
