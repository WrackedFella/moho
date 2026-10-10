//! Shader loading and compilation for the pipeline.
//!
//! This module handles loading and compiling shader modules:
//! - Main shader: 3 WGSL files concatenated (common, vertex, fragment)
//! - Skybox shader: Single WGSL file loaded from disk

use super::PipelineInitError;

/// The main shader's WGSL: common, vertex and fragment sources concatenated.
pub fn main_shader_source() -> String {
    [
        include_str!("../../../shaders/common.wgsl"),
        include_str!("../../../shaders/vertex.wgsl"),
        include_str!("../../../shaders/fragment.wgsl"),
    ]
    .join("\n\n")
}

/// The UI pass shader's WGSL.
pub fn ui_shader_source() -> &'static str {
    include_str!("../../../shaders/ui.wgsl")
}

/// Load and compile all shader modules.
///
/// The main shader is composed of three WGSL files concatenated together:
/// - common.wgsl (shared types and functions)
/// - vertex.wgsl (vertex shader)
/// - fragment.wgsl (fragment shader)
///
/// The skybox shader is loaded from a single file on disk.
///
/// # Returns
///
/// A tuple of (main_shader, skybox_shader) on success.
///
/// # Errors
///
/// Returns `PipelineInitError::SkyboxShaderLoad` if the skybox shader file cannot be read.
pub fn load_shaders(
    device: &wgpu::Device,
) -> Result<(wgpu::ShaderModule, wgpu::ShaderModule), PipelineInitError> {
    let main_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shader"),
        source: wgpu::ShaderSource::Wgsl(main_shader_source().into()),
    });

    // Skybox shader: load from disk
    let skybox_shader_source = std::fs::read_to_string("shaders/skybox.wgsl")
        .map_err(|e| PipelineInitError::SkyboxShaderLoad(e.to_string()))?;
    let skybox_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("skybox-shader"),
        source: wgpu::ShaderSource::Wgsl(skybox_shader_source.into()),
    });

    Ok((main_shader, skybox_shader))
}
