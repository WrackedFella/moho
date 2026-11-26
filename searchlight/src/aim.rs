//! Aiming: the magnification a held Aim gives the view, as a field of view and a look scale.

/// A magnification outside 1 to 12, or not a finite number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("magnification must be 1 to 12")]
pub struct OutOfRange;

/// How many times larger an object looks while aiming: a finite factor from 1 to 12.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Magnification(f32);

impl Magnification {
    /// The factor a fresh install aims with.
    pub const DEFAULT: Self = Self(10.0);

    /// The magnification for `factor`, which must be finite and 1 to 12.
    pub fn new(factor: f32) -> Result<Self, OutOfRange> {
        if (1.0..=12.0).contains(&factor) {
            Ok(Self(factor))
        } else {
            Err(OutOfRange)
        }
    }

    /// The factor.
    pub fn get(self) -> f32 {
        self.0
    }
}

/// The vertical field of view in radians that shows objects `m` times larger than `base_rad`.
///
/// An object's size on screen scales with `1 / tan(fov / 2)`, so the factor divides the tangent.
pub fn aimed_fov_y(base_rad: f32, m: Magnification) -> f32 {
    2.0 * ((base_rad / 2.0).tan() / m.0).atan()
}

/// What mouse look is multiplied by while aiming at `m`: the same on-screen turn per count at
/// the crosshair as unzoomed.
pub fn look_scale(m: Magnification) -> f32 {
    1.0 / m.0
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn magnification(factor: f32) -> Magnification {
        Magnification::new(factor).expect("a factor from 1 to 12")
    }

    #[test]
    fn ten_times_seventy_five_degrees_is_about_eight_point_eight() {
        let aimed = aimed_fov_y(75.0_f32.to_radians(), magnification(10.0)).to_degrees();

        assert!((aimed - 8.78).abs() < 0.01, "{aimed}");
    }

    #[test]
    fn five_times_seventy_five_degrees_is_about_seventeen_point_four() {
        let aimed = aimed_fov_y(75.0_f32.to_radians(), magnification(5.0)).to_degrees();

        assert!((aimed - 17.45).abs() < 0.01, "{aimed}");
    }

    proptest! {
        #[test]
        fn the_tangent_shrinks_by_the_factor(factor in 1.0_f32..=12.0, base_degrees in 50.0_f32..=90.0) {
            let base = base_degrees.to_radians();

            let aimed = aimed_fov_y(base, magnification(factor));

            let scaled = (aimed / 2.0).tan() * factor;
            let expected = (base / 2.0).tan();
            prop_assert!((scaled - expected).abs() <= expected * 1e-4, "{scaled} vs {expected}");
        }
    }

    #[test]
    fn magnification_one_leaves_the_view_and_look_unchanged() {
        let base = 75.0_f32.to_radians();

        let aimed = aimed_fov_y(base, magnification(1.0));
        let scale = look_scale(magnification(1.0));

        assert!((aimed - base).abs() < 1e-6, "{aimed} vs {base}");
        assert_eq!(scale, 1.0);
    }

    #[test]
    fn look_scale_is_one_over_the_factor() {
        assert!((look_scale(magnification(10.0)) - 0.1).abs() < 1e-7);
        assert!((look_scale(magnification(4.0)) - 0.25).abs() < 1e-7);
        assert!((look_scale(magnification(12.0)) - 1.0 / 12.0).abs() < 1e-7);
    }

    #[test]
    fn a_magnification_from_one_to_twelve_is_kept() {
        for factor in [1.0, 1.5, 5.0, 10.0, 12.0] {
            let kept = Magnification::new(factor).map(Magnification::get);

            assert_eq!(kept, Ok(factor), "{factor}");
        }
    }

    #[test]
    fn a_magnification_outside_one_to_twelve_or_not_finite_is_refused() {
        for factor in [
            0.0,
            0.99,
            -1.0,
            12.01,
            100.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(Magnification::new(factor), Err(OutOfRange), "{factor}");
        }
    }

    #[test]
    fn the_default_magnification_is_ten() {
        assert_eq!(Ok(Magnification::DEFAULT), Magnification::new(10.0));
    }
}
