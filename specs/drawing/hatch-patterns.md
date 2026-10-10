---
id: SPEC-drawing-001
title: Hatch patterns for cut materials and surfaces
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ISO 128-50:2001 (representing areas on cuts and sections, general conventions)
  - ISO 128-20:1996 (line widths)
  - Professional drafting practice for architectural plans and sections at 1:20–1:200
---

# Hatch patterns for cut materials and surfaces

## Purpose

People read a plan or section by the hatch inside each cut area: masonry, concrete, insulation and earth must be
told apart at a glance, look the same at every scale, and print identically from every output (screen, PNG, SVG,
PDF, DXF).

## Behaviour

All lengths are **paper millimetres** at hatch scale 1. In a model drawing at 1:N they become N/1000 metres. A fill's
`scale` multiplies every length; its `angle` rotates the whole pattern (radians, counter-clockwise). Patterns are
anchored at the drawing origin, so hatches of neighbouring areas continue across shared edges.

The filled area is the even-odd combination of its rings: each ring toggles inside/outside, so holes need no
particular orientation.

| Pattern | Use | Definition |
| --- | --- | --- |
| none | background colour only | no lines |
| diagonal | general cut | lines at 45°, spacing 2.0 |
| masonry | brick and block | lines at 45°, spacing 0.8 |
| cross-diagonal | reinforced concrete | lines at 45° and 135°, spacing 1.6 each |
| concrete | plain concrete, screed | 5 × 5 tiles of 3 × 3 cells; each cell holds a dot (0.2 long stroke, 60 %), a small triangle (side 0.6–0.9, 25 %) or nothing (15 %), positions and turns varied per tile |
| insulation | soft insulation | zigzag along the area's longest edge spanning its thickness (see below) |
| wood | timber, boards | lines at 0°, spacing 1.2, dashes 7.0 on / 1.0 off, each line shifted 3.1 along itself |
| grid | tiles | lines at 0° and 90°, spacing 3.0 |
| dots | plaster, render, sand | lines at 0°, spacing 1.2, dashes 0.2 on / 1.0 off, each line shifted 0.6 (staggered dots) |
| earth | earth, fill | 4 × 4 tiles with two groups of three 1.3 long strokes at 45°, 0.45 apart, on the tile diagonal |
| gravel | gravel | 3 × 3 tiles of 2 × 2 cells, one circle per cell, radius 0.28–0.50, position varied per tile |
| horizontal | glass, water | lines at 0°, spacing 1.5 |

Pen widths: 0.18 for dots and concrete, 0.13 for every other pattern (ISO 128-20 series). Lines use butt caps.

**Insulation band.** The band direction is the direction of the longest edge of the area. Across that direction the
area's extent is its thickness *t*. If *t* ≤ 15 mm (paper × scale) the zigzag is one row spanning the full thickness;
otherwise the thickness is divided into rows of about 8 mm. The zigzag period is 0.6 × the row height and is
anchored on a grid of that period along the band, then clipped to the area.

**Variation.** Concrete and gravel vary each tile with a fixed hash of the tile's grid position. The result looks
irregular, never repeats visibly at tile size, and is identical on every run and every machine.

**Clipping.** Line patterns use an exact scanline: an edge spans `[min y, max y)` along the line normal, so a line
through a vertex is counted once and never doubled or lost. Motif segments (concrete, earth, gravel, insulation) are
split at every boundary crossing and kept where the middle of the piece is inside.

**Limits.** An area that would need more than 250 000 segments is refused; outputs then draw its background only.

## Edge cases

- Rings with fewer than three points or non-finite coordinates are ignored.
- A non-positive or non-finite `scale` is treated as 1; a non-finite `angle` as 0.
- A hatch line that coincides with a horizontal boundary edge (in the pattern's frame) belongs to the side above it.
- Insulation in an L-shaped area follows only its longest edge.

## Acceptance tests

1. Given a 40 × 40 square with a 20 × 20 hole, when any pattern is expanded, then every segment midpoint is inside
   the area and none is inside the hole, and expanding twice gives identical output.
2. Given a 20 × 20 square and the diagonal pattern, then the total line length is 200 ± 3 (area / spacing) and every
   line is at 45°.
3. Given a diamond whose vertices lie on hatch lines, then each line through a vertex gives exactly one segment.
4. Given a model drawing at 1:50 and the horizontal pattern at scale 2, then lines are 0.15 m apart.
5. Given two squares side by side, then every diagonal line ending on the shared edge continues in the neighbour.
6. Given a 10 m × 0.10 m layer at 1:20 rotated 30°, then the insulation zigzag runs along it and spans its thickness.
7. Given a 1 km square at 1:1 with a 10 000 segment limit, then expansion is refused.

## Open questions

- Should a project be able to define its own patterns (a table of line families and motifs)?

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
