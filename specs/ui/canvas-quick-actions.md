---
id: SPEC-ui-004
title: Quick actions next to the selection and the canvas context menu
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §3 (on-canvas editing: a strip with the 4–6 most likely operations that never covers the element)
  - docs/adr/0005-command-bus.md
---

# Quick actions and context menu

## Purpose

The few operations people do most often on a selected element are one click away, right next to it, without
looking for them in menus.

## Behaviour

### Quick-action strip (plan views)

- Shown when the selection tool is idle, something is selected, no button is held and no view animation runs.
- Placed centred above the selection's extent on screen with a 12-point gap; if there is no room above, below; if
  not below either, to the right. It is never placed over the selection; when no placement avoids it, the strip
  is not shown.
- Operations, most likely first, at most six:
  - a single straight wall: **split** at the point where the wall was clicked;
  - a single door or window: **flip hand** (hinges to the other jamb), **flip side** (opens to the other side);
    an empty opening offers flip hand only;
  - any selection that is not only openings: **copy up** (to the storey above, same position; only when there is a
    storey above), **rotate 90°** anticlockwise about the centre of the selection's extent, **mirror** about the
    vertical axis through that centre, **duplicate** (a copy moved to the right by the extent's width plus 0.5 m);
  - always: **delete**.
- Every operation is a single engine command (`wall.split`, `element.copy_to_storey`, `element.transform`,
  `element.update`, `element.delete`) and one undo step. Copies placed on the current storey become the selection;
  deleting clears it.
- Each button has a tooltip saying what it does.

### Context menu (plan, section and elevation views)

- A secondary click on an element not in the selection selects it first.
- Items: **delete**; **copy to storey** ▸ every other storey; **zoom to selection**; **zoom to fit**; **properties**
  (asks the application to show the selection's properties). Without a selection only zoom to fit is offered.

## Acceptance tests

1. Given a selected wall clicked at (5, 0), then the strip offers split first, then copy up, rotate, mirror, duplicate
   and delete, and each of them yields one command that applies to the document.
2. Given a selected door, then the strip offers exactly flip hand, flip side and delete; flip hand toggles the door's
   hinge side.
3. Given an empty selection, then no strip is shown.

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
