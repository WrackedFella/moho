//! Input filtering: a radial deadzone for the stick and loss-free smoothing for the mouse.

/// Radial deadzone: a vector shorter than the threshold reads as zero.
#[derive(Clone, Debug)]
pub struct DeadzoneFilter {
    threshold: f32,
}

impl DeadzoneFilter {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }

    pub fn apply(&self, delta: (f32, f32)) -> (f32, f32) {
        let magnitude = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
        if magnitude < self.threshold {
            (0.0, 0.0)
        } else {
            delta
        }
    }
}

/// Two-sample mean of raw mouse counts: each output averages this sample and the
/// previous one, so the total output equals the total input once a quiet sample has run.
#[derive(Clone, Debug, Default)]
pub struct MouseSmoothing {
    previous: (f32, f32),
}

impl MouseSmoothing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, counts: (f32, f32)) -> (f32, f32) {
        let out = (
            (counts.0 + self.previous.0) * 0.5,
            (counts.1 + self.previous.1) * 0.5,
        );
        self.previous = counts;
        out
    }

    /// Drops the carried sample so a camera jump cannot follow.
    pub fn reset(&mut self) {
        self.previous = (0.0, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn output_is_the_mean_of_this_and_the_previous_sample_on_both_axes() {
        type Samples = [(f32, f32); 3];
        let rows: [(Samples, Samples); 2] = [
            (
                [(10.0, -4.0), (20.0, 8.0), (0.0, 0.0)],
                [(5.0, -2.0), (15.0, 2.0), (10.0, 4.0)],
            ),
            (
                [(1.0, 0.0), (0.0, 1.0), (0.0, 0.0)],
                [(0.5, 0.0), (0.5, 0.5), (0.0, 0.5)],
            ),
        ];

        for (inputs, expected) in rows {
            let mut smoothing = MouseSmoothing::new();

            let outputs = inputs.map(|sample| smoothing.apply(sample));

            assert_eq!(outputs, expected, "inputs {inputs:?}");
        }
    }

    #[test]
    fn a_quiet_sample_does_not_reset_the_carry() {
        let mut smoothing = MouseSmoothing::new();

        smoothing.apply((8.0, 8.0));
        let quiet = smoothing.apply((0.0, 0.0));
        let after = smoothing.apply((0.0, 0.0));

        assert_eq!(quiet, (4.0, 4.0));
        assert_eq!(after, (0.0, 0.0));
    }

    #[test]
    fn reset_discards_the_carry() {
        let mut smoothing = MouseSmoothing::new();
        smoothing.apply((8.0, 8.0));

        smoothing.reset();
        let out = smoothing.apply((0.0, 0.0));

        assert_eq!(out, (0.0, 0.0));
    }

    proptest! {
        /// Whole counts below 2^24 and halves of them add exactly in f32.
        #[test]
        fn prop_smoothing_total_equals_input_after_a_quiet_sample(
            samples in proptest::collection::vec((-2000i32..=2000, -2000i32..=2000), 0..64),
        ) {
            let mut smoothing = MouseSmoothing::new();
            let mut input = (0.0f32, 0.0f32);
            let mut output = (0.0f32, 0.0f32);

            for &(x, y) in &samples {
                let out = smoothing.apply((x as f32, y as f32));
                input = (input.0 + x as f32, input.1 + y as f32);
                output = (output.0 + out.0, output.1 + out.1);
            }
            let flush = smoothing.apply((0.0, 0.0));
            output = (output.0 + flush.0, output.1 + flush.1);

            prop_assert_eq!(output, input);
        }
    }
}
