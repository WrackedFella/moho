//! A sim-timed action clip over locomotion, driven the way a sim ticks: `advance` once per
//! tick before that tick's commands.

use std::num::NonZeroU32;

use glam::Affine3A;
use glam::{Mat3A, Quat, Vec3};
use moho_level::Assembly;
use moho_level::anim::AimLink;
use moho_level::anim::{Animator, ClipError, ClipPlayer, Playback, Pose, Region};
use moho_level::humanoid::{Bone, Humanoid};
use moho_level::{Model, load_model};
use moho_physics::{HitTag, HitVolume};
use tempfile::TempDir;

use crate::support::humanoid::{HumanoidSpec, RELOAD_DURATION, write_humanoid};

const TICK: f32 = 1.0 / 60.0;

/// A reload of 2.6 s at 60 Hz.
const RELOAD_TICKS: u32 = 156;

const START_TICK: usize = 100;

fn load() -> (TempDir, Model) {
    let spec = HumanoidSpec {
        reload: true,
        ..HumanoidSpec::default()
    };
    let (dir, path) = write_humanoid(&spec);
    let model = load_model(&path).expect("humanoid loads");
    (dir, model)
}

fn walker(model: &Model) -> Animator {
    Animator::new(ClipPlayer::new(model, "walk", Playback::Loop).expect("walk exists"))
}

fn ticks(n: u32) -> Playback {
    Playback::Ticks(NonZeroU32::new(n).expect("n is positive"))
}

/// The animator after each tick's advance and commands, from tick 0 to `last_tick`.
fn simulate(
    model: &Model,
    reload: Option<Playback>,
    cancel_at: Option<usize>,
    last_tick: usize,
) -> Vec<Animator> {
    let mut animator = walker(model);
    let mut states = Vec::new();
    for tick in 0..=last_tick {
        animator.advance(model, TICK);
        if tick == START_TICK
            && let Some(playback) = reload
        {
            animator
                .play_action(model, "reload", playback, Region::Whole)
                .expect("reload exists");
        }
        if Some(tick) == cancel_at {
            animator.cancel_action();
        }
        states.push(animator.clone());
    }
    states
}

fn pose_bits(pose: &Pose) -> Vec<[u32; 12]> {
    pose.nodes()
        .iter()
        .map(|m: &Affine3A| m.to_cols_array().map(f32::to_bits))
        .collect()
}

#[test]
fn reload_for_156_ticks_reaches_its_last_pose_on_tick_256_and_not_before() {
    let (_dir, model) = load();
    let reload = model.clip("reload").expect("reload exists");
    let duration = model.clips()[reload].duration();
    let last = pose_bits(&model.pose(reload, duration));

    let states = simulate(&model, Some(ticks(RELOAD_TICKS)), None, 256);

    assert_eq!(duration, RELOAD_DURATION);
    assert!(states[START_TICK - 1].action().is_none());
    assert_eq!(states[START_TICK].action().expect("started").time(), 0.0);
    let before = &states[255];
    let action = before.action().expect("still playing at tick 255");
    assert!(action.time() < duration, "{}", action.time());
    assert!(!action.is_finished(&model));
    assert_ne!(pose_bits(&before.pose(&model)), last);
    let end = &states[256];
    let action = end.action().expect("still held at tick 256");
    assert_eq!(action.time(), duration);
    assert!(action.is_finished(&model));
    assert_eq!(pose_bits(&end.pose(&model)), last);
}

#[test]
fn two_runs_give_bit_equal_poses_at_tick_130() {
    let (_first_dir, first) = load();
    let (_second_dir, second) = load();

    let a = simulate(&first, Some(ticks(RELOAD_TICKS)), None, 130);
    let b = simulate(&second, Some(ticks(RELOAD_TICKS)), None, 130);

    let (a, b) = (a[130].pose(&first), b[130].pose(&second));
    assert_eq!(pose_bits(&a), pose_bits(&b));
    let never = simulate(&first, None, None, 130);
    assert_ne!(pose_bits(&a), pose_bits(&never[130].pose(&first)));
}

#[test]
fn cancel_at_tick_120_gives_the_pose_of_a_character_that_never_reloaded() {
    let (_dir, model) = load();

    let cancelled = simulate(&model, Some(ticks(RELOAD_TICKS)), Some(120), 180);
    let never = simulate(&model, None, None, 180);

    assert_ne!(
        pose_bits(&cancelled[119].pose(&model)),
        pose_bits(&never[119].pose(&model)),
    );
    for tick in 120..180 {
        assert!(cancelled[tick].action().is_none(), "tick {tick}");
        assert_eq!(
            pose_bits(&cancelled[tick].pose(&model)),
            pose_bits(&never[tick].pose(&model)),
            "tick {tick}"
        );
    }
}

#[test]
fn finished_action_is_dropped_on_the_next_advance() {
    let (_dir, model) = load();

    let states = simulate(&model, Some(ticks(RELOAD_TICKS)), None, 257);
    let never = simulate(&model, None, None, 257);

    assert!(states[256].action().is_some());
    assert!(states[257].action().is_none());
    assert_eq!(
        pose_bits(&states[257].pose(&model)),
        pose_bits(&never[257].pose(&model)),
    );
}

#[test]
fn play_action_replaces_a_running_action() {
    let (_dir, model) = load();
    let duration = model.clips()[model.clip("reload").expect("reload exists")].duration();
    let mut animator = walker(&model);
    let mut states = Vec::new();

    for tick in 0..=286 {
        animator.advance(&model, TICK);
        if tick == START_TICK || tick == 130 {
            animator
                .play_action(&model, "reload", ticks(RELOAD_TICKS), Region::Whole)
                .expect("reload exists");
        }
        states.push(animator.clone());
    }

    assert_eq!(states[130].action().expect("restarted").time(), 0.0);
    for (tick, state) in states.iter().enumerate().take(286).skip(131) {
        let action = state.action().expect("still playing");
        assert!(!action.is_finished(&model), "tick {tick}");
        assert!(action.time() < duration, "tick {tick}");
    }
    let end = states[286].action().expect("held on its last tick");
    assert_eq!(end.time(), duration);
    assert!(end.is_finished(&model));
}

#[test]
fn looping_action_never_ends_until_cancelled() {
    let (_dir, model) = load();

    let states = simulate(&model, Some(Playback::Loop), Some(1000), 1000);
    let never = simulate(&model, None, None, 1000);

    for (tick, state) in states.iter().enumerate().take(1000).skip(START_TICK) {
        let action = state.action().expect("still looping");
        assert!(!action.is_finished(&model), "tick {tick}");
    }
    assert!(states[1000].action().is_none());
    assert_eq!(
        pose_bits(&states[1000].pose(&model)),
        pose_bits(&never[1000].pose(&model)),
    );
}

#[test]
fn unknown_action_clip_changes_nothing() {
    let (_dir, model) = load();
    let idle = walker(&model);
    let mut playing = simulate(&model, Some(ticks(RELOAD_TICKS)), None, 130).remove(130);
    let mut not_playing = idle.clone();
    let playing_before = playing.clone();

    let from_playing = playing.play_action(&model, "dance", Playback::Once, Region::Whole);
    let from_idle = not_playing.play_action(&model, "dance", Playback::Once, Region::Whole);

    let expected = ClipError::UnknownClip {
        name: "dance".into(),
    };
    assert_eq!(from_playing, Err(expected));
    assert_eq!(
        from_idle,
        Err(ClipError::UnknownClip {
            name: "dance".into()
        })
    );
    assert_eq!(playing, playing_before);
    assert_eq!(not_playing, idle);
}

/// Walk index of `spine_01`, the root of the upper body.
const SPINE_01: usize = 1;

/// Walk indices of the upper body: `spine_01` and every node below it.
const UPPER_BODY: std::ops::RangeInclusive<usize> = 1..=10;

/// Walk indices of `pelvis` and the legs.
const LOWER_BODY: [usize; 7] = [0, 11, 12, 13, 14, 15, 16];

fn load_with_idle() -> (TempDir, Model) {
    let spec = HumanoidSpec {
        reload: true,
        idle: true,
        ..HumanoidSpec::default()
    };
    let (dir, path) = write_humanoid(&spec);
    let model = load_model(&path).expect("humanoid loads");
    (dir, model)
}

fn looping(model: &Model, clip: &str) -> Animator {
    Animator::new(ClipPlayer::new(model, clip, Playback::Loop).expect("clip exists"))
}

/// The animator on `base` after each tick's advance and commands, from tick 0 to `last_tick`.
/// `reload` starts on tick `start` over `region` and is cancelled on tick `cancel_at`.
fn simulate_over(
    model: &Model,
    base: &str,
    reload: Option<(Playback, Region)>,
    start: usize,
    cancel_at: Option<usize>,
    last_tick: usize,
) -> Vec<Animator> {
    let mut animator = looping(model, base);
    let mut states = Vec::new();
    for tick in 0..=last_tick {
        animator.advance(model, TICK);
        if tick == start
            && let Some((playback, region)) = reload
        {
            animator
                .play_action(model, "reload", playback, region)
                .expect("reload exists");
        }
        if Some(tick) == cancel_at {
            animator.cancel_action();
        }
        states.push(animator.clone());
    }
    states
}

fn upper_body() -> Region {
    Region::From(SPINE_01)
}

fn assert_nodes_bit_equal(actual: &Pose, expected: &Pose, nodes: &[usize], context: &str) {
    for &node in nodes {
        assert_eq!(
            actual.nodes()[node].to_cols_array().map(f32::to_bits),
            expected.nodes()[node].to_cols_array().map(f32::to_bits),
            "{context}: node {node}"
        );
    }
}

#[test]
fn reload_from_spine_01_over_walk_moves_the_upper_body_and_not_the_legs() {
    let (_dir, model) = load_with_idle();
    let reload = model.clip("reload").expect("reload exists");

    let layered = simulate_over(
        &model,
        "walk",
        Some((ticks(RELOAD_TICKS), upper_body())),
        START_TICK,
        None,
        130,
    );
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 130);

    let state = &layered[130];
    let pose = state.pose(&model);
    let walk = walking[130].pose(&model);
    let time = state.action().expect("reload is playing").time();
    let reload_pose = model.pose(reload, time);
    assert_nodes_bit_equal(&pose, &walk, &LOWER_BODY, "legs follow walk");
    for node in UPPER_BODY {
        let parent = model.nodes()[node].parent.expect("upper body has parents");
        let local = reload_pose.nodes()[parent].inverse() * reload_pose.nodes()[node];
        let expected = pose.nodes()[parent] * local;
        assert!(
            pose.nodes()[node].abs_diff_eq(expected, 1e-5),
            "node {node}: {:?} != {expected:?}",
            pose.nodes()[node]
        );
    }
    let spine_parent = model.nodes()[SPINE_01].parent.expect("spine has a parent");
    assert_eq!(spine_parent, 0);
    assert_ne!(
        reload_pose.nodes()[0],
        walk.nodes()[0],
        "the action's pelvis differs from the base's"
    );
    let spine_local = reload_pose.nodes()[0].inverse() * reload_pose.nodes()[SPINE_01];
    assert!(
        pose.nodes()[SPINE_01].abs_diff_eq(walk.nodes()[0] * spine_local, 1e-5),
        "the upper body rides the base's pelvis"
    );
    assert_ne!(pose.nodes()[SPINE_01], walk.nodes()[SPINE_01]);
    assert_ne!(pose.nodes()[9], walk.nodes()[9]);
}

#[test]
fn upper_body_follows_idle_again_after_reload_ends() {
    let (_dir, model) = load_with_idle();
    let reload = Some((ticks(RELOAD_TICKS), upper_body()));

    let states = simulate_over(&model, "idle", reload, START_TICK, None, 260);
    let never = simulate_over(&model, "idle", None, START_TICK, None, 260);

    let all: Vec<usize> = (0..model.nodes().len()).collect();
    assert_ne!(
        states[256].pose(&model).nodes()[SPINE_01],
        never[256].pose(&model).nodes()[SPINE_01],
        "the layer still holds its last pose on tick 256"
    );
    for tick in 257..=260 {
        assert!(states[tick].action().is_none(), "tick {tick}");
        assert_nodes_bit_equal(
            &states[tick].pose(&model),
            &never[tick].pose(&model),
            &all,
            &format!("tick {tick}"),
        );
    }
}

#[test]
fn upper_body_follows_idle_again_after_reload_is_cancelled() {
    let (_dir, model) = load_with_idle();
    let reload = Some((ticks(RELOAD_TICKS), upper_body()));

    let cancelled = simulate_over(&model, "idle", reload, START_TICK, Some(120), 180);
    let never = simulate_over(&model, "idle", None, START_TICK, None, 180);

    let all: Vec<usize> = (0..model.nodes().len()).collect();
    assert_ne!(
        cancelled[119].pose(&model).nodes()[SPINE_01],
        never[119].pose(&model).nodes()[SPINE_01],
    );
    for tick in 120..=180 {
        assert_nodes_bit_equal(
            &cancelled[tick].pose(&model),
            &never[tick].pose(&model),
            &all,
            &format!("tick {tick}"),
        );
    }
}

#[test]
fn two_runs_of_reload_over_walk_are_bit_equal_at_tick_45() {
    let (_first_dir, first) = load_with_idle();
    let (_second_dir, second) = load_with_idle();
    let reload = Some((ticks(RELOAD_TICKS), upper_body()));

    let a = simulate_over(&first, "walk", reload, 0, None, 45);
    let b = simulate_over(&second, "walk", reload, 0, None, 45);

    let (a, b) = (a[45].pose(&first), b[45].pose(&second));
    assert_eq!(pose_bits(&a), pose_bits(&b));
    let never = simulate_over(&first, "walk", None, 0, None, 45);
    assert_ne!(pose_bits(&a), pose_bits(&never[45].pose(&first)));
    assert_nodes_bit_equal(&a, &never[45].pose(&first), &LOWER_BODY, "legs follow walk");
}

#[test]
fn leg_channels_in_the_action_are_ignored() {
    let (_dir, model) = load_with_idle();
    let reload = model.clip("reload").expect("reload exists");
    let thigh_l = 11;

    let layered = simulate_over(
        &model,
        "idle",
        Some((ticks(RELOAD_TICKS), upper_body())),
        START_TICK,
        None,
        150,
    );
    let idling = simulate_over(&model, "idle", None, START_TICK, None, 150);

    for tick in START_TICK..=150 {
        assert_nodes_bit_equal(
            &layered[tick].pose(&model),
            &idling[tick].pose(&model),
            &LOWER_BODY,
            &format!("tick {tick}"),
        );
    }
    let time = layered[150].action().expect("playing").time();
    assert_ne!(
        model.pose(reload, time).nodes()[thigh_l],
        idling[150].pose(&model).nodes()[thigh_l],
        "the action's leg channel moves the leg, so ignoring it is observable"
    );
}

#[test]
fn switching_the_base_under_an_action_keeps_the_action() {
    let (_dir, model) = load_with_idle();
    let reload = model.clip("reload").expect("reload exists");
    let idle = model.clip("idle").expect("idle exists");
    let mut states = simulate_over(
        &model,
        "walk",
        Some((ticks(RELOAD_TICKS), upper_body())),
        START_TICK,
        None,
        130,
    );
    let mut animator = states.remove(130);
    let action_before = animator.action().expect("playing").clone();

    animator
        .base_mut()
        .play(&model, "idle", Playback::Loop)
        .expect("idle exists");

    assert_eq!(animator.action(), Some(&action_before));
    let expected = model.layered_pose((idle, 0.0), (reload, action_before.time()), SPINE_01);
    assert_eq!(pose_bits(&animator.pose(&model)), pose_bits(&expected));
    let action_pose = model.pose(reload, action_before.time());
    let base_pose = model.pose(idle, 0.0);
    assert_ne!(action_pose.nodes()[0], base_pose.nodes()[0]);
    let spine_local = action_pose.nodes()[0].inverse() * action_pose.nodes()[SPINE_01];
    assert!(
        animator.pose(&model).nodes()[SPINE_01]
            .abs_diff_eq(base_pose.nodes()[0] * spine_local, 1e-5),
        "the upper body rides the new base's pelvis"
    );
    assert_nodes_bit_equal(
        &animator.pose(&model),
        &model.pose(idle, 0.0),
        &LOWER_BODY,
        "legs follow the new base",
    );
}

/// A walker advanced `ticks` ticks, with `reload` just started over `region`.
fn walker_reloading(model: &Model, region: Region, ticks: usize) -> Animator {
    let mut animator = walker(model);
    for _ in 0..ticks {
        animator.advance(model, TICK);
    }
    animator
        .play_action(model, "reload", Playback::Loop, region)
        .expect("reload exists");
    animator
}

#[test]
fn playing_a_whole_action_replaces_an_upper_body_action() {
    let (_dir, model) = load_with_idle();
    let reload = model.clip("reload").expect("reload exists");
    let mut animator = walker_reloading(&model, upper_body(), 30);
    let layered = pose_bits(&animator.pose(&model));

    animator
        .play_action(&model, "reload", Playback::Loop, Region::Whole)
        .expect("reload exists");
    animator.advance(&model, TICK);

    let time = animator.action().expect("playing").time();
    assert_eq!(
        pose_bits(&animator.pose(&model)),
        pose_bits(&model.pose(reload, time))
    );
    assert_ne!(pose_bits(&animator.pose(&model)), layered);
    let upper_only = model.layered_pose(
        (animator.base().clip(), animator.base().time()),
        (reload, time),
        SPINE_01,
    );
    assert_ne!(pose_bits(&animator.pose(&model)), pose_bits(&upper_only));
}

#[test]
fn playing_an_upper_body_action_replaces_a_whole_action() {
    let (_dir, model) = load_with_idle();
    let reload = model.clip("reload").expect("reload exists");
    let mut animator = walker_reloading(&model, Region::Whole, 30);

    animator
        .play_action(&model, "reload", Playback::Loop, upper_body())
        .expect("reload exists");
    animator.advance(&model, TICK);

    let time = animator.action().expect("playing").time();
    let base = (animator.base().clip(), animator.base().time());
    let expected = model.layered_pose(base, (reload, time), SPINE_01);
    assert_eq!(pose_bits(&animator.pose(&model)), pose_bits(&expected));
    assert_ne!(
        pose_bits(&animator.pose(&model)),
        pose_bits(&model.pose(reload, time))
    );
}

#[test]
fn cancelling_an_upper_body_action_returns_the_base_only_pose() {
    let (_dir, model) = load_with_idle();
    let mut animator = walker_reloading(&model, upper_body(), 30);
    let walking = simulate_over(&model, "walk", None, 0, None, 29).remove(29);
    assert_ne!(
        pose_bits(&animator.pose(&model)),
        pose_bits(&walking.pose(&model))
    );

    animator.cancel_action();

    assert!(animator.action().is_none());
    assert_eq!(
        pose_bits(&animator.pose(&model)),
        pose_bits(&walking.pose(&model))
    );
}

fn degrees(angle: f32) -> f32 {
    angle.to_radians()
}

/// A rig with the chain `spine_01`, `spine_02` and `spine_03` under the pelvis, and `reload`
/// and `idle` as clips.
fn load_aimer() -> (TempDir, Model) {
    let spec = HumanoidSpec {
        reload: true,
        idle: true,
        extra_spine: 2,
        ..HumanoidSpec::default()
    };
    let (dir, path) = write_humanoid(&spec);
    let model = load_model(&path).expect("humanoid loads");
    (dir, model)
}

fn node_named(model: &Model, name: &str) -> usize {
    model
        .nodes()
        .iter()
        .position(|n| n.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named {name}"))
}

fn links(model: &Model, shares: &[(&str, f32)]) -> Vec<AimLink> {
    shares
        .iter()
        .map(|&(name, share)| AimLink {
            node: node_named(model, name),
            share,
        })
        .collect()
}

fn three_bone_chain(model: &Model) -> Vec<AimLink> {
    let third = 1.0 / 3.0;
    links(
        model,
        &[
            ("spine_01", third),
            ("spine_02", third),
            ("spine_03", third),
        ],
    )
}

/// Whether `node` is a strict descendant of `ancestor`.
fn is_below(model: &Model, node: usize, ancestor: usize) -> bool {
    let mut current = model.nodes()[node].parent;
    while let Some(parent) = current {
        if parent == ancestor {
            return true;
        }
        current = model.nodes()[parent].parent;
    }
    false
}

/// Every node strictly below `ancestor`.
fn below(model: &Model, ancestor: usize) -> Vec<usize> {
    (0..model.nodes().len())
        .filter(|&node| is_below(model, node, ancestor))
        .collect()
}

/// Every node that is neither `spine_01` nor below it: the pelvis, the legs and loose roots.
fn lower_body(model: &Model) -> Vec<usize> {
    (0..model.nodes().len())
        .filter(|&node| node != SPINE_01 && !is_below(model, node, SPINE_01))
        .collect()
}

fn aiming(animator: &Animator, chain: Vec<AimLink>, pitch: f32) -> Animator {
    let mut aiming = animator.clone();
    aiming.set_aim_chain(chain);
    aiming.set_aim_pitch(pitch);
    aiming
}

/// The one rigid transform that takes every one of `nodes` from `before` to `after`.
fn shared_motion(after: &Pose, before: &Pose, nodes: &[usize]) -> Affine3A {
    let motion_of = |node: usize| after.nodes()[node] * before.nodes()[node].inverse();
    let motion = motion_of(nodes[0]);
    for &node in nodes {
        assert!(
            motion_of(node).abs_diff_eq(motion, 1e-4),
            "node {node}: {:?} != {motion:?}",
            motion_of(node)
        );
    }
    assert!(
        !motion.abs_diff_eq(Affine3A::IDENTITY, 1e-3),
        "the nodes did not move"
    );
    motion
}

fn assert_rotation(motion: Affine3A, expected: Quat) {
    let want = Mat3A::from_quat(expected);
    assert!(
        motion.matrix3.abs_diff_eq(want, 1e-5),
        "{:?} != {want:?}",
        motion.matrix3
    );
}

/// `node`'s transform in the frame of its parent's posed global.
fn relative(pose: &Pose, model: &Model, node: usize) -> Affine3A {
    let parent = model.nodes()[node].parent.expect("node has a parent");
    pose.nodes()[parent].inverse() * pose.nodes()[node]
}

/// `name` sits in `pitched` as in `walk`, turned by `angle` about model X relative to its parent.
fn assert_turned_relative_to_parent(
    model: &Model,
    pitched: &Pose,
    walk: &Pose,
    name: &str,
    angle: f32,
) {
    let node = node_named(model, name);
    let want = Mat3A::from_rotation_x(angle) * relative(walk, model, node).matrix3;
    let got = relative(pitched, model, node).matrix3;
    assert!(got.abs_diff_eq(want, 1e-5), "{name}: {got:?} != {want:?}");
}

#[test]
fn pitch_30_up_turns_everything_above_a_three_bone_chain_and_not_the_legs() {
    let (_dir, model) = load_aimer();
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 130).remove(130);
    let pitching = aiming(&walking, three_bone_chain(&model), degrees(30.0));
    let above = below(&model, node_named(&model, "spine_03"));

    let pitched = pitching.pose(&model);
    let walk = walking.pose(&model);

    assert_nodes_bit_equal(&pitched, &walk, &lower_body(&model), "legs keep walk");
    assert_eq!(above.len(), 9, "neck, head, both arms and hands, the gun");
    let motion = shared_motion(&pitched, &walk, &above);
    assert_rotation(motion, Quat::from_rotation_x(-degrees(30.0)));
    let models = |_key: usize| &model;
    let mut character = Assembly::new(0_usize);
    character
        .attach(
            &models,
            "hand_r",
            Assembly::new(0_usize),
            Affine3A::IDENTITY,
        )
        .expect("hand_r exists");
    let rifle_before = character.posed_placements(&models, Affine3A::IDENTITY, &walk);
    let rifle_after = character.posed_placements(&models, Affine3A::IDENTITY, &pitched);
    assert_eq!(
        rifle_after.len(),
        4,
        "the body's two meshes and the rifle's two"
    );
    for (before, after) in rifle_before.iter().zip(&rifle_after).skip(2) {
        let moved = after.2 * before.2.inverse();
        assert!(moved.abs_diff_eq(motion, 1e-4), "{moved:?} != {motion:?}");
    }
}

#[test]
fn pitch_20_down_over_reload_on_idle_turns_the_reload_pose_and_not_the_legs() {
    let (_dir, model) = load_aimer();
    let reload = Some((ticks(RELOAD_TICKS), upper_body()));
    let reloading = simulate_over(&model, "idle", reload, START_TICK, None, 130).remove(130);
    let idling = simulate_over(&model, "idle", None, START_TICK, None, 130).remove(130);
    let pitching = aiming(&reloading, three_bone_chain(&model), degrees(-20.0));
    let above = below(&model, node_named(&model, "spine_03"));

    let pitched = pitching.pose(&model);
    let reload_pose = reloading.pose(&model);
    let idle = idling.pose(&model);

    assert_ne!(
        reload_pose.nodes()[SPINE_01],
        idle.nodes()[SPINE_01],
        "the reload layer is playing"
    );
    assert_nodes_bit_equal(&pitched, &idle, &lower_body(&model), "legs follow idle");
    let motion = shared_motion(&pitched, &reload_pose, &above);
    assert_rotation(motion, Quat::from_rotation_x(degrees(20.0)));
}

const HEAD: HitTag = HitTag(10);

fn head_volume(head: Affine3A) -> HitVolume {
    HitVolume {
        offset: Vec3::from(head.translation),
        rotation: Quat::from_mat3a(&head.matrix3),
        half_extents: Vec3::splat(0.1),
        tag: HEAD,
    }
}

/// Where the head of a rig at rest ends up when each link turns its node by its share of
/// `pitch` (positive up) about the model's X axis, working down the chain from the pelvis.
fn pitched_head(model: &Model, chain: &[AimLink], pitch: f32) -> Vec3 {
    let rest_of = |node: usize| Vec3::from(model.nodes()[node].rest.translation);
    let mut origin = rest_of(node_named(model, "pelvis"));
    let mut rest_origin = origin;
    let mut rotation = Quat::IDENTITY;
    for link in chain {
        let rest = rest_of(link.node);
        origin += rotation * (rest - rest_origin);
        rotation = Quat::from_rotation_x(-pitch * link.share) * rotation;
        rest_origin = rest;
    }
    origin + rotation * (rest_of(node_named(model, "head")) - rest_origin)
}

#[test]
fn head_hit_volume_follows_the_pitched_head() {
    let (_dir, model) = load_aimer();
    let humanoid = Humanoid::of(&model).expect("the fixture has every bone");
    let chain = three_bone_chain(&model);
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 30).remove(30);
    let pitching = aiming(&walking, chain.clone(), degrees(30.0));

    let flat = head_volume(humanoid.bone(&walking.pose(&model), Bone::Head));
    let raised = head_volume(humanoid.bone(&pitching.pose(&model), Bone::Head));

    let expected = pitched_head(&model, &chain, degrees(30.0));
    assert!(
        raised.offset.abs_diff_eq(expected, 1e-4),
        "{:?} != {expected:?}",
        raised.offset
    );
    assert_ne!(raised.offset, flat.offset);
    assert_ne!(raised.rotation, flat.rotation);
}

/// Pitches the sim sets, by tick.
const PITCHES: [(usize, f32); 4] = [(10, 0.1), (60, -0.3), (90, 0.5), (125, 0.2)];

fn pitched_run(model: &Model, pitching: bool) -> Animator {
    let mut animator = walker(model);
    if pitching {
        animator.set_aim_chain(three_bone_chain(model));
    }
    for tick in 0..=130 {
        animator.advance(model, TICK);
        if tick == START_TICK {
            animator
                .play_action(model, "reload", ticks(RELOAD_TICKS), upper_body())
                .expect("reload exists");
        }
        if pitching && let Some(&(_, radians)) = PITCHES.iter().find(|(at, _)| *at == tick) {
            animator.set_aim_pitch(radians);
        }
    }
    animator
}

#[test]
fn two_runs_with_the_same_pitches_are_bit_equal_at_tick_130() {
    let (_first_dir, first) = load_aimer();
    let (_second_dir, second) = load_aimer();

    let a = pitched_run(&first, true).pose(&first);
    let b = pitched_run(&second, true).pose(&second);

    assert_eq!(pose_bits(&a), pose_bits(&b));
    let flat = pitched_run(&first, false).pose(&first);
    assert_ne!(pose_bits(&a), pose_bits(&flat));
}

#[test]
fn a_one_bone_chain_turns_spine_01_by_the_whole_pitch() {
    let (_dir, model) = load_aimer();
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 30).remove(30);
    let pitching = aiming(&walking, links(&model, &[("spine_01", 1.0)]), degrees(30.0));

    assert_turned_relative_to_parent(
        &model,
        &pitching.pose(&model),
        &walking.pose(&model),
        "spine_01",
        -degrees(30.0),
    );
}

#[test]
fn a_three_bone_chain_turns_spine_02_by_a_third() {
    let (_dir, model) = load_aimer();
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 30).remove(30);
    let pitching = aiming(&walking, three_bone_chain(&model), degrees(30.0));

    let (pitched, walk) = (pitching.pose(&model), walking.pose(&model));

    for name in ["spine_01", "spine_02", "spine_03"] {
        assert_turned_relative_to_parent(&model, &pitched, &walk, name, -degrees(10.0));
    }
}

#[test]
fn shares_need_not_sum_to_one() {
    let (_dir, model) = load_aimer();
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 30).remove(30);
    let chain = links(&model, &[("spine_01", 0.5), ("spine_02", 0.25)]);
    let pitching = aiming(&walking, chain, degrees(30.0));
    let above = below(&model, node_named(&model, "spine_02"));

    let motion = shared_motion(&pitching.pose(&model), &walking.pose(&model), &above);

    assert_rotation(motion, Quat::from_rotation_x(-degrees(22.5)));
}

fn bits_of(animator: &Animator, model: &Model) -> Vec<[u32; 12]> {
    pose_bits(&animator.pose(model))
}

#[test]
fn empty_chain_or_zero_pitch_is_bit_equal_to_the_unpitched_pose() {
    let (_dir, model) = load_aimer();
    let reload = Some((ticks(RELOAD_TICKS), upper_body()));
    let state = simulate_over(&model, "idle", reload, START_TICK, None, 130).remove(130);
    let plain = bits_of(&state, &model);
    let chain = three_bone_chain(&model);
    let chain_only = aiming(&state, chain.clone(), 0.0);
    let pitch_only = aiming(&state, Vec::new(), degrees(30.0));
    let mut pitch_cleared = aiming(&state, chain.clone(), degrees(30.0));
    pitch_cleared.set_aim_pitch(0.0);
    let mut chain_emptied = aiming(&state, chain.clone(), degrees(30.0));
    chain_emptied.set_aim_chain(Vec::<AimLink>::new());
    let no_shares = aiming(&state, links(&model, &[("spine_01", 0.0)]), degrees(30.0));

    let turned = aiming(&state, chain, degrees(30.0));

    for (what, animator) in [
        ("chain without pitch", &chain_only),
        ("pitch without chain", &pitch_only),
        ("pitch set back to zero", &pitch_cleared),
        ("chain emptied", &chain_emptied),
        ("links with no share", &no_shares),
    ] {
        assert_eq!(bits_of(animator, &model), plain, "{what}");
    }
    assert_ne!(bits_of(&turned, &model), plain, "the control turns");
}

#[test]
fn positive_pitch_raises_the_front() {
    let (_dir, model) = load_aimer();
    let humanoid = Humanoid::of(&model).expect("the fixture has every bone");
    let walking = simulate_over(&model, "walk", None, START_TICK, None, 30).remove(30);
    let pitching = aiming(&walking, three_bone_chain(&model), degrees(30.0));

    let ahead = |animator: &Animator| {
        humanoid
            .bone(&animator.pose(&model), Bone::Head)
            .transform_vector3(Vec3::Z)
    };

    let (level, raised) = (ahead(&walking), ahead(&pitching));

    assert!(level.y.abs() < 1e-5, "{level:?}");
    assert!((raised.y - degrees(30.0).sin()).abs() < 1e-4, "{raised:?}");
    assert!(raised.z < level.z, "the front also comes back: {raised:?}");
}

#[test]
fn pitch_holds_across_advances_until_set_again() {
    let (_dir, model) = load_aimer();
    let chain = three_bone_chain(&model);
    let mut held = aiming(&walker(&model), chain.clone(), 0.4);
    let mut plain = walker(&model);

    for _ in 0..5 {
        held.advance(&model, TICK);
        plain.advance(&model, TICK);
    }

    assert_eq!(held.aim_pitch(), 0.4);
    let set_late = aiming(&plain, chain, 0.4);
    assert_eq!(bits_of(&held, &model), bits_of(&set_late, &model));
    assert_ne!(bits_of(&held, &model), bits_of(&plain, &model));
    held.set_aim_pitch(0.0);
    assert_eq!(held.aim_pitch(), 0.0);
    assert_eq!(bits_of(&held, &model), bits_of(&plain, &model));
}

#[test]
fn a_new_chain_replaces_the_old_one() {
    let (_dir, model) = load_aimer();
    let walking = walker(&model);
    let one_bone = links(&model, &[("spine_01", 1.0)]);
    let mut replaced = aiming(&walking, three_bone_chain(&model), degrees(30.0));
    replaced.set_aim_chain(one_bone.clone());
    let fresh = aiming(&walking, one_bone, degrees(30.0));

    assert_eq!(bits_of(&replaced, &model), bits_of(&fresh, &model));
    assert_ne!(bits_of(&replaced, &model), bits_of(&walking, &model));
}

#[test]
fn animators_with_different_aim_state_are_not_equal() {
    let (_dir, model) = load_aimer();
    let walking = walker(&model);
    let chain = three_bone_chain(&model);

    let pitched = aiming(&walking, chain.clone(), 0.3);

    assert_eq!(pitched, aiming(&walking, chain.clone(), 0.3));
    assert_ne!(pitched, aiming(&walking, chain.clone(), 0.4));
    assert_ne!(pitched, aiming(&walking, Vec::new(), 0.3));
    assert_ne!(pitched, aiming(&walking, chain[..2].to_vec(), 0.3));
}

#[test]
fn pitch_turns_about_the_model_x_axis_even_when_a_chain_bone_is_rotated_by_the_clip() {
    let spec = HumanoidSpec {
        extra_spine: 2,
        lean: true,
        ..HumanoidSpec::default()
    };
    let (_dir, path) = write_humanoid(&spec);
    let model = load_model(&path).expect("humanoid loads");
    let leaning = looping(&model, "lean");
    let pitching = aiming(&leaning, three_bone_chain(&model), degrees(30.0));
    let above = below(&model, node_named(&model, "spine_03"));

    let pitched = pitching.pose(&model);
    let lean = leaning.pose(&model);

    let spine = lean.nodes()[SPINE_01];
    let expected = Mat3A::from_rotation_z(0.5) * Mat3A::from_rotation_y(0.4);
    assert!(
        spine.matrix3.abs_diff_eq(expected, 1e-5),
        "the clip turns spine_01 about Z then Y: {:?}",
        spine.matrix3
    );
    let motion = shared_motion(&pitched, &lean, &above);
    assert_rotation(motion, Quat::from_rotation_x(-degrees(30.0)));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "finite")]
fn setting_a_non_finite_pitch_panics_in_debug_builds() {
    let (_dir, model) = load_aimer();
    let mut animator = walker(&model);

    animator.set_aim_pitch(f32::NAN);
}

#[test]
#[should_panic(expected = "is not a node")]
fn posing_with_a_link_past_the_last_node_panics() {
    let (_dir, model) = load_aimer();
    let link = AimLink {
        node: model.nodes().len(),
        share: 1.0,
    };
    let animator = aiming(&walker(&model), vec![link], 0.5);

    animator.pose(&model);
}
