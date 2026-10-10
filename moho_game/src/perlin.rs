//! In-house 2D gradient (Perlin) noise.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Perlin {
    seed: u32,
}

impl Perlin {
    pub(crate) fn new(seed: u32) -> Self {
        Self { seed }
    }

    /// Gradient noise at `(x, z)`: zero at integer lattice points, within `[-1, 1]`.
    pub(crate) fn sample(&self, x: f64, z: f64) -> f64 {
        let x0 = x.floor();
        let z0 = z.floor();
        let (fx, fz) = (x - x0, z - z0);
        let (ix, iz) = (x0 as i32, z0 as i32);

        let corner = |dx: i32, dz: i32| {
            let (gx, gz) = self.gradient(ix.wrapping_add(dx), iz.wrapping_add(dz));
            gx * (fx - f64::from(dx)) + gz * (fz - f64::from(dz))
        };
        let (u, v) = (fade(fx), fade(fz));

        let bottom = lerp(corner(0, 0), corner(1, 0), u);
        let top = lerp(corner(0, 1), corner(1, 1), u);
        lerp(bottom, top, v)
    }

    /// Gradient of length sqrt(2) chosen by hashing the lattice corner and seed.
    fn gradient(&self, ix: i32, iz: i32) -> (f64, f64) {
        let mut h = u64::from(self.seed) | (u64::from(ix as u32) << 32);
        h = h.wrapping_add((u64::from(iz as u32)).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        h = h.wrapping_add(0x9e37_79b9_7f4a_7c15);
        h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        h ^= h >> 31;
        GRADIENTS[(h >> 61) as usize]
    }
}

const R: f64 = std::f64::consts::SQRT_2;

/// Eight directions of equal length sqrt(2), so the output bound is exactly `[-1, 1]`.
const GRADIENTS: [(f64, f64); 8] = [
    (1.0, 1.0),
    (-1.0, 1.0),
    (1.0, -1.0),
    (-1.0, -1.0),
    (R, 0.0),
    (-R, 0.0),
    (0.0, R),
    (0.0, -R),
];

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
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

    #[test]
    fn bound_holds_near_extreme_cell_centres() {
        let offsets: Vec<f64> = (-20..=20)
            .flat_map(|k| [f64::from(k) * 1e-7, f64::from(k) * 1e-3])
            .collect();

        let mut extreme_cells = 0;
        for seed in 0..64 {
            let noise = Perlin::new(seed);

            for i in -16..16 {
                for j in -16..16 {
                    let (cx, cz) = (f64::from(i) + 0.5, f64::from(j) + 0.5);
                    if noise.sample(cx, cz).abs() < 0.999_999 {
                        continue;
                    }
                    extreme_cells += 1;

                    for &a in &offsets {
                        for &b in &offsets {
                            let v = noise.sample(cx + a, cz + b);
                            assert!(
                                v.abs() <= 1.0,
                                "seed {seed} cell ({i}, {j}) offset ({a}, {b}) = {v}"
                            );
                        }
                    }
                }
            }
        }

        assert!(extreme_cells > 0, "no extreme cell centres found");
    }

    #[test]
    fn continuous_when_approaching_lattice_from_below() {
        let noise = Perlin::new(42);

        for ix in -5..=5 {
            let lattice = f64::from(ix);
            for off in [0.3, -0.7] {
                for eps in [1e-9, 1e-12] {
                    let limit = 3.0 * eps + 1e-15;

                    let dx = (noise.sample(lattice, off) - noise.sample(lattice - eps, off)).abs();
                    let dz = (noise.sample(off, lattice) - noise.sample(off, lattice - eps)).abs();

                    assert!(dx <= limit, "x at {ix}, z {off}, eps {eps}: {dx}");
                    assert!(dz <= limit, "z at {ix}, x {off}, eps {eps}: {dz}");
                }
            }
        }
    }

    #[test]
    fn tiny_and_signed_zero_offsets_stay_at_origin_value() {
        let noise = Perlin::new(42);

        for (x, z) in [(-1e-300, 0.0), (-0.0, 0.0), (0.0, 0.0), (-0.0, -0.0)] {
            let v = noise.sample(x, z);

            assert!(v.abs() <= 1e-12, "({x:?}, {z:?}) = {v}");
        }
    }

    #[test]
    fn golden_samples_are_pinned() {
        let noise = Perlin::new(42);
        let pinned = [
            ((0.5, 0.5), -0.5),
            ((1.25, -3.75), 0.413_207_605_066_412),
            ((-7.3, 2.9), 0.472_080_000_000_000_5),
            ((10.1, 10.6), 0.426_318_174_765_844_1),
            ((-0.4, -12.2), -0.478_796_143_677_034_2),
            ((123.456, -654.321), 0.232_055_541_287_439_83),
        ];

        for ((x, z), expected) in pinned {
            let v = noise.sample(x, z);
            assert!(
                (v - expected).abs() < 1e-12,
                "({x}, {z}) = {v}, pinned {expected}"
            );
        }
    }
}
