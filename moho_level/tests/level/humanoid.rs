use std::path::Path;
use std::sync::LazyLock;

use glam::{Affine3A, Quat, Vec3};
use moho_level::anim::{ClipPlayer, Playback, Pose};
use moho_level::humanoid::{Bone, Humanoid, MissingBones, load_humanoid};
use moho_level::{LevelErrorKind, Model, load_model};
use moho_physics::{HitTag, HitVolume, PhysicsWorld, RayHit, RayQuery};
use proptest::prelude::*;
use tempfile::TempDir;

use crate::support::humanoid::{
    BONE_NODES, CROUCH_DROP, CROUCH_DURATION, DUPLICATE_OFFSET, FIRST_DUPLICATE, HEAD_REST_Y,
    HumanoidSpec, IK_HAND_GUN, WALK_DURATION, hand_r_local, hand_r_parent, write_humanoid,
};

/// One simulation tick.
const TICK: f32 = 1.0 / 60.0;

const HEAD: HitTag = HitTag(10);

fn load(spec: &HumanoidSpec) -> (TempDir, Model) {
    let (dir, path) = write_humanoid(spec);
    let model = load_model(&path).expect("humanoid loads");
    (dir, model)
}

fn rig() -> (TempDir, Model) {
    load(&HumanoidSpec::default())
}

fn humanoid(model: &Model) -> Humanoid {
    Humanoid::of(model).expect("the fixture has every bone")
}

/// A player on `clip` advanced by `ticks` ticks from tick 0.
fn player_at(model: &Model, clip: &str, playback: Playback, ticks: usize) -> ClipPlayer {
    let mut player = ClipPlayer::new(model, clip, playback).expect("clip exists");
    for _ in 0..ticks {
        player.advance(model, TICK);
    }
    player
}

fn bits(m: Affine3A) -> [u32; 12] {
    m.to_cols_array().map(f32::to_bits)
}

fn pose_bits(pose: &Pose) -> Vec<[u32; 12]> {
    pose.nodes().iter().copied().map(bits).collect()
}

fn near(actual: Affine3A, expected: Affine3A) {
    assert!(
        actual.abs_diff_eq(expected, 1e-4),
        "{actual:?} != {expected:?}"
    );
}

fn omitting(bones: &[&'static str]) -> HumanoidSpec {
    HumanoidSpec {
        omit: bones.to_vec(),
        ..HumanoidSpec::default()
    }
}

#[test]
fn sim_reads_hand_r_at_tick_30_in_character_space() {
    let (_dir, model) = rig();
    let humanoid = humanoid(&model);
    let player = player_at(&model, "walk", Playback::Loop, 30);

    let hand = humanoid.bone(&player.pose(&model), Bone::HandR);

    near(hand, hand_r_parent() * hand_r_local(0.5));
    let (_, rotation, translation) = hand.to_scale_rotation_translation();
    assert!(
        translation.abs_diff_eq(Vec3::new(0.75, 1.5, 0.2), 1e-4),
        "{translation:?}"
    );
    assert!(
        rotation.abs_diff_eq(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4), 1e-4),
        "{rotation:?}"
    );
    assert_eq!(model.clips()[player.clip()].duration(), WALK_DURATION);
}

#[test]
fn two_runs_give_bit_equal_poses_at_tick_45() {
    let (_first_dir, first_model) = rig();
    let (_second_dir, second_model) = rig();
    let first_player = player_at(&first_model, "walk", Playback::Loop, 45);
    let second_player = player_at(&second_model, "walk", Playback::Loop, 45);

    let first = first_player.pose(&first_model);
    let second = second_player.pose(&second_model);

    let (first_bones, second_bones) = (humanoid(&first_model), humanoid(&second_model));
    for bone in Bone::ALL {
        assert_eq!(
            bits(first_bones.bone(&first, bone)),
            bits(second_bones.bone(&second, bone)),
            "{}",
            bone.name()
        );
    }
    assert_eq!(pose_bits(&first), pose_bits(&second));
    let at_start = player_at(&first_model, "walk", Playback::Loop, 0).pose(&first_model);
    assert_ne!(
        pose_bits(&first),
        pose_bits(&at_start),
        "tick 45 differs from tick 0, so equality is not vacuous"
    );
}

/// Built once: writing and loading glTF per proptest case is the slow part.
static SHARED_RIG: LazyLock<Model> = LazyLock::new(|| rig().1);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn poses_repeat_for_any_tick_count_and_speed(ticks in 0usize..600, speed in -2.0f32..2.0) {
        let model = &*SHARED_RIG;
        let humanoid = humanoid(model);
        let run = || {
            let mut player = ClipPlayer::new(model, "walk", Playback::Loop).expect("clip exists");
            player.set_speed(speed);
            for _ in 0..ticks {
                player.advance(model, TICK);
            }
            player.pose(model)
        };

        let (first, second) = (run(), run());

        prop_assert_eq!(pose_bits(&first), pose_bits(&second));
        for bone in Bone::ALL {
            prop_assert_eq!(
                bits(humanoid.bone(&first, bone)),
                bits(first.nodes()[humanoid.node(bone)]),
                "{}",
                bone.name()
            );
        }
    }
}

/// The hit volume a game builds for the head: the head's model-space pose in the body
/// frame, whose origin is the character's model-space origin.
fn head_volume(head: Affine3A) -> HitVolume {
    HitVolume {
        offset: Vec3::from(head.translation),
        rotation: Quat::from_mat3a(&head.matrix3),
        half_extents: Vec3::splat(0.1),
        tag: HEAD,
    }
}

/// A ray along +X, 3 m short of the character, at `height` above its body origin.
fn ray_at(world: &PhysicsWorld, body: Vec3, height: f32) -> Option<RayHit> {
    world.cast_ray(&RayQuery {
        origin: body + Vec3::new(-3.0, height, 0.0),
        direction: Vec3::X,
        max_distance: 20.0,
        ignore: None,
    })
}

#[test]
fn head_hit_volume_drops_with_the_crouch() {
    let (_dir, model) = rig();
    let humanoid = humanoid(&model);
    let mut player = player_at(&model, "crouch", Playback::Once, 0);
    let standing = head_volume(humanoid.bone(&player.pose(&model), Bone::Head));
    let mut world = PhysicsWorld::new();
    let character = world.add_character(Vec3::new(0.0, 2.0, 0.0));
    world
        .set_hit_volumes(character, &[standing])
        .expect("character is live");
    world.step(TICK);
    let body = world
        .character(character)
        .expect("character is live")
        .position;
    let old_height = standing.offset.y;
    let head_before = ray_at(&world, body, old_height);

    for _ in 0..(CROUCH_DURATION / TICK) as usize + 5 {
        player.advance(&model, TICK);
    }
    let crouched = head_volume(humanoid.bone(&player.pose(&model), Bone::Head));
    world
        .set_hit_volumes(character, &[crouched])
        .expect("character is live");
    world.step(TICK);
    let head_after_at_old_height = ray_at(&world, body, old_height);
    let head_after_at_new_height = ray_at(&world, body, crouched.offset.y);

    let drop = standing.offset.y - crouched.offset.y;
    assert!((drop - CROUCH_DROP).abs() < 1e-4, "dropped {drop}");
    assert!((standing.offset.y - HEAD_REST_Y).abs() < 1e-4);
    assert_eq!(standing.offset.x, crouched.offset.x);
    assert_eq!(standing.offset.z, crouched.offset.z);
    let hit = head_before.expect("a ray at the standing head height hits it");
    assert_eq!(hit.tag, HEAD);
    assert!((hit.distance - 2.9).abs() < 0.02, "{hit:?}");
    assert_eq!(head_after_at_old_height, None);
    assert_eq!(
        head_after_at_new_height.map(|hit| hit.tag),
        Some(HEAD),
        "the volume is at the crouched head height"
    );
}

#[test]
fn drawn_palette_is_the_pose_the_sim_read() {
    let (_dir, model) = rig();
    let humanoid = humanoid(&model);
    let sim = player_at(&model, "walk", Playback::Loop, 30);
    let sim_pose = sim.pose(&model);
    let frame = sim.clone();

    let drawn = model.joint_palette(0, &frame.pose(&model));

    let read = model.joint_palette(0, &sim_pose);
    assert_eq!(model.skins().len(), 1);
    assert_eq!(drawn.len(), Bone::ALL.len());
    assert_eq!(
        drawn.iter().copied().map(bits).collect::<Vec<_>>(),
        read.iter().copied().map(bits).collect::<Vec<_>>()
    );
    for (entry, bone) in drawn.iter().zip(Bone::ALL) {
        near(*entry, humanoid.bone(&sim_pose, bone));
    }
    let rest = model.joint_palette(0, &model.rest_pose());
    assert!(
        !drawn[9].abs_diff_eq(rest[9], 1e-3),
        "hand_r is off its rest at tick 30"
    );
}

#[test]
fn rig_without_head_is_refused_naming_file_and_head() {
    let (_dir, path) = write_humanoid(&omitting(&["head"]));

    let err = load_humanoid(&path).expect_err("no head bone");

    assert_eq!(err.path(), path.as_path());
    let LevelErrorKind::NotHumanoid(missing) = err.kind() else {
        panic!("expected NotHumanoid, got {:?}", err.kind());
    };
    assert_eq!(missing.bones, vec!["head"]);
    let text = err.to_string();
    assert!(text.contains(&path.display().to_string()), "{text}");
    assert!(text.contains("head"), "{text}");
}

#[test]
fn every_missing_bone_is_named_in_bone_order() {
    let (_dir, model) = load(&omitting(&["foot_l", "hand_r", "pelvis", "head"]));

    let err = Humanoid::of(&model).expect_err("four bones are missing");

    assert_eq!(
        err,
        MissingBones {
            bones: vec!["pelvis", "head", "hand_r", "foot_l"]
        }
    );
    assert_eq!(
        err.to_string(),
        "missing humanoid bones: pelvis, head, hand_r, foot_l"
    );
}

#[test]
fn duplicate_name_resolves_to_the_first_in_walk_order() {
    let spec = HumanoidSpec {
        duplicate: vec!["head", "calf_r"],
        ..HumanoidSpec::default()
    };
    let (_dir, model) = load(&spec);

    let humanoid = humanoid(&model);

    assert_eq!(model.nodes()[FIRST_DUPLICATE].name.as_deref(), Some("head"));
    assert_eq!(
        model.nodes()[FIRST_DUPLICATE + 1].name.as_deref(),
        Some("calf_r")
    );
    assert_eq!(humanoid.node(Bone::Head), BONE_NODES[3]);
    assert_eq!(humanoid.node(Bone::CalfR), BONE_NODES[14]);
    let rest = model.rest_pose();
    let head = humanoid.bone(&rest, Bone::Head);
    assert!(
        Vec3::from(head.translation).abs_diff_eq(Vec3::new(0.0, HEAD_REST_Y, 0.0), 1e-4),
        "{head:?}"
    );
    let duplicate = rest.nodes()[FIRST_DUPLICATE];
    assert!(
        Vec3::from(duplicate.translation)
            .abs_diff_eq(Vec3::new(0.0, 1.0, 0.0) + DUPLICATE_OFFSET, 1e-4),
        "the duplicate sits elsewhere, so picking it would show"
    );
}

#[test]
fn extra_nodes_are_ignored() {
    let (_dir, model) = rig();

    let humanoid = humanoid(&model);

    assert_eq!(
        model.nodes()[IK_HAND_GUN].name.as_deref(),
        Some("ik_hand_gun")
    );
    let nodes: Vec<usize> = Bone::ALL.iter().map(|&b| humanoid.node(b)).collect();
    assert_eq!(nodes, BONE_NODES.to_vec());
    for bone in Bone::ALL {
        assert_eq!(
            model.nodes()[humanoid.node(bone)].name.as_deref(),
            Some(bone.name())
        );
    }
}

#[test]
fn a_complete_rig_loads_as_a_humanoid_with_its_model() {
    let (_dir, path) = write_humanoid(&HumanoidSpec::default());

    let (model, humanoid) = load_humanoid(&path).expect("complete rig loads");

    assert_eq!(model.nodes().len(), FIRST_DUPLICATE + 1);
    assert_eq!(humanoid, Humanoid::of(&model).expect("complete rig"));
    assert_eq!(humanoid.node(Bone::HandR), BONE_NODES[9]);
}

#[test]
fn a_file_that_cannot_be_read_fails_as_a_load_error_not_as_not_humanoid() {
    let path = Path::new("no/such/humanoid.gltf");

    let err = load_humanoid(path).expect_err("no such file");

    assert_eq!(err.path(), path);
    assert!(
        matches!(err.kind(), LevelErrorKind::Read(_)),
        "{:?}",
        err.kind()
    );
}
