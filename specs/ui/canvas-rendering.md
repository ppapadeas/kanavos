---
id: SPEC-ui-001
title: Drawing canvas — how derived drawings are shown on screen
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §6 (visual design), docs/architecture.md §7 (performance budgets)
  - ISO 128-20:1996 (line widths), ISO 128-50:2001 (areas on cuts and sections)
  - SPEC-drawing-001 (hatch patterns), specs/drawing/line-patterns.md (line patterns)
  - Own professional drafting practice
---

# Drawing canvas — rendering

## Purpose

An architect reads plans, sections, elevations and sheets on screen the way they will print: black on white, the
pen hierarchy visible, cut materials hatched, text at its printed size. The picture must stay crisp and keep up with
the pointer while panning and zooming.

## Behaviour

Input: a 2D drawing (items with shape, stroke, fill, text, layer and source) in metres at a scale 1:N for model
views, or in millimetres for sheets. Sizes that must read the same on paper (stroke weights, dash lengths, hatch
spacing, text height) are given in paper millimetres.

1. **Background.** Model views draw on white. Sheets draw the page (paper size from the sheet) in white, with a soft
   drop shadow, on a neutral grey ground.
2. **Line weights true to print.** A stroke of weight `w` mm is drawn `w × points-per-paper-mm` points wide, where
   points-per-paper-mm = zoom × N / 1000 for model views (zoom in points per metre) and = zoom for sheets. The width
   is clamped to `[0.5 + 1.5·w, 2 + 8·w]` points so that thin pens never vanish when zoomed out, thick pens never
   become slabs when zoomed in, and a heavier pen is never drawn thinner than a lighter one.
3. **Dashes.** Dash patterns (`specs/drawing/line-patterns.md`) are measured in paper millimetres and run
   continuously along a polyline, across its vertices. When a pattern period would be shorter than 6 points on screen it is stretched to 6 points, so dashed
   lines stay recognisably dashed. Zero-length dashes are drawn as dots.
4. **Fills.** Closed areas are filled with their colour; the first ring is the outline and further rings are holes
   (even-odd). Concave outlines and holes are filled exactly (triangulated), not approximated by their hull.
5. **Hatches.** A hatch fill is drawn as its background colour (if any) plus the hatch lines of its pattern in its
   line colour, exactly as specified for every output in `specs/drawing/hatch-patterns.md` (SPEC-drawing-001):
   the same segments are drawn on screen as are printed. Hatch lines are clipped exactly to the area, holes excluded.
6. **Level of detail.** When hatch lines would be closer than 3 points on screen, the area is drawn as a flat tone
   instead: the line colour spread over the background in proportion to the printed ink coverage. Text whose cap
   height is below 2.8 points is not drawn. Items smaller than a third of a point are skipped.
7. **Arcs and circles** are drawn with enough segments that the chord stays within 0.2 points of the true curve.
8. **Text** is drawn at its cap height in paper mm, anchored at the given anchor of its box (baseline to cap
   height), rotated counter-clockwise by its rotation, bold when asked. Text is drawn above lines.
9. **Highlight.** Items whose source is selected are drawn again on top in selection blue, 0.8 points wider, with a
   translucent blue tint over their fills; selected text turns blue. The element under the pointer is drawn in a
   lighter translucent blue, 2 points wider. Selection and hover are shared by every view.
10. **Colour carries meaning only**: selection (blue), inference (green), nothing decorative.

## Performance

- Zoom-independent work (fill triangulation, hatch lines, bounds) is done once per drawing revision.
- Zoom-dependent work (widths, dashes, level of detail) is done once per zoom level and window and reused while
  panning and while the zoom stays within ±19 %; panning and small zooms only move the cached picture.
- When zoomed far into a large drawing, only items in a window of three by three screens around the view are
  prepared; the window is rebuilt when the view leaves it.
- Target: pan and zoom of a 300-element storey plan at 60 frames per second.

## Edge cases

- Degenerate rings (fewer than three points), non-finite coordinates: the item is drawn as far as possible or
  skipped; never a crash.
- A polygon the triangulator rejects (self-intersecting) is filled as a fan from its first vertex.
- Areas whose hatch would need more than 60 000 segments are drawn as a tone at every zoom.
- An empty plan shows a short hint on how to start (draw a wall).

## Acceptance tests

1. Given a 1:50 plan at 80 points per metre, when a 0.25 mm stroke is drawn, then it is 1 point wide (true width),
   and a 0.13 mm hatch line is drawn at its 0.7 point minimum.
2. Given a 10 × 10 mm square and any line or motif pattern, then hatch segments are drawn and every segment's
   midpoint lies inside the square.
3. Given a 1:50 drawing and a diagonal hatch at scale 2, then its line spacing on screen corresponds to 0.2 m.
4. Given an area that would need more than 60 000 segments, then it is drawn as a tone.
5. Given a hatched wall at a zoom where the hatch spacing is under 3 points, then it is drawn as a tone with far
   fewer vertices than when zoomed in.
6. Given a 10-unit line with a 3 on / 1 off pattern, then it is drawn as three dashes of total length 8; an L of
   2 + 2 with the same pattern draws one dash that turns the corner.
7. Given a polygon with a hole, then its fill covers exactly the outline area minus the hole.
8. Given text rotated 90°, then its box is rotated; anchoring places the anchor point of the box on the position.

## Open questions

- A dedicated drafting typeface (ux.md §6) instead of the system sans.
- Line-end caps and joins for very thick pens (square vs. round).

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
