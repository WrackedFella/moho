use rand::{rng, Rng};
use glam::Vec3;
use crate::*;

#[derive(Copy, Clone)]
pub enum MaterialType {
    Lambertian {
        albedo: Vec3
    },
    Metal {
        albedo: Vec3,
        fuzz: f32
    },
    Dielectric {
        ref_indx: f32
    }
}

impl Scatterable for MaterialType {
    fn scatter(&self, mut r_in: Ray, rec: HittableRecord) -> Option<ScatterRecord> {
        match &self {
            MaterialType::Lambertian { albedo } => {
                let target: Vec3 = rec.p + rec.normal + random_in_unit_sphere();
                return Some(ScatterRecord {
                    attenuation: *albedo,
                    scattered: Ray::new(rec.p, target-rec.p)
                });
            },
            MaterialType::Metal { albedo, fuzz } => {
                let reflected: Vec3 = reflect(unit_vector(r_in.direction()), rec.normal);
                let mut s = Ray::new(rec.p, reflected+*fuzz*random_in_unit_sphere());
                let x = s.direction().dot(rec.normal);
                if x > 0f32 {
                    return Some(ScatterRecord { attenuation: *albedo, scattered: s });
                }
                return None;
            },
            MaterialType::Dielectric { ref_indx } => {
                let outward_normal: Vec3;
                let ni_over_nt: f32;
                let cosine: f32;
                let normalVector: f32 = r_in.direction().dot(rec.normal);
                if normalVector > 0f32 {
                    outward_normal = -rec.normal;
                    ni_over_nt = *ref_indx;
                    cosine = *ref_indx * normalVector / vector_length(r_in.direction());
                } else {
                    outward_normal = rec.normal;
                    ni_over_nt = 1f32 / *ref_indx;
                    cosine = -normalVector / vector_length(r_in.direction());
                }
                
                let refraction_test = refract(r_in.direction(), outward_normal, ni_over_nt);
                let reflect_prob: f32;
                let reflected = reflect(r_in.direction(), rec.normal);
                let scatter: Ray;
                let mut refracted = Vec3::new(0f32,0f32,0f32);

                match refraction_test {
                    Some(v) => { refracted = v; reflect_prob = schlick(cosine, *ref_indx); },
                    None => { reflect_prob = 1f32; }
                }
                let mut rng = rng();
                if rng.random::<f32>() < reflect_prob {
                    scatter = Ray::new(rec.p, reflected);
                } else {
                    scatter = Ray::new(rec.p, refracted);
                }
                return Some(ScatterRecord {
                    attenuation: Vec3::new(1f32,1f32,1f32),
                    scattered: scatter
                });
            }
        }
    }
}
