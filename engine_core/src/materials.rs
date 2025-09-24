use crate::*;
use glam::Vec3;
use rand::{rng, Rng};

#[derive(Copy, Clone, PartialEq)]
pub enum MaterialType {
    Lambertian { albedo: Vec3 },
    Metal { albedo: Vec3, fuzz: f32 },
    Dielectric { ref_indx: f32 },
}

impl Scatterable for MaterialType {
    fn scatter(&self, r_in: Ray, rec: HittableRecord) -> Option<ScatterRecord> {
        match &self {
            MaterialType::Lambertian { albedo } => {
                let target: Vec3 = rec.p + rec.normal + random_in_unit_sphere();
                Some(ScatterRecord {
                    attenuation: *albedo,
                    scattered: Ray::new(rec.p, target - rec.p),
                })
            }
            MaterialType::Metal { albedo, fuzz } => {
                let reflected: Vec3 = reflect(unit_vector(r_in.direction()), rec.normal);
                let s = Ray::new(rec.p, reflected + *fuzz * random_in_unit_sphere());
                let x = s.direction().dot(rec.normal);
                if x > 0f32 {
                    Some(ScatterRecord {
                        attenuation: *albedo,
                        scattered: s,
                    })
                } else {
                    None
                }
            }
            MaterialType::Dielectric { ref_indx } => {
                let normal_vector: f32 = r_in.direction().dot(rec.normal);
                let (outward_normal, ni_over_nt, cosine) = if normal_vector > 0f32 {
                    (
                        -rec.normal,
                        *ref_indx,
                        *ref_indx * normal_vector / vector_length(r_in.direction()),
                    )
                } else {
                    (
                        rec.normal,
                        1f32 / *ref_indx,
                        -normal_vector / vector_length(r_in.direction()),
                    )
                };

                let refraction_test = refract(r_in.direction(), outward_normal, ni_over_nt);
                let reflected = reflect(r_in.direction(), rec.normal);

                let (refracted, reflect_prob) = match refraction_test {
                    Some(v) => (v, schlick(cosine, *ref_indx)),
                    None => (Vec3::new(0f32, 0f32, 0f32), 1f32),
                };

                let mut rng = rng();
                let scatter = if rng.random::<f32>() < reflect_prob {
                    Ray::new(rec.p, reflected)
                } else {
                    Ray::new(rec.p, refracted)
                };

                Some(ScatterRecord {
                    attenuation: Vec3::new(1f32, 1f32, 1f32),
                    scattered: scatter,
                })
            }
        }
    }
}
