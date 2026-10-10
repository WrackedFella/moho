//! In-house 2D gradient (Perlin) noise.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Perlin {
    seed: u32,
}

impl Perlin {
    pub(crate) fn new(seed: u32) -> Self {
        Self { seed }
    }

    pub(crate) fn sample(&self, x: f64, z: f64) -> f64 {
        let _ = (self.seed, x, z);
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(min: f64, max: f64, step: f64) -> Vec<f64> {
        let n = ((max - min) / step).round() as i64;
        (0..=n).map(|i| min + i as f64 * step).collect()
    }

    #[test]
    fn same_seed_gives_identical_samples() {
        let a = Perlin::new(1234);
        let b = Perlin::new(1234);

        for x in grid(-10.0, 10.0, 0.37) {
            for z in grid(-10.0, 10.0, 0.41) {
                assert_eq!(a.sample(x, z), b.sample(x, z), "at ({x}, {z})");
            }
        }
    }

    #[test]
    fn different_seeds_differ_at_non_lattice_points() {
        let points: Vec<(f64, f64)> = (0..100)
            .map(|i| {
                let i = f64::from(i);
                (i * 0.37 - 17.3 + 0.11, i * 0.53 - 23.1 + 0.29)
            })
            .collect();

        for (sa, sb) in [
            (42, 43),
            (42, 42 + (1u32 << 16)),
            (0, 1 << 31),
            (0, u32::MAX),
        ] {
            let a = Perlin::new(sa);
            let b = Perlin::new(sb);

            let differing = points
                .iter()
                .filter(|&&(x, z)| a.sample(x, z) != b.sample(x, z))
                .count();

            assert!(
                differing >= 95,
                "seeds ({sa}, {sb}): only {differing}/100 points differ"
            );
        }
    }

    #[test]
    fn cell_centre_samples_approach_unit_bound() {
        for seed in [0, 7, 42, 99_999] {
            let noise = Perlin::new(seed);

            let mut max_abs = 0.0_f64;
            for ix in -15..15 {
                for iz in -15..15 {
                    let v = noise.sample(f64::from(ix) + 0.5, f64::from(iz) + 0.5);
                    max_abs = max_abs.max(v.abs());
                }
            }

            assert!(max_abs > 0.8, "seed {seed}: max |v| at centres = {max_abs}");
        }
    }

    #[test]
    fn second_derivative_vanishes_at_lattice_points() {
        // Quintic fade has zero second derivative at lattice points; cubic or linear fades give O(1) values.
        let noise = Perlin::new(42);
        let h = 1e-3;

        for ix in -20..=20 {
            for iz in -20..=20 {
                let (x, z) = (f64::from(ix), f64::from(iz));
                let centre = 2.0 * noise.sample(x, z);
                let dxx = (noise.sample(x + h, z) - centre + noise.sample(x - h, z)) / (h * h);
                let dzz = (noise.sample(x, z + h) - centre + noise.sample(x, z - h)) / (h * h);

                assert!(dxx.abs() < 0.5, "d2/dx2 at ({ix}, {iz}) = {dxx}");
                assert!(dzz.abs() < 0.5, "d2/dz2 at ({ix}, {iz}) = {dzz}");
            }
        }
    }

    #[test]
    fn integer_lattice_points_are_zero() {
        for seed in [0, 1, 42, 0xDEAD_BEEF, u32::MAX] {
            let noise = Perlin::new(seed);

            for ix in -20..=20 {
                for iz in -20..=20 {
                    let v = noise.sample(f64::from(ix), f64::from(iz));
                    assert_eq!(v, 0.0, "seed {seed} at ({ix}, {iz})");
                }
            }
        }
    }

    #[test]
    fn samples_stay_within_unit_range() {
        for seed in [0, 7, 42, 99_999] {
            let noise = Perlin::new(seed);

            for x in grid(-30.0, 30.0, 0.173) {
                for z in grid(-30.0, 30.0, 0.191) {
                    let v = noise.sample(x, z);
                    assert!((-1.0..=1.0).contains(&v), "seed {seed} ({x}, {z}) = {v}");
                }
            }
            for ix in -15..15 {
                for iz in -15..15 {
                    let (x, z) = (f64::from(ix) + 0.5, f64::from(iz) + 0.5);
                    let v = noise.sample(x, z);
                    assert!(
                        (-1.0..=1.0).contains(&v),
                        "seed {seed} centre ({x}, {z}) = {v}"
                    );
                }
            }
        }
    }

    #[test]
    fn nearby_points_give_nearby_values() {
        let noise = Perlin::new(42);
        let d = 1e-6;

        for x in grid(-6.0, 6.0, 0.25) {
            for z in grid(-6.0, 6.0, 0.25) {
                let base = noise.sample(x, z);
                let dx = (noise.sample(x + d, z) - base).abs();
                let dz = (noise.sample(x, z + d) - base).abs();
                assert!(dx <= 3e-6, "x step at ({x}, {z}): {dx}");
                assert!(dz <= 3e-6, "z step at ({x}, {z}): {dz}");
            }
        }
    }
}
