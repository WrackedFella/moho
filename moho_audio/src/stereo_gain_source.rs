//! A rodio source that pans its input between two output channels.

use crate::spatial::StereoGain;
use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::time::Duration;

/// Every frame this source emits is left then right.
const STEREO: ChannelCount = ChannelCount::new(2).expect("2 is non-zero");

/// Averages its input's channels to mono and emits stereo frames scaled by a [`StereoGain`].
pub struct StereoGainSource<I> {
    input: I,
    gain: StereoGain,
    /// The right-ear half of the frame whose left half was just emitted.
    pending_right: Option<Sample>,
}

impl<I: Source> StereoGainSource<I> {
    pub fn new(input: I, gain: StereoGain) -> Self {
        Self {
            input,
            gain,
            pending_right: None,
        }
    }
}

impl<I: Source> Iterator for StereoGainSource<I> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(right) = self.pending_right.take() {
            return Some(right);
        }

        let channels = self.input.channels().get();
        let mut sum = 0.0;
        let mut count = 0u16;
        while count < channels {
            let Some(sample) = self.input.next() else {
                break;
            };
            sum += sample;
            count += 1;
        }
        if count == 0 {
            return None;
        }

        let mono = sum / f32::from(count);
        self.pending_right = Some(mono * self.gain.right);
        Some(mono * self.gain.left)
    }
}

impl<I: Source> Source for StereoGainSource<I> {
    fn current_span_len(&self) -> Option<usize> {
        let frames = self.input.current_span_len()? / usize::from(self.input.channels().get());
        Some(frames * usize::from(STEREO.get()) + usize::from(self.pending_right.is_some()))
    }

    fn channels(&self) -> ChannelCount {
        STEREO
    }

    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.input.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::num::NonZero;

    fn buffer_at(channels: u16, rate: u32, samples: Vec<f32>) -> SamplesBuffer {
        SamplesBuffer::new(
            NonZero::new(channels).expect("non-zero channels"),
            NonZero::new(rate).expect("non-zero rate"),
            samples,
        )
    }

    fn buffer(channels: u16, samples: Vec<f32>) -> SamplesBuffer {
        buffer_at(channels, 44_100, samples)
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

        let expected = [0.1, 0.4, -0.1, -0.4, 0.2, 0.8];
        let out: Vec<f32> = source.take(expected.len() + 1).collect();
        assert_samples(&out, &expected);
    }

    #[test]
    fn reports_stereo_format_of_its_input_rate() {
        let input = buffer_at(1, 48_000, vec![0.5, -0.5, 1.0]);
        let source = StereoGainSource::new(input, gain(0.2, 0.8));

        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.sample_rate().get(), 48_000);
        assert_eq!(source.current_span_len(), Some(6));
    }

    #[test]
    fn total_duration_is_the_inputs_duration() {
        let input = buffer_at(1, 48_000, vec![0.5, -0.5, 1.0]);
        let source = StereoGainSource::new(input, gain(0.2, 0.8));

        assert_eq!(source.total_duration(), Some(Duration::from_nanos(62_500)));
    }

    #[test]
    fn stereo_input_is_averaged_to_mono() {
        let input = buffer(2, vec![0.2, 0.6, 1.0, -1.0]);
        let source = StereoGainSource::new(input, gain(0.5, 1.0));

        assert_eq!(source.channels().get(), 2);
        assert_eq!(source.current_span_len(), Some(4));
        let expected = [0.2, 0.4, 0.0, 0.0];
        let out: Vec<f32> = source.take(expected.len() + 1).collect();
        assert_samples(&out, &expected);
    }

    #[test]
    fn ends_when_input_ends() {
        let input = buffer(1, vec![0.5, 0.5]);
        let mut source = StereoGainSource::new(input, gain(1.0, 1.0));

        let frames: Vec<Option<f32>> = (0..6).map(|_| source.next()).collect();

        assert_eq!(&frames[..4], &[Some(0.5); 4]);
        assert_eq!(&frames[4..], &[None, None]);
    }

    #[test]
    fn is_not_exhausted_while_a_sample_remains() {
        let input = buffer(1, vec![0.5, 0.5]);
        let mut source = StereoGainSource::new(input, gain(1.0, 1.0));

        let first_three: Vec<Option<f32>> = (0..3).map(|_| source.next()).collect();

        assert_eq!(first_three, vec![Some(0.5); 3]);
        assert!(
            !source.is_exhausted(),
            "one sample remains, span len {:?}",
            source.current_span_len()
        );
        assert_eq!(source.next(), Some(0.5));
        assert_eq!(source.next(), None);
    }
}
