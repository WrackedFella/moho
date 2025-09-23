use engine_core::actors::InstanceGpu;

/// Minimal placeholder renderer used for builds and tests.
pub fn run(instances: Vec<InstanceGpu>, _camera: (glam::Mat4, glam::Mat4)) {
    println!("Placeholder renderer received {} instances", instances.len());
    // Intentionally does not create a window; real renderer to be implemented incrementally.
}
