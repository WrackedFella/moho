//! Turns platform-free input events into per-tick action frames.

use std::collections::HashSet;
use std::marker::PhantomData;

use winit::event::{DeviceEvent, ElementState, WindowEvent};

use crate::bindings::{Action, ActionBindings, Binding};
use crate::filter::FilterPipeline;
use crate::key::{Key, MouseButton};

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

    /// Releases every held action, e.g. when the window loses focus.
    pub fn release_all(&mut self) {
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

    fn sample_look(&mut self) -> (f32, f32) {
        let (dx, dy) = std::mem::take(&mut self.look_accumulator);
        if dx.abs() < f64::EPSILON && dy.abs() < f64::EPSILON {
            self.filter.reset();
            return (0.0, 0.0);
        }
        self.filter
            .apply((dx as f32 * self.sensitivity, dy as f32 * self.sensitivity))
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
}
