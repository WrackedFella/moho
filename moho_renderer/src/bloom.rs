//! Bloom: quality levels and the mip-chain shape.

/// Total bloom energy added to the scene, split evenly across the chain's levels.
pub(crate) const BLOOM_STRENGTH: f32 = 0.5;

/// How much of the bloom chain runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BloomQuality {
    Off,
    Low,
    High,
}

impl BloomQuality {
    /// Maps the stored setting: 0 Off, 1 Low, 2 High; any other value gives High.
    pub(crate) fn from_u8(_value: u8) -> Self {
        todo!()
    }
}

/// Extents of each bloom mip for a `width` x `height` target, largest first.
pub(crate) fn mip_extents(_width: u32, _height: u32, _quality: BloomQuality) -> Vec<(u32, u32)> {
    todo!()
}

/// Strength of the bloom added in the composite; Low and High add the same total.
pub(crate) fn bloom_intensity(_quality: BloomQuality) -> f32 {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn high_starts_at_half_resolution_with_six_levels() {
        let extents = mip_extents(1920, 1080, BloomQuality::High);

        assert_eq!(
            extents,
            [(960, 540), (480, 270), (240, 135), (120, 67), (60, 33), (30, 16)]
        );
    }

    #[test]
    fn low_starts_at_quarter_resolution_with_five() {
        let extents = mip_extents(1920, 1080, BloomQuality::Low);

        assert_eq!(
            extents,
            [(480, 270), (240, 135), (120, 67), (60, 33), (30, 16)]
        );
    }

    #[test]
    fn odd_sizes_floor_each_halving() {
        let extents = mip_extents(101, 75, BloomQuality::High);

        assert_eq!(extents, [(50, 37), (25, 18), (12, 9), (6, 4), (3, 2), (1, 1)]);
    }

    #[test]
    fn tiny_targets_keep_every_level_at_one_pixel() {
        assert_eq!(mip_extents(1, 1, BloomQuality::High), [(1, 1); 6]);
        assert_eq!(mip_extents(2, 1, BloomQuality::Low), [(1, 1); 5]);
    }

    #[test]
    fn off_has_no_levels_and_zero_intensity() {
        let extents = mip_extents(1920, 1080, BloomQuality::Off);
        let intensity = bloom_intensity(BloomQuality::Off);

        assert!(extents.is_empty());
        assert_eq!(intensity, 0.0);
    }

    #[test]
    fn intensity_is_strength_over_level_count() {
        let high = bloom_intensity(BloomQuality::High);
        let low = bloom_intensity(BloomQuality::Low);

        assert_eq!(high, BLOOM_STRENGTH / 6.0);
        assert_eq!(low, BLOOM_STRENGTH / 5.0);
    }

    #[test]
    fn from_u8_maps_the_stored_setting() {
        assert_eq!(BloomQuality::from_u8(0), BloomQuality::Off);
        assert_eq!(BloomQuality::from_u8(1), BloomQuality::Low);
        assert_eq!(BloomQuality::from_u8(2), BloomQuality::High);
        assert_eq!(BloomQuality::from_u8(3), BloomQuality::High);
        assert_eq!(BloomQuality::from_u8(255), BloomQuality::High);
    }

    proptest! {
        #[test]
        fn levels_never_go_below_one_pixel(
            width in 1u32..=16_384,
            height in 1u32..=16_384,
            quality in prop_oneof![Just(BloomQuality::Low), Just(BloomQuality::High)],
        ) {
            let extents = mip_extents(width, height, quality);

            prop_assert!(!extents.is_empty());
            for (w, h) in &extents {
                prop_assert!(*w >= 1 && *h >= 1, "{extents:?}");
            }
            for pair in extents.windows(2) {
                prop_assert_eq!(pair[1], ((pair[0].0 / 2).max(1), (pair[0].1 / 2).max(1)));
            }
            let divisor = if quality == BloomQuality::High { 2 } else { 4 };
            prop_assert_eq!(extents[0], ((width / divisor).max(1), (height / divisor).max(1)));
        }
    }
}
