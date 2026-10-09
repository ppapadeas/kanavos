---
id: SPEC-geometry-002
title: Weighted straight skeleton for roof planes
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - O. Aichholzer, F. Aurenhammer, D. Alberts, B. Gärtner, "A Novel Type of Skeleton for Polygons", J. UCS 1(12), 1995
  - P. Felkel, Š. Obdržálek, "Straight Skeleton Implementation", SCCG 1998
  - D. Eppstein, J. Erickson, "Raising Roofs, Crashing Cycles, and Playing Pool", DCG 22, 1999
  - T. Biedl, M. Held, S. Huber, D. Kaaser, P. Palfrader, "Weighted Straight Skeletons in the Plane", CGTA 48(2), 2015
---

# Weighted straight skeleton for roof planes

## Purpose

Hip, valley and gable roofs over non-convex outlines (L, T, U shapes) need the region of each roof plane. The
weighted straight skeleton gives exactly that: with every eave edge moving inwards at a speed equal to the cotangent
of its pitch, the time at which the moving edges reach a point is the roof height there.

## Behaviour

- Input: a simple polygon (either orientation) and one speed per edge, finite and not negative, at least one
  positive. Edge `i` runs from vertex `i` to vertex `i + 1`; results refer to input edge numbers.
- Every edge moves inwards, parallel to itself, at its speed; speed zero keeps it in place (a gable).
- The moving polygon changes at edge events (an edge shrinks to nothing) and split events (a reflex corner runs
  into an edge on the other side). Events are found by exhaustive search, earliest first; ties prefer edge events.
- After each event the moving polygon is cleaned at that instant: zero-length edges that are not growing are
  merged, zero-width spikes between opposite edges are cut back, rings without area are closed.
- Two parallel edges that become neighbours with different speeds: the faster edge overruns the slower one, which
  ends there. Collinear neighbouring input edges with different speeds are separated by a fixed (zero-speed)
  connecting edge, a vertical step between the two roof planes.
- Collinear neighbouring input edges with the same speed are merged into one face listing both edges.
- Output per face: the input edges, the edge line (inward normal and offset), the speed and the swept region
  (polygons). The time function of a face is `(normal · p − offset) / speed`.
- The faces must tile the polygon (area check to 10⁻⁶ relative); otherwise an error is returned rather than a wrong
  roof.

## Edge cases

- Rectangles and orthogonal outlines produce many simultaneous events at the same point; they are handled by the
  one-event-then-clean-up rule.
- A region enclosed only by gables and edges moving away from it is never reached; the skeleton reports that the
  moving polygon does not collapse.
- Self-crossing outlines and outlines that double back are rejected.

## Acceptance tests

1. A 4 × 4 square with equal speeds gives four triangular faces of area 4 and a peak at time 2.
2. A 10 × 6 rectangle with equal speeds gives trapezoids of 21 and triangles of 9.
3. The same rectangle with the short edges at speed zero gives two faces of 30 (gable roof); with only the south
   edge moving one face of 60 and maximum time 6 (mono-pitch).
4. An L of 75 m² gives faces tiling it, the south face being the trapezoid of area 18.75, maximum time 2.5.
5. Doubling the south speed of a gabled 10 × 6 rectangle moves the ridge to 4 m from the south eave.
6. For random convex polygons with random speeds, every face's time at its centroid equals the minimum over all
   faces (lower envelope).
7. For 1,000 random orthogonal outlines and 500 random star-shaped outlines, with equal and random speeds, the
   faces tile the outline.

## Open questions

- Polygons with holes (courtyard roofs) are not supported yet.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
