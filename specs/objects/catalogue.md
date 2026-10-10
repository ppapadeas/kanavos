---
id: SPEC-objects-001
title: Object catalogue, parameters, coordinates and plan conventions
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ISO 128-2:2020 and ISO 128-3:2020 (line types; ΕΛΟΤ EN ISO 128 in Greece)
  - ISO 128-20 line widths 0.13 / 0.18 / 0.25 / 0.35 / 0.50 / 0.70 mm
  - ISO 7519:1991 (construction drawings, general arrangement presentation)
  - ISO 16739-1:2024 (IFC 4.3) entity and predefined-type names
  - General architectural drafting practice in Greece and Europe
---

# Object catalogue, parameters, coordinates and plan conventions

## Purpose

Architects place doors, windows, plain openings, furniture and fixtures from a built-in library. Each object must
look right in plan at 1:50 and 1:100, give a convincing 3D model, and adapt to its parameters, without the user
drawing anything twice.

## Behaviour

### Catalogue

- Every object has a stable identifier `<category>.<name>` (e.g. `door.single_swing`) stored in documents. An
  identifier, once released, is never removed or renamed.
- Every object has a display name and every parameter a label, each a message-catalogue key with Greek and
  English text (`objects.<id>`, `objects.param.<key>`, `objects.category.<category>`).
- Categories: doors, windows and plain openings (hosted in walls); furniture, kitchen, sanitary fixtures,
  equipment, plants and vehicles (free objects).
- Every object declares the IFC entity and predefined type it exports as (for example a WC is a sanitary
  terminal of type toilet pan; a tree is a geographic element of type vegetation).

### Parameters

- Openings take width, height and sill from the opening itself. Free objects take width, depth and height from
  parameters `width`, `depth`, `height`, defaulting to the catalogue size and limited to the object's size range.
- Further parameters have a kind (length in metres, whole number, on/off, angle in degrees), a default, a minimum
  and a maximum. Percentages (leaf glazing) are whole numbers.
- A stored value is made valid before use: a missing value takes the default; a non-finite value takes the
  default; any other value is limited to the range; counts and on/off values are rounded.
- Unknown parameters are ignored. An unknown object identifier draws its box with a cross and a plain box in 3D.

### Coordinates

- Openings: `x` runs along the host wall, the opening spanning `−width/2 … +width/2`; `y` runs across the wall,
  positive towards the wall's left face; `z` is up from the sill. The opening's width and height are the hole in
  the wall; frames sit inside the hole.
- Default orientation: door leaves and window sashes open towards the wall's **left** face, which is the inside
  of a wall drawn anticlockwise around a building. A window's outer face (external sill, shutters, roller box)
  is the **right** face. Single hinges sit on the jamb at `x = −width/2`.
- "Swap side" exchanges the two faces; "swap hand" exchanges the two jambs. Both are exact mirror images of the
  default: the four combinations give four reflections of one drawing.
- Free objects: origin at the insertion point, `x` right, `y` back in plan, `z` up from the object's base.
  Objects that stand against a wall have their origin at the middle of their front edge and extend over
  `0 … depth` in `y`, their back against the wall. Free-standing objects (tables, office chair, island, trees,
  shrub, car) are centred on the origin. A car's front points towards +y.

### Plan symbols

- Layers: `door.symbol`, `window.symbol`, `opening.symbol`, `object.symbol`, with suffixes a view may hide:
  `.swing` (swing arcs and sliding arrows), `.above` (dashed parts above the cut plane), `.sill` / `.threshold`,
  `.shutter`.
- Line weights: door leaves 0.35 mm; frames, sashes and cut profiles 0.25 mm; glass, swing arcs, sills and
  furniture outlines 0.18 mm; furniture details 0.13 mm. Parts above the cut plane (wall cabinets, a wall-hung
  water heater, roller boxes, overhead garage-door tracks, lintels of plain openings, a parasol) are dashed
  (0.18 mm). Parts hidden inside a wall or under a worktop use short dashes (0.13 mm).
- Detail follows the scale: at 1:50 and larger symbols show frame and sash profiles, leaf thickness, pillows,
  cushions, burner rings and similar detail; at smaller scales leaves become single lines, frames become their
  outline, and textures are left out. Sizes that must read the same on paper (arrow heads) are in paper mm.
- An opening wholly above the cut plane (a high window) can be drawn as its frame band and glass, dashed.

### 3D parts

- Each object returns closed meshes, one per colour: white frames, timber leaves, glass with alpha about 110,
  light grey sanitary ware, marble sills, satin-metal handles, muted foliage. Glass is the only transparent part.
- Doors are shown closed in 3D; external shutters are shown opened against the façade, as in plan.

## Edge cases

- An opening narrower or lower than 5 cm draws only its jamb lines and has no parts.
- A wall thinner than 2 cm is treated as 2 cm thick for drawing.
- Frame depths and setbacks larger than the wall are limited to the wall.
- An opening whose style belongs to another kind (a window style on a door) is drawn with its kind's default style.

## Acceptance tests

1. Given the catalogue, then identifiers are unique, every name and parameter label exists in Greek and English,
   and every default lies within its range.
2. Given every opening style in a 30 cm wall, when drawn, then items lie on the style's layer prefix, are finite,
   and stay within one width (or height, for overhead tracks) of the opening.
3. Given every opening style, when swap hand is set, then the plan is the mirror image in `x`; when swap side is
   set, the mirror image in `y`; both together, the point reflection.
4. Given every object, when drawn and built, then every part is a closed mesh with positive volume and the 3D
   parts lie within the plan footprint (plus 15 cm for handles and mirrors).
5. Given extreme parameter values (zero, negative, not a number, very large), then nothing fails and every
   coordinate is finite.
6. Given the same object at 1:100, then the symbol has no more items than at 1:50, and for most objects fewer.
7. Visual check: a test renders every object's plan at 1:50 and 1:100 and a shaded 3D view to PNG sheets for
   review, plus a door and windows in a wall in all four swap states.

## Open questions

- Should objects carry an optional text label (e.g. appliance abbreviations) once views pass a locale?
- Should a view hide the swing layer automatically at 1:200?

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
