use crate::*;
use glam::Vec3;

#[derive(Copy, Clone, Debug)]
pub struct Camera {
    pub origin: Vec3,
    pub lower_left_corner: Vec3,
    pub horizontal: Vec3,
    pub vertical: Vec3,
    pub lens_radius: f32,
    pub u: Vec3,
    pub v: Vec3,
    pub w: Vec3,
}

impl Camera {
    pub fn new(
        look_from: Vec3,
        look_at: Vec3,
        v_up: Vec3,
        vfov: f32,
        aspect: f32,
        aperture: f32,
        focus_dist: f32,
    ) -> Camera {
        let theta = vfov * std::f32::consts::PI / 180f32;
        let half_height = (theta / 2f32).tan();
        let half_width = aspect * half_height;
        let temp_w: Vec3 = (look_from - look_at).normalize();
        let temp_u: Vec3 = v_up.cross(temp_w).normalize();
        let temp_v: Vec3 = temp_w.cross(temp_u);

        Camera {
            lower_left_corner: look_from
                - half_width * focus_dist * temp_u
                - half_height * focus_dist * temp_v
                - focus_dist * temp_w,
            horizontal: 2f32 * half_width * focus_dist * temp_u,
            vertical: 2f32 * half_width * focus_dist * temp_v,
            origin: look_from,
            lens_radius: aperture / 2f32,
            w: temp_w,
            u: temp_u,
            v: temp_v,
        }
    }
    pub fn get_ray(self, s: f32, t: f32) -> Ray {
        let rd: Vec3 = self.lens_radius * random_in_unit_disk();
        let offset = self.u * rd.x + self.v * rd.y;
        Ray::new(
            self.origin + offset,
            self.lower_left_corner + s * self.horizontal + t * self.vertical - self.origin - offset,
        )
    }
}
