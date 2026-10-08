# MVP scope: "Permit set for a house"

## Definition of done

A practising architect models a real one- or two-storey house in Kanavos, from survey to sheets, and submits the
architectural drawings for a Greek building permit (e‑Άδειες) exported from Kanavos, with no other CAD/BIM tool
needed for those drawings.

Parity with any commercial BIM product is **not** a goal. Depth on one real workflow is.

## In scope

### Site and setup
- New project from a template (our own), units in metres with decimal comma in Greek locale.
- Import DXF survey (τοπογραφικό) as an underlay, georeferenced to ΕΓΣΑ87 (EPSG:2100).
- Plot boundary, building line (οικοδομική γραμμή), required setbacks, terrain as a simple surface from survey points.
- Building terms per ΝΟΚ: συντελεστής δόμησης, ποσοστό κάλυψης, μέγιστο ύψος, αριθμός ορόφων.

### Modelling
- Storeys with elevations and heights.
- Walls: straight and arc, reference line left/centre/right, layered build-ups, automatic junctions by priority.
- Slabs: polygon with holes, build-ups, edge offset.
- Columns and beams: rectangular and circular profiles.
- Openings: parametric door and window objects (our own), sill, head, reveal, swing.
- Stairs: straight, L and U runs with landings, rise/going rules.
- Roofs: flat roofs with parapets and single-plane pitched roofs; multi-plane hip roofs in a later release.
- Railings: simple post-and-rail along a path.
- Spaces: auto-detected from bounding elements, with name, number, use and ΝΟΚ category; net and gross areas.

### Drawings
- Plans per storey at a cut plane, with projection below/above and overhead lines.
- Sections and elevations with exact hidden-line removal, material hatches at cut.
- Dimensions: linear, chained, automatic for openings and walls; levels (στάθμες); labels; text; hatches.
- Line weights by material and cut/projection; visibility sets; display rules (basic).

### Greek regulation tools
- Live ΝΟΚ meters (κάλυψη, δόμηση, όγκος, ύψος).
- Area tables per storey and space category.
- Διάγραμμα κάλυψης generated from the model.
- Permit sheet-set preset with scales, title block and sheet numbering.

### Output
- Sheets in A-series sizes with our title block (editable).
- Vector PDF (and PDF/A), DXF per sheet, IFC 4.3 export, glTF export, CSV schedules.

### Import
- IFC 4.3 / IFC 2x3 import of walls, slabs, columns, beams, openings and spaces as editable Kanavos elements where they
  map cleanly; everything else as read-only reference geometry.

### Automation
- Headless CLI: open, convert, export sheets and schedules, run a script.
- Control protocol and MCP adapter covering every command.

## Out of scope for the MVP

- Real-time multi-user collaboration and server-based sharing.
- Photorealistic rendering (glTF export covers that through other tools).
- MEP, structural analysis, energy calculations (ΚΕΝΑΚ) — interfaces planned, not built.
- Curtain walls, complex freeform roofs, shells.
- Compatibility with vendor library-object scripting languages (needs legal review first).
- Reading any proprietary native project format (permanently out of scope, see the clean-room protocol §7).

## Success metrics

| Metric | Target |
| --- | --- |
| Benchmark task (two-storey house, survey to permit set) | ≤ 90 min first use, ≤ 45 min after one week |
| Crash-free sessions | ≥ 99.9 % |
| Area table matches manual calculation | Exact to 0.01 m² on 10 reference projects |
| IFC export validity | Passes buildingSMART validation for the exported entities |
| Pilot | 3 permit submissions prepared in Kanavos |
