use crate::MeshRenderer;
use crate::shadow::ShadowSystem;
use crate::types::{DrawKey, GpuInstance, MeshEntry, NO_VERTEX_MATERIAL, Vertex, VertexSource};
use moho_render_api::WorldMesh;
use wgpu::util::DeviceExt;

pub struct Renderer<'a> {
    #[allow(dead_code)]
    window: &'a winit::window::Window,
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    present_modes: Vec<wgpu::PresentMode>,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    material_buffer: Option<wgpu::Buffer>,
    lighting_buffer: wgpu::Buffer,
    _depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    depth_format: wgpu::TextureFormat,
    instance_buffer: Option<wgpu::Buffer>,
    instance_capacity: usize,
    mesh_table: Vec<Option<MeshEntry>>,
    pending_frame: Option<wgpu::SurfaceTexture>,
    pending_draws: Vec<(DrawKey, Vec<GpuInstance>)>,
    textures: crate::textures::TextureTable,
    pending_frame_view: Option<wgpu::TextureView>,
    skybox_pipeline: wgpu::RenderPipeline,
    skybox_vertex_buffer: wgpu::Buffer,
    skybox_vertex_count: u32,
    shadow: ShadowSystem,
    ssao: Option<crate::ssao::SsaoSystem>,
    view_height: Option<f32>,
    depth_sampler: wgpu::Sampler,
    light_manager: crate::lights::LightManager,
    dynamic_lights_buffer: wgpu::Buffer,
    ui_pass: crate::ui_pass::UiPass,
    ui_source: Option<crate::UiSource>,
    marker_pipeline: wgpu::RenderPipeline,
    marker_outline_pipeline: wgpu::RenderPipeline,
    marker_buffers: crate::resources::MarkerBuffers,
    /// This frame's fills followed by its outlines.
    pending_markers: Vec<moho_render_api::MarkerGpu>,
    /// How many leading entries of `pending_markers` are fills.
    pending_fill_count: usize,
    /// Debug view that replaces the game's own in every lighting update, once one is set.
    debug_view: Option<u32>,
    hdr_target: crate::tone_map::HdrTarget,
    bloom: crate::bloom::BloomChain,
    tone_map: crate::tone_map::ToneMapPass,
    exposure: f32,
    tone_map_enabled: bool,
    skinning: crate::skinning::Skinning,
}

impl<'a> Renderer<'a> {
    /// Internal constructor used by RendererBuilder.
    ///
    /// This is intentionally not public to enforce using the builder pattern
    /// for initialization, which ensures all stages happen in the correct order.
    pub(crate) fn from_components(
        window: &'a winit::window::Window,
        device_setup: crate::device::DeviceSetup<'a>,
        pipeline_setup: &crate::pipeline::PipelineSetup,
        resources: crate::resources::ResourcePool,
        shadow: ShadowSystem,
    ) -> Self {
        // Create depth sampler for SSAO
        let depth_sampler = device_setup
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("depth-sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

        // Initialize SSAO system
        let ssao = crate::ssao::SsaoSystem::new(
            &device_setup.device,
            device_setup.config.width,
            device_setup.config.height,
            crate::ssao::SsaoSettings::default(),
        )
        .ok(); // Ignore errors for now (SSAO is optional)

        // Initialize light manager and dynamic lights buffer
        let light_manager = crate::lights::LightManager::new();
        let dynamic_lights_buffer =
            device_setup
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("dynamic-lights-buffer"),
                    contents: bytemuck::bytes_of(light_manager.gpu_data()),
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                });

        let ui_pass = crate::ui_pass::UiPass::new(&device_setup.device, device_setup.config.format);

        let hdr_target = crate::tone_map::HdrTarget::new(
            &device_setup.device,
            device_setup.config.width,
            device_setup.config.height,
        );
        let bloom = crate::bloom::BloomChain::new(
            &device_setup.device,
            hdr_target.view(),
            (device_setup.config.width, device_setup.config.height),
            crate::bloom::BloomQuality::High,
        );
        let tone_map = crate::tone_map::ToneMapPass::new(
            &device_setup.device,
            device_setup.config.format,
            &hdr_target,
            bloom.output(),
        );
        tone_map.set_params(&device_setup.queue, 1.0, true, bloom.intensity());
        let skinning = crate::skinning::Skinning::new(&device_setup.device);
        let textures = crate::textures::TextureTable::new(
            &device_setup.device,
            &device_setup.queue,
            pipeline_setup.texture_bind_group_layout().clone(),
        );

        let mut renderer = Self {
            window,
            surface: device_setup.surface,
            device: device_setup.device,
            queue: device_setup.queue,
            config: device_setup.config,
            present_modes: device_setup.present_modes,
            pipeline: pipeline_setup.main_pipeline().clone(),
            camera_buffer: resources.camera_buffer,
            camera_bind_group: resources.camera_bind_group,
            camera_bind_group_layout: pipeline_setup.camera_bind_group_layout().clone(),
            material_buffer: Some(resources.material_buffer),
            lighting_buffer: resources.lighting_buffer,
            _depth_texture: resources.depth_texture,
            depth_texture_view: resources.depth_texture_view,
            depth_format: pipeline_setup.depth_format(),
            instance_buffer: Some(resources.instance_buffer),
            instance_capacity: resources.instance_capacity,
            mesh_table: Vec::new(),
            pending_frame: None,
            pending_draws: Vec::new(),
            textures,
            pending_frame_view: None,
            skybox_pipeline: pipeline_setup.skybox_pipeline().clone(),
            skybox_vertex_buffer: resources.skybox_vertex_buffer,
            skybox_vertex_count: resources.skybox_vertex_count,
            shadow,
            ssao,
            view_height: None,
            depth_sampler,
            light_manager,
            dynamic_lights_buffer,
            ui_pass,
            ui_source: None,
            marker_pipeline: pipeline_setup.marker_pipeline().clone(),
            marker_outline_pipeline: pipeline_setup.marker_outline_pipeline().clone(),
            marker_buffers: resources.marker_buffers,
            pending_markers: Vec::new(),
            pending_fill_count: 0,
            debug_view: None,
            hdr_target,
            bloom,
            tone_map,
            exposure: 1.0,
            tone_map_enabled: true,
            skinning,
        };

        // Recreate camera bind group with SSAO textures if available
        if renderer.ssao.is_some() {
            renderer.recreate_camera_bind_group();
        }

        renderer
    }

    pub fn new(window: &'a winit::window::Window) -> Result<Self, Box<dyn std::error::Error>> {
        crate::builder::RendererBuilder::new(window)
            .init_device()?
            .create_pipelines()?
            .allocate_resources()?
            .build()
    }

    /// Update lighting parameters and write to GPU buffer.
    pub fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu) {
        let lighting = lighting_with_sky_palette(lighting);
        let lighting = lighting_with_debug_view(lighting, self.debug_view);
        self.shadow.current_lighting = lighting;
        self.queue
            .write_buffer(&self.lighting_buffer, 0, bytemuck::bytes_of(&lighting));
    }

    /// Override the shader's debug view mode, now and in every later lighting update; `None`
    /// leaves the game's own value.
    pub fn set_debug_view(&mut self, mode: Option<u32>) {
        self.debug_view = mode;
        self.update_lighting(self.shadow.current_lighting);
    }

    /// Add a dynamic point light to the scene. Returns the light ID for future updates/removal.
    pub fn add_point_light(
        &mut self,
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        range: f32,
    ) -> u32 {
        let light = crate::lights::Light::new_point(position, color, intensity, range);
        self.light_manager.add_light(light)
    }

    /// Remove a dynamic light by ID.
    pub fn remove_light(&mut self, id: u32) -> bool {
        self.light_manager.remove_light(id)
    }

    /// Update a light's position.
    pub fn set_light_position(&mut self, id: u32, position: glam::Vec3) {
        self.light_manager.set_light_position(id, position);
    }

    /// Enable or disable a light.
    pub fn set_light_enabled(&mut self, id: u32, enabled: bool) {
        self.light_manager.set_light_enabled(id, enabled);
    }

    /// Get all lights as serializable descriptors (for save/load).
    pub fn all_lights_as_descs(&self) -> Vec<moho_render_api::LightDesc> {
        self.light_manager
            .all_lights()
            .iter()
            .map(|l| moho_render_api::LightDesc {
                position: l.position.to_array(),
                color: l.color.to_array(),
                intensity: l.intensity,
                range: l.range,
                enabled: l.enabled,
            })
            .collect()
    }

    /// Update the material table on the GPU. This replaces the storage buffer bound at
    /// @group(0) binding 1 and recreates the camera bind group so the pipeline sees the new buffer.
    pub fn set_material_table(&mut self, materials: &[crate::MaterialGpu]) {
        if materials.is_empty() {
            return;
        }
        let bytes = bytemuck::cast_slice(materials);
        let mat_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("material-buffer"),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        self.material_buffer = Some(mat_buf);
        self.recreate_camera_bind_group();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-texture"),
            size: wgpu::Extent3d {
                width: self.config.width,
                height: self.config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.depth_texture_view =
            depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.hdr_target = crate::tone_map::HdrTarget::new(&self.device, width, height);
        self.bloom
            .resize(&self.device, self.hdr_target.view(), (width, height));
        self.tone_map
            .rebind(&self.device, &self.hdr_target, self.bloom.output());

        if let Some(ssao) = &mut self.ssao {
            ssao.resize(&self.device, width, height);
            self.recreate_camera_bind_group();
        }
    }

    fn recreate_camera_bind_group(&mut self) {
        let Some(mat_buffer) = &self.material_buffer else {
            tracing::error!("material buffer missing when creating camera bind group");
            return;
        };

        let (ssao_view, ssao_sampler) = if let Some(ssao) = &self.ssao {
            (ssao.blurred_ao_view(), &ssao.ao_sampler)
        } else {
            tracing::warn!("recreate_camera_bind_group called without SSAO system");
            return;
        };

        self.camera_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mat_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.lighting_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(ssao_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(ssao_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.dynamic_lights_buffer.as_entire_binding(),
                },
            ],
            label: Some("camera-bind-group"),
        });
    }

    pub fn register_mesh(&mut self, vertices: &[[f32; 3]]) -> u32 {
        let vertex_bytes = bytemuck::cast_slice(vertices);
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh-vertex-buffer"),
                contents: vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            });
        let vertex_count = vertices.len() as u32;
        let entry = MeshEntry {
            buffer: vb,
            vertex_count,
            index_buffer: None,
            index_count: 0,
        };
        let handle = self.mesh_table.len() as u32;
        self.mesh_table.push(Some(entry));
        handle
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_indexed_mesh(
        &mut self,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        ao: &[f32],
        geometry_type: &[u32],
        light_level: &[f32],
        block_light_rgb: &[[f32; 3]],
        sky_exposed: &[f32],
        material: &[u32],
        indices: &[u32],
    ) -> u32 {
        self.register_channels(
            &VertexChannels {
                positions: vertices,
                normals,
                ao,
                geometry_type,
                light_level,
                block_light_rgb,
                sky_exposed,
                material,
                uv: &[],
            },
            indices,
        )
    }

    /// Register a contract mesh with its UVs; `material` is the resolved per-vertex
    /// material channel (empty defers to the instance material).
    pub fn register_world_mesh(&mut self, mesh: &WorldMesh, material: &[u32]) -> u32 {
        let light_level = world_mesh_light_level(mesh);
        self.register_channels(
            &world_mesh_channels(mesh, &light_level, material),
            mesh.indices(),
        )
    }

    /// Upload `image` with its mip chain; returns its texture handle.
    pub fn register_texture(&mut self, image: &moho_render_api::Rgba8Image) -> u32 {
        self.textures.register(&self.device, &self.queue, image)
    }

    /// Free a texture handle; the white default (handle 0) is never freed.
    pub fn unregister_texture(&mut self, texture: u32) {
        self.textures.unregister(texture);
    }

    fn register_channels(&mut self, channels: &VertexChannels, indices: &[u32]) -> u32 {
        let iv = interleave_vertices(channels);
        for (i, v) in iv.iter().enumerate().take(6) {
            tracing::trace!(
                index = i,
                pos = ?v.position,
                normal = ?v.normal,
                "[register_indexed_mesh] vertex"
            );
        }
        if iv.len() > 50 {
            let samples = [
                0usize,
                iv.len() / 4,
                iv.len() / 2,
                3 * iv.len() / 4,
                iv.len() - 1,
            ];
            for idx in samples {
                let v = &iv[idx];
                tracing::debug!(
                    index = idx,
                    pos = ?v.position,
                    normal = ?v.normal,
                    "[register_indexed_mesh] sample vertex"
                );
            }
        }
        let vertex_bytes = bytemuck::cast_slice(&iv);
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh-vertex-buffer"),
                contents: vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            });
        let index_bytes = bytemuck::cast_slice(indices);
        let ib = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh-index-buffer"),
                contents: index_bytes,
                usage: wgpu::BufferUsages::INDEX,
            });
        let entry = MeshEntry {
            buffer: vb,
            vertex_count: iv.len() as u32,
            index_buffer: Some(ib),
            index_count: indices.len() as u32,
        };
        let handle = self.mesh_table.len() as u32;
        self.mesh_table.push(Some(entry));
        handle
    }

    /// Upload the skin of the mesh under `mesh`, so it can be drawn with
    /// [`enqueue_skinned_draw`](Self::enqueue_skinned_draw).
    pub fn register_skin(&mut self, mesh: u32, joints: &[[u16; 4]], weights: &[[f32; 4]]) {
        let Some(Some(entry)) = self.mesh_table.get(mesh as usize) else {
            tracing::error!(mesh, "skin for a mesh that is not registered");
            return;
        };
        self.skinning
            .register(&self.device, mesh, entry, joints, weights);
    }

    /// Unregister a mesh handle and free its GPU buffers.
    pub fn unregister_mesh(&mut self, mesh: u32) {
        self.skinning.unregister(mesh);
        let idx = mesh as usize;
        if idx < self.mesh_table.len() {
            self.mesh_table[idx] = None;
        }
    }

    fn acquire_render_target(&mut self) -> bool {
        if self.pending_frame_view.is_some() {
            return true; // Already acquired
        }

        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                let view = f
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                self.pending_frame = Some(f);
                self.pending_frame_view = Some(view);
                true
            }
            _ => {
                self.surface.configure(&self.device, &self.config);
                false
            }
        }
    }

    fn prepare_instance_buffer(&mut self, instances_gpu: &[GpuInstance]) -> Option<&wgpu::Buffer> {
        if !MeshRenderer::ensure_capacity_and_upload(
            &self.device,
            &self.queue,
            &mut self.instance_buffer,
            &mut self.instance_capacity,
            instances_gpu,
        ) {
            return None;
        }
        self.instance_buffer.as_ref()
    }

    /// Begin a new frame: uploads the camera, culls/updates dynamic lights,
    /// updates shadow matrices, and acquires the swapchain surface texture.
    pub fn begin_frame(
        &mut self,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), crate::FrameError> {
        let (view_mat, proj_mat, cam_pos) = camera;
        let cut = crate::gpu_types::view_height_uniform(self.view_height);
        crate::render_ops::camera_ops::update_camera_uniforms(
            &self.queue,
            &self.camera_buffer,
            view_mat,
            proj_mat,
            cam_pos,
            cut,
        );

        if let Some(ssao) = &self.ssao {
            let inv_proj = proj_mat.inverse();
            let inv_proj_array = inv_proj.to_cols_array_2d();
            ssao.update_camera(&self.queue, &inv_proj_array);
        }

        let view_proj = proj_mat * view_mat;
        self.light_manager.cull_lights(&view_proj);
        self.light_manager.update_gpu_data();
        self.queue.write_buffer(
            &self.dynamic_lights_buffer,
            0,
            bytemuck::bytes_of(self.light_manager.gpu_data()),
        );

        crate::render_ops::shadow_ops::update_shadow_matrices(
            &mut self.shadow,
            &self.queue,
            view_mat,
            proj_mat,
            cut,
        );

        if !self.acquire_render_target() {
            return Err(crate::FrameError::SurfaceUnavailable);
        }

        Ok(())
    }

    /// Queue a previously-registered mesh for drawing this frame, untextured.
    pub fn enqueue_draw(&mut self, mesh: u32, instances: &[moho_render_api::InstanceGpu]) {
        self.enqueue_textured_draw(mesh, crate::textures::WHITE_TEXTURE, instances);
    }

    /// Queue a previously-registered mesh for drawing this frame with `texture` bound in
    /// the main pass.
    pub fn enqueue_textured_draw(
        &mut self,
        mesh: u32,
        texture: u32,
        instances: &[moho_render_api::InstanceGpu],
    ) {
        if !MeshRenderer::validate_mesh(mesh, &self.mesh_table) {
            return;
        }
        let key = DrawKey {
            mesh,
            texture,
            source: VertexSource::Mesh,
        };
        self.queue_draw(key, instances);
    }

    /// Queue a registered skinned mesh for drawing this frame in the pose `palette` gives
    /// (3 rows per joint, `[m_r0, m_r1, m_r2, t_r]`), placed by `instance`. The mesh is
    /// skinned into the frame arena in `submit_frame`, before any pass.
    pub fn enqueue_skinned_draw(
        &mut self,
        mesh: u32,
        texture: u32,
        palette: &[[[f32; 4]; 3]],
        instance: moho_render_api::InstanceGpu,
    ) {
        if !MeshRenderer::validate_mesh(mesh, &self.mesh_table) {
            return;
        }
        let Some(instances_gpu) = self.prepare_instances_for_upload(&[instance]) else {
            return;
        };
        let Some(first_vertex) = self.skinning.queue(mesh, palette) else {
            return;
        };
        self.pending_draws
            .push((DrawKey::skinned(mesh, texture, first_vertex), instances_gpu));
    }

    fn queue_draw(&mut self, key: DrawKey, instances: &[moho_render_api::InstanceGpu]) {
        let Some(instances_gpu) = self.prepare_instances_for_upload(instances) else {
            return;
        };
        self.pending_draws.push((key, instances_gpu));
    }

    /// The GPU form of `instances`, once the instance buffer can hold them; `None` (logged)
    /// when there is no instance buffer.
    fn prepare_instances_for_upload(
        &mut self,
        instances: &[moho_render_api::InstanceGpu],
    ) -> Option<Vec<GpuInstance>> {
        let instances_gpu = MeshRenderer::prepare_instances(instances);
        if self.prepare_instance_buffer(&instances_gpu).is_none() {
            tracing::error!("instance buffer missing when uploading instances");
            return None;
        }
        Some(instances_gpu)
    }

    pub fn set_shadow_quality(&mut self, quality: u8) {
        let quality_enum = match quality {
            0 => crate::shadow::PcssQuality::Off,
            1 => crate::shadow::PcssQuality::Low,
            2 => crate::shadow::PcssQuality::Medium,
            3 => crate::shadow::PcssQuality::High,
            4 => crate::shadow::PcssQuality::Ultra,
            _ => crate::shadow::PcssQuality::Medium,
        };
        self.shadow.pcss_settings.quality = quality_enum;
    }

    /// Sets whether presented frames wait for the display refresh. Stays on when the GPU
    /// has no low-latency mode.
    pub fn set_vsync(&mut self, on: bool) {
        let choice = crate::device::choose_present_mode(on, &self.present_modes);
        if let Some(reason) = choice.fallback {
            tracing::warn!(reason, "vsync stays on");
        }
        if choice.mode == self.config.present_mode {
            return;
        }
        self.config.present_mode = choice.mode;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn set_view_height(&mut self, height: Option<f32>) {
        self.view_height = height;
    }

    /// Set the tone-map exposure from a Brightness value (see [`brightness_exposure`]).
    ///
    /// [`brightness_exposure`]: crate::brightness_exposure
    pub fn set_brightness(&mut self, brightness: f32) {
        self.exposure = crate::tone_map::brightness_exposure(brightness);
        self.write_tone_map_params();
    }

    /// Turn the tone-map curve on or off; off skips the curve and passes the HDR scene
    /// through, still adding bloom when it is enabled.
    pub fn set_tone_map(&mut self, enabled: bool) {
        self.tone_map_enabled = enabled;
        self.write_tone_map_params();
    }

    /// Set the bloom quality: 0 Off, 1 Low, 2 High; any other value gives High.
    pub fn set_bloom_quality(&mut self, quality: u8) {
        let quality = crate::bloom::BloomQuality::from_u8(quality);
        if quality == self.bloom.quality() {
            return;
        }
        self.bloom
            .set_quality(&self.device, self.hdr_target.view(), quality);
        self.tone_map
            .rebind(&self.device, &self.hdr_target, self.bloom.output());
        self.write_tone_map_params();
    }

    fn write_tone_map_params(&self) {
        self.tone_map.set_params(
            &self.queue,
            self.exposure,
            self.tone_map_enabled,
            self.bloom.intensity(),
        );
    }

    pub fn set_ssao_quality(&mut self, quality: u8) {
        let quality_enum = match quality {
            0 => crate::ssao::SsaoQuality::Off,
            1 => crate::ssao::SsaoQuality::Low,
            2 => crate::ssao::SsaoQuality::Medium,
            3 => crate::ssao::SsaoQuality::High,
            4 => crate::ssao::SsaoQuality::Ultra,
            _ => crate::ssao::SsaoQuality::Medium,
        };

        if let Some(ssao) = &mut self.ssao {
            ssao.set_quality(&self.queue, quality_enum);
        }
    }

    /// Store this frame's fill and outline markers (each sorted far to near)
    /// for `submit_frame` to draw.
    pub fn enqueue_markers(
        &mut self,
        fills: &[moho_render_api::MarkerGpu],
        outlines: &[moho_render_api::MarkerGpu],
    ) {
        self.pending_markers.clear();
        self.pending_markers.extend_from_slice(fills);
        self.pending_markers.extend_from_slice(outlines);
        self.pending_fill_count = fills.len();
    }

    /// Set the provider of UI paint data; `None` clears it.
    pub fn set_ui_source(&mut self, source: Option<crate::UiSource>) {
        self.ui_source = source;
    }

    fn draw_ui(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(source), Some(view)) = (&self.ui_source, &self.pending_frame_view) else {
            return;
        };
        let target = [self.config.width, self.config.height];
        let Ok(mut source) = source.lock() else {
            tracing::error!("UI source mutex poisoned; skipping UI pass");
            return;
        };
        let frame = source.ui_frame(target);
        drop(source);
        if let Some(frame) = frame {
            self.ui_pass
                .render(&self.device, &self.queue, encoder, view, target, &frame);
        }
    }

    /// Flush all queued draws (shadow passes, main pass into the HDR target, bloom chain,
    /// tone-map pass, markers, SSAO, UI) and present
    /// the frame. No-op if `begin_frame` wasn't called or didn't acquire a
    /// surface texture.
    pub fn submit_frame(&mut self) {
        if self.pending_frame_view.is_none() {
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("batched-encoder"),
            });

        let (all_instances, offsets) = MeshRenderer::flatten_instances(&self.pending_draws);

        if !MeshRenderer::ensure_capacity_and_upload(
            &self.device,
            &self.queue,
            &mut self.instance_buffer,
            &mut self.instance_capacity,
            &all_instances,
        ) {
            tracing::error!("instance buffer missing when uploading batched instances");
            return;
        }

        let ibuf = self.instance_buffer.as_ref().unwrap();
        let frame_view = self.pending_frame_view.as_ref().unwrap();

        self.skinning
            .dispatch(&self.device, &self.queue, &mut encoder);
        let arena = self.skinning.arena();

        crate::render_ops::shadow_ops::render_shadow_passes(
            &mut encoder,
            &self.shadow,
            ibuf,
            &self.pending_draws,
            &self.mesh_table,
            &offsets,
            arena,
        );

        crate::render_ops::main_pass_ops::render_main_pass(
            &mut encoder,
            self.hdr_target.view(),
            &self.depth_texture_view,
            &self.skybox_pipeline,
            &self.skybox_vertex_buffer,
            self.skybox_vertex_count,
            &self.pipeline,
            &self.camera_bind_group,
            &self.shadow,
            ibuf,
            &self.pending_draws,
            &self.textures,
            &self.mesh_table,
            &offsets,
            arena,
        );

        self.bloom.render(&mut encoder);
        self.tone_map.render(&mut encoder, frame_view);

        if !self.pending_markers.is_empty() {
            self.marker_buffers
                .upload(&self.device, &self.queue, &self.pending_markers);
            crate::render_ops::marker_pass_ops::render_marker_pass(
                &mut encoder,
                frame_view,
                &self.depth_texture_view,
                [&self.marker_pipeline, &self.marker_outline_pipeline],
                &self.camera_bind_group,
                &self.marker_buffers,
                [
                    0..self.pending_fill_count as u32,
                    self.pending_fill_count as u32..self.pending_markers.len() as u32,
                ],
            );
        }

        if let Some(ssao) = &self.ssao {
            ssao.compute_ao(
                &self.device,
                &mut encoder,
                &self.depth_texture_view,
                &self.depth_sampler,
            );
        }

        self.draw_ui(&mut encoder);

        let draw_count = self.pending_draws.len();
        crate::render_ops::frame_ops::finish_frame(
            encoder,
            &self.queue,
            self.pending_frame.take(),
            draw_count,
        );

        self.pending_frame_view = None;
        self.pending_draws.clear();
        self.skinning.end_frame();
        self.pending_markers.clear();
        self.pending_fill_count = 0;
    }
}

/// `lighting` with the sky palette fields (`sky_zenith`, `sky_horizon`, `sky_ground`)
/// filled from the time of day in `params[0]`; a game's values in those fields are replaced.
fn lighting_with_sky_palette(
    lighting: crate::gpu_types::LightingGpu,
) -> crate::gpu_types::LightingGpu {
    let [sky_zenith, sky_horizon, sky_ground] =
        crate::sky::sky_palette(lighting.params[0]).to_gpu();
    crate::gpu_types::LightingGpu {
        sky_zenith,
        sky_horizon,
        sky_ground,
        ..lighting
    }
}

/// `lighting` with the shader's debug mode (`params[1]`) replaced by `debug_view`; `None`
/// keeps the game's own value.
fn lighting_with_debug_view(
    lighting: crate::gpu_types::LightingGpu,
    debug_view: Option<u32>,
) -> crate::gpu_types::LightingGpu {
    let mut lighting = lighting;
    if let Some(mode) = debug_view {
        lighting.params[1] = mode as f32;
    }
    lighting
}

/// Per-vertex channels of an indexed mesh. Every channel but `positions` and `normals`
/// may be shorter than `positions`, or empty.
#[derive(Default)]
pub(crate) struct VertexChannels<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub ao: &'a [f32],
    pub geometry_type: &'a [u32],
    pub light_level: &'a [f32],
    pub block_light_rgb: &'a [[f32; 3]],
    pub sky_exposed: &'a [f32],
    pub material: &'a [u32],
    pub uv: &'a [[f32; 2]],
}

/// Interleave the per-channel slices into GPU vertices. A channel shorter than
/// `positions` fills the rest with its neutral value; `material` falls back to
/// `NO_VERTEX_MATERIAL`, so the instance's material draws, and `uv` to `[0, 0]`.
pub(crate) fn interleave_vertices(channels: &VertexChannels) -> Vec<Vertex> {
    let VertexChannels {
        positions,
        normals,
        ao,
        geometry_type,
        light_level,
        block_light_rgb,
        sky_exposed,
        material,
        uv,
    } = *channels;
    (0..positions.len())
        .map(|i| Vertex {
            position: positions[i],
            normal: normals[i],
            ao: ao.get(i).copied().unwrap_or(1.0),
            geometry_type: geometry_type.get(i).copied().unwrap_or(0),
            light_level: light_level.get(i).copied().unwrap_or(1.0),
            block_light_rgb: block_light_rgb.get(i).copied().unwrap_or([0.0; 3]),
            sky_exposed: sky_exposed.get(i).copied().unwrap_or(1.0),
            material: material.get(i).copied().unwrap_or(NO_VERTEX_MATERIAL),
            uv: uv.get(i).copied().unwrap_or([0.0; 2]),
        })
        .collect()
}

/// A contract mesh's per-vertex light level: the brightest channel of its block light.
pub(crate) fn world_mesh_light_level(mesh: &WorldMesh) -> Vec<f32> {
    mesh.light_rgb()
        .iter()
        .map(|rgb| rgb[0].max(rgb[1]).max(rgb[2]))
        .collect()
}

/// The channels of a contract mesh as `register_world_mesh` uploads them: the mesh's
/// own positions, normals, AO, surface, block light, sky exposure and UVs (empty when it
/// has none), with the caller's derived `light_level` and resolved `material`.
pub(crate) fn world_mesh_channels<'a>(
    mesh: &'a WorldMesh,
    light_level: &'a [f32],
    material: &'a [u32],
) -> VertexChannels<'a> {
    VertexChannels {
        positions: mesh.positions(),
        normals: mesh.normals(),
        ao: mesh.ao(),
        geometry_type: mesh.surface(),
        light_level,
        block_light_rgb: mesh.light_rgb(),
        sky_exposed: mesh.sky_exposure(),
        material,
        uv: mesh.uvs().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use crate::gpu_types::LightingGpu;

    use super::*;

    fn game_lighting() -> LightingGpu {
        LightingGpu {
            sun_direction: [0.1, 0.2, 0.3, 1.0],
            sun_color: [1.0, 0.9, 0.8, 0.0],
            moon_direction: [0.4, 0.5, 0.6, 0.2],
            moon_color: [0.1, 0.1, 0.3, 0.0],
            ambient: [0.2, 0.2, 0.25, 0.3],
            params: [13.5, 2.0, 7.0, 9.0],
            ..LightingGpu::default()
        }
    }

    fn rgb(v: [f32; 4]) -> [f32; 3] {
        [v[0], v[1], v[2]]
    }

    #[test]
    fn update_lighting_fills_the_palette_from_the_time_of_day() {
        for hour in [2.0, 6.0, 12.0, 19.0, 23.0] {
            let lighting = LightingGpu {
                params: [hour, 2.0, 7.0, 9.0],
                ..game_lighting()
            };

            let filled = lighting_with_sky_palette(lighting);

            let palette = crate::sky::sky_palette(hour);
            assert_eq!(rgb(filled.sky_zenith), palette.zenith, "zenith at {hour} h");
            assert_eq!(
                rgb(filled.sky_horizon),
                palette.horizon,
                "horizon at {hour} h"
            );
            assert_eq!(rgb(filled.sky_ground), palette.ground, "ground at {hour} h");
            assert_eq!(filled.params, lighting.params, "params untouched");
            assert_eq!(filled.ambient, lighting.ambient, "ambient untouched");
            assert_eq!(filled.sun_direction, lighting.sun_direction);
        }
    }

    #[test]
    fn a_games_sky_fields_are_replaced() {
        let lighting = LightingGpu {
            sky_zenith: [9.0, 9.0, 9.0, 9.0],
            sky_horizon: [8.0, 8.0, 8.0, 8.0],
            sky_ground: [7.0, 7.0, 7.0, 7.0],
            ..game_lighting()
        };

        let filled = lighting_with_sky_palette(lighting);

        let palette = crate::sky::sky_palette(13.5);
        assert_eq!(rgb(filled.sky_zenith), palette.zenith);
        assert_eq!(rgb(filled.sky_horizon), palette.horizon);
        assert_eq!(rgb(filled.sky_ground), palette.ground);
    }

    #[test]
    fn debug_view_override_replaces_params_y() {
        let lighting = game_lighting();

        let overridden = lighting_with_debug_view(lighting, Some(3));

        assert_eq!(overridden.params, [13.5, 3.0, 7.0, 9.0]);
        assert_eq!(overridden.sun_direction, lighting.sun_direction);
        assert_eq!(overridden.ambient, lighting.ambient);
    }

    #[test]
    fn debug_view_override_of_zero_clears_the_games_view() {
        let lighting = game_lighting();

        let overridden = lighting_with_debug_view(lighting, Some(0));

        assert_eq!(overridden.params[1], 0.0);
    }

    #[test]
    fn no_override_keeps_the_games_params_y() {
        let lighting = game_lighting();

        let kept = lighting_with_debug_view(lighting, None);

        assert_eq!(
            bytemuck::bytes_of(&kept),
            bytemuck::bytes_of(&lighting),
            "every field is untouched"
        );
    }

    #[test]
    fn interleave_without_materials_marks_every_vertex_no_vertex_material() {
        let vertices = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let normals = [[0.0, 1.0, 0.0]; 3];

        let interleaved = interleave_vertices(&VertexChannels {
            positions: &vertices,
            normals: &normals,
            ..Default::default()
        });

        let materials: Vec<u32> = interleaved.iter().map(|v| v.material).collect();
        assert_eq!(materials, vec![NO_VERTEX_MATERIAL; 3]);
    }

    #[test]
    fn interleave_carries_uvs_and_defaults_missing_ones_to_zero() {
        let vertices = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let normals = [[0.0, 1.0, 0.0]; 3];
        let uvs = [[0.25, 0.75], [1.0, 0.5]];

        let with_uvs = interleave_vertices(&VertexChannels {
            positions: &vertices,
            normals: &normals,
            uv: &uvs,
            ..Default::default()
        });
        let without_uvs = interleave_vertices(&VertexChannels {
            positions: &vertices,
            normals: &normals,
            ..Default::default()
        });

        let carried: Vec<[f32; 2]> = with_uvs.iter().map(|v| v.uv).collect();
        assert_eq!(
            carried,
            vec![[0.25, 0.75], [1.0, 0.5], [0.0, 0.0]],
            "a short channel is padded with zero"
        );
        assert!(without_uvs.iter().all(|v| v.uv == [0.0, 0.0]));
    }

    #[test]
    fn world_mesh_channels_carry_every_channel_including_uvs() {
        let uvs = vec![[0.25, 0.75], [1.0, 0.5], [0.0, 1.0]];
        let mesh = WorldMesh::new(
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![0.25, 0.5, 0.75],
            vec![[0.2, 0.9, 0.5], [0.9, 0.1, 0.1], [0.3, 0.4, 0.6]],
            vec![0.1, 0.2, 0.3],
            vec![3, 4, 5],
            vec![0, 1, 2],
        )
        .expect("well-formed test mesh")
        .with_uvs(uvs.clone())
        .expect("one uv per vertex");
        let light_level = [0.9, 0.9, 0.6];
        let material = [2, 5, 9];

        let channels = world_mesh_channels(&mesh, &light_level, &material);

        assert_eq!(channels.positions, mesh.positions());
        assert_eq!(channels.normals, mesh.normals());
        assert_eq!(channels.ao, [0.25, 0.5, 0.75]);
        assert_eq!(channels.geometry_type, [3, 4, 5]);
        assert_eq!(channels.light_level, light_level);
        assert_eq!(channels.block_light_rgb, mesh.light_rgb());
        assert_eq!(channels.sky_exposed, [0.1, 0.2, 0.3]);
        assert_eq!(channels.material, material);
        assert_eq!(channels.uv, uvs);
        let vertices = interleave_vertices(&channels);
        let carried: Vec<[f32; 2]> = vertices.iter().map(|v| v.uv).collect();
        assert_eq!(carried, uvs, "the UVs reach the GPU vertices");
    }

    #[test]
    fn world_mesh_channels_of_a_mesh_without_uvs_have_none() {
        let mesh = WorldMesh::new(
            vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0.0, 1.0, 0.0]; 3],
            vec![1.0; 3],
            vec![[1.0; 3]; 3],
            vec![1.0; 3],
            vec![0; 3],
            vec![0, 1, 2],
        )
        .expect("well-formed test mesh");

        let channels = world_mesh_channels(&mesh, &[1.0; 3], &[]);

        assert!(channels.uv.is_empty());
        assert!(channels.material.is_empty());
    }

    #[test]
    fn interleave_with_materials_carries_each_vertexs_material() {
        let vertices = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let normals = [[0.0, 1.0, 0.0]; 3];

        let interleaved = interleave_vertices(&VertexChannels {
            positions: &vertices,
            normals: &normals,
            material: &[4, 0, 9],
            ..Default::default()
        });

        let materials: Vec<u32> = interleaved.iter().map(|v| v.material).collect();
        assert_eq!(materials, vec![4, 0, 9]);
    }
}
