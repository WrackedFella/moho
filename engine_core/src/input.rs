use std::time::Instant;

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

/// Linear response curve (simplified for Default preset only)
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
        let after_curve = self.sensitivity_curve.apply(after_smoothing);

        after_curve
    }

    pub fn reset(&mut self) {
        self.smoothing_filter.reset();
    }
}

#[derive(Clone, Debug)]
pub struct MouseState {
    pub position: (f64, f64),
    pub delta_accumulator: (f64, f64),
    pub last_update: Instant,
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            position: (0.0, 0.0),
            delta_accumulator: (0.0, 0.0),
            last_update: Instant::now(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InputFrame {
    pub mouse_delta: (f32, f32),
    pub timestamp: Instant,
}

impl Default for InputFrame {
    fn default() -> Self {
        Self {
            mouse_delta: (0.0, 0.0),
            timestamp: Instant::now(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FilterPreset {
    Default,
}

/// Event-frequency collection with frame-frequency processing to eliminate input loss
pub struct InputSystem {
    mouse_state: MouseState,
    current_frame: InputFrame,
    sensitivity: f32,
    filter_pipeline: FilterPipeline,
}

impl InputSystem {
    pub fn new(sensitivity: f32) -> Self {
        Self {
            mouse_state: MouseState::default(),
            current_frame: InputFrame::default(),
            sensitivity,
            filter_pipeline: FilterPipeline::new(),
        }
    }

    pub fn new_with_preset(sensitivity: f32, _preset: FilterPreset) -> Self {
        // Only Default preset is supported
        Self {
            mouse_state: MouseState::default(),
            current_frame: InputFrame::default(),
            sensitivity,
            filter_pipeline: FilterPipeline::new(),
        }
    }

    pub fn set_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity;
    }

    /// Accumulates deltas between frames to prevent input loss
    pub fn collect_mouse_delta(&mut self, delta: (f64, f64)) {
        self.mouse_state.delta_accumulator.0 += delta.0;
        self.mouse_state.delta_accumulator.1 += delta.1;
        self.mouse_state.last_update = Instant::now();
    }

    /// Samples and resets accumulated input for frame processing
    pub fn sample_frame_input(&mut self) -> (f32, f32) {
        let raw_delta = self.mouse_state.delta_accumulator;
        self.mouse_state.delta_accumulator = (0.0, 0.0);

        // If no input accumulated, return zero and reset smoothing
        if raw_delta.0.abs() < f64::EPSILON && raw_delta.1.abs() < f64::EPSILON {
            self.filter_pipeline.reset();
            self.current_frame.mouse_delta = (0.0, 0.0);
            self.current_frame.timestamp = Instant::now();
            return (0.0, 0.0);
        }

        let sensitivity_applied = (
            raw_delta.0 as f32 * self.sensitivity,
            raw_delta.1 as f32 * self.sensitivity,
        );

        let filtered_delta = self.filter_pipeline.apply(sensitivity_applied);

        self.current_frame.mouse_delta = filtered_delta;
        self.current_frame.timestamp = Instant::now();

        filtered_delta
    }

    pub fn has_pending_input(&self) -> bool {
        self.mouse_state.delta_accumulator.0.abs() > f64::EPSILON
            || self.mouse_state.delta_accumulator.1.abs() > f64::EPSILON
    }

    pub fn set_filter_enabled(&mut self, enabled: bool) {
        self.filter_pipeline.set_enabled(enabled);
    }

    pub fn apply_preset(&mut self, _preset: FilterPreset) {
        // Only Default preset is supported
        self.filter_pipeline = FilterPipeline::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_accumulation() {
        let mut input_system = InputSystem::new(1.0);

        // Simulate multiple mouse events between frames
        input_system.collect_mouse_delta((1.0, 0.0));
        input_system.collect_mouse_delta((1.0, 0.0));
        input_system.collect_mouse_delta((0.0, 1.0));

        // Sample should return accumulated delta with filtering applied
        // Exponential smoothing factor 0.8: output = 0.8 * input + 0.2 * previous
        // First frame: previous = 0.0, so output = 0.8 * input
        let (x, y) = input_system.sample_frame_input();
        assert_eq!(x, 1.6); // 0.8 * (1.0 + 1.0)
        assert_eq!(y, 0.8); // 0.8 * (0.0 + 0.0 + 1.0)

        // Should be reset after sampling
        let (x2, y2) = input_system.sample_frame_input();
        assert_eq!(x2, 0.0);
        assert_eq!(y2, 0.0);
    }

    #[test]
    fn test_sensitivity_scaling() {
        let mut input_system = InputSystem::new(2.0);

        input_system.collect_mouse_delta((1.0, 1.0));
        let (x, y) = input_system.sample_frame_input();

        // Input (1.0, 1.0) * sensitivity 2.0 = (2.0, 2.0)
        // After exponential smoothing (factor 0.8): (1.6, 1.6)
        assert_eq!(x, 1.6);
        assert_eq!(y, 1.6);
    }

    #[test]
    fn test_has_pending_input() {
        let mut input_system = InputSystem::new(1.0);

        assert!(!input_system.has_pending_input());

        input_system.collect_mouse_delta((1.0, 0.0));
        assert!(input_system.has_pending_input());

        input_system.sample_frame_input();
        assert!(!input_system.has_pending_input());
    }
}
