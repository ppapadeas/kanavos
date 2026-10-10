---
id: SPEC-tools-001
title: Snapping and inference while drawing and editing
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §3 (Kanavos interaction model: inference and snapping)
  - docs/architecture.md §7 (performance budgets)
  - General drafting practice (contributors' own professional knowledge)
---

# Snapping and inference while drawing and editing

## Purpose

While a tool asks for a point, the pointer moves to the geometry the architect most likely means, and the canvas
says why in words, so drawing is exact without typing coordinates.

## Behaviour

**Candidates** are gathered per storey: wall reference-line ends and midpoints, the wall centreline, both wall
faces and their midpoints, the end caps, the outline corners, and the boundaries between the layers of a wall's
build-up (with their ends); corners and centres of openings; ends, midpoints and edges of slabs, roofs, spaces,
beams, railings, drafting and grid lines; centres and corners of columns; insertion points of objects, text, level
marks and stairs; lines, corners and centres of underlay drawings (surveys). Grid lines without a storey take
part on every storey.

**Radius.** A candidate is in reach when it is within the snap radius (10 screen pixels by default) of the pointer.

**Order.** For one pointer position the first rule that finds something wins:

1. A point candidate or the crossing of two edges in reach. Among those, the nearest wins, with ends, corners and
   crossings preferred over centres and insertion points, and those over midpoints, when they are within
   0.15 radius of each other.
2. The foot of the perpendicular from the previous point of the operation to an edge in reach.
3. The crossing of two inference lines, or of an inference line and an edge, in reach.
4. When no edge is in reach, a single inference line in reach (the pointer is projected onto it).
5. The nearest point on an edge in reach (wall face, centreline, layer boundary or plain edge).
6. The grid (5 cm by default), else the raw position.

**Inference lines.**

- *Angle steps* from the previous point (every 15° by default); the distance along the ray rounds to the grid step.
- *Alignment*: horizontal and vertical lines through the three most recently hovered points (and through the first
  point of the chain being drawn).
- *Extension* of the two most recently hovered edges, beyond their ends.
- *Parallel* and *perpendicular* lines through the previous point, to the two most recently hovered edges.

**Locks and overrides.** Holding Shift locks the current direction from the previous point: the pointer is
projected onto that line and only crossings with edges, and points aligned with it, are offered. Holding Alt
disables snapping; a Shift lock still applies.

**Labels.** Every snap other than grid and free carries a label in the user's language naming the kind of snap and
the element: "άκρο τοίχου 12" / "endpoint of wall 12", "κάθετη στον τοίχο 12" / "perpendicular to wall 12",
"προέκταση τοίχου 12", "παρειά τοίχου 12", "όριο στρώσης τοίχου 12", "γωνία 45°". Elements are named by their tag,
else their number. Crossings of an inference line with an edge combine both labels ("γωνία 0° · παρειά τοίχου 3").
Snaps from inference lines carry the guide segments to draw.

**Performance.** Candidates are kept in a uniform grid index built once per document version and storey and shared
by all tools. On a storey of 2,000 elements a snap query takes well under 2 ms (mean and 99th percentile).

## Edge cases

- Elements being moved or edited are left out of the candidates, so the pointer does not snap to their old
  position.
- Very long items (e.g. survey lines across the plot) are checked on every query instead of being indexed.
- Arc walls: faces, centreline and layer boundaries are offered as short chords within 5 mm of the arc.
- A wall whose build-up is missing offers its reference line only.

## Acceptance tests

1. Given a wall from (0,0) to (5,0), when the pointer is 4 cm from (5,0), then the snap is the wall end, labelled
   "άκρο τοίχου …".
2. Given the starter exterior build-up centred on y = 0, when the pointer is at y = 0.16, then it snaps to the left
   face at y = 0.155; at y = 0.066, to the layer boundary at y = 0.065.
3. Given two crossing walls, the crossing is offered; from a previous point above a wall, the perpendicular foot is
   offered and labelled "perpendicular to wall …".
4. Given two tracked points, near the crossing of their alignment lines the pointer snaps there with two guides.
5. With Shift held after moving at 45°, the pointer stays on the 45° line; with Alt held it does not snap.
6. After hovering a wall face, moving beyond the wall's end along that line snaps to its extension; from a previous
   point, moving parallel or perpendicular to a hovered wall snaps to those lines.
7. On a 2,000-element storey, the mean and 99th-percentile snap query times are below 2 ms.

## Open questions

- Should the snap radius differ for points and edges?
- Should tracked points need a short hover before they are acquired, to avoid clutter?

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
