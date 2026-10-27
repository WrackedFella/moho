use std::path::PathBuf;

use glam::{Affine3A, Mat4, Quat, Vec3};
use moho_level::anim::{ClipPlayer, Playback, Pose};
use moho_level::{LevelError, LevelErrorKind, Model, load_model};
use tempfile::TempDir;

use crate::support::rig::{
    BODY, BONE_A, BONE_B, DEATH, DEATH_DURATION, DEATH_END_DEG, G_ORPHAN, IDLE, IDLE_DURATION,
    MAGAZINE, PROP, ROOT, RigSpec, WALK, WALK_ANGLES_DEG, WALK_DURATION, weights_chan, write_rig,
};

fn load(spec: &RigSpec) -> (TempDir, Model) {
    let (dir, path) = write_rig(spec);
    let model = load_model(&path).expect("rig loads");
    (dir, model)
}

fn rig() -> (TempDir, Model) {
    load(&RigSpec::new())
}

fn load_err(spec: &RigSpec) -> (TempDir, PathBuf, LevelError) {
    let (dir, path) = write_rig(spec);
    let err = load_model(&path).expect_err("rig is refused");
    (dir, path, err)
}

fn t(x: f32, y: f32, z: f32) -> Affine3A {
    Affine3A::from_translation(Vec3::new(x, y, z))
}

fn near(actual: Affine3A, expected: Affine3A) {
    assert!(
        actual.abs_diff_eq(expected, 1e-4),
        "{actual:?} != {expected:?}"
    );
}

fn bits(m: Affine3A) -> [u32; 12] {
    m.to_cols_array().map(f32::to_bits)
}

fn pose_bits(pose: &Pose) -> Vec<[u32; 12]> {
    pose.nodes().iter().copied().map(bits).collect()
}

fn root_node() -> Affine3A {
    t(0.0, 1.0, 0.0)
}

/// `walk`'s `bone_a` and `bone_b` model-space transforms with `bone_a` at z = `z` and
/// `bone_b` turned `degrees` about Z.
fn walk_bones(z: f32, degrees: f32) -> (Affine3A, Affine3A) {
    let bone_a = root_node() * t(1.0, 0.0, z);
    let bone_b = bone_a
        * t(0.0, 2.0, 0.0)
        * Affine3A::from_quat(Quat::from_rotation_z(degrees.to_radians()));
    (bone_a, bone_b)
}

fn walk_player(model: &Model, playback: Playback) -> ClipPlayer {
    ClipPlayer::new(model, "walk", playback).expect("walk exists")
}

#[test]
fn clips_are_found_by_name_with_their_durations() {
    let (_dir, model) = rig();

    let idle = model.clip("idle").expect("idle exists");
    let walk = model.clip("walk").expect("walk exists");
    let death = model.clip("death").expect("death exists");

    assert_eq!(model.clips().len(), 3);
    assert_eq!((idle, walk, death), (IDLE, WALK, DEATH));
    assert_eq!(model.clips()[idle].name(), Some("idle"));
    assert_eq!(model.clips()[walk].name(), Some("walk"));
    assert_eq!(model.clips()[idle].duration(), IDLE_DURATION);
    assert_eq!(model.clips()[walk].duration(), WALK_DURATION);
    assert_eq!(model.clips()[death].duration(), DEATH_DURATION);
    assert_eq!(model.clip("dance"), None);
}

#[test]
fn walk_at_half_its_duration_poses_its_mid_key() {
    let (_dir, model) = rig();
    let mut player = walk_player(&model, Playback::Loop);

    player.advance(&model, WALK_DURATION / 2.0);
    let pose = player.pose(&model);

    let (bone_a, bone_b) = walk_bones(2.0, WALK_ANGLES_DEG[2]);
    assert_eq!(pose.nodes().len(), model.nodes().len());
    near(pose.nodes()[ROOT], root_node());
    near(pose.nodes()[BONE_A], bone_a);
    near(pose.nodes()[BONE_B], bone_b);
    let by_hand = Affine3A::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::new(1.0, 3.0, 2.0),
    );
    near(pose.nodes()[BONE_B], by_hand);
}

#[test]
fn looped_walk_after_one_and_a_quarter_durations_matches_its_quarter_pose() {
    let (_dir, model) = rig();
    let mut player = walk_player(&model, Playback::Loop);

    player.advance(&model, WALK_DURATION * 1.25);
    let pose = player.pose(&model);

    let (bone_a, bone_b) = walk_bones(1.0, WALK_ANGLES_DEG[1]);
    near(pose.nodes()[BONE_A], bone_a);
    near(pose.nodes()[BONE_B], bone_b);
    assert_eq!(pose_bits(&pose), pose_bits(&model.pose(WALK, 1.0)));
}

#[test]
fn death_played_once_holds_its_last_key() {
    let (_dir, model) = rig();
    let mut player = ClipPlayer::new(&model, "death", Playback::Once).expect("death exists");

    player.advance(&model, DEATH_DURATION * 2.0);
    let pose = player.pose(&model);

    let bone_a = root_node()
        * t(1.0, 0.0, 0.0)
        * Affine3A::from_quat(Quat::from_rotation_x(DEATH_END_DEG.to_radians()));
    near(pose.nodes()[BONE_A], bone_a);
    near(pose.nodes()[BONE_B], bone_a * t(0.0, 2.0, 0.0));
    assert_eq!(player.time(), DEATH_DURATION);
    assert_eq!(
        pose_bits(&pose),
        pose_bits(&model.pose(DEATH, DEATH_DURATION))
    );
}

#[test]
fn switching_from_idle_to_walk_poses_walks_first_key() {
    let (_dir, model) = rig();
    let mut player = ClipPlayer::new(&model, "idle", Playback::Loop).expect("idle exists");
    player.advance(&model, 1.5);
    let idle_pose = player.pose(&model);

    player
        .play(&model, "walk", Playback::Loop)
        .expect("walk exists");
    let first = player.pose(&model);
    player.advance(&model, 0.0);
    let still_first = player.pose(&model);

    let (bone_a, bone_b) = walk_bones(0.0, WALK_ANGLES_DEG[0]);
    near(first.nodes()[BONE_A], bone_a);
    near(first.nodes()[BONE_B], bone_b);
    assert_eq!(pose_bits(&first), pose_bits(&model.pose(WALK, 0.0)));
    assert_eq!(pose_bits(&still_first), pose_bits(&first));
    assert_ne!(pose_bits(&first), pose_bits(&idle_pose));
    assert_eq!(player.time(), 0.0);
}

#[test]
fn two_players_on_one_model_pose_their_own_clips() {
    let (_dir, model) = rig();
    let mut idler = ClipPlayer::new(&model, "idle", Playback::Loop).expect("idle exists");
    let mut walker = walk_player(&model, Playback::Loop);

    idler.advance(&model, 1.5);
    walker.advance(&model, 1.5);
    let idle_pose = idler.pose(&model);
    let walk_pose = walker.pose(&model);
    walker.advance(&model, 1.0);
    let idle_after = idler.pose(&model);

    let idle_bone_a = root_node() * t(1.0, 0.5, 0.0);
    near(idle_pose.nodes()[BONE_A], idle_bone_a);
    near(idle_pose.nodes()[BONE_B], idle_bone_a * t(0.0, 2.0, 0.0));
    let (walk_bone_a, walk_bone_b) = walk_bones(1.5, 67.5);
    near(walk_pose.nodes()[BONE_A], walk_bone_a);
    near(walk_pose.nodes()[BONE_B], walk_bone_b);
    assert_eq!(pose_bits(&idle_after), pose_bits(&idle_pose));
}

#[test]
fn the_same_steps_pose_bit_identically_across_two_loads() {
    let steps = [0.1_f32, 0.3, 0.7, 1.3, 0.05, 2.2, 0.0, 0.9];
    let run = || {
        let (_dir, model) = rig();
        let mut player = walk_player(&model, Playback::Loop);
        player.set_speed(1.25);
        steps
            .iter()
            .map(|&dt| {
                player.advance(&model, dt);
                (player.time().to_bits(), pose_bits(&player.pose(&model)))
            })
            .collect::<Vec<_>>()
    };

    let first = run();
    let second = run();

    assert_eq!(first, second);
    assert_ne!(first[0].1, first[5].1, "the poses actually move");
}

#[test]
fn skin_joints_inverse_binds_and_vertex_weights_load() {
    let (_dir, model) = rig();

    let skin = &model.skins()[0];
    let body = &model.meshes()[0];
    let magazine = &model.meshes()[1];

    assert_eq!(model.skins().len(), 1);
    assert_eq!(skin.joints, vec![BONE_A, BONE_B]);
    assert_eq!(skin.inverse_binds.len(), 2);
    near(skin.inverse_binds[0], t(-1.0, -1.0, 0.0));
    near(skin.inverse_binds[1], t(-1.0, -3.0, 0.0));
    assert_eq!(body.node, BODY);
    let mesh_skin = body.skin.as_ref().expect("body is skinned");
    assert_eq!(mesh_skin.skin, 0);
    assert_eq!(
        mesh_skin.joints,
        vec![[0, 0, 0, 0], [0, 1, 0, 0], [1, 0, 0, 0]]
    );
    assert_eq!(
        mesh_skin.weights,
        vec![
            [1.0, 0.0, 0.0, 0.0],
            [0.25, 0.75, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ]
    );
    assert_eq!(magazine.node, MAGAZINE);
    assert!(magazine.skin.is_none());
}

#[test]
fn a_skin_without_inverse_binds_gets_identity() {
    let mut spec = RigSpec::new();
    spec.inverse_binds = None;

    let (_dir, model) = load(&spec);

    assert_eq!(model.skins()[0].inverse_binds, vec![Affine3A::IDENTITY; 2]);
}

#[test]
fn the_rest_poses_joint_palette_is_identity() {
    let (_dir, model) = rig();

    let rest = model.rest_pose();
    let palette = model.joint_palette(0, &rest);

    assert_eq!(rest.nodes().len(), model.nodes().len());
    for (posed, node) in rest.nodes().iter().zip(model.nodes()) {
        assert_eq!(bits(*posed), bits(node.rest));
    }
    assert_eq!(palette.len(), 2);
    for joint in palette {
        near(joint, Affine3A::IDENTITY);
    }
}

#[test]
fn walk_mid_pose_palette_is_pose_times_inverse_bind() {
    let (_dir, model) = rig();
    let pose = model.pose(WALK, 2.0);

    let palette = model.joint_palette(0, &pose);

    assert_eq!(palette.len(), 2);
    near(palette[0], t(0.0, 0.0, 2.0));
    let turned = Affine3A::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::new(4.0, 2.0, 2.0),
    );
    near(palette[1], turned);
}

#[test]
fn a_node_no_channel_targets_keeps_its_local_exactly() {
    let (_dir, model) = rig();

    let walking = model.pose(WALK, 2.0);
    let idling = model.pose(IDLE, 1.5);

    for untouched in [ROOT, BODY, PROP] {
        let rest = bits(model.nodes()[untouched].rest);
        assert_eq!(
            bits(walking.nodes()[untouched]),
            rest,
            "walk, node {untouched}"
        );
        assert_eq!(
            bits(idling.nodes()[untouched]),
            rest,
            "idle, node {untouched}"
        );
    }
    assert_eq!(
        bits(idling.nodes()[MAGAZINE]),
        bits(model.nodes()[MAGAZINE].rest)
    );
    let carried = idling.nodes()[BONE_A] * model.nodes()[BONE_B].local;
    assert_eq!(bits(idling.nodes()[BONE_B]), bits(carried));
    assert_ne!(
        bits(idling.nodes()[BONE_B]),
        bits(model.nodes()[BONE_B].rest)
    );
}

#[test]
fn posed_placements_put_a_rigid_mesh_on_its_animated_node() {
    let (_dir, model) = rig();
    let pose = model.pose(WALK, 1.0);
    let root = t(10.0, 0.0, 0.0);

    let posed: Vec<_> = model.posed_placements(root, &pose).collect();
    let rest: Vec<_> = model.placements(root).collect();

    assert_eq!(posed.len(), 2);
    assert_eq!(posed[0].0, 0);
    assert_eq!(posed[1].0, 1);
    near(posed[0].1, t(10.0, 1.0, 0.0));
    near(posed[1].1, t(10.0, 1.625, 5.0));
    near(rest[1].1, t(10.0, 1.0, 5.0));
}

#[test]
fn weights_are_renormalised_to_sum_one() {
    let mut spec = RigSpec::new();
    spec.vertex_joints[0] = [0, 1, 0, 1];
    spec.vertex_weights = vec![
        [2.0, 1.0, 1.0, 0.0],
        [0.0, 0.0, 3.0, 1.0],
        [1.0, 0.0, 0.0, 0.0],
    ];

    let (_dir, model) = load(&spec);

    let weights = &model.meshes()[0].skin.as_ref().expect("skinned").weights;
    let expected = [
        [0.5, 0.25, 0.25, 0.0],
        [0.0, 0.0, 0.75, 0.25],
        [1.0, 0.0, 0.0, 0.0],
    ];
    for (got, want) in weights.iter().zip(expected) {
        assert!((got.iter().sum::<f32>() - 1.0).abs() < 1e-6, "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-6, "{got:?} != {want:?}");
        }
    }
}

#[test]
fn all_zero_weights_fail_as_bad_mesh() {
    let mut spec = RigSpec::new();
    spec.vertex_weights[1] = [0.0; 4];

    let (_dir, path, err) = load_err(&spec);

    assert!(
        matches!(err.kind(), LevelErrorKind::BadMesh { mesh: 0, .. }),
        "{err}"
    );
    assert_eq!(err.path(), path);
}

#[test]
fn a_vertex_joint_past_the_skin_fails_as_bad_mesh() {
    let mut spec = RigSpec::new();
    spec.vertex_joints[2] = [2, 0, 0, 0];

    let (_dir, path, err) = load_err(&spec);

    assert!(
        matches!(err.kind(), LevelErrorKind::BadMesh { mesh: 0, .. }),
        "{err}"
    );
    assert_eq!(err.path(), path);
}

#[test]
fn a_joint_outside_the_scene_fails_naming_the_path_and_skin() {
    let mut spec = RigSpec::new();
    spec.skin_joints = vec![crate::support::rig::G_BONE_A, G_ORPHAN];

    let (_dir, path, err) = load_err(&spec);

    assert!(
        matches!(err.kind(), LevelErrorKind::BadSkin { skin: 0, .. }),
        "{err}"
    );
    assert_eq!(err.path(), path);
    assert!(err.to_string().starts_with(&path.display().to_string()));
}

#[test]
fn an_inverse_bind_count_mismatch_fails_naming_the_skin() {
    let mut spec = RigSpec::new();
    spec.inverse_binds = Some(vec![Mat4::IDENTITY]);

    let (_dir, path, err) = load_err(&spec);

    assert!(
        matches!(err.kind(), LevelErrorKind::BadSkin { skin: 0, .. }),
        "{err}"
    );
    assert_eq!(err.path(), path);
}

#[test]
fn mismatched_key_and_value_counts_fail_naming_the_animation() {
    let mut spec = RigSpec::new();
    let turns = &mut spec.anims[WALK].channels[1];
    turns.values.truncate(4 * (turns.times.len() - 1));

    let (_dir, path, err) = load_err(&spec);

    assert!(
        matches!(err.kind(), LevelErrorKind::BadAnimation { animation, .. } if *animation == WALK),
        "{err}"
    );
    assert_eq!(err.path(), path);
}

#[test]
fn decreasing_key_times_fail_naming_the_animation() {
    let not_increasing = [vec![3.0, 0.0], vec![0.0, 3.0, 3.0], vec![1.0, 1.0]];

    for times in not_increasing {
        let mut spec = RigSpec::new();
        let turn = &mut spec.anims[DEATH].channels[0];
        turn.values = Quat::IDENTITY.to_array().repeat(times.len());
        turn.times = times.clone();

        let (_dir, path, err) = load_err(&spec);

        assert!(
            matches!(
                err.kind(),
                LevelErrorKind::BadAnimation { animation, reason }
                    if *animation == DEATH && reason.contains("not increasing")
            ),
            "{times:?}: {err}"
        );
        assert_eq!(err.path(), path, "{times:?}");
    }
}

#[test]
fn further_joint_sets_are_ignored_leaving_the_first_sets_data() {
    let (_clean_dir, clean) = rig();
    let mut spec = RigSpec::new();
    spec.further_influences = Some((vec![[1, 1, 1, 1]; 3], vec![[0.5, 0.25, 0.125, 0.125]; 3]));

    let (_dir, model) = load(&spec);

    let got = model.meshes()[0].skin.as_ref().expect("skinned");
    let want = clean.meshes()[0].skin.as_ref().expect("skinned");
    assert_eq!(got.joints, want.joints);
    assert_eq!(got.weights, want.weights);
    assert_eq!(got.skin, want.skin);
}

#[test]
fn an_animation_targeting_a_node_outside_the_scene_fails_naming_the_animation() {
    let mut spec = RigSpec::new();
    spec.anims[IDLE].channels[0].node = G_ORPHAN;

    let (_dir, path, err) = load_err(&spec);

    assert!(
        matches!(err.kind(), LevelErrorKind::BadAnimation { animation, .. } if *animation == IDLE),
        "{err}"
    );
    assert_eq!(err.path(), path);
}

#[test]
fn morph_weight_channels_are_skipped() {
    let (_clean_dir, clean) = rig();
    let mut spec = RigSpec::new();
    spec.anims[IDLE].channels.push(weights_chan());

    let (_dir, model) = load(&spec);

    assert_eq!(model.clips().len(), 3);
    assert_eq!(model.clips()[IDLE].duration(), IDLE_DURATION);
    assert_eq!(
        pose_bits(&model.pose(IDLE, 1.5)),
        pose_bits(&clean.pose(IDLE, 1.5))
    );
}
