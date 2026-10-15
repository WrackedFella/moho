//! The seam between the audio rules and the thing that makes sound.

use crate::error::AudioResult;
use crate::spatial::StereoGain;

/// One sound to start: its encoded file bytes and how to play them.
#[derive(Debug, Clone)]
pub struct Playback {
    /// Where the bytes came from; named in decode errors.
    pub path: String,
    /// The encoded audio file.
    pub bytes: Vec<u8>,
    /// Final playback volume, already scaled by category and master settings.
    pub volume: f32,
    /// Whether the sound repeats until stopped.
    pub looped: bool,
    /// Per-ear gains for a positioned sound; `None` plays the file's own channels.
    pub pan: Option<StereoGain>,
}

/// Plays sounds on behalf of [`AudioSystem`](crate::AudioSystem).
pub trait AudioOutput {
    /// Handle to one started sound.
    type Voice;

    /// Starts playing `playback`.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes cannot be decoded; nothing is started then.
    fn start(&mut self, playback: Playback) -> AudioResult<Self::Voice>;

    /// Changes the volume of a playing voice.
    fn set_volume(&mut self, voice: &Self::Voice, volume: f32);

    /// Stops a voice and releases it.
    fn stop(&mut self, voice: Self::Voice);

    /// Lets a voice play to its end without further control.
    fn detach(&mut self, voice: Self::Voice);
}
