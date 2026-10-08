//! Mouse-delta filtering: deadzone, exponential smoothing and a response curve.

/// Default filter configuration constants
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
