// Renderer crate extracted from the main binary to provide a reusable renderer
// API. The WGPU implementation has been migrated here and organized under a
// `gfx` module as requested. The crate re-exports a `Renderer` type at the
// root so callers can continue to use `engine_renderer::Renderer`.

pub mod prelude {
    pub use crate::Renderer;
}

// Re-export GPU ABI types from a single module so other crates can import
// a stable definition and we avoid duplicate layout definitions across the
// workspace.
pub mod gpu_types;
pub use gpu_types::CameraGpu;
pub use gpu_types::MaterialGpu;

// Quick sanity check: MaterialGpu should be 32 bytes (two vec4s).
const _: () = assert!(std::mem::size_of::<MaterialGpu>() == 32);

pub mod gfx {
    //! Graphics backends grouped under `gfx` for clarity. The WGPU backend is
    //! feature-gated behind `backend-wgpu`.

    #[cfg(feature = "backend-wgpu")]
    pub mod wgpu_impl {
        // Migrated WGPU implementation (was previously in `src/gpu.rs`). Paths
        // to assets/shaders are adjusted for the crate layout.
        use engine_core::actors::InstanceGpu as CpuInstance;
        use wgpu::util::DeviceExt;

        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct Vertex {
            position: [f32; 3],
            normal: [f32; 3],
        }

        // GPU-side per-instance layout: model matrix + material index + object type
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct GpuInstance {
            model: [[f32; 4]; 4],
            material: u32,
            object_type: u32,
            padding: [u32; 2],
        }

        // Use the crate-level MaterialGpu type for the GPU material layout.
        use crate::MaterialGpu;

        pub struct Renderer {
            surface: wgpu::Surface,
            device: wgpu::Device,
            queue: wgpu::Queue,
            config: wgpu::SurfaceConfiguration,
            vertex_buffer: Option<wgpu::Buffer>,
            pipeline: wgpu::RenderPipeline,
            camera_buffer: wgpu::Buffer,
            camera_bind_group: wgpu::BindGroup,
            // keep the bind group layout so we can recreate the bind group when
            // the material storage buffer changes size.
            camera_bind_group_layout: wgpu::BindGroupLayout,
            material_buffer: Option<wgpu::Buffer>,
            _depth_texture: wgpu::Texture,
            depth_texture_view: wgpu::TextureView,
            depth_format: wgpu::TextureFormat,
            instance_buffer: Option<wgpu::Buffer>,
            instance_capacity: usize,
            vertex_count: u32,
            window: Option<winit::window::Window>,
            // mesh table stores optional mesh entries for registered meshes
            mesh_table: Vec<Option<MeshEntry>>,
        }

        // Per-mesh stored data (supports optional index buffer)
        pub struct MeshEntry {
            pub buffer: wgpu::Buffer,
            pub vertex_count: u32,
            pub index_buffer: Option<wgpu::Buffer>,
            pub index_count: u32,
        }

        impl Renderer {
            pub fn new(
                _event_loop: &winit::event_loop::EventLoop<()>,
                window: winit::window::Window,
            ) -> Self {
                log::info!("(wgpu) Initializing renderer (instanced cubes)");
                let size = window.inner_size();
                // Initialize wgpu
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::all(),
                    dx12_shader_compiler: Default::default(),
                });
                let surface = unsafe { instance.create_surface(&window) }.expect("create_surface");
                let adapter =
                    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                    }))
                    .expect("Failed to find an adapter");
                let (device, queue) = pollster::block_on(adapter.request_device(
                    &wgpu::DeviceDescriptor {
                        features: wgpu::Features::empty(),
                        limits: wgpu::Limits::default(),
                        label: None,
                    },
                    None,
                ))
                .expect("Failed to create device");

                let supported_formats = surface.get_capabilities(&adapter).formats;
                let config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: supported_formats[0],
                    width: size.width,
                    height: size.height,
                    present_mode: wgpu::PresentMode::Fifo,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![],
                };
                surface.configure(&device, &config);

                // Vertex data is supplied by the application (from the World) at render time.
                // The renderer will create/update the GPU vertex buffer on demand in
                // `render()` so no mesh is hardcoded here.
                let vertex_buffer = None;
                let vertex_count = 0u32;

                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("shader"),
                    // shader path adjusted for crate layout (engine_renderer/src -> repo root)
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("../../shaders/instance.wgsl").into(),
                    ),
                });
                // Camera uniform bind group (group 0) now contains both the camera
                // uniform (binding 0) and a storage buffer with the material table
                // world position.
                let camera_size = std::mem::size_of::<[f32; 20]>() as u64; // 5 vec4s
                let camera_bgl =
                    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("camera-bgl"),
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                // camera is read in both the vertex and fragment stages
                                visibility: wgpu::ShaderStages::VERTEX
                                    | wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Uniform,
                                    has_dynamic_offset: false,
                                    min_binding_size: Some(
                                        std::num::NonZeroU64::new(camera_size).unwrap(),
                                    ),
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
                        ],
                    });

                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("pipeline-layout"),
                        bind_group_layouts: &[&camera_bgl],
                        push_constant_ranges: &[],
                    });

                // Create camera uniform buffer (mat4x4<f32> + cam_pos vec4)
                let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("camera-buffer"),
                    size: camera_size as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

                // Create an initial one-element material storage buffer so we can
                // create the bind group now. It will be replaced when the app
                // uploads real materials.
                let initial_material = MaterialGpu {
                    albedo: [1.0, 1.0, 1.0, 0.0],
                    params: [0.0, 0.0, 0.0, 0.0],
                };
                let material_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("material-buffer-initial"),
                        contents: bytemuck::cast_slice(&[initial_material]),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });

                let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &camera_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: camera_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: material_buffer.as_entire_binding(),
                        },
                    ],
                    label: Some("camera-bind-group"),
                });

                // Create a depth texture
                let depth_format = wgpu::TextureFormat::Depth24Plus;
                let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("depth-texture"),
                    size: wgpu::Extent3d {
                        width: config.width,
                        height: config.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: depth_format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("render-pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: "vs_main",
                        buffers: &[
                            // Vertex positions + normals
                            wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![
                                    0 => Float32x3,
                                    1 => Float32x3,
                                ],
                            },
                            // Per-instance data: model matrix (4x vec4) + material(u32) + object_type(u32)
                            // followed by per-instance material params: albedo(vec3), fuzz(f32), ref_idx(f32)
                            wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<GpuInstance>()
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
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: "fs_main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: depth_format,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                });

                // create a tiny initial instance buffer (1 element) to avoid special cases
                let initial_instance = GpuInstance {
                    model: [[0.0; 4]; 4],
                    material: 0,
                    object_type: 0,
                    padding: [0, 0],
                };
                let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("instance-buffer-initial"),
                    contents: bytemuck::cast_slice(&[initial_instance]),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });

                Renderer {
                    surface,
                    device,
                    queue,
                    config,
                    vertex_buffer,
                    pipeline,
                    camera_buffer,
                    camera_bind_group,
                    camera_bind_group_layout: camera_bgl,
                    material_buffer: Some(material_buffer),
                    _depth_texture: depth_texture,
                    depth_texture_view: depth_view,
                    depth_format,
                    instance_buffer: Some(instance_buf),
                    instance_capacity: 1,
                    vertex_count,
                    window: Some(window),
                    mesh_table: Vec::new(),
                }
            }

            pub fn render(
                &mut self,
                vertices: &[[f32; 3]],
                instances_cpu: &[CpuInstance],
                camera: (glam::Mat4, glam::Mat4, glam::Vec3),
            ) {
                // Ensure GPU vertex buffer exists and matches the provided vertices.
                let vertex_bytes = bytemuck::cast_slice(vertices);
                let mut recreate_vertex = false;
                if self.vertex_buffer.is_none() {
                    recreate_vertex = true;
                } else if self.vertex_count as usize != vertices.len() {
                    // If the incoming vertex count changed, recreate the buffer to match
                    // (could alternatively allow partial updates, but recreating is
                    // simpler and sufficient for now).
                    recreate_vertex = true;
                }
                if recreate_vertex {
                    let vb = self
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("vertex-buffer"),
                            contents: vertex_bytes,
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                    self.vertex_buffer = Some(vb);
                    self.vertex_count = vertices.len() as u32;
                }

                // Convert CPU-side `InstanceGpu` into the tightly-packed GPU layout used by the shader.
                let mut instances: Vec<GpuInstance> = Vec::with_capacity(instances_cpu.len());
                for ic in instances_cpu {
                    instances.push(GpuInstance {
                        model: ic.model,
                        material: ic.material,
                        object_type: ic.object_type,
                        padding: ic.padding,
                    });
                }

                let frame = match self.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(e) => match e {
                        wgpu::SurfaceError::Lost => {
                            self.surface.configure(&self.device, &self.config);
                            return;
                        }
                        wgpu::SurfaceError::OutOfMemory => panic!("Out of memory"),
                        _ => return,
                    },
                };
                let frame_view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Update camera uniform buffer from provided camera (view, proj)
                let view_mat = camera.0;
                let proj_mat = camera.1;
                let cam_pos = camera.2;
                let viewproj = proj_mat * view_mat;
                let mut cols = viewproj.to_cols_array().to_vec();
                // append camera position as a vec4 (x,y,z,0)
                cols.push(cam_pos.x);
                cols.push(cam_pos.y);
                cols.push(cam_pos.z);
                cols.push(0.0f32);
                // write camera matrix + cam pos to GPU
                self.queue
                    .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&cols));

                // Update or create a persistent instance buffer using exponential growth
                let _instance_stride = std::mem::size_of::<GpuInstance>() as wgpu::BufferAddress;
                let required_count = instances.len().max(1);
                if self.instance_capacity < required_count {
                    // exponential grow: double until capacity >= required_count
                    let mut new_cap = self.instance_capacity.max(1);
                    while new_cap < required_count {
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

                // Write only the used portion of the instance buffer
                let buf = self
                    .instance_buffer
                    .as_ref()
                    .expect("instance buffer was created in new");
                if instances.len() > 0 {
                    self.queue
                        .write_buffer(buf, 0, bytemuck::cast_slice(&instances));
                } else {
                    let zero = GpuInstance {
                        model: [[0.0; 4]; 4],
                        material: 0,
                        object_type: 0,
                        padding: [0, 0],
                    };
                    self.queue
                        .write_buffer(buf, 0, bytemuck::cast_slice(&[zero]));
                }

                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("encoder"),
                        });

                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("rpass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &frame_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_texture_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });
                    rpass.set_pipeline(&self.pipeline);
                    // set camera bind group (group 0)
                    rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                    // set vertex buffer (created on-demand)
                    rpass.set_vertex_buffer(
                        0,
                        self.vertex_buffer
                            .as_ref()
                            .expect("vertex buffer")
                            .slice(..),
                    );
                    // bind the persistent instance buffer
                    let ibuf = self
                        .instance_buffer
                        .as_ref()
                        .expect("instance buffer present");
                    rpass.set_vertex_buffer(1, ibuf.slice(..));
                    let instance_count = instances.len().max(1) as u32;
                    rpass.draw(0..self.vertex_count, 0..instance_count);
                }

                self.queue.submit(Some(encoder.finish()));
                frame.present();
            }

            pub fn request_redraw(&self) {
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            /// Update the material table on the GPU. This replaces the storage
            /// buffer bound at @group(0) binding 1 and recreates the camera
            /// bind group so the pipeline sees the new buffer.
            pub fn set_material_table(&mut self, materials: &[MaterialGpu]) {
                if materials.len() == 0 {
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
                let mat_resource = self.material_buffer.as_ref().unwrap().as_entire_binding();
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
                    log::debug!(
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

            /// Inherent method: render a registered mesh by handle.
            pub fn render_mesh(
                &mut self,
                mesh: u32,
                instances: &[engine_core::actors::InstanceGpu],
                camera: (glam::Mat4, glam::Mat4, glam::Vec3),
            ) {
                let idx = mesh as usize;
                if idx >= self.mesh_table.len() {
                    return;
                }
                if let Some(me) = &self.mesh_table[idx] {
                    // update camera
                    let view_mat = camera.0;
                    let proj_mat = camera.1;
                    let cam_pos = camera.2;
                    let viewproj = proj_mat * view_mat;
                    let mut cols = viewproj.to_cols_array().to_vec();
                    cols.push(cam_pos.x);
                    cols.push(cam_pos.y);
                    cols.push(cam_pos.z);
                    cols.push(0.0f32);
                    self.queue
                        .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&cols));

                    // instances
                    let mut instances_gpu: Vec<GpuInstance> = Vec::with_capacity(instances.len());
                    for ic in instances {
                        instances_gpu.push(GpuInstance {
                            model: ic.model,
                            material: ic.material,
                            object_type: ic.object_type,
                            padding: ic.padding,
                        });
                    }

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

                    let ibuf = self.instance_buffer.as_ref().unwrap();
                    if instances_gpu.len() > 0 {
                        self.queue
                            .write_buffer(ibuf, 0, bytemuck::cast_slice(&instances_gpu));
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

                    let frame = match self.surface.get_current_texture() {
                        Ok(f) => f,
                        Err(_) => {
                            self.surface.configure(&self.device, &self.config);
                            return;
                        }
                    };
                    let frame_view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder =
                        self.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("encoder"),
                            });
                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("rpass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &frame_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: true,
                                },
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &self.depth_texture_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: true,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                        });
                        rpass.set_pipeline(&self.pipeline);
                        rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                        rpass.set_vertex_buffer(0, me.buffer.slice(..));
                        rpass.set_vertex_buffer(1, ibuf.slice(..));
                        let instance_count = instances_gpu.len().max(1) as u32;
                        if let Some(idx_buf) = &me.index_buffer {
                            rpass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                            rpass.draw_indexed(0..me.index_count, 0, 0..instance_count);
                        } else {
                            rpass.draw(0..me.vertex_count, 0..instance_count);
                        }
                    }
                    self.queue.submit(Some(encoder.finish()));
                    frame.present();
                }
            }
        }
    }

    #[cfg(not(feature = "backend-wgpu"))]
    pub mod placeholder {
        use engine_core::actors::InstanceGpu;
        pub struct Renderer {}
        impl Renderer {
            pub fn new() -> Self {
                Renderer {}
            }
            pub fn render(
                &mut self,
                _vertices: &[[f32; 3]],
                _instances: &[InstanceGpu],
                _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
            ) {
            }
            pub fn request_redraw(&self) {}
            pub fn resize(&mut self, _w: u32, _h: u32) {}
        }
    }

    // re-export the concrete renderer at gfx level for convenience
    #[cfg(not(feature = "backend-wgpu"))]
    pub use placeholder::Renderer;
    #[cfg(feature = "backend-wgpu")]
    pub use wgpu_impl::Renderer;
}

// crate root re-export to preserve the previous `engine_renderer::Renderer` API
pub use gfx::Renderer;

// --- NEW: Renderer trait and factory helpers ---------------------------------
// Provide a small object-safe trait so callers can depend on an abstraction
// rather than a concrete backend type. A factory function `create_renderer`
// selects an appropriate backend implementation depending on features.

/// Object-safe renderer backend trait.
pub trait RendererBackend {
    fn render(
        &mut self,
        vertices: &[[f32; 3]],
        instances: &[engine_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    );
    fn request_redraw(&self);
    fn resize(&mut self, width: u32, height: u32);
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
        instances: &[engine_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    );
    /// Replace the material table on the GPU. The caller should prepare a
    /// slice of `MaterialGpu` values describing each distinct material.
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]);
}

#[cfg(feature = "backend-wgpu")]
impl RendererBackend for gfx::wgpu_impl::Renderer {
    fn render(
        &mut self,
        vertices: &[[f32; 3]],
        instances: &[engine_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        gfx::wgpu_impl::Renderer::render(self, vertices, instances, camera)
    }
    fn request_redraw(&self) {
        gfx::wgpu_impl::Renderer::request_redraw(self)
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
        instances: &[engine_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        gfx::wgpu_impl::Renderer::render_mesh(self, mesh, instances, camera)
    }
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]) {
        gfx::wgpu_impl::Renderer::set_material_table(self, materials)
    }
}

#[cfg(not(feature = "backend-wgpu"))]
impl RendererBackend for gfx::placeholder::Renderer {
    fn render(
        &mut self,
        vertices: &[[f32; 3]],
        instances: &[engine_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        gfx::placeholder::Renderer::render(self, vertices, instances, camera)
    }
    fn request_redraw(&self) {
        gfx::placeholder::Renderer::request_redraw(self)
    }
    fn resize(&mut self, width: u32, height: u32) {
        gfx::placeholder::Renderer::resize(self, width, height)
    }
    fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
        // placeholder: no GPU, just return a constant handle (0)
        0
    }
    fn unregister_mesh(&mut self, _mesh: u32) {}
    fn render_mesh(
        &mut self,
        _mesh: u32,
        _instances: &[engine_core::actors::InstanceGpu],
        _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        // no-op in placeholder
    }
    fn register_indexed_mesh(
        &mut self,
        _vertices: &[[f32; 3]],
        _normals: &[[f32; 3]],
        _indices: &[u32],
    ) -> u32 {
        0
    }
    fn set_materials(&mut self, _materials: &[crate::MaterialGpu]) {}
}

/// Create a boxed renderer backend. When `backend-wgpu` is enabled the
/// function takes the `EventLoop` and `Window` so the backend can create a
/// surface. When disabled the parameterless form is provided.
#[cfg(feature = "backend-wgpu")]
pub fn create_renderer(
    event_loop: &winit::event_loop::EventLoop<()>,
    window: winit::window::Window,
) -> Box<dyn RendererBackend> {
    Box::new(gfx::wgpu_impl::Renderer::new(event_loop, window))
}

#[cfg(not(feature = "backend-wgpu"))]
pub fn create_renderer() -> Box<dyn RendererBackend> {
    Box::new(gfx::placeholder::Renderer::new())
}
