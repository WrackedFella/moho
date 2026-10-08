//! The strategy game as a [`moho_app::Game`].

use crate::App;
use moho_game::controller::ControllerInput;

/// One tick's worth of player input.
#[derive(Clone, Copy, Debug, Default)]
pub struct StrategyCommand {
    pub input: ControllerInput,
    pub jump: bool,
}

impl moho_app::Game for App {
    type Command = StrategyCommand;

    fn command(&mut self) -> StrategyCommand {
        todo!("App::command")
    }

    fn tick(&mut self, _ctx: &mut moho_app::TickContext, _command: &StrategyCommand) {
        todo!("App::tick")
    }

    fn frame(&mut self, _ctx: &mut moho_app::FrameContext<'_>, _alpha: f32) {
        todo!("App::frame")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use moho_app::{Game, LoopConfig, TickContext};
    use moho_game::TICK_HZ;
    use moho_input::bindings::Binding;
    use moho_ui::actions::StrategyAction;

    fn playing_app() -> App {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(10.0, 20.0, 30.0), 0.7, 0.0);
        app
    }

    fn hold(app: &mut App, action: StrategyAction) {
        let Binding::Key(key) = *app
            .bindings
            .get(action)
            .first()
            .expect("the action has a default binding");
        app.input.active_keys.insert(key);
    }

    fn tick_context() -> TickContext {
        TickContext {
            tick: 0,
            tick_length: LoopConfig::new(TICK_HZ).tick_length(),
        }
    }

    #[test]
    fn held_forward_binding_is_in_the_tick_command_and_moves_player() {
        let mut app = playing_app();
        hold(&mut app, StrategyAction::MoveForward);
        let (yaw, _) = app.simulation.yaw_pitch();
        let expected_dir = glam::Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let before = app.simulation.position();
        let speed = app.simulation.player_controller.speed;
        let ctx_dt = tick_context().tick_length.as_secs_f32();

        let cmd = Game::command(&mut app);
        Game::tick(&mut app, &mut tick_context(), &cmd);

        let moved = app.simulation.position() - before;
        assert_eq!(cmd.input.forward, 1.0);
        assert!(
            moved.dot(expected_dir) > 0.9 * speed * ctx_dt,
            "player should advance along its facing, moved {moved:?}"
        );
        assert!(
            moved.normalize_or_zero().dot(expected_dir) > 0.99,
            "movement should follow the facing, moved {moved:?}"
        );
    }

    #[test]
    fn tick_without_held_keys_does_not_move_player() {
        let mut app = playing_app();
        let before = app.simulation.position();

        let cmd = Game::command(&mut app);
        Game::tick(&mut app, &mut tick_context(), &cmd);

        assert_eq!(cmd.input.forward, 0.0);
        assert!(!cmd.jump);
        assert!(
            (app.simulation.position() - before).length() < 1e-6,
            "no input, no movement"
        );
    }

    #[test]
    fn held_jump_binding_sets_the_command_jump_flag() {
        let mut app = playing_app();
        hold(&mut app, StrategyAction::Jump);

        let cmd = Game::command(&mut app);

        assert!(cmd.jump);
        assert_eq!(cmd.input.forward, 0.0);
    }

    #[test]
    fn command_outside_playing_is_the_default_command() {
        let mut app = playing_app();
        app.game_state = GameState::Menu;
        hold(&mut app, StrategyAction::MoveForward);
        hold(&mut app, StrategyAction::Jump);

        let cmd = Game::command(&mut app);

        assert_eq!(cmd.input.forward, 0.0);
        assert!(!cmd.jump);
    }
}
