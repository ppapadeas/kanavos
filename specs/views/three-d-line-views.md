---
id: SPEC-views-005
title: 3D views as line drawings
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - J. D. Foley, A. van Dam, S. K. Feiner, J. F. Hughes, Computer Graphics: Principles and Practice, 2nd ed., §6.5 (perspective-to-parallel transformation) and §15.3 (visible-line determination)
  - A. Appel, "The notion of quantitative invisibility and the machine rendering of solids", Proc. ACM National Conference, 1967
  - ISO 5456-3:1996 Technical drawings — Projection methods — Axonometric representations
  - ISO 5456-4:1996 Technical drawings — Projection methods — Central projection
---

# 3D views as line drawings

## Purpose

Permit and presentation sheets often carry an axonometric or a perspective of the building. A 3D view must
therefore be available as a vector line drawing that can be placed on a sheet at a scale.

## Behaviour

1. The view's camera defines the projection: parallel (axonometric) when the camera is parallel, central
   (perspective) otherwise.
2. Coordinates are metres on the projection plane: for a parallel camera, the plane through the target
   perpendicular to the viewing direction; for a perspective camera, the plane through the target, so that
   objects at the target's distance appear at true size. The view's scale then applies on sheets as for plans.
3. Perspective views are clipped at a near plane (1 % of the eye–target distance, at least 5 cm).
4. Visible edges are found as in SPEC-views-004 §Lines beyond. The perspective view is transformed into an
   equivalent parallel one, `(X, Y, Z) → (D·X/Z, D·Y/Z, −D/Z)` in camera coordinates, which keeps lines straight
   and depth order, so the same visibility rules apply.
5. Lines use the beyond pen; profile lines (between the building and the background) use the profile pen.
6. Layers are `<kind>.edge`.

## Edge cases

- A camera looking straight down uses the world X axis as the drawing's right.
- An empty model gives an empty drawing.

## Acceptance tests

1. Given the fixture house and a parallel camera from the south-west, then wall, window and railing edges
   appear and no edge of the far side shows through the walls.
2. Given a perspective camera, then the drawing is produced and its extent is in metres at the target plane.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending — Result: pending
