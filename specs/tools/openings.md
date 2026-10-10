---
id: SPEC-tools-005
title: Placing doors, windows and voids in walls
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §3 (drawing is direct)
  - SPEC-tools-001, SPEC-tools-002
  - specs/objects/doors.md, specs/objects/windows.md, specs/objects/orientation.md (opening orientation)
  - General drafting practice (contributors' own professional knowledge)
---

# Placing doors, windows and voids in walls

## Purpose

Put openings into walls at exact positions with one click, seeing the result before committing.

## Behaviour

**Host.** The wall under the pointer — its body, or within half its thickness plus the snap radius of its reference
line — is the host; the nearest reference line wins. Without a wall under the pointer, nothing previews and the
prompt asks to hover a wall.

**Position.** The opening is centred at the pointer's position along the host's reference line (curved walls by arc
length), kept inside the wall. It then snaps, in order, to:

1. a typed distance: from the wall end nearest the pointer when typing starts to the opening's near jamb;
2. the centre of the wall, when the pointer is within the snap radius of it;
3. flush with the nearest wall end, when the near jamb is within the snap radius of it;
4. steps (5 cm by default) of the distance from the nearest wall end to the near jamb.

The snap label names the rule and the resulting distance, and the preview draws that distance along the wall.

**Side and hand.** The side of the wall the pointer is on sets the side a door swings to, and for windows the side of
the outer face: click inside the room a door opens into, and outside the building for a window. In the model a door
opens towards the wall's right face and a window's outer face is the wall's left face unless the opening is flipped
(SPEC-objects-006), so the tool flips a door when the pointer is on the left side and a window when the pointer is on
the right side. In an exterior wall drawn clockwise these usual clicks give unflipped openings. Voids look the same
from both sides and are never flipped. The flip key swaps the hinge jamb.

**Size and style.** Width and height from the settings per kind (door, window, void); Tab to the width field to type
another width. Windows sit at the set sill height; doors and voids at the wall's base.

**Validity.** An opening wider than its wall, outside the wall's height, or overlapping another opening in the same
wall is drawn in the warning colour with the reason, and a click does nothing.

## Edge cases

- Walls shorter than the opening: the opening centres in the wall and is refused as too wide.
- The distance stays measured from the same end while the typed value is kept, even if the pointer moves past the
  wall's middle.

## Acceptance tests

1. In a wall from (0,0) to (6,0), the pointer at (1.03, 0.05) places a 0.90 m door with its jamb 0.60 m from the
   start (5 cm steps), swinging to the wall's left side (flipped).
2. The pointer near (3, −0.05) centres the door and swings it to the right side (not flipped); near the end it sits
   flush.
3. Typing 0.5 and a width of 1.6 near the end places a 1.6 m window 0.5 m from the end, at the sill height.
4. A second door overlapping the first is refused with an explanation; Flip swaps the hinge jamb.
5. A window placed with the pointer on the wall's left side is not flipped (outer face left); on the right side it
   is flipped.
6. A void wider than its 0.8 m wall is refused.
7. In a half-circle wall of radius 2, the pointer at the arc's middle places the opening at π metres.
8. In walls drawn clockwise, a window placed with the pointer outside and a door placed with the pointer inside are
   both unflipped: the window's frame stands near the outside face and the door opens inwards.
9. Voids placed from either side of a wall are not flipped.

## Open questions

- Distances could also be measured from the face of a joining wall instead of the reference-line end.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
