//! Shader loading and compilation for the pipeline.
//!
//! This module handles loading and compiling shader modules:
//! - Main shader: 4 WGSL files concatenated (common, sky, vertex, fragment)
//! - Skybox shader: 2 WGSL files concatenated (sky, skybox)

/// The main shader's WGSL: common, sky, vertex and fragment sources concatenated.
pub fn main_shader_source() -> String {
    [
        include_str!("../../../shaders/common.wgsl"),
        include_str!("../../../shaders/sky.wgsl"),
        include_str!("../../../shaders/vertex.wgsl"),
        include_str!("../../../shaders/fragment.wgsl"),
    ]
    .join("\n\n")
}

/// The skybox shader's WGSL: the shared sky gradient and the skybox sources concatenated.
pub fn skybox_shader_source() -> String {
    [
        include_str!("../../../shaders/sky.wgsl"),
        include_str!("../../../shaders/skybox.wgsl"),
    ]
    .join("\n\n")
}

/// WGSL source of the UI pass shader.
pub fn ui_shader_source() -> &'static str {
    include_str!("../../../shaders/ui.wgsl")
}

/// Load and compile all shader modules.
///
/// Both modules are built from sources embedded at compile time
/// ([`main_shader_source`], [`skybox_shader_source`]).
///
/// # Returns
///
/// A tuple of (main_shader, skybox_shader).
pub fn load_shaders(device: &wgpu::Device) -> (wgpu::ShaderModule, wgpu::ShaderModule) {
    let main_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shader"),
        source: wgpu::ShaderSource::Wgsl(main_shader_source().into()),
    });
    let skybox_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("skybox-shader"),
        source: wgpu::ShaderSource::Wgsl(skybox_shader_source().into()),
    });

    (main_shader, skybox_shader)
}
