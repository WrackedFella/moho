//! Pipeline setup and configuration for the renderer.
//!
//! This module handles:
//! - Shader loading and compilation (main shader from 3 files, skybox shader from disk)
//! - Bind group layout creation (camera, shadow, lighting)
//! - Pipeline layout creation (main and skybox)
//! - Render pipeline creation (main with depth/blend, skybox without depth write)
//!
//! # Architecture
//!
//! The `PipelineSetup` struct encapsulates all pipeline-related resources needed
//! by the renderer. It's created during initialization and provides access to:
//! - Compiled shader modules
//! - Bind group layouts for camera, materials, and shadows
//! - Pipeline layouts for main rendering and skybox
//! - Render pipelines for main pass and skybox pass
//!
//! # Module Structure
//!
//! - `shaders`: Shader loading and compilation
//! - `layouts`: Bind group and pipeline layout creation
//! - `mod`: Pipeline orchestration and render pipeline creation
//!
//! # Example
//!
//! ```no_run
//! # use moho_renderer::pipeline::PipelineSetup;
//! # use wgpu::{Device, SurfaceConfiguration};
//! # fn example(device: &Device, config: &SurfaceConfiguration) {
//! let pipeline_setup = PipelineSetup::new(device, config).unwrap();
//! let main_pipeline = pipeline_setup.main_pipeline();
//! let skybox_pipeline = pipeline_setup.skybox_pipeline();
//! # }
//! ```

mod layouts;
mod shaders;

use crate::types::{GpuInstance, Vertex};

/// Error type for pipeline initialization failures.
#[derive(Debug)]
pub enum PipelineInitError {
    /// Failed to load skybox shader from disk
    SkyboxShaderLoad(String),
    /// Camera buffer size calculation failed
    CameraBufferSize(String),
    /// Lighting buffer size calculation failed
    LightingBufferSize(String),
    /// Shadow matrix buffer size calculation failed
    ShadowMatrixSize(String),
    /// CSM matrix buffer size calculation failed
    CsmMatrixSize(String),
}

impl std::fmt::Display for PipelineInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SkyboxShaderLoad(msg) => write!(f, "Failed to load skybox shader: {}", msg),
            Self::CameraBufferSize(msg) => write!(f, "Camera buffer size error: {}", msg),
            Self::LightingBufferSize(msg) => write!(f, "Lighting buffer size error: {}", msg),
            Self::ShadowMatrixSize(msg) => write!(f, "Shadow matrix size error: {}", msg),
            Self::CsmMatrixSize(msg) => write!(f, "CSM matrix size error: {}", msg),
        }
    }
}

impl std::error::Error for PipelineInitError {}

/// Encapsulates all pipeline-related resources for the renderer.
///
/// This struct holds compiled shaders, bind group layouts, pipeline layouts,
/// and render pipelines. It's created once during initialization and provides
/// immutable access to these resources throughout the renderer's lifetime.
pub struct PipelineSetup {
    // Shaders (kept for potential future use - hot reloading, debugging)
    #[allow(dead_code)]
    main_shader: wgpu::ShaderModule,
    #[allow(dead_code)]
    skybox_shader: wgpu::ShaderModule,

    // Bind group layouts
    camera_bgl: wgpu::BindGroupLayout,
    shadow_bgl: wgpu::BindGroupLayout,
    #[allow(dead_code)]
    _shadow_pass_bgl: wgpu::BindGroupLayout, // Legacy, kept for compatibility
    #[allow(dead_code)]
    _csm_pass_bgl: wgpu::BindGroupLayout, // Phase 3: CSM support

    // Pipeline layouts (kept for potential future use - additional pipelines)
    #[allow(dead_code)]
    main_pipeline_layout: wgpu::PipelineLayout,
    #[allow(dead_code)]
    skybox_pipeline_layout: wgpu::PipelineLayout,

    // Render pipelines
    main_pipeline: wgpu::RenderPipeline,
    skybox_pipeline: wgpu::RenderPipeline,

    // Configuration
    depth_format: wgpu::TextureFormat,
}

impl PipelineSetup {
    /// Create a new pipeline setup with all shaders, layouts, and pipelines.
    ///
    /// This performs the complete pipeline initialization:
    /// 1. Load and compile shaders (main: 3 files concatenated, skybox: from disk)
    /// 2. Create bind group layouts (camera with 3 bindings, shadow with 3 bindings)
    /// 3. Create pipeline layouts (main: 2 bind groups, skybox: 1 bind group)
    /// 4. Create render pipelines (main: with depth and blend, skybox: no depth write)
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device to create resources on
    /// * `config` - The surface configuration (provides format)
    ///
    /// # Errors
    ///
    /// Returns `PipelineInitError` if:
    /// - Skybox shader file cannot be read
    /// - Buffer size calculations fail (should never happen with valid types)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use moho_renderer::pipeline::PipelineSetup;
    /// # use wgpu::{Device, SurfaceConfiguration};
    /// # fn example(device: &Device, config: &SurfaceConfiguration) {
    /// let pipeline = PipelineSetup::new(device, config).unwrap();
    /// // Use pipeline.main_pipeline(), pipeline.camera_bind_group_layout(), etc.
    /// # }
    /// ```
    pub fn new(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Result<Self, PipelineInitError> {
        // 1. Load and compile shaders
        let (main_shader, skybox_shader) = shaders::load_shaders(device)?;

        // 2. Create bind group layouts
        let camera_bgl = layouts::create_camera_bind_group_layout(device)?;
        let shadow_bgl = layouts::create_shadow_bind_group_layout(device)?;
        let shadow_pass_bgl = layouts::create_shadow_pass_bind_group_layout(device)?;
        let csm_pass_bgl = layouts::create_csm_pass_bind_group_layout(device)?;

        // 3. Create pipeline layouts
        let main_pipeline_layout =
            layouts::create_main_pipeline_layout(device, &camera_bgl, &shadow_bgl);
        let skybox_pipeline_layout = layouts::create_skybox_pipeline_layout(device, &camera_bgl);

        // 4. Create render pipelines
        let depth_format = wgpu::TextureFormat::Depth24Plus;
        let main_pipeline = Self::create_main_render_pipeline(
            device,
            &main_shader,
            &main_pipeline_layout,
            config.format,
            depth_format,
        );
        let skybox_pipeline = Self::create_skybox_render_pipeline(
            device,
            &skybox_shader,
            &skybox_pipeline_layout,
            config.format,
            depth_format,
        );

        Ok(Self {
            main_shader,
            skybox_shader,
            camera_bgl,
            shadow_bgl,
            _shadow_pass_bgl: shadow_pass_bgl,
            _csm_pass_bgl: csm_pass_bgl,
            main_pipeline_layout,
            skybox_pipeline_layout,
            main_pipeline,
            skybox_pipeline,
            depth_format,
        })
    }

    /// Create the main render pipeline.
    ///
    /// This pipeline:
    /// - Uses vertex and instance buffers (positions, normals, model matrices, materials)
    /// - Enables alpha blending for transparency
    /// - Enables back-face culling
    /// - Writes to depth buffer with Less comparison
    fn create_main_render_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        surface_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render-pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    // Vertex positions + normals + AO + geometry type
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x3,  // position
                            1 => Float32x3,  // normal
                            2 => Float32,    // ao
                            3 => Uint32,     // geometry_type
                        ],
                    },
                    // Per-instance data: model matrix (4x vec4) + material(u32) + object_type(u32)
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<GpuInstance>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            4 => Float32x4,
                            5 => Float32x4,
                            6 => Float32x4,
                            7 => Float32x4,
                            8 => Uint32,
                            9 => Uint32,
                        ],
                    },
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    /// Create the skybox render pipeline.
    ///
    /// This pipeline:
    /// - Uses only vertex buffer (fullscreen quad positions)
    /// - No blending (skybox is opaque background)
    /// - No culling (inside a sphere)
    /// - Doesn't write to depth buffer (renders at far plane)
    fn create_skybox_render_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        layout: &wgpu::PipelineLayout,
        surface_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skybox-pipeline"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None, // No blending for skybox
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None, // No culling for skybox
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                depth_write_enabled: false, // Don't write depth for skybox
                depth_compare: wgpu::CompareFunction::LessEqual, // Render at far plane
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    // Public accessors

    /// Get the main render pipeline.
    pub fn main_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.main_pipeline
    }

    /// Get the skybox render pipeline.
    pub fn skybox_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.skybox_pipeline
    }

    /// Get the camera bind group layout.
    pub fn camera_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.camera_bgl
    }

    /// Get the shadow bind group layout.
    pub fn shadow_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.shadow_bgl
    }

    /// Get the depth texture format.
    pub fn depth_format(&self) -> wgpu::TextureFormat {
        self.depth_format
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_init_error_display() {
        let err = PipelineInitError::SkyboxShaderLoad("file not found".to_string());
        assert_eq!(
            format!("{}", err),
            "Failed to load skybox shader: file not found"
        );

        let err = PipelineInitError::CameraBufferSize("zero size".to_string());
        assert_eq!(format!("{}", err), "Camera buffer size error: zero size");

        let err = PipelineInitError::LightingBufferSize("invalid".to_string());
        assert_eq!(format!("{}", err), "Lighting buffer size error: invalid");

        let err = PipelineInitError::ShadowMatrixSize("bad size".to_string());
        assert_eq!(format!("{}", err), "Shadow matrix size error: bad size");

        let err = PipelineInitError::CsmMatrixSize("csm error".to_string());
        assert_eq!(format!("{}", err), "CSM matrix size error: csm error");
    }

    #[test]
    fn test_pipeline_init_error_is_error() {
        let err = PipelineInitError::SkyboxShaderLoad("test".to_string());
        let _: &dyn std::error::Error = &err;
    }
}
