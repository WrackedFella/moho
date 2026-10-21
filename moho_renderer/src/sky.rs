//! The sky's zenith, horizon and ground colours by time of day.

/// The three sky colours the skybox and the ambient light both read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPalette {
    pub zenith: [f32; 3],
    pub horizon: [f32; 3],
    pub ground: [f32; 3],
}

/// The sky palette at `time_of_day` hours; hours outside `0..24` wrap into it.
pub fn sky_palette(time_of_day: f32) -> SkyPalette {
    let _ = time_of_day;
    todo!("port skybox.wgsl::get_sky_colors")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const TOLERANCE: f32 = 1e-5;

    fn assert_colour_eq(actual: [f32; 3], expected: [f32; 3], what: &str) {
        for channel in 0..3 {
            assert!(
                (actual[channel] - expected[channel]).abs() < TOLERANCE,
                "{what}: expected {expected:?}, got {actual:?}"
            );
        }
    }

    fn assert_palette_eq(actual: SkyPalette, expected: SkyPalette, what: &str) {
        assert_colour_eq(actual.zenith, expected.zenith, &format!("{what} zenith"));
        assert_colour_eq(actual.horizon, expected.horizon, &format!("{what} horizon"));
        assert_colour_eq(actual.ground, expected.ground, &format!("{what} ground"));
    }

    const NIGHT: SkyPalette = SkyPalette {
        zenith: [0.05, 0.05, 0.15],
        horizon: [0.1, 0.1, 0.2],
        ground: [0.05, 0.05, 0.1],
    };
    const DAY: SkyPalette = SkyPalette {
        zenith: [0.5, 0.85, 0.95],
        horizon: [0.85, 0.95, 0.98],
        ground: [0.6, 0.65, 0.7],
    };
    /// Halfway between night and dawn: `smoothstep(5, 7, 6) == 0.5`.
    const SIX_OCLOCK: SkyPalette = SkyPalette {
        zenith: [0.175, 0.275, 0.425],
        horizon: [0.55, 0.35, 0.3],
        ground: [0.125, 0.1, 0.15],
    };
    /// Halfway between day and dusk: `smoothstep(17, 21, 19) == 0.5`.
    const SEVEN_PM: SkyPalette = SkyPalette {
        zenith: [0.4, 0.625, 0.775],
        horizon: [0.925, 0.725, 0.64],
        ground: [0.4, 0.4, 0.425],
    };

    #[test]
    fn palette_matches_the_skybox_at_each_period() {
        let table = [
            (2.0, NIGHT),
            (6.0, SIX_OCLOCK),
            (12.0, DAY),
            (19.0, SEVEN_PM),
            (23.0, NIGHT),
        ];

        for (hour, expected) in table {
            assert_palette_eq(sky_palette(hour), expected, &format!("{hour} h"));
        }
    }

    #[test]
    fn palette_blends_by_smoothstep_not_linearly() {
        // smoothstep(5, 7, 5.5) = smoothstep(17, 21, 18) = 0.15625
        let t = 0.15625_f32;
        let dawn_horizon = [1.0_f32, 0.6, 0.4];
        let dusk_ground = [0.2_f32, 0.15, 0.15];

        let early_dawn = sky_palette(5.5);
        let early_dusk = sky_palette(18.0);

        let mix = |a: [f32; 3], b: [f32; 3]| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
        assert_colour_eq(
            early_dawn.horizon,
            mix(NIGHT.horizon, dawn_horizon),
            "5.5 h horizon",
        );
        assert_colour_eq(
            early_dusk.ground,
            mix(DAY.ground, dusk_ground),
            "18 h ground",
        );
    }

    #[test]
    fn palette_wraps_hours_into_a_day() {
        for (hour, same_as) in [
            (26.0, 2.0),
            (30.0, 6.0),
            (-1.0, 23.0),
            (-18.0, 6.0),
            (36.0, 12.0),
        ] {
            assert_palette_eq(
                sky_palette(hour),
                sky_palette(same_as),
                &format!("{hour} h wraps to {same_as} h"),
            );
        }
    }

    #[test]
    fn palette_is_continuous_across_the_24_to_0_wrap() {
        let before = sky_palette(24.0 - 1e-4);
        let after = sky_palette(0.0);

        assert_palette_eq(before, after, "midnight");
    }

    fn max_channel_distance(a: SkyPalette, b: SkyPalette) -> f32 {
        [
            (a.zenith, b.zenith),
            (a.horizon, b.horizon),
            (a.ground, b.ground),
        ]
        .into_iter()
        .flat_map(|(x, y)| (0..3).map(move |i| (x[i] - y[i]).abs()))
        .fold(0.0, f32::max)
    }

    proptest! {
        #[test]
        fn palette_is_continuous_across_period_boundaries(t in 0.0f32..24.0) {
            let step = 1e-3;

            let distance = max_channel_distance(sky_palette(t), sky_palette(t + step));

            // The steepest blend moves under 1 unit per hour, so a thousandth of an
            // hour moves a channel by far less than this.
            prop_assert!(distance < 5e-3, "palette jumps {distance} between {t} and {}", t + step);
        }
    }
}
