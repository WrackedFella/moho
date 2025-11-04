use wgpu::util::DeviceExt;

/// Number of cascaded shadow map cascades
pub const NUM_SHADOW_CASCADES: u32 = 4;

/// Shadow map resolution per cascade
pub const SHADOW_MAP_SIZE: u32 = 4096;

/// Cascade split distances from camera (in world units)
/// Scaled up for increased view distance - covers near terrain to distant features
pub const CASCADE_SPLIT_DISTANCES: [f32; 4] = [50.0, 150.0, 400.0, 800.0];

#[allow(dead_code)]
const CSM_DEBUG_MODE: bool = false;

const CSM_VERBOSE_LOGGING: bool = false;

/// Shadow system resources and state
pub struct ShadowSystem {
    pub shadow_pipeline: wgpu::RenderPipeline,
    pub shadow_matrix_buffer: wgpu::Buffer,
    pub shadow_map_view: wgpu::TextureView,
    pub shadow_pass_bind_group: wgpu::BindGroup,
    pub csm_matrix_buffer: wgpu::Buffer,
    pub csm_cascade_views: Vec<wgpu::TextureView>,
    pub csm_pass_bind_group: wgpu::BindGroup,
    pub csm_shadow_bind_group: wgpu::BindGroup,
    pub current_lighting: crate::gpu_types::LightingGpu,
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

        // CSM texture array
        let csm_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("csm-texture-array"),
            size: wgpu::Extent3d {
                width: SHADOW_MAP_SIZE,
                height: SHADOW_MAP_SIZE,
                depth_or_array_layers: NUM_SHADOW_CASCADES,
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
                "Created CSM texture array: {}x{} with {} layers",
                SHADOW_MAP_SIZE,
                SHADOW_MAP_SIZE,
                NUM_SHADOW_CASCADES
            );
        }

        let csm_cascade_views: Vec<wgpu::TextureView> = (0..NUM_SHADOW_CASCADES)
            .map(|i| {
                csm_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(&format!("csm-cascade-{}-view", i)),
                    format: Some(wgpu::TextureFormat::Depth32Float),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    aspect: wgpu::TextureAspect::All,
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

        let csm_array_view = csm_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("csm-array-view"),
            format: Some(wgpu::TextureFormat::Depth32Float),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: Some(NUM_SHADOW_CASCADES),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
        });

        if CSM_VERBOSE_LOGGING {
            log::info!(
                "Created {} cascade views + 1 array view for CSM sampling",
                NUM_SHADOW_CASCADES
            );
        }

        use crate::gpu_types::ShadowMatrixGpu;
        let initial_shadow_matrix = ShadowMatrixGpu::default();
        let shadow_matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shadow-matrix-buffer"),
            contents: bytemuck::bytes_of(&initial_shadow_matrix),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        use crate::gpu_types::CascadedShadowMatrixGpu;
        let initial_csm_matrix = CascadedShadowMatrixGpu::default();
        let csm_matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("csm-matrix-buffer"),
            contents: bytemuck::bytes_of(&initial_csm_matrix),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        if CSM_VERBOSE_LOGGING {
            log::info!(
                "Created CSM matrix buffer: {} bytes",
                std::mem::size_of::<CascadedShadowMatrixGpu>()
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
            layout: &shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: shadow_matrix_buffer.as_entire_binding(),
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
            csm_logged_once: std::cell::Cell::new(false),
        })
    }

    /// Calculate shadow matrix for legacy single shadow map
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
        let cols2 = matrices[2].to_cols_array_2d();
        let cols3 = matrices[3].to_cols_array_2d();

        let gpu_data = crate::gpu_types::CascadedShadowMatrixGpu {
            cascade0_m0: cols0[0],
            cascade0_m1: cols0[1],
            cascade0_m2: cols0[2],
            cascade0_m3: cols0[3],
            cascade1_m0: cols1[0],
            cascade1_m1: cols1[1],
            cascade1_m2: cols1[2],
            cascade1_m3: cols1[3],
            cascade2_m0: cols2[0],
            cascade2_m1: cols2[1],
            cascade2_m2: cols2[2],
            cascade2_m3: cols2[3],
            cascade3_m0: cols3[0],
            cascade3_m1: cols3[1],
            cascade3_m2: cols3[2],
            cascade3_m3: cols3[3],
            split_distances: CASCADE_SPLIT_DISTANCES,
        };

        if !self.csm_logged_once.get() {
            log::info!("CSM cascade matrices calculated successfully");
            self.csm_logged_once.set(true);
        }

        (matrices, gpu_data)
    }
}
