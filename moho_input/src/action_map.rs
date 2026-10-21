//! Turns platform-free input events into per-tick action frames.

use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;
use std::time::Duration;

use winit::event::{DeviceEvent, ElementState, WindowEvent};

use crate::bindings::{Action, ActionBindings, Binding, Input, Trigger};
use crate::filter::{DeadzoneFilter, MouseSmoothing};
use crate::key::{Key, MouseButton};
use crate::pad::{PadButton, PadInput, Stick, StickAxis, StickDir};

/// An axis reading at or past this magnitude holds its stick direction.
pub const STICK_DIRECTION_THRESHOLD: f32 = 0.5;

/// Degrees one mouse count turns at sensitivity 1.0, as in Source games.
pub const SOURCE_DEGREES_PER_COUNT: f32 = 0.022;

/// Radians one mouse count turns at a Source-scale `sensitivity`.
pub const fn source_look_per_count(sensitivity: f32) -> f32 {
    (SOURCE_DEGREES_PER_COUNT * sensitivity).to_radians()
}

/// Look units a fully deflected right stick adds per second, before sensitivity.
///
/// A half turn per second at the default sensitivity of 3.0.
pub const PAD_LOOK_PER_SECOND: f32 = std::f32::consts::PI / source_look_per_count(3.0);

const STICK_LOOK_DEAD_ZONE: f32 = 0.15;

/// How raw mouse counts become look: the turn per count, and whether to smooth.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LookSettings {
    pub look_per_count: f32,
    pub smoothing: bool,
    /// Negates the mouse's Y counts; the stick is unaffected.
    pub invert_y: bool,
}

impl LookSettings {
    /// Unsmoothed, non-inverted look at `look_per_count`.
    pub fn raw(look_per_count: f32) -> Self {
        Self {
            look_per_count,
            smoothing: false,
            invert_y: false,
        }
    }
}

/// What the actions did over one tick.
///
/// Plain data: masks are indexed by position in `A::ALL`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ActionFrame<A: Action> {
    held: u64,
    pressed: u64,
    released: u64,
    look: (f32, f32),
    pointer: Option<(f32, f32)>,
    press_pointer: Option<(f32, f32)>,
    actions: PhantomData<A>,
}

fn bit<A: Action>(action: A) -> u64 {
    A::ALL
        .iter()
        .position(|&a| a == action)
        .map_or(0, |index| 1 << index)
}

impl<A: Action> ActionFrame<A> {
    /// Down at the end of the tick.
    pub fn held(&self, action: A) -> bool {
        self.held & bit(action) != 0
    }

    /// Went from not held to held at any point during the tick.
    pub fn pressed(&self, action: A) -> bool {
        self.pressed & bit(action) != 0
    }

    /// Went from held to not held at any point during the tick.
    pub fn released(&self, action: A) -> bool {
        self.released & bit(action) != 0
    }

    /// Look for the tick: scaled (and, if enabled, smoothed) mouse counts plus the stick.
    pub fn look(&self) -> (f32, f32) {
        self.look
    }

    /// Pointer position in physical pixels at the end of the tick, `None` when outside the window.
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.pointer
    }

    /// Pointer position at the last mouse press in the tick, else `None`.
    pub fn press_pointer(&self) -> Option<(f32, f32)> {
        self.press_pointer
    }
}

/// Collects input events; frames sample look, and each tick reports it with the edges.
#[derive(Debug)]
pub struct ActionMap<A: Action> {
    bindings: ActionBindings<A>,
    /// Inputs now down, each with the ticks it has been down.
    down: HashMap<Input, u32>,
    /// Inputs some action binds with `Hold`: their `Press` bindings fire on a quick release.
    split: HashSet<Input>,
    held: u64,
    pressed: u64,
    released: u64,
    look_accumulator: (f64, f64),
    /// Look sampled by frames and not yet consumed by a tick.
    pending: (f32, f32),
    /// Left and right stick `(x, y)`, Y up.
    sticks: [(f32, f32); 2],
    look: LookSettings,
    smoothing: MouseSmoothing,
    pointer: Option<(f32, f32)>,
    press_pointer: Option<(f32, f32)>,
}

impl<A: Action> ActionMap<A> {
    /// Raw look: no smoothing.
    pub fn new(bindings: ActionBindings<A>, look_per_count: f32) -> Self {
        const { assert!(A::ALL.len() <= 64, "frame masks hold at most 64 actions") };
        const { assert!(A::HOLD_TICKS >= 1, "a hold lasts at least one tick") };
        let split = A::ALL
            .iter()
            .flat_map(|&action| bindings.get(action))
            .filter(|binding| binding.trigger == Trigger::Hold)
            .map(|binding| binding.input)
            .collect();
        Self {
            bindings,
            down: HashMap::new(),
            split,
            held: 0,
            pressed: 0,
            released: 0,
            look_accumulator: (0.0, 0.0),
            pending: (0.0, 0.0),
            sticks: [(0.0, 0.0); 2],
            look: LookSettings::raw(look_per_count),
            smoothing: MouseSmoothing::new(),
            pointer: None,
            press_pointer: None,
        }
    }

    pub fn bindings(&self) -> &ActionBindings<A> {
        &self.bindings
    }

    pub fn key(&mut self, key: Key, down: bool) {
        self.set_down(Input::Key(key), down);
    }

    pub fn mouse_button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.press_pointer = self.pointer;
        }
        self.set_down(Input::Mouse(button), down);
    }

    /// Adds to the motion reported by the next `end_tick`.
    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        self.look_accumulator.0 += dx;
        self.look_accumulator.1 += dy;
    }

    /// Records the pointer at `(x, y)` physical pixels.
    pub fn pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer = Some((x, y));
    }

    /// Forgets the pointer: it left the window.
    pub fn pointer_left(&mut self) {
        self.pointer = None;
    }

    pub fn pad_button(&mut self, button: PadButton, down: bool) {
        self.set_down(Input::Pad(PadInput::Button(button)), down);
    }

    /// `value` is in `-1.0..=1.0` with positive Y up.
    pub fn pad_axis(&mut self, stick: Stick, axis: StickAxis, value: f32) {
        let slot = &mut self.sticks[stick as usize];
        match axis {
            StickAxis::X => slot.0 = value,
            StickAxis::Y => slot.1 = value,
        }
        self.sync_stick_directions(stick);
    }

    /// Releases pad bindings only and zeroes stored stick axes.
    pub fn pad_disconnected(&mut self) {
        self.sticks = [(0.0, 0.0); 2];
        self.down.retain(|input, _| !input.is_pad());
        self.refresh_held(0);
    }

    /// Releases every held action, e.g. when the window loses focus.
    pub fn release_all(&mut self) {
        self.sticks = [(0.0, 0.0); 2];
        self.down.clear();
        self.refresh_held(0);
    }

    /// Discards unsampled motion, the pending look and smoothing state so a camera jump
    /// cannot follow.
    pub fn reset_look(&mut self) {
        self.look_accumulator = (0.0, 0.0);
        self.pending = (0.0, 0.0);
        self.smoothing.reset();
    }

    /// Applies new look settings; turning smoothing off drops its carry.
    pub fn set_look(&mut self, settings: LookSettings) {
        if !settings.smoothing {
            self.smoothing.reset();
        }
        self.look = settings;
    }

    /// Samples the raw counts and right stick since the last sample into the pending look.
    ///
    /// The stick adds `PAD_LOOK_PER_SECOND` times its deflection over `frame_dt`.
    pub fn sample_frame(&mut self, frame_dt: Duration) {
        let (x, y) = self.sample_look(frame_dt);
        self.pending.0 += x;
        self.pending.1 += y;
    }

    /// Look sampled by frames but not yet consumed by a tick.
    pub fn pending_look(&self) -> (f32, f32) {
        self.pending
    }

    /// Forgets this tick's presses and releases; held actions and the pending look stay.
    pub fn discard_edges(&mut self) {
        self.pressed = 0;
        self.released = 0;
        self.press_pointer = None;
    }

    /// The only read: reports this tick's edges and look, then starts the next tick.
    pub fn end_tick(&mut self) -> ActionFrame<A> {
        for ticks in self.down.values_mut() {
            *ticks = ticks.saturating_add(1);
        }
        self.refresh_held(0);
        let frame = ActionFrame {
            held: self.held,
            pressed: self.pressed,
            released: self.released,
            look: std::mem::take(&mut self.pending),
            pointer: self.pointer,
            press_pointer: self.press_pointer,
            actions: PhantomData,
        };
        self.discard_edges();
        frame
    }

    fn set_down(&mut self, input: Input, down: bool) {
        if let Some(taps) = self.apply(input, down) {
            self.refresh_held(taps);
        }
    }

    /// Records the change; `None` if nothing changed (OS key repeat), else the `Press`
    /// actions of an input released before its hold matured.
    fn apply(&mut self, input: Input, down: bool) -> Option<u64> {
        if down {
            if self.down.contains_key(&input) {
                return None;
            }
            self.down.insert(input, 0);
            return Some(0);
        }
        let ticks = self.down.remove(&input)?;
        let quick = self.split.contains(&input) && ticks < A::HOLD_TICKS;
        Some(if quick { self.press_actions(input) } else { 0 })
    }

    fn press_actions(&self, input: Input) -> u64 {
        self.actions_where(|b| b.input == input && b.trigger == Trigger::Press)
    }

    fn actions_where(&self, matches: impl Fn(&Binding) -> bool) -> u64 {
        A::ALL
            .iter()
            .enumerate()
            .filter(|&(_, &action)| self.bindings.get(action).iter().any(&matches))
            .fold(0, |mask, (index, _)| mask | 1 << index)
    }

    fn binding_down(&self, input: Input, trigger: Trigger) -> bool {
        self.down.get(&input).is_some_and(|&ticks| match trigger {
            Trigger::Press => !self.split.contains(&input),
            Trigger::Hold => ticks >= A::HOLD_TICKS,
        })
    }

    /// Recomputes the held mask; `taps` are actions that press and release in this tick.
    fn refresh_held(&mut self, taps: u64) {
        let held = self.actions_where(|b| self.binding_down(b.input, b.trigger));
        let taps = taps & !held;
        self.pressed |= (held & !self.held) | taps;
        self.released |= (self.held & !held) | taps;
        self.held = held;
    }

    fn sync_stick_directions(&mut self, stick: Stick) {
        let (x, y) = self.sticks[stick as usize];
        let directions = [
            (StickDir::Up, y >= STICK_DIRECTION_THRESHOLD),
            (StickDir::Down, y <= -STICK_DIRECTION_THRESHOLD),
            (StickDir::Left, x <= -STICK_DIRECTION_THRESHOLD),
            (StickDir::Right, x >= STICK_DIRECTION_THRESHOLD),
        ];
        let mut changes = None;
        for (dir, active) in directions {
            let input = Input::Pad(PadInput::Stick(stick, dir));
            if let Some(taps) = self.apply(input, active) {
                changes = Some(changes.unwrap_or(0) | taps);
            }
        }
        if let Some(taps) = changes {
            self.refresh_held(taps);
        }
    }

    fn sample_look(&mut self, frame_dt: Duration) -> (f32, f32) {
        let (dx, dy) = std::mem::take(&mut self.look_accumulator);
        let dy = if self.look.invert_y { -dy } else { dy };
        let counts = (dx as f32, dy as f32);
        let counts = if self.look.smoothing {
            self.smoothing.apply(counts)
        } else {
            counts
        };
        let mouse = (
            counts.0 * self.look.look_per_count,
            counts.1 * self.look.look_per_count,
        );
        let (stick_x, stick_y) = self.stick_look(frame_dt);
        (mouse.0 + stick_x, mouse.1 + stick_y)
    }

    /// Right-stick look over `frame_dt`; stick up looks up, and mouse dy is down-positive.
    fn stick_look(&self, frame_dt: Duration) -> (f32, f32) {
        let (x, y) = DeadzoneFilter::new(STICK_LOOK_DEAD_ZONE)
            .apply(self.sticks[Stick::RightStick as usize]);
        let scale = PAD_LOOK_PER_SECOND * frame_dt.as_secs_f32() * self.look.look_per_count;
        (x * scale, -y * scale)
    }
}

/// Feeds a window event to the map.
pub fn handle_window_event<A: Action>(map: &mut ActionMap<A>, event: &WindowEvent) {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            if let Some(key) = Key::from_winit(event.physical_key) {
                map.key(key, event.state == ElementState::Pressed);
            }
        }
        WindowEvent::MouseInput { state, button, .. } => {
            if let Some(button) = MouseButton::from_winit(*button) {
                map.mouse_button(button, *state == ElementState::Pressed);
            }
        }
        WindowEvent::CursorMoved { position, .. } => {
            map.pointer_moved(position.x as f32, position.y as f32);
        }
        WindowEvent::CursorLeft { .. } => map.pointer_left(),
        WindowEvent::Focused(false) => map.release_all(),
        _ => {}
    }
}

/// Feeds a device event to the map.
pub fn handle_device_event<A: Action>(map: &mut ActionMap<A>, event: &DeviceEvent) {
    if let DeviceEvent::MouseMotion { delta } = event {
        map.mouse_motion(delta.0, delta.1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::Binding;
    use std::collections::BTreeMap;

    /// A power-of-two fraction of a second, so stick arithmetic is exact in f32.
    const FRAME: Duration = Duration::from_micros(15_625);

    fn stick_per_frame() -> f32 {
        PAD_LOOK_PER_SECOND * FRAME.as_secs_f32()
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum TestAction {
        MoveForward,
        Jump,
        Fire,
    }

    impl Action for TestAction {
        const ALL: &'static [Self] = &[TestAction::MoveForward, TestAction::Jump, TestAction::Fire];

        fn name(self) -> &'static str {
            match self {
                TestAction::MoveForward => "move_forward",
                TestAction::Jump => "jump",
                TestAction::Fire => "fire",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            match self {
                TestAction::MoveForward => &[
                    const { Binding::key(Key::W) },
                    const { Binding::key(Key::ArrowUp) },
                ],
                TestAction::Jump => const { &[Binding::key(Key::Space)] },
                TestAction::Fire => const { &[Binding::mouse(MouseButton::Left)] },
            }
        }
    }

    use crate::pad::{PadInput, StickDir};

    fn pad_map(sensitivity: f32) -> ActionMap<TestAction> {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(
            TestAction::Jump,
            vec![Binding::pad(PadInput::Button(PadButton::South))],
        );
        bindings.set(
            TestAction::MoveForward,
            vec![Binding::pad(PadInput::Stick(
                Stick::LeftStick,
                StickDir::Up,
            ))],
        );
        ActionMap::new(bindings, sensitivity)
    }

    #[test]
    fn pad_button_drives_bound_action() {
        let mut map = pad_map(1.0);

        map.pad_button(PadButton::South, true);
        let frame = map.end_tick();
        let next = map.end_tick();

        assert!(frame.pressed(TestAction::Jump));
        assert!(frame.held(TestAction::Jump));
        assert!(next.held(TestAction::Jump));
        assert!(!next.pressed(TestAction::Jump));
        assert!(!frame.held(TestAction::MoveForward));

        map.pad_button(PadButton::South, false);
        let up = map.end_tick();
        assert!(up.released(TestAction::Jump));
        assert!(!up.held(TestAction::Jump));
    }

    #[test]
    fn unbound_pad_button_changes_nothing() {
        let mut map = pad_map(1.0);

        map.pad_button(PadButton::East, true);
        let frame = map.end_tick();

        for &a in TestAction::ALL {
            assert!(!frame.held(a) && !frame.pressed(a), "{a:?}");
        }
    }

    #[test]
    fn stick_direction_held_past_threshold() {
        let cases = [
            (0.0, false),
            (0.3, false),
            (0.49, false),
            (0.5, true),
            (0.6, true),
            (1.0, true),
            (-0.9, false),
        ];

        for (y, held) in cases {
            let mut map = pad_map(1.0);

            map.pad_axis(Stick::LeftStick, StickAxis::Y, y);
            let frame = map.end_tick();

            assert_eq!(frame.held(TestAction::MoveForward), held, "y = {y}");
            assert_eq!(frame.pressed(TestAction::MoveForward), held, "y = {y}");
        }
    }

    #[test]
    fn stick_direction_releases_when_axis_returns_inside_threshold() {
        let mut map = pad_map(1.0);
        map.pad_axis(Stick::LeftStick, StickAxis::Y, 0.8);
        map.end_tick();

        map.pad_axis(Stick::LeftStick, StickAxis::Y, 0.1);
        let frame = map.end_tick();

        assert!(frame.released(TestAction::MoveForward));
        assert!(!frame.held(TestAction::MoveForward));
    }

    #[test]
    fn each_stick_direction_reads_its_own_axis_and_sign() {
        let cases = [
            (StickDir::Up, StickAxis::Y, 0.8),
            (StickDir::Down, StickAxis::Y, -0.8),
            (StickDir::Left, StickAxis::X, -0.8),
            (StickDir::Right, StickAxis::X, 0.8),
        ];
        let wrong = [
            (StickDir::Up, StickAxis::Y, -0.8),
            (StickDir::Down, StickAxis::Y, 0.8),
            (StickDir::Left, StickAxis::X, 0.8),
            (StickDir::Right, StickAxis::X, -0.8),
            (StickDir::Up, StickAxis::X, 0.8),
            (StickDir::Right, StickAxis::Y, 0.8),
        ];

        for (cases, expected) in [(&cases[..], true), (&wrong[..], false)] {
            for &(dir, axis, value) in cases {
                for stick in [Stick::LeftStick, Stick::RightStick] {
                    let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
                    bindings.set(
                        TestAction::Fire,
                        vec![Binding::pad(PadInput::Stick(stick, dir))],
                    );
                    let mut map = ActionMap::new(bindings, 1.0);

                    map.pad_axis(stick, axis, value);
                    let frame = map.end_tick();

                    assert_eq!(
                        frame.held(TestAction::Fire),
                        expected,
                        "{stick:?} {dir:?} {axis:?} {value}"
                    );
                }
            }
        }
    }

    #[test]
    fn right_stick_adds_look_per_sampled_frame() {
        for smoothing in [true, false] {
            let mut map = pad_map(2.0);
            map.set_look(LookSettings {
                look_per_count: 2.0,
                smoothing,
                invert_y: false,
            });

            map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
            map.sample_frame(FRAME);
            let first = map.end_tick().look();
            map.sample_frame(FRAME);
            let second = map.end_tick().look();

            let expected = stick_per_frame() * 2.0;
            assert_close(first, (expected, 0.0));
            assert_close(second, (expected, 0.0));
        }
    }

    #[test]
    fn right_stick_up_looks_up_and_scales_with_deflection() {
        let mut map = pad_map(1.0);

        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.5);
        map.sample_frame(FRAME);
        let look = map.end_tick().look();

        assert_close(look, (0.0, -0.5 * stick_per_frame()));
    }

    #[test]
    fn right_stick_inside_dead_zone_adds_no_look() {
        let mut map = pad_map(1.0);

        map.pad_axis(Stick::RightStick, StickAxis::X, 0.1);
        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.1);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn right_stick_dead_zone_is_radial_not_per_axis() {
        let mut map = pad_map(1.0);

        map.pad_axis(Stick::RightStick, StickAxis::X, 0.12);
        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.12);
        map.sample_frame(FRAME);
        let look = map.end_tick().look();

        assert_close(look, (0.12 * stick_per_frame(), -0.12 * stick_per_frame()));
    }

    #[test]
    fn left_stick_does_not_add_look() {
        let mut map = pad_map(1.0);

        map.pad_axis(Stick::LeftStick, StickAxis::X, 1.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn release_all_stops_stick_look() {
        let mut map = pad_map(1.0);
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        map.sample_frame(FRAME);
        assert_close(map.end_tick().look(), (stick_per_frame(), 0.0));

        map.release_all();
        map.sample_frame(FRAME);
        let frame = map.end_tick();
        map.sample_frame(FRAME);
        let next = map.end_tick();

        assert_eq!(frame.look(), (0.0, 0.0));
        assert_eq!(next.look(), (0.0, 0.0));
    }

    #[test]
    fn resting_or_slightly_deflected_stick_holds_no_direction() {
        for stick in [Stick::LeftStick, Stick::RightStick] {
            for value in [0.0, 0.3, -0.3] {
                let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
                let actions = [TestAction::MoveForward, TestAction::Jump, TestAction::Fire];
                bindings.set(
                    TestAction::MoveForward,
                    vec![Binding::pad(PadInput::Stick(stick, StickDir::Up))],
                );
                bindings.set(
                    TestAction::Jump,
                    vec![
                        Binding::pad(PadInput::Stick(stick, StickDir::Down)),
                        Binding::pad(PadInput::Stick(stick, StickDir::Left)),
                    ],
                );
                bindings.set(
                    TestAction::Fire,
                    vec![Binding::pad(PadInput::Stick(stick, StickDir::Right))],
                );
                let mut map = ActionMap::new(bindings, 1.0);

                map.pad_axis(stick, StickAxis::X, value);
                map.pad_axis(stick, StickAxis::Y, value);
                let frame = map.end_tick();

                for action in actions {
                    assert!(!frame.held(action), "{stick:?} at {value}: {action:?}");
                }
            }
        }
    }

    #[test]
    fn pad_disconnect_releases_pad_bindings() {
        let mut map = pad_map(1.0);
        map.pad_button(PadButton::South, true);
        map.pad_axis(Stick::LeftStick, StickAxis::Y, 1.0);
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        map.end_tick();

        map.pad_disconnected();
        map.sample_frame(FRAME);
        let frame = map.end_tick();
        map.sample_frame(FRAME);
        let next = map.end_tick();

        assert!(frame.released(TestAction::Jump));
        assert!(frame.released(TestAction::MoveForward));
        assert!(!frame.held(TestAction::Jump));
        assert!(!frame.held(TestAction::MoveForward));
        assert_eq!(frame.look(), (0.0, 0.0));
        assert_eq!(next.look(), (0.0, 0.0));
        assert!(!next.released(TestAction::Jump));
    }

    #[test]
    fn pad_disconnect_leaves_a_held_key_on_the_same_action() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(
            TestAction::Jump,
            vec![
                Binding::key(Key::Space),
                Binding::pad(PadInput::Button(PadButton::South)),
            ],
        );
        let mut map = ActionMap::new(bindings, 1.0);
        map.key(Key::Space, true);
        map.pad_button(PadButton::South, true);
        map.end_tick();

        map.pad_disconnected();
        let frame = map.end_tick();

        assert!(frame.held(TestAction::Jump));
        assert!(!frame.released(TestAction::Jump));

        map.key(Key::Space, false);
        let done = map.end_tick();
        assert!(!done.held(TestAction::Jump));
        assert!(done.released(TestAction::Jump));
    }

    fn map(sensitivity: f32) -> ActionMap<TestAction> {
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        ActionMap::new(bindings, sensitivity)
    }

    #[test]
    fn held_key_holds_action_and_presses_once() {
        let mut map = map(1.0);

        map.key(Key::W, true);
        let first = map.end_tick();
        let second = map.end_tick();

        assert!(first.held(TestAction::MoveForward));
        assert!(second.held(TestAction::MoveForward));
        assert!(first.pressed(TestAction::MoveForward));
        assert!(!second.pressed(TestAction::MoveForward));
        assert!(!first.released(TestAction::MoveForward));
        assert!(!first.held(TestAction::Jump));
    }

    #[test]
    fn tap_inside_one_tick_reports_press_and_release() {
        let mut map = map(1.0);

        map.key(Key::Space, true);
        map.key(Key::Space, false);
        let frame = map.end_tick();
        let next = map.end_tick();

        assert!(frame.pressed(TestAction::Jump));
        assert!(frame.released(TestAction::Jump));
        assert!(!frame.held(TestAction::Jump));
        assert!(!next.pressed(TestAction::Jump));
        assert!(!next.released(TestAction::Jump));
    }

    #[test]
    fn release_edge_reported_on_the_tick_the_key_goes_up() {
        let mut map = map(1.0);
        map.key(Key::Space, true);
        map.end_tick();

        map.key(Key::Space, false);
        let frame = map.end_tick();

        assert!(frame.released(TestAction::Jump));
        assert!(!frame.held(TestAction::Jump));
        assert!(!frame.pressed(TestAction::Jump));
    }

    #[test]
    fn mouse_button_drives_bound_action() {
        let mut map = map(1.0);

        map.mouse_button(MouseButton::Left, true);
        let frame = map.end_tick();

        assert!(frame.pressed(TestAction::Fire));
        assert!(frame.held(TestAction::Fire));
        assert!(!frame.pressed(TestAction::Jump));
    }

    #[test]
    fn unbound_key_and_button_change_nothing() {
        let mut map = map(1.0);

        map.key(Key::Q, true);
        map.mouse_button(MouseButton::Right, true);
        let frame = map.end_tick();

        for &a in TestAction::ALL {
            assert!(!frame.held(a) && !frame.pressed(a), "{a:?}");
        }
    }

    #[test]
    fn look_is_scaled_motion_since_previous_tick() {
        let mut map = map(2.0);

        map.mouse_motion(1.0, 0.0);
        map.mouse_motion(2.0, 1.0);
        map.sample_frame(FRAME);
        let frame = map.end_tick();
        map.sample_frame(FRAME);
        let next = map.end_tick();

        assert_eq!(frame.look(), (6.0, 2.0));
        assert_eq!(next.look(), (0.0, 0.0));
    }

    #[test]
    fn set_look_changes_later_look() {
        let mut map = map(1.0);
        map.set_look(LookSettings::raw(3.0));

        map.mouse_motion(1.0, 2.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (3.0, 6.0));
    }

    #[test]
    fn reset_look_discards_pending_motion() {
        let mut map = map(1.0);
        map.mouse_motion(5.0, 5.0);

        map.reset_look();
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    fn smoothed(look_per_count: f32) -> ActionMap<TestAction> {
        let mut map = map(look_per_count);
        map.set_look(LookSettings {
            look_per_count,
            smoothing: true,
            invert_y: false,
        });
        map
    }

    #[test]
    fn mouse_counts_turn_in_proportion_at_any_speed() {
        let rows = [(1u32, 1u32), (4, 4), (300, 1), (300, 60)];

        for (counts, ticks) in rows {
            let mut map = map(0.5);
            let per_tick = f64::from(counts / ticks);
            let expected_tick = (per_tick as f32 * 0.5, -(per_tick as f32) * 0.5);
            let mut total = (0.0f32, 0.0f32);

            for tick in 0..ticks {
                map.mouse_motion(per_tick, -per_tick);
                map.sample_frame(FRAME);
                let look = map.end_tick().look();
                assert_eq!(
                    look, expected_tick,
                    "{counts} counts over {ticks} ticks, tick {tick}"
                );
                total = (total.0 + look.0, total.1 + look.1);
            }

            let expected = counts as f32 * 0.5;
            assert_eq!(
                total,
                (expected, -expected),
                "{counts} counts over {ticks} ticks"
            );
        }
    }

    #[test]
    fn motion_smaller_than_one_count_still_turns() {
        let mut map = map(1.0);

        map.mouse_motion(0.005, 0.0);
        map.mouse_motion(0.0, 0.005);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (0.005, 0.005));
    }

    #[test]
    fn smoothing_spreads_a_motion_without_losing_any() {
        let mut map = smoothed(0.5);

        map.mouse_motion(100.0, -100.0);
        map.sample_frame(FRAME);
        let first = map.end_tick().look();
        map.sample_frame(FRAME);
        let second = map.end_tick().look();
        map.sample_frame(FRAME);
        let third = map.end_tick().look();

        assert!(first.0 > 0.0 && first.0 < 50.0, "first = {first:?}");
        assert!(first.1 < 0.0 && first.1 > -50.0, "first = {first:?}");
        assert_eq!(first.0 + second.0 + third.0, 50.0);
        assert_eq!(first.1 + second.1 + third.1, -50.0);
    }

    #[test]
    fn smoothing_scales_the_smoothed_counts_by_look_per_count() {
        let mut map = smoothed(2.0);

        map.mouse_motion(10.0, 4.0);
        map.sample_frame(FRAME);
        let first = map.end_tick().look();
        map.sample_frame(FRAME);
        let second = map.end_tick().look();

        assert_eq!(first, (10.0, 4.0));
        assert_eq!(second, (10.0, 4.0));
    }

    #[test]
    fn stick_look_is_added_after_the_smoothed_mouse() {
        let mut map = pad_map(1.0);
        map.set_look(LookSettings {
            look_per_count: 1.0,
            smoothing: true,
            invert_y: false,
        });
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        map.mouse_motion(2.0, 0.0);
        map.sample_frame(FRAME);

        let look = map.end_tick().look();

        assert_close(look, (1.0 + stick_per_frame(), 0.0));
    }

    #[test]
    fn reset_look_discards_the_smoothing_carry() {
        let mut map = smoothed(1.0);
        map.mouse_motion(10.0, 10.0);
        map.sample_frame(FRAME);
        map.end_tick();

        map.reset_look();
        map.sample_frame(FRAME);
        let after = map.end_tick().look();

        assert_eq!(after, (0.0, 0.0));
    }

    #[test]
    fn turning_smoothing_off_drops_the_carry() {
        let mut map = smoothed(1.0);
        map.mouse_motion(10.0, 10.0);
        map.sample_frame(FRAME);
        map.end_tick();

        map.set_look(LookSettings::raw(1.0));
        map.set_look(LookSettings {
            look_per_count: 1.0,
            smoothing: true,
            invert_y: false,
        });
        map.sample_frame(FRAME);
        let after = map.end_tick().look();

        assert_eq!(after, (0.0, 0.0));
    }

    #[test]
    fn turning_smoothing_off_makes_look_raw_again() {
        let mut map = smoothed(1.0);
        map.mouse_motion(10.0, 0.0);
        map.sample_frame(FRAME);
        map.end_tick();

        map.set_look(LookSettings::raw(1.0));
        map.mouse_motion(3.0, 0.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (3.0, 0.0));
    }

    #[test]
    fn changing_sensitivity_while_smoothing_keeps_the_carry() {
        let mut map = smoothed(1.0);
        map.mouse_motion(10.0, 0.0);
        map.sample_frame(FRAME);
        map.end_tick();

        map.set_look(LookSettings {
            look_per_count: 2.0,
            smoothing: true,
            invert_y: false,
        });
        map.sample_frame(FRAME);
        let after = map.end_tick().look();

        assert_eq!(after, (10.0, 0.0));
    }

    #[test]
    fn release_all_releases_held_actions() {
        let mut map = map(1.0);
        map.key(Key::W, true);
        map.end_tick();

        map.release_all();
        let frame = map.end_tick();

        assert!(frame.released(TestAction::MoveForward));
        assert!(!frame.held(TestAction::MoveForward));
        assert!(!map.end_tick().released(TestAction::MoveForward));
    }

    #[test]
    fn key_up_after_release_all_does_not_rerelease() {
        let mut map = map(1.0);
        map.key(Key::W, true);
        map.end_tick();
        map.release_all();
        map.end_tick();

        map.key(Key::W, false);
        let frame = map.end_tick();

        assert!(!frame.released(TestAction::MoveForward));
        assert!(!frame.held(TestAction::MoveForward));
    }

    #[test]
    fn one_key_bound_to_two_actions_drives_both() {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(TestAction::Jump, vec![Binding::key(Key::W)]);
        let mut map = ActionMap::new(bindings, 1.0);

        map.key(Key::W, true);
        let frame = map.end_tick();

        assert!(frame.held(TestAction::MoveForward));
        assert!(frame.held(TestAction::Jump));
        assert!(!frame.held(TestAction::Fire));
    }

    #[test]
    fn action_held_while_any_binding_is_down() {
        let mut map = map(1.0);
        map.key(Key::W, true);
        map.key(Key::ArrowUp, true);
        map.end_tick();

        map.key(Key::W, false);
        let still = map.end_tick();
        map.key(Key::ArrowUp, false);
        let done = map.end_tick();

        assert!(still.held(TestAction::MoveForward));
        assert!(!still.released(TestAction::MoveForward));
        assert!(!done.held(TestAction::MoveForward));
        assert!(done.released(TestAction::MoveForward));
    }

    #[test]
    fn second_binding_down_does_not_press_again() {
        let mut map = map(1.0);
        map.key(Key::W, true);
        map.end_tick();

        map.key(Key::ArrowUp, true);
        let frame = map.end_tick();

        assert!(frame.held(TestAction::MoveForward));
        assert!(!frame.pressed(TestAction::MoveForward));
    }

    #[test]
    fn frame_masks_index_by_action_position() {
        let keys = [Key::W, Key::Space, Key::Q];
        let expected = [TestAction::MoveForward, TestAction::Jump, TestAction::Fire];

        for subset in 0u8..8 {
            let mut map = map(1.0);
            let mut want = Vec::new();
            for (i, &action) in expected.iter().enumerate() {
                if subset & (1 << i) == 0 {
                    continue;
                }
                match action {
                    TestAction::Fire => map.mouse_button(MouseButton::Left, true),
                    _ => map.key(keys[i], true),
                }
                want.push(action);
            }

            let frame = map.end_tick();

            for &a in TestAction::ALL {
                assert_eq!(frame.held(a), want.contains(&a), "{subset:03b} {a:?}");
                assert_eq!(frame.pressed(a), want.contains(&a), "{subset:03b} {a:?}");
            }
        }
    }

    #[test]
    fn focus_lost_event_releases_all() {
        let mut map = map(1.0);
        map.key(Key::W, true);
        map.end_tick();

        handle_window_event(&mut map, &WindowEvent::Focused(false));
        let frame = map.end_tick();

        assert!(frame.released(TestAction::MoveForward));
        assert!(!frame.held(TestAction::MoveForward));
    }

    #[test]
    fn focus_gained_event_leaves_held_actions() {
        let mut map = map(1.0);
        map.key(Key::W, true);
        map.end_tick();

        handle_window_event(&mut map, &WindowEvent::Focused(true));
        let frame = map.end_tick();

        assert!(frame.held(TestAction::MoveForward));
    }

    #[test]
    fn mouse_motion_device_event_feeds_look() {
        let mut map = map(1.0);

        handle_device_event(&mut map, &DeviceEvent::MouseMotion { delta: (3.0, -1.0) });
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (3.0, -1.0));
    }

    #[test]
    fn unfiltered_horizontal_only_motion_scales_x_and_leaves_y_zero() {
        let mut map = map(2.0);

        map.mouse_motion(3.0, 0.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (6.0, 0.0));
    }

    #[test]
    fn unfiltered_vertical_only_motion_scales_y_and_leaves_x_zero() {
        let mut map = map(2.0);

        map.mouse_motion(0.0, 3.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (0.0, 6.0));
    }

    #[test]
    fn pointer_moves_reach_the_next_frame() {
        let mut map = map(1.0);

        map.pointer_moved(10.0, 20.0);
        map.pointer_moved(30.5, 40.5);
        let frame = map.end_tick();
        let next = map.end_tick();
        map.pointer_left();
        let gone = map.end_tick();

        assert_eq!(frame.pointer(), Some((30.5, 40.5)));
        assert_eq!(next.pointer(), Some((30.5, 40.5)));
        assert_eq!(gone.pointer(), None);
    }

    #[test]
    fn press_records_the_pointer_at_the_press() {
        let mut map = map(1.0);

        map.pointer_moved(100.0, 200.0);
        map.mouse_button(MouseButton::Left, true);
        map.pointer_moved(300.0, 400.0);
        map.mouse_button(MouseButton::Left, false);
        map.pointer_moved(500.0, 600.0);
        map.mouse_button(MouseButton::Left, true);
        map.pointer_moved(700.0, 800.0);
        map.mouse_button(MouseButton::Left, false);
        map.pointer_moved(900.0, 1000.0);
        let frame = map.end_tick();

        assert_eq!(frame.press_pointer(), Some((500.0, 600.0)));
        assert_eq!(frame.pointer(), Some((900.0, 1000.0)));
    }

    #[test]
    fn press_without_a_pointer_has_no_press_pointer() {
        let mut before_any_move = map(1.0);
        before_any_move.mouse_button(MouseButton::Left, true);
        let mut after_leaving = map(1.0);
        after_leaving.pointer_moved(10.0, 20.0);
        after_leaving.pointer_left();
        after_leaving.mouse_button(MouseButton::Left, true);

        let first = before_any_move.end_tick();
        let second = after_leaving.end_tick();

        assert!(first.pressed(TestAction::Fire));
        assert!(second.pressed(TestAction::Fire));
        assert_eq!(first.press_pointer(), None);
        assert_eq!(second.press_pointer(), None);

        after_leaving.pointer_moved(30.0, 40.0);
        let later = after_leaving.end_tick();
        assert_eq!(later.pointer(), Some((30.0, 40.0)));
        assert_eq!(later.press_pointer(), None);
    }

    #[test]
    fn no_press_means_no_press_pointer() {
        let mut map = map(1.0);

        map.pointer_moved(5.0, 6.0);
        map.key(Key::W, true);
        let frame = map.end_tick();
        map.mouse_button(MouseButton::Left, true);
        map.end_tick();
        let later = map.end_tick();

        assert_eq!(frame.pointer(), Some((5.0, 6.0)));
        assert_eq!(frame.press_pointer(), None);
        assert_eq!(later.press_pointer(), None);
    }

    fn assert_close(actual: (f32, f32), expected: (f32, f32)) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-6 && (actual.1 - expected.1).abs() < 1e-6,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn test_input_accumulation() {
        let mut map = map(1.0);

        map.mouse_motion(1.0, 0.0);
        map.mouse_motion(1.0, 1.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (2.0, 1.0));
        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn test_sensitivity_scaling() {
        let mut map = map(2.0);

        map.mouse_motion(1.0, 3.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (2.0, 6.0));
    }

    #[test]
    fn reset_look_discards_accumulated_delta() {
        let mut dirty = map(1.0);
        dirty.mouse_motion(1.0, 0.0);
        dirty.sample_frame(FRAME);
        dirty.end_tick();
        dirty.mouse_motion(5.0, 5.0);
        dirty.reset_look();
        let mut fresh = map(1.0);

        dirty.mouse_motion(1.0, 0.0);
        fresh.mouse_motion(1.0, 0.0);
        dirty.sample_frame(FRAME);
        fresh.sample_frame(FRAME);

        assert_eq!(dirty.end_tick().look(), fresh.end_tick().look());
    }

    #[test]
    fn stick_look_grows_with_each_frame() {
        let mut map = pad_map(1.0);
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        let mut yaws = Vec::new();

        for _ in 0..4 {
            map.sample_frame(FRAME);
            yaws.push(map.pending_look().0);
        }

        assert!(yaws.windows(2).all(|pair| pair[1] > pair[0]), "{yaws:?}");
        // `PAD_LOOK_PER_SECOND` look units per second for 4 frames of 15.625 ms at sensitivity 1.
        assert_close((yaws[3], 0.0), (PAD_LOOK_PER_SECOND * 0.0625, 0.0));
    }

    #[test]
    fn stick_look_scales_with_frame_time() {
        let mut slow = pad_map(1.0);
        let mut fast = pad_map(1.0);
        slow.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        fast.pad_axis(Stick::RightStick, StickAxis::X, 1.0);

        slow.sample_frame(FRAME * 2);
        fast.sample_frame(FRAME);

        assert_close(slow.pending_look(), (2.0 * fast.pending_look().0, 0.0));
        assert!(fast.pending_look().0 > 0.0);
    }

    #[test]
    fn pending_look_is_zero_before_any_sample() {
        let mut map = map(1.0);

        map.mouse_motion(4.0, 4.0);

        assert_eq!(map.pending_look(), (0.0, 0.0));
    }

    #[test]
    fn frames_without_a_tick_accumulate_pending_look() {
        let mut map = map(2.0);

        map.mouse_motion(1.0, 2.0);
        map.sample_frame(FRAME);
        let first = map.pending_look();
        map.mouse_motion(3.0, 4.0);
        map.sample_frame(FRAME);

        assert_eq!(first, (2.0, 4.0));
        assert_eq!(map.pending_look(), (8.0, 12.0));
    }

    #[test]
    fn end_tick_consumes_exactly_the_pending_look() {
        let mut map = map(2.0);
        map.mouse_motion(1.0, 1.0);
        map.sample_frame(FRAME);
        map.mouse_motion(2.0, 0.0);
        map.sample_frame(FRAME);
        let shown = map.pending_look();

        let frame = map.end_tick();

        assert_eq!(frame.look(), shown);
        assert_eq!(frame.look(), (6.0, 2.0));
        assert_eq!(map.pending_look(), (0.0, 0.0));
    }

    #[test]
    fn end_tick_consumes_exactly_the_pending_look_with_smoothing() {
        let mut map = smoothed(0.5);
        map.mouse_motion(100.0, -40.0);
        map.sample_frame(FRAME);
        map.sample_frame(FRAME);
        map.mouse_motion(7.0, 3.0);
        map.sample_frame(FRAME);
        let shown = map.pending_look();

        let frame = map.end_tick();

        assert_eq!(frame.look(), shown);
        assert!(shown.0 > 0.0);
    }

    #[test]
    fn motion_after_the_last_frame_waits_for_the_next_frame() {
        let mut map = map(1.0);
        map.mouse_motion(2.0, 0.0);
        map.sample_frame(FRAME);
        map.mouse_motion(5.0, 0.0);

        let tick = map.end_tick();
        map.sample_frame(FRAME);
        let next = map.end_tick();

        assert_eq!(tick.look(), (2.0, 0.0));
        assert_eq!(next.look(), (5.0, 0.0));
    }

    #[test]
    fn catch_up_ticks_after_the_first_get_no_look() {
        let mut map = map(1.0);
        map.mouse_motion(3.0, 1.0);
        map.sample_frame(FRAME);

        let first = map.end_tick();
        let second = map.end_tick();
        let third = map.end_tick();

        assert_eq!(first.look(), (3.0, 1.0));
        assert_eq!(second.look(), (0.0, 0.0));
        assert_eq!(third.look(), (0.0, 0.0));
    }

    #[test]
    fn reset_look_discards_pending_look() {
        let mut map = map(1.0);
        map.mouse_motion(5.0, 5.0);
        map.sample_frame(FRAME);
        map.mouse_motion(1.0, 1.0);

        map.reset_look();

        assert_eq!(map.pending_look(), (0.0, 0.0));
        map.sample_frame(FRAME);
        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn source_sensitivity_turns_like_source() {
        let rows = [
            (1.0f32, 1000.0f64, 22.0f32),
            (3.0, 1000.0, 66.0),
            (2.5, 400.0, 22.0),
        ];

        for (sensitivity, counts, degrees) in rows {
            let mut map = map(source_look_per_count(sensitivity));

            map.mouse_motion(counts, 0.0);
            map.sample_frame(FRAME);
            let (yaw, pitch) = map.end_tick().look();

            let turned = yaw.to_degrees();
            assert!(
                (turned - degrees).abs() < 1e-3,
                "sensitivity {sensitivity}, {counts} counts: turned {turned} degrees, want {degrees}"
            );
            assert_eq!(pitch, 0.0, "sensitivity {sensitivity}");
        }
    }

    #[test]
    fn source_sensitivity_scales_vertical_counts_the_same() {
        let mut map = map(source_look_per_count(3.0));

        map.mouse_motion(0.0, 1000.0);
        map.sample_frame(FRAME);
        let (yaw, pitch) = map.end_tick().look();

        assert_eq!(yaw, 0.0);
        assert!((pitch.to_degrees() - 66.0).abs() < 1e-3, "pitch {pitch}");
    }

    #[test]
    fn source_degrees_per_count_is_point_zero_two_two() {
        assert_eq!(SOURCE_DEGREES_PER_COUNT, 0.022);
        assert!((source_look_per_count(1.0).to_degrees() - 0.022).abs() < 1e-6);
    }

    fn inverted_map(look_per_count: f32) -> ActionMap<TestAction> {
        let mut map = pad_map(look_per_count);
        map.set_look(LookSettings {
            invert_y: true,
            ..LookSettings::raw(look_per_count)
        });
        map
    }

    #[test]
    fn invert_y_flips_mouse_pitch() {
        let mut normal = map(1.0);
        let mut inverted = inverted_map(1.0);
        for map in [&mut normal, &mut inverted] {
            map.mouse_motion(2.0, -10.0);
            map.sample_frame(FRAME);
        }

        let normal_look = normal.end_tick().look();
        let inverted_look = inverted.end_tick().look();

        assert_eq!(normal_look, (2.0, -10.0), "mouse up pitches up by default");
        assert_eq!(inverted_look, (2.0, 10.0), "mouse up pitches down inverted");
    }

    #[test]
    fn invert_y_flips_mouse_down_to_pitch_up() {
        let mut map = inverted_map(2.0);

        map.mouse_motion(0.0, 3.0);
        map.sample_frame(FRAME);

        assert_eq!(map.end_tick().look(), (0.0, -6.0));
    }

    #[test]
    fn invert_y_flips_smoothed_mouse_pitch_on_every_tick() {
        let mut normal = smoothed(0.5);
        let mut inverted = smoothed(0.5);
        inverted.set_look(LookSettings {
            look_per_count: 0.5,
            smoothing: true,
            invert_y: true,
        });
        for map in [&mut normal, &mut inverted] {
            map.mouse_motion(40.0, -100.0);
        }

        for _ in 0..3 {
            normal.sample_frame(FRAME);
            inverted.sample_frame(FRAME);
            let want = normal.end_tick().look();
            let got = inverted.end_tick().look();
            assert_eq!(got, (want.0, -want.1));
        }
    }

    #[test]
    fn invert_y_leaves_stick_pitch() {
        let mut map = inverted_map(1.0);

        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.5);
        map.sample_frame(FRAME);
        let look = map.end_tick().look();

        assert_close(look, (0.0, -0.5 * stick_per_frame()));
    }

    #[test]
    fn invert_y_flips_only_the_mouse_share_of_mixed_look() {
        let mut map = inverted_map(1.0);

        map.pad_axis(Stick::RightStick, StickAxis::Y, 1.0);
        map.mouse_motion(0.0, -10.0);
        map.sample_frame(FRAME);
        let look = map.end_tick().look();

        assert_close(look, (0.0, 10.0 - stick_per_frame()));
    }

    #[test]
    fn invert_y_off_by_default_in_raw_settings() {
        assert!(!LookSettings::raw(1.0).invert_y);
    }

    #[test]
    fn full_stick_turns_half_a_turn_per_second_at_default_sensitivity() {
        let mut map = pad_map(source_look_per_count(3.0));
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);

        // 64 frames of 15.625 ms make exactly one second.
        for _ in 0..64 {
            map.sample_frame(FRAME);
        }
        let (yaw, _) = map.end_tick().look();

        assert!(
            (yaw - std::f32::consts::PI).abs() < 0.01,
            "turned {} degrees in one second",
            yaw.to_degrees()
        );
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_turn_total_is_independent_of_frame_split(
                counts in proptest::collection::vec(-40i32..40, 1..24),
                frames_per_tick in proptest::collection::vec(1usize..5, 1..6),
            ) {
                let mut whole = map(1.0);
                for &count in &counts {
                    whole.mouse_motion(f64::from(count), 0.0);
                }
                whole.sample_frame(FRAME);
                let expected = whole.end_tick().look();

                let mut split = map(1.0);
                let mut total = (0.0f32, 0.0f32);
                let mut feed = counts.iter();
                for &frames in &frames_per_tick {
                    for _ in 0..frames {
                        if let Some(&count) = feed.next() {
                            split.mouse_motion(f64::from(count), 0.0);
                        }
                        split.sample_frame(FRAME);
                    }
                    let look = split.end_tick().look();
                    total = (total.0 + look.0, total.1 + look.1);
                }
                for &count in feed {
                    split.mouse_motion(f64::from(count), 0.0);
                }
                split.sample_frame(FRAME);
                let look = split.end_tick().look();
                total = (total.0 + look.0, total.1 + look.1);

                prop_assert_eq!(total, expected);
            }
        }
    }

    #[test]
    fn discard_edges_keeps_held_and_pending_look() {
        let mut map = map(2.0);
        map.key(Key::W, true);
        map.key(Key::ArrowUp, true);
        map.key(Key::ArrowUp, false);
        map.key(Key::Space, true);
        map.key(Key::Space, false);
        map.pointer_moved(10.0, 20.0);
        map.mouse_button(MouseButton::Left, true);
        map.mouse_motion(3.0, -1.0);
        map.sample_frame(FRAME);
        let pending = map.pending_look();

        map.discard_edges();
        let frame = map.end_tick();

        assert_eq!(pending, (6.0, -2.0));
        for action in [TestAction::MoveForward, TestAction::Jump, TestAction::Fire] {
            assert!(!frame.pressed(action), "{action:?} pressed");
            assert!(!frame.released(action), "{action:?} released");
        }
        assert!(frame.held(TestAction::MoveForward), "W is still down");
        assert!(
            frame.held(TestAction::Fire),
            "the mouse button is still down"
        );
        assert!(!frame.held(TestAction::Jump));
        assert_eq!(frame.look(), pending);
        assert_eq!(frame.press_pointer(), None);
        assert_eq!(frame.pointer(), Some((10.0, 20.0)));
    }

    use proptest::prelude::*;

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum Weapon {
        Reload,
        Check,
    }

    impl Action for Weapon {
        const ALL: &'static [Self] = &[Weapon::Reload, Weapon::Check];

        fn name(self) -> &'static str {
            match self {
                Weapon::Reload => "reload",
                Weapon::Check => "check",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            match self {
                Weapon::Reload => const { &[Binding::key(Key::R)] },
                Weapon::Check => const { &[Binding::key(Key::R).held()] },
            }
        }
    }

    /// Same shape as `Weapon`, with a longer hold threshold.
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum Slow {
        Reload,
        Check,
    }

    impl Action for Slow {
        const ALL: &'static [Self] = &[Slow::Reload, Slow::Check];
        const HOLD_TICKS: u32 = 30;

        fn name(self) -> &'static str {
            match self {
                Slow::Reload => "reload",
                Slow::Check => "check",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            match self {
                Slow::Reload => const { &[Binding::key(Key::R)] },
                Slow::Check => const { &[Binding::key(Key::R).held()] },
            }
        }
    }

    const HOLD: usize = crate::bindings::DEFAULT_HOLD_TICKS as usize;

    fn weapon_map() -> ActionMap<Weapon> {
        let (bindings, warnings) = ActionBindings::<Weapon>::load(&BTreeMap::new());
        assert_eq!(warnings, vec![]);
        ActionMap::new(bindings, 1.0)
    }

    fn ticks<A: Action>(map: &mut ActionMap<A>, count: usize) -> Vec<ActionFrame<A>> {
        (0..count).map(|_| map.end_tick()).collect()
    }

    #[test]
    fn tap_on_a_split_key_presses_on_release() {
        for down_ticks in [5, 17] {
            let mut map = weapon_map();
            map.key(Key::R, true);
            let down = ticks(&mut map, down_ticks);

            map.key(Key::R, false);
            let up = map.end_tick();

            for (index, frame) in down.iter().enumerate() {
                for action in Weapon::ALL {
                    assert!(
                        !frame.held(*action),
                        "{action:?} held in tick {index} of {down_ticks}"
                    );
                    assert!(
                        !frame.pressed(*action),
                        "{action:?} pressed in tick {index} of {down_ticks}"
                    );
                }
            }
            assert!(up.pressed(Weapon::Reload), "{down_ticks} ticks down");
            assert!(up.released(Weapon::Reload), "{down_ticks} ticks down");
            assert!(!up.held(Weapon::Reload), "{down_ticks} ticks down");
            assert!(!up.pressed(Weapon::Check), "{down_ticks} ticks down");
            assert!(!up.held(Weapon::Check), "{down_ticks} ticks down");
        }
    }

    #[test]
    fn release_in_the_tick_after_maturing_does_not_tap() {
        let mut map = weapon_map();
        map.key(Key::R, true);
        let down = ticks(&mut map, 18);

        map.key(Key::R, false);
        let up = map.end_tick();

        assert!(down[17].pressed(Weapon::Check), "pressed in tick 18");
        assert!(up.released(Weapon::Check));
        assert!(!up.held(Weapon::Check));
        assert!(!up.pressed(Weapon::Reload));
        assert!(!up.released(Weapon::Reload));
    }

    #[test]
    fn press_and_release_in_one_tick_still_taps() {
        let mut map = weapon_map();

        map.key(Key::R, true);
        map.key(Key::R, false);
        let frame = map.end_tick();
        let next = map.end_tick();

        assert!(frame.pressed(Weapon::Reload));
        assert!(frame.released(Weapon::Reload));
        assert!(!frame.held(Weapon::Reload));
        assert!(!frame.pressed(Weapon::Check));
        assert!(!next.pressed(Weapon::Reload));
        assert!(!next.released(Weapon::Reload));
    }

    #[test]
    fn hold_presses_in_the_tick_it_matures() {
        assert_eq!(HOLD, 18, "the engine default threshold");
        let mut map = weapon_map();
        map.key(Key::R, true);

        let frames = ticks(&mut map, HOLD + 1);

        for (index, frame) in frames.iter().enumerate() {
            let tick = index + 1;
            assert_eq!(frame.pressed(Weapon::Check), tick == HOLD, "tick {tick}");
            assert_eq!(frame.held(Weapon::Check), tick >= HOLD, "tick {tick}");
            assert!(!frame.pressed(Weapon::Reload), "tick {tick}");
            assert!(!frame.held(Weapon::Reload), "tick {tick}");
        }
    }

    #[test]
    fn hold_action_stays_held_until_release() {
        let mut map = weapon_map();
        map.key(Key::R, true);
        let frames = ticks(&mut map, 30);

        map.key(Key::R, false);
        let up = map.end_tick();

        for (index, frame) in frames.iter().enumerate() {
            let tick = index + 1;
            assert_eq!(frame.held(Weapon::Check), tick >= HOLD, "tick {tick}");
            assert!(!frame.pressed(Weapon::Reload), "tick {tick}");
            assert!(!frame.released(Weapon::Check), "tick {tick}");
        }
        assert!(up.released(Weapon::Check));
        assert!(!up.held(Weapon::Check));
        assert!(!up.pressed(Weapon::Reload), "a matured hold is not a tap");
        assert!(!up.released(Weapon::Reload));
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum StickWeapon {
        Reload,
        Check,
    }

    impl Action for StickWeapon {
        const ALL: &'static [Self] = &[StickWeapon::Reload, StickWeapon::Check];

        fn name(self) -> &'static str {
            match self {
                StickWeapon::Reload => "reload",
                StickWeapon::Check => "check",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            const UP: PadInput = PadInput::Stick(Stick::LeftStick, StickDir::Up);
            match self {
                StickWeapon::Reload => const { &[Binding::pad(UP)] },
                StickWeapon::Check => const { &[Binding::pad(UP).held()] },
            }
        }
    }

    #[test]
    fn hold_works_on_a_stick_direction() {
        let (bindings, warnings) = ActionBindings::<StickWeapon>::load(&BTreeMap::new());
        assert_eq!(warnings, vec![]);
        let mut map = ActionMap::new(bindings, 1.0);

        map.pad_axis(Stick::LeftStick, StickAxis::Y, 1.0);
        let held = ticks(&mut map, HOLD);
        map.pad_axis(Stick::LeftStick, StickAxis::Y, 0.0);
        let up = map.end_tick();
        map.pad_axis(Stick::LeftStick, StickAxis::Y, 1.0);
        let tap_down = ticks(&mut map, 3);
        map.pad_axis(Stick::LeftStick, StickAxis::Y, 0.0);
        let tap_up = map.end_tick();

        for (index, frame) in held.iter().enumerate() {
            let tick = index + 1;
            assert_eq!(
                frame.pressed(StickWeapon::Check),
                tick == HOLD,
                "tick {tick}"
            );
            assert!(!frame.pressed(StickWeapon::Reload), "tick {tick}");
        }
        assert!(up.released(StickWeapon::Check));
        assert!(
            !up.pressed(StickWeapon::Reload),
            "a matured hold is not a tap"
        );
        for frame in &tap_down {
            assert!(!frame.pressed(StickWeapon::Reload));
            assert!(!frame.pressed(StickWeapon::Check));
        }
        assert!(tap_up.pressed(StickWeapon::Reload));
        assert!(tap_up.released(StickWeapon::Reload));
        assert!(!tap_up.held(StickWeapon::Reload));
        assert!(!tap_up.pressed(StickWeapon::Check));
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum Reload {
        Reload,
    }

    impl Action for Reload {
        const ALL: &'static [Self] = &[Reload::Reload];

        fn name(self) -> &'static str {
            "reload"
        }

        fn default_bindings(self) -> &'static [Binding] {
            const { &[Binding::key(Key::R), Binding::key(Key::R).held()] }
        }
    }

    #[test]
    fn one_action_on_a_key_and_its_hold() {
        let (bindings, warnings) = ActionBindings::<Reload>::load(&BTreeMap::new());
        assert_eq!(warnings, vec![]);
        let mut map = ActionMap::new(bindings, 1.0);

        map.key(Key::R, true);
        let tap_down = ticks(&mut map, 3);
        map.key(Key::R, false);
        let tap_up = map.end_tick();
        map.key(Key::R, true);
        let hold_down = ticks(&mut map, HOLD + 2);
        map.key(Key::R, false);
        let hold_up = map.end_tick();

        for frame in &tap_down {
            assert!(!frame.pressed(Reload::Reload));
            assert!(!frame.held(Reload::Reload));
        }
        assert!(tap_up.pressed(Reload::Reload));
        assert!(tap_up.released(Reload::Reload));
        assert!(!tap_up.held(Reload::Reload));
        for (index, frame) in hold_down.iter().enumerate() {
            let tick = index + 1;
            assert_eq!(frame.pressed(Reload::Reload), tick == HOLD, "tick {tick}");
            assert_eq!(frame.held(Reload::Reload), tick >= HOLD, "tick {tick}");
            assert!(!frame.released(Reload::Reload), "tick {tick}");
        }
        assert!(hold_up.released(Reload::Reload));
        assert!(!hold_up.held(Reload::Reload));
        assert!(
            !hold_up.pressed(Reload::Reload),
            "no extra press on release"
        );
    }

    #[test]
    fn repress_in_the_release_tick_starts_a_new_hold() {
        let mut map = weapon_map();
        map.key(Key::R, true);
        ticks(&mut map, 10);

        map.key(Key::R, false);
        map.key(Key::R, true);
        let repress = map.end_tick();
        let rest = ticks(&mut map, HOLD - 1);

        assert!(repress.pressed(Weapon::Reload));
        assert!(repress.released(Weapon::Reload));
        assert!(!repress.pressed(Weapon::Check));
        assert!(!repress.held(Weapon::Check));
        for (index, frame) in rest.iter().enumerate() {
            let tick = index + 2;
            assert_eq!(frame.pressed(Weapon::Check), tick == HOLD, "tick {tick}");
            assert_eq!(frame.held(Weapon::Check), tick >= HOLD, "tick {tick}");
            assert!(!frame.pressed(Weapon::Reload), "tick {tick}");
        }
    }

    #[test]
    fn pressed_while_paused_edges_are_discarded_but_counter_waits_for_ticks() {
        let mut map = weapon_map();
        map.key(Key::R, true);
        let before = ticks(&mut map, 5);

        map.discard_edges();
        map.discard_edges();
        map.discard_edges();
        let after = ticks(&mut map, HOLD - 5);

        for frame in &before {
            assert!(!frame.pressed(Weapon::Check));
        }
        let (last, early) = after.split_last().expect("some ticks after the pause");
        for frame in early {
            assert!(!frame.pressed(Weapon::Check));
            assert!(!frame.held(Weapon::Check));
        }
        assert!(last.pressed(Weapon::Check), "matures on the 18th end_tick");
        assert!(last.held(Weapon::Check));
    }

    /// `Weapon`'s split R beside a plain Jump on Space.
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum WeaponAndJump {
        Reload,
        Check,
        Jump,
    }

    impl Action for WeaponAndJump {
        const ALL: &'static [Self] = &[
            WeaponAndJump::Reload,
            WeaponAndJump::Check,
            WeaponAndJump::Jump,
        ];

        fn name(self) -> &'static str {
            match self {
                WeaponAndJump::Reload => "reload",
                WeaponAndJump::Check => "check",
                WeaponAndJump::Jump => "jump",
            }
        }

        fn default_bindings(self) -> &'static [Binding] {
            match self {
                WeaponAndJump::Reload => const { &[Binding::key(Key::R)] },
                WeaponAndJump::Check => const { &[Binding::key(Key::R).held()] },
                WeaponAndJump::Jump => const { &[Binding::key(Key::Space)] },
            }
        }
    }

    #[test]
    fn key_without_a_hold_binding_presses_on_press() {
        let (bindings, warnings) = ActionBindings::<WeaponAndJump>::load(&BTreeMap::new());
        assert_eq!(warnings, vec![]);
        let mut map = ActionMap::new(bindings, 1.0);

        map.key(Key::Space, true);
        let frame = map.end_tick();

        assert!(frame.pressed(WeaponAndJump::Jump));
        assert!(frame.held(WeaponAndJump::Jump));
        assert!(!frame.pressed(WeaponAndJump::Reload));
        assert!(!frame.pressed(WeaponAndJump::Check));
    }

    #[test]
    fn hold_works_on_every_input_kind() {
        type Press = fn(&mut ActionMap<Weapon>, bool);
        let cases: [(&str, Binding, Press); 3] = [
            ("R", Binding::key(Key::R), |m, down| m.key(Key::R, down)),
            (
                "Mouse Right",
                Binding::mouse(MouseButton::Right),
                |m, down| m.mouse_button(MouseButton::Right, down),
            ),
            (
                "Pad West",
                Binding::pad(PadInput::Button(PadButton::West)),
                |m, down| m.pad_button(PadButton::West, down),
            ),
        ];

        for (name, plain, press) in cases {
            let (mut bindings, _) = ActionBindings::<Weapon>::load(&BTreeMap::new());
            bindings.set(Weapon::Reload, vec![plain]);
            bindings.set(Weapon::Check, vec![plain.held()]);
            let mut map = ActionMap::new(bindings, 1.0);
            press(&mut map, true);

            let frames = ticks(&mut map, HOLD);

            let matured = frames.last().unwrap();
            assert!(matured.pressed(Weapon::Check), "{name}");
            assert!(matured.held(Weapon::Check), "{name}");
            for frame in &frames {
                assert!(!frame.pressed(Weapon::Reload), "{name}");
                assert!(!frame.held(Weapon::Reload), "{name}");
            }
            assert!(!frames[HOLD - 2].pressed(Weapon::Check), "{name}");
        }
    }

    #[test]
    fn game_threshold_overrides_the_default() {
        let (bindings, _) = ActionBindings::<Slow>::load(&BTreeMap::new());
        let mut tapped = ActionMap::new(bindings.clone(), 1.0);
        tapped.key(Key::R, true);
        let down = ticks(&mut tapped, 20);
        tapped.key(Key::R, false);
        let up = tapped.end_tick();
        let mut held = ActionMap::new(bindings, 1.0);
        held.key(Key::R, true);

        let frames = ticks(&mut held, 30);

        for frame in &down {
            assert!(!frame.pressed(Slow::Check));
            assert!(!frame.held(Slow::Check));
        }
        assert!(up.pressed(Slow::Reload));
        assert!(!up.pressed(Slow::Check));
        assert!(!frames[28].pressed(Slow::Check), "29th tick");
        assert!(frames[29].pressed(Slow::Check), "30th tick");
    }

    #[test]
    fn release_all_mid_hold_fires_nothing() {
        let mut map = weapon_map();
        map.key(Key::R, true);
        let down = ticks(&mut map, 10);

        map.release_all();
        let lost = map.end_tick();
        map.key(Key::R, false);
        let after = map.end_tick();

        for frame in down.iter().chain([&lost, &after]) {
            for action in Weapon::ALL {
                assert!(!frame.pressed(*action), "{action:?}");
                assert!(!frame.held(*action), "{action:?}");
            }
        }
    }

    #[test]
    fn frame_samples_do_not_advance_a_hold() {
        let mut map = weapon_map();
        map.key(Key::R, true);

        let frames: Vec<_> = (0..HOLD)
            .map(|_| {
                map.sample_frame(FRAME);
                map.end_tick()
            })
            .collect();

        assert!(!frames[HOLD - 2].held(Weapon::Check));
        assert!(frames[HOLD - 1].pressed(Weapon::Check));
        assert!(frames[HOLD - 1].held(Weapon::Check));
    }

    #[test]
    fn pad_disconnected_mid_hold_fires_nothing() {
        let west = Binding::pad(PadInput::Button(PadButton::West));
        let (mut bindings, _) = ActionBindings::<Weapon>::load(&BTreeMap::new());
        bindings.set(Weapon::Reload, vec![west]);
        bindings.set(Weapon::Check, vec![west.held()]);
        let mut map = ActionMap::new(bindings, 1.0);
        map.pad_button(PadButton::West, true);
        let down = ticks(&mut map, 10);

        map.pad_disconnected();
        let lost = map.end_tick();
        let next = map.end_tick();
        map.pad_button(PadButton::West, false);
        let after = map.end_tick();

        for frame in down.iter().chain([&lost, &next, &after]) {
            for action in Weapon::ALL {
                assert!(!frame.pressed(*action), "{action:?}");
                assert!(!frame.released(*action), "{action:?}");
                assert!(!frame.held(*action), "{action:?}");
            }
        }
    }

    #[test]
    fn tap_does_not_press_an_action_already_held() {
        let (mut bindings, _) = ActionBindings::<Weapon>::load(&BTreeMap::new());
        bindings.set(
            Weapon::Reload,
            vec![Binding::key(Key::R), Binding::key(Key::T)],
        );
        bindings.set(Weapon::Check, vec![Binding::key(Key::R).held()]);
        let mut map = ActionMap::new(bindings, 1.0);
        map.key(Key::T, true);
        let press = map.end_tick();
        map.key(Key::R, true);
        ticks(&mut map, 3);

        map.key(Key::R, false);
        let up = map.end_tick();

        assert!(press.pressed(Weapon::Reload));
        assert!(up.held(Weapon::Reload), "T keeps reload held");
        assert!(!up.pressed(Weapon::Reload));
        assert!(!up.released(Weapon::Reload));
    }

    #[test]
    fn key_repeat_does_not_restart_a_hold() {
        let mut map = weapon_map();
        map.key(Key::R, true);

        let frames: Vec<_> = (0..HOLD)
            .map(|_| {
                map.key(Key::R, true);
                map.end_tick()
            })
            .collect();

        assert!(!frames[HOLD - 2].held(Weapon::Check));
        assert!(frames[HOLD - 1].pressed(Weapon::Check));
        assert!(frames.iter().all(|f| !f.pressed(Weapon::Reload)));
    }

    /// The spec's rules for one split key, written independently of `ActionMap`.
    #[derive(Default)]
    struct Model {
        down: bool,
        ticks_down: usize,
        check_held: bool,
        check_pressed: bool,
        check_released: bool,
        reload_tapped: bool,
    }

    impl Model {
        fn settle(&mut self) {
            let held = self.down && self.ticks_down >= HOLD;
            self.check_pressed |= held && !self.check_held;
            self.check_released |= self.check_held && !held;
            self.check_held = held;
        }

        fn event(&mut self, down: bool) {
            if down == self.down {
                return;
            }
            if !down && self.ticks_down < HOLD {
                self.reload_tapped = true;
            }
            self.down = down;
            self.ticks_down = 0;
            self.settle();
        }

        fn end_tick(&mut self) -> (bool, bool, bool, bool) {
            if self.down {
                self.ticks_down += 1;
            }
            self.settle();
            let frame = (
                self.check_held,
                self.check_pressed,
                self.check_released,
                self.reload_tapped,
            );
            self.check_pressed = false;
            self.check_released = false;
            self.reload_tapped = false;
            frame
        }
    }

    fn tick_script() -> impl Strategy<Value = Vec<Vec<bool>>> {
        let tick = prop_oneof![
            6 => Just(Vec::new()),
            1 => proptest::collection::vec(any::<bool>(), 1..4),
        ];
        proptest::collection::vec(tick, 1..120)
    }

    proptest! {
        #[test]
        fn frames_depend_only_on_events_per_tick(
            script in tick_script(),
            reads in proptest::collection::vec(0usize..4, 120),
        ) {
            let mut plain = weapon_map();
            let mut batched = weapon_map();
            let mut model = Model::default();

            for (tick, events) in script.iter().enumerate() {
                for &down in events {
                    plain.key(Key::R, down);
                    batched.sample_frame(FRAME);
                    batched.key(Key::R, down);
                    model.event(down);
                }
                for _ in 0..reads[tick] {
                    batched.sample_frame(FRAME);
                }
                let a = plain.end_tick();
                let b = batched.end_tick();
                let (held, pressed, released, tapped) = model.end_tick();

                for action in Weapon::ALL {
                    prop_assert_eq!(a.held(*action), b.held(*action));
                    prop_assert_eq!(a.pressed(*action), b.pressed(*action));
                    prop_assert_eq!(a.released(*action), b.released(*action));
                }
                prop_assert_eq!(a.held(Weapon::Check), held, "tick {}", tick);
                prop_assert_eq!(a.pressed(Weapon::Check), pressed, "tick {}", tick);
                prop_assert_eq!(a.released(Weapon::Check), released, "tick {}", tick);
                prop_assert_eq!(a.pressed(Weapon::Reload), tapped, "tick {}", tick);
                prop_assert_eq!(a.released(Weapon::Reload), tapped, "tick {}", tick);
                prop_assert!(!a.held(Weapon::Reload), "tick {}", tick);
            }
        }
    }
}
