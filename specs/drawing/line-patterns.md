---
id: SPEC-drawing-002
title: Line patterns on polylines
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ISO 128-20:1996 (basic conventions for lines)
  - ISO 32000-1:2008 §8.4.3.6 (line dash pattern)
  - SVG 1.1 §11.4 (stroke-dasharray)
---

# Line patterns on polylines

## Purpose

Dashed and dash-dot lines (hidden edges, elements above the cut plane, axes, plot boundaries) must keep an even
rhythm along walls and outlines with many corners, and look the same in every output.

## Behaviour

- A pattern is a list of on/off lengths in **paper millimetres**, starting with a dash. In a model drawing at 1:N
  each length is N/1000 metres.
- An odd-length list is repeated once (`[a]` behaves as `[a, a]`), as in PDF and SVG.
- The pattern runs **continuously along the whole polyline**: it does not restart at vertices. A dash that reaches a
  corner continues round it, so a dash may contain corner vertices.
- A closed polyline includes its closing segment.
- A zero-length dash is a dot: a two-point piece with equal points, drawn with round caps.
- Solid lines use round caps and joins; patterned lines use butt caps and round joins.

Standard patterns (paper mm): dashed 3.0/1.5, hidden 1.5/1.0, dotted 0.2/0.8, dash-dot 6.0/1.0/0.5/1.0,
dash-dot-dot 6.0/1.0/0.5/1.0/0.5/1.0.

## Edge cases

- A pattern with a negative or non-finite length, or a total length of zero, draws the line solid.
- A line that would produce more than one million dashes is drawn solid.
- A dash that would start exactly at the end of the line is dropped.

## Acceptance tests

1. Given a 10-unit line and pattern 2/1, then dashes are 0–2, 3–5, 6–8 and 9–10.
2. Given an L-shaped polyline 3 + 3 long and pattern 4/1, then the first dash turns the corner and keeps the corner
   vertex, and the drawn length is 5.
3. Given a unit square (closed) and pattern 0.5/0.5, then half of the perimeter is drawn.
4. Given a model drawing at 1:100 and the dashed pattern, then the first dash is 0.3 m.

## Open questions

- Should very short segments (shorter than one period) be drawn solid so short edges are never blank?

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
