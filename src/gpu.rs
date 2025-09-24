// GPU renderer abstraction. Provides a `Renderer` type backed by either a
// lightweight placeholder implementation for fast iteration, or a feature-
// gated graphics backend (e.g. a WGPU backend) selectable via Cargo features.

#[cfg(not(feature = "backend-wgpu"))]
pub mod placeholder_renderer {
    use engine_core::actors::InstanceGpu;
    use legion::World;
    use legion::query::IntoQuery;

    /// Minimal placeholder renderer used when a GPU backend feature is disabled.
    /// It implements the same `new` and `render` surface so `main` can be kept
    /// feature-agnostic.
    pub struct Renderer {}

    impl Renderer {
        pub fn new() -> Self {
            println!("Using placeholder renderer (no GPU backend).");
            Renderer {}
        }

        pub fn render(&mut self, _world: &mut World, _camera: (glam::Mat4, glam::Mat4)) {
            // For now do nothing; we'll log instance count for visibility.
            let mut instances: Vec<InstanceGpu> = Vec::new();
            let mut q = <&engine_core::actors::Sphere>::query();
            for s in q.iter(_world) {
                instances.push(s.to_instance());
            }
            println!("Placeholder render called ({} instances).", instances.len());
        }
    }

    // Re-export is done at the crate root; no local re-export needed here.
}


// Top-level WGPU backend module (feature: `backend-wgpu`). This is a
// dedicated feature so we avoid confusion with the crate-level dependency
// name `wgpu` and to make feature selection explicit in `cargo`.
#[cfg(feature = "backend-wgpu")]
mod wgpu_impl {
    // ...existing code...
    use legion::World;
    use legion::query::IntoQuery;

    // no extra imports needed

    use wgpu::util::DeviceExt;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex { position: [f32; 3] }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct GpuInstance { model: [[f32;4];4], material: u32, object_type: u32, padding: [u32;2] }

    pub struct Renderer {
        surface: wgpu::Surface,
        device: wgpu::Device,
        queue: wgpu::Queue,
        config: wgpu::SurfaceConfiguration,
        vertex_buffer: wgpu::Buffer,
        pipeline: wgpu::RenderPipeline,
        camera_buffer: wgpu::Buffer,
        camera_bind_group: wgpu::BindGroup,
        _depth_texture: wgpu::Texture,
        depth_texture_view: wgpu::TextureView,
        depth_format: wgpu::TextureFormat,
        instance_buffer: Option<wgpu::Buffer>,
        instance_capacity: usize,
        vertex_count: u32,
        window: Option<winit::window::Window>,
    }

    impl Renderer {
    pub fn new(_event_loop: &winit::event_loop::EventLoop<()>, window: winit::window::Window) -> Self {
            println!("(wgpu) Initializing renderer (instanced cubes)");
            let size = window.inner_size();
            // Initialize wgpu
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), dx12_shader_compiler: Default::default() });
            let surface = unsafe { instance.create_surface(&window) }.expect("create_surface");
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })).expect("Failed to find an adapter");
            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                }, None
            )).expect("Failed to create device");

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

            // Full cube (36 vertices: 6 faces * 2 triangles * 3 vertices)
            let vertices: &[Vertex] = &[
                // front
                Vertex{ position: [-0.5, -0.5,  0.5] }, Vertex{ position: [ 0.5, -0.5,  0.5] }, Vertex{ position: [ 0.5,  0.5,  0.5] },
                Vertex{ position: [-0.5, -0.5,  0.5] }, Vertex{ position: [ 0.5,  0.5,  0.5] }, Vertex{ position: [-0.5,  0.5,  0.5] },
                // back
                Vertex{ position: [ 0.5, -0.5, -0.5] }, Vertex{ position: [-0.5, -0.5, -0.5] }, Vertex{ position: [-0.5,  0.5, -0.5] },
                Vertex{ position: [ 0.5, -0.5, -0.5] }, Vertex{ position: [-0.5,  0.5, -0.5] }, Vertex{ position: [ 0.5,  0.5, -0.5] },
                // left
                Vertex{ position: [-0.5, -0.5, -0.5] }, Vertex{ position: [-0.5, -0.5,  0.5] }, Vertex{ position: [-0.5,  0.5,  0.5] },
                Vertex{ position: [-0.5, -0.5, -0.5] }, Vertex{ position: [-0.5,  0.5,  0.5] }, Vertex{ position: [-0.5,  0.5, -0.5] },
                // right
                Vertex{ position: [ 0.5, -0.5,  0.5] }, Vertex{ position: [ 0.5, -0.5, -0.5] }, Vertex{ position: [ 0.5,  0.5, -0.5] },
                Vertex{ position: [ 0.5, -0.5,  0.5] }, Vertex{ position: [ 0.5,  0.5, -0.5] }, Vertex{ position: [ 0.5,  0.5,  0.5] },
                // top
                Vertex{ position: [-0.5,  0.5,  0.5] }, Vertex{ position: [ 0.5,  0.5,  0.5] }, Vertex{ position: [ 0.5,  0.5, -0.5] },
                Vertex{ position: [-0.5,  0.5,  0.5] }, Vertex{ position: [ 0.5,  0.5, -0.5] }, Vertex{ position: [-0.5,  0.5, -0.5] },
                // bottom
                Vertex{ position: [-0.5, -0.5, -0.5] }, Vertex{ position: [ 0.5, -0.5, -0.5] }, Vertex{ position: [ 0.5, -0.5,  0.5] },
                Vertex{ position: [-0.5, -0.5, -0.5] }, Vertex{ position: [ 0.5, -0.5,  0.5] }, Vertex{ position: [-0.5, -0.5,  0.5] },
            ];

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertex-buffer"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let vertex_count = vertices.len() as u32;

                // No bind group layouts for now (we don't use additional uniforms yet)
                // let instance_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                //     entries: &[],
                //     label: Some("instance-bgl"),
                // });

            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/instance.wgsl").into()),
            });
            // Camera uniform bind group (group 0, binding 0)
            let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                }],
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pipeline-layout"),
                bind_group_layouts: &[&camera_bgl],
                push_constant_ranges: &[],
            });

            // Create camera uniform buffer (mat4x4<f32>)
            let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("camera-buffer"),
                size: std::mem::size_of::<[f32; 16]>() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &camera_bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }],
                label: Some("camera-bind-group"),
            });

            // Create a depth texture
            let depth_format = wgpu::TextureFormat::Depth24Plus;
            let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth-texture"),
                size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
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
                        // Vertex positions
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                        },
                        // Per-instance data: model matrix (4x vec4) + material(u32) + object_type(u32)
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<GpuInstance>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![
                                1 => Float32x4,
                                2 => Float32x4,
                                3 => Float32x4,
                                4 => Float32x4,
                                5 => Uint32,
                                6 => Uint32,
                            ],
                        }
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
            let initial_instance = GpuInstance { model: [[0.0;4];4], material: 0, object_type: 0, padding: [0,0] };
            let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("instance-buffer-initial"),
                contents: bytemuck::cast_slice(&[initial_instance]),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

            Renderer { surface, device, queue, config, vertex_buffer, pipeline, camera_buffer, camera_bind_group, _depth_texture: depth_texture, depth_texture_view: depth_view, depth_format, instance_buffer: Some(instance_buf), instance_capacity: 1, vertex_count, window: Some(window) }
        }

            pub fn render(&mut self, world: &mut World, camera: (glam::Mat4, glam::Mat4)) {
                // Build instance list
                    let mut instances: Vec<GpuInstance> = Vec::new();
                    let mut q = <&engine_core::actors::Sphere>::query();
                    for s in q.iter(world) { instances.push(GpuInstance{ model: s.to_instance().model, material: s.to_instance().material, object_type: 0, padding: [0,0] }); }

                let frame = match self.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(e) => {
                        match e {
                            wgpu::SurfaceError::Lost => { self.surface.configure(&self.device, &self.config); return; }
                            wgpu::SurfaceError::OutOfMemory => panic!("Out of memory"),
                            _ => return,
                        }
                    }
                };
            let frame_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

                // Update camera uniform buffer from provided camera (view, proj)
                let view_mat = camera.0;
                let proj_mat = camera.1;
                let viewproj = proj_mat * view_mat;
                let cols = viewproj.to_cols_array();
                // write camera matrix to GPU
                self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&cols));

                // Update or create a persistent instance buffer
                let instance_stride = std::mem::size_of::<GpuInstance>() as wgpu::BufferAddress;
                let needed = (instances.len().max(1) * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
                if self.instance_capacity < instances.len().max(1) {
                    // allocate a new GPU buffer with COPY_DST so we can update it
                    let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("instance-buffer"),
                        size: needed,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    self.instance_buffer = Some(buf);
                    self.instance_capacity = instances.len().max(1);
                }

                // Write instance data into the persistent buffer (we create an initial buffer in new())
                let buf = self.instance_buffer.as_ref().expect("instance buffer was created in new");
                if instances.len() > 0 {
                    self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&instances));
                } else {
                    let zero = GpuInstance { model: [[0.0;4];4], material: 0, object_type: 0, padding: [0,0] };
                    self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&[zero]));
                }

                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("encoder") });

            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("rpass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: true },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_texture_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: true }),
                        stencil_ops: None,
                    }),
                });
                    rpass.set_pipeline(&self.pipeline);
                    // set camera bind group (group 0)
                    rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    // bind the persistent instance buffer
                    let ibuf = self.instance_buffer.as_ref().expect("instance buffer present");
                    rpass.set_vertex_buffer(1, ibuf.slice(..));
                    let instance_count = instances.len().max(1) as u32;
                    rpass.draw(0..self.vertex_count, 0..instance_count);
            }

            self.queue.submit(Some(encoder.finish()));
            frame.present();
        }

        pub fn request_redraw(&self) {
            if let Some(w) = &self.window { w.request_redraw(); }
        }

        pub fn resize(&mut self, width: u32, height: u32) {
            if width == 0 || height == 0 { return; }
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
            // recreate depth texture for new size
            let depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth-texture"),
                size: wgpu::Extent3d { width: self.config.width, height: self.config.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.depth_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        }
    }

    // concrete renderer type is exported at crate root when the feature is enabled
}

// Export the appropriate Renderer type at the crate root so callers can use
// `gpu::Renderer` regardless of the active feature.
#[cfg(not(feature = "backend-wgpu"))]
pub use placeholder_renderer::Renderer;

#[cfg(feature = "backend-wgpu")]
pub use wgpu_impl::Renderer;

// The legacy Vulkan-specific backend has been removed. Use the `backend-wgpu` feature or
// add another feature for an alternate backend if needed.
