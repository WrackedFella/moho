//! The views the main menu shows the level through, one after another.

use std::time::Duration;

use glam::{Mat4, Vec3};

use crate::arena::MenuCamera;
use crate::lean::rolled_view_matrix;

/// How long each menu camera stays on screen before the feed cuts to the next.
pub const CUT_INTERVAL: Duration = Duration::from_secs(5);

/// The menu cameras and which one is on screen.
#[derive(Debug, Clone)]
pub struct MenuFeed {
    cameras: Vec<MenuCamera>,
    shown: usize,
    elapsed: Duration,
}

impl MenuFeed {
    pub fn new(cameras: Vec<MenuCamera>) -> Self {
        Self {
            cameras,
            shown: 0,
            elapsed: Duration::ZERO,
        }
    }

    /// Lets `dt` of frame time pass, cutting to the next camera for every [`CUT_INTERVAL`]
    /// elapsed and wrapping from the last camera to the first.
    pub fn advance(&mut self, dt: Duration) {
        if self.cameras.is_empty() {
            return;
        }
        let total = (self.elapsed + dt).as_nanos();
        let interval = CUT_INTERVAL.as_nanos();
        let cuts = (total / interval) % self.cameras.len() as u128;
        self.shown = (self.shown + cuts as usize) % self.cameras.len();
        self.elapsed = Duration::from_nanos((total % interval) as u64);
    }

    /// The view matrix and eye of the camera on screen; `None` when there are no cameras.
    pub fn view(&self) -> Option<(Mat4, Vec3)> {
        let camera = self.cameras.get(self.shown)?;
        Some((
            rolled_view_matrix(camera.eye, camera.forward, 0.0),
            camera.eye,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lean::rolled_view_matrix;

    const MS: Duration = Duration::from_millis(1);

    fn camera(x: f32, forward: Vec3) -> MenuCamera {
        MenuCamera {
            eye: Vec3::new(x, 1.7, 0.0),
            forward,
        }
    }

    fn three_cameras() -> [MenuCamera; 3] {
        [
            camera(1.0, Vec3::Z),
            camera(2.0, Vec3::X),
            camera(3.0, Vec3::NEG_Z),
        ]
    }

    fn eye_shown(feed: &MenuFeed) -> Option<Vec3> {
        feed.view().map(|(_, eye)| eye)
    }

    #[test]
    fn a_cut_comes_after_the_interval_and_not_before() {
        let [first, second, _] = three_cameras();
        let mut feed = MenuFeed::new(three_cameras().to_vec());
        let at_start = feed.view();

        feed.advance(Duration::from_millis(4999));
        let just_before = eye_shown(&feed);
        feed.advance(MS);
        let at_the_cut = feed.view();

        assert_eq!(CUT_INTERVAL, Duration::from_secs(5));
        assert_eq!(
            at_start,
            Some((rolled_view_matrix(first.eye, first.forward, 0.0), first.eye))
        );
        assert_eq!(just_before, Some(first.eye));
        assert_eq!(
            at_the_cut,
            Some((
                rolled_view_matrix(second.eye, second.forward, 0.0),
                second.eye
            ))
        );
    }

    #[test]
    fn after_the_last_camera_the_feed_returns_to_the_first() {
        let [first, second, third] = three_cameras();
        let mut feed = MenuFeed::new(three_cameras().to_vec());

        let mut eyes = Vec::new();
        for _ in 0..3 {
            feed.advance(CUT_INTERVAL);
            eyes.push(eye_shown(&feed));
        }

        assert_eq!(eyes, [Some(second.eye), Some(third.eye), Some(first.eye)]);
    }

    #[test]
    fn a_frame_spanning_two_intervals_cuts_twice() {
        let [first, _, third] = three_cameras();
        let mut feed = MenuFeed::new(three_cameras().to_vec());

        feed.advance(CUT_INTERVAL * 2 + 2 * MS);
        let after_the_long_frame = eye_shown(&feed);
        feed.advance(Duration::from_millis(4998));
        let after_the_carried_remainder = eye_shown(&feed);

        assert_eq!(after_the_long_frame, Some(third.eye));
        assert_eq!(after_the_carried_remainder, Some(first.eye));
    }
}
