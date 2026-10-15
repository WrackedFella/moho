//! Test doubles and fixtures for the audio system: a recording [`AudioOutput`] and an
//! in-memory WAV builder, so no test needs a sound device or a file under `assets/`.
#![cfg(test)]

use crate::error::AudioResult;
use crate::output::{AudioOutput, Playback};
use std::cell::RefCell;
use std::rc::Rc;

/// Sample rate of every WAV built by [`wav_bytes`].
pub const WAV_SAMPLE_RATE: u32 = 44_100;

/// What the system did with one started voice.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceRecord {
    pub path: String,
    pub bytes: Vec<u8>,
    pub volume: f32,
    pub looped: bool,
    pub stopped: bool,
    pub detached: bool,
}

/// An [`AudioOutput`] that plays nothing and records every call.
///
/// Clones share one record, so a test keeps a clone as its inspection handle after
/// moving the other into the system under test. A voice is its index in the record.
#[derive(Debug, Clone, Default)]
pub struct RecordingOutput {
    voices: Rc<RefCell<Vec<VoiceRecord>>>,
}

impl RecordingOutput {
    /// Snapshot of every voice started so far, in start order.
    pub fn voices(&self) -> Vec<VoiceRecord> {
        self.voices.borrow().clone()
    }
}

impl AudioOutput for RecordingOutput {
    type Voice = usize;

    fn start(&mut self, playback: Playback) -> AudioResult<usize> {
        let mut voices = self.voices.borrow_mut();
        voices.push(VoiceRecord {
            path: playback.path,
            bytes: playback.bytes,
            volume: playback.volume,
            looped: playback.looped,
            stopped: false,
            detached: false,
        });
        Ok(voices.len() - 1)
    }

    fn set_volume(&mut self, voice: &usize, volume: f32) {
        self.voices.borrow_mut()[*voice].volume = volume;
    }

    fn stop(&mut self, voice: usize) {
        self.voices.borrow_mut()[voice].stopped = true;
    }

    fn detach(&mut self, voice: usize) {
        self.voices.borrow_mut()[voice].detached = true;
    }
}

/// A mono 16-bit PCM WAV of `samples` samples, every one equal to `amplitude`.
pub fn wav_bytes(samples: u32, amplitude: i16) -> Vec<u8> {
    const CHANNELS: u16 = 1;
    const BITS: u16 = 16;
    let data_len = samples * u32::from(BITS / 8);
    let block_align = CHANNELS * BITS / 8;

    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&WAV_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(WAV_SAMPLE_RATE * u32::from(block_align)).to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for _ in 0..samples {
        wav.extend_from_slice(&amplitude.to_le_bytes());
    }
    wav
}

/// Asserts two volumes agree within f32 rounding.
#[track_caller]
pub fn assert_volume_eq(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-5,
        "volume {actual} != expected {expected}"
    );
}
