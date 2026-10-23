/// Exposure multiplier for a Brightness setting: clamped to `[0.5, 2.0]`, and NaN gives 1.0.
pub fn brightness_exposure(_brightness: f32) -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_exposure_clamps_and_rejects_nan() {
        assert_eq!(brightness_exposure(1.0), 1.0);
        assert_eq!(brightness_exposure(0.75), 0.75);
        assert_eq!(brightness_exposure(0.5), 0.5);
        assert_eq!(brightness_exposure(2.0), 2.0);
        assert_eq!(brightness_exposure(0.1), 0.5);
        assert_eq!(brightness_exposure(-3.0), 0.5);
        assert_eq!(brightness_exposure(7.0), 2.0);
        assert_eq!(brightness_exposure(f32::INFINITY), 2.0);
        assert_eq!(brightness_exposure(f32::NEG_INFINITY), 0.5);
        assert_eq!(brightness_exposure(f32::NAN), 1.0);
    }
}
