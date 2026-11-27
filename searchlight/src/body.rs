//! The character body the game uses, and a check that says whether a body is one it can use.

use std::fmt;
use std::path::{Path, PathBuf};

use glam::Affine3A;
use moho_core::assets::{AssetError, AssetRoots};
use moho_level::anim::{ClipError, ClipPlayer, Playback};
use moho_level::humanoid::{Bone, Humanoid, load_humanoid};
use moho_level::{LevelError, MeshSkin, Model, load_model};
use thiserror::Error;

use crate::TICK_HZ;

/// How far the standing head may sit from the standing eye, in metres.
const HEAD_TOLERANCE: f32 = 0.05;

/// The full body, as an asset name.
pub const BODY: &str = "characters/mannequin/mannequin.glb";

/// The headless first-person body, as an asset name.
pub const PLAYER_BODY: &str = "characters/mannequin/mannequin_fp.glb";

/// The head, as an asset name.
pub const HEAD: &str = "characters/mannequin/head.glb";

/// The clips every body carries.
pub const CLIPS: [&str; 10] = [
    "idle",
    "walk",
    "run",
    "crouch_idle",
    "crouch_walk",
    "prone_idle",
    "crawl",
    "rifle_idle",
    "rifle_walk",
    "death",
];

/// What the check found in a body the game can use.
#[derive(Debug)]
pub struct BodyReport {
    body: PathBuf,
    player_body: PathBuf,
    head: PathBuf,
    head_height: f32,
    eye_height: f32,
}

impl fmt::Display for BodyReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, source) in [(BODY, &self.body), (PLAYER_BODY, &self.player_body)] {
            writeln!(f, "{name}: {}", source.display())?;
            writeln!(
                f,
                "  {} bones, {} clips: {}",
                Bone::ALL.len(),
                CLIPS.len(),
                CLIPS.join(", ")
            )?;
        }
        writeln!(f, "{HEAD}: {}", self.head.display())?;
        write!(
            f,
            "standing head height {:.2} m against eye {:.2} m",
            self.head_height, self.eye_height
        )
    }
}

/// Why a body is one the game can't use; each variant names the asset file.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BodyError {
    /// No folder holds the file.
    #[error("{file}: {source}")]
    Resolve {
        /// The asset name.
        file: &'static str,
        /// Why it did not resolve.
        source: AssetError,
    },
    /// The loader refused the file, or it lacks a humanoid bone.
    #[error("{file}: {source}")]
    Load {
        /// The asset name.
        file: &'static str,
        /// What the loader said.
        source: LevelError,
    },
    /// A bone name resolves to a node that is not a joint of the rig.
    #[error("{file}: {bone} is not a joint")]
    NotJoint {
        /// The asset name.
        file: &'static str,
        /// The bone's node name.
        bone: &'static str,
    },
    /// A required clip is missing.
    #[error("{file}: {source}")]
    Clip {
        /// The asset name.
        file: &'static str,
        /// Which clip and why.
        source: ClipError,
    },
    /// A clip's duration or one of its poses is not finite.
    #[error("{file}: clip {clip} gives a non-finite pose at {time} s")]
    NonFinitePose {
        /// The asset name.
        file: &'static str,
        /// The clip's name.
        clip: &'static str,
        /// The time the pose was taken at.
        time: f32,
    },
    /// An unskinned mesh sits on no bone.
    #[error("{file}: the mesh on node {node} is on no bone")]
    DetachedMesh {
        /// The asset name.
        file: &'static str,
        /// The mesh node's name, or its index when it has none.
        node: String,
    },
    /// The headless body carries the head or neck.
    #[error("{file}: the headless body has a mesh on {bone}")]
    HeadInHeadlessBody {
        /// The asset name.
        file: &'static str,
        /// `head` or `neck_01`.
        bone: &'static str,
    },
    /// The head model has no `attach` node.
    #[error("{file}: no node named attach")]
    NoAttach {
        /// The asset name.
        file: &'static str,
    },
    /// The standing head is not under the standing eye.
    #[error(
        "{file}: the head height {height:.2} m is not within {HEAD_TOLERANCE} m of the eye {eye:.2} m"
    )]
    HeadHeight {
        /// The asset name.
        file: &'static str,
        /// The head bone's height in `idle` at time 0.
        height: f32,
        /// The height the camera sits at.
        eye: f32,
    },
}

/// Checks the three character files `assets` resolves, and that the standing head of the
/// full body sits within 0.05 m of `eye_height`.
///
/// # Errors
/// [`BodyError`] naming the file and the fault.
pub fn check(assets: &AssetRoots, eye_height: f32) -> Result<BodyReport, BodyError> {
    let body = resolve(assets, BODY)?;
    let player_body = resolve(assets, PLAYER_BODY)?;
    let head = resolve(assets, HEAD)?;

    let (model, humanoid) = check_rig(BODY, &body)?;
    let (player_model, player_humanoid) = check_rig(PLAYER_BODY, &player_body)?;
    check_headless(PLAYER_BODY, &player_model, player_humanoid)?;

    let head_model = load_model(&head).map_err(|source| BodyError::Load { file: HEAD, source })?;
    if head_model.node("attach").is_none() {
        return Err(BodyError::NoAttach { file: HEAD });
    }

    let head_height = standing_head_height(BODY, &model, humanoid)?;
    if (head_height - eye_height).abs() > HEAD_TOLERANCE {
        return Err(BodyError::HeadHeight {
            file: BODY,
            height: head_height,
            eye: eye_height,
        });
    }
    Ok(BodyReport {
        body,
        player_body,
        head,
        head_height,
        eye_height,
    })
}

fn resolve(assets: &AssetRoots, file: &'static str) -> Result<PathBuf, BodyError> {
    assets
        .resolve(Path::new(file))
        .map_err(|source| BodyError::Resolve { file, source })
}

/// Loads a body and checks its bones, clips and meshes.
fn check_rig(file: &'static str, path: &Path) -> Result<(Model, Humanoid), BodyError> {
    let (model, humanoid) =
        load_humanoid(path).map_err(|source| BodyError::Load { file, source })?;
    check_joints(file, &model, humanoid)?;
    for clip in CLIPS {
        play_clip(file, &model, clip)?;
    }
    check_no_detached_mesh(file, &model, humanoid)?;
    Ok((model, humanoid))
}

/// Every resolved bone node is a joint of a skin, or, in a body with no skin, carries no
/// mesh. A mesh node named like a bone and ahead of it would otherwise win the name lookup.
fn check_joints(file: &'static str, model: &Model, humanoid: Humanoid) -> Result<(), BodyError> {
    for bone in Bone::ALL {
        let node = humanoid.node(bone);
        let is_joint = if model.skins().is_empty() {
            model.meshes().iter().all(|mesh| mesh.node != node)
        } else {
            model.skins().iter().any(|skin| skin.joints.contains(&node))
        };
        if !is_joint {
            return Err(BodyError::NotJoint {
                file,
                bone: bone.name(),
            });
        }
    }
    Ok(())
}

/// Plays `clip` from 0 to its duration in sim ticks; every pose must be finite.
fn play_clip(file: &'static str, model: &Model, clip: &'static str) -> Result<(), BodyError> {
    let mut player = ClipPlayer::new(model, clip, Playback::Once)
        .map_err(|source| BodyError::Clip { file, source })?;
    let duration = model.clips()[player.clip()].duration();
    if !duration.is_finite() {
        return Err(BodyError::NonFinitePose {
            file,
            clip,
            time: duration,
        });
    }
    let tick = 1.0 / TICK_HZ as f32;
    loop {
        let pose = player.pose(model);
        if !pose.nodes().iter().all(Affine3A::is_finite) {
            return Err(BodyError::NonFinitePose {
                file,
                clip,
                time: player.time(),
            });
        }
        if player.is_finished(model) {
            return Ok(());
        }
        player.advance(model, tick);
    }
}

/// Whether `node` is `ancestor` or sits below it.
fn is_under(model: &Model, node: usize, ancestor: usize) -> bool {
    let mut at = Some(node);
    while let Some(current) = at {
        if current == ancestor {
            return true;
        }
        at = model.nodes()[current].parent;
    }
    false
}

/// Every mesh is skinned or sits under `pelvis`.
fn check_no_detached_mesh(
    file: &'static str,
    model: &Model,
    humanoid: Humanoid,
) -> Result<(), BodyError> {
    let pelvis = humanoid.node(Bone::Pelvis);
    for mesh in model.meshes() {
        if mesh.skin.is_none() && !is_under(model, mesh.node, pelvis) {
            let node = model.nodes()[mesh.node]
                .name
                .clone()
                .unwrap_or_else(|| format!("#{}", mesh.node));
            return Err(BodyError::DetachedMesh { file, node });
        }
    }
    Ok(())
}

/// The bones the first-person body must not show.
const HIDDEN_BONES: [Bone; 2] = [Bone::Head, Bone::Neck];

/// No vertex is weighted on the head or neck and no unskinned mesh sits under either.
fn check_headless(file: &'static str, model: &Model, humanoid: Humanoid) -> Result<(), BodyError> {
    for mesh in model.meshes() {
        let shown = match &mesh.skin {
            Some(skin) => weighted_hidden_bone(model, humanoid, skin),
            None => HIDDEN_BONES
                .into_iter()
                .find(|&bone| is_under(model, mesh.node, humanoid.node(bone))),
        };
        if let Some(bone) = shown {
            return Err(BodyError::HeadInHeadlessBody {
                file,
                bone: bone.name(),
            });
        }
    }
    Ok(())
}

/// The first head or neck bone that some vertex of `skin` weighs on.
fn weighted_hidden_bone(model: &Model, humanoid: Humanoid, skin: &MeshSkin) -> Option<Bone> {
    let joints = &model.skins()[skin.skin].joints;
    skin.joints
        .iter()
        .zip(&skin.weights)
        .flat_map(|(influences, weights)| influences.iter().zip(weights))
        .filter(|&(_, &weight)| weight > 0.0)
        .find_map(|(&joint, _)| {
            let node = joints[usize::from(joint)];
            HIDDEN_BONES
                .into_iter()
                .find(|&bone| humanoid.node(bone) == node)
        })
}

/// The head bone's model-space height in `idle` at time 0.
fn standing_head_height(
    file: &'static str,
    model: &Model,
    humanoid: Humanoid,
) -> Result<f32, BodyError> {
    let idle = ClipPlayer::new(model, "idle", Playback::Once)
        .map_err(|source| BodyError::Clip { file, source })?;
    Ok(humanoid.bone(&idle.pose(model), Bone::Head).translation.y)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use moho_core::assets::AssetRoots;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::*;
    use crate::assets_root;
    use crate::tuning::SHIPPED_BODY;

    /// A glTF binary split into its JSON and BIN chunks, so a test can edit either.
    struct Gltf {
        json: Value,
        bin: Vec<u8>,
    }

    fn word(bytes: &[u8], at: usize) -> usize {
        let word: [u8; 4] = bytes[at..at + 4].try_into().expect("four bytes");
        u32::from_le_bytes(word) as usize
    }

    impl Gltf {
        fn parse(bytes: &[u8]) -> Self {
            let json_len = word(bytes, 12);
            let json = serde_json::from_slice(&bytes[20..20 + json_len]).expect("JSON chunk");
            let bin_at = 20 + json_len;
            let bin_len = word(bytes, bin_at);
            let bin = bytes[bin_at + 8..bin_at + 8 + bin_len].to_vec();
            Self { json, bin }
        }

        fn pack(&self) -> Vec<u8> {
            let mut json = serde_json::to_vec(&self.json).expect("JSON serializes");
            while !json.len().is_multiple_of(4) {
                json.push(b' ');
            }
            let total = 12 + 8 + json.len() + 8 + self.bin.len();
            let mut out = b"glTF".to_vec();
            out.extend(2u32.to_le_bytes());
            out.extend(u32::try_from(total).expect("small file").to_le_bytes());
            out.extend(u32::try_from(json.len()).expect("small file").to_le_bytes());
            out.extend(b"JSON");
            out.extend(json);
            out.extend(
                u32::try_from(self.bin.len())
                    .expect("small file")
                    .to_le_bytes(),
            );
            out.extend(b"BIN\0");
            out.extend(&self.bin);
            out
        }

        fn node(&self, name: &str) -> usize {
            self.json["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .position(|node| node["name"] == name)
                .unwrap_or_else(|| panic!("fixture has no node {name}"))
        }

        fn rename_node(&mut self, from: &str, to: &str) {
            let node = self.node(from);
            self.json["nodes"][node]["name"] = json!(to);
        }

        fn rename_clip(&mut self, from: &str, to: &str) {
            let clip = self.json["animations"]
                .as_array_mut()
                .expect("animations")
                .iter_mut()
                .find(|clip| clip["name"] == from)
                .unwrap_or_else(|| panic!("fixture has no clip {from}"));
            clip["name"] = json!(to);
        }

        fn push_child(&mut self, parent: usize, child: usize) {
            let node = self.json["nodes"][parent]
                .as_object_mut()
                .expect("node object");
            node.entry("children")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("children")
                .push(json!(child));
        }

        /// Inserts `node` at index `at`, renumbering every reference, and makes it the
        /// first root of the scene.
        fn insert_root_node(&mut self, at: usize, node: Value) {
            fn bump(value: &mut Value, at: usize) {
                if let Some(index) = value.as_u64().filter(|&index| index as usize >= at) {
                    *value = json!(index + 1);
                }
            }
            for scene in self.json["scenes"].as_array_mut().expect("scenes") {
                scene["nodes"]
                    .as_array_mut()
                    .expect("scene nodes")
                    .iter_mut()
                    .for_each(|n| bump(n, at));
            }
            for existing in self.json["nodes"].as_array_mut().expect("nodes") {
                if let Some(children) = existing.get_mut("children") {
                    children
                        .as_array_mut()
                        .expect("children")
                        .iter_mut()
                        .for_each(|n| bump(n, at));
                }
            }
            for skin in self
                .json
                .get_mut("skins")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                skin["joints"]
                    .as_array_mut()
                    .expect("joints")
                    .iter_mut()
                    .for_each(|n| bump(n, at));
                bump(&mut skin["skeleton"], at);
            }
            for clip in self.json["animations"].as_array_mut().expect("animations") {
                for channel in clip["channels"].as_array_mut().expect("channels") {
                    bump(&mut channel["target"]["node"], at);
                }
            }
            self.json["nodes"]
                .as_array_mut()
                .expect("nodes")
                .insert(at, node);
            self.json["scenes"][0]["nodes"]
                .as_array_mut()
                .expect("scene nodes")
                .insert(0, json!(at));
        }

        /// The node carrying the body mesh and its skin.
        fn skinned_node(&self) -> usize {
            self.json["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .position(|node| node.get("skin").is_some())
                .expect("fixture has a skinned node")
        }

        /// Takes the skin away and hangs the body mesh under `pelvis`.
        fn make_unskinned(&mut self) {
            let mesh_node = self.skinned_node();
            self.json
                .as_object_mut()
                .expect("root object")
                .remove("skins");
            self.json["nodes"][mesh_node]
                .as_object_mut()
                .expect("node object")
                .remove("skin");
            self.json["scenes"][0]["nodes"]
                .as_array_mut()
                .expect("scene nodes")
                .retain(|root| root != mesh_node);
            let pelvis = self.node("pelvis");
            self.push_child(pelvis, mesh_node);
        }

        /// Gives the body mesh's first vertex all its weight on `joint`.
        fn weight_first_vertex_on(&mut self, joint: u8) {
            let mesh = self.json["nodes"][self.skinned_node()]["mesh"]
                .as_u64()
                .expect("mesh index") as usize;
            let attributes = &self.json["meshes"][mesh]["primitives"][0]["attributes"];
            let joints = attributes["JOINTS_0"].as_u64().expect("joints") as usize;
            let weights = attributes["WEIGHTS_0"].as_u64().expect("weights") as usize;
            let start = |accessor: usize| {
                let view = self.json["accessors"][accessor]["bufferView"]
                    .as_u64()
                    .expect("bufferView") as usize;
                self.json["bufferViews"][view]["byteOffset"]
                    .as_u64()
                    .unwrap_or(0) as usize
            };
            let (joints, weights) = (start(joints), start(weights));
            self.bin[joints..joints + 4].copy_from_slice(&[joint, 0, 0, 0]);
            for (slot, weight) in [1.0f32, 0.0, 0.0, 0.0].into_iter().enumerate() {
                let at = weights + slot * 4;
                self.bin[at..at + 4].copy_from_slice(&weight.to_le_bytes());
            }
        }
    }

    /// An art folder holding the placeholder files, `edit` applied to each of `edited`.
    fn art_editing(edited: &[&str], edit: impl Fn(&mut Gltf)) -> TempDir {
        let dir = tempfile::tempdir().expect("temp folder");
        for name in [BODY, PLAYER_BODY, HEAD] {
            let bytes = fs::read(assets_root().join(name)).expect("placeholder file");
            let bytes = if edited.contains(&name) {
                let mut gltf = Gltf::parse(&bytes);
                edit(&mut gltf);
                gltf.pack()
            } else {
                bytes
            };
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().expect("parent")).expect("art folders");
            fs::write(path, bytes).expect("art file");
        }
        dir
    }

    fn check_over(folder: Option<&Path>) -> Result<BodyReport, BodyError> {
        let mut roots = AssetRoots::placeholders(&assets_root()).expect("placeholders");
        if let Some(folder) = folder {
            roots = roots.with_folder(folder).expect("art folder");
        }
        check(&roots, SHIPPED_BODY.standing.eye_height)
    }

    fn file_name(asset: &str) -> &str {
        Path::new(asset)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("asset file name")
    }

    /// Asserts the check refuses `folder`, with a message naming `file` and `named`.
    fn assert_refused(folder: &TempDir, file: &str, named: &str) {
        let error = check_over(Some(folder.path())).expect_err("the body is refused");

        let message = error.to_string();
        assert!(message.contains(file_name(file)), "{message}");
        assert!(message.contains(named), "{message}");
    }

    #[test]
    fn placeholders_pass_on_the_box_mannequin() {
        let report = check_over(None).expect("the box mannequin passes");

        let shown = report.to_string();
        for name in [BODY, PLAYER_BODY, HEAD] {
            let source = assets_root().join(name);
            assert!(shown.contains(&source.display().to_string()), "{shown}");
        }
    }

    #[test]
    fn report_names_sources_bones_clips_and_head_height() {
        let art = art_editing(&[], |_| {});

        let report = check_over(Some(art.path())).expect("a usable body passes");

        let shown = report.to_string();
        for name in [BODY, PLAYER_BODY, HEAD] {
            let source = art.path().join(name);
            assert!(shown.contains(&source.display().to_string()), "{shown}");
        }
        assert!(
            !shown.contains(&assets_root().join(BODY).display().to_string()),
            "{shown}"
        );
        assert!(shown.contains("16"), "{shown}");
        for clip in CLIPS {
            assert!(shown.contains(clip), "{clip} in {shown}");
        }
        assert!(shown.contains("head height"), "{shown}");
        assert!(
            shown.contains(&format!("{:.2}", SHIPPED_BODY.standing.eye_height)),
            "{shown}"
        );
    }

    #[test]
    fn missing_bone_is_named() {
        let art = art_editing(&[BODY], |gltf| gltf.rename_node("hand_r", "hand_right"));

        assert_refused(&art, BODY, "hand_r");
    }

    #[test]
    fn missing_clip_is_named() {
        let art = art_editing(&[BODY], |gltf| gltf.rename_clip("crawl", "crawl_slowly"));

        assert_refused(&art, BODY, "crawl");
    }

    #[test]
    fn bone_name_on_a_non_joint_is_refused() {
        let art = art_editing(&[BODY], |gltf| {
            gltf.insert_root_node(1, json!({ "name": "head", "mesh": 0 }));
        });

        assert_refused(&art, BODY, "head is not a joint");
    }

    #[test]
    fn head_mesh_in_headless_body_is_refused() {
        let art = art_editing(&[PLAYER_BODY], |gltf| {
            let head = gltf.node("head");
            let cap = gltf.json["nodes"].as_array().expect("nodes").len();
            gltf.json["nodes"]
                .as_array_mut()
                .expect("nodes")
                .push(json!({ "name": "cap", "mesh": 0 }));
            gltf.push_child(head, cap);
        });

        assert_refused(&art, PLAYER_BODY, "head");
    }

    #[test]
    fn head_weighted_vertex_in_headless_body_is_refused() {
        let art = art_editing(&[PLAYER_BODY], |gltf| gltf.weight_first_vertex_on(3));

        assert_refused(&art, PLAYER_BODY, "head");
    }

    #[test]
    fn neck_weighted_vertex_in_headless_body_is_refused() {
        let art = art_editing(&[PLAYER_BODY], |gltf| gltf.weight_first_vertex_on(2));

        assert_refused(&art, PLAYER_BODY, "neck_01");
    }

    #[test]
    fn head_far_from_eye_is_refused() {
        let art = art_editing(&[BODY], |gltf| {
            let neck = gltf.node("neck_01");
            gltf.json["nodes"][neck]["translation"] = json!([0.0, 0.3, 0.0]);
        });

        assert_refused(&art, BODY, "the head height");
    }

    #[test]
    fn mesh_on_no_bone_is_refused() {
        let art = art_editing(&[BODY], |gltf| {
            let end = gltf.json["nodes"].as_array().expect("nodes").len();
            gltf.insert_root_node(end, json!({ "name": "sash", "mesh": 0 }));
        });

        assert_refused(&art, BODY, "the mesh");
    }

    #[test]
    fn unskinned_bone_parented_body_passes() {
        let art = art_editing(&[BODY, PLAYER_BODY], Gltf::make_unskinned);

        let result = check_over(Some(art.path()));

        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn bone_name_on_a_mesh_node_in_unskinned_body_is_refused() {
        let art = art_editing(&[BODY], |gltf| {
            gltf.make_unskinned();
            gltf.insert_root_node(1, json!({ "name": "head", "mesh": 0 }));
        });

        assert_refused(&art, BODY, "head is not a joint");
    }

    impl Gltf {
        /// Sets the influences of the body mesh's first vertex.
        fn set_first_vertex_influences(&mut self, joints: [u8; 4], weights: [f32; 4]) {
            let mesh = self.json["nodes"][self.skinned_node()]["mesh"]
                .as_u64()
                .expect("mesh index") as usize;
            let attributes = &self.json["meshes"][mesh]["primitives"][0]["attributes"];
            let joints_accessor = attributes["JOINTS_0"].as_u64().expect("joints") as usize;
            let weights_accessor = attributes["WEIGHTS_0"].as_u64().expect("weights") as usize;
            let start = |accessor: usize| {
                let view = self.json["accessors"][accessor]["bufferView"]
                    .as_u64()
                    .expect("bufferView") as usize;
                self.json["bufferViews"][view]["byteOffset"]
                    .as_u64()
                    .unwrap_or(0) as usize
            };
            let (joints_at, weights_at) = (start(joints_accessor), start(weights_accessor));
            self.bin[joints_at..joints_at + 4].copy_from_slice(&joints);
            for (slot, weight) in weights.into_iter().enumerate() {
                let at = weights_at + slot * 4;
                self.bin[at..at + 4].copy_from_slice(&weight.to_le_bytes());
            }
        }
    }

    #[test]
    fn head_without_attach_is_refused() {
        let art = art_editing(&[HEAD], |gltf| gltf.rename_node("attach", "mount"));

        assert_refused(&art, HEAD, "attach");
    }

    #[test]
    fn zero_weight_on_head_in_headless_body_passes() {
        let art = art_editing(&[PLAYER_BODY], |gltf| {
            gltf.set_first_vertex_influences([0, 3, 0, 0], [1.0, 0.0, 0.0, 0.0]);
        });

        let result = check_over(Some(art.path()));

        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn unskinned_part_on_a_bone_of_a_skinned_body_passes() {
        let art = art_editing(&[BODY], |gltf| {
            let hand = gltf.node("hand_r");
            let strap = gltf.json["nodes"].as_array().expect("nodes").len();
            gltf.json["nodes"]
                .as_array_mut()
                .expect("nodes")
                .push(json!({ "name": "strap", "mesh": 0 }));
            gltf.push_child(hand, strap);
        });

        let result = check_over(Some(art.path()));

        assert!(result.is_ok(), "{:?}", result.err());
    }
}
