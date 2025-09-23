// Default (non-vulkan) placeholder so the workspace builds and tests fast.
#[cfg(not(feature = "vulkan"))]
pub fn run(instances: Vec<engine_core::actors::InstanceGpu>, _camera: (glam::Mat4, glam::Mat4)) {
    println!(
        "Placeholder renderer received {} instances",
        instances.len()
    );
    // Intentionally does not create a window; real renderer implemented under the
    // `vulkan` Cargo feature to allow incremental development without requiring
    // Vulkan at all times.
}

// Feature-gated Vulkano scaffold. This provides a compile-time shader skeleton
// and a small `run` entrypoint that will be expanded later into a full
// initialization, swapchain, pipeline, and instanced draw path.
#[cfg(feature = "vulkan")]
mod vulkan_scaffold {
    use engine_core::actors::InstanceGpu;
    use std::sync::Arc;

    // Push-constant block matching the vertex shader's `Push { mat4 view; mat4 proj; }`.
    // GLSL `mat4` is column-major; `glam::Mat4::to_cols_array_2d()` produces the
    // same layout (`[[f32;4];4]`) so we can copy the matrices directly.
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct PushConstants {
        view: [[f32; 4]; 4],
        proj: [[f32; 4]; 4],
    }

    // NOTE: These imports are intentionally limited so this module only compiles
    // when the `vulkan` feature is enabled.
    // Note: use the shader macro with fully-qualified path inside modules below.

    // Minimal vertex shader that expects per-vertex position and a per-instance
    // model matrix (split across 4 vec4 attributes) plus a material ID.
    // The shader is intentionally small and will be extended to exactly match
    // `engine_core::actors::InstanceGpu` when wiring the pipeline.
    mod vs {
        ::vulkano_shaders::shader! {
            ty: "vertex",
            src: "#version 450
layout(push_constant) uniform Push { mat4 view; mat4 proj; } pc;

layout(location = 0) in vec2 position; // per-vertex
layout(location = 1) in vec4 model_col0; // instance - mat4 split across 4 vec4
layout(location = 2) in vec4 model_col1;
layout(location = 3) in vec4 model_col2;
layout(location = 4) in vec4 model_col3;
            layout(location = 5) in uint material;

layout(location = 0) out vec3 frag_position;
layout(location = 1) out float frag_material;

void main() {
    vec4 local_pos = vec4(position, 0.0, 1.0);
    mat4 model = mat4(model_col0, model_col1, model_col2, model_col3);
    vec4 world_pos = model * local_pos;
    gl_Position = pc.proj * pc.view * world_pos;
    frag_position = world_pos.xyz;
    frag_material = float(material);
}
"
        }
    }

    // Minimal fragment shader returning a debug color derived from the material
    mod fs {
        ::vulkano_shaders::shader! {
            ty: "fragment",
            src: "#version 450
layout(location = 0) in vec3 frag_position;
layout(location = 1) in float frag_material;

layout(location = 0) out vec4 out_color;

void main() {
    // Use material (float) to produce a debug color.
    float m = fract(frag_material);
    out_color = vec4(m, 0.5 * (1.0 - m), 1.0 - m, 1.0);
}
"
        }
    }

    /// Entry point for the scaffolded renderer; currently prints a few helpful
    /// messages and shows where device/pipeline initialization will be placed.
    pub fn run(instances: Vec<InstanceGpu>, camera: (glam::Mat4, glam::Mat4)) {
        println!("Vulkan feature enabled — pipeline scaffold active.");
        println!("Instances to draw: {}", instances.len());

        // Future steps (planned):
        // 1. Create a `vulkano::instance::Instance` and choose a `PhysicalDevice`.
        // 2. Create a `Window` + surface via `winit` + `vulkano_win`.
        // 3. Create a `Device` + `Queue` and a `Swapchain` for the surface.
        // 4. Build shader modules (above) into a `GraphicsPipeline` with an
        //    instance-rate vertex input describing `InstanceGpu`.
        // 5. Create a CPU-side staging buffer and transfer instances to a
        //    device-local instance buffer, then issue an instanced draw call.

        // The shader modules are present at compile time and will be used by
        // the `create_pipeline` helper once device creation is implemented.
        // Load shader modules when building the pipeline below.

        println!("Shaders included: vertex and fragment modules generated at compile time.");

        // --- Begin minimal Vulkan initialization (Library, Instance, Physical device, Device + Queue)
        use vulkano::device::physical::PhysicalDeviceType;
        use vulkano::device::{
            Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo, QueueFlags,
        };
        use vulkano::instance::InstanceCreateInfo;
        use vulkano::library::VulkanLibrary;

        // Load Vulkan library and create an Instance
        let library = VulkanLibrary::new().expect("Failed to load Vulkan library");
    // Use the Surface helpers on the Vulkano type directly (replaces deprecated vulkano-win helpers)
    use vulkano::swapchain::Surface;
    use winit::event_loop::{EventLoop, EventLoopBuilder};
    use winit::window::WindowBuilder;
    // On Windows we may be running inside a test thread; creating an
    // EventLoop outside the main thread panics. Use the platform extension
    // to allow building an EventLoop that can be created on any thread.
    #[cfg(target_os = "windows")]
    use winit::platform::windows::EventLoopBuilderExtWindows;

        // Create the event loop first; the event loop provides the display handle
        // required to compute instance extensions.
    // EventLoop::new() returns a Result in winit 0.29; unwrap here as in examples.
        #[cfg(target_os = "windows")]
        let event_loop = {
            let mut builder = EventLoopBuilder::new();
            // `with_any_thread(true)` is provided by EventLoopBuilderExtWindows
            builder.with_any_thread(true);
            builder.build().expect("Failed to build event loop")
        };

        #[cfg(not(target_os = "windows"))]
        let event_loop = EventLoop::new().expect("Failed to create event loop");
        // Surface::required_extensions returns a Result; unwrap to get InstanceExtensions.
        let required_extensions = Surface::required_extensions(&event_loop)
            .expect("Failed to get required surface extensions");

        let instance = vulkano::instance::Instance::new(
            library,
            InstanceCreateInfo {
                enabled_extensions: required_extensions,
                ..Default::default()
            },
        )
        .expect("Failed to create Vulkan instance");

        // Create a winit window and convert it into a Vulkano Surface
        let window = WindowBuilder::new()
            .with_title("moho - vulkan scaffold")
            .build(&event_loop)
            .expect("Failed to create window");

        let surface = Surface::from_window(instance.clone(), window.into())
            .expect("Failed to create surface from window");

        // Enumerate physical devices and pick one that supports the surface and graphics
        let (physical, queue_family_index) = instance
            .enumerate_physical_devices()
            .expect("Physical device enumeration error")
            .filter_map(|p| {
                p.queue_family_properties()
                    .iter()
                    .enumerate()
                    .find(|(i, q)| {
                        q.queue_flags.intersects(QueueFlags::GRAPHICS)
                            && p.surface_support(*i as u32, &surface).unwrap_or(false)
                    })
                    .map(|(i, _)| (p.clone(), i as u32))
            })
            .max_by_key(|(p, _)| match p.properties().device_type {
                PhysicalDeviceType::DiscreteGpu => 2i32,
                PhysicalDeviceType::IntegratedGpu => 1i32,
                _ => 0i32,
            })
            .expect("No suitable physical device found");

        println!(
            "Selected physical device: {}",
            physical.properties().device_name
        );

        // Create logical device and obtain a graphics queue
        let device_ext = DeviceExtensions {
            khr_swapchain: true,
            ..DeviceExtensions::empty()
        };
        let (device, mut queues) = Device::new(
            physical.clone(),
            DeviceCreateInfo {
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index,
                    queues: vec![1.0],
                    ..Default::default()
                }],
                enabled_extensions: device_ext,
                ..Default::default()
            },
        )
        .expect("Failed to create logical device");

        let queue = queues.next().expect("Failed to retrieve queue");

        // Create a simple swapchain for the surface
        use vulkano::image::ImageUsage;
        use vulkano::swapchain::{CompositeAlpha, PresentMode, Swapchain, SwapchainCreateInfo};

        let caps = physical
            .surface_capabilities(&surface, Default::default())
            .expect("Failed to get surface capabilities");
        let composite_alpha = CompositeAlpha::Opaque;
        let image_format = physical
            .surface_formats(&surface, Default::default())
            .expect("Failed to get surface formats")[0]
            .0;

        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface.clone(),
            SwapchainCreateInfo {
                min_image_count: caps.min_image_count,
                image_format,
                image_extent: caps.current_extent.unwrap_or([1024, 768]),
                image_usage: ImageUsage::COLOR_ATTACHMENT,
                composite_alpha,
                present_mode: PresentMode::Fifo,
                ..Default::default()
            },
        )
        .expect("Failed to create swapchain");

        println!("Swapchain created with {} images", swapchain.image_count());

        println!("Vulkan initialization complete (scaffold). Window and GPU resources created.");

        // Create a minimal RenderPass using the `single_pass_renderpass!` macro
        // (examples in the `examples/` folder use this pattern with v0.34).
        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    format: image_format,
                    samples: 1,
                    load_op: Clear,
                    store_op: Store,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {},
            },
        )
        .unwrap();
        println!("Minimal render pass created (1 color attachment) via macro.");

        // --- Create pipeline (must exist before recording commands that bind it)
        use vulkano::pipeline::graphics::vertex_input::VertexDefinition;
        use vulkano::pipeline::{
            DynamicState, GraphicsPipeline, PipelineLayout, PipelineShaderStageCreateInfo,
            graphics::{
                GraphicsPipelineCreateInfo,
                color_blend::{ColorBlendAttachmentState, ColorBlendState},
                input_assembly::InputAssemblyState,
                multisample::MultisampleState,
                rasterization::RasterizationState,
                viewport::ViewportState,
            },
            layout::PipelineDescriptorSetLayoutCreateInfo,
        };
        use vulkano::render_pass::Subpass;

        // Load shader modules and then obtain entry points. We keep the module values
        // around and pass the module (not the entry point) into `.definition(...)`
        // which some Vulkano versions expect for building vertex input state.
        let vs_module = vs::load(device.clone()).expect("Failed to load vertex shader module");
        let fs_module = fs::load(device.clone()).expect("Failed to load fragment shader module");
        let vs = vs_module.entry_point("main").unwrap();
        let fs = fs_module.entry_point("main").unwrap();

        // Build vertex input state from the Vertex trait implementations for our two types
        // Try using the shader module's parsed interface which some versions expose
        // via an `interface()` method. Arc<ShaderModule> will auto-deref to the
        // module so the method can be called directly.
        // The `VertexDefinition::definition` expects a `&EntryPoint` in this
        // Vulkano version. Use the `vs` entry point's module/entry information
        // available via `vs.module()` to derive the shader interface the method
        // expects.
        // Some Vulkano versions expect a `&ShaderInterface` rather than an
        // `EntryPoint`. Use the entry point's module to access the parsed
        // interface and pass that in.
        // Build the vertex input state from the entry point. The Vulkano
        // `VertexDefinition::definition` implementation in this workspace
        // expects an `&EntryPoint`, so pass the `vs` entry point directly.
        // Pass the entry point directly; in the resolved Vulkano version the
        // `VertexDefinition::definition` implementation accepts an `&EntryPoint`.
        let vertex_input_state = [TriangleVertex::per_vertex(), InstanceData::per_instance()]
            .definition(&vs)
            .expect("Failed to build vertex input state");

        let stages = [
            PipelineShaderStageCreateInfo::new(vs),
            PipelineShaderStageCreateInfo::new(fs),
        ];

        let layout_create_info = PipelineDescriptorSetLayoutCreateInfo::from_stages(&stages)
            .into_pipeline_layout_create_info(device.clone())
            .expect("Failed to build pipeline layout create info");

        let layout = PipelineLayout::new(device.clone(), layout_create_info)
            .expect("Failed to create pipeline layout");

        let subpass =
            Subpass::from(render_pass.clone(), 0).expect("Failed to create subpass for pipeline");

        let pipeline = GraphicsPipeline::new(
            device.clone(),
            None,
            GraphicsPipelineCreateInfo {
                stages: stages.into_iter().collect(),
                // Use the per-vertex / per-instance definitions so instance-rate attributes are set up
                vertex_input_state: Some(vertex_input_state),
                input_assembly_state: Some(InputAssemblyState::default()),
                viewport_state: Some(ViewportState::default()),
                rasterization_state: Some(RasterizationState::default()),
                multisample_state: Some(MultisampleState::default()),
                color_blend_state: Some(ColorBlendState::with_attachment_states(
                    subpass.num_color_attachments(),
                    ColorBlendAttachmentState::default(),
                )),
                dynamic_state: [DynamicState::Viewport].into_iter().collect(),
                subpass: Some(subpass.into()),
                ..GraphicsPipelineCreateInfo::layout(layout)
            },
        )
        .expect("Failed to create graphics pipeline");

        println!("Minimal graphics pipeline created (no vertex/instance inputs).");

        // Create framebuffers for each swapchain image
        use vulkano::image::view::ImageView;
        use vulkano::render_pass::Framebuffer;
        use vulkano::render_pass::FramebufferCreateInfo;

        let framebuffers: Vec<Arc<Framebuffer>> = images
            .into_iter()
            .map(|image| {
                let view = ImageView::new_default(image).unwrap();
                Framebuffer::new(
                    render_pass.clone(),
                    FramebufferCreateInfo {
                        attachments: vec![view],
                        ..Default::default()
                    },
                )
                .unwrap()
            })
            .collect();

        // Allocate memory and create vertex + instance buffers
        use vulkano::buffer::{Buffer, BufferContents, BufferCreateInfo, BufferUsage};
        use vulkano::command_buffer::allocator::StandardCommandBufferAllocator;
        use vulkano::memory::allocator::{
            AllocationCreateInfo, MemoryTypeFilter, StandardMemoryAllocator,
        };
        // Bring the derive macros into scope like the examples do.
        use vulkano::command_buffer::{
            AutoCommandBufferBuilder, CommandBufferUsage, RenderPassBeginInfo,
        };
        use vulkano::pipeline::Pipeline;
        use vulkano::pipeline::graphics::vertex_input::Vertex;
        use vulkano::swapchain::acquire_next_image;
        use vulkano::sync::{self, GpuFuture};

        let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(device.clone()));
        let command_buffer_allocator = Arc::new(StandardCommandBufferAllocator::new(
            device.clone(),
            Default::default(),
        ));
        // Create a trait-object Arc for the allocator since the AutoCommandBufferBuilder
        // API expects an `Arc<dyn CommandBufferAllocator>`.
        let command_buffer_allocator_trait: Arc<
            dyn vulkano::command_buffer::allocator::CommandBufferAllocator,
        > = command_buffer_allocator.clone();

        #[derive(BufferContents, Vertex)]
        #[repr(C)]
        struct TriangleVertex {
            #[format(R32G32_SFLOAT)]
            position: [f32; 2],
        }

        #[derive(BufferContents, Vertex)]
        #[repr(C)]
        struct InstanceData {
            #[format(R32G32B32A32_SFLOAT)]
            model_col0: [f32; 4],
            #[format(R32G32B32A32_SFLOAT)]
            model_col1: [f32; 4],
            #[format(R32G32B32A32_SFLOAT)]
            model_col2: [f32; 4],
            #[format(R32G32B32A32_SFLOAT)]
            model_col3: [f32; 4],
            #[format(R32_UINT)]
            material: u32,
            #[format(R32_UINT)]
            object_type: u32,
            // padding omitted in shader; keep structural padding for alignment
            #[format(R32G32_UINT)]
            _pad: [u32; 2],
        }

        // Create a simple triangle vertex buffer
        let vertices = [
            TriangleVertex {
                position: [-0.5, -0.5],
            },
            TriangleVertex {
                position: [0.5, -0.5],
            },
            TriangleVertex {
                position: [0.0, 0.5],
            },
        ];

        let vertex_buffer = Buffer::from_iter(
            memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            vertices,
        )
        .expect("Failed to create vertex buffer");

        // Convert provided `instances: Vec<InstanceGpu>` into our `InstanceData`
        let instance_data: Vec<InstanceData> = instances
            .iter()
            .map(|i| InstanceData {
                model_col0: i.model[0],
                model_col1: i.model[1],
                model_col2: i.model[2],
                model_col3: i.model[3],
                material: i.material,
                object_type: i.object_type,
                _pad: i.padding,
            })
            .collect();

        let instance_buffer = Buffer::from_iter(
            memory_allocator.clone(),
            BufferCreateInfo {
                usage: BufferUsage::VERTEX_BUFFER,
                ..Default::default()
            },
            AllocationCreateInfo {
                memory_type_filter: MemoryTypeFilter::PREFER_DEVICE
                    | MemoryTypeFilter::HOST_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            instance_data,
        )
        .expect("Failed to create instance buffer");

        // Prepare push-constants from the provided camera (view, proj)
        let push_constants = PushConstants {
            view: camera.0.to_cols_array_2d(),
            proj: camera.1.to_cols_array_2d(),
        };

        // Tests for PushConstants are included here so they are compiled with the
        // `vulkan` feature and can access the private type.
        #[cfg(test)]
        mod push_constants_tests {
            use super::*;
            use bytemuck::{Pod, Zeroable};

            #[test]
            fn push_constants_pod_and_size() {
                // Compile-time trait checks via a helper generic function
                fn _assert_pod<T: Pod + Zeroable>() {}
                _assert_pod::<PushConstants>();

                // Two mat4s = 2 * 64 bytes = 128 bytes; ensure 16-byte alignment
                assert_eq!(std::mem::size_of::<PushConstants>() % 16, 0);
                assert_eq!(std::mem::size_of::<PushConstants>(), 128);
            }
        }

        // Prepare a minimal one-frame render: acquire, record commands, submit, present.
        let mut previous_frame_end: Option<Box<dyn GpuFuture>> =
            Some(sync::now(device.clone()).boxed());

        let (image_index, _suboptimal, acquire_future) =
            acquire_next_image(swapchain.clone(), None)
                .map_err(|e| panic!("failed to acquire next image: {e:?}"))
                .unwrap();

        previous_frame_end.as_mut().unwrap().cleanup_finished();

        let mut builder = AutoCommandBufferBuilder::primary(
            command_buffer_allocator_trait.clone(),
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )
        .unwrap();

        builder
            .begin_render_pass(
                RenderPassBeginInfo {
                    clear_values: vec![Some([0.0, 0.0, 0.0, 1.0].into())],
                    ..RenderPassBeginInfo::framebuffer(framebuffers[image_index as usize].clone())
                },
                Default::default(),
            )
            .unwrap()
            .set_viewport(
                0,
                [vulkano::pipeline::graphics::viewport::Viewport {
                    offset: [0.0, 0.0],
                    extent: {
                        let e = swapchain.image_extent();
                        [e[0] as f32, e[1] as f32]
                    },
                    depth_range: 0.0..=1.0,
                }]
                .into_iter()
                .collect(),
            )
            .unwrap()
            .bind_pipeline_graphics(pipeline.clone())
            .unwrap()
            .bind_vertex_buffers(0, (vertex_buffer.clone(), instance_buffer.clone()))
            .unwrap();

        // Push the camera matrices into the pipeline as push-constants so the
        // vertex shader can read `pc.view` and `pc.proj`.
        // The `push_constants` helper validates ranges and copies the bytes.
        builder
            .push_constants(pipeline.layout().clone(), 0, push_constants)
            .unwrap();

        unsafe {
            builder.draw(
                vertex_buffer.len() as u32,
                instance_buffer.len() as u32,
                0,
                0,
            )
        }
        .unwrap();

        builder.end_render_pass(Default::default()).unwrap();

        let command_buffer = builder.build().unwrap();

        let future = previous_frame_end
            .take()
            .unwrap()
            .join(acquire_future)
            .then_execute(queue.clone(), command_buffer)
            .unwrap()
            .then_swapchain_present(
                queue.clone(),
                vulkano::swapchain::SwapchainPresentInfo::swapchain_image_index(
                    swapchain.clone(),
                    image_index,
                ),
            )
            .then_signal_fence_and_flush();

        match future.map_err(|e| {
            eprintln!("present error: {e:?}");
            e
        }) {
            Ok(f) => {
                f.wait(None).unwrap();
            }
            Err(_) => {}
        }

        println!("One-frame instanced draw submitted.");
    }

    // A helper function signature (stub) that will be fleshed out when creating
    // actual `Device`/`RenderPass` objects. Left unexported for now.
    #[allow(dead_code)]
    fn create_pipeline_placeholder() {
        // Placeholder: when implemented this will accept a `Device` and
        // `Subpass` and return a `GraphicsPipeline` configured for instancing.
    }
}

// Re-export the run entry as the crate-level function when the feature is on.
#[cfg(feature = "vulkan")]
pub use vulkan_scaffold::run;

// Smoke test: builds a tiny scene and calls the Vulkan scaffold's `run`.
// This test is ignored by default because it opens a window and requires a
// working Vulkan driver and an available GPU. Run explicitly with:
//
// ```powershell
// cargo test --features vulkan -- --ignored
// ```
//
// Keep this test in the source so contributors can run it locally when they
// want to do a quick smoke check of the scaffold.
#[cfg(all(test, feature = "vulkan"))]
mod smoke_tests {
    use super::vulkan_scaffold::run as vk_run;
    use engine_core::actors::Sphere;
    use engine_core::materials::MaterialType;
    use glam::Vec3;

    #[test]
    #[ignore]
    fn smoke_render_small_scene() {
        // Create two spheres and convert to GPU instances
        let s1 = Sphere::new(Vec3::new(0.0, 0.0, 0.0), 1.0, MaterialType::Lambertian { albedo: Vec3::new(0.8, 0.3, 0.3) });
        let s2 = Sphere::new(Vec3::new(2.0, 0.0, 0.0), 1.0, MaterialType::Metal { albedo: Vec3::new(0.8, 0.8, 0.8), fuzz: 0.0 });

        let instances = vec![s1.to_instance(), s2.to_instance()];

        // Basic camera (view, proj)
        let eye = Vec3::new(5.0, 2.0, 5.0);
        let center = Vec3::new(0.0, 0.0, 0.0);
        let up = Vec3::new(0.0, 1.0, 0.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);

        // Call the scaffolded run; the test is primarily to catch panics and
        // basic integration errors. It will open a window and present one frame.
        vk_run(instances, (view, proj));
    }
}
