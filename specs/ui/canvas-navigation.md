---
id: SPEC-ui-002
title: Drawing canvas — panning and zooming
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §1 (nothing is modal), §3 (shortcuts, view navigation kept out of the history)
  - Own professional practice with drawing software in general
---

# Drawing canvas — navigation

## Purpose

Move around a drawing quickly and precisely with one hand on the mouse, without leaving the current tool.

## Behaviour

- **Pan**: drag with the middle button, or hold `Space` and drag with the primary button. The drawing follows the
  pointer exactly. Panning never reaches the active tool, and works in every tool.
- **Zoom with the wheel** about the point under the pointer: that drawing point stays under the pointer. One wheel
  notch zooms by about ×1.2. Trackpad pinch zooms the same way; horizontal scrolling pans sideways.
- **Zoom keys**: `+` (or `=`) zooms in ×1.25, `−` zooms out, about the pointer when it is over the canvas, otherwise
  about the centre. While a value is being typed into a tool, `+` and `−` belong to the typed value.
- **Zoom extents**: `Home`, a double click with the middle button, or the context menu fit the whole drawing (the
  whole page for sheets) with a margin. On first display a view is fitted at once.
- **Zoom to selection**: from the context menu (or by the shell) fits the selected elements shown in this view.
- Key-driven and fitting zooms are animated (each frame covers about a third of the remaining distance, in
  logarithmic zoom), the wheel is immediate.
- **Limits**: model views from 0.2 to 200 000 points per metre; sheets from 0.02 to 200 points per millimetre.
- **Per view**: every view tab keeps its own centre and zoom.
- View navigation is not part of the model history (undo does not undo a pan).

## Edge cases

- A degenerate drawing (single point or line) is fitted with a minimum extent of 1 m (10 mm on sheets).
- An empty drawing starts centred on the origin at 40 points per metre.
- Pointer over a menu or the quick-action strip: the wheel does not zoom the canvas.

## Acceptance tests

1. Given a view at zoom 50, when zooming ×2.5 about a screen point, then the drawing point under it stays fixed and the
   zoom is 125.
2. Given zoom limits [1, 100], when zooming ×1000, then the zoom is 100; when zooming by a non-finite factor, nothing
   changes.
3. Given a drag of (30, −20) points, then a grabbed drawing point moves by (30, −20) points on screen.
4. Given a 20 × 10 m drawing in a 1000 × 600 point canvas with 50-point margins, when fitting, then the zoom is 45 and
   the drawing is fully visible.
5. Given an animation towards a target, then it reaches the target exactly within a bounded number of frames.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
