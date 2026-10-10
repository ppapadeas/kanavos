---
id: SPEC-objects-006
title: Orientation of doors and windows in their walls
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - SPEC-objects-001 (opening coordinates), SPEC-objects-002 (doors), SPEC-objects-003 (windows)
  - SPEC-tools-005 (placing openings), SPEC-files-001 (native format)
  - docs/format.md §3 (wall build-ups run from the left face to the right face)
  - General construction practice in Greece: exterior doors open inwards; windows sit near the outside face with
    the external sill, shutters and roller-shutter box outside
---

# Orientation of doors and windows in their walls

## Purpose

One rule decides which face of a wall is "outside", for its build-up and for its doors and windows alike, so that
exterior walls drawn the usual way get the right layers, doors that open inwards and windows that face out, with no
flags set.

## Behaviour

### Wall faces

- Looking along a wall from its start to its end, the **left face** is on the left and the **right face** on the
  right. A wall's build-up lists its layers from the left face to the right face.
- The starter template's exterior build-ups list the outside layer (render, insulation) first. An exterior wall
  therefore has its outside on the left face when it is drawn **clockwise** around the building, seen in plan from
  above. The wall tool's rectangle mode draws clockwise.

```
                      north wall: start ───────────▶ end
                ┌───────────────────────────────────────────┐
                │  outside: left faces, first layers         │
                │   ┌───────────────────────────────────┐   │
      west wall │   │                                   │   │ east wall
         ▲      │   │      inside: right faces          │   │     │
         │      │   │                                   │   │     ▼
       start    │   └───────────────────────────────────┘   │    end
                └───────────────────────────────────────────┘
                      south wall: end ◀─────────── start
```

### Openings

With **swap side** off:

- A window's **outer face** is the wall's **left** face: the frame is set back from that face, and the external
  sill, the shutters and the roller-shutter box lie on it. Its sashes open towards the right face, where the
  internal sill is.
- A door's leaves open towards the wall's **right** face. A hinged door's frame sits flush with the right face; a
  leaf sliding on the face, and a garage door, hang on the right face. Glazed balcony doors combine both: frame,
  shutters and roller box follow the window rule, the leaves open towards the right face.
- Hence, in an exterior wall drawn clockwise, windows face outwards and doors open inwards.

```
   An opening in a wall drawn from left to right, swap side off (plan):

               outside: external sill, shutters, roller box
   ════════════╗ ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔ ╔════════════   left face
   start  ─────║─ window frame ────║──────▶ end      (set back from the left face)
   ════════════╝                   ╚════════════   right face
                 ╲    ╱  sashes open towards the inside

   ════════════╗                   ╔════════════   left face
   start  ─────║───────────────────║──────▶ end
   ════════════╝▄▄▄▄▄ door frame ▄▄╚════════════   right face (frame flush with it)
               ┃               .·´
               ┃          .·´            the leaf opens towards the right face,
               ┃     .·´                 hinged at the jamb nearer the wall's start
               ┃.·´
```

- **Swap side** mirrors the opening across the middle of the wall: the outer face becomes the right face and the
  leaves open towards the left face.
- **Swap hand** is independent: it mirrors the opening along the wall, moving single hinges from the jamb nearer
  the wall's start to the other jamb and swapping the side sliding leaves park on.
- The four combinations are exact mirror images of one drawing, in plan and in 3D.
- Interior walls have no outside: the flags only choose the room a door opens into.
- Plain openings (voids) look the same from both sides; their flags change nothing visible.

### Placing and storing

- The door and window tools read the side of the wall the pointer is on: for a door, the side its leaves open
  towards; for a window, its outer face (SPEC-tools-005). Clicking inside the room for a door and outside the
  building for a window therefore gives unflipped openings in exterior walls drawn clockwise.
- The opening command's swap-side argument defaults to off, with the meaning above.
- Projects saved with the earlier, opposite meaning of swap side (native schema 1) are upgraded on opening: every
  opening's swap-side flag is inverted, so each door and window keeps its physical orientation (SPEC-files-001).

## Edge cases

- A wall of no thickness: the left face counts as lying on the left of the reference line, so the rule still
  decides which side the outer face is on.
- A wall drawn anticlockwise around a building has its build-up's first layer inside; its doors and windows need
  swap side, exactly as its layers would need reversing.

## Acceptance tests

1. Given a single swing door in a 30 cm wall with both flags off, its open leaf lies beyond the right face, nothing
   lies beyond the left face, and its hinge is on the right face at the jamb nearer the wall's start.
2. Given a casement window with shutters, flags off, the shutters lie beyond the left face, the frame lies in the
   left half of the wall, the external sill projects 3 cm beyond the left face and the internal sill 2 cm beyond
   the right face, and the sash swings beyond the right face.
3. Given the door and the window of tests 1–2 in all four flag combinations, swap side moves leaf, hinge,
   shutters, frame and sills to the opposite face, and swap hand moves the hinge to the other jamb, in plan and
   in 3D.
4. Given exterior walls drawn clockwise and openings placed with the opening command's defaults, every window's
   outer face lies outside the building outline and every door opens towards the inside.
5. Given the sample house, every exterior door and window is unflipped, every hinged door opens into the room it
   serves, and every window's frame stands in the outer half of its wall.
6. Given the door tool with the pointer on a wall's left side, the door opens towards the left face (swap side
   on); on the right side it opens towards the right face (off). Given the window tool with the pointer on a
   wall's left side, the outer face is the left face (off); on the right side, the right face (on).
7. Given a schema 1 project file, opening it inverts every opening's swap-side flag and keeps swap hand.

## Open questions

- Whether the opening tools should infer the outside from the building outline instead of the pointer side.

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
