//! The engine's own player settings as ready-made menu rows: the ten values a game places on a
//! Settings screen, read from and written to the prefs they come from.

use moho_core::prefs::{Prefs, WindowMode};

/// The ten rows' values. Volumes are on the prefs scale, 0 to 10; qualities are 0 (Off) to 4
/// (Ultra).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineSettings {
    pub mouse_sensitivity: f32,
    pub mouse_smoothing: bool,
    pub invert_mouse_y: bool,
    pub sound_effect_volume: f32,
    pub music_volume: f32,
    pub ui_volume: f32,
    pub shadow_quality: u32,
    pub ssao_quality: u32,
    pub window_mode: WindowMode,
    pub resolution: (u32, u32),
}

/// One row of the engine's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineSetting {
    MouseSensitivity,
    MouseSmoothing,
    InvertMouseY,
    SoundEffectVolume,
    MusicVolume,
    UiVolume,
    ShadowQuality,
    SsaoQuality,
    WindowMode,
    Resolution,
}

/// How a row is drawn and driven.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Slider { min: f32, max: f32, step: f32 },
    Toggle,
    Choice,
}

impl EngineSetting {
    /// Every row, in the order a Settings screen lists them.
    pub const ALL: [EngineSetting; 10] = [
        EngineSetting::MouseSensitivity,
        EngineSetting::MouseSmoothing,
        EngineSetting::InvertMouseY,
        EngineSetting::SoundEffectVolume,
        EngineSetting::MusicVolume,
        EngineSetting::UiVolume,
        EngineSetting::ShadowQuality,
        EngineSetting::SsaoQuality,
        EngineSetting::WindowMode,
        EngineSetting::Resolution,
    ];

    /// The text drawn at the row's left.
    pub fn label(self) -> &'static str {
        todo!("row labels")
    }

    pub fn control(self) -> Control {
        todo!("row controls")
    }
}

impl EngineSettings {
    /// The rows' values as `prefs` holds them.
    pub fn from_prefs(_prefs: &Prefs) -> Self {
        todo!("read the ten rows from prefs")
    }

    /// Writes the ten rows into `prefs` and nothing else.
    pub fn write_to(&self, _prefs: &mut Prefs) {
        todo!("write the ten rows to prefs")
    }

    /// The labels of `setting`'s choices, in the order Enter cycles them; empty for a slider or
    /// a toggle.
    pub fn choices(&self, _setting: EngineSetting) -> Vec<String> {
        todo!("choice labels")
    }

    /// The text drawn at the row's right for the value held here.
    pub fn value_text(&self, _setting: EngineSetting) -> String {
        todo!("value text")
    }
}

#[cfg(test)]
mod tests {
    use moho_core::prefs::RESOLUTION_PRESETS;
    use proptest::prelude::*;

    use super::*;

    fn distinctive_prefs() -> Prefs {
        let mut prefs = Prefs::default();
        *prefs.mouse_sensitivity_mut() = 2.5;
        *prefs.mouse_smoothing_mut() = true;
        *prefs.invert_mouse_y_mut() = true;
        *prefs.sound_effect_volume_mut() = 1.5;
        *prefs.music_volume_mut() = 2.5;
        *prefs.ui_volume_mut() = 3.5;
        prefs.set_shadow_quality(1);
        prefs.set_ssao_quality(2);
        prefs.set_window_mode(WindowMode::Borderless);
        prefs.set_window_resolution(2560, 1440);
        prefs
    }

    fn settings() -> EngineSettings {
        EngineSettings {
            mouse_sensitivity: 4.5,
            mouse_smoothing: false,
            invert_mouse_y: false,
            sound_effect_volume: 9.5,
            music_volume: 0.5,
            ui_volume: 10.0,
            shadow_quality: 4,
            ssao_quality: 0,
            window_mode: WindowMode::Windowed,
            resolution: (1280, 720),
        }
    }

    #[test]
    fn from_prefs_reads_every_row() {
        let mut prefs = distinctive_prefs();
        *prefs.voice_volume_mut() = 7.5;
        prefs.set_brightness(1.5);

        let settings = EngineSettings::from_prefs(&prefs);

        assert_eq!(
            settings,
            EngineSettings {
                mouse_sensitivity: 2.5,
                mouse_smoothing: true,
                invert_mouse_y: true,
                sound_effect_volume: 1.5,
                music_volume: 2.5,
                ui_volume: 3.5,
                shadow_quality: 1,
                ssao_quality: 2,
                window_mode: WindowMode::Borderless,
                resolution: (2560, 1440),
            }
        );
    }

    #[test]
    fn write_to_keeps_every_other_prefs_value() {
        let mut prefs = distinctive_prefs();
        *prefs.voice_volume_mut() = 7.5;
        *prefs.mouse_acceleration_mut() = true;
        *prefs.mouse_acceleration_strength_mut() = 0.2;
        prefs.set_brightness(1.5);
        prefs.set_bloom(1);
        prefs.set_vsync(false);
        prefs.set_fps_max(144);
        prefs.set_bindings([("jump".to_owned(), "F".to_owned())].into());
        prefs.set_action_modes([("sprint".to_owned(), "Toggle".to_owned())].into());
        prefs.set_content_mods(vec!["heavy".to_owned()]);
        prefs.set_debug_value("god_mode", "1").unwrap();
        let mut expected = prefs.clone();
        *expected.mouse_sensitivity_mut() = 4.5;
        *expected.mouse_smoothing_mut() = false;
        *expected.invert_mouse_y_mut() = false;
        *expected.sound_effect_volume_mut() = 9.5;
        *expected.music_volume_mut() = 0.5;
        *expected.ui_volume_mut() = 10.0;
        expected.set_shadow_quality(4);
        expected.set_ssao_quality(0);
        expected.set_window_mode(WindowMode::Windowed);
        expected.set_window_resolution(1280, 720);

        settings().write_to(&mut prefs);

        assert_eq!(prefs, expected);
    }

    #[test]
    fn an_unlisted_resolution_is_the_first_choice_and_fullscreen_shows_as_fullscreen() {
        let preset_labels: Vec<String> = RESOLUTION_PRESETS
            .iter()
            .map(|(label, _)| (*label).to_owned())
            .collect();
        let listed = settings();
        let unlisted = EngineSettings {
            resolution: (1366, 768),
            ..settings()
        };
        let with_mode = |window_mode| EngineSettings {
            window_mode,
            ..settings()
        };

        let unlisted_choices = unlisted.choices(EngineSetting::Resolution);
        let mut expected_unlisted = vec!["1366×768".to_owned()];
        expected_unlisted.extend(preset_labels.clone());

        assert_eq!(listed.choices(EngineSetting::Resolution), preset_labels);
        assert_eq!(unlisted_choices, expected_unlisted);
        assert_eq!(unlisted.value_text(EngineSetting::Resolution), "1366×768");
        assert_eq!(
            listed.value_text(EngineSetting::Resolution),
            RESOLUTION_PRESETS[0].0
        );
        for mode in [WindowMode::Fullscreen, WindowMode::Borderless] {
            let shown = with_mode(mode);

            assert_eq!(
                shown.choices(EngineSetting::WindowMode),
                ["Windowed", "Fullscreen"]
            );
            assert_eq!(shown.value_text(EngineSetting::WindowMode), "Fullscreen");
        }
        assert_eq!(
            with_mode(WindowMode::Windowed).value_text(EngineSetting::WindowMode),
            "Windowed"
        );
    }

    #[test]
    fn a_fullscreen_saved_in_prefs_stays_fullscreen_through_the_settings() {
        let mut prefs = Prefs::default();
        prefs.set_window_mode(WindowMode::Fullscreen);

        EngineSettings::from_prefs(&prefs).write_to(&mut prefs);

        assert_eq!(prefs.window_mode(), WindowMode::Fullscreen);
    }

    #[test]
    fn a_slider_and_a_quality_show_their_value_as_text() {
        let settings = EngineSettings {
            mouse_sensitivity: 2.5,
            shadow_quality: 2,
            ..settings()
        };

        assert_eq!(settings.value_text(EngineSetting::MouseSensitivity), "2.5");
        assert_eq!(settings.value_text(EngineSetting::ShadowQuality), "Medium");
        assert_eq!(
            settings.choices(EngineSetting::SsaoQuality),
            ["Off", "Low", "Medium", "High", "Ultra"]
        );
        assert!(
            settings
                .choices(EngineSetting::MouseSensitivity)
                .is_empty()
        );
    }

    #[test]
    fn labels_name_each_row_in_listing_order() {
        let labels = EngineSetting::ALL.map(EngineSetting::label);

        assert_eq!(
            labels,
            [
                "Mouse sensitivity",
                "Mouse smoothing",
                "Invert mouse Y",
                "Sound-effect volume",
                "Music volume",
                "UI volume",
                "Shadow quality",
                "Ambient-occlusion quality",
                "Window mode",
                "Resolution",
            ]
        );
    }

    #[test]
    fn each_row_has_the_control_the_design_lists() {
        let volume = Control::Slider {
            min: 0.0,
            max: 10.0,
            step: 0.5,
        };

        let controls = EngineSetting::ALL.map(EngineSetting::control);

        assert_eq!(
            controls,
            [
                Control::Slider {
                    min: 0.1,
                    max: 10.0,
                    step: 0.1
                },
                Control::Toggle,
                Control::Toggle,
                volume,
                volume,
                volume,
                Control::Choice,
                Control::Choice,
                Control::Choice,
                Control::Choice,
            ]
        );
    }

    fn any_mode() -> impl Strategy<Value = WindowMode> {
        prop_oneof![
            Just(WindowMode::Windowed),
            Just(WindowMode::Fullscreen),
            Just(WindowMode::Borderless),
        ]
    }

    /// Values on the grids the rows step through, so the f32s are the ones prefs stores.
    fn any_settings() -> impl Strategy<Value = EngineSettings> {
        (
            (1u32..=100).prop_map(|n| n as f32 / 10.0),
            any::<bool>(),
            any::<bool>(),
            (0u32..=20).prop_map(|n| n as f32 / 2.0),
            (0u32..=20).prop_map(|n| n as f32 / 2.0),
            (0u32..=20).prop_map(|n| n as f32 / 2.0),
            0u32..=4,
            0u32..=4,
            any_mode(),
            (1u32..=8192, 1u32..=8192),
        )
            .prop_map(|(sens, smooth, invert, sfx, music, ui, shadow, ssao, mode, size)| {
                EngineSettings {
                    mouse_sensitivity: sens,
                    mouse_smoothing: smooth,
                    invert_mouse_y: invert,
                    sound_effect_volume: sfx,
                    music_volume: music,
                    ui_volume: ui,
                    shadow_quality: shadow,
                    ssao_quality: ssao,
                    window_mode: mode,
                    resolution: size,
                }
            })
    }

    proptest! {
        #[test]
        fn write_to_then_from_prefs_round_trips(
            earlier in any_settings(),
            later in any_settings(),
        ) {
            let mut prefs = Prefs::default();
            earlier.write_to(&mut prefs);

            later.write_to(&mut prefs);

            prop_assert_eq!(EngineSettings::from_prefs(&prefs), later);
        }
    }
}
