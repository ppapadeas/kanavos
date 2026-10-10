---
id: SPEC-ui-005
title: 3D viewport — navigation, picking, storey filter and section box
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §2 (plan + 3D pair, synced selection and hover), §6 (colour carries meaning)
  - Textbook computer graphics: perspective and parallel projection, Sutherland–Hodgman clipping, the painter's
    algorithm, Möller–Trumbore ray–triangle intersection (T. Möller, B. Trumbore, 1997)
---

# 3D viewport

## Purpose

See the building as a solid model, turn around it, look inside, and pick elements, with the same selection as the
drawings.

## Behaviour

### Renderers

- The viewport draws on the GPU (`kanavos-render`, ADR-0007) when the application runs on wgpu and the renderer
  can be created; otherwise, or when `KANAVOS_3D_SOFTWARE` is set, it draws with the software renderer described
  below. Camera, navigation, picking semantics, the storey filter and the section box are the same on both; the
  status line names the renderer (and why the software one is used).
- On the GPU, hover picks run asynchronously and a click picks synchronously; selection uses the same blue as the
  drawings, and hidden parts of the selection show as dashed edges.

### Display (software renderer)

- Every building part of the model is shown with its material colour, flat-shaded per face from a soft light from
  the sky above, a darker bounce from the ground and a sun from the south-west; faces turned to the sun are lighter.
- Glass is translucent and seen from both sides.
- Feature edges (where faces meet at more than 25°, and open boundaries) are drawn as thin dark lines where they are
  visible; edges hidden behind faces are not drawn. Edges can be switched off.
- A light ground plane under the model, fading towards its rim, is shown when looking from above; the background is
  a soft sky gradient.
- Selected elements are tinted with the selection colour; the element under the pointer is tinted lightly.
- When the model has no building parts, a short message says so.

### Navigation

- **Orbit** with a primary drag about the target point; the pitch is limited to just short of straight down/up.
- **Pan** with a middle or secondary drag; the target plane follows the pointer.
- **Zoom** with the wheel towards the point under the pointer (the model surface hit there, otherwise the plane
  through the target facing the viewer); one notch is about ×1.2.
- **Zoom to fit** (`Home`, double middle click, or the button) frames the visible model.
- **Standard views**: top (north up), front (from the south), side (from the east), axonometric (from the
  south-east, 30° above). Changes of view are animated.
- **Projection**: perspective or parallel; the parallel view keeps the visible height of the perspective view.
- The camera can be read from and written to a 3D view of the model.

### Picking

- A click selects the element whose surface is nearest along the pointer ray (Shift adds, Ctrl toggles, a click on
  nothing clears). Hidden storeys and parts outside the section box cannot be picked.
- `Delete` deletes the selection; `Esc` clears it.

### Storey filter

- All storeys; one storey only; or a storey and everything below it.

### Section box

- An axis-aligned box: only what lies inside it is drawn. Turning it on starts around the selection (0.5 m margin)
  or, with nothing selected, around the whole model cut at half its height. Its six faces can be moved by typing or
  dragging values in metres.
- Where the box cuts a solid, the cut face is closed with a cap in the material colour darkened towards grey, with its
  outline drawn as an edge.

## Performance

- The scene is prepared once per model revision; large faces are subdivided (at most a sixtieth of the scene, at
  least 0.5 m, within a budget of 60 000 triangles).
- Interactive for about 50 000 triangles on the software path: while the camera moves (any change of camera since
  the previous frame), edges are skipped above 15 000 triangles and the visibility buffer runs at about a third of
  the screen resolution (three quarters when still).

## Edge cases

- Triangles crossing the near plane are clipped, never drawn inverted.
- Interpenetrating solids (e.g. walls overlapping at unresolved corners) are drawn without the faces of one showing
  through the other, up to a couple of pixels along the boundary.

## Acceptance tests

1. The target projects to the centre of the viewport in both projections; up is up on screen.
2. The top view shows north up and east to the right.
3. Zooming keeps the focus point fixed on screen; panning moves the target plane with the pointer.
4. A pick ray through a pixel projects back onto that pixel.
5. A cube seen from a general direction draws three faces (half its triangles) and culls the rest; of its 12 edges,
   the 9 on the visible faces show.
6. Of two cubes in line of sight, the nearer is drawn last.
7. A box cutting a 2 m cube at half height produces one 4 m² cap facing up.
8. A box around nothing draws nothing.

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
