---
id: SPEC-drawing-004
title: DXF export layer, style and line type mapping
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - DXF reference (published file format documentation), group codes for TABLES, LTYPE, LAYER, STYLE, HATCH, TEXT
  - ISO 128-20:1996 (line widths)
---

# DXF export layer, style and line type mapping

## Purpose

People exchange drawings with consultants and authorities as DXF. The exported file must open cleanly, keep the
drawing's layers so they can be switched on and off, and print with the same line weights, line types and hatches
as Kanavos.

## Behaviour

**Version and units.** The file is ASCII DXF version AC1015 (R2000). Units are those of the drawing unless chosen:
metres (`$INSUNITS` 6) for model drawings, millimetres (`$INSUNITS` 4) for sheets; choosing the other unit scales
every coordinate, length and text height.

**Layers.** Every item layer becomes a DXF layer of the same name. Characters DXF forbids in names
(`< > / \ " : ; ? * | = , ` ``) become `_`; an empty name becomes `0`. Each layer's colour, lineweight and line type
are the most common among its items (ties go to the lower value). An item whose stroke matches its layer uses
BYLAYER; otherwise the entity carries its own colour, lineweight and line type.

**Colours** are colour indices: black is 7 (shown in the contrasting colour of a CAD screen, printed black), white
is 255 (true white, so masking backgrounds stay white), other colours take the nearest index.

**Lineweights** snap to the nearest DXF lineweight (0.13, 0.18, 0.25, 0.35, 0.50, 0.70, 1.00 mm, …).

**Line types.** Solid strokes use `Continuous`. Each line pattern becomes a line type `KV_DASHED`, `KV_HIDDEN`,
`KV_DOTTED`, `KV_DASHDOT`, `KV_DASHDOTDOT` or `KV_CUSTOMn`, with its dash and gap lengths converted from paper
millimetres to drawing units at the drawing's scale, so `$LTSCALE` stays 1.

**Entities.** Lines → LINE; polylines and polygon rings → LWPOLYLINE (closed rings closed); circles → CIRCLE; arcs →
ARC (counter-clockwise, a full turn as CIRCLE); text → TEXT with height = cap height, rotation in degrees and the
alignment from its anchor (left/centre/right × baseline/middle/top); bold text uses the text style `Bold`.

**Fills.** Solid fills and hatch backgrounds → HATCH with the SOLID pattern on the item's layer, rings as polyline
boundaries with the even-odd (odd parity) style; circles keep an exact boundary of two half-circle bulges. Hatch
lines are expanded (`specs/drawing/hatch-patterns.md`) into LINE entities on the layer `<layer>.hatch`, so hatches
can be hidden or recoloured separately.

**Text encoding.** By default the file is plain ASCII apart from Western accents, with every other character as a
`\U+XXXX` escape. With the Greek code page option, `$DWGCODEPAGE` is `ANSI_1253` and Greek is written as single
bytes. A literal `%%` in text is written so that it reads back unchanged.

## Edge cases

- Items without a stroke and without a fill produce no entity.
- A hatch too dense to expand keeps its solid background only.
- Text with an empty content or a non-positive height is skipped.

## Acceptance tests

1. The exported file has the six sections, all nine tables, unique handles below `$HANDSEED`, and reads back with
   no warnings.
2. A layer whose items are 0.70 mm black solid lines is written with colour 7, lineweight 70 and `Continuous`.
3. At 1:50 in metres, the dashed pattern 3.0/1.5 mm becomes a line type of 0.15/−0.075.
4. Export → import keeps line end points, arc angles, circle radii, text content, anchor, rotation, height and
   weight, line patterns in paper millimetres, white backgrounds and hatch lines on `<layer>.hatch`.

## Open questions

- Should line-family hatches also be offered as native pattern hatches (editable in CAD) instead of lines?

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
