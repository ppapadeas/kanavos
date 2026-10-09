---
id: SPEC-views-001
title: Floor plan drawing
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 128-2:2020 Technical product documentation — Basic conventions for lines
  - ISO 128-3:2020 Technical product documentation — Views, sections and cuts
  - ISO 7519:1991 Construction drawings — General principles of presentation for general arrangement and assembly drawings
  - ν. 4067/2012 (ΝΟΚ), άρθρο 2 and 11 (use categories of spaces)
  - ν. 4495/2017, άρθρο 40 (drawings submitted for a building permit)
  - Professional practice of Greek permit drawings (κατόψεις 1:50 and 1:100)
---

# Floor plan drawing

## Purpose

An architect needs a plan of every storey, derived from the model, that reads like a conventional Greek
permit or working drawing at 1:50 and 1:100 without manual clean-up.

## Behaviour

### What the plan shows

A plan is the storey cut by a horizontal plane at the storey's cut height (default 1,10 m above the finished
floor) and seen from above.

| Part | Representation | Layer |
| --- | --- | --- |
| Cut walls, columns, beams, slabs | Material fill and one heavy outline around everything cut | `<kind>.layer`, `<kind>.cut` |
| Below the cut (slab edges, low walls, sills seen through windows, railings) | Thin solid lines | `<kind>.below` |
| Above the cut (beams, high windows, overhangs) | Thin dashed lines | `<kind>.above` |
| Slabs of the storey above (balconies, canopies, openings in the floor above) | Thin dashed lines | `slab.above` |
| Roofs on the storey, or on a storey above that has no walls | Edges seen from above (eaves, ridges, hips, valleys, parapets), dashed when above the cut | `roof.above`, `roof.below` |
| Doors and windows spanning the cut | Their plan symbols for the drawing scale (less detail at 1:100 than at 1:50), placed in the wall opening | `door.symbol*`, `window.symbol*` |
| Openings wholly above the cut | Their overhead symbol, dashed | `window.symbol.above`, `door.symbol.above` |
| Furniture and fixtures | Their plan symbols for the drawing scale | `object.symbol*` |
| Stairs | See SPEC-views-002 | `stair.*` |
| Spaces | Optional tint by use, stamp | `space.fill`, `space.stamp` |
| Annotation | See SPEC-views-003 | `dimension.*`, `level.*`, `text`, `grid.*` |
| Underlays (survey drawings) | Their own geometry in grey, under everything | `underlay.*` |

Painting order, back to front: underlays, space tints, below, above, cut fills, material boundaries, cut
outline, door and window symbols, stair labels, space stamps, annotation, view markers, north arrow. Cut fills
are opaque, so lines that lie under walls disappear.

### Cut areas

1. Every cut area keeps its material. Its fill is the material's cut hatch over the material's background
   colour, or over white when the material has none.
2. Where cut areas of different elements overlap, the material with the higher junction priority runs through
   and the other is trimmed to it; for equal priorities the element created first runs through. Areas of the
   same element never trim each other. (When element geometry resolves junctions itself, there is nothing left
   to trim and this rule has no effect.)
3. Areas of the same element kind and the same material merge: no line is drawn between two brick walls that
   meet. A thin line separates different materials of the same kind (brick and insulation); a medium line
   separates different kinds (a column inside a wall).
4. A layer thinner than 0,5 mm on paper (e.g. 2 cm plaster at 1:100) is drawn white, without hatch or boundary,
   so thin finishes do not turn into smudges.
5. The union of all cut areas of one phase gets one continuous heavy outline. Its pieces keep the layer and the
   selection source of the element they bound.

### Line weights (paper millimetres)

| Pen | ≤ 1:20 | ≤ 1:50 | ≤ 1:100 | smaller |
| --- | --- | --- | --- | --- |
| Cut outline | 0,70 | 0,70 | 0,50 | 0,35 |
| Minor cut outline | 0,50 | 0,35 | 0,25 | 0,18 |
| Material boundary | 0,18 | 0,13 | 0,13 | 0,13 |
| Below the cut / beyond | 0,25 | 0,25 | 0,18 | 0,13 |
| Above the cut (dashed) | 0,18 | 0,18 | 0,18 | 0,13 |
| Symbols | 0,25 | 0,25 | 0,18 | 0,13 |
| Dimension and extension lines | 0,18 | 0,18 | 0,18 | 0,13 |
| Dimension ticks | 0,50 | 0,35 | 0,35 | 0,25 |

### Spaces

- Optional tint by use category (κύρια χρήση, βοηθητικοί χώροι, κλιμακοστάσιο, ημιυπαίθριοι, εξώστες …), pale
  enough for black-and-white printing.
- Stamp centred on the stamp point: name in bold, number, net area as `Ε = 24,50 m²`; optionally perimeter
  `Π = 20,10 m` and clear height `Υ = 2,90 m`. Numbers use the locale's decimal separator. Text heights are
  3,0 / 2,0 / 2,5 mm (name / number and extras / area) at 1:50 and larger, 2,5 / 1,8 / 2,0 mm at smaller scales.
- Without a stamp point, the stamp goes to the space's centroid; if the stamp would overlap a stair of the
  storey (or one arriving from below), it moves to the point of a 13 × 13 grid over the space where it overlaps
  stairs and the outside of the space least, nearest the centroid among equals.

### View markers and north arrow

- Every section view appears as a thin chain line with heavy stubs at both ends, an arrow at each end pointing in
  the viewing direction and the section letter beside each arrow.
- Every elevation view appears as a filled triangle at the middle of its line, pointing in the viewing
  direction, with its letter behind it.
- A north arrow (circle, half-filled arrow, the letter Β/N) sits at the top right; it points to grid north,
  i.e. project +Y rotated by the site's georeference rotation.

## Edge cases

- Elements whose geometry fails are left out; the rest of the plan is still drawn.
- An opening wholly below the cut is not drawn (the wall above it is cut solid).
- A demolished element trims nothing it does not overlap and keeps its own dashed outline (SPEC-views-006).

## Acceptance tests

1. Given two brick walls meeting at an L, when the plan is drawn, then their cut outline is one ring and no
   line separates them.
2. Given a concrete column inside a brick wall, then the wall fill is split around the column and the column
   keeps a medium outline.
3. Given a window with its sill above the cut, then its overhead symbol is drawn on `window.symbol.above`.
4. Given a balcony slab on the storey above, then its edge appears on `slab.above`, dashed.
5. Given a plan at 1:100, then the cut outline is 0,50 mm and 2 cm plaster layers are white.

## Open questions

- Per-view overrides of the finish threshold and of the tint palette.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending — Result: pending
