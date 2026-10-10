---
id: SPEC-ui-007
title: Attribute managers and context bar
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §1.2, §2
  - ISO 6946 (thermal transmittance of layers in series)
  - ν. 4067/2012 (ΝΟΚ) building terms
---

# Attribute managers and context bar

## Purpose

Edit the project's shared attributes — materials, build-ups, storeys, project information, site terms and
defaults — in live windows, and set tool parameters before drawing.

## Behaviour

### Managers (non-modal windows, centred when first opened)

- **Materials**: searchable list with colour swatches (alphabetical when not searching); new, duplicate, delete
  (disabled with an explanation while the material is used). Details: name, category; colour, cut hatch with a
  small preview, cut background (on/off and colour); junction priority, conductivity λ (W/mK) and density
  (kg/m³), each optional value clearable. A colour being picked is applied once the picker closes, so one pick
  is one undo step.
- **Build-ups**: list grouped by use (walls, slabs, roofs, any) with total thickness; new, duplicate, delete
  (disabled while used). Details: name, use, and the layer table — material, thickness (cm), structural core flag,
  the material's λ, move up/down, remove (not the last layer) — with "Add layer" (copies the last layer's
  material, 5 cm). Layer order is stated: walls left face to right face, slabs and roofs top to bottom. Under
  the table: total thickness, U-value (or why it is unknown) and a to-scale preview of the layers in their
  colours and hatch indications, core layers outlined.
- **Storeys**: one row per storey (highest first) with name, level, storey height, cut plane, below-ground flag,
  element count, and buttons to open its plan, duplicate and delete (with an inline confirmation). Option "move
  the storeys above" when a height changes. Buttons to add a storey above or below. Shows the top level of the
  building.
- **Project information**: title, location, municipality, owner, designer, designer details, date, revision,
  notes — the title-block fields.
- **Plot and building terms**: plot area and corner count (or none), draw the plot, clear it; building
  coefficient, coverage (%), maximum height, maximum floors, volume coefficient, setback Δ (empty means the
  height-based rule), natural ground level; the resulting allowed coverage and floor area; georeference:
  system (EPSG:2100 ΕΓΣΑ87), origin easting and northing, rotation to grid north, altitude of ±0,00.
- **Element defaults**: default build-ups for exterior and interior walls, floor and ground slabs, flat and
  pitched roofs, and the structural and railing materials.
- Every change is one engine command and one undo step.

### Context bar per tool

The tool's icon and name come first; its parameters follow and scroll sideways when they do not fit, so the search
entry always stays at the right edge. Long names in drop-downs end with "…" instead of widening the bar.

- Wall: chain, rectangle or arc; build-up (project default, any wall build-up with its thickness, or a single
  material with thickness in centimetres); height (automatic = storey height, with a reset); reference line
  (left face, axis, right face); base offset.
- Slab, roof, space, fill: how the outline is given — by its corners, as a rectangle, or by clicking inside
  walls (spaces and fills list the click first).
- Slab: build-up, top offset. Column: profile and size, material, height, rotation, rotation by a second click.
  Beam: profile and size, material, top (automatic = storey height).
- Door / window: style from the catalogue (choosing one takes its standard size and sill), width, height, sill
  (windows), position step. Opening: width, height, position step.
- Stair: shape, width, going, maximum riser, waist, gap between flights (half-turn shapes).
- Roof: kind (flat, mono-pitch, gable, hip); parapet with its height (flat) or pitch (others); overhang; base
  (automatic = storey height); build-up.
- Railing: height, infill, post spacing. Space: name, ΝΟΚ category, automatic numbering.
- Object: a picker with search and category filters; rotation, rotation by a second click, mirrored.
- Text: size, alignment, bold. Level: label. Grid line: label (empty continues the sequence). Section and
  elevation: depth and scale of the new view. Dimension: aligned, horizontal or vertical.
- Line, polyline, arc, circle, fill: line weight, pattern, colour, and hatch for fills.
- Select, measure and plot show their one-line description.
- The status bar adds a toggle for inference lines next to the snap toggles.

## Acceptance tests

1. Deleting a used material or build-up is not offered.
2. A build-up's U-value equals `1 / (Rsi + Σ d/λ + Rse)` with the surface resistances for its use, and is
   "unknown" when a layer's λ is missing.

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
