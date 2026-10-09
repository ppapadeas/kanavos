# DXF import and export

`kanavos-dxf` reads survey drawings and other CAD drawings as underlays, and writes Kanavos drawings for exchange.
Behaviour is specified in [`specs/drawing/dxf-layers.md`](../../specs/drawing/dxf-layers.md) (export) and
[`specs/drawing/survey-import.md`](../../specs/drawing/survey-import.md) (import).

```rust
pub fn write_dxf(drawing: &Drawing, opts: &DxfExportOptions) -> Vec<u8>
pub fn read_dxf(data: &[u8], opts: &ImportOptions) -> Result<DxfImport, DxfError>
```

## Export

ASCII DXF, version AC1015 (R2000): the oldest version with lightweight polylines, hatch entities and lineweights,
read by practically every CAD program. The file contains:

| Section | Contents |
| --- | --- |
| HEADER | `$ACADVER`, `$DWGCODEPAGE`, `$INSBASE`, `$EXTMIN`/`$EXTMAX`, `$LIMMIN`/`$LIMMAX`, `$LTSCALE` 1, `$INSUNITS` (6 m or 4 mm), `$MEASUREMENT` 1, `$LUNITS`, `$LWDISPLAY`, `$HANDSEED` |
| CLASSES | empty |
| TABLES | VPORT (`*Active`, framed on the drawing), LTYPE (ByBlock, ByLayer, Continuous, `KV_*`), LAYER (one per item layer and `<layer>.hatch`), STYLE (`Standard`, `Bold`), VIEW, UCS, APPID (`ACAD`), DIMSTYLE (`Standard`), BLOCK_RECORD (model and paper space) |
| BLOCKS | `*Model_Space`, `*Paper_Space` |
| ENTITIES | LINE, LWPOLYLINE, CIRCLE, ARC, TEXT, HATCH (SOLID) |
| OBJECTS | root dictionary, group dictionary, layout dictionary, `Model` and `Layout1` layouts |

Every object has a unique handle and its owner handle. Lines end in CR LF.

Options: output units (metres or millimetres, coordinates scaled), text code page (ASCII with `\U+` escapes by
default, or Windows-1253), font files named by the text styles, hatch expansion limits. Text positions use the
built-in font metrics, so a drawing always exports to the same bytes.

Hatches: backgrounds and solid fills are SOLID hatches; pattern lines are LINE entities on `<layer>.hatch`.

Verified by reading the files back with Kanavos's own reader and with an independent open-source DXF library in
strict mode, whose audit reported no errors and no repairs.

## Import

ASCII DXF from R12 (AC1009) to current versions. Binary DXF is refused.

| Entity | Result |
| --- | --- |
| LINE | line |
| LWPOLYLINE, POLYLINE (2D/3D) | polyline, bulges flattened to arcs; closed ones listed as plot boundary candidates |
| POLYLINE polyface / mesh | face outlines / grid lines |
| CIRCLE, ARC | circle, arc (polylines under non-uniform block scale) |
| ELLIPSE | polyline |
| SPLINE | polyline: NURBS evaluated from control points, knots and weights; else fit points |
| TEXT, ATTRIB | text with anchor, rotation, height; `%%` codes decoded |
| MTEXT | one text per paragraph, formatting removed, attachment point kept |
| POINT | survey point (Z kept) and a small circle |
| INSERT, MINSERT | block expanded: scale, rotation, arrays, nesting, layer 0 and BYBLOCK inheritance; elevation attributes become survey points |
| DIMENSION | its graphics block |
| SOLID, TRACE, 3DFACE | filled outline / outline |
| HATCH | boundary paths (polyline and edge paths); SOLID hatches filled |
| LEADER | polyline |

Other entities (images, viewports, 3D solids, infinite lines) are counted and reported as warnings.

Text encoding: UTF-8 from AC1021; before that the `$DWGCODEPAGE` code page (Windows-1253, Windows-1252,
ISO 8859-7, DOS 737), falling back to Windows-1253; `\U+XXXX` escapes everywhere.

The result, `DxfImport`, holds the drawing in metres (at a chosen scale, default 1:200, which sets text heights
and line type lengths in paper millimetres), the layer list with colours and visibility, survey points with
elevations, closed polylines with areas, extents, warnings and entity counts. Helpers move it into project
coordinates with the georeference (`to_project`, `placement_from_georef`), or by a chosen origin (`translated`,
`suggested_origin`), and produce the model `Underlay`, the plot `Polygon2` and terrain points.

## Sources

DXF reference (published file format documentation); Unicode Consortium code page mapping tables; EPSG:2100.
