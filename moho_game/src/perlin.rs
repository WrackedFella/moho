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
        let a = Perlin::new(42);
        let b = Perlin::new(43);

        let points: Vec<(f64, f64)> = (0..100)
            .map(|i| {
                let i = f64::from(i);
                (i * 0.37 - 17.3 + 0.11, i * 0.53 - 23.1 + 0.29)
            })
            .collect();
        let differing = points
            .iter()
            .filter(|&&(x, z)| a.sample(x, z) != b.sample(x, z))
            .count();

        assert!(differing >= 95, "only {differing}/100 points differ");
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
