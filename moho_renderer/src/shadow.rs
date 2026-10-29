use wgpu::util::DeviceExt;

use crate::gpu_types::ShadowGpu;

mod cascades;
mod light_matrix;
mod tier;

pub(crate) use tier::ShadowTier;

/// Shadow-map layers: the sun's cascades first, then the moon's.
pub(crate) const CASCADE_LAYERS: usize = 6;
/// Sun cascades (layers `0..SUN_CASCADES`).
pub(crate) const SUN_CASCADES: usize = 4;
/// Moon cascades (layers `SUN_CASCADES..CASCADE_LAYERS`).
pub(crate) const MOON_CASCADES: usize = 2;
/// Nearest distance the split scheme treats as the start of the view, in metres.
pub(crate) const SPLIT_NEAR: f32 = 1.0;
/// Blend between logarithmic (1) and uniform (0) splits.
pub(crate) const SPLIT_LAMBDA: f32 = 0.8;
/// Texels of padding so filter taps stay inside a cascade's map.
pub(crate) const FILTER_MARGIN_TEXELS: f32 = 16.0;
/// Fraction of the previous slice's length that a slice starts early, to blend cascades.
pub(crate) const CASCADE_BLEND: f32 = 0.1;
/// How far toward the light, in metres, casters outside a cascade's sphere still land in its map.
pub(crate) const CASTER_REACH: f32 = 500.0;

/// Furthest view depth, in metres, that casts or receives a cascaded shadow.
pub const SHADOW_DISTANCE: f32 = 1000.0;

const CSM_VERBOSE_LOGGING: bool = false;

// ── Shadow geometry ────────────────────────────────────────────────────
/// Depth bias constant factor applied to shadow map.
const SHADOW_DEPTH_BIAS_CONSTANT: i32 = 4;
/// Depth bias slope scale applied to shadow map. The normal offset in the main pass carries the
/// rest of acne prevention; no clamp, since one clamp cannot suit cascades whose texels differ
/// 18-fold.
const SHADOW_DEPTH_BIAS_SLOPE: f32 = 1.0;
/// Minimum light intensity required to cast shadows.
const MIN_SHADOW_LIGHT_INTENSITY: f32 = 0.01;

/// Active shadow-casting light
#[derive(Debug, Clone)]
pub struct ActiveShadowLight {
    pub light_index: u32, // 0 = sun, 1 = moon
    pub intensity: f32,
}

/// Shadow system resources and state
#[derive(Debug)]
pub struct ShadowSystem {
    pub shadow_pipeline: wgpu::RenderPipeline,
    pub csm_matrix_buffer: wgpu::Buffer,
    /// One view per shadow-map layer, to render into it.
    pub csm_cascade_views: Vec<wgpu::TextureView>,
    pub csm_pass_bind_group: wgpu::BindGroup,
    pub csm_shadow_bind_group: wgpu::BindGroup,
    pub current_lighting: crate::gpu_types::LightingGpu,
    /// Active shadow-casting lights for current frame
    pub active_lights: Vec<ActiveShadowLight>,
    tier: ShadowTier,
    /// Texels per side of the cascade array as allocated: the tier's size within the device limit.
    map_size: u32,
    /// Largest 2D texture the device allows.
    device_limit: u32,
    /// Kept to rebuild `csm_shadow_bind_group` when the array is recreated.
    shadow_bind_group_layout: wgpu::BindGroupLayout,
    shadow_sampler: wgpu::Sampler,
    csm_logged_once: bool,
    /// The block uploaded last frame, kept when a frame's projection has no cascades.
    last_block: ShadowGpu,
    projection_warned: bool,
}

impl ShadowSystem {
    pub fn new(
        device: &wgpu::Device,
        _camera_bind_group_layout: &wgpu::BindGroupLayout,
        shadow_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let tier = ShadowTier::High;
        let device_limit = device.limits().max_texture_dimension_2d;
        let map_size = tier.map_size(device_limit);
        let (csm_cascade_views, csm_array_view) = Self::create_shadow_textures(device, map_size);
        let csm_matrix_buffer = Self::create_uniform_buffer(device);
        let shadow_sampler = Self::create_depth_sampler(device);
        let csm_pass_bgl = crate::pipeline::layouts::create_csm_pass_bind_group_layout(device)?;
        let (csm_pass_bind_group, csm_shadow_bind_group) = Self::create_bind_groups(
            device,
            &csm_pass_bgl,
            shadow_bind_group_layout,
            &csm_matrix_buffer,
            &csm_array_view,
            &shadow_sampler,
        );
        let shadow_pipeline = Self::create_render_pipeline(device, &csm_pass_bgl)?;

        Ok(Self {
            shadow_pipeline,
            csm_matrix_buffer,
            csm_cascade_views,
            csm_pass_bind_group,
            csm_shadow_bind_group,
            current_lighting: crate::gpu_types::LightingGpu::default(),
            active_lights: Vec::new(),
            tier,
            map_size,
            device_limit,
            shadow_bind_group_layout: shadow_bind_group_layout.clone(),
            shadow_sampler,
            csm_logged_once: false,
            last_block: ShadowGpu::default(),
            projection_warned: false,
        })
    }

    /// Stage 1: Allocate the cascade array, one layer per cascade.
    /// Returns the per-layer views (for rendering into each layer) and the array view (for
    /// sampling all layers in the fragment shader).
    fn create_shadow_textures(
        device: &wgpu::Device,
        map_size: u32,
    ) -> (Vec<wgpu::TextureView>, wgpu::TextureView) {
        let csm_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cascade-shadow-array"),
            size: wgpu::Extent3d {
                width: map_size,
                height: map_size,
                depth_or_array_layers: CASCADE_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        if CSM_VERBOSE_LOGGING {
            tracing::info!(
                size = map_size,
                layers = CASCADE_LAYERS,
                "Created cascade shadow array"
            );
        }

        let csm_cascade_views: Vec<wgpu::TextureView> = (0..CASCADE_LAYERS as u32)
            .map(|i| {
                csm_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some(&format!("shadow-layer-{i}-view")),
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

        // Full-array view consumed by the fragment shader sampler
        let csm_array_view = csm_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("cascade-shadow-array-view"),
            format: Some(wgpu::TextureFormat::Depth32Float),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: Some(CASCADE_LAYERS as u32),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
        });

        (csm_cascade_views, csm_array_view)
    }

    /// Stage 2: Allocate the cascade uniform buffer, zero-initialized.
    fn create_uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cascade-shadow-buffer"),
            contents: bytemuck::bytes_of(&ShadowGpu::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        if CSM_VERBOSE_LOGGING {
            tracing::info!(
                bytes = std::mem::size_of::<ShadowGpu>(),
                "Created cascade shadow buffer"
            );
        }

        buffer
    }

    /// Stage 3: Create the comparison sampler used for PCF shadow lookups.
    fn create_depth_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        })
    }

    /// Stage 4: Instantiate the pass and sampling bind groups using the layouts and resources
    /// from the previous stages.
    fn create_bind_groups(
        device: &wgpu::Device,
        csm_pass_bgl: &wgpu::BindGroupLayout,
        shadow_bind_group_layout: &wgpu::BindGroupLayout,
        csm_matrix_buffer: &wgpu::Buffer,
        csm_array_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
    ) -> (wgpu::BindGroup, wgpu::BindGroup) {
        let csm_pass_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("csm-pass-bind-group"),
            layout: csm_pass_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: csm_matrix_buffer.as_entire_binding(),
            }],
        });

        let csm_shadow_bind_group = Self::create_sampling_bind_group(
            device,
            shadow_bind_group_layout,
            csm_matrix_buffer,
            csm_array_view,
            shadow_sampler,
        );

        (csm_pass_bind_group, csm_shadow_bind_group)
    }

    /// The bind group the main fragment shader reads shadow maps through.
    fn create_sampling_bind_group(
        device: &wgpu::Device,
        shadow_bind_group_layout: &wgpu::BindGroupLayout,
        csm_matrix_buffer: &wgpu::Buffer,
        csm_array_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("csm-shadow-bind-group"),
            layout: shadow_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: csm_matrix_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(csm_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
            ],
        })
    }

    /// Switch to `tier`. The cascade array, its views and the sampling bind group are recreated
    /// only when the map size within the device limit changes. Call between frames: the old array
    /// must not be in a recorded pass.
    pub fn set_tier(&mut self, device: &wgpu::Device, tier: ShadowTier) {
        self.tier = tier;
        let map_size = tier.map_size(self.device_limit);
        if !tier::needs_resize(self.map_size, map_size) {
            return;
        }
        let (views, array_view) = Self::create_shadow_textures(device, map_size);
        self.csm_shadow_bind_group = Self::create_sampling_bind_group(
            device,
            &self.shadow_bind_group_layout,
            &self.csm_matrix_buffer,
            &array_view,
            &self.shadow_sampler,
        );
        self.csm_cascade_views = views;
        self.map_size = map_size;
    }

    /// Stage 6: Compile the shadow shader and assemble the depth-only render pipeline; its
    /// fragment stage only discards above the view-height cut.
    fn create_render_pipeline(
        device: &wgpu::Device,
        csm_pass_bgl: &wgpu::BindGroupLayout,
    ) -> Result<wgpu::RenderPipeline, Box<dyn std::error::Error>> {
        let shadow_shader_source = std::fs::read_to_string("shaders/shadow.wgsl")
            .map_err(|e| format!("Failed to read shadow shader: {e}"))?;
        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shadow-shader"),
            source: wgpu::ShaderSource::Wgsl(shadow_shader_source.into()),
        });

        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow-pipeline-layout"),
                bind_group_layouts: &[Some(csm_pass_bgl)],
                immediate_size: 4,
            });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow-pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<crate::types::Vertex>()
                            as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    }),
                    Some(wgpu::VertexBufferLayout {
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
                    }),
                ],
            },
            // Fragment stage only discards above the view-height cut; no colour output.
            fragment: Some(wgpu::FragmentState {
                module: &shadow_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Revert to Standard Back-Face Culling (Exp 17)
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                // Enable Hardware Depth Bias (Exp 17)
                bias: wgpu::DepthBiasState {
                    constant: SHADOW_DEPTH_BIAS_CONSTANT,
                    slope_scale: SHADOW_DEPTH_BIAS_SLOPE,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Ok(pipeline)
    }

    /// The shadow-map layers to render this frame, from `active_lights`.
    pub fn layers_to_render(&self) -> Vec<u32> {
        light_matrix::shadow_passes(&self.active_lights, self.tier)
    }

    /// Fit the sun's and moon's cascades to the camera and return the block to upload.
    /// Also refreshes `active_lights`. A projection with no cascades (not a perspective one)
    /// keeps last frame's block.
    pub fn update_cascades(
        &mut self,
        sun_dir: glam::Vec3,
        moon_dir: glam::Vec3,
        view: glam::Mat4,
        proj: glam::Mat4,
    ) -> ShadowGpu {
        // Intensities come from current_lighting (set by update_lighting), which the frame
        // processor derives from the time of day.
        let sun_intensity = self.current_lighting.sun_direction[3];
        let moon_intensity = self.current_lighting.moon_direction[3];
        self.active_lights = light_matrix::active_shadow_lights(sun_intensity, moon_intensity);

        let Some(frustum) = cascades::CameraFrustum::from_projection(proj) else {
            if !self.projection_warned {
                tracing::warn!(
                    "camera projection is not a perspective one; shadows keep last frame's cascades"
                );
                self.projection_warned = true;
            }
            return self.last_block;
        };

        let camera_world = view.inverse();
        let sun_splits = cascades::practical_splits::<SUN_CASCADES>(frustum.near, frustum.far);
        let moon_splits = cascades::practical_splits::<MOON_CASCADES>(frustum.near, frustum.far);
        let sun =
            cascades::fit_cascades(sun_dir, frustum, camera_world, &sun_splits, self.map_size);
        let moon =
            cascades::fit_cascades(moon_dir, frustum, camera_world, &moon_splits, self.map_size);
        let splits = (sun_splits, moon_splits);
        let camera_forward = cascades::camera_forward(camera_world);
        // An inactive light reads as zero, so the shader never samples cascades that were not drawn.
        let active_intensity = |slot: u32| {
            self.active_lights
                .iter()
                .find(|light| light.light_index == slot)
                .map_or(0.0, |light| light.intensity)
        };

        let block = light_matrix::shadow_gpu(
            &sun,
            &moon,
            splits,
            camera_forward,
            (active_intensity(0), active_intensity(1)),
            self.tier,
        );

        if !self.csm_logged_once {
            tracing::info!(
                sun_intensity,
                moon_intensity,
                active_lights = self.active_lights.len(),
                "Cascaded shadows"
            );
            self.csm_logged_once = true;
        }

        self.last_block = block;
        block
    }
}

#[cfg(test)]
mod tests {
    /// The value `fragment.wgsl` declares for `NORMAL_OFFSET_TEXELS`.
    const NORMAL_OFFSET_TEXELS: f32 = 2.0;

    #[test]
    fn main_shader_declares_the_normal_offset_the_card_chose() {
        let source = crate::pipeline::main_shader_source();

        let declaration = format!("const NORMAL_OFFSET_TEXELS: f32 = {NORMAL_OFFSET_TEXELS:?};");

        assert!(source.contains(&declaration), "missing `{declaration}`");
    }

    #[test]
    fn main_shader_cascade_counts_match_the_rust_constants() {
        let source = crate::pipeline::main_shader_source();

        let sun = format!("const SUN_CASCADES: u32 = {}u;", super::SUN_CASCADES);
        let moon = format!("const MOON_CASCADES: u32 = {}u;", super::MOON_CASCADES);

        assert!(source.contains(&sun), "missing `{sun}`");
        assert!(source.contains(&moon), "missing `{moon}`");
    }
}
