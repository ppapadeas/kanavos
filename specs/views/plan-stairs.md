---
id: SPEC-views-002
title: Stairs in floor plans
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 7519:1991 Construction drawings — General principles of presentation (stairs and ramps in plan)
  - ISO 128-2:2020 Basic conventions for lines (dashed lines for hidden and overhead parts)
  - Professional practice of Greek permit drawings (σύμβολα κλιμάκων: «ΑΝ», «ΚΑΤ», σημείωση βαθμίδων)
---

# Stairs in floor plans

## Purpose

Show a stair in the plan of the storey where it starts and in the plan of the storey it arrives at, so a
reader sees where it goes, which way is up and how its steps are sized.

## Behaviour

### On the storey where the stair starts

1. The cut plane meets the stair at the first tread whose top is above the cut elevation. Through the middle of
   that tread's front edge runs the **break line**, at 45° to the walking direction, crossing the flight from
   side to side and a little beyond, with a zig-zag placed off-centre.
2. Risers, flight sides and landings on the lower side of the break line are thin solid lines. Everything on
   the upper side, later flights and landings above the cut, are thin dashed lines (this can be switched off,
   in which case they are omitted).
3. The **walking line** runs along the middle of each flight and through the centres of the landings. It starts
   with a filled dot at the first riser, is solid up to the break line and ends there with a filled arrow head;
   beyond the break it continues dashed.
4. Labels: «ΑΝ» (en: UP) in bold beside the start; the **riser note** `n×h/g` along the first flight beside
   the walking line, where `n` is the number of risers, `h` the riser height in cm and `g` the going in cm,
   each with one decimal unless whole (e.g. `18×17,8/28`).
5. If the whole stair lies below the cut, every line is solid and the arrow is at the end of the walking line.

### On the storey the stair arrives at

1. The same break line is drawn (from the lower storey's cut elevation).
2. The part of the stair above the break line is drawn with thin solid lines; the part below it is omitted.
3. The walking line runs from the top of the stair to the break line, starts with a dot at the top and ends at
   the break with an arrow pointing down the stair. «ΚΑΤ» (en: DN) in bold stands beyond the top end.

### Layers

`stair.below`, `stair.above`, `stair.break`, `stair.line`, `stair.label`.

Labels are painted after cut areas so walls never hide them.

## Edge cases

- When the first step above the cut is a landing, the break line crosses the end of the first flight.
- Stairs whose layout fails are skipped.

## Acceptance tests

1. Given a half-turn stair of 18 risers of 17,8 cm and 28 cm going, when the start storey's plan is drawn,
   then it contains «ΑΝ», the note `18×17,8/28`, a break line and dashed treads beyond it.
2. When the arrival storey's plan is drawn, then it contains «ΚΑΤ» and only the treads above the break line.

## Open questions

- Curved walking lines on landings of quarter-turn stairs.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending — Result: pending
