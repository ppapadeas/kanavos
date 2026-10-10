---
id: SPEC-elements-004
title: Stairs and railings
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - Κτιριοδομικός Κανονισμός, Υ.Α. ΥΠΕΝ/ΔΑΟΚΑ/66006/2360, ΦΕΚ Β΄ 3985/22.06.2023, άρθρο 11 (Κλίμακες) §2, §4 (πίνακας τύπων Ι–ΙΙΙ), §4.1–§4.6
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

### Stair use

Every stair says what it serves; the default is the internal stair of a single dwelling. The uses follow the
building uses of article 11 §4.1–§4.5 and the widths of §2:

| Use | Stair types allowed | Clear width (§2) |
| --- | --- | --- |
| Internal stair of a single dwelling (default) | I, II or III (§4.1) | ≥ 0.60 m |
| Shared stair of a residential building (category Α) | I, II or III (§4.1) | ≥ 0.90 m with up to three storeys above ground (pilotis counted, basements not), else ≥ 1.20 m |
| Other uses: temporary accommodation, correctional, commerce, offices, industry, storage, parking (Β, Ζ, Η, Θ, Ι, Κ, Λ) | I or II (§4.2) | ≥ 0.90 m (type II); a note below 1.20 m, since every building needs one stair of 1.20 m |
| Assembly and education (Γ, Δ) | I (§4.4) | ≥ 1.20 m |
| Health and social care (Ε) | I (§4.3) | ≥ 1.20 m |
| Only auxiliary rooms of an industrial building, or storage rooms of a commerce or storage building | relaxed limits of §4.5 | ≥ 0.60 m |

The exceptions of §4.3 and §4.4 (type II or III stairs that patients, young children or more than 20 people never
use) depend on how the building is run, which the model does not know; such a stair is given the use "other
uses".

### Stair checks (warnings and notes, never errors)

The table of article 11 §4: riser 0.13–0.18 m (type I), 0.13–0.20 m (type II), 0.13–0.18 m (type III, curved
flights); going ≥ 0.28 m (I) or ≥ 0.25 m (II, III); 2 risers + going 0.60–0.66 m; clear headroom ≥ 2.20 m; rise
between landings ≤ 3.60 m; landing length along the flight ≥ 1.20 m (I, III) or ≥ 0.90 m (II). Straight and
two-flight stairs are checked against type II where the use allows type I or II, with notes where type I would not
be met, and against type I where only type I is allowed:

| Check | Uses allowing type II | Type I only | Auxiliary (§4.5) | Level |
| --- | --- | --- | --- | --- |
| Riser height | ≤ 0.20 m, ≥ 0.13 m | ≤ 0.18 m, ≥ 0.13 m | ≤ 0.23 m | warning |
| Riser height above type I | > 0.18 m | — | — | note |
| Going | ≥ 0.25 m | ≥ 0.28 m | ≥ 0.23 m | warning |
| Going below type I | < 0.28 m | — | — | note |
| 2 risers + going | 0.60–0.66 m | 0.60–0.66 m | — | warning |
| Clear width | per use, table above | ≥ 1.20 m | ≥ 0.60 m | warning |
| Clear width below 1.20 m (other uses) | note | — | — | note |
| Rise between landings | ≤ 3.60 m | ≤ 3.60 m | — | warning |
| Landing length along the flight | ≥ 0.90 m | ≥ 1.20 m | — | warning |
| Clear headroom over treads and landings | ≥ 2.20 m | ≥ 2.20 m | — | warning |
| Total rise against the storey above | within 1 cm of the floor-to-floor height | same | same | note |

Headroom is the smallest vertical distance from a tread or landing up to the underside of a slab (any storey)
whose outline, holes excluded, covers more than 1 cm² of it; a slab the step runs into gives zero. Roofs and beams
are not considered.

Each check carries its value, limit and source; messages exist in Greek and English. The inspector lists the
checks of a selected stair or railing; the use is chosen in the inspector and in the stair tool's settings.

### Railings

- Posts (40 mm square) at every path point and at most the post spacing apart; a top rail (50 × 40 mm) swept along
  the path with mitred corners; infill of vertical bars (16 mm, 100 mm clear gap, on a bottom rail), glass panels
  between posts, or a solid panel.
- Plan: its two edge lines, seen when the railing stands below the cut plane and dashed when it is entirely
  above; a solid panel crossed by the cut plane is a cut band instead.
- Checks (article 13): height ≥ 1.00 m; a solid guard ≤ 1.20 m high. Bar gaps are generated at 0.10 m, below the
  0.15 m limit of §3.
- Infill material: the infill (bars, glass, panels) may have its own material, for its colour in 3D, its hatch
  where a solid panel is cut in plan, and its material in exports; without one it uses the railing's material.
  The railing tool gives glass infill the document's first glass material.
- Following a slope: a railing may have one base height per path point; the base then runs straight between
  points (a stair flight, a ramp), and the railing's height is measured from it. Without them the whole railing
  stands at its base offset. Equal bases are a flat railing.
  - Posts and bars stay vertical, from the base under them; the rails follow the slope with their section square
    to the rail; glass and solid panels are cut parallel to the slope at top and bottom (parallelograms in the
    segment's vertical plane).
  - Length (quantities, IFC) is measured along the rail: each segment's plan length combined with its rise.
  - Plan drawing is unchanged, judged by the lowest base (seen below the cut) and the highest top.
  - The number of bases must equal the number of path points. Inserting a path point with the selection tool
    gives it a base interpolated along the edge.
- Railing along a stair (command `railing.along_stair`): one railing per flight, on the chosen side looking up the
  stair, set in from the flight's edge (default 5 cm), following the line of the tread nosings from the first
  nosing (one riser up) to the level of the landing or floor the flight arrives at, so its height (default
  1.00 m) is measured from the nosings. Landings are not railed by the command.

## Edge cases

- A first flight with no treads (first = 1) gives a landing at the first riser.
- A slab only touching a tread (overlap up to 1 cm²) does not limit its headroom.

## Acceptance tests

1. 18 risers over 3.06 m, going 0.28: 17 treads of 0.17 m risers; walking length 4.76 m.
2. Quarter and half turns with 18 risers: treads + landing = 17; landing at 1.53 m; second flight starts at riser 9.
3. A half-turn landing of width 1.0 and gap 0.1 covers 1.0 × 2.1 m.
4. Flight and landing solids are closed; a flight's volume lies between its steps alone and steps plus waist.
5. Cut at 1.10 m on a straight 18-riser stair: six treads below, eleven above, a cut section.
6. 18 risers up to the first floor at +3.20 m, going 0.28, width 1.00 m: no remarks; the same stair rising only
   3.06 m gets the rise note. 0.204 / 0.22 / 0.80 m: riser and going warnings and no width remark for the
   internal stair of a dwelling; messages show "0,20" in Greek and "0.20" in English. 22 risers over 3.80 m
   straight: rise-between-landings warning.
7. Railings with bars, glass and solid infill have solids and no remarks at 1.00 m; at 0.90 m a height warning.
   Cut at 0.50 m, bars and glass give their edge lines, a solid panel a cut band.
8. The good 1.00 m stair of test 6: no remarks as a shared residential stair of a two-storey building; a width
   note (1.20 m) for other uses; a width warning (1.20 m) for assembly, education and health care; with four
   storeys above ground, a width warning (1.20 m) as a shared residential stair. 0.19 m risers and 0.27 m goings:
   type I notes for a dwelling, riser and going warnings (0.18, 0.28) for health care. 0.204 m risers, 0.24 m
   goings, 0.70 m wide, for auxiliary rooms: no remarks other than the rise note.
9. A straight 18-riser stair under a 27.5 cm floor slab at +3.20: a stairwell over the whole flight gives no
   remark; one starting 2.0 m from the first riser gives a headroom warning of 2.925 − 8 × 3.2 / 18 m, limit
   2.20 m; a stair for auxiliary rooms is not checked for headroom.
10. A railing beside a straight 18-riser flight rising 3.20 m, from 0.178 m to 3.20 m over 4.76 m, with bars, glass
    and solid infill and a glass infill material: closed solids of positive volume, the infill in the glass
    material and the rest in the frame's; the rail tops out at 4.20 m; the posts start at 0.178 m; a solid panel's
    volume is 4.76 × (1.00 − 0.04) × 0.05 m³; the length is √(4.76² + 3.02²) m; cut at 1.10 m it gives its edge
    lines (a solid panel a cut band in the glass material). Two bases for three points are refused; equal bases
    give the same solids as none.
11. `railing.along_stair` on the right of a straight stair rising 3.00 m with 18 risers walking north from
    (2.0, 1.0): one railing from (2.45, 1.0) to (2.45, 5.76) with bases 3.0 / 18 and 3.0 m and no remarks; on a
    half-turn stair, two railings, the second from the landing (10 risers up) to 3.0 m.

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

Revised on 2026-10-10 after that review: stair use and the checks per use with headroom, railings following a slope, infill material and railings along a stair, acceptance tests 6 and 8–11. The revision awaits review; the approval above covers the earlier text.

Note: the reviewer is also the author of record. Clean-room protocol §2 asks for a second reviewer; add one when a second contributor joins, before the first public release.
