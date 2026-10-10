//! Writes small glTF/GLB levels from an in-code scene description.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use tempfile::TempDir;

const MODE_TRIANGLES: u32 = 4;
pub const MODE_LINES: u32 = 1;

/// One primitive: vertex channels plus how it is drawn.
#[derive(Clone)]
pub struct Prim {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Option<Vec<u32>>,
    pub mode: u32,
    /// Index into [`Scene::materials`].
    pub material: Option<usize>,
}

#[derive(Clone, Default)]
pub enum Xform {
    #[default]
    Identity,
    Trs {
        t: [f32; 3],
        /// Quaternion, x y z w.
        r: [f32; 4],
        s: [f32; 3],
    },
}

impl Xform {
    pub fn translate(t: [f32; 3]) -> Self {
        Self::Trs {
            t,
            r: [0.0, 0.0, 0.0, 1.0],
            s: [1.0; 3],
        }
    }

    pub fn scale(s: [f32; 3]) -> Self {
        Self::Trs {
            t: [0.0; 3],
            r: [0.0, 0.0, 0.0, 1.0],
            s,
        }
    }

    pub fn rotate(r: [f32; 4]) -> Self {
        Self::Trs {
            t: [0.0; 3],
            r,
            s: [1.0; 3],
        }
    }
}

#[derive(Clone, Default)]
pub struct Node {
    pub xform: Xform,
    pub mesh: Option<usize>,
    pub children: Vec<usize>,
}

#[derive(Clone)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub roots: Vec<usize>,
    /// Each mesh is a list of primitives.
    pub meshes: Vec<Vec<Prim>>,
    pub materials: Vec<Option<String>>,
    /// Put positions and normals of a primitive in one strided buffer view.
    pub interleave: bool,
    /// The `uri` written into the `.gltf` buffer entry.
    pub bin_uri: String,
    /// File name the buffer is saved under next to the `.gltf`; `None` writes no file.
    pub bin_file: Option<String>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            roots: Vec::new(),
            meshes: Vec::new(),
            materials: Vec::new(),
            interleave: false,
            bin_uri: "level.bin".into(),
            bin_file: Some("level.bin".into()),
        }
    }

    /// One root node per mesh, untransformed.
    pub fn with_meshes(meshes: Vec<Vec<Prim>>) -> Self {
        let mut scene = Self::new();
        for mesh in 0..meshes.len() {
            scene.roots.push(scene.nodes.len());
            scene.nodes.push(Node {
                mesh: Some(mesh),
                ..Node::default()
            });
        }
        scene.meshes = meshes;
        scene
    }
}

/// Axis-aligned quad of side 20 at height `y`, facing up.
pub fn floor_prim(y: f32) -> Prim {
    Prim {
        positions: vec![
            [-10.0, y, -10.0],
            [-10.0, y, 10.0],
            [10.0, y, 10.0],
            [10.0, y, -10.0],
        ],
        normals: vec![[0.0, 1.0, 0.0]; 4],
        indices: Some(vec![0, 1, 2, 0, 2, 3]),
        mode: MODE_TRIANGLES,
        material: None,
    }
}

/// Unit cube centred on the origin, 24 vertices, faces wound counter-clockwise from outside.
pub fn cube_prim() -> Prim {
    let faces = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for (n, u, v) in faces {
        let base = positions.len() as u32;
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            positions.push(std::array::from_fn(|i| {
                0.5 * (n[i] + su * u[i] + sv * v[i])
            }));
            normals.push(n);
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
    Prim {
        positions,
        normals,
        indices: Some(indices),
        mode: MODE_TRIANGLES,
        material: None,
    }
}

/// A two-vertex line primitive.
pub fn line_prim() -> Prim {
    Prim {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        normals: vec![[0.0, 1.0, 0.0]; 2],
        indices: None,
        mode: MODE_LINES,
        material: None,
    }
}

fn pad4(bytes: &mut Vec<u8>, fill: u8) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(fill);
    }
}

fn push_f32s(blob: &mut Vec<u8>, values: &[[f32; 3]]) {
    for v in values {
        for c in v {
            blob.extend_from_slice(&c.to_le_bytes());
        }
    }
}

fn bounds(positions: &[[f32; 3]]) -> (Vec<f32>, Vec<f32>) {
    let mut min = vec![f32::MAX; 3];
    let mut max = vec![f32::MIN; 3];
    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    (min, max)
}

/// Builds the glTF JSON (without a buffer uri) and the binary blob.
fn build(scene: &Scene) -> (Value, Vec<u8>) {
    let mut blob = Vec::new();
    let mut views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();

    let mut add_view = |blob: &mut Vec<u8>, start: usize, stride: Option<usize>| -> usize {
        let mut view = json!({
            "buffer": 0,
            "byteOffset": start,
            "byteLength": blob.len() - start,
        });
        if let Some(stride) = stride {
            view["byteStride"] = json!(stride);
        }
        views.push(view);
        views.len() - 1
    };

    for mesh in &scene.meshes {
        let mut prims = Vec::new();
        for prim in mesh {
            let count = prim.positions.len();
            let (min, max) = bounds(&prim.positions);
            let (pos_view, pos_offset, nrm_view, nrm_offset);
            if scene.interleave {
                let start = blob.len();
                for (p, n) in prim.positions.iter().zip(&prim.normals) {
                    push_f32s(&mut blob, &[*p, *n]);
                }
                let view = add_view(&mut blob, start, Some(24));
                (pos_view, pos_offset, nrm_view, nrm_offset) = (view, 0, view, 12);
            } else {
                let start = blob.len();
                push_f32s(&mut blob, &prim.positions);
                pos_view = add_view(&mut blob, start, None);
                let start = blob.len();
                push_f32s(&mut blob, &prim.normals);
                nrm_view = add_view(&mut blob, start, None);
                (pos_offset, nrm_offset) = (0, 0);
            }
            accessors.push(json!({
                "bufferView": pos_view, "byteOffset": pos_offset, "componentType": 5126,
                "count": count, "type": "VEC3", "min": min, "max": max,
            }));
            let pos_acc = accessors.len() - 1;
            accessors.push(json!({
                "bufferView": nrm_view, "byteOffset": nrm_offset, "componentType": 5126,
                "count": count, "type": "VEC3",
            }));
            let nrm_acc = accessors.len() - 1;
            let mut p = json!({
                "attributes": { "POSITION": pos_acc, "NORMAL": nrm_acc },
                "mode": prim.mode,
            });
            if let Some(indices) = &prim.indices {
                let start = blob.len();
                for i in indices {
                    blob.extend_from_slice(&i.to_le_bytes());
                }
                let view = add_view(&mut blob, start, None);
                accessors.push(json!({
                    "bufferView": view, "componentType": 5125,
                    "count": indices.len(), "type": "SCALAR",
                }));
                p["indices"] = json!(accessors.len() - 1);
            }
            if let Some(material) = prim.material {
                p["material"] = json!(material);
            }
            prims.push(p);
        }
        meshes.push(json!({ "primitives": prims }));
    }
    pad4(&mut blob, 0);

    let nodes: Vec<Value> = scene
        .nodes
        .iter()
        .map(|node| {
            let mut n = json!({});
            if let Xform::Trs { t, r, s } = &node.xform {
                n["translation"] = json!(t);
                n["rotation"] = json!(r);
                n["scale"] = json!(s);
            }
            if let Some(mesh) = node.mesh {
                n["mesh"] = json!(mesh);
            }
            if !node.children.is_empty() {
                n["children"] = json!(node.children);
            }
            n
        })
        .collect();
    let materials: Vec<Value> = scene
        .materials
        .iter()
        .map(|name| {
            name.as_ref()
                .map_or_else(|| json!({}), |n| json!({ "name": n }))
        })
        .collect();

    let mut doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": scene.roots }],
        "nodes": nodes,
        "meshes": meshes,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{ "byteLength": blob.len() }],
    });
    if !materials.is_empty() {
        doc["materials"] = json!(materials);
    }
    (doc, blob)
}

/// Write `level.gltf` plus its `.bin` into a fresh temp dir.
pub fn write_gltf(scene: &Scene) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut doc, blob) = build(scene);
    doc["buffers"][0]["uri"] = json!(scene.bin_uri);
    let path = dir.path().join("level.gltf");
    fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json")).expect("write gltf");
    if let Some(file) = &scene.bin_file {
        fs::write(dir.path().join(file), blob).expect("write bin");
    }
    (dir, path)
}

/// Write `level.glb` (JSON chunk plus embedded BIN chunk) into a fresh temp dir.
pub fn write_glb(scene: &Scene) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (doc, mut blob) = build(scene);
    let mut json_chunk = serde_json::to_vec(&doc).expect("json");
    pad4(&mut json_chunk, b' ');
    pad4(&mut blob, 0);

    let total = 12 + 8 + json_chunk.len() + 8 + blob.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_chunk);
    glb.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&blob);

    let path = dir.path().join("level.glb");
    fs::write(&path, glb).expect("write glb");
    (dir, path)
}
