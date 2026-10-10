---
id: SPEC-tools-002
title: Typed input while drawing, with ΕΓΣΑ87 coordinates
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §3 (drawing and typed input)
  - EPSG:2100 (ΕΓΣΑ87 / Greek Grid) definition
  - General drafting practice (contributors' own professional knowledge)
---

# Typed input while drawing, with ΕΓΣΑ87 coordinates

## Purpose

Architects type exact values while drawing, without dialogs: lengths with arithmetic and units, directions, offsets
from the previous point, project coordinates and national-grid coordinates from the survey.

## Behaviour

**Fields.** While a tool waits for a point it shows fields next to the pointer, e.g. length, angle and offset when
a previous point exists, or position before the first point. Typing goes into the active field; Tab moves to the
next field. A typed field is locked and shown as typed; the others follow the pointer. Enter commits; Backspace
deletes the last character (and, with nothing typed, steps the operation back); Escape clears what is typed, then
backs out of the operation.

**Values.**

| Form | Meaning |
| --- | --- |
| `3.20`, `3,20`, `320cm`, `2500mm`, `3.20+0.25`, `2*(1.5+0.5)` | A length in metres; units m, cm, mm; `+ - * /` and parentheses |
| `@2.5<90` | Polar: 2.5 m from the previous point at 90° (counter-clockwise from east) |
| `@1.2,0.8` or `@1,2;0,8` | Offset from the previous point |
| `3.5,4.0` or `3,5;4` | Absolute project coordinates |
| `#476532.12,4097345.88` | Absolute ΕΓΣΑ87 easting and northing |

A bare `3,4` is the length 3.4 m (decimal comma). Pairs need `;`, or a `.` decimal point beside the `,`.

**Length, angle and offset.** The next point is the previous point plus the length along the direction, plus the
offset to the left of the direction. Without a typed angle the direction follows the pointer; without a typed
length, the pointer's distance along the direction is used.

**National grid.** The site's georeference places the project origin at an easting and northing and gives the angle
from project +Y to grid north, counter-clockwise positive. A grid point (E, N) becomes the project point
R(angle) · (E − E₀, N − N₀). No scale factor is applied: over a building plot the ΕΓΣΑ87 scale factor changes
distances by less than one part in a thousand, and surveys are drawn at grid scale.

**Errors.** Text that cannot be read shows a short explanation with examples; nothing is committed.

## Edge cases

- Polar and relative input before the first point are measured from the project origin.
- A typed bare length before the first point has no meaning and is ignored on Enter.

## Acceptance tests

1. From (1,1), typing `4,5`, Tab, `90`, Enter gives the point (1, 5.5).
2. `3.20+25cm` along +X from (1, 5.5) gives (4.45, 5.5); `@2<270` then `@-1;0` continue the chain.
3. With the georeference at (476500, 4097300) and no rotation, `#476510;4097305` is the project point (10, 5).
4. With a rotation of 90°, the grid point 10 m north of the origin is the project point (−10, 0), and converting
   back gives the original grid coordinates.
5. `3+` shows an explanation; deleting characters clears it.

## Open questions

- Should a typed angle be relative to the previous segment when a modifier is held?

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
