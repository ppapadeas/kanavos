---
id: SPEC-files-003
title: Exporting a model as IFC 4.3
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 16739-1:2024 (IFC 4.3), entities and property/quantity set templates named below
  - buildingSMART IFC 4.3 ADD2 documentation (IfcGloballyUniqueId compression, IfcMaterialLayerSetUsage,
    IfcMapConversion, IfcExtrudedAreaSolid, IfcFacetedBrep)
  - ISO 10303-21:2016 exchange structure (header and data sections, parameter encoding, string control
    directives)
  - RFC 9562 §5.5 (name-based UUIDs, version 5); FIPS 180-4 (SHA-1)
  - EPSG:2100 (GGRS87 / Greek Grid)
---

# Exporting a model as IFC 4.3

## Purpose

People send their model to consultants, authorities and other programs as an open IFC 4.3 file that keeps the
building's structure, element types, sizes, materials, quantities and position on the Greek grid.

## Behaviour

### File

1. The file is ISO 10303-21 clear text in pure ASCII with schema `IFC4X3_ADD2`. The header carries the file name,
   time stamp, author, organisation and originating application given by the caller.
2. Text outside printable ASCII is written with the `\X2\…\X0\` directive (`\X4\…\X0\` beyond the Basic
   Multilingual Plane). Apostrophes and backslashes are doubled.
3. Reals always contain a decimal point; they keep 12 significant digits when that changes them by less than
   10⁻¹¹ of their magnitude, else their exact shortest form; magnitudes below 10⁻¹² are written as zero.
4. Units are metre, square metre, cubic metre and radian. Representation contexts: `Model` (3D) with `Body`,
   `Axis` and `FootPrint` sub-contexts, and `Plan` (2D). True north follows the georeference rotation.
5. The same document and options always produce the same file.

### Identifiers

6. Every `GlobalId` is the version-5 UUID of a project namespace and a stable object name (`element:<id>`,
   `storey:<id>`, `rel:voids:<id>` …), written in the 22-character IFC compressed form.
7. The project namespace is the one the caller gives, else the version-5 UUID of the Kanavos namespace and the
   project's name, location, municipality and owner.
8. An element carrying a valid `ifc:GlobalId` property (kept by import) is written with that identifier, unless
   an earlier element already used it.

### Structure

9. One project, one site, one building, one storey per storey (placed at its elevation), linked by aggregation.
   Spaces are aggregated by their storey; building elements are contained in their storey; openings are linked
   only through their wall.
10. A georeference with any non-zero value is written as a map conversion from the model context to the projected
    CRS `EPSG:<code>`: eastings, northings, orthogonal height = altitude of ±0.00, X axis direction
    (cos r, −sin r) for a rotation r from project +Y to grid north, scale 1.

### Elements

11. Walls: a reference-line axis and a vertical extrusion of the plan outline by the height from the base. Two
    straight walls whose ends meet within 1 mm, with no third wall at that point and not parallel, are mitred:
    both outlines end on the line through the intersections of their matching faces, unless that corner lies
    more than four wall thicknesses from the meeting point. Curved walls have a trimmed-circle axis and a band
    outline tessellated within 5 mm.
12. Wall materials: a layer set usage along the wall's Y axis, layers in Kanavos order (left face → right face),
    stacked in the negative direction from the left face. A single material is a one-layer set.
13. Doors and windows: an opening element voiding the wall (a box through the wall plus 50 mm each side, at the
    opening's centre, sill and size) filled by a door or window with overall width and height, placed relative to
    the opening; empty openings are an opening element only.
14. Slabs: extruded down from the top surface by the thickness, holes as voids; layer set usage along Z, layers
    top → bottom, negative direction from the top.
15. Columns and beams: extruded rectangle or circle profiles; beams with their top at the given level.
16. Spaces: extruded boundary by the space height (storey height when not set); name = number, long name = name;
    the ΝΟΚ category and finish in `Kanavos_Space`.
17. Stairs: per flight an extruded side profile (sawtooth over a soffit parallel to the pitch line, waist measured
    perpendicular); per landing an extruded slab. A flight of k risers has k − 1 treads; two-flight shapes split
    the risers as given or evenly; quarter turns use a square landing, half turns a landing spanning both flights
    and the gap.
18. Roofs: flat roofs as an extrusion down from the roof top, with a parapet band when present. Pitched roofs with
    a convex outline as a closed faceted solid: the top is the lower envelope of one plane per sloped edge rising
    inwards at its pitch from the eaves height; the bottom is the same planes lowered by thickness / cos(pitch);
    gable edges are vertical. Concave pitched outlines are exported flat with a warning.
19. Railings: per path segment a top rail and an infill panel. Objects: furniture, sanitary terminal or proxy by
    catalogue family, with a placeholder box from the width, depth and height parameters.
20. Annotation (dimensions, text, levels, drafting, grids, underlays) is not exported and is counted in the report.
21. A caller-supplied mesh for an element replaces its body, written as a faceted solid in the element's frame.
22. Property sets: the IFC common sets with `Status` from the phase (and `IsExternal`, `LoadBearing`,
    `Reference` where meaningful), `Kanavos_Opening`, `Kanavos_Space`, `Kanavos_Object` and `Kanavos_Properties`
    (user properties). Quantity sets for walls, openings, doors, windows, slabs, columns, beams, spaces, roofs,
    railings and storeys, computed from the element parameters.

## Edge cases

- Elements with zero length, thickness, height or area are skipped with a warning; an opening whose wall was
  skipped is skipped too.
- A copied element carrying the same stored `GlobalId` as another gets a derived identifier.
- Elements without a storey are contained in the building.

## Acceptance tests

1. Given the small house (two storeys, four walls with a door and a window, a slab with a hole, a space, a column,
   a flat roof with a parapet), the export parses back and has exactly one project, site and building, two
   storeys, four walls, two opening elements, one door, one window, two voids and two fills relations, one slab,
   space, column and roof; no reference dangles.
2. Every rooted entity has a valid, unique `GlobalId`; exporting twice gives identical files; a different project
   namespace changes every identifier and nothing else.
3. The wall quantities match: length 10 m, width 0.31 m, height 3.2 m, net side area 32 − 2.2 m²; the space net
   floor area is 9.69 × 7.69 m²; the slab net area excludes its hole.
4. The four walls share one layer set usage (axis 2, negative, offset 0.155 m) whose layers equal the build-up.
5. Greek names are written with `\X2\` and read back unchanged.
6. With a georeference, the map conversion and `EPSG:2100` CRS carry the eastings, northings, altitude and
   rotation.
7. A mesh supplied for the column replaces its body with a faceted solid of 12 triangles.
8. The full house exports a stair, beam, railing, pitched (hip) roof as a faceted solid, furniture, a sanitary
   terminal, a proxy, a curved wall with a trimmed-circle axis, and reports the text note as skipped.

## Open questions

- Whether to export space boundaries (`IfcRelSpaceBoundary`) for energy analysis.
- Which model view definition to declare once buildingSMART publishes IFC 4.3 views beyond reference views.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
