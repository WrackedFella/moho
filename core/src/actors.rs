extern crate glam;
use glam::Vec3;
use crate::*;
use crate::materials::MaterialType;

#[derive(Copy, Clone)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
    pub mat_ptr: MaterialType
}

impl Sphere {
    pub fn new(cen: Vec3, r: f32, mat: MaterialType) -> Sphere {
        Sphere {
            center: cen,
            radius: r,
            mat_ptr: mat
        }
    }
}

impl Hittable for Sphere {
    fn hit<'b>(&self, mut r: Ray, t_min: f32, t_max: f32) -> Option<HittableRecord> {
        let oc: Vec3 = r.origin() - &self.center;
        let a: f32 = r.direction().dot(r.direction());
        let b: f32 = oc.dot(r.direction());
        let c: f32 = oc.dot(oc) - self.radius*self.radius;
        let discriminant: f32 = b*b - a*c;
        if discriminant > 0f32 {
            let mut temp = (-b - (b*b-a*c).sqrt()) / a;
            if temp < t_max && temp > t_min {
                let point = r.point_at_parameter(temp);
                // ...existing code omitted intentionally; return a simple record
                return Some(HittableRecord {
                    t: temp,
                    p: point,
                    normal: (point - &self.center) / self.radius,
                    mat_ptr: self.mat_ptr
                });
            }
            temp = (-b + (b*b-a*c).sqrt()) / a;
            if temp < t_max && temp > t_min {
                let point = r.point_at_parameter(temp);
                return Some(HittableRecord {
                    t: temp,
                    p: point,
                    normal: (point - &self.center) / self.radius,
                    mat_ptr: self.mat_ptr
                });
            }
        }
        return None;
    }
}
