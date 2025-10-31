// Renderer crate extracted from the main binary to provide a reusable renderer
// API. The WGPU implementation has been migrated here and organized under a
// `gfx` module as requested. The crate re-exports a `Renderer` type at the
// root so callers can continue to use `moho_renderer::Renderer`.

pub mod prelude {
    pub use crate::Renderer;
}

// Cross-platform alias for the surface texture format. When the wgpu
// backend is enabled this maps to `wgpu::TextureFormat`. Otherwise it
// is a unit type so the public API remains compilable when wgpu is not
// available.
#[cfg(feature = "backend-wgpu")]
pub type TextureFormatRepr = wgpu::TextureFormat;

#[cfg(not(feature = "backend-wgpu"))]
pub type TextureFormatRepr = ();

// Compact material representation exposed by the crate so backends and the
// application can share a single, stable memory layout for the GPU material
// table. This type is always available (feature-independent) which keeps the
// public `RendererBackend` trait signature consistent across features.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaterialGpu {
    // Use two vec4-sized fields so the GPU storage layout is a clean 32-byte
    // stride per element which matches WGSL `vec4` alignment rules.
    pub albedo: [f32; 4],
    pub params: [f32; 4], // params.x = fuzz, params.y = ref_idx, others unused
}

impl MaterialGpu {
    /// Return true if this material was marked as potentially transparent
    /// by the application (params[2] > 0.0).
    pub fn is_transparent(&self) -> bool {
        self.params[2] > 0.0
    }
}

// Material table implementation (moved from the binary to the renderer crate)
mod materials;
pub use materials::MaterialTable;
mod scene;
pub use scene::Scene;
mod gpu_types;
pub use gpu_types::{CameraGpu, LightingGpu, ShadowMatrixGpu, CascadedShadowMatrixGpu};

pub mod gfx {
    //! Graphics backends grouped under `gfx` for clarity. The WGPU backend is
    //! feature-gated behind `backend-wgpu`.

    #[cfg(feature = "backend-wgpu")]
    pub mod wgpu_impl {
        // Make sure the `winit` crate name is available when the feature
        // is enabled (helps rustc resolve `winit::...` paths in some envs).
        extern crate winit;
        // Migrated WGPU implementation (was previously in `src/gpu.rs`). Paths
        // to assets/shaders are adjusted for the crate layout.
        use moho_core::actors::InstanceGpu as CpuInstance;
        use wgpu::util::DeviceExt;
        
        // ===== Shadow Mapping Configuration =====
        /// Number of cascaded shadow map cascades for CSM (Cascaded Shadow Maps)
        const NUM_SHADOW_CASCADES: u32 = 4;

        /// Shadow map resolution per cascade (4096x4096 for high quality)
        const SHADOW_MAP_SIZE: u32 = 4096;

        /// Cascade split distances from camera (in world units)
        /// Option B: Moderate distances (20, 50, 100, 200)
        /// - Near cascade (0-20): Very high precision for closest geometry
        /// - Mid-near cascade (20-50): High precision for nearby gameplay area  
        /// - Mid-far cascade (50-100): Medium precision for visible terrain
        /// - Far cascade (100-200): Lower precision for distant geometry
        /// Note: Distances can be tweaked until voxel chunk size is finalized
        const CASCADE_SPLIT_DISTANCES: [f32; 4] = [20.0, 50.0, 100.0, 200.0];

        /// Debug flag: Enable CSM debug visualization (cascade color-coding)
        const CSM_DEBUG_MODE: bool = false;

        /// Debug flag: Enable verbose logging of cascade calculations
        const CSM_VERBOSE_LOGGING: bool = false;
        // ===== End Shadow Mapping Configuration =====

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

        // cube actor import removed: not used in this module

        pub struct Renderer<'a> {
            // The renderer borrows the application Window. Callers must
            // ensure the Window outlives the Renderer.
            #[allow(dead_code)]
            window: &'a winit::window::Window,
            surface: wgpu::Surface<'a>,
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
            lighting_buffer: wgpu::Buffer,
            _depth_texture: wgpu::Texture,
            depth_texture_view: wgpu::TextureView,
            depth_format: wgpu::TextureFormat,
            instance_buffer: Option<wgpu::Buffer>,
            instance_capacity: usize,
            vertex_count: u32,
            // renderer does not own the application Window; the app keeps the Window
            // mesh table stores optional mesh entries for registered meshes
            mesh_table: Vec<Option<MeshEntry>>,
            // cached mesh handle for the unit cube (created on-demand)
            // cube_mesh was removed as it was never read
            // Pending frame state to allow multiple mesh draws to share the same
            // acquired surface texture. We only present and submit when the
            // caller indicates `finalize=true`.
            pending_frame: Option<wgpu::SurfaceTexture>,
            // Collect draws (mesh handle + instance list) for the current
            // application frame. We will record them all in one render pass
            // and submit when the caller finalizes the frame.
            pending_draws: Vec<(u32, Vec<GpuInstance>)>,
            pending_frame_view: Option<wgpu::TextureView>,
            // Optional raw pointer to an application-provided FrameCallback.
            // Stored as a raw pointer to avoid borrow-checker lifetime issues
            // between the renderer and the application-owned adapter.
            frame_callback_raw: Option<*mut dyn crate::FrameCallback>,
            // Optional safe Arc+Mutex-wrapped callback. Prefer this when set.
            frame_callback_arc: Option<std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
            // Skybox rendering resources
            skybox_pipeline: wgpu::RenderPipeline,
            skybox_vertex_buffer: wgpu::Buffer,
            skybox_vertex_count: u32,
            // Shadow mapping resources (legacy single shadow map)
            shadow_map_texture: wgpu::Texture,
            shadow_map_view: wgpu::TextureView,
            shadow_pipeline: wgpu::RenderPipeline,
            shadow_matrix_buffer: wgpu::Buffer,
            shadow_bind_group_layout: wgpu::BindGroupLayout,
            shadow_bind_group: wgpu::BindGroup,
            shadow_sampler: wgpu::Sampler,
            // Shadow pass bind group (for rendering shadow map)
            shadow_pass_bind_group: wgpu::BindGroup,
            // Current lighting state (cached for shadow matrix calculation)
            current_lighting: crate::gpu_types::LightingGpu,
            // CSM (Cascaded Shadow Maps) resources
            csm_texture: wgpu::Texture,
            csm_cascade_views: Vec<wgpu::TextureView>,
            csm_array_view: wgpu::TextureView,
            // Note: CSM matrix buffer, bind groups, etc. will be added in Phase 2
        }

        // Per-mesh stored data (supports optional index buffer)
        pub struct MeshEntry {
            pub buffer: wgpu::Buffer,
            pub vertex_count: u32,
            pub index_buffer: Option<wgpu::Buffer>,
            pub index_count: u32,
        }

        impl<'a> Renderer<'a> {
            pub fn new(
                window: &'a winit::window::Window,
            ) -> Result<Self, Box<dyn std::error::Error>> {
                log::info!("(wgpu) Initializing renderer (instanced cubes)");
                // Use the borrowed window directly. The Surface is created
                // with a reference to the provided Window; the returned
                // Surface borrows the Window for the same lifetime.
                let size = window.inner_size();
                // Initialize wgpu
                let instance_desc = wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::all(),
                    ..Default::default()
                };
                let instance = wgpu::Instance::new(&instance_desc);
                // create_surface takes a reference to the window; pass a borrow
                // from the Arc. Keep the Arc in the struct so the Window
                // remains alive for the Surface's use.
                let surface = instance
                    .create_surface(window)
                    .map_err(|e| format!("create_surface failed: {:?}", e))?;

                let adapter =
                    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                    }))
                    .map_err(|e| format!("Failed to request adapter: {:?}", e))?;

                // Gate experimental features behind an explicit cargo feature.
                let experimental = {
                    #[cfg(feature = "wgpu-experimental")]
                    {
                        unsafe { wgpu::ExperimentalFeatures::enabled() }
                    }
                    #[cfg(not(feature = "wgpu-experimental"))]
                    {
                        wgpu::ExperimentalFeatures::disabled()
                    }
                };

                // Only request features the adapter actually supports.
                let desired_features = wgpu::Features::empty();
                let required_features = desired_features & adapter.features();

                let (device, queue) =
                    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                        label: None,
                        required_features,
                        required_limits: wgpu::Limits::default(),
                        memory_hints: Default::default(),
                        trace: Default::default(),
                        experimental_features: experimental,
                    }))
                    .map_err(|e| format!("Failed to create device: {:?}", e))?;

                let supported_formats = surface.get_capabilities(&adapter).formats;
                let config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: supported_formats[0],
                    width: size.width,
                    height: size.height,
                    present_mode: wgpu::PresentMode::Fifo,
                    alpha_mode: wgpu::CompositeAlphaMode::Auto,
                    view_formats: vec![],
                    desired_maximum_frame_latency: 0,
                };
                surface.configure(&device, &config);

                // Vertex data is supplied by the application (from the World) at render time.
                // The renderer will create/update the GPU vertex buffer on demand in
                // `render()` so no mesh is hardcoded here.
                let vertex_buffer = None;
                let vertex_count = 0u32;

                let shader_source = [
                    include_str!("../../shaders/common.wgsl"),
                    include_str!("../../shaders/vertex.wgsl"),
                    include_str!("../../shaders/fragment.wgsl"),
                ]
                .join("\n\n");
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("shader"),
                    // shader path adjusted for crate layout (moho_renderer/src -> repo root)
                    source: wgpu::ShaderSource::Wgsl(shader_source.into()),
                });
                // Camera uniform bind group (group 0) now contains:
                //   binding 0: camera uniform (VP matrix + position)
                //   binding 1: material storage buffer
                //   binding 2: lighting uniform (sun + ambient)
                let camera_size = std::mem::size_of::<[f32; 20]>() as u64; // 5 vec4s
                let lighting_size = std::mem::size_of::<[f32; 12]>() as u64; // 3 vec4s
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
                                        std::num::NonZeroU64::new(camera_size)
                                            .ok_or("camera size was zero")?,
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
                            wgpu::BindGroupLayoutEntry {
                                binding: 2,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Uniform,
                                    has_dynamic_offset: false,
                                    min_binding_size: Some(
                                        std::num::NonZeroU64::new(lighting_size)
                                            .ok_or("lighting size was zero")?,
                                    ),
                                },
                                count: None,
                            },
                        ],
                    });

                // Create shadow bind group layout for main pass (group 1: shadow sampling)
                let shadow_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shadow-bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: Some(
                                    std::num::NonZeroU64::new(std::mem::size_of::<ShadowMatrixGpu>() as u64)
                                        .ok_or("shadow matrix size was zero")?,
                                ),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Depth,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                            count: None,
                        },
                    ],
                });

                // Create shadow pass bind group layout (group 0 for shadow pass: just shadow matrix)
                let shadow_pass_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("shadow-pass-bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: Some(
                                    std::num::NonZeroU64::new(std::mem::size_of::<ShadowMatrixGpu>() as u64)
                                        .ok_or("shadow matrix size was zero")?,
                                ),
                            },
                            count: None,
                        },
                    ],
                });

                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("pipeline-layout"),
                        bind_group_layouts: &[&camera_bgl, &shadow_bind_group_layout],
                        push_constant_ranges: &[],
                    });

                // Skybox pipeline layout (only needs camera bind group, no shadows)
                let skybox_pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("skybox-pipeline-layout"),
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

                // Create lighting uniform buffer with default lighting
                use crate::gpu_types::LightingGpu;
                let initial_lighting = LightingGpu::default();
                let lighting_buffer =
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("lighting-buffer"),
                        contents: bytemuck::bytes_of(&initial_lighting),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
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
                        contents: bytemuck::bytes_of(&initial_material),
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
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: lighting_buffer.as_entire_binding(),
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
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
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
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
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
                        front_face: wgpu::FrontFace::Ccw, // Counter-clockwise winding
                        cull_mode: Some(wgpu::Face::Back), // Enable back-face culling
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

                // Create skybox rendering resources
                let skybox_shader_source = std::fs::read_to_string("shaders/skybox.wgsl")
                    .map_err(|e| format!("Failed to read skybox shader: {}", e))?;
                let skybox_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("skybox-shader"),
                    source: wgpu::ShaderSource::Wgsl(skybox_shader_source.into()),
                });

                // Create skybox pipeline
                let skybox_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("skybox-pipeline"),
                    layout: Some(&skybox_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &skybox_shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                        }],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &skybox_shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend: None, // No blending for skybox
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None, // No culling for skybox (inside a sphere)
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: depth_format,
                        depth_write_enabled: false, // Don't write depth for skybox
                        depth_compare: wgpu::CompareFunction::LessEqual, // LessEqual for skybox at far plane
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

                // Generate skybox sphere geometry (UV sphere)
                let (skybox_vertices, skybox_vertex_count) = Self::generate_skybox_sphere(32, 16);
                let skybox_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("skybox-vertex-buffer"),
                    contents: bytemuck::cast_slice(&skybox_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                // Create shadow mapping resources
                
                // ===== Legacy Single Shadow Map (will be phased out) =====
                // Shadow map depth texture (single layer for backward compatibility)
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
                
                let shadow_map_view = shadow_map_texture.create_view(&wgpu::TextureViewDescriptor::default());
                
                // ===== CSM Texture Array (new) =====
                // Create texture array for cascaded shadow maps (4 layers)
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
                    log::info!("Created CSM texture array: {}x{} with {} layers", 
                               SHADOW_MAP_SIZE, SHADOW_MAP_SIZE, NUM_SHADOW_CASCADES);
                }
                
                // Create views for each cascade layer (for rendering)
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
                            usage: Some(wgpu::TextureUsages::RENDER_ATTACHMENT),
                        })
                    })
                    .collect();
                
                // Create full array view (for sampling in shaders)
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
                    log::info!("Created {} cascade views + 1 array view for CSM sampling", NUM_SHADOW_CASCADES);
                }
                
                // Shadow matrix uniform buffer
                use crate::gpu_types::ShadowMatrixGpu;
                let initial_shadow_matrix = ShadowMatrixGpu::default();
                let shadow_matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("shadow-matrix-buffer"),
                    contents: bytemuck::bytes_of(&initial_shadow_matrix),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                
                // Shadow sampler (comparison sampler for PCF)
                let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("shadow-sampler"),
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    mipmap_filter: wgpu::FilterMode::Nearest,
                    compare: Some(wgpu::CompareFunction::LessEqual), // Comparison sampler for shadow mapping
                    ..Default::default()
                });
                
                // Shadow pass bind group (for shadow rendering - just the shadow matrix)
                let shadow_pass_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shadow-pass-bind-group"),
                    layout: &shadow_pass_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: shadow_matrix_buffer.as_entire_binding(),
                        },
                    ],
                });
                
                // Shadow bind group (for main pass - shadow sampling with matrix + texture + sampler)
                let shadow_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shadow-bind-group"),
                    layout: &shadow_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: shadow_matrix_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&shadow_map_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                        },
                    ],
                });
                
                // Create shadow pipeline (simple depth-only rendering)
                let shadow_shader_source = std::fs::read_to_string("shaders/shadow.wgsl")
                    .map_err(|e| format!("Failed to read shadow shader: {}", e))?;
                let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("shadow-shader"),
                    source: wgpu::ShaderSource::Wgsl(shadow_shader_source.into()),
                });
                
                // Shadow pipeline uses shadow_pass_bind_group_layout (just shadow matrix for rendering)
                let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shadow-pipeline-layout"),
                    bind_group_layouts: &[&shadow_pass_bind_group_layout],
                    push_constant_ranges: &[],
                });
                
                let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("shadow-pipeline"),
                    layout: Some(&shadow_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shadow_shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[
                            // Vertex buffer (position + normal)
                            wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                            },
                            // Instance buffer
                            wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<GpuInstance>() as wgpu::BufferAddress,
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
                    fragment: None, // Depth-only pass, no fragment shader
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
                            constant: 2, // Shadow bias to reduce acne
                            slope_scale: 2.0,
                            clamp: 0.0,
                        },
                    }),
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

                Ok(Renderer {
                    window,
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
                    lighting_buffer,
                    _depth_texture: depth_texture,
                    depth_texture_view: depth_view,
                    depth_format,
                    instance_buffer: Some(instance_buf),
                    instance_capacity: 1,
                    vertex_count,
                    // window is owned by the application; don't store it here
                    mesh_table: Vec::new(),
                    pending_frame: None,
                    pending_draws: Vec::new(),
                    pending_frame_view: None,
                    frame_callback_raw: None,
                    frame_callback_arc: None,
                    skybox_pipeline,
                    skybox_vertex_buffer,
                    skybox_vertex_count,
                    shadow_map_texture,
                    shadow_map_view,
                    shadow_pipeline,
                    shadow_matrix_buffer,
                    shadow_bind_group_layout,
                    shadow_bind_group,
                    shadow_sampler,
                    shadow_pass_bind_group,
                    current_lighting: initial_lighting,
                    csm_texture,
                    csm_cascade_views,
                    csm_array_view,
                })
            }

            /// Generate a UV sphere for the skybox
            /// Returns (vertices, vertex_count)
            fn generate_skybox_sphere(longitude_segments: u32, latitude_segments: u32) -> (Vec<[f32; 3]>, u32) {
                let mut vertices = Vec::new();
                
                // Generate vertices for UV sphere
                for lat in 0..=latitude_segments {
                    let theta = std::f32::consts::PI * (lat as f32) / (latitude_segments as f32);
                    let sin_theta = theta.sin();
                    let cos_theta = theta.cos();
                    
                    for lon in 0..=longitude_segments {
                        let phi = 2.0 * std::f32::consts::PI * (lon as f32) / (longitude_segments as f32);
                        let sin_phi = phi.sin();
                        let cos_phi = phi.cos();
                        
                        // Unit sphere position
                        let x = sin_theta * cos_phi;
                        let y = cos_theta;
                        let z = sin_theta * sin_phi;
                        
                        vertices.push([x, y, z]);
                    }
                }
                
                // Generate triangle indices (convert to triangle list)
                let mut triangle_vertices = Vec::new();
                for lat in 0..latitude_segments {
                    for lon in 0..longitude_segments {
                        let current = (lat * (longitude_segments + 1) + lon) as usize;
                        let next = current + (longitude_segments + 1) as usize;
                        
                        // First triangle
                        triangle_vertices.push(vertices[current]);
                        triangle_vertices.push(vertices[next]);
                        triangle_vertices.push(vertices[current + 1]);
                        
                        // Second triangle
                        triangle_vertices.push(vertices[current + 1]);
                        triangle_vertices.push(vertices[next]);
                        triangle_vertices.push(vertices[next + 1]);
                    }
                }
                
                let vertex_count = triangle_vertices.len() as u32;
                (triangle_vertices, vertex_count)
            }

            /// Return the configured surface format for the renderer.
            pub fn surface_format(&self) -> wgpu::TextureFormat {
                self.config.format
            }

            /// Update lighting parameters and write to GPU buffer.
            /// This also updates the current_lighting field for shadow matrix calculation.
            pub fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu) {
                self.current_lighting = lighting;
                self.queue.write_buffer(
                    &self.lighting_buffer,
                    0,
                    bytemuck::bytes_of(&lighting),
                );
            }

            /// Calculate shadow matrix (light view-projection) based on sun direction.
            /// Creates an orthographic projection from the light's perspective to cover the scene.
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
                        wgpu::SurfaceError::OutOfMemory => {
                            log::error!("wgpu::SurfaceError::OutOfMemory");
                            panic!("Out of memory")
                        }
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
                // Debug: print camera position and first element of viewproj so we
                // can confirm the renderer sees the updated camera each frame.
                log::trace!(
                    "[wgpu] render: cam_pos={:?} viewproj0={:?}",
                    cam_pos,
                    viewproj.to_cols_array()[0]
                );
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
                let buf = match self.instance_buffer.as_ref() {
                    Some(b) => b,
                    None => {
                        log::error!("instance buffer missing when writing instances");
                        return;
                    }
                };
                if !instances.is_empty() {
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

                // Calculate and update shadow matrix based on current sun direction
                let sun_dir = glam::Vec3::new(
                    self.current_lighting.sun_direction[0],
                    self.current_lighting.sun_direction[1],
                    self.current_lighting.sun_direction[2],
                );
                let shadow_matrix = self.calculate_shadow_matrix(sun_dir, cam_pos);
                let cols = shadow_matrix.to_cols_array_2d();
                let shadow_matrix_gpu = crate::gpu_types::ShadowMatrixGpu {
                    sm0: cols[0],
                    sm1: cols[1],
                    sm2: cols[2],
                    sm3: cols[3],
                };
                self.queue.write_buffer(
                    &self.shadow_matrix_buffer,
                    0,
                    bytemuck::bytes_of(&shadow_matrix_gpu),
                );

                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("encoder"),
                        });

                // SHADOW PASS: Render scene from light's perspective to shadow map
                {
                    let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("shadow-pass"),
                        color_attachments: &[], // No color output for depth-only pass
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.shadow_map_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0), // Clear to maximum depth
                                store: wgpu::StoreOp::Store,     // Store shadow map for use in main pass
                            }),
                            stencil_ops: None,
                        }),
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });

                    shadow_pass.set_pipeline(&self.shadow_pipeline);
                    shadow_pass.set_bind_group(0, &self.shadow_pass_bind_group, &[]);
                    
                    // Render scene geometry to shadow map
                    if let Some(vb) = self.vertex_buffer.as_ref() {
                        shadow_pass.set_vertex_buffer(0, vb.slice(..));
                    }
                    if let Some(ibuf) = self.instance_buffer.as_ref() {
                        shadow_pass.set_vertex_buffer(1, ibuf.slice(..));
                    }
                    let instance_count = instances.len().max(1) as u32;
                    shadow_pass.draw(0..self.vertex_count, 0..instance_count);
                }

                // MAIN PASS: Render scene with shadows
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("rpass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &frame_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth_texture_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Store,
                            }),
                            stencil_ops: None,
                        }),
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });
                    
                    // Render skybox first (at maximum depth)
                    rpass.set_pipeline(&self.skybox_pipeline);
                    rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                    rpass.set_vertex_buffer(0, self.skybox_vertex_buffer.slice(..));
                    rpass.draw(0..self.skybox_vertex_count, 0..1);
                    
                    // Then render main scene
                    rpass.set_pipeline(&self.pipeline);
                    // set camera bind group (group 0)
                    rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                    // set shadow bind group (group 1)
                    rpass.set_bind_group(1, &self.shadow_bind_group, &[]);
                    // set vertex buffer (created on-demand)
                    if let Some(vb) = self.vertex_buffer.as_ref() {
                        rpass.set_vertex_buffer(0, vb.slice(..));
                    } else {
                        log::trace!("vertex buffer missing, skipping set_vertex_buffer");
                    }
                    // bind the persistent instance buffer
                    let ibuf = match self.instance_buffer.as_ref() {
                        Some(b) => b,
                        None => {
                            log::error!("instance buffer missing during draw");
                            return;
                        }
                    };
                    rpass.set_vertex_buffer(1, ibuf.slice(..));
                    let instance_count = instances.len().max(1) as u32;
                    rpass.draw(0..self.vertex_count, 0..instance_count);
                }

                self.queue.submit(Some(encoder.finish()));
                frame.present();
            }

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
                if self.mesh_table[idx].is_some() {
                    // update camera
                    let view_mat = camera.0;
                    let proj_mat = camera.1;
                    let cam_pos = camera.2;
                    let viewproj = proj_mat * view_mat;
                    // Debug: log camera values for render_mesh path so we can
                    // correlate main-side camera computation with what the
                    // renderer writes for mesh-based rendering.
                    log::trace!(
                        "[wgpu] render_mesh: cam_pos={:?} viewproj0={:?}",
                        cam_pos,
                        viewproj.to_cols_array()[0]
                    );
                    let mut cols = viewproj.to_cols_array().to_vec();
                    cols.push(cam_pos.x);
                    cols.push(cam_pos.y);
                    cols.push(cam_pos.z);
                    cols.push(0.0f32);
                    self.queue
                        .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&cols));

                    // Calculate and update shadow matrix based on current sun direction
                    let sun_dir = glam::Vec3::new(
                        self.current_lighting.sun_direction[0],
                        self.current_lighting.sun_direction[1],
                        self.current_lighting.sun_direction[2],
                    );
                    let shadow_matrix = self.calculate_shadow_matrix(sun_dir, cam_pos);
                    let cols = shadow_matrix.to_cols_array_2d();
                    let shadow_matrix_gpu = crate::gpu_types::ShadowMatrixGpu {
                        sm0: cols[0],
                        sm1: cols[1],
                        sm2: cols[2],
                        sm3: cols[3],
                    };
                    self.queue.write_buffer(
                        &self.shadow_matrix_buffer,
                        0,
                        bytemuck::bytes_of(&shadow_matrix_gpu),
                    );

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

                    let ibuf = match self.instance_buffer.as_ref() {
                        Some(b) => b,
                        None => {
                            log::error!("instance buffer missing when uploading instances");
                            return;
                        }
                    };
                    if !instances_gpu.is_empty() {
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

                    // Push this draw into pending_draws to batch multiple mesh
                    // draws into a single render pass per application frame.
                    self.pending_draws.push((mesh, instances_gpu));

                    // If we need to acquire the frame (this is the first pending
                    // draw), do so now. If acquire fails attempt reconfigure.
                    if self.pending_frame_view.is_none() {
                        match self.surface.get_current_texture() {
                            Ok(f) => {
                                let view = f
                                    .texture
                                    .create_view(&wgpu::TextureViewDescriptor::default());
                                self.pending_frame = Some(f);
                                self.pending_frame_view = Some(view);
                            }
                            Err(_) => {
                                self.surface.configure(&self.device, &self.config);
                                return;
                            }
                        }
                    }

                    // If finalize requested, record a single render pass that
                    // iterates all pending_draws and issues draw calls for each
                    // registered mesh. We will write instance data for each draw
                    // into the instance buffer sequentially and use vertex buffer
                    // offsets when binding if supported; wgpu allows setting the
                    // vertex buffer with an offset in bytes via slice(offset..).
                    if finalize {
                        // Ensure we have a frame view and a pending frame to present
                        let frame_view = match &self.pending_frame_view {
                            Some(v) => v,
                            None => return,
                        };

                        // Create an encoder to record the batched render pass.
                        let mut encoder =
                            self.device
                                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("batched-encoder"),
                                });

                        // Before recording, we need to flatten all instance lists
                        // into a contiguous buffer and record the offsets for each draw.
                        let mut all_instances: Vec<GpuInstance> = Vec::new();
                        let mut offsets: Vec<usize> = Vec::with_capacity(self.pending_draws.len());
                        for (_m, insts) in &self.pending_draws {
                            offsets.push(all_instances.len());
                            all_instances.extend_from_slice(insts);
                        }

                        // Ensure instance buffer capacity for all_instances
                        let required = all_instances.len().max(1);
                        if self.instance_capacity < required {
                            let mut new_cap = self.instance_capacity.max(1);
                            while new_cap < required {
                                new_cap = new_cap.saturating_mul(2);
                            }
                            let size_bytes = (new_cap * std::mem::size_of::<GpuInstance>())
                                as wgpu::BufferAddress;
                            let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                                label: Some("instance-buffer"),
                                size: size_bytes,
                                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                                mapped_at_creation: false,
                            });
                            self.instance_buffer = Some(buf);
                            self.instance_capacity = new_cap;
                        }

                        // Upload all instances into the instance buffer
                        let ibuf = match self.instance_buffer.as_ref() {
                            Some(b) => b,
                            None => {
                                log::error!(
                                    "instance buffer missing when uploading batched instances"
                                );
                                return;
                            }
                        };
                        if !all_instances.is_empty() {
                            self.queue
                                .write_buffer(ibuf, 0, bytemuck::cast_slice(&all_instances));
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

                        // SHADOW PASS: Render scene from light's perspective to shadow map
                        {
                            let mut shadow_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("shadow-pass"),
                                color_attachments: &[],
                                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                                    view: &self.shadow_map_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Store,
                                    }),
                                    stencil_ops: None,
                                }),
                                occlusion_query_set: None,
                                timestamp_writes: None,
                            });

                            shadow_pass.set_pipeline(&self.shadow_pipeline);
                            shadow_pass.set_bind_group(0, &self.shadow_pass_bind_group, &[]);

                            // Draw all pending meshes from light's perspective with instances
                            for (i, (mesh_handle, insts)) in self.pending_draws.iter().enumerate() {
                                let idx = *mesh_handle as usize;
                                if idx >= self.mesh_table.len() {
                                    continue;
                                }
                                if let Some(me) = &self.mesh_table[idx] {
                                    shadow_pass.set_vertex_buffer(0, me.buffer.slice(..));
                                    
                                    // Bind instance buffer with offset for this draw
                                    let offset_instances = offsets[i];
                                    let actual_instance_count = insts.len();
                                    
                                    if actual_instance_count > 0 {
                                        let offset_bytes = (offset_instances * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
                                        let end_bytes = ((offset_instances + actual_instance_count) * std::mem::size_of::<GpuInstance>()) as wgpu::BufferAddress;
                                        shadow_pass.set_vertex_buffer(1, ibuf.slice(offset_bytes..end_bytes));
                                        
                                        let instance_count_u32 = actual_instance_count as u32;
                                        if let Some(idx_buf) = &me.index_buffer {
                                            shadow_pass.set_index_buffer(idx_buf.slice(..), wgpu::IndexFormat::Uint32);
                                            shadow_pass.draw_indexed(0..me.index_count, 0, 0..instance_count_u32);
                                        } else {
                                            shadow_pass.draw(0..me.vertex_count, 0..instance_count_u32);
                                        }
                                    }
                                }
                            }
                        }

                        // Begin the single render pass and issue draw calls for
                        // each pending draw in order.
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("batched-rpass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: frame_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: Some(
                                wgpu::RenderPassDepthStencilAttachment {
                                    view: &self.depth_texture_view,
                                    depth_ops: Some(wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(1.0),
                                        store: wgpu::StoreOp::Store,
                                    }),
                                    stencil_ops: None,
                                },
                            ),
                            occlusion_query_set: None,
                            timestamp_writes: None,
                        });
                        rpass.set_pipeline(&self.pipeline);
                        rpass.set_bind_group(0, &self.camera_bind_group, &[]);
                        rpass.set_bind_group(1, &self.shadow_bind_group, &[]);

                        // Iterate draws and issue draw calls
                        for (i, (mesh_handle, insts)) in self.pending_draws.iter().enumerate() {
                            let idx = *mesh_handle as usize;
                            if idx >= self.mesh_table.len() {
                                continue;
                            }
                            if let Some(me) = &self.mesh_table[idx] {
                                // Bind the mesh's vertex buffer
                                rpass.set_vertex_buffer(0, me.buffer.slice(..));

                                // Bind the instance buffer with offset for this draw
                                // Skip instance buffer binding if there are no instances (finalize draw)
                                let offset_instances = offsets[i];
                                let actual_instance_count = insts.len();

                                if actual_instance_count > 0 {
                                    let offset_bytes = (offset_instances
                                        * std::mem::size_of::<GpuInstance>())
                                        as wgpu::BufferAddress;
                                    let end_bytes = ((offset_instances + actual_instance_count)
                                        * std::mem::size_of::<GpuInstance>())
                                        as wgpu::BufferAddress;
                                    rpass.set_vertex_buffer(1, ibuf.slice(offset_bytes..end_bytes));

                                    // Draw with actual instance count
                                    let instance_count_u32 = actual_instance_count as u32;
                                    if let Some(idx_buf) = &me.index_buffer {
                                        rpass.set_index_buffer(
                                            idx_buf.slice(..),
                                            wgpu::IndexFormat::Uint32,
                                        );
                                        rpass.draw_indexed(
                                            0..me.index_count,
                                            0,
                                            0..instance_count_u32,
                                        );
                                    } else {
                                        rpass.draw(0..me.vertex_count, 0..instance_count_u32);
                                    }
                                }
                                // If actual_instance_count is 0, this is a finalize-only draw, skip rendering
                            }
                        }

                        drop(rpass);

                        // If an application registered a FrameCallback, call it
                        // now so it can record UI commands into the same encoder
                        // before we finish and submit it. Prefer the safe Arc<Mutex<..>>
                        // wrapper when available, otherwise fall back to the raw
                        // pointer path for backward compatibility.
                        if let Some(cb_arc) = &self.frame_callback_arc {
                            log::debug!("[wgpu] finalize: calling frame_callback_arc");
                            if let Some(view) = self.pending_frame_view.as_ref() {
                                // Lock the mutex briefly while calling into the callback.
                                if let Ok(mut guard) = cb_arc.lock() {
                                    guard.call(
                                        &self.device,
                                        &self.queue,
                                        view,
                                        &mut encoder,
                                        self.config.width,
                                        self.config.height,
                                    );
                                    log::debug!("[wgpu] finalize: frame_callback_arc returned");
                                } else {
                                    log::warn!(
                                        "[wgpu] finalize: failed to lock frame_callback_arc"
                                    );
                                }
                            }
                        } else if let Some(cb_ptr) = self.frame_callback_raw {
                            unsafe {
                                log::info!("[wgpu] finalize: calling frame_callback_raw");
                                if let Some(view) = self.pending_frame_view.as_ref() {
                                    let cb: &mut dyn crate::FrameCallback = &mut *cb_ptr;
                                    cb.call(
                                        &self.device,
                                        &self.queue,
                                        view,
                                        &mut encoder,
                                        self.config.width,
                                        self.config.height,
                                    );
                                    log::info!("[wgpu] finalize: frame_callback_raw returned");
                                }
                            }
                        }

                        // Submit and present
                        let finished = encoder.finish();
                        // Debug: print that we're about to submit/present a batched frame
                        log::debug!(
                            "[wgpu] finalize: submitting {} draws",
                            self.pending_draws.len()
                        );
                        if let Some(frame) = self.pending_frame.take() {
                            self.queue.submit(Some(finished));
                            frame.present();
                        }

                        // Clear pending state
                        self.pending_frame_view = None;
                        self.pending_draws.clear();
                    }
                }
            }
        }
    }

    #[cfg(not(feature = "backend-wgpu"))]
    pub mod placeholder {
        use moho_core::actors::InstanceGpu;
        pub struct Renderer {}
        impl Default for Renderer {
            fn default() -> Self {
                Self::new()
            }
        }
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
    fn render(
        &mut self,
        vertices: &[[f32; 3]],
        instances: &[moho_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    );
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
    /// Return the surface texture format used by the renderer (if applicable).
    /// This is useful for UI integrations that need to create GPU pipelines
    /// with the same format as the swapchain.
    fn surface_format(&self) -> Option<TextureFormatRepr> {
        None
    }
    /// Set an optional raw FrameCallback pointer. The renderer will call the
    /// callback during finalization so the application can record UI commands
    /// into the frame encoder. The pointer must remain valid until cleared.
    #[cfg(feature = "backend-wgpu")]
    fn set_frame_callback_raw(&mut self, ptr: Option<*mut dyn FrameCallback>);
    /// Set an optional safe Arc<Mutex<dyn FrameCallback>>. Prefer this
    /// registration method when possible; it's thread-safe and avoids raw
    /// pointer lifetime issues. Passing `None` clears the registration.
    #[cfg(feature = "backend-wgpu")]
    fn set_frame_callback_arc(
        &mut self,
        cb: Option<std::sync::Arc<std::sync::Mutex<dyn FrameCallback>>>,
    );
}

#[cfg(feature = "backend-wgpu")]
impl<'a> RendererBackend for gfx::wgpu_impl::Renderer<'a> {
    fn render(
        &mut self,
        vertices: &[[f32; 3]],
        instances: &[moho_core::actors::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        gfx::wgpu_impl::Renderer::render(self, vertices, instances, camera)
    }
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
    fn surface_format(&self) -> Option<TextureFormatRepr> {
        Some(self.surface_format())
    }
    #[cfg(feature = "backend-wgpu")]
    fn set_frame_callback_raw(&mut self, ptr: Option<*mut dyn FrameCallback>) {
        self.set_frame_callback_raw_inherent(ptr);
    }
    #[cfg(feature = "backend-wgpu")]
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

#[cfg(not(feature = "backend-wgpu"))]
impl RendererBackend for gfx::placeholder::Renderer {
    fn render(
        &mut self,
        vertices: &[[f32; 3]],
        instances: &[moho_core::actors::InstanceGpu],
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
        _instances: &[moho_core::actors::InstanceGpu],
        _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        _finalize: bool,
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

    fn surface_format(&self) -> Option<TextureFormatRepr> {
        None
    }

    fn set_cursor_visible(&self, _visible: bool) {
        // placeholder: no-op
    }

    fn set_cursor_grab(&self, _locked: bool) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

/// Create a boxed renderer backend. When `backend-wgpu` is enabled the
/// function takes the `EventLoop` and `Window` so the backend can create a
/// surface. When disabled the parameterless form is provided.
/// Create a boxed renderer backend. Always returns a Result so callers have a
/// single, fallible API to initialize a renderer regardless of feature flags.
#[cfg(feature = "backend-wgpu")]
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

#[cfg(not(feature = "backend-wgpu"))]
pub fn create_renderer(
    _window: Option<std::sync::Arc<()>>,
) -> Result<Box<dyn RendererBackend>, Box<dyn std::error::Error>> {
    Ok(Box::new(gfx::placeholder::Renderer::new()))
}

/// Compatibility wrapper: always return a Result<Box<dyn RendererBackend>, Box<dyn Error>>.
/// This lets callers use a single API regardless of whether the crate was built with
/// the `backend-wgpu` feature enabled (which changes the signature of `create_renderer`).
pub fn create_renderer_any<'a>(
    _window: Option<&'a ()>,
) -> Result<Box<dyn RendererBackend + 'a>, Box<dyn std::error::Error>> {
    // Avoid referencing winit types in the signature so this function
    // compiles regardless of feature flags. When the GPU backend is
    // enabled callers should call `create_renderer` directly with a
    // `winit::window::Window` reference. This helper is intended for
    // the non-backend placeholder path and will return an Err when the
    // backend is enabled to make that explicit.
    #[cfg(feature = "backend-wgpu")]
    {
        Err(Box::from(
            "create_renderer_any is not available when backend-wgpu is enabled; call create_renderer instead",
        ))
    }

    #[cfg(not(feature = "backend-wgpu"))]
    {
        Ok(Box::new(gfx::placeholder::Renderer::new()))
    }
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
#[cfg(feature = "backend-wgpu")]
pub fn create_renderer_from_arc<'a>(
    window: &'a std::sync::Arc<winit::window::Window>,
) -> Result<Box<dyn RendererBackend + 'a>, Box<dyn std::error::Error>> {
    create_renderer(Some(std::sync::Arc::as_ref(window)))
}

#[cfg(not(feature = "backend-wgpu"))]
pub fn create_renderer_from_arc(
    _window: &std::sync::Arc<()>,
) -> Result<Box<dyn RendererBackend>, Box<dyn std::error::Error>> {
    // Placeholder backend ignores the window. Match the placeholder
    // factory's parameter type (Arc<()>) so this helper is available
    // even when the wgpu/backend feature is disabled.
    create_renderer(None)
}

/// Callback trait for UI rendering. Implement this to composite UI elements
/// into the renderer's command encoder.
#[cfg(feature = "backend-wgpu")]
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
