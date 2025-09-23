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

// impl Hittable for Sphere {
//     fn hit<'b>(&'b self, mut r: Ray, t_min: f32, t_max: f32,  rec: &mut HittableRecord) -> bool {
//         let oc: Vec3 = r.origin() - &self.center;
//         let a: f32 = glam::dot(r.direction(), r.direction());
//         let b: f32 = glam::dot(oc, r.direction());
//         let c: f32 = glam::dot(oc, oc) - self.radius*self.radius;
//         let discriminant: f32 = b*b - a*c;
//         if discriminant > 0f32 {
//             let mut temp = (-b - (b*b-a*c).sqrt()) / a;
//             if temp < t_max && temp > t_min {
//                 rec.t = temp;
//                 rec.p = r.point_at_parameter(rec.t);
//                 rec.normal = (rec.p - &self.center) / self.radius;
//                 // rec.mat_ptr = &self.mat_ptr.Copy();
//                 return true;
//             }
//             temp = (-b + (b*b-a*c).sqrt()) / a;
//             if temp < t_max && temp > t_min {
//                 rec.t = temp;
//                 rec.p = r.point_at_parameter(rec.t);
//                 rec.normal = (rec.p - &self.center) / self.radius;
//                 // rec.mat_ptr = self.mat_ptr;
//                 return true;
//             }
//         }
//         return false;
//     }
// }
