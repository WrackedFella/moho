//! Player input: the action set, its default bindings and the per-tick command.

use moho_input::Action;
use moho_input::action_map::ActionFrame;
use moho_input::bindings::Binding;
use moho_input::key::{Key, MouseButton};
use moho_physics::CharacterCommand;

/// Everything the player can ask of the prototype.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SearchlightAction {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Jump,
    Fire,
    Reload,
    CheckMagazine,
    Restart,
}

impl Action for SearchlightAction {
    const ALL: &'static [Self] = &[
        SearchlightAction::MoveForward,
        SearchlightAction::MoveBack,
        SearchlightAction::MoveLeft,
        SearchlightAction::MoveRight,
        SearchlightAction::Jump,
        SearchlightAction::Fire,
        SearchlightAction::Reload,
        SearchlightAction::CheckMagazine,
        SearchlightAction::Restart,
    ];

    fn name(self) -> &'static str {
        match self {
            SearchlightAction::MoveForward => "move_forward",
            SearchlightAction::MoveBack => "move_back",
            SearchlightAction::MoveLeft => "move_left",
            SearchlightAction::MoveRight => "move_right",
            SearchlightAction::Jump => "jump",
            SearchlightAction::Fire => "fire",
            SearchlightAction::Reload => "reload",
            SearchlightAction::CheckMagazine => "check_magazine",
            SearchlightAction::Restart => "restart",
        }
    }

    fn default_bindings(self) -> &'static [Binding] {
        match self {
            SearchlightAction::MoveForward => &[Binding::Key(Key::W)],
            SearchlightAction::MoveBack => &[Binding::Key(Key::S)],
            SearchlightAction::MoveLeft => &[Binding::Key(Key::A)],
            SearchlightAction::MoveRight => &[Binding::Key(Key::D)],
            SearchlightAction::Jump => &[Binding::Key(Key::Space)],
            SearchlightAction::Fire => &[Binding::Mouse(MouseButton::Left)],
            SearchlightAction::Reload => &[Binding::Key(Key::R)],
            SearchlightAction::CheckMagazine => &[Binding::Key(Key::C)],
            SearchlightAction::Restart => &[Binding::Key(Key::Enter)],
        }
    }
}

/// One tick of player intent: movement plus the weapon edges that began this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SearchlightCommand {
    pub movement: CharacterCommand,
    /// The fire input went down this tick (semi-auto: holding does not repeat).
    pub fire: bool,
    /// The reload input went down this tick.
    pub reload: bool,
    /// The check-magazine input went down this tick.
    pub check: bool,
    /// The restart input went down this tick.
    pub restart: bool,
}

impl SearchlightCommand {
    /// The command for the tick `frame` describes.
    pub fn from_frame(frame: &ActionFrame<SearchlightAction>) -> Self {
        let axis = |positive, negative| {
            f32::from(u8::from(frame.held(positive))) - f32::from(u8::from(frame.held(negative)))
        };
        // Mouse motion is right- and down-positive; positive yaw turns left and positive pitch
        // looks up.
        let (look_x, look_y) = frame.look();
        Self {
            movement: CharacterCommand {
                forward: axis(SearchlightAction::MoveForward, SearchlightAction::MoveBack),
                right: axis(SearchlightAction::MoveRight, SearchlightAction::MoveLeft),
                yaw_delta: -look_x,
                pitch_delta: -look_y,
                // A tap shorter than one tick still jumps.
                jump: frame.held(SearchlightAction::Jump) || frame.pressed(SearchlightAction::Jump),
            },
            fire: frame.pressed(SearchlightAction::Fire),
            reload: frame.pressed(SearchlightAction::Reload),
            check: frame.pressed(SearchlightAction::CheckMagazine),
            restart: frame.pressed(SearchlightAction::Restart),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_input::action_map::ActionMap;
    use moho_input::bindings::ActionBindings;

    use super::*;
    use crate::test_support::press;

    fn fresh_map() -> ActionMap<SearchlightAction> {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        ActionMap::new(bindings, 1.0)
    }

    fn release(map: &mut ActionMap<SearchlightAction>, action: SearchlightAction) {
        match action.default_bindings()[0] {
            Binding::Key(key) => map.key(key, false),
            Binding::Mouse(button) => map.mouse_button(button, false),
            Binding::Pad(_) => panic!("pad binding"),
        }
    }

    #[test]
    fn every_action_has_a_distinct_name_and_a_binding() {
        for &action in SearchlightAction::ALL {
            assert!(!action.default_bindings().is_empty(), "{action:?}");
        }
        let mut names: Vec<_> = SearchlightAction::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SearchlightAction::ALL.len());
    }

    #[test]
    fn move_keys_become_axes_and_opposites_cancel() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::MoveForward);
        press(&mut map, SearchlightAction::MoveRight);

        let both = SearchlightCommand::from_frame(&map.end_tick());
        press(&mut map, SearchlightAction::MoveBack);
        press(&mut map, SearchlightAction::MoveLeft);
        let opposed = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!((both.movement.forward, both.movement.right), (1.0, 1.0));
        assert_eq!(
            (opposed.movement.forward, opposed.movement.right),
            (0.0, 0.0)
        );
    }

    #[test]
    fn back_and_left_are_negative() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::MoveBack);
        press(&mut map, SearchlightAction::MoveLeft);

        let command = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!(
            (command.movement.forward, command.movement.right),
            (-1.0, -1.0)
        );
    }

    #[test]
    fn jump_is_held_not_an_edge() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::Jump);
        let _first = map.end_tick();

        let held = SearchlightCommand::from_frame(&map.end_tick());

        assert!(held.movement.jump);
    }

    #[test]
    fn fire_is_semi_auto_on_the_press_edge() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::Fire);

        let pressed = SearchlightCommand::from_frame(&map.end_tick());
        let held = SearchlightCommand::from_frame(&map.end_tick());
        release(&mut map, SearchlightAction::Fire);
        let released = SearchlightCommand::from_frame(&map.end_tick());
        press(&mut map, SearchlightAction::Fire);
        let again = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!(
            [pressed.fire, held.fire, released.fire, again.fire],
            [true, false, false, true]
        );
    }

    #[test]
    fn reload_and_check_fire_once_per_press() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::Reload);
        press(&mut map, SearchlightAction::CheckMagazine);

        let pressed = SearchlightCommand::from_frame(&map.end_tick());
        let held = SearchlightCommand::from_frame(&map.end_tick());

        assert!(pressed.reload && pressed.check);
        assert!(!pressed.fire);
        assert!(!held.reload && !held.check);
    }

    #[test]
    fn mouse_motion_turns_and_pitches_with_up_positive() {
        let mut map = fresh_map();
        map.mouse_motion(0.0, 10.0);

        let down = SearchlightCommand::from_frame(&map.end_tick());
        map.mouse_motion(10.0, 0.0);
        let sideways = SearchlightCommand::from_frame(&map.end_tick());

        assert!(down.movement.pitch_delta < 0.0, "{down:?}");
        assert_eq!(down.movement.yaw_delta, 0.0);
        assert!(sideways.movement.yaw_delta != 0.0, "{sideways:?}");
        assert_eq!(sideways.movement.pitch_delta, 0.0);
    }

    #[test]
    fn enter_is_bound_to_restart() {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        let mut map = ActionMap::<SearchlightAction>::new(bindings, 1.0);
        map.key(Key::Enter, true);

        let pressed = SearchlightCommand::from_frame(&map.end_tick());
        let held = SearchlightCommand::from_frame(&map.end_tick());
        map.key(Key::Enter, false);
        let released = SearchlightCommand::from_frame(&map.end_tick());
        map.key(Key::Enter, true);
        let again = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!(
            SearchlightAction::Restart.default_bindings(),
            [Binding::Key(Key::Enter)]
        );
        assert!(SearchlightAction::ALL.contains(&SearchlightAction::Restart));
        assert_eq!(
            [
                pressed.restart,
                held.restart,
                released.restart,
                again.restart
            ],
            [true, false, false, true]
        );
        assert!(!pressed.fire && !pressed.reload && !pressed.check);
    }

    #[test]
    fn an_idle_tick_is_an_empty_command() {
        let mut map = fresh_map();

        let command = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!(command, SearchlightCommand::default());
    }
}

#[cfg(test)]
mod look_direction_tests {
    use std::collections::BTreeMap;

    use moho_input::action_map::ActionMap;
    use moho_input::bindings::ActionBindings;

    use super::*;

    fn yaw_after_mouse_motion(dx: f64) -> f32 {
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut map = ActionMap::<SearchlightAction>::new(bindings, 1.0);
        map.mouse_motion(dx, 0.0);
        SearchlightCommand::from_frame(&map.end_tick())
            .movement
            .yaw_delta
    }

    #[test]
    fn moving_the_mouse_right_turns_right_and_left_turns_left() {
        let right = yaw_after_mouse_motion(10.0);
        let left = yaw_after_mouse_motion(-10.0);

        assert!(right < 0.0, "{right}");
        assert!(left > 0.0, "{left}");
    }
}
