//! Screen-Space Ambient Occlusion (SSAO) system using Ground Truth Ambient Occlusion (GTAO).
//!
//! This module implements GTAO for computing ambient occlusion in screen space.
//! GTAO provides high-quality AO without requiring pre-baked data, making it ideal
//! for dynamically modified terrain.
//!
//! # Algorithm Overview
//!
//! 1. **GTAO Pass**: Samples depth buffer in screen space, computes horizon angles
//!    to determine occlusion. Outputs raw AO values.
//! 2. **Blur Pass**: Applies bilateral blur to reduce noise while preserving edges.
//! 3. **Integration**: Fragment shader samples AO texture and blends based on geometry type:
//!    - Smooth geometry (geometry_type=0): Uses SSAO directly
//!    - Blocky geometry (geometry_type=1): Multiplies vertex AO with SSAO
//!
//! # Resources
//!
//! - Depth texture (sampled from main render pass)
//! - AO texture (output of GTAO pass, Rgba8Unorm format - AO in red channel, rest unused)
//! - Blurred AO texture (output of blur pass)
//! - Quality settings uniform (sample count, radius, intensity)

use wgpu::util::DeviceExt;

/// SSAO quality presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaoQuality {
    /// Off: SSAO disabled
    Off,
    /// Low quality: 4 samples, small radius
    Low,
    /// Medium quality: 8 samples, medium radius
    Medium,
    /// High quality: 16 samples, large radius
    High,
    /// Ultra quality: 32 samples, very large radius
    Ultra,
}

impl SsaoQuality {
    /// Get the number of samples for this quality level
    pub fn sample_count(self) -> u32 {
        match self {
            SsaoQuality::Off => 0,
            SsaoQuality::Low => 4,
            SsaoQuality::Medium => 8,
            SsaoQuality::High => 16,
            SsaoQuality::Ultra => 32,
        }
    }

    /// Get the sampling radius for this quality level (in pixels)
    pub fn radius(self) -> f32 {
        match self {
            SsaoQuality::Off => 0.0,
            SsaoQuality::Low => 8.0,
            SsaoQuality::Medium => 12.0,
            SsaoQuality::High => 16.0,
            SsaoQuality::Ultra => 24.0,
        }
    }
}

/// SSAO settings and uniforms
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SsaoSettings {
    /// Number of samples per pixel
    pub sample_count: u32,
    /// Sampling radius in pixels
    pub radius: f32,
    /// AO intensity multiplier (0.0 = no AO, 1.0 = full strength)
    pub intensity: f32,
    /// Bias to prevent self-occlusion artifacts
    pub bias: f32,
}

impl Default for SsaoSettings {
    fn default() -> Self {
        Self {
            sample_count: 8,
            radius: 12.0,
            intensity: 1.0,
            bias: 0.025,
        }
    }
}

impl SsaoSettings {
    /// Create settings from a quality preset
    pub fn from_quality(quality: SsaoQuality) -> Self {
        Self {
            sample_count: quality.sample_count(),
            radius: quality.radius(),
            ..Default::default()
        }
    }
}

/// SSAO system managing textures, bind groups, and compute passes
#[derive(Debug)]
pub struct SsaoSystem {
    /// Raw AO texture (output of GTAO pass)
    ao_texture: wgpu::Texture,
    ao_texture_view: wgpu::TextureView,

    /// Blurred AO texture (output of blur pass)
    blurred_ao_texture: wgpu::Texture,
    blurred_ao_texture_view: wgpu::TextureView,

    /// Settings uniform buffer
    settings_buffer: wgpu::Buffer,

    /// Camera uniform buffer (inv_proj matrix for depth reconstruction)
    camera_buffer: wgpu::Buffer,

    /// Current settings
    settings: SsaoSettings,

    /// GTAO compute pipeline
    gtao_pipeline: wgpu::ComputePipeline,
    gtao_bind_group_layout: wgpu::BindGroupLayout,

    /// Blur compute pipeline
    blur_pipeline: wgpu::ComputePipeline,
    blur_bind_group_layout: wgpu::BindGroupLayout,

    /// AO texture sampler
    pub(crate) ao_sampler: wgpu::Sampler,

    /// Surface dimensions (for recreating textures on resize)
    width: u32,
    height: u32,
}

impl SsaoSystem {
    /// Create a new SSAO system
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device
    /// * `width` - Surface width
    /// * `height` - Surface height
    /// * `settings` - Initial SSAO settings
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        settings: SsaoSettings,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (ao_texture, ao_texture_view, blurred_ao_texture, blurred_ao_texture_view) =
            Self::create_textures(device, width, height);
        let (settings_buffer, camera_buffer) =
            Self::create_uniform_buffers(device, &settings);
        let ao_sampler = Self::create_sampler(device);
        let (gtao_bind_group_layout, gtao_pipeline) = Self::create_gtao_pipeline(device)?;
        let (blur_bind_group_layout, blur_pipeline) = Self::create_blur_pipeline(device)?;

        Ok(Self {
            ao_texture,
            ao_texture_view,
            blurred_ao_texture,
            blurred_ao_texture_view,
            settings_buffer,
            camera_buffer,
            settings,
            gtao_pipeline,
            gtao_bind_group_layout,
            blur_pipeline,
            blur_bind_group_layout,
            ao_sampler,
            width,
            height,
        })
    }

    /// Stage 1: Allocate the raw AO texture and the blurred AO texture.
    fn create_textures(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::Texture, wgpu::TextureView) {
        let (ao_texture, ao_texture_view) =
            Self::create_ao_texture(device, width, height, "ao-texture");
        let (blurred_ao_texture, blurred_ao_texture_view) =
            Self::create_ao_texture(device, width, height, "blurred-ao-texture");
        (ao_texture, ao_texture_view, blurred_ao_texture, blurred_ao_texture_view)
    }

    /// Stage 2: Allocate the settings uniform buffer (pre-filled) and the per-frame
    /// camera buffer (inv_proj, written each frame by `update_camera`).
    fn create_uniform_buffers(
        device: &wgpu::Device,
        settings: &SsaoSettings,
    ) -> (wgpu::Buffer, wgpu::Buffer) {
        let settings_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ssao-settings-buffer"),
            contents: bytemuck::cast_slice(&[*settings]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // 64 bytes = one mat4x4; written each frame with the inverse projection matrix
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ssao-camera-buffer"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        (settings_buffer, camera_buffer)
    }

    /// Stage 3: Create the linear sampler used for AO texture reads in the blur pass.
    fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ao slovenije"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        })
    }

    /// Stage 4: Load the GTAO compute shader and build its bind group layout + pipeline.
    fn create_gtao_pipeline(
        device: &wgpu::Device,
    ) -> Result<(wgpu::BindGroupLayout, wgpu::ComputePipeline), Box<dyn std::error::Error>> {
        let gtao_shader_source = std::fs::read_to_string("shaders/gtao.wgsl")
            .map_err(|e| format!("Failed to read GTAO shader: {}", e))?;
        let gtao_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gtao-shader"),
            source: wgpu::ShaderSource::Wgsl(gtao_shader_source.into()),
        });

        let gtao_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gtao-bind-group-layout"),
                entries: &[
                    // @binding(0): depth_texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // @binding(1): depth_sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // @binding(2): ao_output (storage texture)
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                    // @binding(3): settings uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // @binding(4): camera uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let gtao_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gtao-pipeline-layout"),
                bind_group_layouts: &[&gtao_bind_group_layout],
                push_constant_ranges: &[],
            });

        let gtao_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gtao-pipeline"),
            layout: Some(&gtao_pipeline_layout),
            module: &gtao_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok((gtao_bind_group_layout, gtao_pipeline))
    }

    /// Stage 5: Load the bilateral blur compute shader and build its bind group layout + pipeline.
    fn create_blur_pipeline(
        device: &wgpu::Device,
    ) -> Result<(wgpu::BindGroupLayout, wgpu::ComputePipeline), Box<dyn std::error::Error>> {
        let blur_shader_source = std::fs::read_to_string("shaders/ssao_blur.wgsl")
            .map_err(|e| format!("Failed to read blur shader: {}", e))?;
        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ssao-blur-shader"),
            source: wgpu::ShaderSource::Wgsl(blur_shader_source.into()),
        });

        let blur_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blur-bind-group-layout"),
                entries: &[
                    // @binding(0): input_texture (raw AO)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // @binding(1): input_sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // @binding(2): depth_texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    // @binding(3): depth_sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // @binding(4): output_texture (blurred AO)
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            });

        let blur_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blur-pipeline-layout"),
                bind_group_layouts: &[&blur_bind_group_layout],
                push_constant_ranges: &[],
            });

        let blur_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("ssao-blur-pipeline"),
            layout: Some(&blur_pipeline_layout),
            module: &blur_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok((blur_bind_group_layout, blur_pipeline))
    }

    /// Create an AO texture (Rgba8Unorm format - AO in red channel)
    fn create_ao_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Get the blurred AO texture view (for sampling in fragment shader)
    pub fn blurred_ao_view(&self) -> &wgpu::TextureView {
        &self.blurred_ao_texture_view
    }

    /// Update SSAO settings
    pub fn update_settings(&mut self, queue: &wgpu::Queue, settings: SsaoSettings) {
        self.settings = settings;
        queue.write_buffer(&self.settings_buffer, 0, bytemuck::cast_slice(&[settings]));
    }

    /// Set SSAO quality level
    pub fn set_quality(&mut self, queue: &wgpu::Queue, quality: SsaoQuality) {
        self.settings.sample_count = quality.sample_count();
        self.settings.radius = quality.radius();
        queue.write_buffer(
            &self.settings_buffer,
            0,
            bytemuck::cast_slice(&[self.settings]),
        );
    }

    /// Update camera matrices for SSAO computation
    ///
    /// # Arguments
    /// * `queue` - WGPU queue for buffer writes
    /// * `inv_proj` - Inverse projection matrix (for depth reconstruction)
    pub fn update_camera(&self, queue: &wgpu::Queue, inv_proj: &[[f32; 4]; 4]) {
        // Pack inverse projection matrix (64 bytes)
        let mut camera_data = Vec::with_capacity(64);
        for row in inv_proj {
            camera_data.extend_from_slice(bytemuck::cast_slice(row));
        }
        queue.write_buffer(&self.camera_buffer, 0, &camera_data);
    }

    /// Resize textures when surface size changes
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;

        // Recreate AO textures
        (self.ao_texture, self.ao_texture_view) =
            Self::create_ao_texture(device, width, height, "ao-texture");
        (self.blurred_ao_texture, self.blurred_ao_texture_view) =
            Self::create_ao_texture(device, width, height, "blurred-ao-texture");
    }

    /// Run SSAO compute passes
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device
    /// * `encoder` - Command encoder to record compute passes
    /// * `depth_view` - Depth texture view from main render pass
    /// * `depth_sampler` - Depth texture sampler
    /// * `camera_buffer` - Camera uniform buffer
    pub fn compute_ao(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
        depth_sampler: &wgpu::Sampler,
    ) {
        // Create GTAO bind group
        let gtao_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gtao-bind-group"),
            layout: &self.gtao_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(depth_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.ao_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.settings_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.camera_buffer.as_entire_binding(),
                },
            ],
        });

        // GTAO compute pass
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("gtao-compute-pass"),
                timestamp_writes: None,
            });

            compute_pass.set_pipeline(&self.gtao_pipeline);
            compute_pass.set_bind_group(0, &gtao_bind_group, &[]);

            // Dispatch with 8x8 workgroups
            let workgroup_count_x = self.width.div_ceil(8);
            let workgroup_count_y = self.height.div_ceil(8);
            compute_pass.dispatch_workgroups(workgroup_count_x, workgroup_count_y, 1);
        }

        // Create blur bind group
        let blur_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur-bind-group"),
            layout: &self.blur_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.ao_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.ao_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(depth_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.blurred_ao_texture_view),
                },
            ],
        });

        // Blur compute pass
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("ssao-blur-compute-pass"),
                timestamp_writes: None,
            });

            compute_pass.set_pipeline(&self.blur_pipeline);
            compute_pass.set_bind_group(0, &blur_bind_group, &[]);

            // Dispatch with 8x8 workgroups
            let workgroup_count_x = self.width.div_ceil(8);
            let workgroup_count_y = self.height.div_ceil(8);
            compute_pass.dispatch_workgroups(workgroup_count_x, workgroup_count_y, 1);
        }
    }
}
