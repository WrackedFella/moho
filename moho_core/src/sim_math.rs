//! Deterministic math for simulation code: transcendentals computed by `libm`, never by the platform.

/// Sine and cosine of `angle` radians, as `(sin, cos)`.
pub fn sin_cos(angle: f32) -> (f32, f32) {
    (libm::sinf(angle), libm::cosf(angle))
}

/// Tangent of `angle` radians.
pub fn tan(angle: f32) -> f32 {
    libm::tanf(angle)
}

/// Angle in radians of the point `(x, y)` from the positive x axis, in `[-pi, pi]`.
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;
    use proptest::prelude::*;

    fn finite() -> impl Strategy<Value = f32> {
        any::<f32>().prop_filter("finite", |v| v.is_finite())
    }

    #[test]
    fn sin_cos_matches_libm_at_an_angle_where_the_platform_differs() {
        let angle = 0.887_671_8_f32;

        let (sin, cos) = sin_cos(angle);

        assert_eq!(sin.to_bits(), libm::sinf(angle).to_bits());
        assert_eq!(cos.to_bits(), libm::cosf(angle).to_bits());
    }

    #[test]
    fn tan_matches_libm_at_an_angle_where_the_platform_differs() {
        let angle = 0.300_000_25_f32;

        let result = tan(angle);

        assert_eq!(result.to_bits(), libm::tanf(angle).to_bits());
    }

    #[test]
    fn tan_keeps_the_sign_of_negative_zero() {
        let result = tan(-0.0_f32);

        assert_eq!(result.to_bits(), libm::tanf(-0.0_f32).to_bits());
        assert_eq!(result.to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn tan_at_a_right_angle_is_libm_and_huge_but_finite() {
        let angle = std::f32::consts::FRAC_PI_2;

        let result = tan(angle);

        assert_eq!(result.to_bits(), libm::tanf(angle).to_bits());
        assert!(result.is_finite() && result.abs() > 1e6, "got {result}");
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_000))]

        #[test]
        fn tan_equals_libm_bit_for_bit(angle in finite()) {
            prop_assert_eq!(tan(angle).to_bits(), libm::tanf(angle).to_bits());
        }

        #[test]
        fn sin_cos_equals_libm_bit_for_bit(angle in finite()) {
            let (sin, cos) = sin_cos(angle);

            prop_assert_eq!(sin.to_bits(), libm::sinf(angle).to_bits());
            prop_assert_eq!(cos.to_bits(), libm::cosf(angle).to_bits());
        }

        #[test]
        fn atan2_equals_libm_bit_for_bit(y in finite(), x in finite()) {
            prop_assert_eq!(atan2(y, x).to_bits(), libm::atan2f(y, x).to_bits());
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2_000))]

        #[test]
        fn glam_rotation_uses_libm_sin_and_cos(angle in -8.0f32..8.0) {
            let rotation = Quat::from_rotation_y(angle);

            let half = angle * 0.5;
            prop_assert_eq!(rotation.y.to_bits(), libm::sinf(half).to_bits());
            prop_assert_eq!(rotation.w.to_bits(), libm::cosf(half).to_bits());
        }
    }
}
