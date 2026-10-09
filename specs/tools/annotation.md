---
id: SPEC-tools-007
title: Dimensions, text, level marks, grid lines, section and elevation lines, and drafting
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §3 and §4 (drawing, permit drawings)
  - SPEC-tools-001, SPEC-tools-002
  - General architectural drafting practice for dimensions, levels, grids and section markers (contributors' own
    professional knowledge)
---

# Dimensions, text, level marks, grid lines, section and elevation lines, and drafting

## Purpose

Annotate plans and draw 2D linework with the same direct, typed, snapped interaction as the modelling tools.

## Behaviour

**Dimensions.** Click the points to measure (snapped); Enter, a double click or clicking the last point again ends
the points. The dimension line then follows the pointer; a click places it, or a typed offset places it that far
from the outermost measured point on the pointer's side. The line runs aligned with the first and last points,
horizontally or vertically; the flip key cycles these. Clicking a wall away from its snap points (as the first
click) measures that wall's face on the clicked side: its two ends and the jambs of its openings.

**Text.** A click opens a text box at the snapped point. Typed characters go into it; Shift+Enter starts a new line;
Backspace deletes; Enter creates the text with the size, alignment and weight from the settings. Clicking elsewhere
creates the current text and opens a new box there; Escape discards it.

**Level marks.** A click places a level mark showing the storey's floor level, or a typed value. The label from the
settings precedes the value.

**Grid lines.** Two clicks make a project-wide grid line, shown on every storey. Lines running mostly east–west are
labelled with the next letter (A, B, … Z, AA, …) after the highest letter in the project; lines running mostly
north–south with the next number (1, 2, 3, …). A label in the settings overrides the sequence.

**Section and elevation lines.** Click the two ends of the line (typed length and angle work for the second), then
click on the side the view looks to. A new section or elevation view is created with the next marker letter not used
by any section or elevation (Α, Β, Γ, … Ω, then Α1, Β1, …), named "Τομή Α-Α" / "Όψη Α" (in the user's language),
with the depth and scale from the settings. The preview shows the marker as the plan will draw it.

**Drafting.** Lines chain like walls (each click adds a line). Polylines collect points and end with Enter or a
double click; clicking the first point closes them. Arcs: start, end, then a point on the arc (or a typed rise).
Circles: centre, then a point on the circle or a typed radius. All use the drafting style from the settings.

## Edge cases

- Grid lines and the plot are not storey elements; everything else is created on the active storey.
- A dimension needs two points; fewer are ignored on Enter.

## Acceptance tests

1. Points (0,0), (3,0), (5,0), Enter, click at (2,1) create a horizontal three-point dimension through (2,1).
2. Points (0,0), (4,3), double click, flip (horizontal), typed 0.8 places the line at y = 3.8.
3. Clicking the left face of a 6 m wall with a 0.90 m door at 2 m measures four points along the face (y = 0.155):
   x = 0, 1.55, 2.45 and 6.
4. Text with Shift+Enter and Backspace commits "Σαλόνι\n25 m"; Escape discards; clicking elsewhere commits.
5. Level marks without a typed value show the storey floor; a typed `+3,20` sets 3.20.
6. Two north–south grid lines are labelled 1 and 2, an east–west line A; grid lines have no storey.
7. Two sections and an elevation get markers Α, Β, Γ; clicking on the right of start → end looks right.
8. Lines chain, a polyline closes on its first point, an arc passes through its third point, a circle takes a typed
   radius.

## Open questions

- Automatic dimension chains for a whole wall run (several walls in line) could follow later.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
