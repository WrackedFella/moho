//! The humanoid bone convention: the Unreal mannequin names both game lines key off.

use std::path::Path;

use glam::Affine3A;

use crate::anim::Pose;
use crate::{LevelError, Model, load_model};

/// A bone every humanoid rig has, named as in the Unreal mannequin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Bone {
    Pelvis,
    Spine,
    Neck,
    Head,
    UpperArmL,
    LowerArmL,
    HandL,
    UpperArmR,
    LowerArmR,
    HandR,
    ThighL,
    CalfL,
    FootL,
    ThighR,
    CalfR,
    FootR,
}

impl Bone {
    /// Every required bone, in the order [`MissingBones`] lists them.
    pub const ALL: [Bone; 16] = [
        Bone::Pelvis,
        Bone::Spine,
        Bone::Neck,
        Bone::Head,
        Bone::UpperArmL,
        Bone::LowerArmL,
        Bone::HandL,
        Bone::UpperArmR,
        Bone::LowerArmR,
        Bone::HandR,
        Bone::ThighL,
        Bone::CalfL,
        Bone::FootL,
        Bone::ThighR,
        Bone::CalfR,
        Bone::FootR,
    ];

    /// The node name a rig gives this bone.
    pub fn name(self) -> &'static str {
        match self {
            Bone::Pelvis => "pelvis",
            Bone::Spine => "spine_01",
            Bone::Neck => "neck_01",
            Bone::Head => "head",
            Bone::UpperArmL => "upperarm_l",
            Bone::LowerArmL => "lowerarm_l",
            Bone::HandL => "hand_l",
            Bone::UpperArmR => "upperarm_r",
            Bone::LowerArmR => "lowerarm_r",
            Bone::HandR => "hand_r",
            Bone::ThighL => "thigh_l",
            Bone::CalfL => "calf_l",
            Bone::FootL => "foot_l",
            Bone::ThighR => "thigh_r",
            Bone::CalfR => "calf_r",
            Bone::FootR => "foot_r",
        }
    }
}

/// A model's humanoid bones resolved to node indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Humanoid {
    nodes: [usize; 16],
}

/// The required bones a rig has no node for.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("missing humanoid bones: {}", .bones.join(", "))]
pub struct MissingBones {
    /// The node names, in [`Bone::ALL`] order.
    pub bones: Vec<&'static str>,
}

impl Humanoid {
    /// Resolve each bone to the first node with its name.
    ///
    /// # Errors
    /// [`MissingBones`] listing every bone with no node.
    pub fn of(model: &Model) -> Result<Humanoid, MissingBones> {
        let mut nodes = [0; 16];
        let mut missing = Vec::new();
        for (slot, bone) in nodes.iter_mut().zip(Bone::ALL) {
            let found = model
                .nodes()
                .iter()
                .position(|n| n.name.as_deref() == Some(bone.name()));
            match found {
                Some(index) => *slot = index,
                None => missing.push(bone.name()),
            }
        }
        if missing.is_empty() {
            Ok(Humanoid { nodes })
        } else {
            Err(MissingBones { bones: missing })
        }
    }

    /// The index in [`Model::nodes`] of `bone`.
    pub fn node(self, bone: Bone) -> usize {
        self.nodes[bone as usize]
    }

    /// The model-space transform of `bone` in `pose`.
    ///
    /// # Panics
    /// When `pose` is not of the model this was resolved on.
    pub fn bone(self, pose: &Pose, bone: Bone) -> Affine3A {
        pose.nodes()[self.node(bone)]
    }
}

/// Load the model at `path` and resolve its humanoid bones.
///
/// # Errors
/// As [`load_model`]; [`crate::LevelErrorKind::NotHumanoid`] when a required bone is missing.
pub fn load_humanoid(path: &Path) -> Result<(Model, Humanoid), LevelError> {
    let model = load_model(path)?;
    match Humanoid::of(&model) {
        Ok(humanoid) => Ok((model, humanoid)),
        Err(missing) => Err(LevelError::new(path, missing.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::Bone;

    #[test]
    fn bone_names_are_the_mannequin_names() {
        let names: Vec<&str> = Bone::ALL.iter().map(|bone| bone.name()).collect();

        assert_eq!(
            names,
            [
                "pelvis",
                "spine_01",
                "neck_01",
                "head",
                "upperarm_l",
                "lowerarm_l",
                "hand_l",
                "upperarm_r",
                "lowerarm_r",
                "hand_r",
                "thigh_l",
                "calf_l",
                "foot_l",
                "thigh_r",
                "calf_r",
                "foot_r",
            ]
        );
    }
}
