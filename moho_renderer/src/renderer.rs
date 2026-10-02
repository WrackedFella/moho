use crate::MeshRenderer;
use crate::shadow::ShadowSystem;
use crate::types::{GpuInstance, MeshEntry};
use wgpu::util::DeviceExt;

pub struct Renderer<'a> {
    #[allow(dead_code)]
    window: &'a winit::window::Window,
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
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
    pending_draws: Vec<(u32, Vec<GpuInstance>)>,
    pending_frame_view: Option<wgpu::TextureView>,
    frame_callback_raw: Option<*mut dyn crate::FrameCallback>,
    frame_callback_arc: Option<std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
    skybox_pipeline: wgpu::RenderPipeline,
    skybox_vertex_buffer: wgpu::Buffer,
    skybox_vertex_count: u32,
    shadow: ShadowSystem,
    ssao: Option<crate::ssao::SsaoSystem>,
    depth_sampler: wgpu::Sampler,
    light_manager: crate::lights::LightManager,
    dynamic_lights_buffer: wgpu::Buffer,
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

        let mut renderer = Self {
            window,
            surface: device_setup.surface,
            device: device_setup.device,
            queue: device_setup.queue,
            config: device_setup.config,
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
            pending_frame_view: None,
            frame_callback_raw: None,
            frame_callback_arc: None,
            skybox_pipeline: pipeline_setup.skybox_pipeline().clone(),
            skybox_vertex_buffer: resources.skybox_vertex_buffer,
            skybox_vertex_count: resources.skybox_vertex_count,
            shadow,
            ssao,
            depth_sampler,
            light_manager,
            dynamic_lights_buffer,
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

    /// Return the configured surface format for the renderer.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Update lighting parameters and write to GPU buffer.
    pub fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu) {
        self.shadow.current_lighting = lighting;
        self.queue
            .write_buffer(&self.lighting_buffer, 0, bytemuck::bytes_of(&lighting));
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

    /// Get mutable access to a light for detailed modifications.
    pub fn get_light_mut(&mut self, id: u32) -> Option<&mut crate::lights::Light> {
        self.light_manager.get_light_mut(id)
    }

    /// Get read-only access to a light.
    pub fn get_light(&self, id: u32) -> Option<&crate::lights::Light> {
        self.light_manager.get_light(id)
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

    /// Get the number of visible lights after frustum culling.
    pub fn visible_light_count(&self) -> usize {
        self.light_manager.visible_light_count()
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

    pub fn set_frame_callback_raw_inherent(&mut self, ptr: Option<*mut dyn crate::FrameCallback>) {
        self.frame_callback_raw = ptr;
    }

    pub fn set_frame_callback_arc_inherent(
        &mut self,
        cb: Option<std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
    ) {
        self.frame_callback_arc = cb;
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

        if let Some(ssao) = &mut self.ssao {
            ssao.resize(&self.device, width, height);
            self.recreate_camera_bind_group();
        }
    }

    fn recreate_camera_bind_group(&mut self) {
        let mat_buffer = match &self.material_buffer {
            Some(b) => b,
            None => {
                log::error!("material buffer missing when creating camera bind group");
                return;
            }
        };

        let (ssao_view, ssao_sampler) = if let Some(ssao) = &self.ssao {
            (ssao.blurred_ao_view(), &ssao.ao_sampler)
        } else {
            log::warn!("recreate_camera_bind_group called without SSAO system");
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
                usage: wgpu::BufferUsages::VERTEX,
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
        indices: &[u32],
    ) -> u32 {
        let mut iv: Vec<crate::types::Vertex> = Vec::with_capacity(vertices.len());
        for i in 0..vertices.len() {
            iv.push(crate::types::Vertex {
                position: vertices[i],
                normal: normals[i],
                ao: ao.get(i).copied().unwrap_or(1.0),
                geometry_type: geometry_type.get(i).copied().unwrap_or(0),
                light_level: light_level.get(i).copied().unwrap_or(1.0),
                block_light_rgb: block_light_rgb.get(i).copied().unwrap_or([0.0; 3]),
                sky_exposed: sky_exposed.get(i).copied().unwrap_or(1.0),
            });
        }
        for (i, v) in iv.iter().enumerate().take(6) {
            log::trace!(
                "[register_indexed_mesh] v{} pos=({:.3},{:.3},{:.3}) nor=({:.3},{:.3},{:.3})",
                i,
                v.position[0],
                v.position[1],
                v.position[2],
                v.normal[0],
                v.normal[1],
                v.normal[2]
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
                log::debug!(
                    "[register_indexed_mesh] sample v{} pos=({:.3},{:.3},{:.3}) nor=({:.3},{:.3},{:.3})",
                    idx,
                    v.position[0],
                    v.position[1],
                    v.position[2],
                    v.normal[0],
                    v.normal[1],
                    v.normal[2]
                );
            }
        }
        let vertex_bytes = bytemuck::cast_slice(&iv);
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh-vertex-buffer"),
                contents: vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX,
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
            vertex_count: vertices.len() as u32,
            index_buffer: Some(ib),
            index_count: indices.len() as u32,
        };
        let handle = self.mesh_table.len() as u32;
        self.mesh_table.push(Some(entry));
        handle
    }

    /// Unregister a mesh handle and free its GPU buffers.
    pub fn unregister_mesh(&mut self, mesh: u32) {
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
        if self.instance_capacity < instances_gpu.len().max(1) {
            let mut new_cap = self.instance_capacity.max(1);
            while new_cap < instances_gpu.len().max(1) {
                new_cap = new_cap.saturating_mul(2);
            }
            let size_bytes = (new_cap * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instance-buffer"),
                size: size_bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer = Some(buf);
            self.instance_capacity = new_cap;
        }

        let ibuf = self.instance_buffer.as_ref()?;
        if !instances_gpu.is_empty() {
            self.queue
                .write_buffer(ibuf, 0, bytemuck::cast_slice(instances_gpu));
        } else {
            let zero = GpuInstance {
                model: [[0.0; 4]; 4],
                material: 0,
                object_type: 0,
                padding: [0, 0],
            };
            self.queue
                .write_buffer(ibuf, 0, bytemuck::cast_slice(&[zero]));
        }

        Some(ibuf)
    }

    /// Begin a new frame: uploads the camera, culls/updates dynamic lights,
    /// updates shadow matrices, and acquires the swapchain surface texture.
    pub fn begin_frame(
        &mut self,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), crate::FrameError> {
        let (view_mat, proj_mat, cam_pos) = camera;
        crate::render_ops::camera_ops::update_camera_uniforms(
            &self.queue,
            &self.camera_buffer,
            view_mat,
            proj_mat,
            cam_pos,
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
            cam_pos,
        );

        if !self.acquire_render_target() {
            return Err(crate::FrameError::SurfaceUnavailable);
        }

        Ok(())
    }

    /// Queue a previously-registered mesh for drawing this frame.
    pub fn enqueue_draw(&mut self, mesh: u32, instances: &[moho_render_api::InstanceGpu]) {
        if !MeshRenderer::validate_mesh(mesh, &self.mesh_table) {
            return;
        }

        let instances_gpu = MeshRenderer::prepare_instances(instances);

        if self.prepare_instance_buffer(&instances_gpu).is_none() {
            log::error!("instance buffer missing when uploading instances");
            return;
        }

        self.pending_draws.push((mesh, instances_gpu));
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

    /// Flush all queued draws (shadow passes, main pass, SSAO) and present
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
            log::error!("instance buffer missing when uploading batched instances");
            return;
        }

        let ibuf = self.instance_buffer.as_ref().unwrap();
        let frame_view = self.pending_frame_view.as_ref().unwrap();

        crate::render_ops::shadow_ops::render_shadow_passes(
            &mut encoder,
            &self.shadow,
            ibuf,
            &self.pending_draws,
            &self.mesh_table,
            &offsets,
        );

        crate::render_ops::main_pass_ops::render_main_pass(
            &mut encoder,
            frame_view,
            &self.depth_texture_view,
            &self.skybox_pipeline,
            &self.skybox_vertex_buffer,
            self.skybox_vertex_count,
            &self.pipeline,
            &self.camera_bind_group,
            &self.shadow,
            ibuf,
            &self.pending_draws,
            &self.mesh_table,
            &offsets,
        );

        if let Some(ssao) = &self.ssao {
            ssao.compute_ao(
                &self.device,
                &mut encoder,
                &self.depth_texture_view,
                &self.depth_sampler,
            );
        }

        let frame_callback = if let Some(cb_arc) = &self.frame_callback_arc {
            crate::render_ops::frame_ops::FrameCallbackWrapper::Arc(cb_arc)
        } else if let Some(cb_ptr) = self.frame_callback_raw {
            crate::render_ops::frame_ops::FrameCallbackWrapper::Raw(cb_ptr)
        } else {
            crate::render_ops::frame_ops::FrameCallbackWrapper::None
        };

        let draw_count = self.pending_draws.len();
        crate::render_ops::frame_ops::finish_frame(
            encoder,
            &self.queue,
            self.pending_frame.take(),
            self.pending_frame_view.as_ref(),
            frame_callback,
            self.config.width,
            self.config.height,
            &self.device,
            draw_count,
        );

        self.pending_frame_view = None;
        self.pending_draws.clear();
    }
}
