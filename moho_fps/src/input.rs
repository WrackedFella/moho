//! Player input: the action set, its default bindings and the per-tick command.

use moho_input::Action;
use moho_input::action_map::ActionFrame;
use moho_input::bindings::Binding;
use moho_input::key::{Key, MouseButton};
use moho_physics::CharacterCommand;

/// Everything the player can ask of the prototype.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FpsAction {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Jump,
    Fire,
    Reload,
    CheckMagazine,
}

impl Action for FpsAction {
    const ALL: &'static [Self] = &[
        FpsAction::MoveForward,
        FpsAction::MoveBack,
        FpsAction::MoveLeft,
        FpsAction::MoveRight,
        FpsAction::Jump,
        FpsAction::Fire,
        FpsAction::Reload,
        FpsAction::CheckMagazine,
    ];

    fn name(self) -> &'static str {
        match self {
            FpsAction::MoveForward => "move_forward",
            FpsAction::MoveBack => "move_back",
            FpsAction::MoveLeft => "move_left",
            FpsAction::MoveRight => "move_right",
            FpsAction::Jump => "jump",
            FpsAction::Fire => "fire",
            FpsAction::Reload => "reload",
            FpsAction::CheckMagazine => "check_magazine",
        }
    }

    fn default_bindings(self) -> &'static [Binding] {
        match self {
            FpsAction::MoveForward => &[Binding::Key(Key::W)],
            FpsAction::MoveBack => &[Binding::Key(Key::S)],
            FpsAction::MoveLeft => &[Binding::Key(Key::A)],
            FpsAction::MoveRight => &[Binding::Key(Key::D)],
            FpsAction::Jump => &[Binding::Key(Key::Space)],
            FpsAction::Fire => &[Binding::Mouse(MouseButton::Left)],
            FpsAction::Reload => &[Binding::Key(Key::R)],
            FpsAction::CheckMagazine => &[Binding::Key(Key::C)],
        }
    }
}

/// One tick of player intent: movement plus the weapon edges that began this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FpsCommand {
    pub movement: CharacterCommand,
    /// The fire input went down this tick (semi-auto: holding does not repeat).
    pub fire: bool,
    /// The reload input went down this tick.
    pub reload: bool,
    /// The check-magazine input went down this tick.
    pub check: bool,
}

impl FpsCommand {
    /// The command for the tick `frame` describes.
    pub fn from_frame(frame: &ActionFrame<FpsAction>) -> Self {
        let axis = |positive, negative| {
            f32::from(u8::from(frame.held(positive))) - f32::from(u8::from(frame.held(negative)))
        };
        // Mouse motion is right- and down-positive; positive yaw turns left and positive pitch
        // looks up.
        let (look_x, look_y) = frame.look();
        Self {
            movement: CharacterCommand {
                forward: axis(FpsAction::MoveForward, FpsAction::MoveBack),
                right: axis(FpsAction::MoveRight, FpsAction::MoveLeft),
                yaw_delta: -look_x,
                pitch_delta: -look_y,
                // A tap shorter than one tick still jumps.
                jump: frame.held(FpsAction::Jump) || frame.pressed(FpsAction::Jump),
            },
            fire: frame.pressed(FpsAction::Fire),
            reload: frame.pressed(FpsAction::Reload),
            check: frame.pressed(FpsAction::CheckMagazine),
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

    fn fresh_map() -> ActionMap<FpsAction> {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        ActionMap::new(bindings, 1.0)
    }

    fn release(map: &mut ActionMap<FpsAction>, action: FpsAction) {
        match action.default_bindings()[0] {
            Binding::Key(key) => map.key(key, false),
            Binding::Mouse(button) => map.mouse_button(button, false),
            Binding::Pad(_) => panic!("pad binding"),
        }
    }

    #[test]
    fn every_action_has_a_distinct_name_and_a_binding() {
        for &action in FpsAction::ALL {
            assert!(!action.default_bindings().is_empty(), "{action:?}");
        }
        let mut names: Vec<_> = FpsAction::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), FpsAction::ALL.len());
    }

    #[test]
    fn move_keys_become_axes_and_opposites_cancel() {
        let mut map = fresh_map();
        press(&mut map, FpsAction::MoveForward);
        press(&mut map, FpsAction::MoveRight);

        let both = FpsCommand::from_frame(&map.end_tick());
        press(&mut map, FpsAction::MoveBack);
        press(&mut map, FpsAction::MoveLeft);
        let opposed = FpsCommand::from_frame(&map.end_tick());

        assert_eq!((both.movement.forward, both.movement.right), (1.0, 1.0));
        assert_eq!(
            (opposed.movement.forward, opposed.movement.right),
            (0.0, 0.0)
        );
    }

    #[test]
    fn back_and_left_are_negative() {
        let mut map = fresh_map();
        press(&mut map, FpsAction::MoveBack);
        press(&mut map, FpsAction::MoveLeft);

        let command = FpsCommand::from_frame(&map.end_tick());

        assert_eq!(
            (command.movement.forward, command.movement.right),
            (-1.0, -1.0)
        );
    }

    #[test]
    fn jump_is_held_not_an_edge() {
        let mut map = fresh_map();
        press(&mut map, FpsAction::Jump);
        let _first = map.end_tick();

        let held = FpsCommand::from_frame(&map.end_tick());

        assert!(held.movement.jump);
    }

    #[test]
    fn fire_is_semi_auto_on_the_press_edge() {
        let mut map = fresh_map();
        press(&mut map, FpsAction::Fire);

        let pressed = FpsCommand::from_frame(&map.end_tick());
        let held = FpsCommand::from_frame(&map.end_tick());
        release(&mut map, FpsAction::Fire);
        let released = FpsCommand::from_frame(&map.end_tick());
        press(&mut map, FpsAction::Fire);
        let again = FpsCommand::from_frame(&map.end_tick());

        assert_eq!(
            [pressed.fire, held.fire, released.fire, again.fire],
            [true, false, false, true]
        );
    }

    #[test]
    fn reload_and_check_fire_once_per_press() {
        let mut map = fresh_map();
        press(&mut map, FpsAction::Reload);
        press(&mut map, FpsAction::CheckMagazine);

        let pressed = FpsCommand::from_frame(&map.end_tick());
        let held = FpsCommand::from_frame(&map.end_tick());

        assert!(pressed.reload && pressed.check);
        assert!(!pressed.fire);
        assert!(!held.reload && !held.check);
    }

    #[test]
    fn mouse_motion_turns_and_pitches_with_up_positive() {
        let mut map = fresh_map();
        map.set_filtering(false);
        map.mouse_motion(0.0, 10.0);

        let down = FpsCommand::from_frame(&map.end_tick());
        map.mouse_motion(10.0, 0.0);
        let sideways = FpsCommand::from_frame(&map.end_tick());

        assert!(down.movement.pitch_delta < 0.0, "{down:?}");
        assert_eq!(down.movement.yaw_delta, 0.0);
        assert!(sideways.movement.yaw_delta != 0.0, "{sideways:?}");
        assert_eq!(sideways.movement.pitch_delta, 0.0);
    }

    #[test]
    fn an_idle_tick_is_an_empty_command() {
        let mut map = fresh_map();

        let command = FpsCommand::from_frame(&map.end_tick());

        assert_eq!(command, FpsCommand::default());
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
        let mut map = ActionMap::<FpsAction>::new(bindings, 1.0);
        map.set_filtering(false);
        map.mouse_motion(dx, 0.0);
        FpsCommand::from_frame(&map.end_tick()).movement.yaw_delta
    }

    #[test]
    fn moving_the_mouse_right_turns_right_and_left_turns_left() {
        let right = yaw_after_mouse_motion(10.0);
        let left = yaw_after_mouse_motion(-10.0);

        assert!(right < 0.0, "{right}");
        assert!(left > 0.0, "{left}");
    }
}
