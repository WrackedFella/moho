//! The views the main menu cuts between behind itself, like a security feed.

use std::time::Duration;

use glam::{Mat4, Vec3};

/// How long each camera shows before the feed cuts to the next.
pub const CUT_INTERVAL: Duration = Duration::from_secs(5);

/// One view of the level: where the eye is and the way it looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuCamera {
    pub eye: Vec3,
    /// Unit direction the camera looks along.
    pub forward: Vec3,
}

/// The cameras of a level and which one shows now.
#[derive(Debug, Clone)]
pub struct MenuFeed {
    cameras: Vec<MenuCamera>,
    shown: usize,
    elapsed: Duration,
}

impl MenuFeed {
    /// A feed that starts on the first of `cameras`.
    pub fn new(cameras: Vec<MenuCamera>) -> Self {
        Self {
            cameras,
            shown: 0,
            elapsed: Duration::ZERO,
        }
    }

    /// Lets `dt` of frame time pass, cutting to the next camera (wrapping after the last) each
    /// [`CUT_INTERVAL`].
    pub fn advance(&mut self, _dt: Duration) {
        todo!("advance the feed")
    }

    /// The view matrix and eye of the camera showing now, or `None` without cameras.
    pub fn view(&self) -> Option<(Mat4, Vec3)> {
        todo!("the shown camera's view")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-4;

    fn camera(x: f32) -> MenuCamera {
        MenuCamera {
            eye: Vec3::new(x, 1.7, 0.0),
            forward: Vec3::Z,
        }
    }

    fn feed_of(count: usize) -> MenuFeed {
        MenuFeed::new((0..count).map(|i| camera(i as f32 * 10.0)).collect())
    }

    /// The eye of the camera showing now.
    fn shown_x(feed: &MenuFeed) -> f32 {
        feed.view().expect("the feed has cameras").1.x
    }

    #[test]
    fn the_cut_interval_is_five_seconds() {
        assert_eq!(CUT_INTERVAL, Duration::from_secs(5));
    }

    #[test]
    fn a_feed_starts_on_its_first_camera() {
        let feed = feed_of(3);

        assert_eq!(shown_x(&feed), 0.0);
    }

    #[test]
    fn the_view_looks_from_the_eye_along_forward() {
        let looking = MenuCamera {
            eye: Vec3::new(4.0, 1.7, -2.0),
            forward: Vec3::X,
        };
        let feed = MenuFeed::new(vec![looking]);

        let (view, eye) = feed.view().expect("one camera");

        assert_eq!(eye, looking.eye);
        let at_eye = view.transform_point3(looking.eye);
        let ahead = view.transform_point3(looking.eye + looking.forward * 3.0);
        assert!(at_eye.length() < EPSILON, "{at_eye}");
        assert!(
            (ahead - Vec3::new(0.0, 0.0, -3.0)).length() < EPSILON,
            "the camera looks down its own -Z: {ahead}"
        );
    }

    #[test]
    fn a_feed_without_cameras_has_no_view_and_ignores_time() {
        let mut feed = feed_of(0);

        feed.advance(Duration::from_secs(60));

        assert_eq!(feed.view(), None);
    }

    #[test]
    fn a_cut_comes_after_the_interval_and_not_before() {
        let mut feed = feed_of(3);

        feed.advance(CUT_INTERVAL - Duration::from_millis(1));
        let before = shown_x(&feed);
        feed.advance(Duration::from_millis(1));
        let at = shown_x(&feed);

        assert_eq!(before, 0.0, "cut early");
        assert_eq!(at, 10.0, "no cut at the interval");
    }

    #[test]
    fn small_frames_add_up_to_a_cut() {
        let mut feed = feed_of(3);

        for _ in 0..299 {
            feed.advance(Duration::from_secs(1) / 60);
        }
        let before = shown_x(&feed);
        feed.advance(Duration::from_secs(1) / 60);
        let after = shown_x(&feed);

        assert_eq!(before, 0.0);
        assert_eq!(after, 10.0);
    }

    #[test]
    fn after_the_last_camera_the_feed_returns_to_the_first() {
        let mut feed = feed_of(3);
        let mut seen = vec![shown_x(&feed)];

        for _ in 0..3 {
            feed.advance(CUT_INTERVAL);
            seen.push(shown_x(&feed));
        }

        assert_eq!(seen, [0.0, 10.0, 20.0, 0.0]);
    }

    #[test]
    fn a_frame_spanning_two_intervals_cuts_twice() {
        let mut feed = feed_of(3);

        feed.advance(CUT_INTERVAL * 2);

        assert_eq!(shown_x(&feed), 20.0);
    }

    #[test]
    fn a_frame_spanning_more_than_a_whole_loop_wraps() {
        let mut feed = feed_of(3);

        feed.advance(CUT_INTERVAL * 4);

        assert_eq!(shown_x(&feed), 10.0);
    }

    #[test]
    fn the_time_left_over_after_a_cut_counts_toward_the_next() {
        let mut feed = feed_of(3);

        feed.advance(CUT_INTERVAL + Duration::from_secs(2));
        let after_cut = shown_x(&feed);
        feed.advance(Duration::from_millis(2999));
        let just_before = shown_x(&feed);
        feed.advance(Duration::from_millis(1));
        let at_next = shown_x(&feed);

        assert_eq!(after_cut, 10.0);
        assert_eq!(just_before, 10.0);
        assert_eq!(at_next, 20.0);
    }

    #[test]
    fn a_single_camera_stays_on_itself() {
        let mut feed = feed_of(1);

        feed.advance(CUT_INTERVAL * 3);

        assert_eq!(shown_x(&feed), 0.0);
    }
}
