//! What a game asks of the loop during a callback, applied by the loop afterwards.

use crate::{SimSpeed, WindowMode};

/// Pause, speed and window-mode changes a game asked for, applied by the loop after the
/// callback returns.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LoopRequests {
    pub(crate) paused: Option<bool>,
    pub(crate) speed: Option<SimSpeed>,
    pub(crate) window_mode: Option<WindowMode>,
}

impl LoopRequests {
    /// The window mode the runner holds once these requests are applied to `current`.
    pub(crate) fn window_mode_after(self, current: WindowMode) -> WindowMode {
        self.window_mode.unwrap_or(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventContext, FrameContext, GameClock};
    use std::time::Duration;

    const TICK: Duration = Duration::from_millis(16);

    #[test]
    fn a_requested_mode_reaches_the_runner_state() {
        let clock = GameClock::default();
        let mut event = EventContext::new(&clock, None, None);
        let mut frame = FrameContext::new(TICK, &clock, None, None);

        event.set_window_mode(WindowMode::Borderless);
        frame.set_window_mode(WindowMode::Borderless);
        let from_event = event
            .loop_requests()
            .window_mode_after(WindowMode::Windowed);
        let from_frame = frame
            .loop_requests()
            .window_mode_after(WindowMode::Windowed);
        let back_from_event = {
            let mut ctx = EventContext::new(&clock, None, None);
            ctx.set_window_mode(WindowMode::Windowed);
            ctx.loop_requests()
                .window_mode_after(WindowMode::Borderless)
        };

        assert_eq!(from_event, WindowMode::Borderless);
        assert_eq!(from_frame, WindowMode::Borderless);
        assert_eq!(back_from_event, WindowMode::Windowed);
    }

    #[test]
    fn no_request_leaves_the_runner_mode_alone() {
        let clock = GameClock::default();
        let ctx = EventContext::new(&clock, None, None);

        let kept = ctx
            .loop_requests()
            .window_mode_after(WindowMode::Borderless);

        assert_eq!(kept, WindowMode::Borderless);
        assert_eq!(ctx.requested_window_mode(), None);
    }

    #[test]
    fn a_game_reads_the_current_mode_in_event_and_frame() {
        let clock = GameClock::default();

        for mode in [WindowMode::Windowed, WindowMode::Borderless] {
            let event = EventContext::new(&clock, None, None).with_window_mode(mode);
            let frame = FrameContext::new(TICK, &clock, None, None).with_window_mode(mode);

            assert_eq!(event.window_mode(), mode);
            assert_eq!(frame.window_mode(), mode);
        }
    }

    #[test]
    fn a_request_does_not_change_the_mode_the_context_reports() {
        let clock = GameClock::default();
        let mut event =
            EventContext::new(&clock, None, None).with_window_mode(WindowMode::Windowed);
        let mut frame =
            FrameContext::new(TICK, &clock, None, None).with_window_mode(WindowMode::Windowed);

        event.set_window_mode(WindowMode::Borderless);
        frame.set_window_mode(WindowMode::Borderless);

        assert_eq!(event.window_mode(), WindowMode::Windowed);
        assert_eq!(frame.window_mode(), WindowMode::Windowed);
        assert_eq!(event.requested_window_mode(), Some(WindowMode::Borderless));
        assert_eq!(frame.requested_window_mode(), Some(WindowMode::Borderless));
    }
}
