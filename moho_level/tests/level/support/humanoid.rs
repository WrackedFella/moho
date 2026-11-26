//! A hand-built humanoid written as glTF from code: the sixteen mannequin bones in a
//! tree, one extra node (`ik_hand_gun`) under `hand_r`, a skinned triangle over all
//! sixteen bones with identity inverse binds, and the clips `walk` and `crouch` (plus
//! `reload` when the spec asks for it).
//!
//! Made for this repository from the numbers below; there is no third-party source and
//! no licence to record. The glTF node array is written in reverse walk order, so a node's
//! file index never equals its walk index.
//!
//! Walk order: `pelvis`, `spine_01`, `neck_01`, `head`, `upperarm_l`, `lowerarm_l`,
//! `hand_l`, `upperarm_r`, `lowerarm_r`, `hand_r`, `ik_hand_gun`, `thigh_l`, `calf_l`,
//! `foot_l`, `thigh_r`, `calf_r`, `foot_r`, then one node per duplicated name, then `body`.

use std::f32::consts::FRAC_PI_2;
use std::fs;
use std::path::PathBuf;

use glam::{Affine3A, Quat, Vec3};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Walk indices of the nodes `Humanoid::node` resolves, in `Bone::ALL` order.
pub const BONE_NODES: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16];

/// Index in `Model::meshes` of the rigid triangle on `hand_r` (the nodes are walked
/// before `body`, so it comes first).
pub const HAND_MESH: usize = 0;

/// Index in `Model::meshes` of the skinned triangle on `body`.
pub const BODY_MESH: usize = 1;

/// Walk index of the extra node under `hand_r`.
pub const IK_HAND_GUN: usize = 10;

/// Walk index of the first duplicated-name node, when the spec asks for any.
pub const FIRST_DUPLICATE: usize = 17;

/// Where a duplicated-name node sits relative to the pelvis, away from the real bone.
pub const DUPLICATE_OFFSET: Vec3 = Vec3::new(9.0, 0.0, 0.0);

/// `walk` loops over this many seconds; `hand_r` swings out and back.
pub const WALK_DURATION: f32 = 2.0;

/// `crouch` plays once over this many seconds.
pub const CROUCH_DURATION: f32 = 0.5;

/// How far `crouch` lowers the pelvis, and so every bone above it.
pub const CROUCH_DROP: f32 = 0.4;

/// Rest height of the head in model space: pelvis 1.0, spine 0.2, neck 0.4, head 0.15.
pub const HEAD_REST_Y: f32 = 1.75;

/// `lowerarm_r`'s rest transform in model space; `hand_r` hangs off it.
pub fn hand_r_parent() -> Affine3A {
    Affine3A::from_translation(Vec3::new(0.5, 1.5, 0.0))
}

/// `hand_r`'s local transform the `walk` clip keys at `time` seconds: it moves 0.4 m along
/// +Z and turns a quarter turn about +Y by t = 1, then returns by t = 2.
pub fn hand_r_local(time: f32) -> Affine3A {
    let out = 1.0 - (time - 1.0).abs();
    Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2 * out),
        Vec3::new(0.25, 0.0, 0.4 * out),
    )
}

/// Which names the written rig leaves out or repeats.
#[derive(Clone, Default)]
pub struct HumanoidSpec {
    /// Bones whose node keeps its place in the tree but loses its mannequin name.
    pub omit: Vec<&'static str>,
    /// Bones that get a second node of the same name, a child of the pelvis walked after
    /// the real bone and written before it in the file.
    pub duplicate: Vec<&'static str>,
    /// `(bone, name)`: the bone's node is written under `name` instead.
    pub rename: Vec<(&'static str, &'static str)>,
    /// Adds the `reload` clip after `walk` and `crouch`.
    pub reload: bool,
    /// Adds the `idle` clip after `reload` (or after `crouch` without it).
    pub idle: bool,
    /// Extra spine bones `spine_02`, `spine_03`, ... chained under `spine_01`, each 0.1 m above
    /// its parent; `neck_01` and both `upperarm_*` hang off the last one. The walk indices of
    /// everything after `spine_01` shift up by this count, so the index constants above hold
    /// only with 0; look nodes up by name otherwise.
    pub extra_spine: usize,
}

/// How far each extra spine bone sits above its parent.
pub const EXTRA_SPINE_STEP: f32 = 0.1;

/// `reload` lasts this many seconds when the spec asks for it; `spine_01` rises and `hand_r`
/// moves up, and `pelvis`, `thigh_l` and `calf_r` move too (an upper-body layer must ignore those),
/// linearly from the first key to the last.
pub const RELOAD_DURATION: f32 = 2.0;

/// `idle` lasts this many seconds when the spec asks for it; `hand_l` and `thigh_l` drift.
pub const IDLE_DURATION: f32 = 1.0;

/// `(name, parent name, local translation)` in walk order.
const NODES: [(&str, Option<&str>, [f32; 3]); 17] = [
    ("pelvis", None, [0.0, 1.0, 0.0]),
    ("spine_01", Some("pelvis"), [0.0, 0.2, 0.0]),
    ("neck_01", Some("spine_01"), [0.0, 0.4, 0.0]),
    ("head", Some("neck_01"), [0.0, 0.15, 0.0]),
    ("upperarm_l", Some("spine_01"), [-0.2, 0.3, 0.0]),
    ("lowerarm_l", Some("upperarm_l"), [-0.3, 0.0, 0.0]),
    ("hand_l", Some("lowerarm_l"), [-0.25, 0.0, 0.0]),
    ("upperarm_r", Some("spine_01"), [0.2, 0.3, 0.0]),
    ("lowerarm_r", Some("upperarm_r"), [0.3, 0.0, 0.0]),
    ("hand_r", Some("lowerarm_r"), [0.25, 0.0, 0.0]),
    ("ik_hand_gun", Some("hand_r"), [0.0, 0.0, 0.1]),
    ("thigh_l", Some("pelvis"), [-0.1, -0.05, 0.0]),
    ("calf_l", Some("thigh_l"), [0.0, -0.45, 0.0]),
    ("foot_l", Some("calf_l"), [0.0, -0.45, 0.0]),
    ("thigh_r", Some("pelvis"), [0.1, -0.05, 0.0]),
    ("calf_r", Some("thigh_r"), [0.0, -0.45, 0.0]),
    ("foot_r", Some("calf_r"), [0.0, -0.45, 0.0]),
];

const HAND_L: usize = 6;
const HAND_R: usize = 9;
const THIGH_L: usize = 11;
const CALF_R: usize = 15;
const PELVIS: usize = 0;

/// One node in walk order.
struct Def {
    name: String,
    parent: Option<usize>,
    translation: [f32; 3],
    body: bool,
}

/// Index of the bone called `name` in `NODES`, which is also its index in the defs.
fn position_of(name: &str) -> usize {
    NODES
        .iter()
        .position(|n| n.0 == name)
        .expect("the bone is in the table")
}

fn defs(spec: &HumanoidSpec) -> Vec<Def> {
    let mut defs: Vec<Def> = NODES
        .iter()
        .map(|&(name, parent, translation)| Def {
            name: if let Some((_, renamed)) = spec.rename.iter().find(|(bone, _)| *bone == name) {
                (*renamed).to_owned()
            } else if spec.omit.contains(&name) {
                format!("unnamed_{name}")
            } else {
                name.to_owned()
            },
            parent: parent.map(|p| {
                NODES
                    .iter()
                    .position(|n| n.0 == p)
                    .expect("parent is in the table")
            }),
            translation,
            body: false,
        })
        .collect();
    if spec.extra_spine > 0 {
        let first = defs.len();
        let spine_01 = position_of("spine_01");
        for k in 0..spec.extra_spine {
            defs.push(Def {
                name: format!("spine_{:02}", k + 2),
                parent: Some(if k == 0 { spine_01 } else { first + k - 1 }),
                translation: [0.0, EXTRA_SPINE_STEP, 0.0],
                body: false,
            });
        }
        let last = first + spec.extra_spine - 1;
        for name in ["neck_01", "upperarm_l", "upperarm_r"] {
            defs[position_of(name)].parent = Some(last);
        }
    }
    for name in &spec.duplicate {
        defs.push(Def {
            name: (*name).to_owned(),
            parent: Some(PELVIS),
            translation: DUPLICATE_OFFSET.to_array(),
            body: false,
        });
    }
    defs.push(Def {
        name: "body".to_owned(),
        parent: None,
        translation: [0.0; 3],
        body: true,
    });
    defs
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

    /// A `LINEAR` sampler's input and output accessors.
    fn keys(&mut self, times: &[f32], values: &[f32], kind: &str, width: usize) -> (usize, usize) {
        let bytes: Vec<u8> = times.iter().flat_map(|f| f.to_le_bytes()).collect();
        let bounds = (vec![times[0]], vec![times[times.len() - 1]]);
        let input = self.accessor(&bytes, 5126, times.len(), "SCALAR", Some(bounds));
        let output = self.floats(values, values.len() / width, kind);
        (input, output)
    }
}

const TRIANGLE: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [0.5, 0.0, 0.0], [0.0, 0.5, 0.0]];

fn build(spec: &HumanoidSpec) -> (Value, Vec<u8>) {
    let defs = defs(spec);
    let count = defs.len();
    // Nodes are written in reverse walk order.
    let file = |walk: usize| count - 1 - walk;

    let mut blob = Blob {
        bytes: Vec::new(),
        views: Vec::new(),
        accessors: Vec::new(),
    };
    let positions: Vec<f32> = TRIANGLE.iter().flatten().copied().collect();
    let position_bytes: Vec<u8> = positions.iter().flat_map(|f| f.to_le_bytes()).collect();
    let positions = blob.accessor(
        &position_bytes,
        5126,
        3,
        "VEC3",
        Some((vec![0.0; 3], vec![0.5, 0.5, 0.0])),
    );
    let normals = blob.floats(&[0.0, 0.0, 1.0].repeat(3), 3, "VEC3");
    // Vertices ride the pelvis, the head and the right hand.
    let joint_bytes: Vec<u8> = [[0u16, 0, 0, 0], [3, 0, 0, 0], [9, 0, 0, 0]]
        .iter()
        .flatten()
        .flat_map(|j| j.to_le_bytes())
        .collect();
    let joints = blob.accessor(&joint_bytes, 5123, 3, "VEC4", None);
    let weights = blob.floats(&[1.0, 0.0, 0.0, 0.0].repeat(3), 3, "VEC4");
    let identity: Vec<f32> = (0..16)
        .flat_map(|_| glam::Mat4::IDENTITY.to_cols_array())
        .collect();
    let inverse_binds = blob.floats(&identity, 16, "MAT4");
    let skin_joints: Vec<usize> = BONE_NODES.iter().map(|&n| file(n)).collect();

    let walk = {
        let (hand_times, hand_values) = ([0.0, 1.0, 2.0], {
            let at = |t: f32| hand_r_local(t).translation;
            [0.0_f32, 1.0, 2.0]
                .iter()
                .flat_map(|&t| at(t).to_array())
                .collect::<Vec<f32>>()
        });
        let turns: Vec<f32> = [0.0_f32, 1.0, 2.0]
            .iter()
            .flat_map(|&t| Quat::from_mat3a(&hand_r_local(t).matrix3).to_array())
            .collect();
        let (t_in, t_out) = blob.keys(&hand_times, &hand_values, "VEC3", 3);
        let (r_in, r_out) = blob.keys(&hand_times, &turns, "VEC4", 4);
        json!({
            "name": "walk",
            "samplers": [
                { "input": t_in, "output": t_out, "interpolation": "LINEAR" },
                { "input": r_in, "output": r_out, "interpolation": "LINEAR" },
            ],
            "channels": [
                { "sampler": 0, "target": { "node": file(HAND_R), "path": "translation" } },
                { "sampler": 1, "target": { "node": file(HAND_R), "path": "rotation" } },
            ],
        })
    };
    let crouch = {
        let drop = [0.0, 1.0, 0.0, 0.0, 1.0 - CROUCH_DROP, 0.0];
        let (input, output) = blob.keys(&[0.0, CROUCH_DURATION], &drop, "VEC3", 3);
        json!({
            "name": "crouch",
            "samplers": [{ "input": input, "output": output, "interpolation": "LINEAR" }],
            "channels": [
                { "sampler": 0, "target": { "node": file(PELVIS), "path": "translation" } },
            ],
        })
    };

    let mut animations = vec![walk, crouch];
    if spec.reload {
        let times = [0.0, RELOAD_DURATION];
        let spine = [0.0, 0.2, 0.0, 0.0, 0.5, 0.0];
        let hand = [0.25, 0.0, 0.0, 0.25, 0.3, 0.0];
        let (spine_in, spine_out) = blob.keys(&times, &spine, "VEC3", 3);
        let (hand_in, hand_out) = blob.keys(&times, &hand, "VEC3", 3);
        let thigh = [-0.1, -0.05, 0.0, -0.1, -0.05, 0.3];
        let (thigh_in, thigh_out) = blob.keys(&times, &thigh, "VEC3", 3);
        let calf = [0.0, -0.45, 0.0, 0.2, -0.45, 0.0];
        let (calf_in, calf_out) = blob.keys(&times, &calf, "VEC3", 3);
        let pelvis = [0.0, 1.0, 0.0, 0.0, 1.3, 0.0];
        let (pelvis_in, pelvis_out) = blob.keys(&times, &pelvis, "VEC3", 3);
        animations.push(json!({
            "name": "reload",
            "samplers": [
                { "input": spine_in, "output": spine_out, "interpolation": "LINEAR" },
                { "input": hand_in, "output": hand_out, "interpolation": "LINEAR" },
                { "input": thigh_in, "output": thigh_out, "interpolation": "LINEAR" },
                { "input": calf_in, "output": calf_out, "interpolation": "LINEAR" },
                { "input": pelvis_in, "output": pelvis_out, "interpolation": "LINEAR" },
            ],
            "channels": [
                { "sampler": 0, "target": { "node": file(1), "path": "translation" } },
                { "sampler": 1, "target": { "node": file(HAND_R), "path": "translation" } },
                { "sampler": 2, "target": { "node": file(THIGH_L), "path": "translation" } },
                { "sampler": 3, "target": { "node": file(CALF_R), "path": "translation" } },
                { "sampler": 4, "target": { "node": file(PELVIS), "path": "translation" } },
            ],
        }));
    }
    if spec.idle {
        let times = [0.0, IDLE_DURATION];
        let hand = [-0.25, 0.0, 0.0, -0.25, 0.05, 0.0];
        let (hand_in, hand_out) = blob.keys(&times, &hand, "VEC3", 3);
        let thigh = [-0.1, -0.05, 0.0, -0.1, -0.05, -0.1];
        let (thigh_in, thigh_out) = blob.keys(&times, &thigh, "VEC3", 3);
        animations.push(json!({
            "name": "idle",
            "samplers": [
                { "input": hand_in, "output": hand_out, "interpolation": "LINEAR" },
                { "input": thigh_in, "output": thigh_out, "interpolation": "LINEAR" },
            ],
            "channels": [
                { "sampler": 0, "target": { "node": file(HAND_L), "path": "translation" } },
                { "sampler": 1, "target": { "node": file(THIGH_L), "path": "translation" } },
            ],
        }));
    }

    let mut nodes = Vec::new();
    for walk_index in (0..count).rev() {
        let def = &defs[walk_index];
        let mut node = json!({ "name": def.name, "translation": def.translation });
        let children: Vec<usize> = defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.parent == Some(walk_index))
            .map(|(i, _)| file(i))
            .collect();
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        if def.body {
            node["mesh"] = json!(0);
            node["skin"] = json!(0);
        }
        if walk_index == HAND_R {
            node["mesh"] = json!(1);
        }
        nodes.push(node);
    }
    let roots: Vec<usize> = defs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.parent.is_none())
        .map(|(i, _)| file(i))
        .collect();

    let doc = json!({
        "asset": { "version": "2.0" },
        "scene": 0,
        "scenes": [{ "nodes": roots }],
        "nodes": nodes,
        "meshes": [
            { "primitives": [{ "attributes": {
                "POSITION": positions, "NORMAL": normals,
                "JOINTS_0": joints, "WEIGHTS_0": weights,
            }}]},
            { "primitives": [{ "attributes": {
                "POSITION": positions, "NORMAL": normals,
            }}]},
        ],
        "skins": [{ "joints": skin_joints, "inverseBindMatrices": inverse_binds }],
        "animations": animations,
        "accessors": blob.accessors,
        "bufferViews": blob.views,
        "buffers": [{ "byteLength": blob.bytes.len(), "uri": "humanoid.bin" }],
    });
    (doc, blob.bytes)
}

/// Write `humanoid.gltf` plus `humanoid.bin` into a fresh temp dir.
pub fn write_humanoid(spec: &HumanoidSpec) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (doc, bytes) = build(spec);
    let path = dir.path().join("humanoid.gltf");
    fs::write(&path, serde_json::to_vec_pretty(&doc).expect("json")).expect("write gltf");
    fs::write(dir.path().join("humanoid.bin"), bytes).expect("write bin");
    (dir, path)
}
