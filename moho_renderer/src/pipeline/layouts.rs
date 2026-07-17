//! Bind group and pipeline layout creation for the renderer.
//!
//! This module handles creating all bind group layouts and pipeline layouts:
//! - Camera bind group layout (view/proj matrices, materials, lighting)
//! - Shadow bind group layout (shadow matrices, shadow map, sampler)
//! - Shadow pass bind group layout (legacy single shadow)
//! - CSM pass bind group layout (cascaded shadow mapping)
//! - Main pipeline layout (camera + shadow bind groups)
//! - Skybox pipeline layout (camera bind group only)

use super::PipelineInitError;
use crate::gpu_types::{MultiLightShadowGpu, ShadowMatrixGpu};

/// Create the camera bind group layout.
///
/// This layout has 5 bindings:
/// - Binding 0: Camera uniform buffer (view + projection matrices, 80 bytes)
/// - Binding 1: Material storage buffer (array of materials, read-only)
/// - Binding 2: Lighting uniform buffer (sun/moon/ambient lighting, 96 bytes)
/// - Binding 3: SSAO texture (ambient occlusion, R8Unorm)
/// - Binding 4: SSAO sampler (linear filtering)
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
                        || PipelineInitError::CameraBufferSize("camera size was zero".to_string()),
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
                    min_binding_size: Some(std::num::NonZeroU64::new(lighting_size).ok_or_else(
                        || {
                            PipelineInitError::LightingBufferSize(
                                "lighting size was zero".to_string(),
                            )
                        },
                    )?),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None, // Dynamic lights buffer size varies
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
/// - Binding 0: Cascaded shadow matrix uniform buffer (light space transforms for all cascades + split distances)
/// - Binding 1: Shadow map texture (depth array for all cascades)
/// - Binding 2: Shadow sampler (comparison sampler for PCF)
///
/// # Errors
///
/// Create bind group layout for shadow pass (used in main render pipeline).
///
/// This layout has 3 bindings:
/// - Binding 0: Shadow matrices uniform buffer (multi-light shadow data, 288 bytes)
/// - Binding 1: Shadow map texture array (depth texture, one layer per light)
/// - Binding 2: Shadow sampler (comparison sampler for PCF)
///
/// Returns `PipelineInitError` if shadow matrix size calculation fails.
pub fn create_shadow_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, PipelineInitError> {
    let shadow_matrix_size = std::mem::size_of::<MultiLightShadowGpu>() as u64;

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
                    view_dimension: wgpu::TextureViewDimension::D2Array, // Array texture for multiple lights
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
                min_binding_size: Some(std::num::NonZeroU64::new(shadow_matrix_size).ok_or_else(
                    || {
                        PipelineInitError::ShadowMatrixSize(
                            "shadow matrix size was zero".to_string(),
                        )
                    },
                )?),
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
/// Create bind group layout for multi-light shadow pass.
///
/// This layout provides the shadow matrices for all active lights (sun, moon, dynamic lights).
///
/// Returns `PipelineInitError` if shadow matrix size calculation fails.
pub fn create_csm_pass_bind_group_layout(
    device: &wgpu::Device,
) -> Result<wgpu::BindGroupLayout, PipelineInitError> {
    let csm_matrix_size = std::mem::size_of::<MultiLightShadowGpu>() as u64;

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
        bind_group_layouts: &[Some(camera_bgl), Some(shadow_bgl)],
        immediate_size: 0,
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
        bind_group_layouts: &[Some(camera_bgl)],
        immediate_size: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_types::CascadedShadowMatrixGpu;

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
}
