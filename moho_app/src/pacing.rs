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
    pub fn from_fps(_fps: Option<NonZeroU32>) -> Self {
        Self::new(None)
    }

    /// Whether a frame may be drawn at `now`.
    pub fn due(&self, _now: Instant) -> bool {
        true
    }

    /// When the next frame becomes due; `None` when it is due at any time
    /// (uncapped, or no frame drawn yet).
    pub fn next_due(&self) -> Option<Instant> {
        None
    }

    /// Records that a frame was drawn at `now`.
    pub fn frame_drawn(&mut self, _now: Instant) {}
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
        assert!(!pacer.due(t0 + INTERVAL - Duration::from_nanos(1)));
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
    fn no_fps_cap_is_uncapped() {
        let t0 = Instant::now();
        let mut pacer = FramePacer::from_fps(None);

        pacer.frame_drawn(t0);

        assert!(pacer.due(t0));
        assert_eq!(pacer.next_due(), None);
    }
}
