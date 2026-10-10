---
id: SPEC-views-006
title: Construction phases in derived drawings
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ISO 7519:1991 Construction drawings — General principles of presentation
  - ISO 128-2:2020 Basic conventions for lines
  - Professional practice of Greek drawings for alterations and legalisations (υφιστάμενα, προς καθαίρεση, νέα)
---

# Construction phases in derived drawings

## Purpose

Alteration and extension projects need drawings that tell existing, demolished and new work apart at a glance,
in black-and-white print.

## Behaviour

| Phase | Cut areas | Outlines and lines |
| --- | --- | --- |
| New (default) | Material hatch | Black, solid |
| Existing, to remain | Solid light grey (RGB 200, 200, 200), no hatch | Grey (RGB 110, 110, 110), solid |
| Existing, to be demolished | White | Black, short dashes; the cut outline uses the minor cut pen |

1. The rules apply to every part of an element: cut areas, lines below and above the cut, symbols, lines beyond
   in sections and elevations, and 3D edges.
2. Cut areas of different phases never merge; each phase gets its own outline.
3. Trimming by junction priority (SPEC-views-001) applies across phases.

## Edge cases

- An element without a phase is new.

## Acceptance tests

1. Given an existing wall, then its cut fill is solid grey and its outline grey.
2. Given a demolished wall, then its cut outline is dashed and its fill white.

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
