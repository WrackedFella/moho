//! Animation clips and the player that poses a [`Model`] from them. A pose is a value
//! computed from a model, a clip and a time; it never changes the model.

use glam::{Affine3A, Quat, Vec3};

use crate::Model;

/// How a channel blends between neighbouring keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Interpolation {
    Step,
    /// Rotation blends by slerp.
    Linear,
    /// Hermite spline; the track holds in-tangent, value and out-tangent per key. A
    /// rotation is renormalised after the Hermite step.
    CubicSpline,
}

/// Key times with the values of one node property.
#[derive(Clone, Debug)]
pub(crate) enum Track {
    Translation { times: Vec<f32>, values: Vec<Vec3> },
    Rotation { times: Vec<f32>, values: Vec<Quat> },
    Scale { times: Vec<f32>, values: Vec<Vec3> },
}

/// One node property animated by keys.
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    /// Index into [`Model::nodes`].
    pub node: usize,
    pub interpolation: Interpolation,
    pub track: Track,
}

/// A named set of channels.
#[derive(Clone, Debug)]
pub struct Clip {
    name: Option<String>,
    channels: Vec<Channel>,
}

impl Clip {
    pub(crate) fn new(name: Option<String>, channels: Vec<Channel>) -> Self {
        Self { name, channels }
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The latest key time over its channels; 0 with none.
    pub fn duration(&self) -> f32 {
        todo!("latest key time over the channels")
    }
}

/// Each node's model-space transform in walk order: the posed counterpart of
/// [`crate::ModelNode::rest`].
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    nodes: Vec<Affine3A>,
}

impl Pose {
    pub fn nodes(&self) -> &[Affine3A] {
        &self.nodes
    }
}

/// What a player does at the end of its clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    Loop,
    Once,
}

/// Why a clip could not be played.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClipError {
    #[error("model has no clip named `{name}`")]
    UnknownClip { name: String },
}

/// One character's playback position in one clip of a model.
#[derive(Clone, Debug)]
pub struct ClipPlayer {
    clip: usize,
    time: f32,
    speed: f32,
    playback: Playback,
}

impl ClipPlayer {
    /// A player at time 0 and speed 1 on the first clip of `model` called `clip`.
    ///
    /// # Errors
    /// [`ClipError::UnknownClip`] when the model has no such clip.
    pub fn new(model: &Model, clip: &str, playback: Playback) -> Result<Self, ClipError> {
        let _ = (model, clip, playback);
        todo!("find the clip and start at time 0")
    }

    /// Switch to `clip` and restart at time 0; speed is kept.
    ///
    /// # Errors
    /// [`ClipError::UnknownClip`] when the model has no such clip; nothing changes.
    pub fn play(&mut self, model: &Model, clip: &str, playback: Playback) -> Result<(), ClipError> {
        let _ = (model, clip, playback);
        todo!("switch clip, restart, leave state alone on an unknown name")
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn playback(&self) -> Playback {
        self.playback
    }

    /// Add `dt * speed` to the time; `Loop` wraps, `Once` clamps to the clip.
    pub fn advance(&mut self, model: &Model, dt: f32) {
        let _ = (model, dt);
        todo!("advance, wrap or clamp")
    }

    /// Index into [`Model::clips`].
    pub fn clip(&self) -> usize {
        self.clip
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn pose(&self, model: &Model) -> Pose {
        let _ = model;
        todo!("pose the model at the player's clip and time")
    }
}

#[cfg(test)]
mod tests {
    use glam::{Affine3A, Quat, Vec3};
    use proptest::prelude::*;

    use super::{Channel, Clip, ClipError, ClipPlayer, Interpolation, Playback, Track};
    use crate::{Model, ModelNode};

    fn track_x(times: &[f32], xs: &[f32]) -> Track {
        Track::Translation {
            times: times.to_vec(),
            values: xs.iter().map(|&x| Vec3::new(x, 0.0, 0.0)).collect(),
        }
    }

    fn clip(name: &str, channels: Vec<Channel>) -> Clip {
        Clip::new(Some(name.to_owned()), channels)
    }

    fn channel(interpolation: Interpolation, track: Track) -> Channel {
        Channel {
            node: 0,
            interpolation,
            track,
        }
    }

    fn model_with(clips: Vec<Clip>) -> Model {
        let node = ModelNode {
            name: None,
            parent: None,
            local: Affine3A::IDENTITY,
            rest: Affine3A::IDENTITY,
        };
        Model::for_tests(vec![node], clips)
    }

    /// A 4-second clip moving node 0 along x from 0 to 4 at one unit per second.
    fn ramp() -> Clip {
        clip(
            "ramp",
            vec![channel(
                Interpolation::Linear,
                track_x(&[0.0, 4.0], &[0.0, 4.0]),
            )],
        )
    }

    fn x_at(player: &ClipPlayer, model: &Model) -> f32 {
        player.pose(model).nodes()[0].translation.x
    }

    fn x_after(model: &Model, playback: Playback, dt: f32) -> f32 {
        let mut player = ClipPlayer::new(model, "ramp", playback).expect("clip exists");
        player.advance(model, dt);
        x_at(&player, model)
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn duration_is_the_latest_key_time_over_all_channels_and_zero_with_none() {
        let two = clip(
            "two",
            vec![
                channel(
                    Interpolation::Step,
                    Track::Scale {
                        times: vec![0.0, 2.0, 3.5],
                        values: vec![Vec3::ONE; 3],
                    },
                ),
                channel(Interpolation::Linear, track_x(&[0.0, 1.5], &[0.0, 1.0])),
            ],
        );
        let empty = clip("empty", Vec::new());

        assert_eq!(two.duration(), 3.5);
        assert_eq!(empty.duration(), 0.0);
        assert_eq!(two.name(), Some("two"));
    }

    #[test]
    fn a_looping_player_after_one_and_a_quarter_durations_is_at_a_quarter() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");

        player.advance(&model, 5.0);

        assert_near(player.time(), 1.0);
        assert_near(x_at(&player, &model), 1.0);
    }

    #[test]
    fn a_once_player_holds_the_end_after_twice_its_duration() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Once).expect("clip exists");

        player.advance(&model, 8.0);

        assert_eq!(player.time(), 4.0);
        assert_near(x_at(&player, &model), 4.0);
    }

    #[test]
    fn playing_another_clip_restarts_at_time_zero() {
        let other = clip(
            "other",
            vec![channel(
                Interpolation::Linear,
                track_x(&[0.0, 2.0], &[7.0, 9.0]),
            )],
        );
        let model = model_with(vec![ramp(), other]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
        player.set_speed(2.0);
        player.advance(&model, 1.0);

        player
            .play(&model, "other", Playback::Once)
            .expect("clip exists");

        assert_eq!(player.clip(), 1);
        assert_eq!(player.time(), 0.0);
        assert_eq!(player.playback(), Playback::Once);
        assert_eq!(player.speed(), 2.0);
        assert_near(x_at(&player, &model), 7.0);
    }

    #[test]
    fn a_looping_player_advanced_by_exactly_one_duration_is_at_time_zero() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");

        player.advance(&model, 4.0);

        assert_eq!(player.time(), 0.0);
        assert_near(x_at(&player, &model), 0.0);
    }

    #[test]
    fn the_first_of_two_clips_sharing_a_name_is_found() {
        let first = clip("same", Vec::new());
        let second = clip("same", Vec::new());
        let model = model_with(vec![ramp(), first, second]);

        assert_eq!(model.clip("same"), Some(1));
        assert_eq!(model.clip("ramp"), Some(0));
        assert_eq!(model.clip("missing"), None);
    }

    #[test]
    fn translation_and_rotation_channels_on_one_node_both_apply() {
        let quarter_turn = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let translation = || {
            channel(
                Interpolation::Step,
                Track::Translation {
                    times: vec![0.0],
                    values: vec![Vec3::new(3.0, 0.0, 0.0)],
                },
            )
        };
        let rotation = || {
            channel(
                Interpolation::Step,
                Track::Rotation {
                    times: vec![0.0],
                    values: vec![quarter_turn],
                },
            )
        };
        let moves_then_turns = clip("a", vec![translation(), rotation()]);
        let turns_then_moves = clip("b", vec![rotation(), translation()]);
        let model = model_with(vec![moves_then_turns, turns_then_moves]);

        let expected = Affine3A::from_rotation_translation(quarter_turn, Vec3::new(3.0, 0.0, 0.0));
        for name in ["a", "b"] {
            let player = ClipPlayer::new(&model, name, Playback::Once).expect("clip exists");
            let posed = player.pose(&model).nodes()[0];
            assert!(posed.abs_diff_eq(expected, 1e-5), "{name}: {posed:?}");
        }
    }

    #[test]
    fn a_cubic_rotation_is_renormalised_after_the_hermite_step() {
        let quarter_turn = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let zero = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
        // Zero tangents: the midpoint is the raw average of the two keys, which is
        // shorter than unit length.
        let spline = clip(
            "ramp",
            vec![channel(
                Interpolation::CubicSpline,
                Track::Rotation {
                    times: vec![0.0, 2.0],
                    values: vec![zero, Quat::IDENTITY, zero, zero, quarter_turn, zero],
                },
            )],
        );
        let model = model_with(vec![spline]);
        let raw = (Quat::IDENTITY * 0.5) + (quarter_turn * 0.5);
        assert!(raw.length() < 0.95, "the raw Hermite result is not unit");

        let mut player = ClipPlayer::new(&model, "ramp", Playback::Once).expect("clip exists");
        player.advance(&model, 1.0);

        let posed = player.pose(&model).nodes()[0];
        let expected = Affine3A::from_quat(raw.normalize());
        assert!(
            posed.abs_diff_eq(expected, 1e-5),
            "{posed:?} != {expected:?}"
        );
        for axis in [posed.x_axis, posed.y_axis, posed.z_axis] {
            assert_near(axis.length(), 1.0);
        }
    }

    #[test]
    fn playing_an_unknown_clip_fails_naming_it_and_keeps_clip_and_time() {
        let other = clip(
            "other",
            vec![channel(
                Interpolation::Step,
                track_x(&[0.0, 1.0], &[0.0, 1.0]),
            )],
        );
        let model = model_with(vec![ramp(), other]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
        player.advance(&model, 1.5);

        let err = player
            .play(&model, "dance", Playback::Once)
            .expect_err("no such clip");
        let at_creation =
            ClipPlayer::new(&model, "dance", Playback::Loop).expect_err("no such clip either");

        assert_eq!(
            err,
            ClipError::UnknownClip {
                name: "dance".into()
            }
        );
        assert_eq!(err.to_string(), "model has no clip named `dance`");
        assert_eq!(at_creation, err);
        assert_eq!(player.clip(), 0);
        assert_eq!(player.time(), 1.5);
        assert_eq!(player.playback(), Playback::Loop);
        player.advance(&model, 3.0);
        assert_near(player.time(), 0.5);
    }

    #[test]
    fn step_holds_the_previous_key() {
        let stepped = clip(
            "ramp",
            vec![channel(
                Interpolation::Step,
                track_x(&[0.0, 1.0, 2.0], &[0.0, 10.0, 20.0]),
            )],
        );
        let model = model_with(vec![stepped]);

        let before = x_after(&model, Playback::Once, 0.99);
        let on = x_after(&model, Playback::Once, 1.0);
        let between = x_after(&model, Playback::Once, 1.5);
        let end = x_after(&model, Playback::Once, 2.0);

        assert_eq!(before, 0.0);
        assert_eq!(on, 10.0);
        assert_eq!(between, 10.0);
        assert_eq!(end, 20.0);
    }

    #[test]
    fn linear_rotation_slerps() {
        let quarter_turn = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let turning = clip(
            "ramp",
            vec![channel(
                Interpolation::Linear,
                Track::Rotation {
                    times: vec![0.0, 1.0],
                    values: vec![Quat::IDENTITY, quarter_turn],
                },
            )],
        );
        let model = model_with(vec![turning]);

        let (eighth, half, whole) = (0.25_f32, 0.5_f32, 1.0_f32);
        let at = |t: f32| {
            let mut player = ClipPlayer::new(&model, "ramp", Playback::Once).expect("clip exists");
            player.advance(&model, t);
            player.pose(&model).nodes()[0]
        };

        for t in [eighth, half, whole] {
            let expected =
                Affine3A::from_quat(Quat::from_rotation_z(t * std::f32::consts::FRAC_PI_2));
            assert!(
                at(t).abs_diff_eq(expected, 1e-5),
                "t={t}: {:?} != {expected:?}",
                at(t)
            );
        }
    }

    #[test]
    fn cubic_spline_passes_through_keys_and_follows_tangents() {
        // Per key: in-tangent, value, out-tangent. Keys at 0, 2 and 4, so a tangent
        // contributes `0.125 * 2 * tangent` at a segment's midpoint. Second segment
        // midpoint: 0.5 * 1 + 0.25 * 2 + 0.5 * 3 - 0.25 * 4 = 1.5.
        let spline = |out_of_first: f32, into_second: f32| {
            let v = |x: f32| Vec3::new(x, 0.0, 0.0);
            clip(
                "ramp",
                vec![channel(
                    Interpolation::CubicSpline,
                    Track::Translation {
                        times: vec![0.0, 2.0, 4.0],
                        values: vec![
                            v(9.0),
                            v(0.0),
                            v(out_of_first),
                            v(into_second),
                            v(1.0),
                            v(2.0),
                            v(4.0),
                            v(3.0),
                            v(9.0),
                        ],
                    },
                )],
            )
        };
        let out_tangent = model_with(vec![spline(1.0, 0.0)]);
        let in_tangent = model_with(vec![spline(0.0, 1.0)]);

        let start = x_after(&out_tangent, Playback::Once, 0.0);
        let end = x_after(&out_tangent, Playback::Once, 2.0);
        let mid_out = x_after(&out_tangent, Playback::Once, 1.0);
        let mid_in = x_after(&in_tangent, Playback::Once, 1.0);
        let second_mid = x_after(&out_tangent, Playback::Once, 3.0);
        let last = x_after(&out_tangent, Playback::Once, 4.0);

        assert_near(start, 0.0);
        assert_near(end, 1.0);
        assert_near(mid_out, 0.75);
        assert_near(mid_in, 0.25);
        assert_near(second_mid, 1.5);
        assert_near(last, 3.0);
    }

    #[test]
    fn outside_the_keys_holds_the_first_or_last_value() {
        let held = clip(
            "ramp",
            vec![
                channel(Interpolation::Linear, track_x(&[1.0, 2.0], &[10.0, 20.0])),
                channel(
                    Interpolation::Linear,
                    Track::Scale {
                        times: vec![0.0, 4.0],
                        values: vec![Vec3::ONE, Vec3::splat(3.0)],
                    },
                ),
            ],
        );
        let model = model_with(vec![held]);

        let before = x_after(&model, Playback::Once, 0.5);
        let after = x_after(&model, Playback::Once, 3.0);

        assert_eq!(before, 10.0);
        assert_eq!(after, 20.0);
    }

    #[test]
    fn a_scale_channel_replaces_only_the_scale_of_a_decomposed_local() {
        let local = Affine3A::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::from_rotation_y(0.5),
            Vec3::new(1.0, 2.0, 3.0),
        );
        let node = ModelNode {
            name: None,
            parent: None,
            local,
            rest: local,
        };
        let growing = clip(
            "grow",
            vec![channel(
                Interpolation::Linear,
                Track::Scale {
                    times: vec![0.0, 1.0],
                    values: vec![Vec3::ONE, Vec3::splat(4.0)],
                },
            )],
        );
        let model = Model::for_tests(vec![node], vec![growing]);
        let mut player = ClipPlayer::new(&model, "grow", Playback::Once).expect("clip exists");

        player.advance(&model, 1.0);

        let expected = Affine3A::from_scale_rotation_translation(
            Vec3::splat(4.0),
            Quat::from_rotation_y(0.5),
            Vec3::new(1.0, 2.0, 3.0),
        );
        assert!(
            player.pose(&model).nodes()[0].abs_diff_eq(expected, 1e-5),
            "{:?}",
            player.pose(&model).nodes()[0]
        );
    }

    #[test]
    fn advance_scales_by_speed_and_negative_speed_wraps_backwards() {
        let model = model_with(vec![ramp()]);
        let mut fast = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
        fast.set_speed(2.0);
        let mut backwards = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
        backwards.set_speed(-1.0);
        let mut backwards_once =
            ClipPlayer::new(&model, "ramp", Playback::Once).expect("clip exists");
        backwards_once.set_speed(-1.0);

        fast.advance(&model, 1.0);
        let after_one = fast.time();
        fast.advance(&model, 1.5);
        backwards.advance(&model, 1.0);
        backwards_once.advance(&model, 1.0);

        assert_near(after_one, 2.0);
        assert_near(fast.time(), 1.0);
        assert_near(backwards.time(), 3.0);
        assert_eq!(backwards_once.time(), 0.0);
        assert_eq!(fast.speed(), 2.0);
    }

    #[test]
    fn a_non_finite_step_or_zero_length_clip_leaves_time_unchanged() {
        let still = clip("still", Vec::new());
        let model = model_with(vec![ramp(), still]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
        player.advance(&model, 1.0);
        let mut empty_loop = ClipPlayer::new(&model, "still", Playback::Loop).expect("clip exists");
        let mut empty_once = ClipPlayer::new(&model, "still", Playback::Once).expect("clip exists");

        player.advance(&model, f32::NAN);
        player.advance(&model, f32::INFINITY);
        player.advance(&model, f32::NEG_INFINITY);
        player.set_speed(f32::INFINITY);
        player.advance(&model, 1.0);
        player.advance(&model, 0.0);
        empty_loop.advance(&model, 1.0);
        empty_once.advance(&model, 1.0);

        assert_eq!(player.time(), 1.0);
        assert_eq!(empty_loop.time(), 0.0);
        assert_eq!(empty_once.time(), 0.0);
    }

    proptest! {
        #[test]
        fn linear_translation_stays_between_neighbouring_keys(
            a in -100.0_f32..100.0,
            b in -100.0_f32..100.0,
            start in 0.0_f32..10.0,
            span in 0.5_f32..10.0,
            fraction in 0.0_f32..1.0,
        ) {
            let end = start + span;
            let moving = clip(
                "ramp",
                vec![channel(Interpolation::Linear, track_x(&[start, end], &[a, b]))],
            );
            let model = model_with(vec![moving]);
            let time = start + span * fraction;

            let x = x_after(&model, Playback::Once, time);

            let (low, high) = (a.min(b), a.max(b));
            prop_assert!(x >= low - 1e-3 && x <= high + 1e-3, "{x} outside [{low}, {high}]");
            prop_assert!((x - (a + (b - a) * fraction)).abs() <= 0.05, "{x}");
        }
    }
}
