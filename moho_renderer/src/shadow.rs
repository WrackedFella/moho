use wgpu::util::DeviceExt;

use crate::gpu_types::MAX_SHADOW_LIGHTS;

/// Number of cascaded shadow map cascades (legacy - kept for compatibility)
/// Reduced to 2 for performance (50% fewer shadow passes and memory)
pub const NUM_SHADOW_CASCADES: u32 = 2;

/// Shadow map resolution per cascade/light
pub const SHADOW_MAP_SIZE: u32 = 4096;

/// Cascade split distances from camera (in world units)
pub const CASCADE_SPLIT_DISTANCES: [f32; 2] = [400.0, 1500.0];

/// Shadow distance for multi-light system
pub const SHADOW_DISTANCE: f32 = 1000.0;

#[allow(dead_code)]
const CSM_DEBUG_MODE: bool = false;

const CSM_VERBOSE_LOGGING: bool = false;

/// PCSS (Percentage Closer Soft Shadows) quality levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcssQuality {
    /// Disabled: Use fixed 3x3 PCF (best performance)
    Off = 0,
    /// Low: 8 blocker samples + 16 PCF samples
    Low = 1,
    /// Medium: 12 blocker samples + 24 PCF samples
    Medium = 2,
    /// High: 16 blocker samples + 32 PCF samples
    High = 3,
}

impl PcssQuality {
    /// Get blocker search sample count for this quality level
    pub fn blocker_samples(self) -> u32 {
        match self {
            PcssQuality::Off => 0,
            PcssQuality::Low => 8,
            PcssQuality::Medium => 12,
            PcssQuality::High => 16,
        }
    }

    /// Get PCF sample count for this quality level
    pub fn pcf_samples(self) -> u32 {
        match self {
            PcssQuality::Off => 9,  // 3x3 fixed kernel
            PcssQuality::Low => 16,
            PcssQuality::Medium => 24,
            PcssQuality::High => 32,
        }
    }
}

/// PCSS settings for configuring shadow softness
#[derive(Debug, Clone, Copy)]
pub struct PcssSettings {
    /// Quality level
    pub quality: PcssQuality,
    /// Angular size of light source (affects penumbra width)
    /// Typical range: 0.01-0.1 (0.03 is a good default for sun)
    pub light_size: f32,
    /// Search radius for blocker search (in shadow map texels)
    pub search_radius: f32,
    /// Minimum penumbra size (prevents aliasing)
    pub min_penumbra: f32,
    /// Maximum penumbra size (performance limit)
    pub max_penumbra: f32,
}

impl Default for PcssSettings {
    fn default() -> Self {
        Self {
            quality: PcssQuality::Off,  // Temporarily disabled for testing
            light_size: 0.03,          // Clear day default
            search_radius: 15.0,       // Search area in texels
            min_penumbra: 1.0,         // At least 1 texel
            max_penumbra: 32.0,        // Max 32 texel radius
        }
    }
}

impl PcssSettings {
    /// Create settings for a specific quality level with default parameters
    pub fn from_quality(quality: PcssQuality) -> Self {
        Self {
            quality,
            ..Default::default()
        }
    }

    /// Create settings for specific weather conditions
    pub fn for_weather(weather: &str) -> Self {
        match weather {
            "clear" => Self {
                quality: PcssQuality::High,
                light_size: 0.02,
                ..Default::default()
            },
            "cloudy" => Self {
                quality: PcssQuality::Medium,
                light_size: 0.05,
                ..Default::default()
            },
            "overcast" => Self {
                quality: PcssQuality::Medium,
                light_size: 0.08,
                ..Default::default()
            },
            "rain" | "storm" => Self {
                quality: PcssQuality::Low,
                light_size: 0.12,
                ..Default::default()
            },
            _ => Default::default(),
        }
    }
}

/// Light type for shadow system
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightType {
    Sun,
    Moon,
    Dynamic,
}

/// Active shadow-casting light
#[derive(Debug, Clone)]
pub struct ActiveShadowLight {
    pub light_type: LightType,
    pub light_index: u32, // 0-3 for shader array index
    pub matrix: glam::Mat4,
    pub intensity: f32,
}

/// Shadow system resources and state
pub struct ShadowSystem {
    pub shadow_pipeline: wgpu::RenderPipeline,
    pub shadow_matrix_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    pub shadow_map_view: wgpu::TextureView,
    #[allow(dead_code)]
    pub shadow_pass_bind_group: wgpu::BindGroup,
    pub csm_matrix_buffer: wgpu::Buffer,
    pub csm_cascade_views: Vec<wgpu::TextureView>,
    pub csm_pass_bind_group: wgpu::BindGroup,
    pub csm_shadow_bind_group: wgpu::BindGroup,
    pub current_lighting: crate::gpu_types::LightingGpu,
    /// Active shadow-casting lights for current frame
    pub active_lights: Vec<ActiveShadowLight>,
    /// PCSS (Percentage Closer Soft Shadows) settings
    pub pcss_settings: PcssSettings,
    csm_logged_once: std::cell::Cell<bool>,
}

impl ShadowSystem {
    pub fn new(
        device: &wgpu::Device,
        _camera_bind_group_layout: &wgpu::BindGroupLayout,
        shadow_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Legacy single shadow map
        let shadow_map_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow-map-texture"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let shadow_map_view =
            shadow_map_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Multi-light shadow map array (one layer per light)
        let csm_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("multi-light-shadow-array"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: MAX_SHADOW_LIGHTS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        if CSM_VERBOSE_LOGGING {
            log::info!(
                "Created multi-light shadow array: {}x{} x {} lights",
                SHADOW_MAP_SIZE,
                SHADOW_MAP_SIZE,
                MAX_SHADOW_LIGHTS
            );
        }

        // Create individual views for each light layer
        let csm_cascade_views: Vec<wgpu::TextureView> = (0..MAX_SHADOW_LIGHTS as u32)
            .map(|i| {
                csm_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(&format!("shadow-light-{}-view", i)),
                    format: Some(wgpu::TextureFormat::Depth32Float),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    aspect: wgpu::TextureAspect::DepthOnly,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: i,
                    array_layer_count: Some(1),
                    usage: Some(
                        wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                    ),
                })
            })
            .collect();

        if CSM_VERBOSE_LOGGING {
            log::info!(
                "Created {} light views for shadow rendering",
                csm_cascade_views.len()
            );
        }

        let csm_array_view = csm_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("multi-light-shadow-array-view"),
            format: Some(wgpu::TextureFormat::Depth32Float),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: Some(MAX_SHADOW_LIGHTS as u32),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
        });

        if CSM_VERBOSE_LOGGING {
            log::info!(
                "Created multi-light shadow array view: {} layers",
                MAX_SHADOW_LIGHTS
            );
        }

        use crate::gpu_types::ShadowMatrixGpu;
        let initial_shadow_matrix = ShadowMatrixGpu::default();
        let shadow_matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shadow-matrix-buffer"),
            contents: bytemuck::bytes_of(&initial_shadow_matrix),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        use crate::gpu_types::MultiLightShadowGpu;
        let initial_csm_matrix = MultiLightShadowGpu::default();
        let csm_matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("multi-light-shadow-buffer"),
            contents: bytemuck::bytes_of(&initial_csm_matrix),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        if CSM_VERBOSE_LOGGING {
            log::info!(
                "Created multi-light shadow buffer: {} bytes",
                std::mem::size_of::<MultiLightShadowGpu>()
            );
        }

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        // Bind group layouts
        let shadow_pass_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shadow-pass-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let csm_pass_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("csm-pass-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let shadow_pass_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-pass-bind-group"),
            layout: &shadow_pass_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: shadow_matrix_buffer.as_entire_binding(),
            }],
        });

        let csm_pass_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("csm-pass-bind-group"),
            layout: &csm_pass_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: csm_matrix_buffer.as_entire_binding(),
            }],
        });

        let csm_shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("csm-shadow-bind-group"),
            layout: shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: csm_matrix_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&csm_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });

        if CSM_VERBOSE_LOGGING {
            log::info!("Created CSM bind groups: pass (rendering) + shadow (sampling cascade 0)");
        }

        let shadow_shader_source = std::fs::read_to_string("shaders/shadow.wgsl")
            .map_err(|e| format!("Failed to read shadow shader: {}", e))?;
        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow-shader"),
            source: wgpu::ShaderSource::Wgsl(shadow_shader_source.into()),
        });

        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow-pipeline-layout"),
                bind_group_layouts: &[&csm_pass_bind_group_layout],
                push_constant_ranges: &[wgpu::PushConstantRange {
                    stages: wgpu::ShaderStages::VERTEX,
                    range: 0..4,
                }],
            });

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<crate::types::Vertex>()
                            as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<crate::types::GpuInstance>()
                            as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            2 => Float32x4,
                            3 => Float32x4,
                            4 => Float32x4,
                            5 => Float32x4,
                            6 => Uint32,
                            7 => Uint32,
                        ],
                    },
                ],
            },
            fragment: None,
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
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            shadow_pipeline,
            shadow_matrix_buffer,
            shadow_map_view,
            shadow_pass_bind_group,
            csm_matrix_buffer,
            csm_cascade_views,
            csm_pass_bind_group,
            csm_shadow_bind_group,
            current_lighting: crate::gpu_types::LightingGpu::default(),
            active_lights: Vec::new(),
            pcss_settings: PcssSettings::default(),
            csm_logged_once: std::cell::Cell::new(false),
        })
    }

    /// Calculate shadow matrix for legacy single-cascade shadow mapping
    /// This creates a view-projection matrix from the sun's perspective
    #[allow(dead_code)]
    pub fn calculate_shadow_matrix(&self, sun_dir: glam::Vec3, cam_pos: glam::Vec3) -> glam::Mat4 {
        let light_dir = sun_dir.normalize();
        let scene_center = cam_pos;
        let light_distance = 300.0;
        let light_pos = scene_center - light_dir * light_distance;

        let light_view = glam::Mat4::look_at_rh(light_pos, scene_center, glam::Vec3::Y);
        let ortho_size = 100.0;
        let near = 1.0;
        let far = 400.0;

        let light_proj = glam::Mat4::orthographic_rh(
            -ortho_size,
            ortho_size,
            -ortho_size,
            ortho_size,
            near,
            far,
        );

        light_proj * light_view
    }

    /// Calculate cascade frustum bounds in view space
    fn calculate_cascade_splits(&self) -> [(f32, f32); NUM_SHADOW_CASCADES as usize] {
        let mut splits = [(0.0f32, 0.0f32); NUM_SHADOW_CASCADES as usize];
        splits[0] = (0.1, CASCADE_SPLIT_DISTANCES[0]);

        for i in 1..NUM_SHADOW_CASCADES as usize {
            splits[i] = (CASCADE_SPLIT_DISTANCES[i - 1], CASCADE_SPLIT_DISTANCES[i]);
        }

        if !self.csm_logged_once.get() {
            log::info!("CSM cascade splits:");
            for (i, (near, far)) in splits.iter().enumerate() {
                log::info!("  Cascade {}: {:.1} -> {:.1} units", i, near, far);
            }
        }

        splits
    }

    /// Calculate tight orthographic projection for a specific cascade
    fn calculate_cascade_matrix(
        &self,
        cascade_idx: u32,
        light_dir: glam::Vec3,
        cam_pos: glam::Vec3,
        _near: f32,
        far: f32,
    ) -> glam::Mat4 {
        let cascade_center = cam_pos;
        let light_distance = far * 2.0;
        let light_pos = cascade_center - light_dir * light_distance;

        let light_view = glam::Mat4::look_at_rh(light_pos, cascade_center, glam::Vec3::Y);
        let cascade_radius = far * 1.5;

        let light_proj = glam::Mat4::orthographic_rh(
            -cascade_radius,
            cascade_radius,
            -cascade_radius,
            cascade_radius,
            1.0,
            light_distance + far,
        );

        if !self.csm_logged_once.get() {
            log::info!(
                "  Cascade {} matrix: center={:?}, radius={:.1}, light_dist={:.1}",
                cascade_idx,
                cascade_center,
                cascade_radius,
                light_distance
            );
        }

        light_proj * light_view
    }

    /// Calculate all cascade matrices
    pub fn calculate_cascade_matrices(
        &self,
        sun_dir: glam::Vec3,
        cam_pos: glam::Vec3,
    ) -> (
        [glam::Mat4; NUM_SHADOW_CASCADES as usize],
        crate::gpu_types::CascadedShadowMatrixGpu,
    ) {
        let light_dir = sun_dir.normalize();
        let splits = self.calculate_cascade_splits();

        if !self.csm_logged_once.get() {
            log::info!(
                "Calculating CSM cascade matrices for sun_dir={:?}, cam_pos={:?}",
                light_dir,
                cam_pos
            );
        }

        let mut matrices = [glam::Mat4::IDENTITY; NUM_SHADOW_CASCADES as usize];
        for i in 0..NUM_SHADOW_CASCADES as usize {
            let (near, far) = splits[i];
            matrices[i] = self.calculate_cascade_matrix(i as u32, light_dir, cam_pos, near, far);
        }

        let cols0 = matrices[0].to_cols_array_2d();
        let cols1 = matrices[1].to_cols_array_2d();

        let gpu_data = crate::gpu_types::CascadedShadowMatrixGpu {
            cascade0_m0: cols0[0],
            cascade0_m1: cols0[1],
            cascade0_m2: cols0[2],
            cascade0_m3: cols0[3],
            cascade1_m0: cols1[0],
            cascade1_m1: cols1[1],
            cascade1_m2: cols1[2],
            cascade1_m3: cols1[3],
            split_distances: [
                CASCADE_SPLIT_DISTANCES[0],
                CASCADE_SPLIT_DISTANCES[1],
                0.0, // unused
                0.0, // unused
            ],
        };

        if !self.csm_logged_once.get() {
            log::info!("CSM cascade matrices calculated successfully");
            self.csm_logged_once.set(true);
        }

        (matrices, gpu_data)
    }

    /// Calculate light matrix for a directional light at given direction
    fn calculate_light_matrix(&self, light_dir: glam::Vec3, cam_pos: glam::Vec3) -> glam::Mat4 {
        let light_dir = light_dir.normalize();
        let cascade_center = cam_pos;
        let light_distance = SHADOW_DISTANCE * 2.0;
        let light_pos = cascade_center - light_dir * light_distance;

        let light_view = glam::Mat4::look_at_rh(light_pos, cascade_center, glam::Vec3::Y);
        let cascade_radius = SHADOW_DISTANCE * 1.5;

        let light_proj = glam::Mat4::orthographic_rh(
            -cascade_radius,
            cascade_radius,
            -cascade_radius,
            cascade_radius,
            1.0,
            light_distance + SHADOW_DISTANCE,
        );

        light_proj * light_view
    }

    /// Calculate multi-light shadow matrices for sun and moon
    /// Returns updated active_lights list and GPU data for shader
    pub fn calculate_multi_light_matrices(
        &mut self,
        sun_dir: glam::Vec3,
        moon_dir: glam::Vec3,
        cam_pos: glam::Vec3,
    ) -> crate::gpu_types::MultiLightShadowGpu {
        // Clear previous frame's active lights
        self.active_lights.clear();

        // Use intensity values from current_lighting (set by update_lighting)
        // These are calculated by the frame processor based on time of day
        let sun_intensity = self.current_lighting.sun_direction[3];
        let moon_intensity = self.current_lighting.moon_direction[3];

        // Calculate sun shadow matrix (light 0)
        let sun_matrix = self.calculate_light_matrix(sun_dir, cam_pos);

        // Calculate moon shadow matrix (light 1)
        let moon_matrix = self.calculate_light_matrix(moon_dir, cam_pos);

        // Add sun to active lights if above horizon
        if sun_intensity > 0.01 {
            self.active_lights.push(ActiveShadowLight {
                light_type: LightType::Sun,
                light_index: 0,
                matrix: sun_matrix,
                intensity: sun_intensity,
            });
        }

        // Add moon to active lights if above horizon
        if moon_intensity > 0.01 {
            self.active_lights.push(ActiveShadowLight {
                light_type: LightType::Moon,
                light_index: 1,
                matrix: moon_matrix,
                intensity: moon_intensity,
            });
        }

        if !self.csm_logged_once.get() {
            log::info!(
                "Multi-light shadows: sun_intensity={:.2}, moon_intensity={:.2}, active_lights={}",
                sun_intensity,
                moon_intensity,
                self.active_lights.len()
            );
        }

        // Build GPU data structure
        let sun_cols = sun_matrix.to_cols_array_2d();
        let moon_cols = moon_matrix.to_cols_array_2d();

        let gpu_data = crate::gpu_types::MultiLightShadowGpu {
            light0_m0: sun_cols[0],
            light0_m1: sun_cols[1],
            light0_m2: sun_cols[2],
            light0_m3: sun_cols[3],

            light1_m0: moon_cols[0],
            light1_m1: moon_cols[1],
            light1_m2: moon_cols[2],
            light1_m3: moon_cols[3],

            // Lights 2-3 are identity (unused for now)
            light2_m0: [1.0, 0.0, 0.0, 0.0],
            light2_m1: [0.0, 1.0, 0.0, 0.0],
            light2_m2: [0.0, 0.0, 1.0, 0.0],
            light2_m3: [0.0, 0.0, 0.0, 1.0],

            light3_m0: [1.0, 0.0, 0.0, 0.0],
            light3_m1: [0.0, 1.0, 0.0, 0.0],
            light3_m2: [0.0, 0.0, 1.0, 0.0],
            light3_m3: [0.0, 0.0, 0.0, 1.0],

            light_intensities: [sun_intensity, moon_intensity, 0.0, 0.0],
            // metadata: [shadow_distance, light_size, pcss_quality, unused]
            metadata: [
                SHADOW_DISTANCE,
                self.pcss_settings.light_size,
                self.pcss_settings.quality as u32 as f32,
                0.0,
            ],
        };

        if !self.csm_logged_once.get() {
            log::info!("Multi-light shadow matrices calculated successfully");
            self.csm_logged_once.set(true);
        }

        gpu_data
    }

    /// Update PCSS settings for shadow softness control
    /// Changes take effect on the next frame when shadow matrices are recalculated
    pub fn set_pcss_settings(&mut self, settings: PcssSettings) {
        self.pcss_settings = settings;
        log::info!(
            "PCSS settings updated: quality={:?}, light_size={:.3}",
            settings.quality,
            settings.light_size
        );
    }

    /// Get current PCSS settings
    pub fn pcss_settings(&self) -> &PcssSettings {
        &self.pcss_settings
    }
}
