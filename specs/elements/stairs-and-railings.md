---
id: SPEC-elements-004
title: Stairs and railings
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - Κτιριοδομικός Κανονισμός, Υ.Α. ΥΠΕΝ/ΔΑΟΚΑ/66006/2360, ΦΕΚ Β΄ 3985/22.06.2023, άρθρο 11 (Κλίμακες) §2, §4 (πίνακας τύπων Ι–ΙΙΙ), §4.1, §4.6
  - Κτιριοδομικός Κανονισμός (ως άνω), άρθρο 13 (Στηθαία) §1–§3
  - Step comfort rule 2 risers + 1 going ≈ one stride (F. Blondel, Cours d'architecture, 1675), as adopted by the regulation's table
---

# Stairs and railings

## Purpose

Stairs must be laid out, drawn, built in 3D and checked against the Greek building regulation while designing;
railings protect stairs, balconies and terraces.

## Behaviour

### Stair layout

- Inputs: origin (midpoint of the first riser), walking direction, shape (straight, quarter turn left/right, half
  turn left/right), clear width, total rise, number of risers, going, risers in the first flight (default half),
  gap between half-turn flights, waist thickness.
- Riser height = rise / risers. Treads = risers − 1 (the last riser arrives at the upper floor).
- Two-flight shapes: the first flight has (first − 1) treads, then a landing at the height of riser `first`, then
  the second flight with the remaining treads. A quarter-turn landing is width × width; a half-turn landing spans
  both flights and the gap.
- Walking line through the flights and landings; its length is the stair's length.

### Stair solids and plan

- Each flight: steps on a sloped waist whose thickness is measured perpendicular to the pitch line; the flight rests
  on the floor it starts from and stops at the front of the landing or upper floor. Landings are slabs of the waist
  thickness under their top.
- Plan: treads and landings below the cut plane are drawn, those above dashed; the flight's section at the cut
  plane is filled with the stair material.

### Stair checks (warnings and notes, never errors)

The regulation's table (article 11 §4) allows any of types I–III for dwellings (§4.1). Checks use type II limits
and note where type I would not be met:

| Check | Limit | Level |
| --- | --- | --- |
| Riser height | ≤ 0.20 m (type II), ≥ 0.13 m | warning |
| Riser height above type I | > 0.18 m | note |
| Going | ≥ 0.25 m (type II) | warning |
| Going below type I | < 0.28 m | note |
| 2 risers + going | 0.60–0.66 m | warning |
| Clear width | ≥ 0.60 m (internal stair of one dwelling, §2) | warning |
| Clear width below 0.90 m | enough only inside one dwelling (§2) | note |
| Rise between landings | ≤ 3.60 m | warning |
| Landing length along the flight | ≥ 0.90 m (type II) | warning |
| Total rise against the storey above | within 1 cm of the floor-to-floor height | note |

Each check carries its value, limit and source; messages exist in Greek and English.

### Railings

- Posts (40 mm square) at every path point and at most the post spacing apart; a top rail (50 × 40 mm) swept along
  the path with mitred corners; infill of vertical bars (16 mm, 100 mm clear gap, on a bottom rail), glass panels
  between posts, or a solid panel.
- Plan: its two edge lines, seen when the railing stands below the cut plane and dashed when it is entirely
  above; a solid panel crossed by the cut plane is a cut band instead.
- Checks (article 13): height ≥ 1.00 m; a solid guard ≤ 1.20 m high. Bar gaps are generated at 0.10 m, below the
  0.15 m limit of §3.

## Edge cases

- A first flight with no treads (first = 1) gives a landing at the first riser.
- Headroom (2.20 m) is not checked yet: it needs the slabs above.

## Acceptance tests

1. 18 risers over 3.06 m, going 0.28: 17 treads of 0.17 m risers; walking length 4.76 m.
2. Quarter and half turns with 18 risers: treads + landing = 17; landing at 1.53 m; second flight starts at riser 9.
3. A half-turn landing of width 1.0 and gap 0.1 covers 1.0 × 2.1 m.
4. Flight and landing solids are closed; a flight's volume lies between its steps alone and steps plus waist.
5. Cut at 1.10 m on a straight 18-riser stair: six treads below, eleven above, a cut section.
6. 18 risers up to the first floor at +3.20 m, going 0.28, width 1.00 m: no remarks; the same stair rising only
   3.06 m gets the rise note. 0.204 / 0.22 / 0.80 m: riser and going warnings and a width note; messages
   show "0,20" in Greek and "0.20" in English. 22 risers over 3.80 m straight: rise-between-landings warning.
7. Railings with bars, glass and solid infill have solids and no remarks at 1.00 m; at 0.90 m a height warning.
   Cut at 0.50 m, bars and glass give their edge lines, a solid panel a cut band.

## Open questions

- The 2025 amendment of the regulation (ΦΕΚ Β΄ 6995/2025) touches the stair article; values must be re-checked
  against the consolidated text before approval.
- Winders and spiral stairs are not covered.

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
