//! The views the main menu cuts between behind itself, like a security feed.

use std::time::Duration;

use glam::{Mat4, Vec3};

/// How long each camera shows before the feed cuts to the next.
pub const CUT_INTERVAL: Duration = Duration::from_secs(5);

/// How far along Y a unit direction points before Y is no use as the view's up.
const NEAR_VERTICAL: f32 = 0.9999;

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
    pub fn advance(&mut self, dt: Duration) {
        if self.cameras.is_empty() {
            return;
        }
        let interval = CUT_INTERVAL.as_nanos();
        // Whole nanoseconds in u128 cannot overflow for any two `Duration`s, however long.
        let total = self.elapsed.as_nanos() + dt.as_nanos();
        let cuts = total / interval;
        let count = self.cameras.len() as u128;
        let shown = (self.shown as u128 + cuts % count) % count;
        self.shown = usize::try_from(shown).expect("below the camera count");
        self.elapsed = Duration::from_nanos(
            u64::try_from(total % interval).expect("a remainder under five seconds fits"),
        );
    }

    /// The view matrix and eye of the camera showing now, or `None` without cameras.
    pub fn view(&self) -> Option<(Mat4, Vec3)> {
        let camera = self.cameras.get(self.shown)?;
        // Straight up or down, Y gives no sideways direction; any horizontal axis will do.
        let up = if camera.forward.normalize().dot(Vec3::Y).abs() > NEAR_VERTICAL {
            Vec3::Z
        } else {
            Vec3::Y
        };
        Some((
            glam::camera::rh::view::look_to_mat4(camera.eye, camera.forward, up),
            camera.eye,
        ))
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

        feed.advance(Duration::from_millis(4999));
        let before = shown_x(&feed);
        feed.advance(Duration::from_millis(1));
        let at = shown_x(&feed);

        assert_eq!(before, 0.0, "cut early");
        assert_eq!(at, 10.0, "no cut at the interval");
    }

    #[test]
    fn small_frames_add_up_to_a_cut() {
        let mut feed = feed_of(3);

        for _ in 0..99 {
            feed.advance(Duration::from_millis(50));
        }
        let before = shown_x(&feed);
        feed.advance(Duration::from_millis(50));
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

    #[test]
    fn a_camera_looking_straight_up_or_down_still_has_a_finite_view() {
        for forward in [Vec3::Y, Vec3::NEG_Y] {
            let eye = Vec3::new(3.0, 1.7, -2.0);
            let feed = MenuFeed::new(vec![MenuCamera { eye, forward }]);

            let (view, shown_eye) = feed.view().expect("one camera");

            assert!(view.is_finite(), "{forward}: {view:?}");
            assert_eq!(shown_eye, eye);
            let at_eye = view.transform_point3(eye);
            assert!(at_eye.length() < EPSILON, "{forward}: {at_eye}");
            let ahead = view.transform_point3(eye + forward * 3.0);
            assert!(
                (ahead - Vec3::new(0.0, 0.0, -3.0)).length() < EPSILON,
                "{forward}: looks down its own -Z: {ahead}"
            );
        }
    }

    /// The camera a feed of three shows after `secs` whole seconds, by the arithmetic of cuts.
    fn expected_after(secs: u64) -> f32 {
        let index = usize::try_from(secs / CUT_INTERVAL.as_secs() % 3).expect("below three");
        [0.0, 10.0, 20.0][index]
    }

    #[test]
    fn a_year_of_frame_time_lands_on_the_camera_its_cuts_give() {
        let secs = 3600 * 24 * 365 + 5;
        let mut feed = feed_of(3);

        feed.advance(Duration::from_secs(secs));

        assert_eq!(shown_x(&feed), expected_after(secs));
        assert_eq!(expected_after(secs), 10.0, "the case must move the feed");
    }

    #[test]
    fn an_enormous_frame_time_neither_hangs_nor_panics_and_lands_on_the_right_camera() {
        let secs = u64::MAX / 4 + 5;
        let mut feed = feed_of(3);

        feed.advance(Duration::from_secs(secs));
        let first = shown_x(&feed);
        feed.advance(Duration::from_secs(2));
        feed.advance(Duration::from_secs(3));

        assert_eq!(first, expected_after(secs));
        assert_eq!(shown_x(&feed), expected_after(secs + 5));
    }

    #[test]
    fn two_enormous_frames_in_a_row_do_not_overflow_the_clock() {
        let mut feed = feed_of(3);

        feed.advance(Duration::MAX);
        feed.advance(Duration::MAX);

        assert!(feed.view().is_some());
    }
}
