//! Turns platform-free input events into per-tick action frames.

use std::collections::HashSet;
use std::marker::PhantomData;
use std::time::Duration;

use winit::event::{DeviceEvent, ElementState, WindowEvent};

use crate::bindings::{Action, ActionBindings, Binding};
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
    /// Extra look per count for each count per millisecond of speed; 0 is off.
    pub acceleration: f32,
}

impl LookSettings {
    /// Unsmoothed, non-inverted look at `look_per_count`.
    pub fn raw(look_per_count: f32) -> Self {
        Self {
            look_per_count,
            smoothing: false,
            invert_y: false,
            acceleration: 0.0,
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
    down: HashSet<Binding>,
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
        Self {
            bindings,
            down: HashSet::new(),
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
        self.set_down(Binding::Key(key), down);
    }

    pub fn mouse_button(&mut self, button: MouseButton, down: bool) {
        if down {
            self.press_pointer = self.pointer;
        }
        self.set_down(Binding::Mouse(button), down);
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
        self.set_down(Binding::Pad(PadInput::Button(button)), down);
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
        self.down.retain(|binding| !binding.is_pad());
        self.refresh_held();
    }

    /// Releases every held action, e.g. when the window loses focus.
    pub fn release_all(&mut self) {
        self.sticks = [(0.0, 0.0); 2];
        self.down.clear();
        self.refresh_held();
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

    fn set_down(&mut self, binding: Binding, down: bool) {
        // Re-inserting an already-down binding (OS key repeat) changes nothing.
        let changed = if down {
            self.down.insert(binding)
        } else {
            self.down.remove(&binding)
        };
        if changed {
            self.refresh_held();
        }
    }

    fn refresh_held(&mut self) {
        let held = A::ALL
            .iter()
            .enumerate()
            .filter(|&(_, &action)| {
                self.bindings
                    .get(action)
                    .iter()
                    .any(|binding| self.down.contains(binding))
            })
            .fold(0, |mask, (index, _)| mask | 1 << index);
        self.pressed |= held & !self.held;
        self.released |= self.held & !held;
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
        let mut changed = false;
        for (dir, active) in directions {
            let binding = Binding::Pad(PadInput::Stick(stick, dir));
            changed |= if active {
                self.down.insert(binding)
            } else {
                self.down.remove(&binding)
            };
        }
        if changed {
            self.refresh_held();
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
                TestAction::MoveForward => &[Binding::Key(Key::W), Binding::Key(Key::ArrowUp)],
                TestAction::Jump => &[Binding::Key(Key::Space)],
                TestAction::Fire => &[Binding::Mouse(MouseButton::Left)],
            }
        }
    }

    use crate::pad::{PadInput, StickDir};

    fn pad_map(sensitivity: f32) -> ActionMap<TestAction> {
        let (mut bindings, _) = ActionBindings::<TestAction>::load(&BTreeMap::new());
        bindings.set(
            TestAction::Jump,
            vec![Binding::Pad(PadInput::Button(PadButton::South))],
        );
        bindings.set(
            TestAction::MoveForward,
            vec![Binding::Pad(PadInput::Stick(
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
                        vec![Binding::Pad(PadInput::Stick(stick, dir))],
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
                acceleration: 0.0,
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
                    vec![Binding::Pad(PadInput::Stick(stick, StickDir::Up))],
                );
                bindings.set(
                    TestAction::Jump,
                    vec![
                        Binding::Pad(PadInput::Stick(stick, StickDir::Down)),
                        Binding::Pad(PadInput::Stick(stick, StickDir::Left)),
                    ],
                );
                bindings.set(
                    TestAction::Fire,
                    vec![Binding::Pad(PadInput::Stick(stick, StickDir::Right))],
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
                Binding::Key(Key::Space),
                Binding::Pad(PadInput::Button(PadButton::South)),
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
            acceleration: 0.0,
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
            acceleration: 0.0,
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
            acceleration: 0.0,
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
            acceleration: 0.0,
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
        bindings.set(TestAction::Jump, vec![Binding::Key(Key::W)]);
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
            acceleration: 0.0,
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
            acceleration: 0.0,
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

    const TEN_MS: Duration = Duration::from_millis(10);

    fn accelerated(look_per_count: f32, acceleration: f32) -> ActionMap<TestAction> {
        let mut map = pad_map(look_per_count);
        map.set_look(LookSettings {
            acceleration,
            ..LookSettings::raw(look_per_count)
        });
        map
    }

    fn assert_near(actual: (f32, f32), expected: (f32, f32)) {
        let near = |a: f32, e: f32| (a - e).abs() <= 1e-4 * e.abs().max(1.0);
        assert!(
            near(actual.0, expected.0) && near(actual.1, expected.1),
            "expected {expected:?}, got {actual:?}"
        );
    }

    /// Total yaw of `counts_per_frame` fed over `frames` frames of `dt` each.
    fn total_yaw(
        mut map: ActionMap<TestAction>,
        counts_per_frame: f64,
        frames: usize,
        dt: Duration,
    ) -> f32 {
        let mut total = 0.0;
        for _ in 0..frames {
            map.mouse_motion(counts_per_frame, 0.0);
            map.sample_frame(dt);
            total += map.end_tick().look().0;
        }
        total
    }

    #[test]
    fn without_acceleration_speed_does_not_change_the_turn() {
        let fast = total_yaw(map(1.0), 100.0, 1, TEN_MS);
        let slow = total_yaw(map(1.0), 10.0, 10, TEN_MS);

        assert_eq!(fast, 100.0);
        assert_eq!(slow, 100.0);
    }

    #[test]
    fn acceleration_turns_a_fast_motion_further() {
        let fast = total_yaw(accelerated(1.0, 0.5), 100.0, 1, TEN_MS);
        let slow = total_yaw(accelerated(1.0, 0.5), 10.0, 10, TEN_MS);

        assert!(fast > slow, "fast {fast}, slow {slow}");
        assert_near((fast, slow), (600.0, 150.0));
    }

    #[test]
    fn zero_strength_acceleration_matches_off() {
        let mut off = pad_map(0.7);
        let mut zero = accelerated(0.7, 0.0);
        for map in [&mut off, &mut zero] {
            map.mouse_motion(123.0, -45.0);
            map.sample_frame(TEN_MS);
        }

        assert_eq!(zero.end_tick().look(), off.end_tick().look());
    }

    #[test]
    fn zero_frame_dt_sample_is_not_accelerated() {
        let mut map = accelerated(2.0, 1000.0);

        map.mouse_motion(10.0, -5.0);
        map.sample_frame(Duration::ZERO);

        assert_eq!(map.end_tick().look(), (20.0, -10.0));
    }

    #[test]
    fn acceleration_speed_is_the_diagonal_counts_per_millisecond() {
        let mut map = accelerated(2.0, 0.1);

        map.mouse_motion(-30.0, 40.0);
        map.sample_frame(TEN_MS);

        // hypot(30, 40) = 50 counts over 10 ms is 5 counts/ms: 2 + 0.1 * 5 per count.
        assert_near(map.end_tick().look(), (-75.0, 100.0));
    }

    #[test]
    fn acceleration_uses_the_smoothed_counts() {
        let mut map = map(1.0);
        map.set_look(LookSettings {
            smoothing: true,
            acceleration: 0.5,
            ..LookSettings::raw(1.0)
        });

        map.mouse_motion(10.0, 4.0);
        map.sample_frame(TEN_MS);

        // Smoothing releases (5, 2) in the first frame.
        let factor = 1.0 + 0.5 * (5.0f32.hypot(2.0) / 10.0);
        assert_near(map.end_tick().look(), (5.0 * factor, 2.0 * factor));
    }

    #[test]
    fn acceleration_scales_with_the_frame_duration() {
        let mut short = accelerated(1.0, 0.5);
        let mut long = accelerated(1.0, 0.5);

        short.mouse_motion(100.0, 0.0);
        short.sample_frame(Duration::from_millis(5));
        long.mouse_motion(100.0, 0.0);
        long.sample_frame(Duration::from_millis(20));

        assert_near(short.end_tick().look(), (100.0 * (1.0 + 0.5 * 20.0), 0.0));
        assert_near(long.end_tick().look(), (100.0 * (1.0 + 0.5 * 5.0), 0.0));
    }

    #[test]
    fn stick_is_not_accelerated() {
        let mut off = pad_map(1.0);
        let mut on = accelerated(1.0, 5.0);
        for map in [&mut off, &mut on] {
            map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
            map.pad_axis(Stick::RightStick, StickAxis::Y, -0.5);
            map.sample_frame(FRAME);
        }

        assert_eq!(on.end_tick().look(), off.end_tick().look());
    }

    #[test]
    fn acceleration_applies_only_to_the_mouse_share_of_mixed_look() {
        let mut map = accelerated(1.0, 0.5);

        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        map.mouse_motion(100.0, 0.0);
        map.sample_frame(TEN_MS);
        let stick = PAD_LOOK_PER_SECOND * TEN_MS.as_secs_f32();

        assert_near(map.end_tick().look(), (600.0 + stick, 0.0));
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn acceleration_never_reduces_the_turn(
                dx in -500i32..500,
                dy in -500i32..500,
                dt_micros in 0u64..50_000,
                acceleration in 0.0f32..2.0,
                look_per_count in 0.001f32..3.0,
            ) {
                let dt = Duration::from_micros(dt_micros);
                let mut plain = map(look_per_count);
                let mut fast = accelerated(look_per_count, acceleration);
                for map in [&mut plain, &mut fast] {
                    map.mouse_motion(f64::from(dx), f64::from(dy));
                    map.sample_frame(dt);
                }

                let plain = plain.end_tick().look();
                let fast = fast.end_tick().look();

                prop_assert!(fast.0.abs() >= plain.0.abs(), "yaw {fast:?} vs {plain:?}");
                prop_assert!(fast.1.abs() >= plain.1.abs(), "pitch {fast:?} vs {plain:?}");
            }

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
}
