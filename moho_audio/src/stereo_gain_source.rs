//! A rodio source that pans its input between two output channels.

use crate::spatial::StereoGain;
use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::time::Duration;

/// Averages its input's channels to mono and emits stereo frames scaled by a [`StereoGain`].
pub struct StereoGainSource<I> {
    input: I,
    gain: StereoGain,
}

impl<I: Source> StereoGainSource<I> {
    pub fn new(input: I, gain: StereoGain) -> Self {
        Self { input, gain }
    }
}

impl<I: Source> Iterator for StereoGainSource<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let _ = (&mut self.input, self.gain);
        todo!()
    }
}

impl<I: Source> Source for StereoGainSource<I> {
    fn current_span_len(&self) -> Option<usize> {
        todo!()
    }

    fn channels(&self) -> ChannelCount {
        todo!()
    }

    fn sample_rate(&self) -> SampleRate {
        todo!()
    }

    fn total_duration(&self) -> Option<Duration> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::num::NonZero;

    fn buffer(channels: u16, samples: Vec<f32>) -> SamplesBuffer {
        SamplesBuffer::new(
            NonZero::new(channels).expect("non-zero channels"),
            NonZero::new(44_100).expect("non-zero rate"),
            samples,
        )
    }

    fn gain(left: f32, right: f32) -> StereoGain {
        StereoGain { left, right }
    }

    #[track_caller]
    fn assert_samples(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-6, "{actual:?} != expected {expected:?}");
        }
    }

    #[test]
    fn gains_reach_their_channels() {
        let input = buffer(1, vec![0.5, -0.5, 1.0]);
        let source = StereoGainSource::new(input, gain(0.2, 0.8));

        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.sample_rate().get(), 44_100);
        assert_eq!(source.current_span_len(), Some(6));
        let out: Vec<f32> = source.collect();
        assert_samples(&out, &[0.1, 0.4, -0.1, -0.4, 0.2, 0.8]);
    }

    #[test]
    fn stereo_input_is_averaged_to_mono() {
        let input = buffer(2, vec![0.2, 0.6, 1.0, -1.0]);
        let source = StereoGainSource::new(input, gain(0.5, 1.0));

        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.current_span_len(), Some(4));
        let out: Vec<f32> = source.collect();
        assert_samples(&out, &[0.2, 0.4, 0.0, 0.0]);
    }

    #[test]
    fn ends_when_input_ends() {
        let input = buffer(1, vec![0.5, 0.5]);
        let mut source = StereoGainSource::new(input, gain(1.0, 1.0));

        let frames: Vec<Option<f32>> = (0..6).map(|_| source.next()).collect();

        assert_eq!(&frames[..4], &[Some(0.5); 4]);
        assert_eq!(&frames[4..], &[None, None]);
    }
}
