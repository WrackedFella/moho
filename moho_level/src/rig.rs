//! glTF skins, vertex influences and animations into the plain types of [`crate::anim`]
//! and [`crate::model`]. Node references resolve through the scene walk's index map, so
//! a joint or channel target outside the default scene is refused.

use std::borrow::Cow;

use glam::{Affine3A, Mat4, Quat, Vec3};
use gltf::animation::util::ReadOutputs;
use gltf::animation::{Interpolation as GltfInterpolation, Property};
use gltf::mesh::Semantic;

use crate::anim::{Channel, Clip, Interpolation, Track};
use crate::error::LevelErrorKind;
use crate::{MeshSkin, ModelSkin};

/// Every skin in the file, in file order.
pub(crate) fn skins(
    document: &gltf::Document,
    buffers: &[Cow<'_, [u8]>],
    walk_index: &[Option<usize>],
) -> Result<Vec<ModelSkin>, LevelErrorKind> {
    document
        .skins()
        .map(|skin| {
            let bad = |reason: String| LevelErrorKind::BadSkin {
                skin: skin.index(),
                reason,
            };
            let joints = skin
                .joints()
                .map(|joint| {
                    walk_index[joint.index()].ok_or_else(|| {
                        bad(format!(
                            "joint node {} is outside the default scene",
                            joint.index()
                        ))
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let inverse_binds = if skin.inverse_bind_matrices().is_some() {
                skin.reader(|b| buffers.get(b.index()).map(|data| &**data))
                    .read_inverse_bind_matrices()
                    .ok_or_else(|| bad("inverse-bind matrices out of buffer range".into()))?
                    .map(|m| Affine3A::from_mat4(Mat4::from_cols_array_2d(&m)))
                    .collect()
            } else {
                vec![Affine3A::IDENTITY; joints.len()]
            };
            if inverse_binds.len() != joints.len() {
                return Err(bad(format!(
                    "{} joints but {} inverse-bind matrices",
                    joints.len(),
                    inverse_binds.len()
                )));
            }
            Ok(ModelSkin {
                joints,
                inverse_binds,
            })
        })
        .collect()
}

/// The first joint and weight set of `primitive` on a node skinned by `skin`, weights
/// renormalised to sum to 1. Further sets are ignored with a warning.
pub(crate) fn mesh_skin(
    mesh: &gltf::Mesh<'_>,
    primitive: &gltf::Primitive<'_>,
    skin: &gltf::Skin<'_>,
    buffers: &[Cow<'_, [u8]>],
) -> Result<MeshSkin, LevelErrorKind> {
    let bad = |reason: String| LevelErrorKind::BadMesh {
        mesh: mesh.index(),
        reason,
    };
    if primitive.get(&Semantic::Joints(1)).is_some() {
        tracing::warn!(
            mesh = mesh.index(),
            "ignoring joint sets past the first: four influences per vertex"
        );
    }
    let reader = primitive.reader(|b| buffers.get(b.index()).map(|data| &**data));
    let joints: Vec<[u16; 4]> = reader
        .read_joints(0)
        .ok_or_else(|| bad("skinned primitive has no readable JOINTS_0".into()))?
        .into_u16()
        .collect();
    let raw_weights: Vec<[f32; 4]> = reader
        .read_weights(0)
        .ok_or_else(|| bad("skinned primitive has no readable WEIGHTS_0".into()))?
        .into_f32()
        .collect();
    let vertices = primitive
        .get(&Semantic::Positions)
        .map_or(0, |positions| positions.count());
    if joints.len() != vertices || raw_weights.len() != vertices {
        return Err(bad(format!(
            "{vertices} vertices but {} joint and {} weight entries",
            joints.len(),
            raw_weights.len()
        )));
    }
    let joint_count = skin.joints().len();
    for (vertex, influences) in joints.iter().enumerate() {
        if let Some(&joint) = influences.iter().find(|&&j| usize::from(j) >= joint_count) {
            return Err(bad(format!(
                "vertex {vertex} names joint {joint} but the skin has {joint_count}"
            )));
        }
    }
    let weights = raw_weights
        .iter()
        .enumerate()
        .map(|(vertex, w)| {
            let sum: f32 = w.iter().sum();
            if sum.is_finite() && sum > 0.0 {
                Ok(w.map(|x| x / sum))
            } else {
                Err(bad(format!("vertex {vertex} has weights summing to {sum}")))
            }
        })
        .collect::<Result<_, _>>()?;
    Ok(MeshSkin {
        skin: skin.index(),
        joints,
        weights,
    })
}

/// Every animation in the file, in file order. Morph-target weight channels are skipped
/// with a warning.
pub(crate) fn clips(
    document: &gltf::Document,
    buffers: &[Cow<'_, [u8]>],
    walk_index: &[Option<usize>],
) -> Result<Vec<Clip>, LevelErrorKind> {
    document
        .animations()
        .map(|animation| {
            let channels = animation
                .channels()
                .filter(|channel| {
                    let morph = channel.target().property() == Property::MorphTargetWeights;
                    if morph {
                        tracing::warn!(
                            animation = animation.index(),
                            channel = channel.index(),
                            "skipping morph-target weight channel"
                        );
                    }
                    !morph
                })
                .map(|channel| self::channel(&channel, buffers, walk_index))
                .collect::<Result<_, _>>()?;
            Ok(Clip::new(animation.name().map(str::to_owned), channels))
        })
        .collect()
}

fn channel(
    channel: &gltf::animation::Channel<'_>,
    buffers: &[Cow<'_, [u8]>],
    walk_index: &[Option<usize>],
) -> Result<Channel, LevelErrorKind> {
    let bad = |reason: String| LevelErrorKind::BadAnimation {
        animation: channel.animation().index(),
        reason,
    };
    let target = channel.target().node().index();
    let node = walk_index[target].ok_or_else(|| {
        bad(format!(
            "channel {} targets node {target}, outside the default scene",
            channel.index()
        ))
    })?;
    let interpolation = match channel.sampler().interpolation() {
        GltfInterpolation::Step => Interpolation::Step,
        GltfInterpolation::Linear => Interpolation::Linear,
        GltfInterpolation::CubicSpline => Interpolation::CubicSpline,
    };
    let reader = channel.reader(|b| buffers.get(b.index()).map(|data| &**data));
    let out_of_range = || {
        bad(format!(
            "channel {} keys out of buffer range",
            channel.index()
        ))
    };
    let times: Vec<f32> = reader.read_inputs().ok_or_else(out_of_range)?.collect();
    if times.is_empty() {
        return Err(bad(format!("channel {} has no keys", channel.index())));
    }
    if !times.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(bad(format!(
            "channel {} key times are not increasing",
            channel.index()
        )));
    }
    // A cubic spline holds in-tangent, value and out-tangent per key.
    let per_key = if interpolation == Interpolation::CubicSpline {
        3
    } else {
        1
    };
    let keys = times.len();
    let counted = |values: usize| {
        if values == keys * per_key {
            Ok(())
        } else {
            Err(bad(format!(
                "channel {} has {keys} key times but {values} values",
                channel.index()
            )))
        }
    };
    let track = match reader.read_outputs().ok_or_else(out_of_range)? {
        ReadOutputs::Translations(values) => {
            let values: Vec<Vec3> = values.map(Vec3::from).collect();
            counted(values.len())?;
            Track::Translation { times, values }
        }
        ReadOutputs::Rotations(values) => {
            let values: Vec<Quat> = values.into_f32().map(Quat::from_array).collect();
            counted(values.len())?;
            Track::Rotation { times, values }
        }
        ReadOutputs::Scales(values) => {
            let values: Vec<Vec3> = values.map(Vec3::from).collect();
            counted(values.len())?;
            Track::Scale { times, values }
        }
        ReadOutputs::MorphTargetWeights(_) => {
            unreachable!("morph-target weight channels are filtered out before reading")
        }
    };
    Ok(Channel {
        node,
        interpolation,
        track,
    })
}
