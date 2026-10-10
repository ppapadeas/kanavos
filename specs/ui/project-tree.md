---
id: SPEC-ui-002
title: Project tree
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §2, §4
---

# Project tree

## Purpose

Show the structure of the project — site, storeys, views, sheets, schedules — and open any of them in one click.

## Behaviour

- Header: a small "PROJECT" caption with a collapse button, then the project title (click opens the project
  information window).
- Groups, each collapsible, with an icon, a title and buttons on the right:
  - **Site** (button: plot and building terms). Rows: "Building terms" with the plot area (or "no plot"),
    then the site plan and coverage diagram views with their scales.
  - **Storeys**, highest first (buttons: new storey on top; storeys window). Each row shows the name and the
    floor level with sign (`+3,20`, `±0,00`); the active storey is highlighted. Click: make it active and open
    its plan. Double-click: rename in place. Context menu: open plan, rename, duplicate, new storey above, new
    storey below, properties, delete.
  - **Views** (buttons: new section, new elevation — they start the section or elevation tool on a plan),
    grouped under Sections, Elevations and 3D captions, each with its scale. Click opens; double-click renames;
    context menu: open, rename, delete. Storey plans are listed under Storeys, not here.
  - **Sheets** (buttons: new sheet; generate the permit set). Rows show number, title and paper size. An empty
    list offers the permit-set button inline.
  - **Schedules**: doors, windows, spaces, walls, materials, and the ΝΟΚ area table.
- Rows of open tabs are highlighted when their tab is focused.
- New storey on top: elevation at the top of the highest storey, same height, name "Floor n". New storey below:
  below-ground storey of 3,00 m under the lowest one, named "Basement".
- Duplicate storey: a new storey on top of the building with the same height and cut height and a copy of every
  element, as one undo step.
- Deleting a storey, view or sheet asks for confirmation, saying what will be removed (for storeys, how many
  elements) and that it can be undone. The last storey cannot be deleted. In-place rename commits on Enter or
  when focus leaves, and is abandoned with Esc or when empty.

## Edge cases

- Views whose kind is a schedule or the area table are reached through the Schedules group.
- A storey's plan view cannot be deleted on its own (it goes with the storey).

## Acceptance tests

1. Given the sample house, when the ground floor is duplicated, then there is one more storey, it holds as many
   elements as the ground floor, and history has one new step.
2. Given the starter project, when a storey is added above and one below, then there are four storeys and the
   lowest is below ground.

## Open questions

- Dragging views onto sheets from the tree.

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
