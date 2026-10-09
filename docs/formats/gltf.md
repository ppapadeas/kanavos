# glTF export

Kanavos exports the 3D model as a single binary glTF 2.0 file (`.glb`) for visualisation, rendering and
presentation tools. Implementation: `crates/gltf`. Reference: Khronos glTF 2.0 specification, §4.4 (GLB).

## What is written

| Kanavos | glTF |
| --- | --- |
| Element (wall, slab, roof, door …) | One node, named `<category>-<id>`, with `extras.kanavos_id` and `extras.category` |
| Element parts of one colour | One primitive (triangles, flat normals) |
| Material colour | Material with `baseColorFactor` (sRGB converted to linear), roughness 0.85, not metallic, double-sided |
| Glass and other transparent colours | `alphaMode: BLEND` |

## Coordinates

- glTF is Y-up; Kanavos is Z-up. A model point `(x, y, z)` is written as `(x, z, −y)`: plan north (+Y) becomes
  glTF −Z.
- Positions are relative to the centre of the model's plan bounding box at elevation ±0.00, to keep 32-bit
  floats precise for georeferenced projects. The origin, in project metres, is stored in `scenes[0].extras.origin`.

## Not written (yet)

Textures, cameras, lights, storey hierarchy, annotation and spaces.
