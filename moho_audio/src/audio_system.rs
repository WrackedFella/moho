use crate::audio_cache::AudioCache;
use crate::audio_events::AudioEvent;
use crate::audio_settings::AudioSettings;
use crate::audio_source::{AudioCategory, AudioSource};
#[cfg(test)]
use crate::error::AudioError;
use crate::error::AudioResult;
use crate::output::{AudioOutput, Playback};
use crate::rodio_output::RodioOutput;
use crate::spatial::{Listener, distance_gain, stereo_pan};
use glam::Vec3;
use std::path::Path;
use tracing::{debug, warn};

const CLICK_SOUND: &str = "assets/audio/ui/button_click.mp3";

/// Main audio system for the Moho engine.
///
/// AudioSystem manages audio playback, asset loading, and provides
/// a clean interface for the rest of the engine to play sounds.
pub struct AudioSystem<O: AudioOutput = RodioOutput> {
    output: O,

    /// Current audio settings
    settings: AudioSettings,

    /// Audio cache manager (handles loading and caching)
    audio_cache: AudioCache,

    /// Looping background music voice, kept for live volume changes.
    music_voice: Option<O::Voice>,

    listener: Listener,
}

impl AudioSystem {
    /// Create a new audio system playing through the default audio device.
    ///
    /// # Errors
    ///
    /// Returns an error if the device or the UI sound cache cannot be opened.
    pub fn new() -> AudioResult<Self> {
        let output = RodioOutput::open_default()?;

        debug!("Audio system initialized successfully");

        Ok(Self::with_output(output, AudioCache::new()?))
    }
}

impl<O: AudioOutput> AudioSystem<O> {
    pub(crate) fn with_output(output: O, audio_cache: AudioCache) -> Self {
        Self {
            output,
            settings: AudioSettings::default(),
            audio_cache,
            music_voice: None,
            listener: Listener::default(),
        }
    }

    /// Moves the listener that positioned sounds are measured from. Sounds already
    /// playing keep the gain they started with.
    pub fn set_listener(&mut self, listener: Listener) {
        self.listener = listener;
    }

    /// Get current audio settings
    pub fn settings(&self) -> &AudioSettings {
        &self.settings
    }

    /// Update audio settings
    pub fn set_settings(&mut self, settings: AudioSettings) {
        self.settings = settings;

        // Update music volume if currently playing
        if let Some(voice) = &self.music_voice {
            let music_volume = self.settings.effective_volume(&AudioCategory::Music);
            self.output.set_volume(voice, music_volume);
        }
    }

    /// Play a sound from an AudioSource with optimized loading
    ///
    /// Sound-effect and voice sources with a position are attenuated by their distance
    /// from the listener and not started at all beyond audible range; music and UI
    /// sources ignore a position.
    pub fn play_audio_source(&mut self, source: &AudioSource) -> AudioResult<()> {
        let Some(distance_gain) = self.distance_gain_for(source) else {
            debug!(
                path = source.path(),
                "Skipping audio playback: out of audible range or non-finite distance"
            );
            return Ok(());
        };
        let effective_volume =
            self.settings.effective_volume(source.category()) * source.volume() * distance_gain;

        if effective_volume <= 0.0 {
            debug!(
                path = source.path(),
                "Skipping audio playback due to zero volume"
            );
            return Ok(());
        }

        // Use pre-loaded data for UI sounds, or load on-demand for others
        let bytes = if *source.category() == AudioCategory::UserInterface {
            self.audio_cache.get_ui_sound(source.path())?
        } else {
            self.audio_cache.get_audio(source.path())?
        };

        let pan = Self::world_position(source).map(|position| stereo_pan(&self.listener, position));
        let voice = self.output.start(Playback {
            path: source.path().to_owned(),
            bytes,
            volume: effective_volume,
            looped: source.looped(),
            pan,
        })?;

        if *source.category() == AudioCategory::Music && source.looped() {
            if let Some(old_voice) = self.music_voice.take() {
                self.output.stop(old_voice);
            }
            self.music_voice = Some(voice);
        } else {
            self.output.detach(voice);
        }

        debug!(
            path = source.path(),
            volume = effective_volume,
            pan = ?pan,
            "Playing audio"
        );
        Ok(())
    }

    fn distance_gain_for(&self, source: &AudioSource) -> Option<f32> {
        match Self::world_position(source) {
            Some(position) => distance_gain(self.listener.position.distance(position)),
            None => Some(1.0),
        }
    }

    /// Position of a sound heard in space: only positioned sound effects and voices.
    fn world_position(source: &AudioSource) -> Option<Vec3> {
        match (source.category(), source.position()) {
            (AudioCategory::SoundEffect | AudioCategory::Voice, Some(position)) => {
                Some(Vec3::from_array(position))
            }
            _ => None,
        }
    }

    /// Play a sound file with specified volume
    pub fn play_sound<P: AsRef<Path>>(&mut self, path: P, volume: f32) -> AudioResult<()> {
        let source = AudioSource::new(path).with_volume(volume);
        self.play_audio_source(&source)
    }

    /// Handle audio events from the UI or game
    pub fn handle_event(&mut self, event: AudioEvent) -> AudioResult<()> {
        match event {
            AudioEvent::ButtonClick => self.play_click(0.8),
            AudioEvent::MenuNavigate => self.play_click(0.6),
            AudioEvent::Confirm => self.play_click(0.8),
            AudioEvent::Cancel => self.play_click(0.7),
            AudioEvent::Error => self.play_click(0.9),
            AudioEvent::PlaySound { path, volume } => self.play_sound(&path, volume),
            AudioEvent::PlaySoundAt {
                path,
                volume,
                position,
            } => {
                let source = AudioSource::new(&path)
                    .with_volume(volume)
                    .with_position(position.to_array());
                self.play_audio_source(&source)
            }
            AudioEvent::MusicStart {
                path,
                volume,
                looped,
            } => {
                let source = AudioSource::background_music(&path, volume).with_looping(looped);
                self.play_audio_source(&source)
            }
            AudioEvent::MusicStop => {
                self.stop_audio(Some(AudioCategory::Music));
                Ok(())
            }
            AudioEvent::MusicVolumeChanged { volume: _ } => {
                // TODO: set the music voice's volume to `volume` times the music category volume.
                Ok(())
            }
            AudioEvent::StopAll => {
                self.stop_audio(None);
                Ok(())
            }
        }
    }

    /// Play the UI click, served from the pre-loaded UI cache for low latency.
    fn play_click(&mut self, volume: f32) -> AudioResult<()> {
        self.play_audio_source(&AudioSource::ui_sound(CLICK_SOUND, volume))
    }

    /// Stop audio based on category
    pub fn stop_audio(&mut self, category: Option<AudioCategory>) {
        match category {
            None | Some(AudioCategory::Music) => {
                // Individual sound effects can't be stopped once detached
                if let Some(voice) = self.music_voice.take() {
                    self.output.stop(voice);
                }
            }
            Some(
                AudioCategory::SoundEffect | AudioCategory::UserInterface | AudioCategory::Voice,
            ) => {
                // Individual sound effects auto-cleanup when finished
                warn!("Cannot stop individual sound effects once started");
            }
        }
    }
}

impl<O: AudioOutput> Drop for AudioSystem<O> {
    fn drop(&mut self) {
        if let Some(voice) = self.music_voice.take() {
            self.output.stop(voice);
        }
        debug!("Audio system shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::{Listener, MAX_AUDIBLE_DISTANCE};
    use crate::test_support::{RecordingOutput, assert_volume_eq, wav_bytes};
    use glam::{Quat, Vec3};

    const CLICK: &str = "assets/audio/ui/button_click.mp3";
    const SOUND: &str = "sounds/boom.wav";
    const MENU_MUSIC: &str = "music/menu.wav";
    const BATTLE_MUSIC: &str = "music/battle.wav";

    fn system(settings: AudioSettings) -> (AudioSystem<RecordingOutput>, RecordingOutput) {
        // The click is seeded with different bytes in each cache so a test can tell
        // which cache a UI event was served from.
        let cache = AudioCache::seeded(
            &[
                (CLICK, wav_bytes(50, 100)),
                (SOUND, wav_bytes(100, 1000)),
                (MENU_MUSIC, wav_bytes(200, 2000)),
                (BATTLE_MUSIC, wav_bytes(300, 3000)),
            ],
            &[(CLICK, wav_bytes(10, 500))],
        );
        let output = RecordingOutput::default();
        let mut system = AudioSystem::with_output(output.clone(), cache);
        system.set_settings(settings);
        (system, output)
    }

    fn full_volume() -> AudioSettings {
        AudioSettings::new()
            .with_master_volume(1.0)
            .with_ui_volume(1.0)
            .with_sound_effects_volume(1.0)
            .with_music_volume(1.0)
    }

    fn music_start(path: &str, volume: f32) -> AudioEvent {
        AudioEvent::MusicStart {
            path: path.to_owned(),
            volume,
            looped: true,
        }
    }

    fn play_at(path: &str, volume: f32, position: Vec3) -> AudioEvent {
        AudioEvent::PlaySoundAt {
            path: path.to_owned(),
            volume,
            position,
        }
    }

    fn listener_at(position: Vec3) -> Listener {
        Listener {
            position,
            ..Listener::default()
        }
    }

    #[test]
    fn ui_event_plays_click_once_at_its_volume() {
        let rows = [
            (AudioEvent::ButtonClick, 0.8),
            (AudioEvent::MenuNavigate, 0.6),
            (AudioEvent::Confirm, 0.8),
            (AudioEvent::Cancel, 0.7),
            (AudioEvent::Error, 0.9),
        ];

        for (event, volume) in rows {
            let (mut system, output) = system(full_volume());

            let result = system.handle_event(event.clone());

            result.unwrap_or_else(|e| panic!("{event:?} failed: {e}"));
            let voices = output.voices();
            assert_eq!(voices.len(), 1, "{event:?}");
            assert_eq!(voices[0].path, CLICK, "{event:?}");
            assert_eq!(
                voices[0].bytes,
                wav_bytes(10, 500),
                "{event:?} used the UI cache"
            );
            assert_volume_eq(voices[0].volume, volume);
            assert!(!voices[0].looped, "{event:?}");
            assert!(voices[0].detached && !voices[0].stopped, "{event:?}");
        }
    }

    #[test]
    fn ui_event_volume_is_scaled_by_the_ui_category_volume() {
        let (mut system, output) = system(full_volume().with_ui_volume(0.5));

        system.handle_event(AudioEvent::Cancel).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_volume_eq(voices[0].volume, 0.35);
    }

    #[test]
    fn play_sound_scales_by_effect_volume() {
        let (mut system, output) = system(full_volume().with_sound_effects_volume(0.5));

        system
            .handle_event(AudioEvent::PlaySound {
                path: SOUND.to_owned(),
                volume: 0.8,
            })
            .unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].path, SOUND);
        assert_eq!(voices[0].bytes, wav_bytes(100, 1000));
        assert_volume_eq(voices[0].volume, 0.4);
        assert!(!voices[0].looped);
    }

    #[test]
    fn muted_play_sound_starts_nothing_and_succeeds() {
        let (mut system, output) = system(full_volume().set_muted(true));

        let result = system.handle_event(AudioEvent::PlaySound {
            path: SOUND.to_owned(),
            volume: 0.8,
        });

        assert!(result.is_ok(), "{result:?}");
        assert!(output.voices().is_empty());
    }

    #[test]
    fn music_start_stops_previous_track_and_loops_new_one() {
        let (mut system, output) = system(full_volume());
        system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();

        system.handle_event(music_start(BATTLE_MUSIC, 1.0)).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].path, MENU_MUSIC);
        assert!(voices[0].stopped);
        assert_eq!(voices[1].path, BATTLE_MUSIC);
        assert!(voices[1].looped);
        assert!(!voices[1].stopped);
        assert!(!voices[1].detached, "the music voice is kept, not detached");
    }

    #[test]
    fn set_settings_changes_playing_music_volume() {
        let (mut system, output) = system(full_volume());
        system.handle_event(music_start(MENU_MUSIC, 0.8)).unwrap();
        assert_volume_eq(output.voices()[0].volume, 0.8);

        system.set_settings(full_volume().with_music_volume(0.5));

        // The category volume replaces the track's own volume (pinned, not endorsed).
        assert_volume_eq(output.voices()[0].volume, 0.5);
    }

    #[test]
    fn stop_events_stop_music() {
        for event in [AudioEvent::MusicStop, AudioEvent::StopAll] {
            let (mut system, output) = system(full_volume());
            system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();

            let result = system.handle_event(event.clone());

            assert!(result.is_ok(), "{event:?}: {result:?}");
            assert!(output.voices()[0].stopped, "{event:?}");
            system.set_settings(full_volume().with_music_volume(0.1));
            assert_volume_eq(output.voices()[0].volume, 1.0);
        }
    }

    #[test]
    fn missing_file_errors_with_its_path_and_starts_nothing() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("absent.wav");
        let path = missing.to_str().expect("utf-8 path").to_owned();
        let (mut system, output) = system(full_volume());

        let result = system.handle_event(AudioEvent::PlaySound {
            path: path.clone(),
            volume: 0.8,
        });

        match result {
            Err(AudioError::FileNotFound(message)) => {
                assert!(message.contains(&path), "{message}");
            }
            other => panic!("expected FileNotFound, got {other:?}"),
        }
        assert!(output.voices().is_empty());
    }

    #[test]
    fn one_shot_is_detached_and_music_is_kept() {
        let (mut system, output) = system(full_volume());

        system
            .handle_event(AudioEvent::PlaySound {
                path: SOUND.to_owned(),
                volume: 1.0,
            })
            .unwrap();
        system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();
        system.handle_event(AudioEvent::StopAll).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 2);
        assert!(voices[0].detached && !voices[0].stopped, "one-shot");
        assert!(!voices[1].detached && voices[1].stopped, "music");
    }

    #[test]
    fn dropping_the_system_stops_the_music() {
        let (mut system, output) = system(full_volume());
        system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();

        drop(system);

        assert!(output.voices()[0].stopped);
    }

    #[test]
    fn unlooped_music_start_is_a_one_shot_and_keeps_the_playing_track() {
        let (mut system, output) = system(full_volume());
        system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();

        system
            .handle_event(AudioEvent::MusicStart {
                path: BATTLE_MUSIC.to_owned(),
                volume: 1.0,
                looped: false,
            })
            .unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].path, MENU_MUSIC);
        assert!(!voices[0].stopped && !voices[0].detached, "menu track kept");
        assert_eq!(voices[1].path, BATTLE_MUSIC);
        assert!(voices[1].detached && !voices[1].stopped, "battle one-shot");
        assert!(!voices[1].looped);
        system.set_settings(full_volume().with_music_volume(0.5));
        let voices = output.voices();
        assert_volume_eq(voices[0].volume, 0.5);
        assert_volume_eq(voices[1].volume, 1.0);
    }

    #[test]
    fn failed_music_start_keeps_the_playing_track() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("absent.wav");
        let path = missing.to_str().expect("utf-8 path").to_owned();
        let (mut system, output) = system(full_volume());
        system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();

        let result = system.handle_event(music_start(&path, 1.0));

        assert!(
            matches!(result, Err(AudioError::FileNotFound(_))),
            "{result:?}"
        );
        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert!(!voices[0].stopped);
        system.handle_event(AudioEvent::MusicStop).unwrap();
        assert!(output.voices()[0].stopped, "the menu voice was retained");
    }

    #[test]
    fn stopping_a_non_music_category_leaves_music_playing() {
        let categories = [
            AudioCategory::SoundEffect,
            AudioCategory::UserInterface,
            AudioCategory::Voice,
        ];

        for category in categories {
            let (mut system, output) = system(full_volume());
            system.handle_event(music_start(MENU_MUSIC, 1.0)).unwrap();

            system.stop_audio(Some(category.clone()));

            assert!(!output.voices()[0].stopped, "{category:?}");
            system.set_settings(full_volume().with_music_volume(0.25));
            assert_volume_eq(output.voices()[0].volume, 0.25);
        }
    }

    #[test]
    fn nearer_positioned_sound_plays_louder() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());

        system
            .handle_event(play_at(SOUND, 1.0, Vec3::new(3.0, 0.0, -4.0)))
            .unwrap();
        system
            .handle_event(play_at(SOUND, 1.0, Vec3::new(0.0, 0.0, -20.0)))
            .unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].path, SOUND);
        assert_eq!(voices[0].bytes, wav_bytes(100, 1000));
        assert_volume_eq(voices[0].volume, 0.4);
        assert_volume_eq(voices[1].volume, 0.1);
        assert!(voices[0].detached && !voices[0].looped, "one-shot");
    }

    #[test]
    fn positioned_sound_within_reference_plays_at_its_volume() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());

        system
            .handle_event(play_at(SOUND, 0.8, Vec3::new(0.0, 0.5, 0.0)))
            .unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_volume_eq(voices[0].volume, 0.8);
    }

    #[test]
    fn positioned_sound_beyond_range_starts_nothing_and_succeeds() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());
        let beyond = Vec3::new(MAX_AUDIBLE_DISTANCE + 1.0, 0.0, 0.0);

        let result = system.handle_event(play_at(SOUND, 1.0, beyond));

        assert!(result.is_ok(), "{result:?}");
        assert!(output.voices().is_empty());
    }

    #[test]
    fn positioned_sound_at_a_non_finite_position_starts_nothing_and_succeeds() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());
        let nowhere = Vec3::new(f32::NAN, 0.0, 0.0);

        let result = system.handle_event(play_at(SOUND, 1.0, nowhere));

        assert!(result.is_ok(), "{result:?}");
        assert!(output.voices().is_empty());
    }

    #[test]
    fn distance_is_measured_from_the_current_listener() {
        let (mut system, output) = system(full_volume());
        let sound = Vec3::new(20.0, 0.0, 0.0);
        system.set_listener(Listener::default());
        system.handle_event(play_at(SOUND, 1.0, sound)).unwrap();
        system.set_listener(listener_at(Vec3::new(18.0, 0.0, 0.0)));

        system.handle_event(play_at(SOUND, 1.0, sound)).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 2);
        assert_volume_eq(voices[0].volume, 0.1);
        assert_volume_eq(voices[1].volume, 1.0);
    }

    #[test]
    fn positioned_sound_obeys_category_volume() {
        let (mut system, output) = system(
            full_volume()
                .with_master_volume(1.0)
                .with_sound_effects_volume(0.5),
        );
        let here = Vec3::new(3.0, 4.0, 5.0);
        system.set_listener(listener_at(here));

        system.handle_event(play_at(SOUND, 1.0, here)).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_volume_eq(voices[0].volume, 0.5);
    }

    #[test]
    fn muted_positioned_sound_starts_nothing_and_succeeds() {
        let (mut system, output) = system(full_volume().set_muted(true));
        system.set_listener(Listener::default());

        let result = system.handle_event(play_at(SOUND, 1.0, Vec3::ZERO));

        assert!(result.is_ok(), "{result:?}");
        assert!(output.voices().is_empty());
    }

    #[test]
    fn positioned_sound_scales_by_distance_and_category_together() {
        let (mut system, output) = system(
            full_volume()
                .with_master_volume(0.5)
                .with_sound_effects_volume(0.5),
        );
        system.set_listener(Listener::default());

        system
            .handle_event(play_at(SOUND, 0.8, Vec3::new(0.0, 0.0, 8.0)))
            .unwrap();

        assert_volume_eq(output.voices()[0].volume, 0.05);
    }

    #[test]
    fn positioned_voice_source_is_attenuated() {
        let (mut system, output) = system(full_volume().with_voice_volume(1.0));
        system.set_listener(Listener::default());
        let source = AudioSource::new(SOUND)
            .with_category(AudioCategory::Voice)
            .with_position([10.0, 0.0, 0.0]);

        system.play_audio_source(&source).unwrap();

        assert_volume_eq(output.voices()[0].volume, 0.2);
    }

    #[test]
    fn unpositioned_sound_ignores_the_listener() {
        let (mut system, output) = system(full_volume());
        system.set_listener(listener_at(Vec3::new(100.0, 0.0, 0.0)));

        system
            .handle_event(AudioEvent::PlaySound {
                path: SOUND.to_owned(),
                volume: 0.8,
            })
            .unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_volume_eq(voices[0].volume, 0.8);
    }

    #[test]
    fn positioned_sound_carries_its_pan() {
        let listener = Listener {
            position: Vec3::new(5.0, 0.0, 5.0),
            orientation: Quat::from_rotation_y(std::f32::consts::PI),
        };
        let east = Vec3::new(15.0, 0.0, 5.0);
        let voice_source = AudioSource::new(SOUND)
            .with_category(AudioCategory::Voice)
            .with_position(east.to_array());
        let rows = [("play event", None), ("voice source", Some(voice_source))];

        for (name, source) in rows {
            let (mut system, output) = system(full_volume());
            system.set_listener(listener);

            match source {
                Some(source) => system.play_audio_source(&source).unwrap(),
                None => system.handle_event(play_at(SOUND, 1.0, east)).unwrap(),
            }

            let voices = output.voices();
            assert_eq!(voices.len(), 1, "{name}");
            let pan = voices[0].pan.expect("a positioned sound is panned");
            assert!(
                (pan.left - 1.0).abs() < 1e-5 && pan.right.abs() < 1e-5,
                "{name}: {pan:?}"
            );
        }
    }

    #[test]
    fn unpositioned_sound_has_no_pan() {
        let (mut system, output) = system(full_volume());
        system.set_listener(listener_at(Vec3::new(100.0, 0.0, 0.0)));

        system.play_sound(SOUND, 0.8).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].pan, None);
    }

    #[test]
    fn sound_behind_plays_quieter_than_in_front() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());
        let north = Vec3::new(0.0, 0.0, -10.0);
        let south = Vec3::new(0.0, 0.0, 10.0);
        let east = Vec3::new(10.0, 0.0, 0.0);

        system.handle_event(play_at(SOUND, 1.0, north)).unwrap();
        system.handle_event(play_at(SOUND, 1.0, east)).unwrap();
        system.handle_event(play_at(SOUND, 1.0, south)).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 3);
        assert!(voices[2].volume < voices[0].volume, "south vs north");
        assert!(voices[2].volume < voices[1].volume, "south vs east");
        assert_volume_eq(voices[2].volume, 0.2 * crate::spatial::REAR_GAIN);
    }

    #[test]
    fn sound_in_front_plays_at_its_distance_volume() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());

        system
            .handle_event(play_at(SOUND, 1.0, Vec3::new(0.0, 0.0, -10.0)))
            .unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        let expected = distance_gain(10.0).expect("10 m is audible");
        assert_volume_eq(voices[0].volume, expected);
    }

    #[test]
    fn unpositioned_sound_gets_no_rear_cue() {
        let (mut system, output) = system(full_volume());
        system.set_listener(Listener::default());

        system.play_sound(SOUND, 0.8).unwrap();

        let voices = output.voices();
        assert_eq!(voices.len(), 1);
        assert_volume_eq(voices[0].volume, 0.8);
    }

    #[test]
    fn music_and_ui_ignore_a_position() {
        let far = [500.0, 0.0, 0.0];
        let sources = [
            AudioSource::background_music(MENU_MUSIC, 0.8).with_position(far),
            AudioSource::ui_sound(CLICK, 0.8).with_position(far),
        ];

        for source in sources {
            let (mut system, output) = system(full_volume());
            system.set_listener(Listener::default());

            let result = system.play_audio_source(&source);

            assert!(result.is_ok(), "{:?}: {result:?}", source.category());
            let voices = output.voices();
            assert_eq!(voices.len(), 1, "{:?}", source.category());
            assert_volume_eq(voices[0].volume, 0.8);
            assert_eq!(voices[0].pan, None, "{:?}", source.category());
        }
    }
}
