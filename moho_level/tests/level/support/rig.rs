//! A two-bone rigged character written as glTF from code: a skinned triangle on two
//! bones, a rigid triangle on an animated node, and `idle`, `walk` and `death` clips.
//!
//! Made for this repository from the numbers below; there is no third-party source and
//! no licence to record. The glTF node array is deliberately not in scene-walk order, so
//! joint and channel targets only resolve through the loader's walk-index map.
//!
//! Walk order: `root`, `bone_a`, `bone_b`, `body`, `magazine`, `prop`.

use std::f32::consts::FRAC_1_SQRT_2;
use std::fs;
use std::path::PathBuf;

use glam::{Mat4, Quat, Vec3};
use serde_json::{Value, json};
use tempfile::TempDir;

/// glTF node indices in the file.
pub const G_BODY: usize = 0;
pub const G_PROP: usize = 1;
pub const G_BONE_B: usize = 2;
pub const G_ROOT: usize = 3;
pub const G_MAGAZINE: usize = 4;
pub const G_BONE_A: usize = 5;
/// A node outside the default scene.
pub const G_ORPHAN: usize = 6;

/// Node indices in `Model::nodes()` (scene-walk order).
pub const ROOT: usize = 0;
pub const BONE_A: usize = 1;
pub const BONE_B: usize = 2;
pub const BODY: usize = 3;
pub const MAGAZINE: usize = 4;
pub const PROP: usize = 5;

/// Clip indices in `Model::clips()`.
pub const IDLE: usize = 0;
pub const WALK: usize = 1;
pub const DEATH: usize = 2;

pub const IDLE_DURATION: f32 = 2.0;
pub const WALK_DURATION: f32 = 4.0;
pub const DEATH_DURATION: f32 = 3.0;

/// `walk`: `bone_b` turns about Z through these angles at t = 0, 1, 2, 3, 4.
pub const WALK_ANGLES_DEG: [f32; 5] = [20.0, 45.0, 90.0, 45.0, 20.0];

/// `death`: `bone_a` turns about X from 0 to this angle over its duration.
pub const DEATH_END_DEG: f32 = 90.0;

/// One animation channel.
#[derive(Clone)]
pub struct Chan {
    /// glTF node index.
    pub node: usize,
    /// `translation`, `rotation`, `scale` or `weights`.
    pub path: &'static str,
    /// `STEP`, `LINEAR` or `CUBICSPLINE`.
    pub interpolation: &'static str,
    pub times: Vec<f32>,
    /// Flat components: 3 per translation or scale value, 4 per rotation, 1 per weight.
    pub values: Vec<f32>,
}

#[derive(Clone)]
pub struct Anim {
    pub name: &'static str,
    pub channels: Vec<Chan>,
}

/// Per-vertex joints and weights of a second influence set.
pub type FurtherInfluences = (Vec<[u16; 4]>, Vec<[f32; 4]>);

/// The rig as a description tests may edit before writing it.
#[derive(Clone)]
pub struct RigSpec {
    /// Per vertex of the skinned triangle.
    pub vertex_joints: Vec<[u16; 4]>,
    pub vertex_weights: Vec<[f32; 4]>,
    /// A `JOINTS_1`/`WEIGHTS_1` pair per vertex; `None` writes only the first set.
    pub further_influences: Option<FurtherInfluences>,
    /// glTF node indices.
    pub skin_joints: Vec<usize>,
    /// `None` writes a skin without `inverseBindMatrices`.
    pub inverse_binds: Option<Vec<Mat4>>,
    pub anims: Vec<Anim>,
    /// Write every rotation channel's output as normalized `i8` (`round(v * 127)`).
    pub quantise_rotations: bool,
}

impl RigSpec {
    pub fn new() -> Self {
        Self {
            vertex_joints: vec![[0, 0, 0, 0], [0, 1, 0, 0], [1, 0, 0, 0]],
            vertex_weights: vec![
                [1.0, 0.0, 0.0, 0.0],
                [0.25, 0.75, 0.0, 0.0],
                [1.0, 0.0, 0.0, 0.0],
            ],
            further_influences: None,
            skin_joints: vec![G_BONE_A, G_BONE_B],
            inverse_binds: Some(vec![
                Mat4::from_translation(Vec3::new(-1.0, -1.0, 0.0)),
                Mat4::from_translation(Vec3::new(-1.0, -3.0, 0.0)),
            ]),
            anims: vec![idle(), walk(), death()],
            quantise_rotations: false,
        }
    }
}

fn v(x: f32, y: f32, z: f32) -> [f32; 3] {
    [x, y, z]
}

fn flat3(values: &[[f32; 3]]) -> Vec<f32> {
    values.iter().flatten().copied().collect()
}

fn idle() -> Anim {
    Anim {
        name: "idle",
        channels: vec![Chan {
            node: G_BONE_A,
            path: "translation",
            interpolation: "STEP",
            times: vec![0.0, 1.0, 2.0],
            values: flat3(&[v(1.0, 0.0, 0.0), v(1.0, 0.5, 0.0), v(1.0, 1.0, 0.0)]),
        }],
    }
}

fn walk() -> Anim {
    let turns = WALK_ANGLES_DEG
        .iter()
        .flat_map(|deg| Quat::from_rotation_z(deg.to_radians()).to_array())
        .collect();
    Anim {
        name: "walk",
        channels: vec![
            Chan {
                node: G_BONE_A,
                path: "translation",
                interpolation: "LINEAR",
                times: vec![0.0, 4.0],
                values: flat3(&[v(1.0, 0.0, 0.0), v(1.0, 0.0, 4.0)]),
            },
            Chan {
                node: G_BONE_B,
                path: "rotation",
                interpolation: "LINEAR",
                times: vec![0.0, 1.0, 2.0, 3.0, 4.0],
                values: turns,
            },
            // In-tangent, value, out-tangent per key; zero tangents ease from 0 to 4 in y.
            Chan {
                node: G_MAGAZINE,
                path: "translation",
                interpolation: "CUBICSPLINE",
                times: vec![0.0, 4.0],
                values: flat3(&[
                    v(0.0, 0.0, 0.0),
                    v(0.0, 0.0, 5.0),
                    v(0.0, 0.0, 0.0),
                    v(0.0, 0.0, 0.0),
                    v(0.0, 4.0, 5.0),
                    v(0.0, 0.0, 0.0),
                ]),
            },
        ],
    }
}

fn death() -> Anim {
    let end = Quat::from_rotation_x(DEATH_END_DEG.to_radians());
    Anim {
        name: "death",
        channels: vec![Chan {
            node: G_BONE_A,
            path: "rotation",
            interpolation: "LINEAR",
            times: vec![0.0, 3.0],
            values: [Quat::IDENTITY.to_array(), end.to_array()]
                .into_iter()
                .flatten()
                .collect(),
        }],
    }
}

/// A channel of a morph-target weight animation on the body, ending at t = 9.
pub fn weights_chan() -> Chan {
    Chan {
        node: G_BODY,
        path: "weights",
        interpolation: "LINEAR",
        times: vec![0.0, 9.0],
        values: vec![0.0, 1.0],
    }
}

/// Accumulates the binary buffer with one view and one accessor per call.
struct Blob {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Blob {
    fn accessor(
        &mut self,
        data: &[u8],
        component_type: u32,
        count: usize,
        kind: &str,
        bounds: Option<(Vec<f32>, Vec<f32>)>,
    ) -> usize {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(data);
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        self.views.push(json!({
            "buffer": 0, "byteOffset": start, "byteLength": data.len(),
        }));
        let mut accessor = json!({
            "bufferView": self.views.len() - 1, "componentType": component_type,
            "count": count, "type": kind,
        });
        if let Some((min, max)) = bounds {
            accessor["min"] = json!(min);
            accessor["max"] = json!(max);
        }
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    fn floats(&mut self, values: &[f32], count: usize, kind: &str) -> usize {
        let data: Vec<u8> = values.iter().flat_map(|f| f.to_le_bytes()).collect();
        self.accessor(&data, 5126, count, kind, None)
    }
}

const TRIANGLE: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

fn build(spec: &RigSpec) -> (Value, Vec<u8>) {
    let mut blob = Blob {
        bytes: Vec::new(),
        views: Vec::new(),
        accessors: Vec::new(),
    };
    let positions = blob.accessor(
        &flat3(&TRIANGLE)
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect::<Vec<u8>>(),
        5126,
        3,
        "VEC3",
        Some((vec![0.0; 3], vec![1.0, 1.0, 0.0])),
    );
    let normals = blob.floats(&flat3(&[v(0.0, 0.0, 1.0); 3]), 3, "VEC3");
    let joints: Vec<u8> = spec
        .vertex_joints
        .iter()
        .flatten()
        .flat_map(|j| j.to_le_bytes())
        .collect();
    let joints = blob.accessor(&joints, 5123, spec.vertex_joints.len(), "VEC4", None);
    let weights: Vec<f32> = spec.vertex_weights.iter().flatten().copied().collect();
    let weights = blob.floats(&weights, spec.vertex_weights.len(), "VEC4");
    let further = spec.further_influences.as_ref().map(|(joints, weights)| {
        let joints_bytes: Vec<u8> = joints
            .iter()
            .flatten()
            .flat_map(|j| j.to_le_bytes())
            .collect();
        let joints_1 = blob.accessor(&joints_bytes, 5123, joints.len(), "VEC4", None);
        let weights_flat: Vec<f32> = weights.iter().flatten().copied().collect();
        let weights_1 = blob.floats(&weights_flat, weights.len(), "VEC4");
        (joints_1, weights_1)
    });
    let rigid_positions = blob.accessor(
        &flat3(&TRIANGLE)
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect::<Vec<u8>>(),
        5126,
        3,
        "VEC3",
        Some((vec![0.0; 3], vec![1.0, 1.0, 0.0])),
    );
    let rigid_normals = blob.floats(&flat3(&[v(0.0, 0.0, 1.0); 3]), 3, "VEC3");

    let mut skin = json!({ "joints": spec.skin_joints });
    if let Some(binds) = &spec.inverse_binds {
        let flat: Vec<f32> = binds.iter().flat_map(Mat4::to_cols_array).collect();
        skin["inverseBindMatrices"] = json!(blob.floats(&flat, binds.len(), "MAT4"));
    }

    let mut animations = Vec::new();
    for anim in &spec.anims {
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for chan in &anim.channels {
            let (components, kind) = match chan.path {
                "rotation" => (4, "VEC4"),
                "weights" => (1, "SCALAR"),
                _ => (3, "VEC3"),
            };
            let input = blob.accessor(
                &chan
                    .times
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<u8>>(),
                5126,
                chan.times.len(),
                "SCALAR",
                Some((
                    vec![chan.times.iter().copied().fold(f32::MAX, f32::min)],
                    vec![chan.times.iter().copied().fold(f32::MIN, f32::max)],
                )),
            );
            let output = if spec.quantise_rotations && chan.path == "rotation" {
                let bytes: Vec<u8> = chan
                    .values
                    .iter()
                    .flat_map(|f| ((f * 127.0).round() as i8).to_le_bytes())
                    .collect();
                let output = blob.accessor(&bytes, 5120, chan.values.len() / 4, "VEC4", None);
                blob.accessors[output]["normalized"] = json!(true);
                output
            } else {
                blob.floats(&chan.values, chan.values.len() / components, kind)
            };
            samplers.push(json!({
                "input": input, "output": output, "interpolation": chan.interpolation,
            }));
            channels.push(json!({
                "sampler": samplers.len() - 1,
                "target": { "node": chan.node, "path": chan.path },
            }));
        }
        animations.push(json!({
            "name": anim.name, "samplers": samplers, "channels": channels,
        }));
    }

    let mut skinned_attributes = json!({
        "POSITION": positions, "NORMAL": normals,
        "JOINTS_0": joints, "WEIGHTS_0": weights,
    });
    if let Some((joints_1, weights_1)) = further {
        skinned_attributes["JOINTS_1"] = json!(joints_1);
        skinned_attributes["WEIGHTS_1"] = json!(weights_1);
    }

    let s = FRAC_1_SQRT_2;
    let doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": [G_ROOT] }],
        "nodes": [
            { "name": "body", "mesh": 0, "skin": 0 },
            {
                "name": "prop", "translation": [7.0, 0.0, 0.0],
                "rotation": [0.0, s, 0.0, s], "scale": [1.0, 2.0, 3.0],
            },
            { "name": "bone_b", "translation": [0.0, 2.0, 0.0] },
            {
                "name": "root", "translation": [0.0, 1.0, 0.0],
                "children": [G_BONE_A, G_BODY, G_MAGAZINE, G_PROP],
            },
            { "name": "magazine", "translation": [0.0, 0.0, 5.0], "mesh": 1 },
            { "name": "bone_a", "translation": [1.0, 0.0, 0.0], "children": [G_BONE_B] },
            { "name": "orphan" },
        ],
        "meshes": [
            { "primitives": [{ "attributes": skinned_attributes }]},
            { "primitives": [{ "attributes": {
                "POSITION": rigid_positions, "NORMAL": rigid_normals,
            }}]},
        ],
        "skins": [skin],
        "animations": animations,
        "accessors": blob.accessors,
        "bufferViews": blob.views,
        "buffers": [{ "byteLength": blob.bytes.len(), "uri": "rig.bin" }],
    });
    (doc, blob.bytes)
}

/// Write `rig.gltf` plus `rig.bin` into a fresh temp dir.
pub fn write_rig(spec: &RigSpec) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (doc, bytes) = build(spec);
    let path = dir.path().join("rig.gltf");
    fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json")).expect("write gltf");
    fs::write(dir.path().join("rig.bin"), bytes).expect("write bin");
    (dir, path)
}
