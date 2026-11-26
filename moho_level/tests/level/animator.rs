//! A sim-timed action clip over locomotion, driven the way a sim ticks: `advance` once per
//! tick before that tick's commands.

use std::num::NonZeroU32;

use glam::Affine3A;
use moho_level::anim::{Animator, ClipError, ClipPlayer, Playback, Pose, Region};
use moho_level::{Model, load_model};
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
