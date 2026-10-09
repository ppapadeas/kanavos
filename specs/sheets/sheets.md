---
id: SPEC-sheets-001
title: Sheets, title block, viewports and automatic layout
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 216 (A-series paper sizes)
  - ISO 5457 (drawing sheet frame) and ISO 7200 (title block data fields), general content only
  - General drafting practice for Greek permit drawings (our own title-block layout)
---

# Sheets, title block, viewports and automatic layout

## Purpose

Compose printable sheets from views at true scale, with a title block holding what Greek permit drawings carry,
and place views automatically on the smallest sheet that holds them.

## Behaviour

### Frame

Paper A4–A0 or custom, portrait or landscape. The frame is drawn 10 mm from the top, right and bottom edges and
20 mm from the left edge (binding margin), 0.70 mm.

### Title block

180 mm wide (it stays visible when a large sheet is folded to A4), 76 mm high, in the lower-right corner of the
frame. Rows, top to bottom: discipline heading; project; location with municipality; owner; designer with
registration details (two lines) beside a signature-and-stamp field; drawing title; bottom row with scale, date,
revision and sheet number (large). Labels sit in a 24 mm column; values are bold and shrink (to 1.8 mm) and then
shorten with an ellipsis to fit. The scale is the single scale of the sheet's model views, "as shown" when they
differ, a dash for table-only sheets. Values come from the project information; labels from the `sheets`
catalogue.

### Viewports

- A viewport places a view so that the centre of its drawing's extent lies at the viewport centre, at the
  viewport scale or the view's.
- An optional model-space crop rectangle clips lines and polylines (segment by segment), polygons (ring by
  ring), and keeps text, arcs and circles whose anchor or extent lies inside.
- Under each model view: its title (bold, 3.5 mm, underlined) and "Scale 1:n". Tables carry their own title.
- Plans get a north arrow at the top-right of their extent.
- A view that cannot be drawn (3D before line drawings exist) shows a dashed 120 × 80 mm placeholder with a
  note, and the sheet is still produced.

### View dispatch

Plans, sections and elevations come from view derivation; 3D views from view derivation as line drawings;
coverage diagram and site plan from the regulation crate; the area table view shows the building data table
above the area table; schedule views show the schedule.

### Automatic layout

Each view needs a box: its drawing's extent at scale plus 6 mm all round, plus a 16 mm label band. Boxes are
placed in rows from the top of the frame, left to right in order, 8 mm apart; a row that reaches down beside the
title block may only use the width left of it. Rows are centred horizontally in their width and the group
vertically when it stays clear of the title block. Papers are tried in the order A4 portrait, A3 landscape, A3
portrait, A2 landscape, A2 portrait, A1 landscape, A1 portrait, A0 landscape, A0 portrait; the first that holds
all boxes is used. Content that fits nowhere goes in one row on A0 landscape.

## Edge cases

- Empty views get a 40 × 20 mm box.
- Missing views in viewports are skipped.

## Acceptance tests

1. A 150 × 120 mm view fits A4 portrait; a 300 × 150 mm view needs A3 landscape; a 300 × 200 mm view does not
   fit A3 landscape beside the title block.
2. Three 180 × 100 mm boxes on A3 landscape stay clear of the title block.
3. The title block of an A3 sheet ends 10 mm from the right and bottom edges and is 180 × 76 mm.
4. A 3D viewport produces a placeholder.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
