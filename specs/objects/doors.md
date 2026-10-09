---
id: SPEC-objects-002
title: Doors
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - SPEC-objects-001 (coordinates, layers, line weights)
  - ISO 128-2:2020 (line types), ISO 7519:1991 (construction drawings)
  - General architectural drafting practice in Greece and Europe
  - Common product dimensions of interior, entrance, balcony and garage doors
---

# Doors

## Purpose

Architects place the doors of a house in its walls and get the usual plan symbol (frame, leaf, swing), a closed
door in 3D and correct behaviour when the door is turned to open the other way.

## Behaviour

### Styles and defaults

| Identifier | Greek name | Default hole (w × h) | Notes |
| --- | --- | --- | --- |
| `door.single_swing` | Μονόφυλλη ανοιγόμενη πόρτα | 0.90 × 2.10 | |
| `door.double_swing` | Δίφυλλη ανοιγόμενη πόρτα | 1.40 × 2.10 | two equal leaves |
| `door.unequal_double` | Δίφυλλη πόρτα με άνισα φύλλα | 1.30 × 2.10 | main leaf 0.85 |
| `door.sliding_single` | Μονόφυλλη συρόμενη πόρτα επί του τοίχου | 0.90 × 2.10 | leaf on a rail on the face |
| `door.sliding` | Δίφυλλη επάλληλη συρόμενη πόρτα | 1.60 × 2.20 | two leaves on parallel tracks |
| `door.pocket` | Συρόμενη πόρτα μέσα στον τοίχο | 0.90 × 2.10 | 1 or 2 leaves into wall pockets |
| `door.folding` | Πτυσσόμενη πόρτα | 1.20 × 2.10 | 4 folding panels |
| `door.entrance` | Πόρτα εισόδου με πλαϊνό σταθερό υαλοστάσιο | 1.35 × 2.20 | 0.35 side light, threshold |
| `door.balcony` | Ανοιγόμενη μπαλκονόπορτα | 1.40 × 2.30 | 2 glazed leaves, 75 % glass |
| `door.balcony_sliding` | Επάλληλη συρόμενη μπαλκονόπορτα | 2.00 × 2.30 | 2–4 glazed panes |
| `door.garage` | Σπαστή γκαραζόπορτα | 2.50 × 2.20 | 5 sections |

### Parameters

Frame face width (`frame`), frame depth across the wall (`frame_depth`), leaf thickness (`leaf`), swing angle in
plan (`swing`, default 90°, 15–180°), threshold (`threshold`), leaf glazing as a percentage of the leaf height
(`glazing`), handle (`handle`); per style: main leaf width, number of leaves or panels or panes, side-light
width, number of garage-door sections; balcony doors also take frame setback, shutters and roller shutter (see
SPEC-objects-003).

### Plan

- Hinged doors: the frame sits flush with the face the door opens to, `frame_depth` deep (at most the wall). Its
  two jambs are drawn as cut profiles (0.25 mm). Each leaf is drawn open by the swing angle, hinged at the inner
  edge of its jamb, as a rectangle of leaf thickness at 1:50 or a line at smaller scales (0.35 mm). A thin arc
  (0.18 mm, swing layer) runs from the leaf's tip to the strike jamb. Double doors hinge on both jambs; unequal
  doors put the main leaf on the hinge jamb.
- Threshold: a thin line along each wall face across the opening.
- Entrance door: as a single door with a mullion and a fixed side light (glass line) on the strike side.
- Sliding door on the wall face: the leaf, 5 cm wider than the hole each side, just off the inner face; the rail
  above (dashed) runs on beyond the hole for the parked leaf; an arrow shows the sliding direction.
- Bypass sliding door: two leaves on parallel tracks in the middle of the wall, overlapping 5 cm, each with an
  arrow on its side of the wall.
- Pocket door: the leaf in the closed position and the pocket inside the wall next to the hole, short-dashed;
  an arrow towards the pocket. Two leaves use a pocket on each side.
- Folding door: the panels drawn part-folded as a zigzag towards the hinge jamb, the remaining track dashed, and a
  thin arc for the first panel.
- Garage sectional door: the closed door along the inner face, its overhead tracks and the raised door dashed
  (above the cut plane), and an arrow showing the door travelling inwards.
- Balcony doors use the window frame band and sill rules of SPEC-objects-003 with leaves drawn open like doors.

### 3D

Frames white, timber leaves closed, glazed parts transparent, lever handles on both faces at 1.05 m on the
leaf's free edge, pulls on sliding leaves, a marble threshold slab when set, sections with gaps on garage doors,
rails and tracks in metal.

## Edge cases

- The main leaf of an unequal door is limited so the other leaf is at least 0.20 m.
- The side light is limited so the door leaf keeps at least 0.60 m.
- A swing of 180° draws the leaf flat against the wall face.

## Acceptance tests

1. Given a single swing door in a 30 cm wall, when drawn by default, then its leaf reaches about one leaf width
   beyond the left face and lies at the −x jamb; nothing lies beyond the right face.
2. Given each door style in the four swap states, then the four plans are mirror images (SPEC-objects-001 test 3).
3. Given each door style, then its parts are closed and lie between floor and head (plus rails and tracks).
4. Given a door with swing 45°, then its leaf and arc stop at 45°.

## Open questions

- Double-acting (swing both ways) doors and revolving doors are not in this set.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
