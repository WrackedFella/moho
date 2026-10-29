//! Shadow quality tiers: cascade map size and filter radius per level, with no GPU types.

/// A shadow quality level, chosen by `r_shadow_quality` 0-4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShadowTier {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl ShadowTier {
    /// The tier for a 0-4 level; anything above 4 is Medium.
    pub(crate) fn from_level(level: u8) -> Self {
        match level {
            0 => Self::Off,
            1 => Self::Low,
            3 => Self::High,
            4 => Self::Ultra,
            _ => Self::Medium,
        }
    }

    /// Texels per side of every cascade map, clamped to `device_limit` and at least 1.
    pub(crate) fn map_size(self, device_limit: u32) -> u32 {
        let wanted = match self {
            Self::Off => 1,
            Self::Low => 1024,
            Self::Medium => 2048,
            Self::High => 4096,
            Self::Ultra => 8192,
        };
        wanted.min(device_limit).max(1)
    }

    /// Filter radius in texels of the cascade being read.
    pub(crate) fn pcf_radius(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low | Self::Medium => 1,
            Self::High => 2,
            Self::Ultra => 3,
        }
    }

    /// Whether this tier renders shadows at all.
    pub(crate) fn casts_shadows(self) -> bool {
        self != Self::Off
    }
}

/// Whether the shadow array must be recreated when the clamped map size moves from `old` to `new`.
pub(crate) fn needs_resize(old: u32, new: u32) -> bool {
    old != new
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: [(u8, ShadowTier, u32, u32); 5] = [
        (0, ShadowTier::Off, 1, 0),
        (1, ShadowTier::Low, 1024, 1),
        (2, ShadowTier::Medium, 2048, 1),
        (3, ShadowTier::High, 4096, 2),
        (4, ShadowTier::Ultra, 8192, 3),
    ];

    #[test]
    fn levels_map_to_size_and_filter_radius() {
        for (level, tier, size, radius) in LEVELS {
            let from_level = ShadowTier::from_level(level);

            assert_eq!(from_level, tier, "level {level}");
            assert_eq!(from_level.map_size(u32::MAX), size, "level {level} size");
            assert_eq!(from_level.pcf_radius(), radius, "level {level} radius");
        }
    }

    #[test]
    fn only_off_casts_no_shadows() {
        for (level, tier, _, _) in LEVELS {
            assert_eq!(tier.casts_shadows(), level != 0, "level {level}");
        }
    }

    #[test]
    fn map_size_is_clamped_to_the_device_limit() {
        assert_eq!(ShadowTier::Ultra.map_size(4096), 4096);
        assert_eq!(ShadowTier::High.map_size(4096), 4096);
        assert_eq!(ShadowTier::Low.map_size(4096), 1024);
        assert_eq!(ShadowTier::Ultra.map_size(2048), 2048);
        assert_eq!(ShadowTier::Low.map_size(0), 1, "floored at 1");
        assert_eq!(ShadowTier::Off.map_size(0), 1, "floored at 1");
    }

    #[test]
    fn resize_is_needed_only_when_the_clamped_size_changes() {
        let size = |tier: ShadowTier, limit| tier.map_size(limit);

        assert!(!needs_resize(
            size(ShadowTier::Low, u32::MAX),
            size(ShadowTier::Low, u32::MAX)
        ));
        assert!(needs_resize(
            size(ShadowTier::Low, u32::MAX),
            size(ShadowTier::Medium, u32::MAX)
        ));
        assert!(
            !needs_resize(size(ShadowTier::High, 4096), size(ShadowTier::Ultra, 4096)),
            "High and Ultra share a 4096 map on a 4096 device"
        );
        assert!(needs_resize(
            size(ShadowTier::Off, u32::MAX),
            size(ShadowTier::Low, u32::MAX)
        ));
    }

    #[test]
    fn out_of_range_level_falls_back_to_medium() {
        for level in [5, 6, 100, u8::MAX] {
            assert_eq!(
                ShadowTier::from_level(level),
                ShadowTier::Medium,
                "level {level}"
            );
        }
    }
}
