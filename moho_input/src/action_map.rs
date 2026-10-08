//! Turns platform-free input events into per-tick action frames.

use std::marker::PhantomData;

use winit::event::{DeviceEvent, WindowEvent};

use crate::bindings::{Action, ActionBindings};
use crate::key::{Key, MouseButton};

/// What the actions did over one tick.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ActionFrame<A: Action> {
    held: u64,
    pressed: u64,
    released: u64,
    look: (f32, f32),
    actions: PhantomData<A>,
}

impl<A: Action> ActionFrame<A> {
    pub fn held(&self, _action: A) -> bool {
        todo!()
    }

    pub fn pressed(&self, _action: A) -> bool {
        todo!()
    }

    pub fn released(&self, _action: A) -> bool {
        todo!()
    }

    pub fn look(&self) -> (f32, f32) {
        todo!()
    }
}

/// Collects input events and reports them once per tick.
#[derive(Debug)]
pub struct ActionMap<A: Action> {
    actions: PhantomData<A>,
}

impl<A: Action> ActionMap<A> {
    pub fn new(_bindings: ActionBindings<A>, _sensitivity: f32) -> Self {
        todo!()
    }

    pub fn key(&mut self, _key: Key, _down: bool) {
        todo!()
    }

    pub fn mouse_button(&mut self, _button: MouseButton, _down: bool) {
        todo!()
    }

    pub fn mouse_motion(&mut self, _dx: f64, _dy: f64) {
        todo!()
    }

    pub fn release_all(&mut self) {
        todo!()
    }

    pub fn reset_look(&mut self) {
        todo!()
    }

    pub fn set_sensitivity(&mut self, _sensitivity: f32) {
        todo!()
    }

    pub fn set_filtering(&mut self, _enabled: bool) {
        todo!()
    }

    pub fn end_tick(&mut self) -> ActionFrame<A> {
        todo!()
    }
}

/// Feeds a window event to the map.
pub fn handle_window_event<A: Action>(_map: &mut ActionMap<A>, _event: &WindowEvent) {
    todo!()
}

/// Feeds a device event to the map.
pub fn handle_device_event<A: Action>(_map: &mut ActionMap<A>, _event: &DeviceEvent) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::Binding;

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
        let (bindings, _) = ActionBindings::load(&Default::default());
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
}
