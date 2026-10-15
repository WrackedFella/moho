//! [`AudioOutput`] backed by rodio and the default audio device.

use crate::error::{AudioError, AudioResult};
use crate::output::{AudioOutput, Playback};
use crate::stereo_gain_source::StereoGainSource;
use rodio::Source;
use rodio::mixer::Mixer;

/// Plays voices through a rodio mixer, normally the default device's.
pub struct RodioOutput {
    mixer: Mixer,
    /// Keeps the audio stream alive; absent for a headless mixer.
    _device: Option<rodio::MixerDeviceSink>,
}

impl RodioOutput {
    /// Opens the default audio device.
    ///
    /// # Errors
    ///
    /// Returns [`AudioError::InitializationFailed`] if no device can be opened.
    pub fn open_default() -> AudioResult<Self> {
        let device = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|e| AudioError::InitializationFailed(e.to_string()))?;
        Ok(Self {
            mixer: device.mixer().clone(),
            _device: Some(device),
        })
    }

    /// An output into a bare mixer, with the mixer's read end for the test to pull samples from.
    #[cfg(test)]
    pub(crate) fn headless() -> (Self, rodio::mixer::MixerSource) {
        use std::num::NonZero;
        let (mixer, source) = rodio::mixer::mixer(
            NonZero::new(2).expect("two channels"),
            NonZero::new(44_100).expect("44.1 kHz"),
        );
        (
            Self {
                mixer,
                _device: None,
            },
            source,
        )
    }
}

impl AudioOutput for RodioOutput {
    type Voice = rodio::Player;

    fn start(&mut self, playback: Playback) -> AudioResult<rodio::Player> {
        let decoder = rodio::Decoder::new(std::io::Cursor::new(playback.bytes)).map_err(|e| {
            AudioError::LoadFailed(format!("Failed to decode {}: {}", playback.path, e))
        })?;

        let player = rodio::Player::connect_new(&self.mixer);
        player.set_volume(playback.volume);
        match (playback.looped, playback.pan) {
            (true, Some(pan)) => {
                player.append(StereoGainSource::new(decoder.repeat_infinite(), pan));
            }
            (true, None) => player.append(decoder.repeat_infinite()),
            (false, Some(pan)) => player.append(StereoGainSource::new(decoder, pan)),
            (false, None) => player.append(decoder),
        }
        player.play();
        Ok(player)
    }

    fn set_volume(&mut self, voice: &rodio::Player, volume: f32) {
        voice.set_volume(volume);
    }

    fn stop(&mut self, voice: rodio::Player) {
        voice.stop();
    }

    fn detach(&mut self, voice: rodio::Player) {
        voice.detach();
    }
}

#[cfg(test)]
mod tests {
    use crate::error::AudioError;
    use crate::output::{AudioOutput, Playback};
    use crate::rodio_output::RodioOutput;
    use crate::spatial::StereoGain;
    use crate::test_support::{stereo_wav_bytes, wav_bytes};
    use rodio::mixer::MixerSource;

    /// Frames to consume so rodio's ~20 ms fade-in (882 frames) and the 5 ms
    /// control-update period are well past.
    const SETTLE_FRAMES: usize = 4_000;
    /// Amplitude 16384 / 32768.
    const FILE_LEVEL: f32 = 0.5;
    const TOLERANCE: f32 = 0.01;

    fn playback(samples: u32, volume: f32, looped: bool) -> Playback {
        Playback {
            path: "tone.wav".to_owned(),
            bytes: wav_bytes(samples, 16_384),
            volume,
            looped,
            pan: None,
        }
    }

    fn skip_frames(source: &mut MixerSource, frames: usize) {
        for _ in 0..frames * 2 {
            source.next();
        }
    }

    /// Left-channel level of the next frame; an ended mixer reads as silence.
    fn next_level(source: &mut MixerSource) -> f32 {
        let left = source.next().unwrap_or(0.0);
        source.next();
        left.abs()
    }

    /// Left and right levels of the next frame; an ended mixer reads as silence.
    fn next_frame(source: &mut MixerSource) -> (f32, f32) {
        let left = source.next().unwrap_or(0.0);
        let right = source.next().unwrap_or(0.0);
        (left, right)
    }

    #[track_caller]
    fn assert_frame(source: &mut MixerSource, left: f32, right: f32) {
        let (l, r) = next_frame(source);
        assert!(
            (l - left).abs() < TOLERANCE && (r - right).abs() < TOLERANCE,
            "frame ({l}, {r}) != expected ({left}, {right})"
        );
    }

    #[track_caller]
    fn assert_level(source: &mut MixerSource, expected: f32) {
        let level = next_level(source);
        assert!(
            (level - expected).abs() < TOLERANCE,
            "level {level} != expected {expected}"
        );
    }

    #[test]
    fn started_voice_is_heard_at_its_volume() {
        let (mut output, mut mixer) = RodioOutput::headless();

        let voice = output.start(playback(40_000, 0.5, false));

        assert!(voice.is_ok());
        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, FILE_LEVEL * 0.5);
    }

    #[test]
    fn unlooped_voice_falls_silent_when_its_file_ends() {
        let (mut output, mut mixer) = RodioOutput::headless();
        let voice = output.start(playback(10_000, 1.0, false));
        assert!(voice.is_ok());
        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, FILE_LEVEL);

        skip_frames(&mut mixer, 10_000);

        assert_level(&mut mixer, 0.0);
    }

    #[test]
    fn looped_voice_plays_past_its_length() {
        let (mut output, mut mixer) = RodioOutput::headless();

        let voice = output.start(playback(10_000, 1.0, true));

        assert!(voice.is_ok());
        skip_frames(&mut mixer, 25_000);
        assert_level(&mut mixer, FILE_LEVEL);
    }

    #[test]
    fn stopped_voice_falls_silent() {
        let (mut output, mut mixer) = RodioOutput::headless();
        let voice = output.start(playback(10_000, 1.0, true)).expect("start");
        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, FILE_LEVEL);

        output.stop(voice);

        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, 0.0);
    }

    #[test]
    fn set_volume_relevels_a_playing_voice() {
        let (mut output, mut mixer) = RodioOutput::headless();
        let voice = output.start(playback(10_000, 1.0, true)).expect("start");
        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, FILE_LEVEL);

        output.set_volume(&voice, 0.25);

        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, FILE_LEVEL * 0.25);
    }

    #[test]
    fn detached_voice_keeps_playing() {
        let (mut output, mut mixer) = RodioOutput::headless();
        let voice = output.start(playback(40_000, 1.0, false)).expect("start");

        output.detach(voice);

        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_level(&mut mixer, FILE_LEVEL);
    }

    #[test]
    fn undecodable_bytes_fail_to_load() {
        let (mut output, _mixer) = RodioOutput::headless();
        let request = Playback {
            path: "noise.wav".to_owned(),
            bytes: vec![0xde, 0xad, 0xbe, 0xef, 1, 2, 3, 4],
            volume: 1.0,
            looped: false,
            pan: None,
        };

        let result = output.start(request);

        match result.err() {
            Some(AudioError::LoadFailed(message)) => {
                assert!(message.contains("noise.wav"), "{message}");
            }
            other => panic!("expected LoadFailed, got {other:?}"),
        }
    }

    #[test]
    fn unpanned_voice_keeps_its_channels() {
        let (mut output, mut mixer) = RodioOutput::headless();
        let request = Playback {
            bytes: stereo_wav_bytes(40_000, 16_384, 8_192),
            ..playback(0, 1.0, false)
        };

        let _voice = output.start(request).expect("start");

        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_frame(&mut mixer, 0.5, 0.25);
    }

    #[test]
    fn panned_voice_is_louder_in_its_ear() {
        let rows = [
            (
                "east",
                StereoGain {
                    left: 0.2,
                    right: 0.8,
                },
                0.1,
                0.4,
            ),
            (
                "west",
                StereoGain {
                    left: 0.8,
                    right: 0.2,
                },
                0.4,
                0.1,
            ),
        ];

        for (side, pan, left, right) in rows {
            let (mut output, mut mixer) = RodioOutput::headless();
            let request = Playback {
                pan: Some(pan),
                ..playback(40_000, 1.0, false)
            };

            let _voice = output.start(request).expect("start");

            skip_frames(&mut mixer, SETTLE_FRAMES);
            let (l, r) = next_frame(&mut mixer);
            assert!(
                (l - left).abs() < TOLERANCE && (r - right).abs() < TOLERANCE,
                "{side}: frame ({l}, {r}) != expected ({left}, {right})"
            );
        }
    }

    #[test]
    fn panned_stereo_voice_is_averaged_before_panning() {
        let (mut output, mut mixer) = RodioOutput::headless();
        let request = Playback {
            bytes: stereo_wav_bytes(40_000, 16_384, 0),
            pan: Some(StereoGain {
                left: 1.0,
                right: 0.5,
            }),
            ..playback(0, 1.0, false)
        };

        let _voice = output.start(request).expect("start");

        skip_frames(&mut mixer, SETTLE_FRAMES);
        assert_frame(&mut mixer, 0.25, 0.125);
    }
}
