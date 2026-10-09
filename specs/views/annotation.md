---
id: SPEC-views-003
title: Dimensions, level marks and automatic exterior dimensions
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 129-1:2018 Technical product documentation — Presentation of dimensions and tolerances
  - ISO 7519:1991 Construction drawings — General principles of presentation (level indication)
  - ISO 128-2:2020 Basic conventions for lines
  - Professional practice of Greek permit drawings (στάθμες ±0,00, αλυσίδες διαστάσεων)
---

# Dimensions, level marks and automatic exterior dimensions

## Purpose

Annotation must read the way Greek architects expect: chains of dimensions in metres with a decimal comma,
oblique ticks, level marks with a sign, and a quick way to dimension the outside of a building.

## Behaviour

### Dimension chains

1. Points are projected on the dimension line along its direction and sorted; points closer than 1 µm merge.
2. The dimension line extends 2 mm (paper) beyond the outermost points.
3. Every point has an extension line from 1,5 mm away from the measured point to 2 mm beyond the dimension
   line; it is omitted when the point is within 1,5 mm of the line.
4. Every point has a 45° oblique tick, 2,5 mm long, with the tick pen.
5. Each span's length, in metres with two decimals and the locale's separator (`3,25` in Greek, `3.25` in
   English), stands 0,8 mm above the line, centred, reading from below or from the right. When it does not fit
   in its span, consecutive texts are staggered one text height upwards.

### Level marks

- **Plan**: a circle of 1,6 mm radius with a cross, two opposite quarters filled, and the level to its right with
  an explicit sign: `±0,00`, `+3,20`, `-0,45`. An optional label precedes the value.
- **Section and elevation**: a filled triangle with its tip on the level, a short level line and the value above
  it. An optional label follows the value (e.g. `-0,45 Φ.Ε.` for natural ground).

### Automatic exterior dimensions

For a storey, the walls and columns are united into the building outline. Its main axis is the direction of the
longest outline edge. On each of the four sides (seen along the main axes):

1. The **façade chain** holds every outline corner on an edge facing that side and every jamb of a door or window
   lying on the outline, provided a ray from the point towards that side leaves the outline without crossing it.
   Points closer than 5 mm merge. The chain is proposed only when it has more than two points.
2. The **overall dimension** spans the outermost outline points along that side.
3. The first chain lies 12 mm (paper, at the storey's plan scale or 1:50) outside the outline, the next one 8 mm
   further out.

The result is a list of dimension definitions; turning them into elements is a command.

### Other annotation

Grid lines (thin chain lines with 4 mm bubbles and their labels at both ends), text notes and drafting items are
drawn as stored. Annotation that belongs to a section or elevation view is drawn in that view's coordinates.

## Edge cases

- A storey without walls gets no automatic dimensions.
- Openings on interior walls do not appear in exterior chains.

## Acceptance tests

1. Given points at 0, 3,25 and 4,00 m, when drawn in Greek, then the texts are `3,25` and `0,75`, with three
   ticks and three extension lines.
2. Given the fixture house, when automatic dimensions are computed for its ground floor, then each side has an
   overall dimension of 12,31 m or 9,31 m and the south chain includes the jambs of the entrance door.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending — Result: pending
