/// Audio settings and configuration for the Moho engine.
///
/// Provides user-configurable audio options including volume levels
/// for different audio categories.

#[derive(Debug, Clone)]
pub struct AudioSettings {
    /// Master volume (0.0 to 1.0)
    master_volume: f32,

    /// Sound effects volume (0.0 to 1.0)
    sound_effects_volume: f32,

    /// Background music volume (0.0 to 1.0)
    music_volume: f32,

    /// UI sounds volume (0.0 to 1.0)
    ui_volume: f32,

    /// Voice/dialogue volume (0.0 to 1.0)
    voice_volume: f32,

    /// Whether audio is globally muted
    muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 1.0,
            sound_effects_volume: 0.8,
            music_volume: 0.6,
            ui_volume: 0.7,
            voice_volume: 0.9,
            muted: false,
        }
    }
}

impl AudioSettings {
    /// Create new audio settings with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Levels from saved prefs: each category is its 0..=10 pref divided by ten.
    pub fn from_prefs(_prefs: &moho_core::prefs::Prefs) -> Self {
        todo!()
    }

    /// Set master volume (affects all audio)
    pub fn with_master_volume(mut self, volume: f32) -> Self {
        self.master_volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Set sound effects volume
    pub fn with_sound_effects_volume(mut self, volume: f32) -> Self {
        self.sound_effects_volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Set background music volume
    pub fn with_music_volume(mut self, volume: f32) -> Self {
        self.music_volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Set UI sounds volume
    pub fn with_ui_volume(mut self, volume: f32) -> Self {
        self.ui_volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Set voice/dialogue volume
    pub fn with_voice_volume(mut self, volume: f32) -> Self {
        self.voice_volume = volume.clamp(0.0, 1.0);
        self
    }

    /// Mute or unmute all audio
    pub fn set_muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// Get effective volume for a specific audio category
    pub fn effective_volume(&self, category: &crate::audio_source::AudioCategory) -> f32 {
        if self.muted {
            return 0.0;
        }

        let category_volume = match category {
            crate::audio_source::AudioCategory::SoundEffect => self.sound_effects_volume,
            crate::audio_source::AudioCategory::Music => self.music_volume,
            crate::audio_source::AudioCategory::UserInterface => self.ui_volume,
            crate::audio_source::AudioCategory::Voice => self.voice_volume,
        };

        self.master_volume * category_volume
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_source::AudioCategory;

    type VolumeBuilder = fn(AudioSettings, f32) -> AudioSettings;

    const TOLERANCE: f32 = 1e-6;

    const CATEGORY_ROWS: [(AudioCategory, VolumeBuilder); 4] = [
        (
            AudioCategory::SoundEffect,
            AudioSettings::with_sound_effects_volume,
        ),
        (AudioCategory::Music, AudioSettings::with_music_volume),
        (AudioCategory::UserInterface, AudioSettings::with_ui_volume),
        (AudioCategory::Voice, AudioSettings::with_voice_volume),
    ];

    fn default_volume(category: &AudioCategory) -> f32 {
        match category {
            AudioCategory::SoundEffect => 0.8,
            AudioCategory::Music => 0.6,
            AudioCategory::UserInterface => 0.7,
            AudioCategory::Voice => 0.9,
        }
    }

    fn prefs_with(sfx: f32, music: f32, ui: f32, voice: f32) -> moho_core::prefs::Prefs {
        let mut prefs = moho_core::prefs::Prefs::default();
        *prefs.sound_effect_volume_mut() = sfx;
        *prefs.music_volume_mut() = music;
        *prefs.ui_volume_mut() = ui;
        *prefs.voice_volume_mut() = voice;
        prefs
    }

    fn assert_volumes(settings: &AudioSettings, expected: [f32; 4]) {
        for ((category, _), want) in CATEGORY_ROWS.iter().zip(expected) {
            let actual = settings.effective_volume(category);

            assert!(
                (actual - want).abs() < TOLERANCE,
                "category={category:?} expected={want} actual={actual}"
            );
        }
    }

    #[test]
    fn default_is_today_s_levels_and_matches_prefs_defaults() {
        let prefs = moho_core::prefs::Prefs::default();

        let defaults = AudioSettings::default();
        let from_defaults = AudioSettings::from_prefs(&prefs);

        assert_volumes(&defaults, [0.8, 0.6, 0.7, 0.9]);
        assert_volumes(&from_defaults, [0.8, 0.6, 0.7, 0.9]);
        assert_eq!(
            [
                prefs.sound_effect_volume(),
                prefs.music_volume(),
                prefs.ui_volume(),
                prefs.voice_volume()
            ],
            [8.0, 6.0, 7.0, 9.0]
        );
    }

    #[test]
    fn from_prefs_zero_volume_is_silent() {
        let settings = AudioSettings::from_prefs(&prefs_with(0.0, 0.0, 0.0, 0.0));

        assert_volumes(&settings, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn from_prefs_scales_each_category_by_a_tenth() {
        let settings = AudioSettings::from_prefs(&prefs_with(5.0, 10.0, 3.0, 1.0));

        assert_volumes(&settings, [0.5, 1.0, 0.3, 0.1]);
        assert!(!settings.muted);
        assert_eq!(settings.master_volume, 1.0);
    }

    #[test]
    fn from_prefs_clamps_out_of_range_values() {
        let settings = AudioSettings::from_prefs(&prefs_with(25.0, -3.0, 11.0, 100.0));

        assert_volumes(&settings, [1.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn category_volume_scales_by_master_and_leaves_others_at_default() {
        for (set, builder) in &CATEGORY_ROWS {
            let settings = builder(AudioSettings::new().with_master_volume(0.5), 0.5);

            for (read, _) in &CATEGORY_ROWS {
                let expected = if read == set {
                    0.25
                } else {
                    default_volume(read) * 0.5
                };
                let actual = settings.effective_volume(read);

                assert!(
                    (actual - expected).abs() < TOLERANCE,
                    "set={set:?} read={read:?} expected={expected} actual={actual}"
                );
            }
        }
    }

    #[test]
    fn category_volume_above_one_clamps_to_one() {
        for (category, builder) in &CATEGORY_ROWS {
            let settings = builder(AudioSettings::new().with_master_volume(1.0), 1.5);

            let actual = settings.effective_volume(category);

            assert!(
                (actual - 1.0).abs() < TOLERANCE,
                "category={category:?} actual={actual}"
            );
        }
    }

    #[test]
    fn mute_silences_every_category() {
        let settings = CATEGORY_ROWS
            .iter()
            .fold(
                AudioSettings::new().with_master_volume(1.0),
                |acc, (_, builder)| builder(acc, 1.0),
            )
            .set_muted(true);

        for (category, _) in &CATEGORY_ROWS {
            let actual = settings.effective_volume(category);

            assert!(actual == 0.0, "category={category:?} actual={actual}");
        }
    }

    #[test]
    fn effective_volume_multiplies_clamped_master_and_category() {
        let settings = AudioSettings::new()
            .with_master_volume(0.5)
            .with_music_volume(1.5);

        let music = settings.effective_volume(&AudioCategory::Music);

        assert!((music - 0.5).abs() < f32::EPSILON, "music={music}");
    }

    #[test]
    fn effective_volume_clamps_master() {
        let settings = AudioSettings::new()
            .with_master_volume(2.0)
            .with_music_volume(0.5);

        let music = settings.effective_volume(&AudioCategory::Music);

        assert!((music - 0.5).abs() < f32::EPSILON, "music={music}");
    }
}
