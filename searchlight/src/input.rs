//! Player input: the action set, its default bindings and the per-tick command.

use moho_core::prefs::Prefs;
use moho_input::action_map::{ActionFrame, ActionMap, source_look_per_count};
use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::{Key, MouseButton};
use moho_input::{Action, LookSettings};
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

/// Look settings from `prefs`. A sensitivity not above zero would freeze or reverse the
/// view, so it falls back to the preferences default; smoothing and invert Y are unaffected.
pub fn look_settings(prefs: &Prefs) -> LookSettings {
    let saved = prefs.mouse_sensitivity();
    let sensitivity = if saved > 0.0 {
        saved
    } else {
        tracing::warn!(
            saved,
            "Mouse sensitivity must be above zero; using the default"
        );
        Prefs::default().mouse_sensitivity()
    };
    LookSettings {
        look_per_count: source_look_per_count(sensitivity),
        smoothing: prefs.mouse_smoothing(),
        invert_y: prefs.invert_mouse_y(),
    }
}

/// The action map for `bindings`, with mouse look following `prefs`.
pub fn build_action_map(
    bindings: ActionBindings<SearchlightAction>,
    prefs: &Prefs,
) -> ActionMap<SearchlightAction> {
    let look = look_settings(prefs);
    let mut map = ActionMap::new(bindings, look.look_per_count);
    map.set_look(look);
    map
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
    /// God mode is on: a level, carried on every tick like a pause.
    pub god_mode: bool,
}

/// Yaw and pitch deltas for a look. Mouse motion is right- and down-positive; positive yaw
/// turns left and positive pitch looks up.
///
/// The one mapping, shared by the command and the camera so a tick lands where the frame drew.
pub fn look_to_turn((look_x, look_y): (f32, f32)) -> (f32, f32) {
    (-look_x, -look_y)
}

impl SearchlightCommand {
    /// The command for the tick `frame` describes.
    pub fn from_frame(frame: &ActionFrame<SearchlightAction>) -> Self {
        let axis = |positive, negative| {
            f32::from(u8::from(frame.held(positive))) - f32::from(u8::from(frame.held(negative)))
        };
        let (yaw_delta, pitch_delta) = look_to_turn(frame.look());
        Self {
            movement: CharacterCommand {
                forward: axis(SearchlightAction::MoveForward, SearchlightAction::MoveBack),
                right: axis(SearchlightAction::MoveRight, SearchlightAction::MoveLeft),
                yaw_delta,
                pitch_delta,
                // A tap shorter than one tick still jumps.
                jump: frame.held(SearchlightAction::Jump) || frame.pressed(SearchlightAction::Jump),
            },
            fire: frame.pressed(SearchlightAction::Fire),
            reload: frame.pressed(SearchlightAction::Reload),
            check: frame.pressed(SearchlightAction::CheckMagazine),
            restart: frame.pressed(SearchlightAction::Restart),
            god_mode: false,
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

    const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

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
        map.sample_frame(FRAME);

        let down = SearchlightCommand::from_frame(&map.end_tick());
        map.mouse_motion(10.0, 0.0);
        map.sample_frame(FRAME);
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
    fn a_command_starts_with_god_mode_off() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::Fire);

        let sampled = SearchlightCommand::from_frame(&map.end_tick());

        assert!(!sampled.god_mode);
        assert!(!SearchlightCommand::default().god_mode);
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
        map.sample_frame(std::time::Duration::from_millis(16));
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

#[cfg(test)]
mod look_settings_tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::*;

    const FRAME: Duration = Duration::from_millis(16);

    fn prefs_with(mutate: impl FnOnce(&mut Prefs)) -> Prefs {
        let mut prefs = Prefs::default();
        mutate(&mut prefs);
        prefs
    }

    fn map_for(prefs: &Prefs) -> ActionMap<SearchlightAction> {
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        build_action_map(bindings, prefs)
    }

    fn look_after_motion(prefs: &Prefs, dx: f64, dy: f64) -> (f32, f32) {
        let mut map = map_for(prefs);
        map.mouse_motion(dx, dy);
        map.sample_frame(FRAME);
        let command = SearchlightCommand::from_frame(&map.end_tick());
        (command.movement.yaw_delta, command.movement.pitch_delta)
    }

    #[test]
    fn default_prefs_give_the_previous_look() {
        assert_eq!(
            look_settings(&Prefs::default()),
            LookSettings::raw(source_look_per_count(3.0))
        );
    }

    #[test]
    fn higher_sensitivity_turns_further() {
        let faster = prefs_with(|p| *p.mouse_sensitivity_mut() = 6.0);

        let settings = look_settings(&faster);
        let (yaw, _) = look_after_motion(&faster, 10.0, 0.0);
        let (default_yaw, _) = look_after_motion(&Prefs::default(), 10.0, 0.0);

        assert_eq!(settings.look_per_count, source_look_per_count(6.0));
        assert!(settings.look_per_count > source_look_per_count(3.0));
        assert!(yaw.abs() > default_yaw.abs(), "{yaw} vs {default_yaw}");
    }

    #[test]
    fn smoothing_and_invert_y_pass_through_independently() {
        let smooth = look_settings(&prefs_with(|p| *p.mouse_smoothing_mut() = true));
        let inverted = look_settings(&prefs_with(|p| *p.invert_mouse_y_mut() = true));

        assert!(smooth.smoothing && !smooth.invert_y);
        assert!(inverted.invert_y && !inverted.smoothing);
    }

    #[test]
    fn invert_y_negates_vertical_look_only() {
        let inverted = prefs_with(|p| *p.invert_mouse_y_mut() = true);

        let (yaw, pitch) = look_after_motion(&Prefs::default(), 7.0, 5.0);
        let (inv_yaw, inv_pitch) = look_after_motion(&inverted, 7.0, 5.0);

        assert!(yaw != 0.0 && pitch != 0.0);
        assert_eq!(inv_yaw, yaw);
        assert_eq!(inv_pitch, -pitch);
    }

    #[test]
    fn smoothing_eases_the_view() {
        let smooth = prefs_with(|p| *p.mouse_smoothing_mut() = true);

        let (_, raw_pitch) = look_after_motion(&Prefs::default(), 0.0, 10.0);
        let (_, eased_pitch) = look_after_motion(&smooth, 0.0, 10.0);

        assert!(eased_pitch != 0.0, "{eased_pitch}");
        assert!(
            eased_pitch.abs() < raw_pitch.abs(),
            "{eased_pitch} vs {raw_pitch}"
        );
    }

    #[test]
    fn non_positive_sensitivity_uses_the_default() {
        for bad in [0.0, -1.0] {
            let prefs = prefs_with(|p| {
                *p.mouse_sensitivity_mut() = bad;
                *p.mouse_smoothing_mut() = true;
                *p.invert_mouse_y_mut() = true;
            });

            assert_eq!(
                look_settings(&prefs),
                LookSettings {
                    look_per_count: source_look_per_count(3.0),
                    smoothing: true,
                    invert_y: true,
                },
                "sensitivity {bad}"
            );
        }
    }

    #[test]
    fn a_malformed_look_value_keeps_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("searchlight.ini");
        std::fs::write(
            &path,
            "[prefs]\nmouse_sensitivity=fast\ninvert_mouse_y=true\n",
        )
        .unwrap();

        let (prefs, _) = Prefs::load_from(&path);

        let settings = look_settings(&prefs);
        assert_eq!(settings.look_per_count, source_look_per_count(3.0));
        assert!(settings.invert_y);
    }
}
