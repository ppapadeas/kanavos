---
id: SPEC-objects-003
title: Windows, sills, shutters and roller shutters
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - SPEC-objects-001 (coordinates, layers, line weights)
  - ISO 128-2:2020 (line types), ISO 7519:1991 (construction drawings)
  - General architectural drafting practice in Greece and Europe
  - Common Greek window construction: frames set back from the outer face, marble external sills, external
    hinged shutters, roller shutters in a box at the head
---

# Windows, sills, shutters and roller shutters

## Purpose

Architects place the windows of a house and get the usual plan symbol (frame, glass, sill, swing), the Greek
options of external shutters and roller shutters, and a convincing 3D window.

## Behaviour

### Styles and defaults

| Identifier | Greek name | Default hole (w × h, sill) | Notes |
| --- | --- | --- | --- |
| `window.casement_1` | Μονόφυλλο ανοιγόμενο παράθυρο | 0.80 × 1.20, 0.90 | |
| `window.casement_2` | Δίφυλλο ανοιγόμενο παράθυρο | 1.20 × 1.40, 0.90 | leaves meet without a mullion |
| `window.casement_3` | Τρίφυλλο ανοιγόμενο παράθυρο | 1.80 × 1.40, 0.90 | single leaf, mullion, pair |
| `window.tilt_turn` | Ανοιγοανακλινόμενο παράθυρο | 0.90 × 1.40, 0.90 | 1 or 2 leaves |
| `window.top_hung` | Προβαλλόμενο παράθυρο | 0.80 × 0.60, 1.50 | |
| `window.sliding` | Επάλληλα συρόμενο παράθυρο | 1.80 × 1.40, 0.90 | 2, 3 or 4 panes |
| `window.fixed` | Σταθερό παράθυρο | 1.00 × 1.00, 1.00 | optional mullions |

### Parameters

Frame face width (`frame`, 6 cm), frame depth (`frame_depth`, 7 cm; 10 cm for sliding), setback of the frame from
the outer face (`setback`, 6 cm), external sill (`sill_out`, on) and its projection (`sill_projection`, 3 cm),
internal sill (`sill_in`, on), shutters (`shutters`, off), roller shutter (`roller`, off) and box height
(`roller_box`, 22 cm), handle (`handle`), top-light height (`transom`, 0 = none); per style: leaves, panes,
mullions.

### Plan

- The frame occupies a band across the wall `setback` from the outer face, `frame_depth` deep (limited to the
  wall). At 1:50 the jambs and mullions are drawn as cut profiles and each sash as its two cut stiles with a glass
  line between; at smaller scales the band is drawn as its outline with one glass line.
- Casement and tilt-and-turn sashes also show their swing: the sash opened 90° towards the inner face from the
  inner edge of the frame, with a thin arc, on the swing layer. Top-hung and fixed windows show no swing.
- Sliding panes sit on tracks across the frame band (two tracks for two or four panes, three for three), overlap
  5 cm, and carry arrows on the inner side showing the sliding direction; with four panes the middle pair slides
  outwards over the end panes.
- External sill: its projection beyond the outer face, running 3 cm into each jamb. Internal sill: 2 cm beyond
  the inner face. Both 0.18 mm on the sill layer.
- Shutters: one leaf for holes narrower than 0.75 m, two otherwise; each leaf drawn opened flat against the outer
  face beside the hole, as a thin rectangle 3 cm thick, on the shutter layer.
- Roller shutter: the box at the head drawn dashed (above the cut plane) from the outer face to the frame, and the
  guide rails at the jambs outside the frame.

### 3D

Marble sills; a white frame standing on the external sill; sashes with profiles and transparent glass; window
levers on the inner face at mid-height of the opening sash (bottom rail for top-hung); a fixed top light above a
transom bar; louvred shutters opened against the façade; the roller box at the head, which lowers the frame, with
guide rails and one slat of the curtain showing.

## Edge cases

- When the frame band does not fit the setback, the setback is reduced; the frame depth never exceeds the wall.
- A window too low for its frame, roller box and top light keeps only its sills and accessories.

## Acceptance tests

1. Given a two-leaf casement with shutters and a roller shutter in a 30 cm wall, then the shutters lie beyond the
   right face and wider than the hole, the roller box items are dashed, and the sills reach past both faces.
2. Given the same window with swap side, then the shutters lie beyond the left face.
3. Given a sliding window with four panes, then the symbol has more items than with two.
4. Given every window style in the four swap states, then the plans are mirror images and the parts are closed.

## Open questions

- Folding (multi-leaf) shutters for very wide windows; external roller boxes projecting from the façade.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
