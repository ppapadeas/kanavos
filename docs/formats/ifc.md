# IFC import and export

Kanavos exchanges models with other programs through IFC (ISO 16739-1), written and read in the clear-text
encoding of ISO 10303-21. The implementation is the `kanavos-ifc` crate; the behaviour is specified in
[`specs/files/ifc-export.md`](../../specs/files/ifc-export.md) and
[`specs/files/ifc-import.md`](../../specs/files/ifc-import.md).

- **Export** writes IFC 4.3 (`FILE_SCHEMA(('IFC4X3_ADD2'))`).
- **Import** reads IFC 2x3, IFC 4 and IFC 4.3 files.
- Both use the crate's own Part 21 reader and writer (`kanavos_ifc::step`); no external STEP library is involved.

## 1. Export

### 1.1 File

| Item | Value |
| --- | --- |
| Header | `FILE_DESCRIPTION(('ViewDefinition [DesignTransferView]'),'2;1')`, `FILE_NAME` with the caller's file name, time stamp, author, organisation and application, `FILE_SCHEMA(('IFC4X3_ADD2'))` |
| Encoding | Pure ASCII. Text outside printable ASCII (Greek, symbols, line breaks) is written with `\X2\…\X0\` (and `\X4\…\X0\` outside the Basic Multilingual Plane); `'` and `\` are doubled. |
| Numbers | Reals always have a decimal point (`3.`, `1.E-5`); values are rounded to 12 significant digits when that changes them by less than 10⁻¹¹ relative, and magnitudes below 10⁻¹² are written as `0.` |
| Units | Metre, square metre, cubic metre, radian (`IfcSIUnit`, `IfcUnitAssignment`) |
| Contexts | `Model` (3D, precision a tenth of the project tolerance, true north from the georeference) with sub-contexts `Body` (MODEL_VIEW), `Axis` (GRAPH_VIEW), `FootPrint` (PLAN_VIEW); `Plan` (2D) |
| Owner history | Not written (optional in IFC 4 and later) |
| Order | Deterministic: the same document and options always give the same file |

### 1.2 Identifiers

Every `GlobalId` is derived, never random:

1. A project namespace UUID: the one the caller passes (`ExportOptions::project_guid`), or else the document's
   permanent project id ([`../format.md`](../format.md) §3.2.1). Renaming the project or editing its information
   therefore keeps every identifier. Only a document without a project id (an older file not saved since, or a
   fresh IFC import) falls back to the version-5 UUID of the Kanavos namespace (`urn:kanavos:ifc` in the RFC 9562
   URL namespace) and the project's name, location, municipality and owner.
2. Each object gets the version-5 UUID of the project namespace and a stable name: `project`, `site`, `building`,
   `storey:<id>`, `element:<id>`, `opening:<id>` (the void of a door or window), `pset:<owner>:<set name>`,
   `qto:<owner>:<set name>`, `rel:contains:storey:<id>`, `rel:aggregates:…`, `rel:voids:<id>`, `rel:fills:<id>`,
   `rel:material:<key>`, `rel:pset:…`, `rel:qto:…`.
3. The 128 bits are written in the IFC compressed form (22 characters of `0-9A-Za-z_$`, most significant first).

Exporting the same project twice gives the same identifiers, so other tools can match objects between issues of
a model. An element imported from IFC keeps its original `GlobalId` as its external id (`Element::external_id`);
export reuses it when it is a valid IFC identifier not yet used in the file. Copies of elements have no external
id, so they get derived identifiers. (Files from before the field kept the identifier in the user property
`ifc:GlobalId`; it is moved into the field when such a file is opened, and export still honours the property for
documents built in memory that way.)

### 1.3 Spatial structure

| Kanavos | IFC |
| --- | --- |
| Project information | `IfcProject` (Name = title, LongName = location, Description = notes) |
| Site | `IfcSite` (Name = location; `FootPrint` curve of the plot when drawn; RefElevation = altitude of ±0.00) |
| Georeference (when easting, northing, altitude or rotation is set) | `IfcMapConversion` from the `Model` context to `IfcProjectedCRS` `EPSG:<code>` (Eastings, Northings, OrthogonalHeight = altitude, XAxisAbscissa = cos r, XAxisOrdinate = −sin r for a rotation r from project +Y to grid north, Scale 1) |
| — | `IfcBuilding` (Name = project title, address from location and municipality, ElevationOfRefHeight = altitude, ElevationOfTerrain = ground level) |
| Storey | `IfcBuildingStorey` placed at its elevation (Name, Elevation); `Pset_BuildingStoreyCommon.AboveGround`; `Qto_BuildingStoreyBaseQuantities.GrossHeight` = floor-to-floor height |
| Relations | `IfcRelAggregates` project → site → building → storeys, storey → spaces; `IfcRelContainedInSpatialStructure` storey → building elements |

### 1.4 Elements

Every product has `Name` = its tag (or `<category> <id>`), `Tag` = its tag, and is placed relative to its storey.
Shared by all elements: `Status` (NEW / EXISTING / DEMOLISH from the phase) in the common property set, and the
user properties in `Kanavos_Properties`.

| Kanavos | IFC entity | Placement | Geometry | Material | Data |
| --- | --- | --- | --- | --- | --- |
| Wall (straight) | `IfcWall` (NOTDEFINED) | At the start of the reference line, X along it, Z at the base | `Axis`: polyline along the reference line. `Body`: extrusion of the plan outline by the height — a rectangle profile, or the outline polygon where corners are mitred | `IfcMaterialLayerSetUsage` (AXIS2, NEGATIVE, offset = left face) of the build-up, or of a one-layer set for a single material | `Pset_WallCommon` (Reference = build-up, IsExternal = uses the default exterior build-up, Status); `Qto_WallBaseQuantities` (Length, Width, Height, GrossFootprintArea, Gross/NetSideArea, Gross/NetVolume) |
| Wall (curved) | `IfcWall` | At the start, X along the chord | `Axis`: `IfcTrimmedCurve` of an `IfcCircle` (sense = bulge sign); `Body`: extrusion of the tessellated band (≤ 5 mm deviation) | as above | as above |
| Door, window | `IfcOpeningElement` + `IfcRelVoidsElement` to the wall; `IfcDoor` / `IfcWindow` + `IfcRelFillsElement` | Opening: on the reference line at the opening centre, at the sill, relative to the wall. Door/window: relative to the opening | Opening: box width × (thickness + 0.1 m) × height. Door/window: panel box width × min(max(thickness, 20 mm), 80 mm) × height in the middle of the wall | — | OverallWidth, OverallHeight, ObjectType = object style; `Pset_DoorCommon`/`Pset_WindowCommon`; `Kanavos_Opening` (Style, FlipSide, FlipHand, style parameters); `Qto_DoorBaseQuantities`/`Qto_WindowBaseQuantities`, `Qto_OpeningElementBaseQuantities` |
| Empty opening | `IfcOpeningElement` + `IfcRelVoidsElement` | as above | box as above | — | `Kanavos_Opening` |
| Slab | `IfcSlab` (FLOOR) | At the top surface | Extrusion of the outline (holes as `IfcArbitraryProfileDefWithVoids`) downwards by the thickness | `IfcMaterialLayerSetUsage` (AXIS3, NEGATIVE, offset 0): layers top → bottom | `Pset_SlabCommon`; `Qto_SlabBaseQuantities` (Width = thickness, Perimeter, Gross/NetArea, Gross/NetVolume) |
| Column | `IfcColumn` (COLUMN) | At the centre and base, rotated | Extrusion of `IfcRectangleProfileDef` (width along X, depth along Y) or `IfcCircleProfileDef` | `IfcMaterial` | `Pset_ColumnCommon` (LoadBearing); `Qto_ColumnBaseQuantities` |
| Beam | `IfcBeam` (BEAM) | At the start, X along the beam, Z at the top | `Axis` polyline; extrusion of the cross-section along the beam | `IfcMaterial` | `Pset_BeamCommon`; `Qto_BeamBaseQuantities` |
| Space | `IfcSpace` (INTERNAL, EXTERNAL for balconies, semi-open spaces and pilotis, PARKING) | At the storey floor | `FootPrint` polyline; extrusion of the boundary by the space height (storey height when not set) | — | Name = number, LongName = name; `Pset_SpaceCommon` (Reference, IsExternal); `Kanavos_Space` (Category = ΝΟΚ use category key, Finish); `Qto_SpaceBaseQuantities` (Height, perimeters, floor areas, volumes) |
| Stair | `IfcStair` (STRAIGHT_RUN, QUARTER_TURN, HALF_TURN) | At the first riser, X along the walking direction | One extruded side profile per flight (sawtooth over a soffit parallel to the pitch line, waist measured perpendicular), extruded across the width; extruded landings | `IfcMaterial` | `Pset_StairCommon` (NumberOfRiser, NumberOfTreads, RiserHeight, TreadLength) |
| Roof (flat) | `IfcRoof` (FLAT_ROOF) | At the storey floor | Extrusion of the outline downwards from the roof top, holes as `IfcArbitraryProfileDefWithVoids`; parapet as an extruded band (outline minus the outline inset by the parapet thickness) | `IfcMaterialLayerSetUsage` (AXIS3, NEGATIVE, offset = roof base) | `Pset_RoofCommon`; `Qto_RoofBaseQuantities` (GrossArea over the whole outline, NetArea without the holes, ProjectedArea without the holes) |
| Roof hole | `IfcOpeningElement` (OPENING) + `IfcRelVoidsElement` to the roof, one per hole | Relative to the roof | Extrusion of the hole polygon vertically through the whole roof (from below the lowest underside to above the highest point the steepest plane reaches) | — | `Qto_OpeningElementBaseQuantities` (Area, Depth). The roof body already has the holes cut; the openings state them for tools that list or fill openings. |
| Roof (pitched) | `IfcRoof` (SHED / GABLE / HIP) | At the storey floor | `IfcFacetedBrep`: top = lower envelope of one plane per sloped edge, bottom = the same planes lowered by thickness / cos(pitch), vertical sides and gables. Concave outlines are exported flat, with a warning | `IfcMaterialLayerSet` | as above (GrossArea = sloped top area, ProjectedArea) |
| Railing | `IfcRailing` (GUARDRAIL) | At the base | Per path segment: a 60 × 40 mm top rail and a 20 mm infill panel; on a sloped segment both are profiles in the segment's vertical plane extruded across it, following the base | `IfcMaterial`, or with an infill material an `IfcMaterialConstituentSet` (constituents `Frame` and `Infill`); the panels are styled with the infill material | `Pset_RailingCommon` (Height); `Qto_RailingBaseQuantities` (Length along the rail) |
| Object | `IfcFurniture` (`furniture.*`, `kitchen.*`), `IfcSanitaryTerminal` (`sanitary.*`, `bathroom.*`; WC, basin, shower, bath, bidet, sink recognised from the name), else `IfcBuildingElementProxy` | At the insertion point, rotated | Placeholder box from the `width`, `depth`, `height` parameters (defaults 0.6 × 0.6 × 0.75 m) | — | ObjectType = catalogue identifier; `Kanavos_Object` (Definition, Mirrored, parameters) |
| Dimension, text, level, drafting, grid, underlay | not exported (counted in the report) | | | | |

Bodies carry `IfcStyledItem`s with an `IfcSurfaceStyle` coloured like the element's (core) material; doors,
windows and spaces have fixed styles.

**Geometry callback.** The caller may pass a function returning a triangle mesh (project coordinates) per
element. When it returns a mesh, that mesh replaces the exporter's own body, written as an `IfcFacetedBrep` in the
element's local frame. This is how detailed geometry from the modelling core reaches the file.

## 2. Import

### 2.1 What is read

| Item | Handling |
| --- | --- |
| Schemas | `IFC2X3`, `IFC4` (also `IFC4X1`, `IFC4X2`), `IFC4X3*`. Others are refused. |
| Syntax | Full ISO 10303-21 clear text: comments, typed parameters, lists, enumerations, `$`, `*`, binary values, complex instances (kept, not mapped), strings with `''`, `\\`, `\S\`, `\P?\` (ISO 8859-1 and 8859-7), `\X\`, `\X2\`, `\X4\`; UTF-8 bytes in strings are accepted. |
| Units | Length and plane-angle units from `IfcProject.UnitsInContext`: `IfcSIUnit` with prefixes, `IfcConversionBasedUnit` (inch, foot, degree …) through their conversion factors. Everything is converted to metres. |
| Placements | `IfcLocalPlacement` chains of `IfcAxis2Placement3D`/`2D`. |
| Geometry | `IfcExtrudedAreaSolid` (also tapered, read as plain), the first operand of `IfcBooleanResult`/`IfcBooleanClippingResult`, `IfcMappedItem` (with `IfcCartesianTransformationOperator3D`, also non-uniform). Profiles: rectangle (also hollow and rounded, read as plain), circle (also hollow), ellipse, arbitrary closed, arbitrary with voids, derived. Curves: `IfcPolyline`, `IfcIndexedPolyCurve` (line and arc segments), `IfcCompositeCurve`, `IfcTrimmedCurve` of lines and circles, `IfcCircle`, `IfcEllipse`. Other bodies (B-rep, tessellation) are not read. |

### 2.2 Mapping

The import produces a new document. By default it starts from the starter template **without its storeys**, so
materials, build-ups, defaults, the 3D view, site plan, coverage diagram, area table and schedules are ready;
`ImportBase::Empty` starts from an empty document instead.

| IFC | Kanavos | Rules |
| --- | --- | --- |
| `IfcBuildingStorey` | Storey + plan view | Elevation = world Z of its placement (else `Elevation`). Height = `GrossHeight` quantity, else the distance to the next storey, else the top of the tallest element on it, else 3 m. Below ground from `AboveGround`, else when the whole storey is below ±0.00. A file without storeys gets one at ±0.00. |
| `IfcWall`, `IfcWallStandardCase`, `IfcWallElementedCase` | Wall | Needs a vertical extrusion. Reference line from the `Axis` (straight polyline, or a trimmed circle → curved wall with bulge); without an axis, the centre line of the body. Thickness = extent of the body across the reference line; the reference line is the left face, right face or centre depending on where the axis lies (otherwise moved to the centre). Height = vertical extent; base offset from the storey elevation. |
| `IfcSlab` (all predefined types) | Slab | Needs a vertical extrusion. Outline (with holes) from the largest profile; top = highest point; thickness = vertical extent. Roof slabs are imported as slabs. |
| `IfcColumn` | Column | Vertical extrusion of a rectangle or circle: position = profile centre, rotation = profile X axis, height, base offset. |
| `IfcBeam` | Beam | Horizontal extrusion of a rectangle or circle: start/end = profile centre at both ends; the profile axis closer to vertical is the depth; top offset = top of the section. |
| `IfcSpace` | Space | Boundary from the extruded profile; height = extrusion height; name = LongName, number = Name (name = Name when LongName is empty); category and finish from `Kanavos_Space`. |
| `IfcOpeningElement` in a wall (`IfcRelVoidsElement`) with `IfcDoor` / `IfcWindow` (`IfcRelFillsElement`), or without filling | Opening (door, window, void) | Position = centre of the opening body along the wall's reference line (arc length for curved walls); width and height = OverallWidth/OverallHeight of the filling when given, else the body extents; sill = bottom of the body above the storey. Style and parameters from `Kanavos_Opening`, else the defaults in `ImportOptions`. |
| `IfcOpeningElement` in a slab | Hole in the slab outline | When its vertical body lies inside the slab outline. |
| `IfcRoof` with its own extruded body | Flat roof | Outline, holes and thickness from the lowest solid (the roof slab); a band with a hole on top becomes the parapet. Roofs made of parts are imported through the parts; pitched roof bodies are not imported yet. |
| `IfcOpeningElement` in a roof | Hole in the roof | When its vertical body lies inside the roof outline; an opening matching a hole already in the roof body is recorded as mapped without adding it twice. |
| `IfcFurniture`, `IfcFurnishingElement`, `IfcSanitaryTerminal`, `IfcFlowTerminal`, `IfcBuildingElementProxy` with a `Kanavos_Object` set | Object | Position and rotation from the placement. Others have no Kanavos equivalent yet. |
| `IfcRelAssociatesMaterial` (on the element or its type) | Structure | A layer set (usage or direct) with layer thicknesses summing to the geometric thickness (within 2 mm or 2 %) → build-up (an existing one with the same name and layers is reused); layers stacked in the positive direction are reversed into Kanavos order (walls left → right, slabs top → bottom). Otherwise one material with the geometric thickness. |
| `IfcMaterial` | Material | Reused by name, else created; family guessed from the name and IFC category (English and Greek keywords); colour from its styled representation, else a default for the family. |
| `IfcMapConversion` + `IfcProjectedCRS` | Georeference | EPSG code from the CRS name, eastings, northings, altitude (in the CRS map unit), rotation from the X axis direction. |
| `IfcProject` Name, LongName; building address | Project title, location, municipality | The imported document has no project id; it gets one when first saved. |
| `GlobalId`, `Tag`, `Pset_*Common.Status`, `Kanavos_Properties` | External id, tag, phase, user properties | The external id is shown, read-only, in the inspector. |

### 2.3 Report

Every import returns a report with: the schema; the number of instances of each entity type; the number mapped
per type; every product not mapped, with its id, type, `GlobalId` and reason (for example "no Kanavos equivalent
yet", "the body is not a vertical extrusion", "the host is not an imported wall"), also counted per type and
reason; the simplifications made (clipping ignored, axis moved to the centre, roof slab imported as a slab …);
and file-level remarks (units converted, storeys created).

## 3. Known gaps

- Export: walls are joined only at simple L-corners (mitre); T-junctions overlap; no `IfcRelSpaceBoundary`;
  objects are placeholder boxes unless the geometry callback supplies meshes; railings are simplified.
- Import: pitched roofs, stairs, railings, curtain walls, B-rep and tessellated bodies, polygonal (non-band) walls
  beyond their bounding band, walls with non-circular curved axes, and element types are not mapped; they are
  listed in the report.
