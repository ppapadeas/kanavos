---
id: SPEC-drawing-005
title: Survey drawing import (τοπογραφικό) from DXF
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - DXF reference (published file format documentation)
  - EPSG:2100 (ΕΓΣΑ87 / Greek Grid) definition
  - Unicode Consortium code page mapping tables (Windows-1253, Windows-1252, ISO 8859-7, DOS 737)
  - Professional practice of Greek topographic surveys for building permits
---

# Survey drawing import (τοπογραφικό) from DXF

## Purpose

A permit project starts from the surveyor's drawing: plot boundary, spot heights, neighbouring buildings, roads, in
ΕΓΣΑ87 coordinates with Greek text. People bring it in as an underlay, pick the plot boundary, and get terrain
points with elevations, positioned in project coordinates.

## Behaviour

**Reading.** ASCII DXF from R12 to current versions. Binary DXF is refused with a message asking for ASCII.

**Text encoding.** Files from AC1021 on are UTF-8. Older files use the code page in `$DWGCODEPAGE`: Windows-1253
(`ANSI_1253`), Windows-1252, ISO 8859-7 and DOS 737 are read; an unknown or missing code page falls back to
Windows-1253 (an option). A value that is valid UTF-8 with non-ASCII characters is read as UTF-8 whatever the code
page (Greek Windows-1253 text is never valid UTF-8). `\U+XXXX` escapes and `%%d`, `%%p`, `%%c`, `%%%` codes are
decoded; MTEXT formatting is removed and its paragraphs become separate lines.

**Units.** `$INSUNITS` sets metres per unit; a file without units is taken to be in metres (an option). The result
is in metres.

**Geometry.** Lines, polylines (bulges as arcs), circles, arcs, ellipses, splines (evaluated from control points and
knots, otherwise through fit points), texts, solids and faces, hatch boundaries (solid hatches filled), leaders and
dimension graphics become drawing items on their DXF layers, with colour, lineweight and line type. Block inserts
are expanded with scale, rotation, arrays and nesting (16 levels); entities on layer `0` inside a block take the
insert's layer, BYBLOCK properties take the insert's. Entities in object coordinate systems (including mirrored
ones) are placed correctly. Paper-space and invisible entities are left out by default. Text heights and line type
lengths are converted to paper millimetres at the import scale (default 1:200).

**Survey points.**

1. A POINT is a survey point; a non-zero Z is its elevation.
2. A block insert is a survey point when it has an elevation attribute (tag Z, H, Η, ELEV, ELEVATION, HEIGHT, ALT,
   ΥΨ, ΥΨΟΜΕΤΡΟ, ΥΨΟΣ) or, at the top level, a non-zero insertion Z. A short label attribute (NO, ID, PT, POINT,
   NUM, NAME, ΑΡ, Α/Α, ΣΗΜΕΙΟ…) becomes the point's label.
3. A text that reads as an elevation — a number with one to three decimals, optionally signed, prefixed `H=`, `Η=`,
   `Z=` or followed by `m`, between −500 and 3000 m — and that is not part of a dimension gives its elevation to the
   nearest point without one within max(2 m, three text heights). If the nearest point already has that elevation,
   nothing changes. Otherwise the text becomes a point at its insertion point, marked as approximate.

**Closed polylines** at the top level (not inside blocks), closed by flag or by a repeated last vertex, are listed
with layer, area and perimeter as plot boundary candidates. Areas are computed relative to the first vertex so grid
coordinates do not lose precision.

**Placing in the project.** With the project georeference (grid easting and northing of the project origin,
rotation from project +Y to grid north counter-clockwise, altitude of ±0.00), everything moves to project
coordinates: `project = R(rotation) · (grid − origin)`, elevations relative to ±0.00. Without a georeference, a
round origin near the south-west corner of the drawing (multiples of 100 m) can be used. The drawing becomes an
underlay; a chosen closed polyline becomes the plot polygon (counter-clockwise); points with elevations become
terrain points.

## Edge cases

- Undefined blocks, external references, unsupported entities and over-deep nesting produce warnings, not errors.
- More than two million items: the rest is skipped with a warning.
- A truncated file imports what was read.
- Dimension texts such as `2,00` are not elevations.

## Acceptance tests

1. A Windows-1253 file with Greek layer names and text reads them correctly.
2. A POINT at Z 125.34 gives a point with that elevation; a 2D point with `+126,10` next to it gets 126.10; an
   insert with attributes H = 124.80 and ΑΡ = Σ12 gives a point at 124.80 labelled Σ12; a dimension `2,00` gives
   nothing; three points in total.
3. A closed polyline 30 × 20 m with one bulged edge (bulge 0.2) lists one boundary of 620–632 m².
4. A block inside a block, inserted at ×2 and 90°, lands where expected, on the insert's layer, in its colour.
5. A circle with extrusion (0, 0, −1) at OCS (−480040, 4200030) is at world (480040, 4200030).
6. With origin (480000, 4200000) and altitude 120, the point at Z 125.34 is at project (10, 5, 5.34).

## Open questions

- Should contour lines (3D polylines at constant elevation) also feed terrain data?

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
