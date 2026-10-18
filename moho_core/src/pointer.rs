//! Turns a screen point into a world ray.

use glam::{Mat4, UVec2, Vec2, Vec3};

/// A half-line in world space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    /// Unit length.
    pub direction: Vec3,
}

/// The world ray through `point`, in physical pixels from the top-left of a `viewport`.
///
/// `proj` must map depth to `0..1` (`glam::camera::rh::proj::directx`).
/// Returns `None` for a zero-sized viewport or non-finite input.
pub fn screen_ray(_view: Mat4, _proj: Mat4, _viewport: UVec2, _point: Vec2) -> Option<Ray> {
    Some(Ray {
        origin: Vec3::ZERO,
        direction: Vec3::ZERO,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};
    use proptest::prelude::*;

    const NEAR: f32 = 0.1;
    const FAR: f32 = 500.0;
    const FOV_Y: f32 = std::f32::consts::FRAC_PI_3;

    fn camera(eye: Vec3, forward: Vec3, viewport: UVec2) -> (Mat4, Mat4) {
        let view = look_at_mat4(eye, eye + forward, Vec3::Y);
        let aspect = viewport.x as f32 / viewport.y as f32;
        (view, perspective(FOV_Y, aspect, NEAR, FAR))
    }

    fn pixel_of(world: Vec3, view: Mat4, proj: Mat4, viewport: UVec2) -> Vec2 {
        let ndc = (proj * view).project_point3(world);
        Vec2::new(
            (ndc.x + 1.0) * 0.5 * viewport.x as f32,
            (1.0 - ndc.y) * 0.5 * viewport.y as f32,
        )
    }

    fn distance_to_line(ray: Ray, point: Vec3) -> f32 {
        (point - ray.origin).cross(ray.direction).length()
    }

    #[test]
    fn screen_centre_ray_runs_along_forward() {
        let viewport = UVec2::new(1280, 720);
        let eye = Vec3::new(1.0, 2.0, 3.0);
        let forward = Vec3::new(0.3, -0.2, -1.0).normalize();
        let (view, proj) = camera(eye, forward, viewport);

        let ray = screen_ray(view, proj, viewport, Vec2::new(640.0, 360.0)).expect("ray");

        assert!(
            ray.direction.distance(forward) < 1e-4,
            "{:?} vs {forward:?}",
            ray.direction
        );
    }

    #[test]
    fn corner_ray_runs_along_the_frustum_edge() {
        let viewport = UVec2::new(1280, 720);
        let eye = Vec3::new(-4.0, 1.5, 8.0);
        let forward = Vec3::new(-0.4, -0.1, -1.0).normalize();
        let (view, proj) = camera(eye, forward, viewport);
        let half_height = (FOV_Y * 0.5).tan();
        let half_width = half_height * 1280.0 / 720.0;
        let corners = [
            (
                Vec2::new(0.0, 0.0),
                Vec3::new(-half_width, half_height, -1.0),
            ),
            (
                Vec2::new(1280.0, 0.0),
                Vec3::new(half_width, half_height, -1.0),
            ),
            (
                Vec2::new(0.0, 720.0),
                Vec3::new(-half_width, -half_height, -1.0),
            ),
            (
                Vec2::new(1280.0, 720.0),
                Vec3::new(half_width, -half_height, -1.0),
            ),
        ];

        for (point, view_space_edge) in corners {
            let ray = screen_ray(view, proj, viewport, point).expect("ray");
            let expected = view
                .inverse()
                .transform_vector3(view_space_edge)
                .normalize();

            assert!(
                ray.direction.distance(expected) < 1e-4,
                "{point:?}: {:?} vs {expected:?}",
                ray.direction
            );
        }
    }

    #[test]
    fn ray_starts_within_the_near_distance_of_the_eye() {
        let viewport = UVec2::new(1280, 720);
        let eye = Vec3::new(10.0, 20.0, -5.0);
        let forward = Vec3::new(1.0, -0.5, 0.2).normalize();
        let (view, proj) = camera(eye, forward, viewport);

        for point in [
            Vec2::new(640.0, 360.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(1280.0, 720.0),
        ] {
            let ray = screen_ray(view, proj, viewport, point).expect("ray");
            let offset = ray.origin - eye;

            assert!(
                (offset.dot(forward) - NEAR).abs() < 1e-4,
                "{point:?}: depth {}",
                offset.dot(forward)
            );
            assert!(offset.length() >= NEAR - 1e-4, "{point:?}");
            assert!(offset.length() < NEAR * 2.0, "{point:?}");
        }
    }

    proptest! {
        #[test]
        fn ray_passes_through_the_point_drawn_at_that_pixel(
            eye in prop::array::uniform3(-100.0f32..100.0),
            yaw in -3.1f32..3.1,
            pitch in -1.2f32..1.2,
            ndc_x in -0.9f32..0.9,
            ndc_y in -0.9f32..0.9,
            depth in 1.0f32..100.0,
        ) {
            let viewport = UVec2::new(1280, 720);
            let eye = Vec3::from_array(eye);
            let forward = Vec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                -yaw.cos() * pitch.cos(),
            );
            let (view, proj) = camera(eye, forward, viewport);
            let half_height = (FOV_Y * 0.5).tan() * depth;
            let half_width = half_height * 1280.0 / 720.0;
            let world = view
                .inverse()
                .transform_point3(Vec3::new(ndc_x * half_width, ndc_y * half_height, -depth));
            let pixel = pixel_of(world, view, proj, viewport);

            let ray = screen_ray(view, proj, viewport, pixel).expect("ray");

            let miss = distance_to_line(ray, world) / world.distance(eye);
            prop_assert!(miss < 1e-3, "missed by {miss} relative");
            prop_assert!((ray.direction.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn same_point_at_any_viewport_size() {
        let eye = Vec3::new(2.0, 3.0, 4.0);
        let forward = Vec3::new(0.1, -0.3, -1.0).normalize();
        let world = eye + view_offset(forward, Vec3::new(0.5, 0.3, -10.0));
        let mut directions = Vec::new();

        for viewport in [
            UVec2::new(1280, 720),
            UVec2::new(2560, 1440),
            UVec2::new(1000, 1000),
        ] {
            let (view, proj) = camera(eye, forward, viewport);
            let pixel = pixel_of(world, view, proj, viewport);

            let ray = screen_ray(view, proj, viewport, pixel).expect("ray");

            assert!((ray.direction.length() - 1.0).abs() < 1e-4, "{viewport:?}");
            let miss = distance_to_line(ray, world) / world.distance(eye);
            assert!(miss < 1e-3, "{viewport:?}: missed by {miss} relative");
            directions.push(ray.direction);
        }

        for direction in &directions {
            assert!(
                direction.distance(directions[0]) < 1e-3,
                "{direction:?} vs {:?}",
                directions[0]
            );
        }
    }

    fn view_offset(forward: Vec3, view_space: Vec3) -> Vec3 {
        look_at_mat4(Vec3::ZERO, forward, Vec3::Y)
            .inverse()
            .transform_vector3(view_space)
    }

    #[test]
    fn zero_viewport_has_no_ray() {
        let (view, proj) = camera(Vec3::ZERO, Vec3::NEG_Z, UVec2::new(1280, 720));

        for viewport in [UVec2::new(0, 720), UVec2::new(1280, 0), UVec2::ZERO] {
            assert_eq!(
                screen_ray(view, proj, viewport, Vec2::new(10.0, 10.0)),
                None,
                "{viewport:?}"
            );
        }
    }

    #[test]
    fn non_finite_point_has_no_ray() {
        let viewport = UVec2::new(1280, 720);
        let (view, proj) = camera(Vec3::ZERO, Vec3::NEG_Z, viewport);

        for point in [Vec2::new(f32::NAN, 10.0), Vec2::new(10.0, f32::INFINITY)] {
            assert_eq!(screen_ray(view, proj, viewport, point), None, "{point:?}");
        }
    }
}
