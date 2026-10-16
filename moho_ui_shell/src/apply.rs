//! The default way a game applies the engine's commands.

use moho_app::FrameContext;

use crate::console::registry::EngineCommand;

/// Applies `command`: `Quit` asks the runner to exit, as closing the window does.
pub fn apply_engine_command(command: &EngineCommand, ctx: &mut FrameContext<'_>) {
    match command {
        EngineCommand::Quit => ctx.request_exit(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use moho_app::GameClock;

    use super::*;

    #[test]
    fn quit_requests_exit() {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, None, None);
        assert!(!ctx.exit_requested());

        apply_engine_command(&EngineCommand::Quit, &mut ctx);

        assert!(ctx.exit_requested());
    }
}
