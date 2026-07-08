//! GPU rendering backend for the Moho game engine.
//!
//! Built on [`wgpu`], this crate handles:
//!
//! - **[`Renderer`]** — Central render orchestrator (device, pipelines, frame submit)
//! - **[`Scene`]** — Per-frame scene state (meshes, lights, camera)
//! - **[`ShadowSystem`]** — Cascaded shadow maps with optional PCSS soft shadows
//! - **[`SsaoSystem`]** — Screen-space ambient occlusion (GTAO)
//! - **[`LightManager`]** — Dynamic point/spot light management
//! - **[`MaterialTable`]** — GPU-side material storage buffer
//! - **[`pipeline`]** — Render pipeline construction helpers

pub type TextureFormatRepr = wgpu::TextureFormat;

/// Type alias for backward compatibility.
pub type Material = MaterialGpu;

mod materials;
pub use materials::MaterialTable;
mod scene;
pub use moho_render_api::LightDesc;
pub use scene::Scene;
mod gpu_types;
pub use gpu_types::{
    CameraGpu, CascadedShadowMatrixGpu, LightingGpu, MAX_SHADOW_LIGHTS, MultiLightShadowGpu,
    ShadowMatrixGpu,
};
pub use moho_render_api::{InstanceGpu, MaterialGpu};
mod buffer_manager;
pub use buffer_manager::BufferManager;
mod instance_collector;
pub use instance_collector::InstanceCollector;
mod mesh_renderer;
pub use mesh_renderer::MeshRenderer;

pub mod device;
mod render_ops;
mod shadow;
pub use shadow::{ActiveShadowLight, LightType, PcssQuality, PcssSettings, ShadowSystem};
mod ssao;
pub use ssao::{SsaoQuality, SsaoSettings, SsaoSystem};
mod lights;
pub use lights::{Light, LightManager};
mod types;
pub use device::{DeviceInitError, DeviceSetup};
pub mod pipeline;
pub use pipeline::{PipelineInitError, PipelineSetup};
pub mod resources;
pub use resources::ResourcePool;
pub mod builder;
pub use builder::RendererBuilder;

mod renderer;
pub use renderer::Renderer;

/// Object-safe renderer backend trait.
///
/// Exists to allow the main binary to hold a `Box<dyn RendererBackend + '_>` without
/// being coupled to the concrete wgpu type. This makes headless test renderers and
/// future alternative backends possible without changing call sites.
pub trait RendererBackend {
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
        ao: &[f32],
        geometry_type: &[u32],
        light_level: &[f32],
        indices: &[u32],
    ) -> u32;
    /// Unregister a previously-registered mesh handle and free GPU resources.
    fn unregister_mesh(&mut self, mesh: u32);
    /// Render a previously-registered mesh by handle using the provided instances and camera.
    fn render_mesh(
        &mut self,
        mesh: u32,
        instances: &[moho_render_api::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        finalize: bool,
    );
    /// Replace the material table on the GPU.
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]);
    /// Update lighting parameters (sun, moon, ambient) and write to GPU buffer.
    fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu);

    /// Add a dynamic point light to the scene. Returns the light ID for future updates/removal.
    fn add_point_light(
        &mut self,
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        range: f32,
    ) -> u32;

    /// Remove a dynamic light by ID.
    fn remove_light(&mut self, id: u32) -> bool;

    /// Update a light's position.
    fn set_light_position(&mut self, id: u32, position: glam::Vec3);

    /// Enable or disable a light.
    fn set_light_enabled(&mut self, id: u32, enabled: bool);

    /// Get all registered lights as serializable descriptors (for save/load).
    /// Default returns empty; only the real renderer overrides this.
    fn all_lights_as_descs(&self) -> Vec<moho_render_api::LightDesc> {
        Vec::new()
    }

    /// Set shadow quality level.
    fn set_shadow_quality(&mut self, quality: u8);

    /// Set SSAO quality level.
    fn set_ssao_quality(&mut self, quality: u8);

    /// Return the surface texture format used by the renderer (if applicable).
    /// Useful for UI integrations that need to create GPU pipelines matching the swapchain format.
    fn surface_format(&self) -> Option<TextureFormatRepr> {
        None
    }
    /// Set an optional raw FrameCallback pointer. The renderer will call the callback during
    /// finalization so the application can record UI commands into the frame encoder.
    /// The pointer must remain valid until cleared.
    fn set_frame_callback_raw(&mut self, ptr: Option<*mut dyn FrameCallback>);
    /// Set an optional safe Arc<Mutex<dyn FrameCallback>>. Prefer this over the raw pointer
    /// variant; it's thread-safe and avoids pointer lifetime issues. Passing `None` clears.
    fn set_frame_callback_arc(
        &mut self,
        cb: Option<std::sync::Arc<std::sync::Mutex<dyn FrameCallback>>>,
    );
}

impl<'a> RendererBackend for Renderer<'a> {
    fn resize(&mut self, width: u32, height: u32) {
        self.resize(width, height)
    }
    fn register_mesh(&mut self, vertices: &[[f32; 3]]) -> u32 {
        self.register_mesh(vertices)
    }
    fn register_indexed_mesh(
        &mut self,
        vertices: &[[f32; 3]],
        normals: &[[f32; 3]],
        ao: &[f32],
        geometry_type: &[u32],
        light_level: &[f32],
        indices: &[u32],
    ) -> u32 {
        self.register_indexed_mesh(vertices, normals, ao, geometry_type, light_level, indices)
    }
    fn unregister_mesh(&mut self, mesh: u32) {
        self.unregister_mesh(mesh)
    }
    fn render_mesh(
        &mut self,
        mesh: u32,
        instances: &[moho_render_api::InstanceGpu],
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        finalize: bool,
    ) {
        self.render_mesh(mesh, instances, camera, finalize)
    }
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]) {
        self.set_material_table(materials)
    }
    fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu) {
        self.update_lighting(lighting)
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
    fn add_point_light(
        &mut self,
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        range: f32,
    ) -> u32 {
        self.add_point_light(position, color, intensity, range)
    }
    fn remove_light(&mut self, id: u32) -> bool {
        self.remove_light(id)
    }
    fn set_light_position(&mut self, id: u32, position: glam::Vec3) {
        self.set_light_position(id, position);
    }
    fn set_light_enabled(&mut self, id: u32, enabled: bool) {
        self.set_light_enabled(id, enabled);
    }
    fn all_lights_as_descs(&self) -> Vec<moho_render_api::LightDesc> {
        self.all_lights_as_descs()
    }
    fn set_shadow_quality(&mut self, quality: u8) {
        self.set_shadow_quality(quality);
    }
    fn set_ssao_quality(&mut self, quality: u8) {
        self.set_ssao_quality(quality);
    }
}

/// Create a boxed renderer backend. Always returns a Result so callers have a
/// single, fallible API to initialize a renderer.
pub fn create_renderer<'a>(
    window: Option<&'a winit::window::Window>,
) -> Result<Box<dyn RendererBackend + 'a>, Box<dyn std::error::Error>> {
    let win = window
        .ok_or_else(|| Box::new(RendererInitError::MissingWindow) as Box<dyn std::error::Error>)?;
    let r = Renderer::new(win).map_err(|e| {
        let msg = format!("{}", e);
        Box::new(RendererInitError::WgpuInit(msg)) as Box<dyn std::error::Error>
    })?;
    Ok(Box::new(r))
}

/// Convenience helper: create a renderer from an Arc<Window>.
///
/// The renderer borrows the provided `Window` for the lifetime of the returned trait object.
/// Callers typically keep an `Arc<winit::window::Window>` and pass a reference here so the
/// application retains ownership while the renderer uses a borrow.
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
