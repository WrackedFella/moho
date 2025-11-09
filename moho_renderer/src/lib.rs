pub mod prelude {
    pub use crate::Renderer;
}

pub type TextureFormatRepr = wgpu::TextureFormat;

/// GPU material layout (32-byte stride for WGSL vec4 alignment)
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialGpu {
    pub albedo: [f32; 4],
    pub params: [f32; 4], // fuzz, ref_idx
}

/// Type alias for Material (same as MaterialGpu)
pub type Material = MaterialGpu;

impl MaterialGpu {
    pub fn is_transparent(&self) -> bool {
        self.params[2] > 0.0
    }
}

mod materials;
pub use materials::MaterialTable;
mod scene;
pub use scene::Scene;
mod gpu_types;
pub use gpu_types::{CameraGpu, CascadedShadowMatrixGpu, LightingGpu, ShadowMatrixGpu};
mod buffer_manager;
pub use buffer_manager::BufferManager;
mod instance_collector;
pub use instance_collector::InstanceCollector;

pub mod device;
mod render_ops;
mod shadow;
mod types;
pub use device::{DeviceInitError, DeviceSetup};
pub mod pipeline;
pub use pipeline::{PipelineInitError, PipelineSetup};
pub mod resources;
pub use resources::ResourcePool;
pub mod builder;
pub use builder::RendererBuilder;

pub mod gfx {

    pub mod wgpu_impl {
        extern crate winit;
        use crate::MaterialGpu;
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
            #[allow(dead_code)]
            vertex_buffer: Option<wgpu::Buffer>,
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
            #[allow(dead_code)]
            vertex_count: u32,
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
                Self {
                    window,
                    surface: device_setup.surface,
                    device: device_setup.device,
                    queue: device_setup.queue,
                    config: device_setup.config,
                    vertex_buffer: None,
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
                    vertex_count: 0,
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
                }
            }

            pub fn new(
                window: &'a winit::window::Window,
            ) -> Result<Self, Box<dyn std::error::Error>> {
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

            // Shadow calculation methods moved to shadow module
            /*
            fn calculate_shadow_matrix(&self, sun_dir: glam::Vec3, cam_pos: glam::Vec3) -> glam::Mat4 {
                // Normalize sun direction
                let light_dir = sun_dir.normalize();

                // Center shadow frustum on camera position (follow the view)
                // For isometric/high angle view, need to cover much larger area
                let scene_center = cam_pos;
                let light_distance = 300.0; // Far back to see large area
                let light_pos = scene_center - light_dir * light_distance;

                // Create light view matrix (looking from light toward camera/scene center)
                let light_view = glam::Mat4::look_at_rh(
                    light_pos,
                    scene_center,
                    glam::Vec3::Y, // Up vector
                );

                // Create orthographic projection for directional light
                // Optimized for 128x128 terrain (4 chunks of 64 units each)
                // Tighter frustum = better shadow map resolution
                let ortho_size = 100.0; // 200x200 unit coverage - fits terrain with margin
                let near = 1.0;
                let far = 400.0; // Deep enough to capture terrain depth

                let light_proj = glam::Mat4::orthographic_rh(
                    -ortho_size,
                    ortho_size,
                    -ortho_size,
                    ortho_size,
                    near,
                    far,
                );

                // Return light view-projection matrix
                light_proj * light_view
            }

            /// CSM: Calculate cascade frustum bounds in view space.
            /// Returns an array of (near, far) distances for each cascade split.
            fn calculate_cascade_splits(&self) -> [(f32, f32); NUM_SHADOW_CASCADES as usize] {
                let mut splits = [(0.0f32, 0.0f32); NUM_SHADOW_CASCADES as usize];

                // First cascade starts at near plane (very close to camera)
                splits[0] = (0.1, CASCADE_SPLIT_DISTANCES[0]);

                // Remaining cascades use configured split distances
                for i in 1..NUM_SHADOW_CASCADES as usize {
                    splits[i] = (
                        CASCADE_SPLIT_DISTANCES[i - 1],
                        CASCADE_SPLIT_DISTANCES[i],
                    );
                }

                // Log splits only once (on first call)
                if !self.csm_logged_once.get() {
                    log::info!("CSM cascade splits:");
                    for (i, (near, far)) in splits.iter().enumerate() {
                        log::info!("  Cascade {}: {:.1} -> {:.1} units", i, near, far);
                    }
                }

                splits
            }

            /// CSM: Calculate tight orthographic projection for a specific cascade.
            /// This computes a tight-fitting frustum around the visible geometry in the cascade slice.
            fn calculate_cascade_matrix(
                &self,
                cascade_idx: u32,
                light_dir: glam::Vec3,
                cam_pos: glam::Vec3,
                _near: f32,
                far: f32,
            ) -> glam::Mat4 {
                // For now, use a simple approach: expand ortho size based on cascade distance
                // More sophisticated approach would project view frustum corners into light space

                // Center the cascade frustum on camera position
                let cascade_center = cam_pos;

                // Position light far enough back to see the entire cascade range
                let light_distance = far * 2.0;
                let light_pos = cascade_center - light_dir * light_distance;

                // Create light view matrix
                let light_view = glam::Mat4::look_at_rh(
                    light_pos,
                    cascade_center,
                    glam::Vec3::Y,
                );

                // Calculate orthographic size based on cascade distance
                // Closer cascades need smaller frustums (higher resolution)
                // Further cascades need larger frustums (lower resolution)
                let cascade_radius = far * 1.5; // Generous coverage with margin

                // Create orthographic projection for this cascade
                let light_proj = glam::Mat4::orthographic_rh(
                    -cascade_radius,
                    cascade_radius,
                    -cascade_radius,
                    cascade_radius,
                    1.0, // Near plane in light space
                    light_distance + far, // Far plane to capture full depth
                );

                // Log matrix details only once (on first call)
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

            /// CSM: Calculate all cascade matrices based on sun direction and camera position.
            /// Returns an array of 4 matrices and the CascadedShadowMatrixGpu structure.
            fn calculate_cascade_matrices(
                &self,
                sun_dir: glam::Vec3,
                cam_pos: glam::Vec3,
            ) -> ([glam::Mat4; NUM_SHADOW_CASCADES as usize], crate::gpu_types::CascadedShadowMatrixGpu) {
                let light_dir = sun_dir.normalize();
                let splits = self.calculate_cascade_splits();

                // Log calculation details only once (on first call)
                if !self.csm_logged_once.get() {
                    log::info!("Calculating CSM cascade matrices for sun_dir={:?}, cam_pos={:?}", light_dir, cam_pos);
                }

                let mut matrices = [glam::Mat4::IDENTITY; NUM_SHADOW_CASCADES as usize];

                for i in 0..NUM_SHADOW_CASCADES as usize {
                    let (near, far) = splits[i];
                    matrices[i] = self.calculate_cascade_matrix(i as u32, light_dir, cam_pos, near, far);
                }

                // Convert to GPU structure
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

                // Mark that we've logged once and set the flag
                if !self.csm_logged_once.get() {
                    log::info!("CSM cascade matrices calculated successfully");
                    self.csm_logged_once.set(true);
                }

                (matrices, gpu_data)
            }
            */
            // END shadow calculation methods moved to shadow module

            // Note: The old render() method has been removed. Use render_mesh() instead.
            // Note: window-related helpers (request_redraw, cursor control)
            // are intentionally not exposed from the renderer. The
            // application owns the Window and should call those methods
            // directly to avoid renderer needing to keep a reference with
            // 'static lifetime or leaking the Window.

            /// Update the material table on the GPU. This replaces the storage
            /// buffer bound at @group(0) binding 1 and recreates the camera
            /// bind group so the pipeline sees the new buffer.
            pub fn set_material_table(&mut self, materials: &[MaterialGpu]) {
                if materials.is_empty() {
                    return;
                }
                let bytes = bytemuck::cast_slice(materials);
                let _size = bytes.len() as wgpu::BufferAddress;
                // Create a new storage buffer for materials and copy data into it.
                let mat_buf = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("material-buffer"),
                        contents: bytes,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
                self.material_buffer = Some(mat_buf);
                // Recreate the camera bind group to include the new material buffer.
                let mat_resource = match self.material_buffer.as_ref() {
                    Some(b) => b.as_entire_binding(),
                    None => {
                        log::error!("material buffer missing when creating bind group");
                        return;
                    }
                };
                self.camera_bind_group =
                    self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &self.camera_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: self.camera_buffer.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: mat_resource,
                            },
                        ],
                        label: Some("camera-bind-group"),
                    });
            }

            /// Inherent setter for the optional raw FrameCallback pointer.
            /// Placed here so it can access the private field directly.
            pub fn set_frame_callback_raw_inherent(
                &mut self,
                ptr: Option<*mut dyn crate::FrameCallback>,
            ) {
                self.frame_callback_raw = ptr;
            }
            /// Inherent setter for the optional Arc<Mutex<dyn FrameCallback>>.
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
                // recreate depth texture for new size
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
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                self.depth_texture_view =
                    depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
            }

            /// Inherent method: register a mesh into the renderer's mesh table.
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

            pub fn register_indexed_mesh(
                &mut self,
                vertices: &[[f32; 3]],
                normals: &[[f32; 3]],
                indices: &[u32],
            ) -> u32 {
                // Interleave positions and normals into the Vertex struct
                #[repr(C)]
                #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
                struct InterleavedVertex {
                    pos: [f32; 3],
                    nor: [f32; 3],
                }
                // Build a temporary vec of interleaved vertices
                let mut iv: Vec<InterleavedVertex> = Vec::with_capacity(vertices.len());
                for i in 0..vertices.len() {
                    iv.push(InterleavedVertex {
                        pos: vertices[i],
                        nor: normals[i],
                    });
                }
                // Debug: print first few interleaved vertices to ensure normals exist
                for (i, v) in iv.iter().enumerate().take(6) {
                    log::trace!(
                        "[register_indexed_mesh] v{} pos=({:.3},{:.3},{:.3}) nor=({:.3},{:.3},{:.3})",
                        i,
                        v.pos[0],
                        v.pos[1],
                        v.pos[2],
                        v.nor[0],
                        v.nor[1],
                        v.nor[2]
                    );
                }
                // Print a few sampled indices across the mesh to check variation
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
                            v.pos[0],
                            v.pos[1],
                            v.pos[2],
                            v.nor[0],
                            v.nor[1],
                            v.nor[2]
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

            /// Unregister a mesh handle and free its GPU buffers by dropping
            /// the mesh table entry. This is an inherent method so it can
            /// access private fields of `Renderer`.
            pub fn unregister_mesh(&mut self, mesh: u32) {
                let idx = mesh as usize;
                if idx < self.mesh_table.len() {
                    self.mesh_table[idx] = None;
                }
            }

            /// Render all pending draws into the 4 CSM (Cascaded Shadow Map) cascades.
            ///
            /// This creates a separate depth-only render pass for each cascade, using
            /// the shadow pipeline. Each cascade renders all pending meshes from the
            /// Acquire render target (surface texture view).
            /// Returns true if successful, false if reconfiguration is needed.
            fn acquire_render_target(&mut self) -> bool {
                if self.pending_frame_view.is_some() {
                    return true; // Already acquired
                }

                match self.surface.get_current_texture() {
                    Ok(f) => {
                        let view = f
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        self.pending_frame = Some(f);
                        self.pending_frame_view = Some(view);
                        true
                    }
                    Err(_) => {
                        self.surface.configure(&self.device, &self.config);
                        false
                    }
                }
            }

            /// Prepare instance buffer with provided instances.
            /// Ensures capacity, resizes if needed, and uploads instance data.
            /// Returns reference to the instance buffer.
            fn prepare_instance_buffer(
                &mut self,
                instances_gpu: &[GpuInstance],
            ) -> Option<&wgpu::Buffer> {
                // Ensure capacity
                if self.instance_capacity < instances_gpu.len().max(1) {
                    let mut new_cap = self.instance_capacity.max(1);
                    while new_cap < instances_gpu.len().max(1) {
                        new_cap = new_cap.saturating_mul(2);
                    }
                    let size_bytes =
                        (new_cap * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
                    let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("instance-buffer"),
                        size: size_bytes,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.instance_buffer = Some(buf);
                    self.instance_capacity = new_cap;
                }

                // Upload instance data
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

            /// Acquire render target (surface texture view).
            /// Returns true if successful, false if reconfiguration is needed.
            /// Inherent method: render a registered mesh by handle.
            pub fn render_mesh(
                &mut self,
                mesh: u32,
                instances: &[moho_core::actors::InstanceGpu],
                camera: (glam::Mat4, glam::Mat4, glam::Vec3),
                finalize: bool,
            ) {
                let idx = mesh as usize;
                if idx >= self.mesh_table.len() {
                    return;
                }
                if self.mesh_table[idx].is_none() {
                    return;
                }

                // Update camera uniforms (delegate to render_ops)
                let (view_mat, proj_mat, cam_pos) = camera;
                crate::render_ops::camera_ops::update_camera_uniforms(
                    &self.queue,
                    &self.camera_buffer,
                    view_mat,
                    proj_mat,
                    cam_pos,
                );

                // Update shadow matrices (delegate to render_ops)
                crate::render_ops::shadow_ops::update_shadow_matrices(
                    &mut self.shadow,
                    &self.queue,
                    cam_pos,
                );

                // Convert instances to GPU format
                let instances_gpu = self.convert_instances(instances);

                // Prepare and upload instance buffer
                if self.prepare_instance_buffer(&instances_gpu).is_none() {
                    log::error!("instance buffer missing when uploading instances");
                    return;
                }

                // Push this draw into pending_draws to batch multiple mesh draws
                self.pending_draws.push((mesh, instances_gpu));

                // Acquire frame if this is the first pending draw
                if !self.acquire_render_target() {
                    return; // Reconfiguration needed
                }

                // If finalize requested, record render pass for all pending draws
                if finalize {
                    self.finalize_frame();
                }
            }

            /// Convert instances from external format to internal GPU format.
            fn convert_instances(
                &self,
                instances: &[moho_core::actors::InstanceGpu],
            ) -> Vec<GpuInstance> {
                instances
                    .iter()
                    .map(|ic| GpuInstance {
                        model: ic.model,
                        material: ic.material,
                        object_type: ic.object_type,
                        padding: ic.padding,
                    })
                    .collect()
            }

            /// Finalize the current frame: flatten instances, upload to GPU, render passes, present.
            fn finalize_frame(&mut self) {
                // Ensure we have a frame view
                if self.pending_frame_view.is_none() {
                    return;
                }

                // Create encoder for batched render pass
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("batched-encoder"),
                        });

                // Flatten all instance lists into contiguous buffer with offsets
                let (all_instances, offsets) = self.flatten_instances();

                // Ensure capacity and upload all instances
                if !self.ensure_instance_capacity_and_upload(&all_instances) {
                    log::error!("instance buffer missing when uploading batched instances");
                    return;
                }

                // Get references we need for rendering (avoids borrow conflicts)
                let ibuf = self.instance_buffer.as_ref().unwrap();
                let frame_view = self.pending_frame_view.as_ref().unwrap();

                // Render all pending draws into CSM cascades (delegate to render_ops)
                crate::render_ops::shadow_ops::render_shadow_passes(
                    &mut encoder,
                    &self.shadow,
                    ibuf,
                    &self.pending_draws,
                    &self.mesh_table,
                    &offsets,
                );

                // Render main color pass with skybox and meshes (delegate to render_ops)
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

                // Finish rendering and present frame (delegate to render_ops)
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

                // Clear pending state for next frame
                self.pending_frame_view = None;
                self.pending_draws.clear();
            }

            /// Flatten all pending instance lists into a single contiguous buffer.
            /// Returns (all_instances, offsets) where offsets[i] is the start index for draw i.
            fn flatten_instances(&self) -> (Vec<GpuInstance>, Vec<usize>) {
                let mut all_instances: Vec<GpuInstance> = Vec::new();
                let mut offsets: Vec<usize> = Vec::with_capacity(self.pending_draws.len());
                for (_m, insts) in &self.pending_draws {
                    offsets.push(all_instances.len());
                    all_instances.extend_from_slice(insts);
                }
                (all_instances, offsets)
            }

            /// Ensure instance buffer has sufficient capacity and upload instances.
            /// Returns true if successful, false if buffer is missing.
            fn ensure_instance_capacity_and_upload(
                &mut self,
                all_instances: &[GpuInstance],
            ) -> bool {
                let required = all_instances.len().max(1);
                if self.instance_capacity < required {
                    let mut new_cap = self.instance_capacity.max(1);
                    while new_cap < required {
                        new_cap = new_cap.saturating_mul(2);
                    }
                    let size_bytes =
                        (new_cap * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
                    let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("instance-buffer"),
                        size: size_bytes,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.instance_buffer = Some(buf);
                    self.instance_capacity = new_cap;
                }

                let buffer = match self.instance_buffer.as_ref() {
                    Some(b) => b,
                    None => return false,
                };

                // Upload instances
                if !all_instances.is_empty() {
                    self.queue
                        .write_buffer(buffer, 0, bytemuck::cast_slice(all_instances));
                } else {
                    // Upload a zero instance to keep buffer valid
                    let zero = GpuInstance {
                        model: [[0.0; 4]; 4],
                        material: 0,
                        object_type: 0,
                        padding: [0, 0],
                    };
                    self.queue
                        .write_buffer(buffer, 0, bytemuck::cast_slice(&[zero]));
                }

                true
            }
        }
    }

    // re-export the concrete renderer at gfx level for convenience
    pub use wgpu_impl::Renderer;
}

// crate root re-export to preserve the previous `moho_renderer::Renderer` API
// Note: when the `backend-wgpu` feature is enabled the concrete type is
// `gfx::wgpu_impl::Renderer<'a>` which borrows a `&'a winit::window::Window`.
// Callers should create the renderer by passing a borrow of a Window that
// outlives the returned boxed trait object (the application typically keeps
// an `Arc<Window>` and passes `Some(&*arc_window)` to `create_renderer`).
pub use gfx::Renderer;

// --- NEW: Renderer trait and factory helpers ---------------------------------
// Provide a small object-safe trait so callers can depend on an abstraction
// rather than a concrete backend type. A factory function `create_renderer`
// selects an appropriate backend implementation depending on features.

/// Object-safe renderer backend trait.
///
/// Implementations are expected to map to a concrete backend. Note that the
/// WGPU backend currently requires a borrow of a `winit::window::Window` for
/// the lifetime of the renderer (see `create_renderer` under `backend-wgpu`).
/// The trait itself is object-safe so callers can hold `Box<dyn RendererBackend + '_>`.
pub trait RendererBackend {
    fn request_redraw(&self);
    fn resize(&mut self, width: u32, height: u32);
    /// Set whether the cursor is visible (for FPS-style hide/show).
    fn set_cursor_visible(&self, visible: bool);
    /// Request a specific cursor grab mode. Returns Ok(()) on success or Err(()) on failure.
    fn set_cursor_grab(&self, locked: bool) -> Result<(), Box<dyn std::error::Error>>;
    /// Register a mesh represented by an array of positions. Returns a handle
    /// that can be used with `render_mesh` to render that mesh without
    /// re-supplying the vertex data every frame.
    fn register_mesh(&mut self, vertices: &[[f32; 3]]) -> u32;
    /// Register a mesh with an index buffer. `indices` are 32-bit indices.
    fn register_indexed_mesh(
        &mut self,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
    ) -> u32;
    /// Unregister a previously-registered mesh handle and free GPU resources.
    fn unregister_mesh(&mut self, mesh: u32);
    /// Render a previously-registered mesh by handle using the provided
    /// instances and camera.
    fn render_mesh(
        &mut self,
        mesh: u32,
        instances: &[moho_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        finalize: bool,
    );
    /// Replace the material table on the GPU. The caller should prepare a
    /// slice of `MaterialGpu` values describing each distinct material.
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]);
    /// Update lighting parameters (sun, moon, ambient) and write to GPU buffer.
    fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu);
    /// Return the surface texture format used by the renderer (if applicable).
    /// This is useful for UI integrations that need to create GPU pipelines
    /// with the same format as the swapchain.
    fn surface_format(&self) -> Option<TextureFormatRepr> {
        None
    }
    /// Set an optional raw FrameCallback pointer. The renderer will call the
    /// callback during finalization so the application can record UI commands
    /// into the frame encoder. The pointer must remain valid until cleared.
    fn set_frame_callback_raw(&mut self, ptr: Option<*mut dyn FrameCallback>);
    /// Set an optional safe Arc<Mutex<dyn FrameCallback>>. Prefer this
    /// registration method when possible; it's thread-safe and avoids raw
    /// pointer lifetime issues. Passing `None` clears the registration.
    fn set_frame_callback_arc(
        &mut self,
        cb: Option<std::sync::Arc<std::sync::Mutex<dyn FrameCallback>>>,
    );
}

impl<'a> RendererBackend for gfx::wgpu_impl::Renderer<'a> {
    fn request_redraw(&self) {
        // Renderer no longer owns the Window; request_redraw must be
        // performed by the application via the Window instance.
        // Keep this no-op to satisfy trait but prefer calling Window directly.
    }
    fn resize(&mut self, width: u32, height: u32) {
        gfx::wgpu_impl::Renderer::resize(self, width, height)
    }
    fn register_mesh(&mut self, vertices: &[[f32; 3]]) -> u32 {
        gfx::wgpu_impl::Renderer::register_mesh(self, vertices)
    }
    fn register_indexed_mesh(
        &mut self,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u32],
    ) -> u32 {
        gfx::wgpu_impl::Renderer::register_indexed_mesh(self, vertices, normals, indices)
    }
    fn unregister_mesh(&mut self, mesh: u32) {
        gfx::wgpu_impl::Renderer::unregister_mesh(self, mesh)
    }
    fn render_mesh(
        &mut self,
        mesh: u32,
        instances: &[moho_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        finalize: bool,
    ) {
        gfx::wgpu_impl::Renderer::render_mesh(self, mesh, instances, camera, finalize)
    }
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]) {
        gfx::wgpu_impl::Renderer::set_material_table(self, materials)
    }
    fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu) {
        gfx::wgpu_impl::Renderer::update_lighting(self, lighting)
    }
    fn surface_format(&self) -> Option<TextureFormatRepr> {
        Some(self.surface_format())
    }
    fn set_frame_callback_raw(&mut self, ptr: Option<*mut dyn FrameCallback>) {
        self.set_frame_callback_raw_inherent(ptr);
    }
    fn set_frame_callback_arc(
        &mut self,
        cb: Option<std::sync::Arc<std::sync::Mutex<dyn FrameCallback>>>,
    ) {
        self.set_frame_callback_arc_inherent(cb);
    }
    fn set_cursor_visible(&self, _visible: bool) {
        // no-op: application should control the Window cursor visibility
    }
    fn set_cursor_grab(&self, _locked: bool) -> Result<(), Box<dyn std::error::Error>> {
        // no-op: application should control cursor grab on the Window
        Ok(())
    }
}

/// Create a boxed renderer backend. Always returns a Result so callers have a
/// single, fallible API to initialize a renderer.
pub fn create_renderer<'a>(
    window: Option<&'a winit::window::Window>,
) -> Result<Box<dyn RendererBackend + 'a>, Box<dyn std::error::Error>> {
    let win = window
        .ok_or_else(|| Box::new(RendererInitError::MissingWindow) as Box<dyn std::error::Error>)?;
    let r = gfx::wgpu_impl::Renderer::new(win).map_err(|e| {
        let msg = format!("{}", e);
        Box::new(RendererInitError::WgpuInit(msg)) as Box<dyn std::error::Error>
    })?;
    Ok(Box::new(r))
}

/// Convenience helper: create a renderer from an Arc<Window>.
///
/// The renderer implementation currently borrows the provided `Window` for
/// the lifetime of the returned trait object. Callers typically keep an
/// `Arc<winit::window::Window>` (or otherwise own the Window) and pass a
/// reference to that Arc here so the application retains ownership while
/// the renderer uses a borrow.
///
/// Note: this helper intentionally takes `&Arc<...>` rather than consuming
/// the Arc. The caller retains ownership and is responsible for ensuring
/// the Arc (and the underlying Window) outlives the renderer.
pub fn create_renderer_from_arc<'a>(
    window: &'a std::sync::Arc<winit::window::Window>,
) -> Result<Box<dyn RendererBackend + 'a>, Box<dyn std::error::Error>> {
    create_renderer(Some(std::sync::Arc::as_ref(window)))
}

/// Callback trait for UI rendering. Implement this to composite UI elements
/// into the renderer's command encoder.
pub trait FrameCallback {
    fn call(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        surface_width: u32,
        surface_height: u32,
    );
}

/// Errors that can occur during renderer initialization.
#[derive(thiserror::Error, Debug)]
pub enum RendererInitError {
    #[error("missing window for wgpu backend initialization")]
    MissingWindow,
    #[error("wgpu backend error: {0}")]
    WgpuInit(String),
}
