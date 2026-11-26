//! Animation clips and the player that poses a [`Model`] from them. A pose is a value
//! computed from a model, a clip and a time; it never changes the model.

use std::num::NonZeroU32;
use std::ops::{Add, Mul};

use glam::{Affine3A, Quat, Vec3};

use crate::{Model, ModelNode};

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

impl Track {
    fn times(&self) -> &[f32] {
        match self {
            Track::Translation { times, .. }
            | Track::Rotation { times, .. }
            | Track::Scale { times, .. } => times,
        }
    }
}

/// One node property animated by keys.
///
/// Invariant (upheld by the importer): key times are non-empty and strictly increasing,
/// with one value per key, or three (in-tangent, value, out-tangent) for `CubicSpline`.
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    /// Index into [`Model::nodes`].
    pub node: usize,
    pub interpolation: Interpolation,
    pub track: Track,
}

impl Channel {
    /// Replace this channel's property of a decomposed `(scale, rotation, translation)`
    /// with its value at `time`.
    fn apply(&self, time: f32, (scale, rotation, translation): &mut (Vec3, Quat, Vec3)) {
        let interpolation = self.interpolation;
        match &self.track {
            Track::Translation { times, values } => {
                *translation = sample(interpolation, times, values, time, Vec3::lerp);
            }
            Track::Scale { times, values } => {
                *scale = sample(interpolation, times, values, time, Vec3::lerp);
            }
            Track::Rotation { times, values } => {
                let sampled = sample(interpolation, times, values, time, Quat::slerp);
                // glTF rotations are unit quaternions; a Hermite blend of them is not.
                *rotation = if interpolation == Interpolation::CubicSpline {
                    sampled.normalize()
                } else {
                    sampled
                };
            }
        }
    }
}

/// The value of a track at `time`, holding the first value before the first key (and on
/// a NaN time) and the last after the last. `linear` blends two values for `Linear`.
fn sample<T>(
    interpolation: Interpolation,
    times: &[f32],
    values: &[T],
    time: f32,
    linear: impl Fn(T, T, f32) -> T,
) -> T
where
    T: Copy + Add<Output = T> + Mul<f32, Output = T>,
{
    let cubic = interpolation == Interpolation::CubicSpline;
    let value = |key: usize| {
        if cubic {
            values[key * 3 + 1]
        } else {
            values[key]
        }
    };
    let last = times.len() - 1;
    if time.is_nan() || time <= times[0] {
        return value(0);
    }
    if time >= times[last] {
        return value(last);
    }
    let key = times.partition_point(|&t| t <= time) - 1;
    let span = times[key + 1] - times[key];
    let u = (time - times[key]) / span;
    match interpolation {
        Interpolation::Step => value(key),
        Interpolation::Linear => linear(value(key), value(key + 1), u),
        Interpolation::CubicSpline => {
            // glTF tangents are per unit of key time, so they scale by the key interval.
            let out_tangent = values[key * 3 + 2] * span;
            let in_tangent = values[(key + 1) * 3] * span;
            let (u2, u3) = (u * u, u * u * u);
            value(key) * (2.0 * u3 - 3.0 * u2 + 1.0)
                + out_tangent * (u3 - 2.0 * u2 + u)
                + value(key + 1) * (3.0 * u2 - 2.0 * u3)
                + in_tangent * (u3 - u2)
        }
    }
}

/// A named set of channels.
#[derive(Clone, Debug)]
pub struct Clip {
    name: Option<String>,
    channels: Vec<Channel>,
    duration: f32,
}

impl Clip {
    pub(crate) fn new(name: Option<String>, channels: Vec<Channel>) -> Self {
        let duration = channels
            .iter()
            .filter_map(|c| c.track.times().last().copied())
            .fold(0.0, f32::max);
        Self {
            name,
            channels,
            duration,
        }
    }

    /// The animation's name in the file, if it has one.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The latest key time over its channels; 0 with none.
    pub fn duration(&self) -> f32 {
        self.duration
    }

    /// Per node of `nodes`, its decomposed `(scale, rotation, translation)` at `time`:
    /// the node's decomposed `local` with each channel's property replaced, in channel
    /// order, or `None` where no channel targets the node.
    pub(crate) fn locals_at(
        &self,
        nodes: &[ModelNode],
        time: f32,
    ) -> Vec<Option<(Vec3, Quat, Vec3)>> {
        let mut parts: Vec<Option<(Vec3, Quat, Vec3)>> = vec![None; nodes.len()];
        for channel in &self.channels {
            let srt = parts[channel.node]
                .get_or_insert_with(|| nodes[channel.node].local.to_scale_rotation_translation());
            channel.apply(time, srt);
        }
        parts
    }
}

/// Each node's model-space transform in walk order: the posed counterpart of
/// [`crate::ModelNode::rest`].
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    nodes: Vec<Affine3A>,
}

impl Pose {
    pub(crate) fn new(nodes: Vec<Affine3A>) -> Self {
        Self { nodes }
    }

    /// One model-space transform per [`Model::nodes`] entry, in the same order.
    pub fn nodes(&self) -> &[Affine3A] {
        &self.nodes
    }
}

/// What a player does at the end of its clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    Loop,
    Once,
    /// Play once, reaching the clip's end on the n-th advance. Exact for n up to 2^24
    /// (the time is an `f32` ratio of counts).
    Ticks(NonZeroU32),
}

/// The nodes an action poses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Every node.
    Whole,
    /// This node index and every node below it. [`Animator::play_action`] rejects an
    /// index past the last node.
    From(usize),
}

/// One node of an aim chain and its share of the aim pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimLink {
    /// Index into [`Model::nodes`].
    pub node: usize,
    pub share: f32,
}

/// A running action: its player and the nodes it poses.
#[derive(Clone, Debug, PartialEq)]
struct Action {
    player: ClipPlayer,
    region: Region,
}

/// Why a clip could not be played.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClipError {
    #[error("model has no clip named `{name}`")]
    UnknownClip { name: String },
    #[error("region starts at node {node}, but the model has {count} nodes")]
    NodeOutOfRange { node: usize, count: usize },
}

/// One character's playback position in one clip of a model.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipPlayer {
    clip: usize,
    time: f32,
    speed: f32,
    playback: Playback,
    elapsed: u32,
}

impl ClipPlayer {
    /// A player at time 0 and speed 1 on the first clip of `model` called `clip`.
    ///
    /// # Errors
    /// [`ClipError::UnknownClip`] when the model has no such clip.
    pub fn new(model: &Model, clip: &str, playback: Playback) -> Result<Self, ClipError> {
        Ok(Self {
            clip: find(model, clip)?,
            time: 0.0,
            speed: 1.0,
            playback,
            elapsed: 0,
        })
    }

    /// Switch to `clip` and restart at time 0; speed is kept.
    ///
    /// # Errors
    /// [`ClipError::UnknownClip`] when the model has no such clip; nothing changes.
    pub fn play(&mut self, model: &Model, clip: &str, playback: Playback) -> Result<(), ClipError> {
        self.clip = find(model, clip)?;
        self.time = 0.0;
        self.elapsed = 0;
        self.playback = playback;
        Ok(())
    }

    /// Multiplier on every later [`ClipPlayer::advance`] step; negative plays backwards.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn playback(&self) -> Playback {
        self.playback
    }

    /// Add `dt * speed` to the time; `Loop` wraps into `[0, duration)`, `Once` clamps to
    /// `[0, duration]`. A non-finite step or a zero-length clip leaves the time unchanged.
    ///
    /// `Ticks(n)` ignores `dt` and speed: the n-th call sets the time to exactly the clip's
    /// duration (computed from the call count, so it does not drift) and later calls hold it.
    /// A zero-length clip still counts its calls.
    ///
    /// # Panics
    /// When `model` has fewer clips than the one this player was made on.
    pub fn advance(&mut self, model: &Model, dt: f32) {
        let duration = model.clips()[self.clip].duration();
        match self.playback {
            Playback::Ticks(total) => {
                self.elapsed = self.elapsed.saturating_add(1).min(total.get());
                self.time = duration * (self.elapsed as f32 / total.get() as f32);
            }
            Playback::Loop => self.step(dt, duration, true),
            Playback::Once => self.step(dt, duration, false),
        }
    }

    /// Add `dt * speed` to the time, wrapping when `wrap` and clamping otherwise.
    fn step(&mut self, dt: f32, duration: f32, wrap: bool) {
        let step = dt * self.speed;
        if !step.is_finite() || duration <= 0.0 {
            return;
        }
        let time = self.time + step;
        self.time = if wrap {
            // A tiny negative time rounds up to exactly `duration`, which is the next loop's 0.
            let wrapped = time.rem_euclid(duration);
            if wrapped < duration { wrapped } else { 0.0 }
        } else {
            time.clamp(0.0, duration)
        };
    }

    /// Whether the clip has reached its end and will not move again: `Ticks` after its
    /// n-th advance, `Once` at its clamp end (the duration for speed >= 0, 0 for a negative
    /// speed), `Loop` never.
    ///
    /// # Panics
    /// When `model` has fewer clips than the one this player was made on.
    pub fn is_finished(&self, model: &Model) -> bool {
        match self.playback {
            Playback::Loop => false,
            Playback::Once if self.speed < 0.0 => self.time <= 0.0,
            Playback::Once => self.time >= model.clips()[self.clip].duration(),
            Playback::Ticks(total) => self.elapsed >= total.get(),
        }
    }

    /// Index into [`Model::clips`].
    pub fn clip(&self) -> usize {
        self.clip
    }

    /// Seconds into the clip.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// The model posed by this player's clip at its time; see [`Model::pose`].
    ///
    /// # Panics
    /// When `model` has fewer clips than the one this player was made on.
    pub fn pose(&self, model: &Model) -> Pose {
        model.pose(self.clip, self.time)
    }
}

/// One character's playback: a base clip (locomotion) and an optional action clip (a
/// reload) that poses its [`Region`] of the body while it plays; the rest follows the base.
///
/// The base keeps advancing under an action, so cancelling or finishing the action returns
/// the pose the character would have had with no action.
///
/// Tick order: call [`Animator::advance`] once per tick, before that tick's commands. A clip
/// started on a tick then shows its first pose on that tick. An action started on tick 100
/// as `Playback::Ticks(156)` (2.6 s at 60 Hz) shows its last pose on tick 256, and the base
/// returns on tick 257. Advancing after the commands makes a timed clip end one tick early.
#[derive(Clone, Debug, PartialEq)]
pub struct Animator {
    base: ClipPlayer,
    action: Option<Action>,
}

impl Animator {
    /// An animator playing `base` with no action.
    pub fn new(base: ClipPlayer) -> Self {
        Self { base, action: None }
    }

    pub fn base(&self) -> &ClipPlayer {
        &self.base
    }

    pub fn base_mut(&mut self) -> &mut ClipPlayer {
        &mut self.base
    }

    pub fn action(&self) -> Option<&ClipPlayer> {
        self.action.as_ref().map(|action| &action.player)
    }

    /// Replace any running action with `clip` over `region`, restarted at time 0.
    ///
    /// # Errors
    /// [`ClipError::UnknownClip`] when the model has no such clip;
    /// [`ClipError::NodeOutOfRange`] when `region` starts past the model's last node.
    /// Nothing changes on an error.
    pub fn play_action(
        &mut self,
        model: &Model,
        clip: &str,
        playback: Playback,
        region: Region,
    ) -> Result<(), ClipError> {
        let count = model.nodes().len();
        if let Region::From(node) = region
            && node >= count
        {
            return Err(ClipError::NodeOutOfRange { node, count });
        }
        self.action = Some(Action {
            player: ClipPlayer::new(model, clip, playback)?,
            region,
        });
        Ok(())
    }

    /// Replace the aim chain. Not implemented yet.
    pub fn set_aim_chain(&mut self, _links: impl IntoIterator<Item = AimLink>) {}

    /// Hold the aim pitch in radians. Not implemented yet.
    pub fn set_aim_pitch(&mut self, _radians: f32) {}

    /// The held aim pitch in radians. Not implemented yet.
    pub fn aim_pitch(&self) -> f32 {
        0.0
    }

    /// Drop the action; the pose is the base's from now on.
    pub fn cancel_action(&mut self) {
        self.action = None;
    }

    /// Advance the base by `dt`. An action that was already finished before this call is
    /// dropped; otherwise it advances too. So an action's last pose is read on the tick it
    /// is reached and the base returns on the next. See the type docs for tick order.
    ///
    /// # Panics
    /// When `model` has fewer clips than a player was made on.
    pub fn advance(&mut self, model: &Model, dt: f32) {
        self.base.advance(model, dt);
        if self
            .action
            .as_ref()
            .is_some_and(|action| action.player.is_finished(model))
        {
            self.action = None;
        } else if let Some(action) = &mut self.action {
            action.player.advance(model, dt);
        }
    }

    /// The base's pose, with the action's clip posing its region if one is running; see
    /// [`Model::layered_pose`].
    ///
    /// # Panics
    /// When `model` has fewer clips than a player was made on.
    pub fn pose(&self, model: &Model) -> Pose {
        match &self.action {
            None => self.base.pose(model),
            Some(Action {
                player,
                region: Region::Whole,
            }) => player.pose(model),
            Some(Action {
                player,
                region: Region::From(node),
            }) => model.layered_pose(
                (self.base.clip(), self.base.time()),
                (player.clip(), player.time()),
                *node,
            ),
        }
    }
}

/// The index of `model`'s first clip called `name`.
fn find(model: &Model, name: &str) -> Result<usize, ClipError> {
    model.clip(name).ok_or_else(|| ClipError::UnknownClip {
        name: name.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use glam::{Affine3A, Quat, Vec3};
    use proptest::prelude::*;

    use super::{
        Animator, Channel, Clip, ClipError, ClipPlayer, Interpolation, Playback, Region, Track,
    };
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
    fn a_looping_player_stepped_back_by_a_hair_stays_below_its_duration() {
        let model = model_with(vec![ramp()]);
        let duration = model.clips()[0].duration();

        for hair in [1e-8_f32, 1e-7] {
            let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
            player.set_speed(-1.0);

            player.advance(&model, hair);

            assert!(
                (0.0..duration).contains(&player.time()),
                "stepping back {hair} gave {} outside [0, {duration})",
                player.time()
            );
        }
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

    fn ticks(n: u32) -> Playback {
        Playback::Ticks(NonZeroU32::new(n).expect("n is positive"))
    }

    #[test]
    fn ticks_playback_ignores_dt_and_speed() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", ticks(4)).expect("clip exists");
        player.set_speed(-3.0);
        let mut times = Vec::new();

        for dt in [100.0, f32::NAN, 0.0, -5.0] {
            player.advance(&model, dt);
            times.push(player.time());
        }
        player.advance(&model, 1.0);

        assert_eq!(times, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(player.time(), 4.0);
        assert_eq!(player.speed(), -3.0);
        assert!(player.is_finished(&model));
    }

    #[test]
    fn play_resets_ticks_elapsed() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", ticks(4)).expect("clip exists");
        for _ in 0..3 {
            player.advance(&model, 0.0);
        }

        player.play(&model, "ramp", ticks(4)).expect("clip exists");

        assert_eq!(player.time(), 0.0);
        assert!(!player.is_finished(&model));
        for _ in 0..3 {
            player.advance(&model, 0.0);
        }
        assert_eq!(player.time(), 3.0);
        assert!(!player.is_finished(&model));
        player.advance(&model, 0.0);
        assert_eq!(player.time(), 4.0);
        assert!(player.is_finished(&model));
    }

    #[test]
    fn once_backwards_is_finished_at_zero() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Once).expect("clip exists");

        assert!(!player.is_finished(&model));
        player.set_speed(-1.0);
        assert!(player.is_finished(&model));

        player.set_speed(1.0);
        player.advance(&model, 2.0);
        assert!(!player.is_finished(&model));
        player.set_speed(-1.0);
        assert!(!player.is_finished(&model));
        player.advance(&model, 2.0);
        assert_eq!(player.time(), 0.0);
        assert!(player.is_finished(&model));

        player.set_speed(1.0);
        player.advance(&model, 4.0);
        assert!(player.is_finished(&model));
    }

    /// A 2-second clip moving node 0 along x from 0 to `to`.
    fn moving(name: &str, to: f32) -> Clip {
        clip(
            name,
            vec![channel(
                Interpolation::Linear,
                track_x(&[0.0, 2.0], &[0.0, to]),
            )],
        )
    }

    fn animator_on(model: &Model) -> Animator {
        Animator::new(ClipPlayer::new(model, "ramp", Playback::Loop).expect("clip exists"))
    }

    #[test]
    fn a_one_tick_action_shows_its_last_pose_at_once_and_is_dropped_on_the_next_advance() {
        let model = model_with(vec![ramp(), moving("act", 9.0)]);
        let mut animator = animator_on(&model);
        animator
            .play_action(&model, "act", ticks(1), Region::Whole)
            .expect("clip exists");

        animator.advance(&model, 0.5);

        let action = animator.action().expect("held on its last tick");
        assert!(action.is_finished(&model));
        assert_eq!(action.time(), 2.0);
        assert_eq!(animator.pose(&model), model.pose(1, 2.0));
        animator.advance(&model, 0.5);
        assert!(animator.action().is_none());
        assert_eq!(animator.pose(&model), animator.base().pose(&model));
        assert_eq!(animator.base().time(), 1.0);
    }

    #[test]
    fn ticks_playback_at_the_end_of_a_maximal_count_holds_without_overflow() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", ticks(u32::MAX)).expect("clip exists");
        player.elapsed = u32::MAX;

        player.advance(&model, 1.0);

        assert_eq!(player.elapsed, u32::MAX);
        assert_eq!(player.time(), 4.0);
        assert!(player.is_finished(&model));
    }

    #[test]
    fn a_once_action_on_a_zero_length_clip_is_finished_and_dropped_on_the_next_advance() {
        let model = model_with(vec![ramp(), clip("still", Vec::new())]);
        let mut animator = animator_on(&model);

        animator
            .play_action(&model, "still", Playback::Once, Region::Whole)
            .expect("clip exists");

        assert!(animator.action().expect("playing").is_finished(&model));
        animator.advance(&model, 0.5);
        assert!(animator.action().is_none());
        assert_eq!(animator.pose(&model), animator.base().pose(&model));
    }

    #[test]
    fn switching_the_base_clip_under_an_action_leaves_the_action_alone() {
        let model = model_with(vec![ramp(), moving("act", 9.0), moving("other", 5.0)]);
        let mut animator = animator_on(&model);
        animator
            .play_action(&model, "act", ticks(10), Region::Whole)
            .expect("clip exists");
        for _ in 0..3 {
            animator.advance(&model, 0.5);
        }
        let action_before = animator.action().expect("playing").clone();

        animator
            .base_mut()
            .play(&model, "other", Playback::Loop)
            .expect("clip exists");

        assert_eq!(animator.action(), Some(&action_before));
        assert_eq!(animator.pose(&model), action_before.pose(&model));
        animator.cancel_action();
        assert_eq!(animator.pose(&model), model.pose(2, 0.0));
    }

    #[test]
    fn play_action_from_a_node_past_the_last_is_rejected_naming_the_node_and_count() {
        let model = model_with(vec![ramp(), moving("act", 9.0)]);
        let count = model.nodes().len();

        for node in [count, count + 5] {
            let mut animator = animator_on(&model);

            let result = animator.play_action(&model, "act", Playback::Once, Region::From(node));

            let error = result.expect_err("region starts past the last node");
            assert_eq!(error, ClipError::NodeOutOfRange { node, count });
            let text = error.to_string();
            assert!(text.contains(&node.to_string()), "{text}");
            assert!(text.contains(&count.to_string()), "{text}");
            assert!(animator.action().is_none());
        }
    }

    #[test]
    fn a_rejected_region_leaves_the_running_action_and_the_animator_unchanged() {
        let model = model_with(vec![ramp(), moving("act", 9.0), moving("other", 5.0)]);
        let count = model.nodes().len();
        let mut animator = animator_on(&model);
        animator
            .play_action(&model, "act", ticks(10), Region::Whole)
            .expect("clip exists");
        animator.advance(&model, 0.5);
        let before = animator.clone();

        let result = animator.play_action(&model, "other", Playback::Once, Region::From(count));

        assert_eq!(
            result,
            Err(ClipError::NodeOutOfRange { node: count, count })
        );
        assert_eq!(animator, before);
        assert_eq!(animator.action(), before.action());
    }

    #[test]
    fn play_action_from_the_last_node_is_accepted() {
        let model = model_with(vec![ramp(), moving("act", 9.0)]);
        let last = model.nodes().len() - 1;
        let mut animator = animator_on(&model);

        let result = animator.play_action(&model, "act", Playback::Once, Region::From(last));

        assert_eq!(result, Ok(()));
        assert!(animator.action().is_some());
    }

    #[test]
    fn a_looping_player_is_never_finished() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");

        for _ in 0..3 {
            player.advance(&model, 4.0);
            assert!(!player.is_finished(&model));
        }

        let mut backwards = ClipPlayer::new(&model, "ramp", Playback::Loop).expect("clip exists");
        backwards.set_speed(-1.0);
        assert_eq!(backwards.time(), 0.0);
        assert!(!backwards.is_finished(&model));
    }

    #[test]
    fn a_once_player_at_speed_zero_and_time_zero_is_not_finished() {
        let model = model_with(vec![ramp()]);
        let mut player = ClipPlayer::new(&model, "ramp", Playback::Once).expect("clip exists");

        player.set_speed(0.0);

        assert_eq!(player.time(), 0.0);
        assert!(!player.is_finished(&model));
    }

    #[test]
    fn ticks_playback_on_a_zero_length_clip_still_finishes_on_the_nth_advance() {
        let model = model_with(vec![clip("still", Vec::new())]);
        let mut player = ClipPlayer::new(&model, "still", ticks(3)).expect("clip exists");

        for _ in 0..2 {
            player.advance(&model, 0.0);
            assert!(!player.is_finished(&model));
        }
        player.advance(&model, 0.0);

        assert!(player.is_finished(&model));
        assert_eq!(player.time(), 0.0);
    }

    proptest! {
        #[test]
        fn ticks_playback_ends_on_the_nth_advance_for_any_n_and_duration(
            n in 1_u32..10_000,
            duration in 0.01_f32..120.0,
        ) {
            let long = clip(
                "long",
                vec![channel(Interpolation::Linear, track_x(&[0.0, duration], &[0.0, 1.0]))],
            );
            let model = model_with(vec![long]);
            let mut player = ClipPlayer::new(&model, "long", ticks(n)).expect("clip exists");

            for _ in 1..n {
                player.advance(&model, 1.0 / 60.0);
                prop_assert!(player.time() < duration, "{} >= {duration}", player.time());
                prop_assert!(!player.is_finished(&model));
            }
            player.advance(&model, 1.0 / 60.0);

            prop_assert_eq!(player.time(), duration);
            prop_assert!(player.is_finished(&model));
        }

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
