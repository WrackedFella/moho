//! Mouse-delta filtering: deadzone, exponential smoothing and a response curve.

const DEADZONE_DEFAULT: f32 = 0.01;
const SMOOTHING_DEFAULT: f32 = 0.8;

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

#[derive(Clone, Debug)]
pub struct ExponentialFilter {
    smoothing_factor: f32,
    previous_output: (f32, f32),
}

impl ExponentialFilter {
    pub fn new(smoothing_factor: f32) -> Self {
        Self {
            smoothing_factor: smoothing_factor.clamp(0.0, 1.0),
            previous_output: (0.0, 0.0),
        }
    }

    pub fn apply(&mut self, input: (f32, f32)) -> (f32, f32) {
        let output = (
            self.smoothing_factor * input.0
                + (1.0 - self.smoothing_factor) * self.previous_output.0,
            self.smoothing_factor * input.1
                + (1.0 - self.smoothing_factor) * self.previous_output.1,
        );
        self.previous_output = output;
        output
    }

    pub fn reset(&mut self) {
        self.previous_output = (0.0, 0.0);
    }
}

/// Linear response curve.
#[derive(Clone, Debug)]
pub struct SensitivityCurve {
    curve_type: CurveType,
}

#[derive(Clone, Debug)]
pub enum CurveType {
    Linear,
}

impl SensitivityCurve {
    pub fn linear() -> Self {
        Self {
            curve_type: CurveType::Linear,
        }
    }

    pub fn apply(&self, input: (f32, f32)) -> (f32, f32) {
        match self.curve_type {
            CurveType::Linear => input,
        }
    }
}

/// Three-stage filtering pipeline: deadzone → smoothing → curves
#[derive(Clone, Debug)]
pub struct FilterPipeline {
    deadzone_filter: DeadzoneFilter,
    smoothing_filter: ExponentialFilter,
    sensitivity_curve: SensitivityCurve,
    enabled: bool,
}

impl FilterPipeline {
    pub fn new() -> Self {
        Self {
            deadzone_filter: DeadzoneFilter::new(DEADZONE_DEFAULT),
            smoothing_filter: ExponentialFilter::new(SMOOTHING_DEFAULT),
            sensitivity_curve: SensitivityCurve::linear(),
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.smoothing_filter.reset();
        }
    }

    pub fn apply(&mut self, input: (f32, f32)) -> (f32, f32) {
        if !self.enabled {
            return input;
        }

        let after_deadzone = self.deadzone_filter.apply(input);
        let after_smoothing = self.smoothing_filter.apply(after_deadzone);
        self.sensitivity_curve.apply(after_smoothing)
    }

    pub fn reset(&mut self) {
        self.smoothing_filter.reset();
    }
}

impl Default for FilterPipeline {
    fn default() -> Self {
        Self::new()
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

    pub fn apply(&mut self, _counts: (f32, f32)) -> (f32, f32) {
        todo!()
    }

    pub fn reset(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn output_is_the_mean_of_this_and_the_previous_sample_on_both_axes() {
        let rows: [([(f32, f32); 3], [(f32, f32); 3]); 2] = [
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
