//! The prototype as a [`moho_app::Game`]: samples input, ticks the sim, and turns its
//! feedback into sound, a title status line, a hit flash and a crosshair.

use moho_app::{FrameContext, Game, TickContext};
use moho_input::action_map::ActionMap;

use crate::input::{FpsAction, FpsCommand};
use crate::sim::FpsSim;

/// The FPS prototype's game: one sim and the input that drives it.
#[derive(Debug)]
pub struct FpsGame {
    _private: (),
}

impl FpsGame {
    /// A game ticking `sim` with commands sampled from `input`.
    pub fn new(_sim: FpsSim, _input: ActionMap<FpsAction>) -> Self {
        unimplemented!("hold the sim and the input map")
    }

    pub fn sim(&self) -> &FpsSim {
        unimplemented!("the sim")
    }

    /// The input map, for feeding it device events.
    pub fn input_mut(&mut self) -> &mut ActionMap<FpsAction> {
        unimplemented!("the input map")
    }
}

impl Game for FpsGame {
    type Command = FpsCommand;

    fn command(&mut self) -> FpsCommand {
        unimplemented!("sample the action map into a command")
    }

    fn tick(&mut self, _ctx: &mut TickContext<'_>, _command: &FpsCommand) {
        unimplemented!("tick the sim at the loop's tick length")
    }

    fn frame(&mut self, _ctx: &mut FrameContext<'_>, _alpha: f32) {
        unimplemented!("drain the sim's feedback into sound, title, flash and crosshair")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use glam::Vec3;
    use moho_app::{HeadlessLoop, LoopConfig};
    use moho_input::bindings::ActionBindings;

    use super::*;
    use crate::sim::Feedback;
    use crate::test_support::{flat_floor, mag, press, test_config};
    use moho_fps_rules::Rifle;

    fn game() -> FpsGame {
        let sim = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(Rifle::loaded(mag(30, 5)), vec![]),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        FpsGame::new(sim, ActionMap::new(bindings, 1.0))
    }

    #[test]
    fn a_headless_tick_turns_a_trigger_press_into_a_shot() {
        let mut game = game();
        press(game.input_mut(), FpsAction::Fire);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));

        headless.step(&mut game, 1);

        assert_eq!(game.sim().pending_feedback(), [Feedback::Fired]);
        assert_eq!(game.sim().rifle().magazine().rounds(), 3);
    }

    #[test]
    fn a_headless_frame_drains_the_feedback_without_audio_or_window() {
        let mut game = game();
        press(game.input_mut(), FpsAction::Fire);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.step(&mut game, 1);

        headless.advance(&mut game, Duration::ZERO);

        assert!(game.sim().pending_feedback().is_empty());
    }
}
