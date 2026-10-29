//! Player input: the action set, its default bindings and the per-tick command.

use moho_core::prefs::Prefs;
use moho_input::action_map::{ActionFrame, ActionMap, source_look_per_count};
use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::{Key, MouseButton};
use moho_input::{Action, LookSettings};
use moho_physics::CharacterCommand;
use searchlight_rules::{Grade, GravityScale, LeanSide};

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
    LeanLeft,
    LeanRight,
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
        SearchlightAction::LeanLeft,
        SearchlightAction::LeanRight,
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
            SearchlightAction::LeanLeft => "lean_left",
            SearchlightAction::LeanRight => "lean_right",
        }
    }

    fn default_bindings(self) -> &'static [Binding] {
        match self {
            SearchlightAction::MoveForward => const { &[Binding::key(Key::W)] },
            SearchlightAction::MoveBack => const { &[Binding::key(Key::S)] },
            SearchlightAction::MoveLeft => const { &[Binding::key(Key::A)] },
            SearchlightAction::MoveRight => const { &[Binding::key(Key::D)] },
            SearchlightAction::Jump => const { &[Binding::key(Key::Space)] },
            SearchlightAction::Fire => const { &[Binding::mouse(MouseButton::Left)] },
            SearchlightAction::Reload => const { &[Binding::key(Key::R)] },
            SearchlightAction::CheckMagazine => {
                const { &[Binding::key(Key::C), Binding::key(Key::R).held()] }
            }
            SearchlightAction::Restart => const { &[Binding::key(Key::F8)] },
            SearchlightAction::LeanLeft => const { &[Binding::key(Key::Q)] },
            SearchlightAction::LeanRight => const { &[Binding::key(Key::E)] },
        }
    }
}

/// Look settings from `prefs`. A sensitivity not above zero would freeze or reverse the
/// view, so it falls back to the preferences default; smoothing and invert Y are unaffected.
pub fn look_settings(prefs: &Prefs) -> LookSettings {
    look_from(
        prefs.mouse_sensitivity(),
        prefs.mouse_smoothing(),
        prefs.invert_mouse_y(),
    )
}

/// Look settings from the three values `look_settings` reads, with the same fallback for a
/// sensitivity not above zero.
pub fn look_from(saved: f32, smoothing: bool, invert_y: bool) -> LookSettings {
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
        smoothing,
        invert_y,
        acceleration: 0.0,
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
    /// Reload was pressed this tick; on a key that also carries a hold, that is a tap's release.
    pub reload: bool,
    /// Check magazine was pressed this tick.
    pub check: bool,
    /// The restart input went down this tick.
    pub restart: bool,
    /// God mode is on: a level, carried on every tick like a pause.
    pub god_mode: bool,
    /// The grade every enemy spawned this tick gets, or `None` for the normal grade: a level,
    /// carried on every tick. A host action like `god_mode`: once commands are per player
    /// slot, only slot 0's is read.
    pub forced_grade: Option<Grade>,
    /// Simple is on: shots are hitscan, not rounds in flight. A host level like `god_mode`.
    pub simple: bool,
    /// How hard gravity pulls on rounds, as a multiple of standard gravity. A host level like
    /// `god_mode`.
    pub round_gravity: GravityScale,
    /// The side the player holds a lean toward, after key resolution.
    pub lean: Option<LeanSide>,
}

/// The lean side held this tick: a side pressed this tick wins (right on a same-tick tie),
/// otherwise `previous` while it is still held, otherwise whichever side is held.
pub fn resolve_lean(
    frame: &ActionFrame<SearchlightAction>,
    previous: Option<LeanSide>,
) -> Option<LeanSide> {
    let action = |side| match side {
        LeanSide::Left => SearchlightAction::LeanLeft,
        LeanSide::Right => SearchlightAction::LeanRight,
    };
    let pressed = |side: &LeanSide| frame.pressed(action(*side));
    let held = |side: &LeanSide| {
        let action = action(*side);
        frame.held(action) || frame.pressed(action)
    };
    let sides = [LeanSide::Right, LeanSide::Left];
    sides
        .into_iter()
        .find(pressed)
        .or(previous.filter(held))
        .or_else(|| sides.into_iter().find(held))
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
                ..CharacterCommand::default()
            },
            fire: frame.pressed(SearchlightAction::Fire),
            reload: frame.pressed(SearchlightAction::Reload),
            check: frame.pressed(SearchlightAction::CheckMagazine),
            restart: frame.pressed(SearchlightAction::Restart),
            god_mode: false,
            forced_grade: None,
            simple: false,
            round_gravity: GravityScale::default(),
            lean: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_input::action_map::ActionMap;
    use moho_input::bindings::{ActionBindings, Input, Modifiers};

    use super::*;
    use crate::test_support::press;

    const NONE: Modifiers = Modifiers::NONE;

    const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

    fn fresh_map() -> ActionMap<SearchlightAction> {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        ActionMap::new(bindings, 1.0)
    }

    fn release(map: &mut ActionMap<SearchlightAction>, action: SearchlightAction) {
        match action.default_bindings()[0].input {
            Input::Key(key) => map.key(key, false),
            Input::Mouse(button) => map.mouse_button(button, false),
            Input::Pad(_) => panic!("pad binding"),
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
    fn tap_r_reloads_on_release() {
        let mut map = fresh_map();
        map.key(Key::R, true);
        let down = SearchlightCommand::from_frame(&map.end_tick());

        map.key(Key::R, false);
        let up = SearchlightCommand::from_frame(&map.end_tick());
        let after = SearchlightCommand::from_frame(&map.end_tick());

        assert!(!down.reload && !down.check, "a press alone fires nothing");
        assert!(up.reload && !up.check, "the tap reloads on release");
        assert!(!after.reload && !after.check);
    }

    #[test]
    fn hold_r_checks_the_magazine_and_never_reloads() {
        let mut map = fresh_map();
        map.key(Key::R, true);

        let frames: Vec<_> = (0..18)
            .map(|_| SearchlightCommand::from_frame(&map.end_tick()))
            .collect();
        map.key(Key::R, false);
        let released = SearchlightCommand::from_frame(&map.end_tick());

        assert!(frames[17].check, "the hold checks in tick 18");
        assert!(frames[..17].iter().all(|f| !f.check));
        assert!(frames.iter().all(|f| !f.reload));
        assert!(!released.reload && !released.check);
    }

    #[test]
    fn c_still_checks_on_press() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::CheckMagazine);

        let pressed = SearchlightCommand::from_frame(&map.end_tick());
        let held = SearchlightCommand::from_frame(&map.end_tick());

        assert!(pressed.check && !pressed.reload);
        assert!(!held.check);
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
    fn f8_is_bound_to_restart() {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        let mut map = ActionMap::<SearchlightAction>::new(bindings, 1.0);
        map.key(Key::F8, true);

        let pressed = SearchlightCommand::from_frame(&map.end_tick());
        let held = SearchlightCommand::from_frame(&map.end_tick());
        map.key(Key::F8, false);
        let released = SearchlightCommand::from_frame(&map.end_tick());
        map.key(Key::F8, true);
        let again = SearchlightCommand::from_frame(&map.end_tick());
        map.key(Key::F8, false);
        map.key(Key::Enter, true);
        let enter = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!(
            SearchlightAction::Restart.default_bindings(),
            [Binding::key(Key::F8)]
        );
        assert!(!enter.restart);
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
    fn searchlight_default_bindings_name_no_protected_key() {
        let protected = [
            Key::Enter,
            Key::Escape,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
        ];

        for &action in SearchlightAction::ALL {
            for binding in action.default_bindings() {
                let on_protected_key = protected
                    .iter()
                    .any(|&key| binding.input == Input::Key(key) && binding.modifiers == NONE);
                assert!(!on_protected_key, "{}: {}", action.name(), binding.name());
                assert!(!binding.is_protected(), "{}", action.name());
            }
        }
        let (bindings, _) = ActionBindings::<SearchlightAction>::load(&BTreeMap::new());
        let (_, warnings) = ActionBindings::<SearchlightAction>::load(&bindings.to_section());
        assert_eq!(warnings, vec![]);
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
    fn a_command_starts_with_simple_off_and_round_gravity_1() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::Fire);

        let sampled = SearchlightCommand::from_frame(&map.end_tick());

        let one = GravityScale::new(1.0).unwrap();
        assert!(!sampled.simple);
        assert_eq!(sampled.round_gravity, one);
        assert!(!SearchlightCommand::default().simple);
        assert_eq!(SearchlightCommand::default().round_gravity, one);
    }

    #[test]
    fn an_idle_tick_is_an_empty_command() {
        let mut map = fresh_map();

        let command = SearchlightCommand::from_frame(&map.end_tick());

        assert_eq!(command, SearchlightCommand::default());
    }

    /// Ends the tick on `map` and resolves its lean against `previous`.
    fn lean_after(
        map: &mut ActionMap<SearchlightAction>,
        previous: Option<LeanSide>,
    ) -> Option<LeanSide> {
        resolve_lean(&map.end_tick(), previous)
    }

    #[test]
    fn a_lean_key_alone_leans_to_its_side_while_held_and_stops_on_release() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        let down = lean_after(&mut map, None);
        let held = lean_after(&mut map, down);
        release(&mut map, SearchlightAction::LeanLeft);
        let up = lean_after(&mut map, held);
        press(&mut map, SearchlightAction::LeanRight);
        let right = lean_after(&mut map, up);

        assert_eq!(
            [down, held, up, right],
            [
                Some(LeanSide::Left),
                Some(LeanSide::Left),
                None,
                Some(LeanSide::Right)
            ]
        );
    }

    #[test]
    fn no_lean_key_means_no_lean_even_after_a_lean() {
        let mut map = fresh_map();

        let idle = lean_after(&mut map, Some(LeanSide::Left));

        assert_eq!(idle, None);
    }

    #[test]
    fn a_key_already_held_with_no_previous_side_still_leans() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        let _first = map.end_tick();

        let held = lean_after(&mut map, None);

        assert_eq!(held, Some(LeanSide::Left));
    }

    #[test]
    fn pressing_e_while_q_is_held_leans_right() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        let left = lean_after(&mut map, None);
        press(&mut map, SearchlightAction::LeanRight);

        let both = lean_after(&mut map, left);
        let still = lean_after(&mut map, both);

        assert_eq!(left, Some(LeanSide::Left));
        assert_eq!(both, Some(LeanSide::Right));
        assert_eq!(still, Some(LeanSide::Right));
    }

    #[test]
    fn pressing_q_while_e_is_held_leans_left() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanRight);
        let right = lean_after(&mut map, None);
        press(&mut map, SearchlightAction::LeanLeft);

        let both = lean_after(&mut map, right);

        assert_eq!(both, Some(LeanSide::Left));
    }

    #[test]
    fn releasing_e_with_q_still_held_leans_left() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        let left = lean_after(&mut map, None);
        press(&mut map, SearchlightAction::LeanRight);
        let right = lean_after(&mut map, left);
        release(&mut map, SearchlightAction::LeanRight);

        let back = lean_after(&mut map, right);
        let steady = lean_after(&mut map, back);

        assert_eq!(right, Some(LeanSide::Right));
        assert_eq!(back, Some(LeanSide::Left));
        assert_eq!(steady, Some(LeanSide::Left));
    }

    #[test]
    fn a_lean_tap_shorter_than_a_tick_leans_that_tick_only() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        release(&mut map, SearchlightAction::LeanLeft);

        let tapped = lean_after(&mut map, None);
        let next = lean_after(&mut map, tapped);

        assert_eq!(tapped, Some(LeanSide::Left));
        assert_eq!(next, None);
    }

    #[test]
    fn q_and_e_pressed_on_one_tick_lean_right() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        press(&mut map, SearchlightAction::LeanRight);

        let tied = lean_after(&mut map, None);

        assert_eq!(tied, Some(LeanSide::Right));
    }

    #[test]
    fn both_keys_held_with_no_previous_side_lean_right() {
        let mut map = fresh_map();
        press(&mut map, SearchlightAction::LeanLeft);
        press(&mut map, SearchlightAction::LeanRight);
        let _first = map.end_tick();

        let held = lean_after(&mut map, None);

        assert_eq!(held, Some(LeanSide::Right));
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
                    acceleration: 0.0,
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
