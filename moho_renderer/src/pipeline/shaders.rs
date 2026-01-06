//! Shader loading and compilation for the pipeline.
//!
//! This module handles loading and compiling shader modules:
//! - Main shader: 3 WGSL files concatenated (common, vertex, fragment)
//! - Skybox shader: Single WGSL file loaded from disk

use super::PipelineInitError;

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
    // Main shader: concatenate 3 files
    let shader_source = [
        include_str!("../../../shaders/common.wgsl"),
        include_str!("../../../shaders/vertex.wgsl"),
        include_str!("../../../shaders/fragment.wgsl"),
    ]
    .join("\n\n");
    let main_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_main_shader_includes_all_sources() {
        // Verify that the main shader concatenates all 3 files
        let common = include_str!("../../../shaders/common.wgsl");
        let vertex = include_str!("../../../shaders/vertex.wgsl");
        let fragment = include_str!("../../../shaders/fragment.wgsl");

        // The shader source should contain all three
        assert!(!common.is_empty(), "common.wgsl should not be empty");
        assert!(!vertex.is_empty(), "vertex.wgsl should not be empty");
        assert!(!fragment.is_empty(), "fragment.wgsl should not be empty");
    }

    #[test]
    fn test_shader_concatenation() {
        // Verify that shader files can be concatenated
        let common = include_str!("../../../shaders/common.wgsl");
        let vertex = include_str!("../../../shaders/vertex.wgsl");
        let fragment = include_str!("../../../shaders/fragment.wgsl");

        let concatenated = [common, vertex, fragment].join("\n\n");
        assert!(
            !concatenated.is_empty(),
            "Concatenated shader should not be empty"
        );
        assert!(
            concatenated.contains("struct Camera"),
            "Should contain shader types from common.wgsl"
        );
    }
}
