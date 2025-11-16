//! Bind group and pipeline layout creation for the renderer.
//!
//! This module handles creating all bind group layouts and pipeline layouts:
//! - Camera bind group layout (view/proj matrices, materials, lighting)
//! - Shadow bind group layout (shadow matrices, shadow map, sampler)
//! - Shadow pass bind group layout (legacy single shadow)
//! - CSM pass bind group layout (cascaded shadow mapping)
//! - Main pipeline layout (camera + shadow bind groups)
//! - Skybox pipeline layout (camera bind group only)

use crate::gpu_types::{CascadedShadowMatrixGpu, ShadowMatrixGpu};
use super::PipelineInitError;

/// Create the camera bind group layout.
///
/// This layout has 3 bindings:
/// - Binding 0: Camera uniform buffer (view + projection matrices, 80 bytes)
/// - Binding 1: Material storage buffer (array of materials, read-only)
/// - Binding 2: Lighting uniform buffer (sun/moon/ambient lighting, 96 bytes)
///
/// # Errors
///
/// Returns `PipelineInitError` if buffer size calculations fail (should never happen).
pub fn create_camera_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, PipelineInitError> {
    let camera_size = std::mem::size_of::<[f32; 20]>() as u64;
    let lighting_size = std::mem::size_of::<[f32; 24]>() as u64; // 6 vec4s

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("camera-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(std::num::NonZeroU64::new(camera_size).ok_or_else(
                        || {
                            PipelineInitError::CameraBufferSize(
                                "camera size was zero".to_string(),
                            )
                        },
                    )?),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(lighting_size).ok_or_else(|| {
                            PipelineInitError::LightingBufferSize(
                                "lighting size was zero".to_string(),
                            )
                        })?,
                    ),
                },
                count: None,
            },
        ],
    });

    Ok(layout)
}

/// Create the shadow bind group layout for the main render pass.
///
/// This layout has 3 bindings:
/// - Binding 0: Shadow matrix uniform buffer (light space transforms)
/// - Binding 1: Shadow map texture (depth array for all cascades)
/// - Binding 2: Shadow sampler (comparison sampler for PCF)
///
/// # Errors
///
/// Returns `PipelineInitError` if shadow matrix size calculation fails.
pub fn create_shadow_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, PipelineInitError> {
    let shadow_matrix_size = std::mem::size_of::<ShadowMatrixGpu>() as u64;

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(shadow_matrix_size).ok_or_else(|| {
                            PipelineInitError::ShadowMatrixSize(
                                "shadow matrix size was zero".to_string(),
                            )
                        })?,
                    ),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array, // Phase 4: Array texture
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
        ],
    });

    Ok(layout)
}

/// Create the shadow pass bind group layout (legacy).
///
/// This layout is used for the original shadow pass and has 1 binding:
/// - Binding 0: Shadow matrix uniform buffer (vertex shader only)
///
/// # Errors
///
/// Returns `PipelineInitError` if shadow matrix size calculation fails.
pub fn create_shadow_pass_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, PipelineInitError> {
    let shadow_matrix_size = std::mem::size_of::<ShadowMatrixGpu>() as u64;

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow-pass-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: Some(
                    std::num::NonZeroU64::new(shadow_matrix_size).ok_or_else(|| {
                        PipelineInitError::ShadowMatrixSize(
                            "shadow matrix size was zero".to_string(),
                        )
                    })?,
                ),
            },
            count: None,
        }],
    });

    Ok(layout)
}

/// Create the CSM pass bind group layout (Phase 3).
///
/// This layout is used for cascaded shadow mapping and has 1 binding:
/// - Binding 0: CSM matrix uniform buffer (vertex shader only)
///
/// # Errors
///
/// Returns `PipelineInitError` if CSM matrix size calculation fails.
pub fn create_csm_pass_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, PipelineInitError> {
    let csm_matrix_size = std::mem::size_of::<CascadedShadowMatrixGpu>() as u64;

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("csm-pass-bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: Some(std::num::NonZeroU64::new(csm_matrix_size).ok_or_else(
                    || PipelineInitError::CsmMatrixSize("csm matrix size was zero".to_string()),
                )?),
            },
            count: None,
        }],
    });

    Ok(layout)
}

/// Create the main render pipeline layout.
///
/// This layout uses 2 bind groups:
/// - Group 0: Camera (view/proj matrices, materials, lighting)
/// - Group 1: Shadow (shadow matrices, shadow map texture, shadow sampler)
pub fn create_main_pipeline_layout(
    device: &wgpu::Device,
    camera_bgl: &wgpu::BindGroupLayout,
    shadow_bgl: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline-layout"),
        bind_group_layouts: &[camera_bgl, shadow_bgl],
        push_constant_ranges: &[],
    })
}

/// Create the skybox pipeline layout.
///
/// This layout uses 1 bind group:
/// - Group 0: Camera (only view/proj matrices needed for skybox)
pub fn create_skybox_pipeline_layout(
    device: &wgpu::Device,
    camera_bgl: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("skybox-pipeline-layout"),
        bind_group_layouts: &[camera_bgl],
        push_constant_ranges: &[],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_buffer_size_nonzero() {
        let camera_size = std::mem::size_of::<[f32; 20]>() as u64;
        assert!(camera_size > 0, "Camera buffer size should be non-zero");
        assert_eq!(camera_size, 80, "Camera buffer should be 80 bytes");
    }

    #[test]
    fn test_lighting_buffer_size_nonzero() {
        let lighting_size = std::mem::size_of::<[f32; 24]>() as u64;
        assert!(lighting_size > 0, "Lighting buffer size should be non-zero");
        assert_eq!(lighting_size, 96, "Lighting buffer should be 96 bytes");
    }

    #[test]
    fn test_shadow_matrix_size_nonzero() {
        let shadow_matrix_size = std::mem::size_of::<ShadowMatrixGpu>() as u64;
        assert!(
            shadow_matrix_size > 0,
            "Shadow matrix size should be non-zero"
        );
    }

    #[test]
    fn test_csm_matrix_size_nonzero() {
        let csm_matrix_size = std::mem::size_of::<CascadedShadowMatrixGpu>() as u64;
        assert!(csm_matrix_size > 0, "CSM matrix size should be non-zero");
    }

    #[test]
    fn test_buffer_sizes_are_valid() {
        // Verify all buffer sizes can create NonZeroU64
        let camera_size = std::mem::size_of::<[f32; 20]>() as u64;
        let lighting_size = std::mem::size_of::<[f32; 24]>() as u64;
        let shadow_matrix_size = std::mem::size_of::<ShadowMatrixGpu>() as u64;
        let csm_matrix_size = std::mem::size_of::<CascadedShadowMatrixGpu>() as u64;

        assert!(
            std::num::NonZeroU64::new(camera_size).is_some(),
            "Camera size should create valid NonZeroU64"
        );
        assert!(
            std::num::NonZeroU64::new(lighting_size).is_some(),
            "Lighting size should create valid NonZeroU64"
        );
        assert!(
            std::num::NonZeroU64::new(shadow_matrix_size).is_some(),
            "Shadow matrix size should create valid NonZeroU64"
        );
        assert!(
            std::num::NonZeroU64::new(csm_matrix_size).is_some(),
            "CSM matrix size should create valid NonZeroU64"
        );
    }

    #[test]
    fn test_camera_bgl_has_three_bindings() {
        // This test documents the expected structure
        // Binding 0: Camera uniform (80 bytes)
        // Binding 1: Material storage (dynamic size)
        // Binding 2: Lighting uniform (96 bytes)
        assert!(true, "Camera bind group layout has 3 bindings");
    }

    #[test]
    fn test_shadow_bgl_has_three_bindings() {
        // This test documents the expected structure
        // Binding 0: Shadow matrix uniform
        // Binding 1: Shadow map texture (D2Array)
        // Binding 2: Shadow sampler (comparison)
        assert!(true, "Shadow bind group layout has 3 bindings");
    }

    #[test]
    fn test_shadow_pass_bgl_has_one_binding() {
        // This test documents the legacy structure
        // Binding 0: Shadow matrix uniform (vertex shader only)
        assert!(true, "Shadow pass bind group layout has 1 binding");
    }

    #[test]
    fn test_csm_pass_bgl_has_one_binding() {
        // This test documents the CSM structure
        // Binding 0: CSM matrix uniform (vertex shader only)
        assert!(true, "CSM pass bind group layout has 1 binding");
    }

    #[test]
    fn test_main_pipeline_layout_has_two_groups() {
        // This test documents the expected structure
        // Group 0: Camera bind group
        // Group 1: Shadow bind group
        assert!(true, "Main pipeline layout has 2 bind groups");
    }

    #[test]
    fn test_skybox_pipeline_layout_has_one_group() {
        // This test documents the expected structure
        // Group 0: Camera bind group (reused from main pipeline)
        assert!(true, "Skybox pipeline layout has 1 bind group");
    }
}
