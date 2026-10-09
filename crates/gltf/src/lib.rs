//! glTF 2.0 export of the 3D model as a single binary `.glb` file.
//!
//! Layer L6. See `docs/architecture.md` §6 ("glTF export for visualisation").
//!
//! Format: Khronos glTF 2.0 specification, section 4.4 (GLB file format). Conventions:
//! - glTF is Y-up; Kanavos is Z-up. A model point `(x, y, z)` is written as `(x, z, −y)`, so the plan's north
//!   (+Y) points along glTF −Z, the usual "forward" axis.
//! - Coordinates are relative to an origin at the centre of the model's plan bounding box (at elevation 0) to
//!   keep 32-bit floats precise even for georeferenced projects; the origin is stored in `scene.extras.origin`.
//! - Every element becomes one node named after its category and id, with `extras.kanavos_id` and
//!   `extras.category`, holding one mesh with one primitive per colour. Normals are flat (vertices are not shared
//!   between triangles), which suits architecture.

use std::collections::BTreeMap;

use kanavos_drawing::Rgba;
use kanavos_model::{Document, Id};
use kanavos_views::{Scene, SceneOptions, scene};
use serde_json::{Value, json};

/// Why an export failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GltfError {
    /// There is nothing to export.
    EmptyScene,
    /// The binary buffer would exceed the 4 GiB limit of GLB.
    TooLarge,
    /// JSON serialisation failed.
    Json(String),
}

impl std::fmt::Display for GltfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GltfError::EmptyScene => write!(f, "the model has no 3D elements to export"),
            GltfError::TooLarge => write!(f, "the model is too large for a single glTF binary file"),
            GltfError::Json(e) => write!(f, "could not write glTF JSON: {e}"),
        }
    }
}

impl std::error::Error for GltfError {}

/// Export options.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GltfOptions {
    /// Which elements to include.
    pub scene: SceneOptions,
}

struct Primitive {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    color: Rgba,
}

struct NodeData {
    name: String,
    element: Id,
    category: &'static str,
    primitives: Vec<Primitive>,
}

fn to_gltf(p: kanavos_geom::Point3, origin: (f64, f64)) -> [f32; 3] {
    [(p.x - origin.0) as f32, p.z as f32, -(p.y - origin.1) as f32]
}

fn collect(scene: &Scene) -> (Vec<NodeData>, (f64, f64)) {
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for item in &scene.items {
        for p in &item.mesh.positions {
            minx = minx.min(p.x);
            miny = miny.min(p.y);
            maxx = maxx.max(p.x);
            maxy = maxy.max(p.y);
        }
    }
    let origin = if minx <= maxx { ((minx + maxx) / 2.0, (miny + maxy) / 2.0) } else { (0.0, 0.0) };
    let mut nodes: BTreeMap<Id, NodeData> = BTreeMap::new();
    for item in &scene.items {
        let node = nodes.entry(item.element).or_insert_with(|| NodeData {
            name: format!("{}-{}", item.category.key(), item.element.0),
            element: item.element,
            category: item.category.key(),
            primitives: Vec::new(),
        });
        let idx = match node.primitives.iter().position(|p| p.color == item.color) {
            Some(i) => i,
            None => {
                node.primitives.push(Primitive {
                    positions: Vec::new(),
                    normals: Vec::new(),
                    color: item.color,
                });
                node.primitives.len() - 1
            }
        };
        let Some(prim) = node.primitives.get_mut(idx) else { continue };
        for [a, b, c] in item.mesh.triangle_points() {
            let (pa, pb, pc) = (to_gltf(a, origin), to_gltf(b, origin), to_gltf(c, origin));
            let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len.is_nan() || len <= 1e-12 {
                continue; // Degenerate triangle.
            }
            let n = [n[0] / len, n[1] / len, n[2] / len];
            prim.positions.extend([pa, pb, pc]);
            prim.normals.extend([n, n, n]);
        }
    }
    let nodes =
        nodes.into_values().filter(|n| n.primitives.iter().any(|p| !p.positions.is_empty())).collect();
    (nodes, origin)
}

fn push_f32s(buf: &mut Vec<u8>, data: &[[f32; 3]]) {
    for v in data {
        for c in v {
            buf.extend_from_slice(&c.to_le_bytes());
        }
    }
}

fn pad4(buf: &mut Vec<u8>, byte: u8) {
    while !buf.len().is_multiple_of(4) {
        buf.push(byte);
    }
}

fn srgb_to_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// Exports a prepared scene as GLB bytes.
pub fn scene_to_glb(scene: &Scene, title: &str) -> Result<Vec<u8>, GltfError> {
    let (nodes, origin) = collect(scene);
    if nodes.is_empty() {
        return Err(GltfError::EmptyScene);
    }
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views: Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut material_index: BTreeMap<(u8, u8, u8, u8), usize> = BTreeMap::new();
    let mut meshes: Vec<Value> = Vec::new();
    let mut gnodes: Vec<Value> = Vec::new();
    for node in &nodes {
        let mut prims = Vec::new();
        for p in &node.primitives {
            if p.positions.is_empty() {
                continue;
            }
            let key = (p.color.r, p.color.g, p.color.b, p.color.a);
            let mat = match material_index.get(&key) {
                Some(i) => *i,
                None => {
                    let alpha = f64::from(p.color.a) / 255.0;
                    let mut m = json!({
                        "name": p.color.to_hex(),
                        "pbrMetallicRoughness": {
                            "baseColorFactor": [srgb_to_linear(p.color.r), srgb_to_linear(p.color.g), srgb_to_linear(p.color.b), alpha],
                            "metallicFactor": 0.0,
                            "roughnessFactor": 0.85
                        },
                        "doubleSided": true
                    });
                    if p.color.a < 255
                        && let Some(obj) = m.as_object_mut()
                    {
                        obj.insert("alphaMode".into(), json!("BLEND"));
                    }
                    materials.push(m);
                    material_index.insert(key, materials.len() - 1);
                    materials.len() - 1
                }
            };
            let count = p.positions.len();
            let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &p.positions {
                for (i, c) in v.iter().enumerate() {
                    if let (Some(lo), Some(hi)) = (min.get_mut(i), max.get_mut(i)) {
                        *lo = lo.min(*c);
                        *hi = hi.max(*c);
                    }
                }
            }
            let pos_offset = bin.len();
            push_f32s(&mut bin, &p.positions);
            buffer_views.push(
                json!({"buffer": 0, "byteOffset": pos_offset, "byteLength": count * 12, "target": 34962}),
            );
            accessors.push(json!({
                "bufferView": buffer_views.len() - 1, "componentType": 5126, "count": count, "type": "VEC3",
                "min": min, "max": max
            }));
            let pos_acc = accessors.len() - 1;
            let nrm_offset = bin.len();
            push_f32s(&mut bin, &p.normals);
            buffer_views.push(
                json!({"buffer": 0, "byteOffset": nrm_offset, "byteLength": count * 12, "target": 34962}),
            );
            accessors.push(json!({"bufferView": buffer_views.len() - 1, "componentType": 5126, "count": count, "type": "VEC3"}));
            let nrm_acc = accessors.len() - 1;
            prims.push(
                json!({"attributes": {"POSITION": pos_acc, "NORMAL": nrm_acc}, "material": mat, "mode": 4}),
            );
        }
        meshes.push(json!({"name": node.name, "primitives": prims}));
        gnodes.push(json!({
            "name": node.name,
            "mesh": meshes.len() - 1,
            "extras": {"kanavos_id": node.element.0, "category": node.category}
        }));
    }
    if bin.len() > u32::MAX as usize - 1024 {
        return Err(GltfError::TooLarge);
    }
    pad4(&mut bin, 0);
    let root = json!({
        "asset": {"version": "2.0", "generator": format!("Kanavos {}", env!("CARGO_PKG_VERSION"))},
        "scene": 0,
        "scenes": [{"name": title, "nodes": (0..gnodes.len()).collect::<Vec<_>>(), "extras": {"origin": [origin.0, origin.1, 0.0]}}],
        "nodes": gnodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{"byteLength": bin.len()}]
    });
    let mut json_bytes = serde_json::to_vec(&root).map_err(|e| GltfError::Json(e.to_string()))?;
    pad4(&mut json_bytes, b' ');
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let total = u32::try_from(total).map_err(|_| GltfError::TooLarge)?;
    let json_len = u32::try_from(json_bytes.len()).map_err(|_| GltfError::TooLarge)?;
    let bin_len = u32::try_from(bin.len()).map_err(|_| GltfError::TooLarge)?;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes()); // "glTF"
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&json_len.to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&bin_len.to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&bin);
    Ok(out)
}

/// Exports the model as GLB bytes.
pub fn export_glb(doc: &Document, options: &GltfOptions) -> Result<Vec<u8>, GltfError> {
    let s = scene(doc, &options.scene);
    let title = if doc.info().name.is_empty() { "Kanavos" } else { doc.info().name.as_str() };
    scene_to_glb(&s, title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kanavos_geom::Point2;
    use kanavos_model::{Element, ElementKind, ReferenceLine, Structure, Wall, starter};
    use kanavos_units::Length;

    fn house() -> Document {
        let doc = starter();
        let st = doc.storeys()[0].id;
        let bu = doc.defaults().exterior_wall.unwrap();
        let corners = [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0), (0.0, 6.0)];
        let mut doc = doc;
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let (d, id) = doc.with_new_id();
            doc = d.with_element(Element::on_storey(
                id,
                st,
                ElementKind::Wall(Wall {
                    start: Point2::new(a.0, a.1),
                    end: Point2::new(b.0, b.1),
                    bulge: 0.0,
                    reference: ReferenceLine::Centre,
                    structure: Structure::Layered { build_up: bu },
                    height: Length::m(3.0),
                    base_offset: Length::ZERO,
                }),
            ));
        }
        doc
    }

    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }

    #[test]
    fn writes_a_valid_glb_header_and_json() {
        let glb = export_glb(&house(), &GltfOptions::default()).unwrap();
        assert_eq!(&glb[0..4], b"glTF");
        assert_eq!(u32_at(&glb, 4), 2);
        assert_eq!(u32_at(&glb, 8) as usize, glb.len());
        let json_len = u32_at(&glb, 12) as usize;
        assert_eq!(&glb[16..20], b"JSON");
        let json: Value = serde_json::from_slice(&glb[20..20 + json_len]).unwrap();
        assert_eq!(json["asset"]["version"], "2.0");
        assert_eq!(json["nodes"].as_array().unwrap().len(), 4);
        let bin_start = 20 + json_len;
        assert_eq!(&glb[bin_start + 4..bin_start + 8], b"BIN\0");
        assert_eq!(
            u32_at(&glb, bin_start) as usize,
            json["buffers"][0]["byteLength"].as_u64().unwrap() as usize
        );
        assert_eq!(glb.len() % 4, 0);
    }

    #[test]
    fn empty_model_is_an_error() {
        assert_eq!(export_glb(&starter(), &GltfOptions::default()), Err(GltfError::EmptyScene));
    }
}
