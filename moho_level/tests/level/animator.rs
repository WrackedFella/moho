//! A sim-timed action clip over locomotion, driven the way a sim ticks: `advance` once per
//! tick before that tick's commands.

use std::num::NonZeroU32;

use glam::Affine3A;
use moho_level::anim::{Animator, ClipError, ClipPlayer, Playback, Pose};
use moho_level::{Model, load_model};
use tempfile::TempDir;

use crate::support::humanoid::{HumanoidSpec, RELOAD_DURATION, write_humanoid};

/// One simulation tick.
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
                .play_action(model, "reload", playback)
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
                .play_action(&model, "reload", ticks(RELOAD_TICKS))
                .expect("reload exists");
        }
        states.push(animator.clone());
    }

    assert_eq!(states[130].action().expect("restarted").time(), 0.0);
    for tick in 131..286 {
        let action = states[tick].action().expect("still playing");
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

    let from_playing = playing.play_action(&model, "dance", Playback::Once);
    let from_idle = not_playing.play_action(&model, "dance", Playback::Once);

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
