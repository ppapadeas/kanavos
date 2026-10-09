---
id: SPEC-sheets-002
title: Permit drawing set
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ν. 5306/2026 (ΚΧΠ) άρθρα 325 και 326 (permit dossier: architectural study, topographic plan, coverage diagram)
  - General practice for architectural permit drawings in Greece (plans, sections, elevations at 1:50–1:100)
---

# Permit drawing set

## Purpose

One command produces the architectural drawings of a Greek building permit as numbered sheets, creating the
views that are missing.

## Behaviour

`permit_set(document) → document` is a pure function (the engine wraps it as the `sheet.permit_set` command).

### Views

Found or created:

- **site plan** (1:200) when a plot exists, and **coverage diagram** (1:200);
- **plans** of the storeys that hold elements (all plans when nothing is modelled);
- **sections**, at the most common plan scale, through a frame aligned with the longest edge of the building
  footprint: **Α-Α** along the longer side, through the first stair when one lies inside the building, else
  through the middle, looking across the building; **Β-Β** across, through the middle. Each runs 2 m past the
  building; its depth reaches 1 m beyond the far side. A section with marker Α (or A) or Β (or B) is reused;
- **elevations**, one per façade direction of the frame (±long axis, ±short axis): a line 3 m outside the
  façade, 2 m longer than the building on each side, looking at it, named by the compass direction the façade
  faces (8 sectors from grid north: "Όψη νότια", "Όψη βορειοανατολική" …), marked 1–4 in bearing order. An
  existing elevation looking within 30° of the same direction is reused;
- **tables**: the area table view (created if missing), then existing door, window and space schedule views.

Without a building footprint no section or elevation is created.

### Sheets

Groups, in order: site plan with coverage diagram; one sheet per plan; sections; elevations; an indicative 3D
view (ΚΧΠ 325 §3 η'), only when the 3D view can be drawn as lines; tables. Each group
takes as many consecutive views as fit on an A1 sheet (a larger view gets its own sheet), and each sheet the
smallest paper that holds it (sheets.md). Sheets are numbered `Α-01`, `Α-02`, … after the highest existing
`Α-` number; groups split over several sheets are named "… (1/2)". Every sheet has a title block.

## Edge cases

- Running the command again creates no new views and adds a second set numbered after the first.
- An empty project still gets the coverage diagram, the plans and the tables.

## Acceptance tests

1. The sample house gets six sheets Α-01…Α-06: site plan + coverage diagram; ground plan; first-floor plan; two
   sections; four elevations; tables.
2. The elevations are named north, east, south and west with grid north 12° from +Y.
3. Every viewport and its label band lies inside the frame, clear of the title block and of each other.
4. A second run numbers its first sheet Α-07 and its last Α-12.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
