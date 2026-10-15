#[cfg(test)]
mod tests {
    use crate::error::AudioError;
    use crate::output::{AudioOutput, Playback};
    use crate::rodio_output::RodioOutput;
    use crate::test_support::wav_bytes;
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
        };

        let result = output.start(request);

        match result.err() {
            Some(AudioError::LoadFailed(message)) => {
                assert!(message.contains("noise.wav"), "{message}");
            }
            other => panic!("expected LoadFailed, got {other:?}"),
        }
    }
}
