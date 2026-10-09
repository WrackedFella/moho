//! Turns platform-free input events into per-tick action frames.

use std::collections::HashSet;
use std::marker::PhantomData;

use winit::event::{DeviceEvent, ElementState, WindowEvent};

use crate::bindings::{Action, ActionBindings, Binding};
use crate::filter::{DeadzoneFilter, FilterPipeline};
use crate::key::{Key, MouseButton};
use crate::pad::{PadButton, PadInput, Stick, StickAxis, StickDir};

/// An axis reading at or past this magnitude holds its stick direction.
pub const STICK_DIRECTION_THRESHOLD: f32 = 0.5;

/// Look units a fully deflected right stick adds per tick, before sensitivity.
///
/// 26 turns about 180 degrees per second at the 60 Hz tick with default
/// sensitivity: the binary scales look by `0.002` radians per unit, and
/// `26 * 0.002 * 60` is about `pi`.
pub const PAD_LOOK_PER_TICK: f32 = 26.0;

/// Right-stick deflection below this radius turns nothing.
const STICK_LOOK_DEAD_ZONE: f32 = 0.15;

/// What the actions did over one tick.
///
/// Plain data: masks are indexed by position in `A::ALL`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ActionFrame<A: Action> {
    held: u64,
    pressed: u64,
    released: u64,
    look: (f32, f32),
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

    /// Filtered, sensitivity-scaled mouse motion for the tick.
    pub fn look(&self) -> (f32, f32) {
        self.look
    }
}

/// Collects input events and reports them once per tick.
#[derive(Debug)]
pub struct ActionMap<A: Action> {
    bindings: ActionBindings<A>,
    down: HashSet<Binding>,
    held: u64,
    pressed: u64,
    released: u64,
    look_accumulator: (f64, f64),
    /// Left and right stick `(x, y)`, Y up.
    sticks: [(f32, f32); 2],
    sensitivity: f32,
    filter: FilterPipeline,
}

impl<A: Action> ActionMap<A> {
    pub fn new(bindings: ActionBindings<A>, sensitivity: f32) -> Self {
        const { assert!(A::ALL.len() <= 64, "frame masks hold at most 64 actions") };
        Self {
            bindings,
            down: HashSet::new(),
            held: 0,
            pressed: 0,
            released: 0,
            look_accumulator: (0.0, 0.0),
            sticks: [(0.0, 0.0); 2],
            sensitivity,
            filter: FilterPipeline::new(),
        }
    }

    pub fn bindings(&self) -> &ActionBindings<A> {
        &self.bindings
    }

    pub fn key(&mut self, key: Key, down: bool) {
        self.set_down(Binding::Key(key), down);
    }

    pub fn mouse_button(&mut self, button: MouseButton, down: bool) {
        self.set_down(Binding::Mouse(button), down);
    }

    /// Adds to the motion reported by the next `end_tick`.
    pub fn mouse_motion(&mut self, dx: f64, dy: f64) {
        self.look_accumulator.0 += dx;
        self.look_accumulator.1 += dy;
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

    /// Discards pending motion and smoothing state so a camera jump cannot follow.
    pub fn reset_look(&mut self) {
        self.look_accumulator = (0.0, 0.0);
        self.filter.reset();
    }

    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity;
    }

    pub fn set_filtering(&mut self, enabled: bool) {
        self.filter.set_enabled(enabled);
    }

    /// The only read: reports this tick's edges and look, then starts the next tick.
    pub fn end_tick(&mut self) -> ActionFrame<A> {
        let frame = ActionFrame {
            held: self.held,
            pressed: self.pressed,
            released: self.released,
            look: self.sample_look(),
            actions: PhantomData,
        };
        self.pressed = 0;
        self.released = 0;
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

    fn sample_look(&mut self) -> (f32, f32) {
        let (dx, dy) = std::mem::take(&mut self.look_accumulator);
        let mouse = if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
            self.filter.reset();
            (0.0, 0.0)
        } else {
            self.filter
                .apply((dx as f32 * self.sensitivity, dy as f32 * self.sensitivity))
        };
        let (stick_x, stick_y) = self.stick_look();
        (mouse.0 + stick_x, mouse.1 + stick_y)
    }

    /// Right-stick look for one tick; stick up looks up, and mouse dy is down-positive.
    fn stick_look(&self) -> (f32, f32) {
        let (x, y) = DeadzoneFilter::new(STICK_LOOK_DEAD_ZONE)
            .apply(self.sticks[Stick::RightStick as usize]);
        let scale = PAD_LOOK_PER_TICK * self.sensitivity;
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
        // (y, held)
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
    fn right_stick_adds_per_tick_look() {
        for filtering in [true, false] {
            let mut map = pad_map(2.0);
            map.set_filtering(filtering);

            map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
            let first = map.end_tick().look();
            let second = map.end_tick().look();

            let expected = PAD_LOOK_PER_TICK * 2.0;
            assert_close(first, (expected, 0.0));
            assert_close(second, (expected, 0.0));
        }
    }

    #[test]
    fn right_stick_up_looks_up_and_scales_with_deflection() {
        let mut map = pad_map(1.0);
        map.set_filtering(false);

        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.5);
        let look = map.end_tick().look();

        assert_close(look, (0.0, -0.5 * PAD_LOOK_PER_TICK));
    }

    #[test]
    fn right_stick_inside_dead_zone_adds_no_look() {
        let mut map = pad_map(1.0);
        map.set_filtering(false);

        map.pad_axis(Stick::RightStick, StickAxis::X, 0.1);
        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.1);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn right_stick_dead_zone_is_radial_not_per_axis() {
        let mut map = pad_map(1.0);
        map.set_filtering(false);

        map.pad_axis(Stick::RightStick, StickAxis::X, 0.12);
        map.pad_axis(Stick::RightStick, StickAxis::Y, 0.12);
        let look = map.end_tick().look();

        assert_close(look, (0.12 * PAD_LOOK_PER_TICK, -0.12 * PAD_LOOK_PER_TICK));
    }

    #[test]
    fn left_stick_does_not_add_look() {
        let mut map = pad_map(1.0);
        map.set_filtering(false);

        map.pad_axis(Stick::LeftStick, StickAxis::X, 1.0);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn stick_look_is_added_after_the_mouse_filter() {
        let mut map = pad_map(1.0);
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        map.mouse_motion(1.0, 0.0);

        let look = map.end_tick().look();

        assert_close(look, (0.8 + PAD_LOOK_PER_TICK, 0.0));
    }

    #[test]
    fn release_all_stops_stick_look() {
        let mut map = pad_map(1.0);
        map.set_filtering(false);
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        assert_close(map.end_tick().look(), (PAD_LOOK_PER_TICK, 0.0));

        map.release_all();
        let frame = map.end_tick();
        let next = map.end_tick();

        assert_eq!(frame.look(), (0.0, 0.0));
        assert_eq!(next.look(), (0.0, 0.0));
    }

    #[test]
    fn pad_disconnect_releases_pad_bindings() {
        let mut map = pad_map(1.0);
        map.set_filtering(false);
        map.pad_button(PadButton::South, true);
        map.pad_axis(Stick::LeftStick, StickAxis::Y, 1.0);
        map.pad_axis(Stick::RightStick, StickAxis::X, 1.0);
        map.end_tick();

        map.pad_disconnected();
        let frame = map.end_tick();
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
        map.set_filtering(false);

        map.mouse_motion(1.0, 0.0);
        map.mouse_motion(2.0, 1.0);
        let frame = map.end_tick();
        let next = map.end_tick();

        assert_eq!(frame.look(), (6.0, 2.0));
        assert_eq!(next.look(), (0.0, 0.0));
    }

    #[test]
    fn filtered_look_accumulates_then_smooths_and_resets() {
        let mut map = map(1.0);

        map.mouse_motion(1.0, 0.0);
        map.mouse_motion(1.0, 0.0);
        map.mouse_motion(0.0, 1.0);
        let frame = map.end_tick();
        let next = map.end_tick();

        assert_eq!(frame.look(), (1.6, 0.8));
        assert_eq!(next.look(), (0.0, 0.0));
    }

    #[test]
    fn filtered_look_applies_sensitivity_before_smoothing() {
        let mut map = map(2.0);

        map.mouse_motion(1.0, 1.0);
        let frame = map.end_tick();

        assert_eq!(frame.look(), (1.6, 1.6));
    }

    #[test]
    fn set_sensitivity_changes_later_look() {
        let mut map = map(1.0);
        map.set_filtering(false);
        map.set_sensitivity(3.0);

        map.mouse_motion(1.0, 2.0);

        assert_eq!(map.end_tick().look(), (3.0, 6.0));
    }

    #[test]
    fn reset_look_discards_pending_motion() {
        let mut map = map(1.0);
        map.set_filtering(false);
        map.mouse_motion(5.0, 5.0);

        map.reset_look();

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
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
        map.set_filtering(false);

        handle_device_event(&mut map, &DeviceEvent::MouseMotion { delta: (3.0, -1.0) });

        assert_eq!(map.end_tick().look(), (3.0, -1.0));
    }

    #[test]
    fn unfiltered_horizontal_only_motion_scales_x_and_leaves_y_zero() {
        let mut map = map(2.0);
        map.set_filtering(false);

        map.mouse_motion(3.0, 0.0);

        assert_eq!(map.end_tick().look(), (6.0, 0.0));
    }

    #[test]
    fn unfiltered_vertical_only_motion_scales_y_and_leaves_x_zero() {
        let mut map = map(2.0);
        map.set_filtering(false);

        map.mouse_motion(0.0, 3.0);

        assert_eq!(map.end_tick().look(), (0.0, 6.0));
    }

    #[test]
    fn filtered_motion_below_deadzone_reads_zero() {
        let mut map = map(1.0);

        map.mouse_motion(0.005, 0.0);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn filtered_diagonal_motion_above_deadzone_by_magnitude_is_smoothed_not_zeroed() {
        let mut map = map(1.0);

        map.mouse_motion(0.009, 0.009);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.0072).abs() < 1e-6, "x = {x}");
        assert!((y - 0.0072).abs() < 1e-6, "y = {y}");
    }

    #[test]
    fn consecutive_filtered_ticks_blend_with_previous_output_on_both_axes() {
        let mut map = map(1.0);

        map.mouse_motion(1.0, 1.0);
        let first = map.end_tick().look();
        map.mouse_motion(1.0, 1.0);
        let second = map.end_tick().look();

        assert!((first.0 - 0.8).abs() < 1e-6 && (first.1 - 0.8).abs() < 1e-6);
        assert!((second.0 - 0.96).abs() < 1e-6, "x = {}", second.0);
        assert!((second.1 - 0.96).abs() < 1e-6, "y = {}", second.1);
    }

    #[test]
    fn idle_tick_clears_smoothing_state() {
        let mut map = map(1.0);
        map.mouse_motion(1.0, 1.0);
        map.end_tick();
        map.end_tick();

        map.mouse_motion(1.0, 1.0);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.8).abs() < 1e-6, "x = {x}");
        assert!((y - 0.8).abs() < 1e-6, "y = {y}");
    }

    #[test]
    fn enabling_filtering_while_enabled_keeps_smoothing_state() {
        let mut map = map(1.0);
        map.mouse_motion(1.0, 1.0);
        map.end_tick();

        map.set_filtering(true);
        map.mouse_motion(1.0, 1.0);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.96).abs() < 1e-6, "x = {x}");
        assert!((y - 0.96).abs() < 1e-6, "y = {y}");
    }

    #[test]
    fn disabling_filtering_discards_smoothing_state() {
        let mut map = map(1.0);
        map.mouse_motion(1.0, 1.0);
        map.end_tick();

        map.set_filtering(false);
        map.set_filtering(true);
        map.mouse_motion(1.0, 1.0);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.8).abs() < 1e-6, "x = {x}");
        assert!((y - 0.8).abs() < 1e-6, "y = {y}");
    }

    #[test]
    fn filtered_vertical_motion_below_deadzone_reads_zero() {
        let mut map = map(1.0);

        map.mouse_motion(0.0, 0.005);

        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn filtered_motion_exactly_at_deadzone_threshold_passes() {
        let mut map = map(1.0);

        map.mouse_motion(0.01, 0.0);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.008).abs() < 1e-6, "x = {x}");
        assert_eq!(y, 0.0);
    }

    #[test]
    fn horizontal_motion_of_exactly_epsilon_is_not_idle_and_keeps_smoothing() {
        let mut map = map(1.0);
        map.mouse_motion(1.0, 1.0);
        map.end_tick();

        map.mouse_motion(f64::EPSILON, 0.0);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.16).abs() < 1e-6, "x = {x}");
        assert!((y - 0.16).abs() < 1e-6, "y = {y}");
    }

    #[test]
    fn vertical_motion_of_exactly_epsilon_is_not_idle_and_keeps_smoothing() {
        let mut map = map(1.0);
        map.mouse_motion(1.0, 1.0);
        map.end_tick();

        map.mouse_motion(0.0, f64::EPSILON);
        let (x, y) = map.end_tick().look();

        assert!((x - 0.16).abs() < 1e-6, "x = {x}");
        assert!((y - 0.16).abs() < 1e-6, "y = {y}");
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
        map.set_filtering(false);

        map.mouse_motion(1.0, 0.0);
        map.mouse_motion(1.0, 1.0);

        assert_eq!(map.end_tick().look(), (2.0, 1.0));
        assert_eq!(map.end_tick().look(), (0.0, 0.0));
    }

    #[test]
    fn test_sensitivity_scaling() {
        let mut map = map(2.0);
        map.set_filtering(false);

        map.mouse_motion(1.0, 3.0);

        assert_eq!(map.end_tick().look(), (2.0, 6.0));
    }

    #[test]
    fn filter_smooths_across_frames() {
        let mut map = map(1.0);

        map.mouse_motion(1.0, 2.0);
        let first = map.end_tick().look();
        map.mouse_motion(1.0, 2.0);
        let second = map.end_tick().look();

        assert_close(first, (0.8, 1.6));
        // 0.8 * input + 0.2 * previous output, per axis
        assert_close(second, (0.96, 1.92));
    }

    #[test]
    fn filter_resets_on_idle_frame() {
        let mut map = map(1.0);

        map.mouse_motion(1.0, 0.0);
        map.end_tick();
        let idle = map.end_tick().look();
        map.mouse_motion(1.0, 0.0);
        let after_idle = map.end_tick().look();

        assert_eq!(idle, (0.0, 0.0));
        assert_close(after_idle, (0.8, 0.0));
    }

    #[test]
    fn deadzone_zeroes_tiny_output() {
        let look_of = |dx, dy| {
            let mut map = map(1.0);
            map.mouse_motion(dx, dy);
            map.end_tick().look()
        };

        assert_eq!(look_of(0.005, 0.0), (0.0, 0.0));
        assert_eq!(look_of(0.0, 0.005), (0.0, 0.0));
        assert_close(look_of(0.02, 0.0), (0.016, 0.0));
        // Each axis is below 0.01, the magnitude is not.
        assert_close(look_of(0.0075, 0.0075), (0.006, 0.006));
        assert_close(look_of(0.01, 0.0), (0.008, 0.0));
    }

    #[test]
    fn reset_look_discards_accumulated_delta() {
        let mut dirty = map(1.0);
        dirty.mouse_motion(1.0, 0.0);
        dirty.end_tick();
        dirty.mouse_motion(5.0, 5.0);
        dirty.reset_look();
        let mut fresh = map(1.0);

        dirty.mouse_motion(1.0, 0.0);
        fresh.mouse_motion(1.0, 0.0);

        assert_eq!(dirty.end_tick().look(), fresh.end_tick().look());
    }
}
