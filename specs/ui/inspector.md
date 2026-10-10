---
id: SPEC-ui-003
title: Inspector, value fields and multi-selection editing
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §1, §2, §3 (typed input)
  - ISO 6946 (U-value of layered build-ups)
---

# Inspector, value fields and multi-selection editing

## Purpose

Let people read and change the properties of the selection directly, with the same typed input they use while
drawing, and change many elements at once.

## Behaviour

### Value fields

- A length field shows metres with two decimals, or three when the millimetre matters (`0,125`), in the
  interface language's decimal separator, followed by a muted unit.
- Focusing the field selects its text. The user may type: `3,20` or `3.20`; unit suffixes `m`, `cm`, `mm`
  (`320cm`); arithmetic with `+ − × ÷` and parentheses (`3,20+0,25`, `2*1,5`). `Enter` or leaving the field
  commits; `Esc` abandons. A value the field cannot read leaves the old value and outlines the field in red,
  with a tooltip explaining the accepted forms.
- Thickness fields of build-up layers show centimetres: bare numbers are centimetres (`8`, `2*9`), explicit
  units are honoured (`80mm`).
- Number fields (angles, coefficients, counts) accept either decimal separator and arithmetic, ignore a trailing
  `°` or `%`, and reject unit letters.
- Committing the same value (within 0,1 mm) does nothing.

### Selection header

- One element: its category icon and name, its mark (tag) and id.
- Several: "n elements", followed by one chip per category with its count ("Walls 3"); clicking a chip narrows
  the selection to that category. A close button clears the selection.

### Sections per element type

Grouped by what people change most; each section collapses.

- Walls: build-up (or single material with thickness), thickness and U-value (read-only), reference line;
  height, base offset, length (straight walls), start and end coordinates (single selection).
- Doors, windows, openings: style (catalogue entries of the same kind, by localised name), width, height,
  sill, head height (read-only); position along the host wall, swap opening side, swap hinge side, host wall;
  catalogue parameters (lengths, counts, toggles, angles) with their limits.
- Slabs: build-up, top offset; area and number of holes (single).
- Columns: profile (rectangular width × depth, or circular diameter), material, height, base offset, rotation,
  position (single). Beams: profile, material, top offset, length (read-only), start and end (single).
- Stairs: shape, width, total rise, number of risers, riser height (read-only), going, gap between flights,
  waist, material, base offset, direction, origin (single).
- Roofs: flat or pitched, base, build-up; flat: parapet on/off with height and thickness; pitched: one pitch
  for all sloped edges, and for a single roof each edge as sloped (with its pitch) or gable.
- Railings: height, base offset, post spacing, infill, material, length (read-only).
- Spaces: name, number, ΝΟΚ category, own height (otherwise the storey's), floor finish; area (total for
  several), perimeter.
- Objects: catalogue entry, width, depth, height and catalogue parameters; rotation, height above floor,
  mirrored, position (single).
- Text: content, size in paper mm, rotation, alignment, bold. Dimensions: direction, number of points. Level
  marks: automatic value or a fixed one, label. Drafting: line weight, line pattern, colour, hatch for fills.
  Grid lines: label, start, end. Underlays: name, source file.
- Every type: mark (tag), phase (new, existing, to be demolished), storey (not for openings, which follow
  their wall).
- One element: a collapsed "Quantities" section with length, height, thickness, net and gross area, perimeter
  and volume where they apply.

### Mixed values

- A field over several elements shows the shared value, or "—" when they differ. Typing into a "—" field
  sets the value on every selected element.
- Every edit of one field is one undo step, named "Change n elements" (or "Change properties" for one).
- When the selection mixes element types (other than doors and windows, which share the opening sections), only
  the shared identity fields are shown, with a hint to narrow the selection.

## Edge cases

- Choosing "single material" for a layered element keeps its total thickness and uses its core material.
- Changing an object's catalogue entry clears its parameter overrides.

## Acceptance tests

1. `3,20`, `320cm`, `3.20+0.25`, `2*1,5`, `250mm`, `(3+1)/2` read as 3,20 / 3,20 / 3,45 / 3,00 / 0,25 /
   2,00 m; empty, "—", words and `3,2,1` are rejected.
2. `22,5°` and `60%` read as numbers; `5cm` is rejected as a number.
3. Layer thickness `8` is 8 cm, `2*9` is 18 cm, `80mm` is 8 cm.
4. Two walls with heights 3,00 and 2,50 show "—"; setting 2,80 produces two updates, both 2,80.
5. Updates to the same element from different fields merge into one command.

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
