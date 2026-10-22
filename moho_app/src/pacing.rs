//! Frame-rate cap: decides when the runner may draw the next frame.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

/// Tracks when the last frame was drawn and whether the next one is due.
///
/// With no interval every wake is due. With one, the next frame is due one interval
/// after the previous frame was actually drawn, so a late frame does not make the
/// following ones bunch up.
#[derive(Debug, Clone, Copy, Default)]
pub struct FramePacer {
    interval: Option<Duration>,
    last_frame: Option<Instant>,
}

impl FramePacer {
    /// A pacer drawing at most one frame per `interval`; `None` is uncapped.
    pub fn new(interval: Option<Duration>) -> Self {
        Self {
            interval,
            last_frame: None,
        }
    }

    /// A pacer drawing at most `fps` frames per second; `None` is uncapped.
    pub fn from_fps(fps: Option<NonZeroU32>) -> Self {
        Self::new(fps.map(|fps| Duration::from_secs(1) / fps.get()))
    }

    /// Whether the pacer limits the frame rate.
    pub fn is_capped(&self) -> bool {
        self.interval.is_some()
    }

    /// Whether a frame may be drawn at `now`.
    pub fn due(&self, now: Instant) -> bool {
        self.next_due().is_none_or(|due| now >= due)
    }

    /// When the next frame becomes due; `None` when it is due at any time
    /// (uncapped, or no frame drawn yet).
    pub fn next_due(&self) -> Option<Instant> {
        Some(self.last_frame? + self.interval?)
    }

    /// Records that a frame was drawn at `now`.
    pub fn frame_drawn(&mut self, now: Instant) {
        self.last_frame = Some(now);
    }

    /// When the runner should next wake: the frame deadline while it is still in the
    /// future and before `next_tick`, otherwise `next_tick`.
    pub fn wake_at(&self, now: Instant, next_tick: Instant) -> Instant {
        self.next_due()
            .filter(|&due| due > now)
            .map_or(next_tick, |due| next_tick.min(due))
    }

    /// Changes the cap to `fps` (`None` is uncapped), keeping the last drawn frame.
    pub fn set_fps(&mut self, fps: Option<NonZeroU32>) {
        self.interval = fps.map(|fps| Duration::from_secs(1) / fps.get());
    }

    /// Whether the runner should request a redraw on waking at `now`: the pacer is
    /// capped and a frame is due. Uncapped frames chain from the previous redraw.
    pub fn redraw_on_wake(&self, now: Instant) -> bool {
        self.is_capped() && self.due(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(10);

    #[test]
    fn uncapped_pacer_is_always_due() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::new(None);

        let before_any_frame = pacer.due(t0);
        pacer.frame_drawn(t0);

        assert!(before_any_frame);
        assert!(pacer.due(t0));
        assert!(pacer.due(t0 + Duration::from_nanos(1)));
        assert_eq!(pacer.next_due(), None);
    }

    #[test]
    fn capped_pacer_is_due_for_its_first_frame() {
        let pacer = FramePacer::new(Some(INTERVAL));

        assert!(pacer.due(Instant::now()));
        assert_eq!(pacer.next_due(), None);
    }

    #[test]
    fn capped_pacer_is_due_once_per_interval() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::new(Some(INTERVAL));

        pacer.frame_drawn(t0);

        assert!(!pacer.due(t0));
        assert!(!pacer.due(t0 + INTERVAL / 2));
        assert!(!pacer.due(t0 + Duration::from_nanos(9_999_999)));
        assert!(pacer.due(t0 + INTERVAL));
        assert!(pacer.due(t0 + INTERVAL * 3));
    }

    #[test]
    fn drawing_a_frame_restarts_the_interval() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::new(Some(INTERVAL));
        pacer.frame_drawn(t0);

        pacer.frame_drawn(t0 + INTERVAL);

        assert!(!pacer.due(t0 + INTERVAL + INTERVAL / 2));
        assert!(pacer.due(t0 + INTERVAL * 2));
    }

    #[test]
    fn next_due_is_last_frame_plus_interval() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::new(Some(INTERVAL));

        pacer.frame_drawn(t0);

        assert_eq!(pacer.next_due(), Some(t0 + INTERVAL));
    }

    #[test]
    fn a_late_frame_does_not_bunch_the_next_ones() {
        let t0 = Instant::now();
        let late = t0 + INTERVAL * 5 / 2;
        let mut pacer = FramePacer::new(Some(INTERVAL));
        pacer.frame_drawn(t0);

        pacer.frame_drawn(late);

        assert_eq!(pacer.next_due(), Some(late + INTERVAL));
        assert!(!pacer.due(t0 + INTERVAL * 3));
        assert!(pacer.due(late + INTERVAL));
    }

    #[test]
    fn a_cap_in_frames_per_second_sets_the_matching_interval() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(NonZeroU32::new(100));

        pacer.frame_drawn(t0);

        assert_eq!(pacer.next_due(), Some(t0 + Duration::from_millis(10)));
    }

    #[test]
    fn wake_at_uncapped_waits_for_the_next_tick() {
        let now = Instant::now();
        let next_tick = now + INTERVAL;
        let mut pacer = FramePacer::new(None);
        pacer.frame_drawn(now);

        assert_eq!(pacer.wake_at(now, next_tick), next_tick);
    }

    #[test]
    fn wake_at_capped_before_first_frame_waits_for_the_next_tick() {
        let now = Instant::now();
        let next_tick = now + INTERVAL;
        let pacer = FramePacer::new(Some(INTERVAL));

        assert_eq!(pacer.wake_at(now, next_tick), next_tick);
    }

    #[test]
    fn wake_at_wakes_for_a_future_frame_due_before_the_next_tick() {
        let t0 = Instant::now();
        let now = t0 + INTERVAL / 4;
        let next_tick = t0 + INTERVAL * 5;
        let mut pacer = FramePacer::new(Some(INTERVAL));
        pacer.frame_drawn(t0);

        assert_eq!(pacer.wake_at(now, next_tick), t0 + INTERVAL);
    }

    #[test]
    fn wake_at_prefers_the_next_tick_when_the_frame_is_due_after_it() {
        let t0 = Instant::now();
        let now = t0 + INTERVAL / 4;
        let next_tick = t0 + INTERVAL / 2;
        let mut pacer = FramePacer::new(Some(INTERVAL));
        pacer.frame_drawn(t0);

        assert_eq!(pacer.wake_at(now, next_tick), next_tick);
    }

    #[test]
    fn wake_at_ignores_an_overdue_frame_and_waits_for_the_next_tick() {
        let t0 = Instant::now();
        let now = t0 + INTERVAL * 2;
        let next_tick = now + INTERVAL;
        let mut pacer = FramePacer::new(Some(INTERVAL));
        pacer.frame_drawn(t0);

        assert_eq!(pacer.wake_at(now, next_tick), next_tick);
    }

    #[test]
    fn wake_at_ignores_a_frame_due_exactly_now() {
        let t0 = Instant::now();
        let now = t0 + INTERVAL;
        let next_tick = now + INTERVAL;
        let mut pacer = FramePacer::new(Some(INTERVAL));
        pacer.frame_drawn(t0);

        assert_eq!(pacer.wake_at(now, next_tick), next_tick);
    }

    #[test]
    fn set_fps_to_the_same_cap_keeps_the_last_frame() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(NonZeroU32::new(100));
        pacer.frame_drawn(t0);

        pacer.set_fps(NonZeroU32::new(100));

        assert!(!pacer.due(t0 + INTERVAL / 2));
        assert_eq!(pacer.next_due(), Some(t0 + INTERVAL));
    }

    #[test]
    fn set_fps_to_a_different_cap_uses_the_new_interval_from_the_last_frame() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(NonZeroU32::new(100));
        pacer.frame_drawn(t0);

        pacer.set_fps(NonZeroU32::new(50));

        assert_eq!(pacer.next_due(), Some(t0 + Duration::from_millis(20)));
        assert!(!pacer.due(t0 + INTERVAL));
    }

    #[test]
    fn set_fps_none_makes_the_pacer_uncapped_and_always_due() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(NonZeroU32::new(100));
        pacer.frame_drawn(t0);

        pacer.set_fps(None);

        assert!(!pacer.is_capped());
        assert!(pacer.due(t0));
        assert_eq!(pacer.next_due(), None);
    }

    #[test]
    fn set_fps_from_uncapped_to_a_cap_applies_to_the_last_frame() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(None);
        pacer.frame_drawn(t0);

        pacer.set_fps(NonZeroU32::new(100));

        assert_eq!(pacer.next_due(), Some(t0 + INTERVAL));
    }

    #[test]
    fn no_fps_cap_is_uncapped() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(None);

        pacer.frame_drawn(t0);

        assert!(pacer.due(t0));
        assert_eq!(pacer.next_due(), None);
    }
}
