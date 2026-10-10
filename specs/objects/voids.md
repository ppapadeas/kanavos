---
id: SPEC-objects-004
title: Plain openings
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - SPEC-objects-001 (coordinates, layers, line weights)
  - ISO 128-2:2020 (dashed lines for edges above the cut plane)
  - General architectural drafting practice in Greece and Europe
---

# Plain openings

## Purpose

Architects cut openings without a door or window (passages, serving hatches, arches without leaves) and want the
wall above them shown in plan, optionally with lined reveals.

## Behaviour

- `void.rectangular` (Άνοιγμα χωρίς κούφωμα), default 1.00 × 2.20 on the floor: no parts in 3D. In plan, when
  `lintel` is on (default), the wall above the opening is shown by dashed lines along both wall faces across the
  opening (layer `opening.symbol.above`).
- `void.lined` (Άνοιγμα με επένδυση λαμπάδων): as above, plus lining boards `lining` thick (2.5 cm, at most a
  quarter of the width) on both jambs across the whole wall, drawn as cut profiles in plan; in 3D, timber boards
  on both jambs, under the head and, for openings raised above the floor, on the sill.
- Seen from below (an opening wholly above the cut plane), both wall faces across the opening are dashed.

## Edge cases

- With `lintel` off, a plain opening draws nothing; the gap in the wall is shown by the wall itself.

## Acceptance tests

1. Given a plain opening, then its symbol is two dashed lines on `opening.symbol.above` and it has no 3D parts.
2. Given a lined opening raised 0.9 m above the floor, then it has four lining boards.

## Open questions

- Arched heads.

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
