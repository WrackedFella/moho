//! The engine's own player settings as ready-made menu rows: the values a game places on a
//! Settings screen, read from and written to the prefs they come from.

use std::borrow::Cow;

use moho_core::prefs::{Prefs, RESOLUTION_PRESETS, WindowMode};

/// The rows' values. Volumes are on the prefs scale, 0 to 10; qualities are 0 (Off) to 4
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
    pub effects_quality: u32,
    pub window_mode: WindowMode,
    pub resolution: (u32, u32),
}

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
    EffectsQuality,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Slider { min: f32, max: f32, step: f32 },
    Toggle,
    Choice,
}

/// A game's own slider row; `key` names its value among the screen's game values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameSlider {
    pub key: String,
    pub label: String,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    /// Shown after the value, after a space, when non-empty ("%").
    pub unit: String,
    /// The value the row starts from.
    pub value: i32,
}

impl GameSlider {
    /// `value` moved one step towards `direction`, then snapped.
    pub(crate) fn stepped(&self, value: i32, direction: i8) -> i32 {
        let (_, _, step) = self.range();
        self.snap(f64::from(value) + f64::from(direction) * step)
    }

    /// The value `fraction` (0 to 1) of the way along the range, snapped.
    pub(crate) fn at_fraction(&self, fraction: f32) -> i32 {
        let (min, max, _) = self.range();
        self.snap(min + f64::from(fraction) * (max - min))
    }

    /// `value` on the row's step grid from `min`, clamped to the range.
    fn snap(&self, value: f64) -> i32 {
        let (min, max, step) = self.range();
        let snapped = min + ((value - min) / step).round() * step;
        snapped.clamp(min, max) as i32
    }

    /// `(min, max, step)` as used. Bad row data (`max` below `min`, a step under 1) is read as
    /// the nearest sane range rather than panicking.
    fn range(&self) -> (f64, f64, f64) {
        (
            f64::from(self.min),
            f64::from(self.max.max(self.min)),
            f64::from(self.step.max(1)),
        )
    }

    pub(crate) fn value_text(&self, value: i32) -> String {
        if self.unit.is_empty() {
            value.to_string()
        } else {
            format!("{value} {}", self.unit)
        }
    }
}

impl EngineSetting {
    /// Every row, in the order a Settings screen lists them.
    pub const ALL: [EngineSetting; 11] = [
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
        EngineSetting::EffectsQuality,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EngineSetting::MouseSensitivity => "Mouse sensitivity",
            EngineSetting::MouseSmoothing => "Mouse smoothing",
            EngineSetting::InvertMouseY => "Invert mouse Y",
            EngineSetting::SoundEffectVolume => "Sound-effect volume",
            EngineSetting::MusicVolume => "Music volume",
            EngineSetting::UiVolume => "UI volume",
            EngineSetting::ShadowQuality => "Shadow quality",
            EngineSetting::SsaoQuality => "Ambient-occlusion quality",
            EngineSetting::WindowMode => "Window mode",
            EngineSetting::Resolution => "Resolution",
            EngineSetting::EffectsQuality => "Effects quality",
        }
    }

    pub fn control(self) -> Control {
        const VOLUME: Control = Control::Slider {
            min: 0.0,
            max: 10.0,
            step: 0.5,
        };
        match self {
            EngineSetting::MouseSensitivity => Control::Slider {
                min: 0.1,
                max: 10.0,
                step: 0.1,
            },
            EngineSetting::MouseSmoothing | EngineSetting::InvertMouseY => Control::Toggle,
            EngineSetting::SoundEffectVolume
            | EngineSetting::MusicVolume
            | EngineSetting::UiVolume => VOLUME,
            EngineSetting::ShadowQuality
            | EngineSetting::SsaoQuality
            | EngineSetting::EffectsQuality
            | EngineSetting::WindowMode
            | EngineSetting::Resolution => Control::Choice,
        }
    }
}

pub(crate) const QUALITY_NAMES: [&str; 5] = ["Off", "Low", "Medium", "High", "Ultra"];
const WINDOW_MODES: [&str; 2] = ["Windowed", "Fullscreen"];

impl EngineSettings {
    /// The rows' values as `prefs` holds them.
    pub fn from_prefs(prefs: &Prefs) -> Self {
        Self {
            mouse_sensitivity: prefs.mouse_sensitivity(),
            mouse_smoothing: prefs.mouse_smoothing(),
            invert_mouse_y: prefs.invert_mouse_y(),
            sound_effect_volume: prefs.sound_effect_volume(),
            music_volume: prefs.music_volume(),
            ui_volume: prefs.ui_volume(),
            shadow_quality: prefs.shadow_quality(),
            ssao_quality: prefs.ssao_quality(),
            effects_quality: prefs.effects_quality(),
            window_mode: prefs.window_mode(),
            resolution: prefs.window_resolution(),
        }
    }

    /// Writes the rows into `prefs` and nothing else.
    pub fn write_to(&self, prefs: &mut Prefs) {
        *prefs.mouse_sensitivity_mut() = self.mouse_sensitivity;
        *prefs.mouse_smoothing_mut() = self.mouse_smoothing;
        *prefs.invert_mouse_y_mut() = self.invert_mouse_y;
        *prefs.sound_effect_volume_mut() = self.sound_effect_volume;
        *prefs.music_volume_mut() = self.music_volume;
        *prefs.ui_volume_mut() = self.ui_volume;
        prefs.set_shadow_quality(self.shadow_quality);
        prefs.set_ssao_quality(self.ssao_quality);
        prefs.set_effects_quality(self.effects_quality);
        prefs.set_window_mode(self.window_mode);
        prefs.set_window_resolution(self.resolution.0, self.resolution.1);
    }

    /// The labels of `setting`'s choices, in the order Enter cycles them; empty for a slider or
    /// a toggle.
    pub fn choices(&self, setting: EngineSetting) -> Vec<String> {
        (0..self.choice_count(setting))
            .filter_map(|index| self.choice_label(setting, index))
            .map(Cow::into_owned)
            .collect()
    }

    /// How many choices `setting` has; 0 for a slider or a toggle.
    pub(crate) fn choice_count(&self, setting: EngineSetting) -> usize {
        match setting {
            EngineSetting::ShadowQuality
            | EngineSetting::SsaoQuality
            | EngineSetting::EffectsQuality => QUALITY_NAMES.len(),
            EngineSetting::WindowMode => WINDOW_MODES.len(),
            EngineSetting::Resolution => {
                RESOLUTION_PRESETS.len() + usize::from(!self.resolution_is_listed())
            }
            _ => 0,
        }
    }

    /// The label of choice `index` of `setting`, if it has one.
    fn choice_label(&self, setting: EngineSetting, index: usize) -> Option<Cow<'static, str>> {
        match setting {
            EngineSetting::ShadowQuality
            | EngineSetting::SsaoQuality
            | EngineSetting::EffectsQuality => {
                QUALITY_NAMES.get(index).map(|label| Cow::Borrowed(*label))
            }
            EngineSetting::WindowMode => WINDOW_MODES.get(index).map(|label| Cow::Borrowed(*label)),
            EngineSetting::Resolution => {
                if self.resolution_is_listed() {
                    RESOLUTION_PRESETS
                        .get(index)
                        .map(|(label, _)| Cow::Borrowed(*label))
                } else if index == 0 {
                    Some(Cow::Owned(resolution_label(self.resolution)))
                } else {
                    RESOLUTION_PRESETS
                        .get(index - 1)
                        .map(|(label, _)| Cow::Borrowed(*label))
                }
            }
            _ => None,
        }
    }

    pub fn value_text(&self, setting: EngineSetting) -> String {
        match setting {
            EngineSetting::MouseSensitivity => format!("{:.1}", self.mouse_sensitivity),
            EngineSetting::SoundEffectVolume => format!("{:.1}", self.sound_effect_volume),
            EngineSetting::MusicVolume => format!("{:.1}", self.music_volume),
            EngineSetting::UiVolume => format!("{:.1}", self.ui_volume),
            EngineSetting::MouseSmoothing => on_off(self.mouse_smoothing).to_owned(),
            EngineSetting::InvertMouseY => on_off(self.invert_mouse_y).to_owned(),
            EngineSetting::ShadowQuality
            | EngineSetting::SsaoQuality
            | EngineSetting::EffectsQuality
            | EngineSetting::WindowMode
            | EngineSetting::Resolution => self
                .choice_label(setting, self.choice_index(setting))
                .expect("choice_index is always within the row's choices")
                .into_owned(),
        }
    }

    fn resolution_is_listed(&self) -> bool {
        RESOLUTION_PRESETS
            .iter()
            .any(|(_, size)| *size == self.resolution)
    }

    /// A slider row's value; 0 for any other row.
    pub(crate) fn slider_value(&self, setting: EngineSetting) -> f32 {
        match setting {
            EngineSetting::MouseSensitivity => self.mouse_sensitivity,
            EngineSetting::SoundEffectVolume => self.sound_effect_volume,
            EngineSetting::MusicVolume => self.music_volume,
            EngineSetting::UiVolume => self.ui_volume,
            _ => 0.0,
        }
    }

    /// Sets a slider row to `value` clamped to its range and snapped to its step; other rows
    /// are left alone.
    pub(crate) fn set_slider(&mut self, setting: EngineSetting, value: f32) {
        let Control::Slider { min, max, step } = setting.control() else {
            return;
        };
        let snapped = (min + ((value - min) / step).round() * step).clamp(min, max);
        match setting {
            EngineSetting::MouseSensitivity => self.mouse_sensitivity = snapped,
            EngineSetting::SoundEffectVolume => self.sound_effect_volume = snapped,
            EngineSetting::MusicVolume => self.music_volume = snapped,
            EngineSetting::UiVolume => self.ui_volume = snapped,
            _ => {}
        }
    }

    /// Flips a toggle row; other rows are left alone.
    pub(crate) fn flip(&mut self, setting: EngineSetting) {
        match setting {
            EngineSetting::MouseSmoothing => self.mouse_smoothing = !self.mouse_smoothing,
            EngineSetting::InvertMouseY => self.invert_mouse_y = !self.invert_mouse_y,
            _ => {}
        }
    }

    /// The position of the held value among a choice row's [`choices`](Self::choices).
    pub(crate) fn choice_index(&self, setting: EngineSetting) -> usize {
        match setting {
            EngineSetting::ShadowQuality => {
                (self.shadow_quality as usize).min(QUALITY_NAMES.len() - 1)
            }
            EngineSetting::SsaoQuality => (self.ssao_quality as usize).min(QUALITY_NAMES.len() - 1),
            EngineSetting::EffectsQuality => {
                (self.effects_quality as usize).min(QUALITY_NAMES.len() - 1)
            }
            EngineSetting::WindowMode => usize::from(self.window_mode != WindowMode::Windowed),
            EngineSetting::Resolution => RESOLUTION_PRESETS
                .iter()
                .position(|(_, size)| *size == self.resolution)
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// Selects choice `index` of a choice row. Fullscreen stands in for the borderless mode and
    /// for a saved fullscreen, which stays as it is when Fullscreen is selected again.
    pub(crate) fn set_choice_index(&mut self, setting: EngineSetting, index: usize) {
        match setting {
            EngineSetting::ShadowQuality => self.shadow_quality = index as u32,
            EngineSetting::SsaoQuality => self.ssao_quality = index as u32,
            EngineSetting::EffectsQuality => self.effects_quality = index as u32,
            EngineSetting::WindowMode => {
                self.window_mode = match (index, self.window_mode) {
                    (0, _) => WindowMode::Windowed,
                    (_, WindowMode::Windowed) => WindowMode::Borderless,
                    (_, mode) => mode,
                };
            }
            EngineSetting::Resolution => {
                let offset = usize::from(!self.resolution_is_listed());
                if let Some((_, size)) = index
                    .checked_sub(offset)
                    .and_then(|at| RESOLUTION_PRESETS.get(at))
                {
                    self.resolution = *size;
                }
            }
            _ => {}
        }
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn resolution_label(size: (u32, u32)) -> String {
    format!("{}×{}", size.0, size.1)
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
        prefs.set_effects_quality(1);
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
            effects_quality: 2,
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
                effects_quality: 1,
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
        expected.set_effects_quality(2);
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
        assert!(settings.choices(EngineSetting::MouseSensitivity).is_empty());
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
                "Effects quality",
            ]
        );
    }

    #[test]
    fn effects_quality_is_listed_with_the_quality_names() {
        let settings = settings();

        let choices = settings.choices(EngineSetting::EffectsQuality);

        assert_eq!(EngineSetting::EffectsQuality.label(), "Effects quality");
        assert_eq!(EngineSetting::EffectsQuality.control(), Control::Choice);
        assert_eq!(
            EngineSetting::ALL.last(),
            Some(&EngineSetting::EffectsQuality)
        );
        assert_eq!(choices, ["Off", "Low", "Medium", "High", "Ultra"]);
        assert_eq!(settings.value_text(EngineSetting::EffectsQuality), "Medium");
    }

    #[test]
    fn effects_quality_is_staged_until_apply_and_dropped_by_back() {
        let mut prefs = Prefs::default();
        prefs.set_effects_quality(3);
        let mut staged = EngineSettings::from_prefs(&prefs);

        staged.set_choice_index(EngineSetting::EffectsQuality, 1);
        let before_apply = prefs.effects_quality();
        let mut discarded = prefs.clone();
        let back = EngineSettings::from_prefs(&discarded);
        back.write_to(&mut discarded);
        staged.write_to(&mut prefs);

        assert_eq!(staged.effects_quality, 1);
        assert_eq!(staged.choice_index(EngineSetting::EffectsQuality), 1);
        assert_eq!(before_apply, 3);
        assert_eq!(discarded.effects_quality(), 3);
        assert_eq!(prefs.effects_quality(), 1);
        assert_eq!(EngineSettings::from_prefs(&prefs).effects_quality, 1);
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
            0u32..=4,
            any_mode(),
            (1u32..=8192, 1u32..=8192),
        )
            .prop_map(
                |(sens, smooth, invert, sfx, music, ui, shadow, ssao, effects, mode, size)| {
                    EngineSettings {
                        mouse_sensitivity: sens,
                        mouse_smoothing: smooth,
                        invert_mouse_y: invert,
                        sound_effect_volume: sfx,
                        music_volume: music,
                        ui_volume: ui,
                        shadow_quality: shadow,
                        ssao_quality: ssao,
                        effects_quality: effects,
                        window_mode: mode,
                        resolution: size,
                    }
                },
            )
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
