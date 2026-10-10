---
id: SPEC-schedules-001
title: Schedules of openings, spaces, walls, slabs, materials and objects
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - General construction and quantity-surveying practice (our own table layouts)
  - RFC 4180 (CSV) for the export format, with the locale's separator
---

# Schedules of openings, spaces, walls, slabs, materials and objects

## Purpose

List the model's elements as tables that stay in sync with the model, for sheets, review and export to
spreadsheets (CSV).

## Behaviour

Every schedule is a table with a title, columns, body rows and footer rows (totals, bold). Numbers have two
decimals in the locale's format (decimal comma in Greek); totals are sums of unrounded values. CSV export uses
`;` in Greek and `,` in English, with a UTF-8 byte-order mark. All headings come from the `schedules`
catalogue; object and opening style names come from the objects catalogue, falling back to the style id when it
has no text.

### Doors and windows

Openings of one kind are grouped into **types**: same style, width, height and sill to the millimetre. Types are
ordered by style, then width, height and sill. A type takes the tag shared by all its openings; otherwise an
automatic mark: prefix (`Θ` doors, `Π` windows, `Α` voids in Greek; `D`, `W`, `O` in English) and position.
Columns: mark, type (style name), width, height, sill, storeys where it occurs (lowest first), quantity. Footer:
total quantity.

### Spaces

One row per space, ordered by storey elevation, then number in natural order (`0.2` before `0.10`), then name.
Columns: number, name, category, storey, net area, perimeter, clear height. Clear height is the space's own
value, or the storey height less the thickest slab of the storey above covering the space's centroid, or less
the underside of a roof on the storey itself, whichever is lower. Footer: a subtotal per storey when there is
more than one storey, and the total area.

### Walls

Grouped by structure: the build-up name, or "material thickness cm" for single-material walls. Columns:
build-up, thickness, quantity, length (reference line), net side area (openings subtracted), volume (net area ×
thickness). Footer: totals.

### Slabs

Grouped by storey and structure. Columns: storey, build-up, thickness, quantity, net area, perimeter (outer and
hole edges), volume. Footer: totals.

### Material take-off

Per material: area = for every wall, slab or roof layer of that material, the element's net area; volume =
layer area × layer thickness, plus the solid volume of columns, beams, stairs and railings; mass = volume ×
density when the density is known. Footer: total volume and mass.

### Objects

Placed catalogue objects counted by category and type. Footer: total.

## Edge cases

- Elements whose geometry fails (missing build-up) are left out of quantity schedules.
- Openings without a storey list no storey.

## Acceptance tests

1. Given four windows of two sizes on two storeys, then two types, the larger with three instances on two
   storeys, marks Π1 and Π2, total 4, and CSV `1,20;1,40;0,90`.
2. Given two walls of 10 m and 6 m, 3 m high, 0.31 m thick, with a 1.2 × 1.5 m window, then length 16.00,
   area 46.20, volume 14.32.
3. Given two 10 × 8 m slabs of 0.275 m, then area 160.00, volume 44.00.
4. Given a 10 × 3 m wall of the starter's exterior build-up, then brick 30 m² / 6 m³ and plaster 60 m² / 0.9 m³.

## Open questions

- Opening schedules by storey (one table per storey) for large projects.

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
