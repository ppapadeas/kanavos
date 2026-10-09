---
id: SPEC-files-004
title: Importing IFC models
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - ISO 16739-1:2024 (IFC 4.3) and the buildingSMART IFC 2x3 and IFC 4 documentation, entities named below
  - ISO 10303-21:2016 exchange structure (syntax, typed parameters, complex instances, string control
    directives)
  - ISO 8859-1 and ISO 8859-7 (code pages selectable inside Part 21 strings)
  - SPEC-files-003 (what Kanavos writes, which import must read back)
---

# Importing IFC models

## Purpose

People bring a model made in another program into Kanavos by exporting IFC there and opening it here: storeys,
walls with their build-ups, doors and windows, slabs, columns, beams and spaces become editable Kanavos elements,
and everything else is listed so nothing is lost silently.

## Behaviour

### Reading

1. Files in IFC 2x3, IFC 4 (including 4.1 and 4.2 drafts) and IFC 4.3 are accepted; other schemas are refused
   with the schema named. A file that is not valid Part 21 is refused with the line and column of the problem. A
   file without a project is refused.
2. Part 21 is read completely: comments, typed parameters, lists, enumerations, unset and derived values, binary
   values, complex instances (kept but not mapped), strings with doubled apostrophes and all control directives
   (`\\`, `\S\` with ISO 8859-1 or 8859-7 code pages, `\X\`, `\X2\`, `\X4\`). Strings stored as UTF-8 bytes are
   accepted; other non-ASCII bytes are read as ISO 8859-1.
3. Lengths are converted to metres from the project's length unit, including SI prefixes and conversion-based
   units such as inches; angles likewise to radians.
4. Placements are followed through any chain of local placements; cycles and chains deeper than 64 are cut.

### Mapping

5. The document starts from the starter template without its storeys (default) or from an empty document.
6. Storeys: elevation from the placement (else the elevation attribute); height from the gross height quantity,
   else the next storey, else the tallest element on it, else 3 m; below ground from `AboveGround` when given;
   a plan view each. A file without storeys gets one storey at ±0.00.
7. Elements are placed on the storey that contains them, directly, through a space, or through the element they
   are part of; otherwise on the highest storey at or below their base.
8. Walls need a vertical extrusion. The reference line comes from the straight or circular-arc axis; without an
   axis it is the centre line of the body. The thickness is the body's extent across the reference line; the
   reference line becomes the left face, right face or centre according to where the axis lies, or is moved to
   the centre. Height and base come from the body's vertical extent.
9. Wall, slab and roof materials: a layer set whose thickness matches the geometry (within 2 mm or 2 %) becomes a
   build-up — an existing one with the same name and layers is reused — with layers stacked in the positive
   direction reversed into Kanavos order; otherwise one material with the geometric thickness. Materials are
   matched by name or created, with a family guessed from the name and IFC category, and a colour from the
   material's style or a default per family.
10. Slabs: outline with holes from the largest vertical extrusion, top and thickness from the vertical extent.
    Openings cut through a slab become holes in its outline.
11. Columns: vertical rectangle or circle extrusions. Beams: horizontal rectangle or circle extrusions, with the
    profile axis closer to vertical as the depth.
12. Spaces: boundary and height from the extrusion; name from the long name and number from the name.
13. Doors, windows and empty openings in imported walls become openings: centre along the reference line (arc
    length for curved walls), width and height from the door's or window's overall size when given, else from the
    opening body, sill from the bottom of the body.
14. Flat roofs with their own extruded body become flat roofs (a banded solid on top becomes the parapet); objects
    written by Kanavos become objects again.
15. Each element keeps its tag, its phase (from `Status`), its user properties and, unless switched off, its
    `GlobalId` (property `ifc:GlobalId`).
16. The georeference comes from the map conversion: EPSG code, eastings, northings, altitude and rotation.

### Report

17. The report lists the schema, the instance count per type, the count mapped per type, every product not
    mapped with its id, type, `GlobalId` and reason (also counted per type and reason), the simplifications made,
    and file-level remarks.
18. Damaged files never crash the import: it either fails with an error or produces a consistent document.

## Edge cases

- Clipped walls are read without their clipping; boolean unions are read as their first operand.
- Mapped representations are read through their mapping transformation.
- A door or window without an opening in an imported wall is reported, not placed.
- Layer sets whose thickness disagrees with the geometry fall back to one material, noted in the report.

## Acceptance tests

1. Given a hand-written IFC 2x3 file in millimetres, the import gives two storeys (3 m apart), a 5 m wall from
   (1, 0) to (1, 5) with its axis on the right face, 0.25 m thick, 2.8 m high, whose build-up "Wall 250" lists
   gypsum plaster, brick, render (reversed from the positive layer direction); a 0.9 × 2.1 m door 1.45 m along the
   wall; a 29 m² slab with one hole; a 0.4 m round column; a 6 m beam with its top at 2.8 m; a 12 m² space named
   "Living room" numbered "101"; and reports the furnishing element as not mapped.
2. Given a hand-written IFC 4 file in metres, the import gives a wall without an axis centred on its body, marked
   existing, of a new concrete material; a window 2 m along it at a 1 m sill; a column moved by its mapped
   representation to (6, 6); an L-shaped 33 m² slab from a composite curve with a 1 m² shaft cut by an opening
   element; the roof slab of a roof without its own body; and the georeference with a rotation of −90°.
3. Given a file in inches, a 120 × 8 × 96 inch wall imports as 3.048 m × 0.2032 m × 2.4384 m.
4. Exporting the small house and importing the result preserves storeys, wall count, reference lines, lengths,
   thicknesses, heights and build-ups; openings with kind, position, size, style and parameters; the space's
   area, name, number, category and finish; the slab with its hole; the column; the flat roof with its parapet.
5. The full house round trip preserves a curved wall (bulge and window position), objects, georeference, tags and
   user properties, and reports railings, stairs and the pitched roof as not imported.
6. Identifiers survive import and a second export under a different project namespace.
7. Random edits to a valid file (bytes changed, removed, inserted; references redirected) never crash the import.

## Open questions

- Pitched roofs, stairs and railings from other programs need their own geometry analysis.
- Walls made of several layers as separate extrusions are read as one wall over their combined extent.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
