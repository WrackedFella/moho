//! GPU rendering backend for the Moho game engine.
//!
//! Built on [`wgpu`], which stays behind [`RendererBackend`]: games reach the renderer
//! through [`create_renderer`] and hand it data ([`Scene`], [`WorldMeshes`],
//! [`MaterialTable`], the GPU data types), never GPU objects.
//!
//! - **[`Scene`]** — Per-frame scene state (meshes, lights, camera)
//! - **[`MaterialTable`]** — GPU-side material storage buffer
//! - **[`pipeline`]** — Shader sources and vertex layouts, for shader validation

mod materials;
pub use materials::MaterialTable;
mod scene;
pub use moho_render_api::LightDesc;
pub use scene::Scene;
mod gpu_types;
mod sky;
pub use gpu_types::LightingGpu;
pub use moho_render_api::{InstanceGpu, MaterialGpu};
use moho_render_api::{Rgba8Image, WorldMesh};
mod block_on;
mod bloom;
mod markers;
mod ui_pass;
pub use markers::{MARKER_INFLATE, MarkerDraws, Markers};
mod world_meshes;
pub use world_meshes::WorldMeshes;
mod instance_collector;
pub(crate) use instance_collector::InstanceCollector;
mod mesh_renderer;
pub(crate) use mesh_renderer::MeshRenderer;

mod builder;
mod device;
mod lights;
pub mod pipeline;
mod render_ops;
mod resources;
mod shadow;
mod ssao;
mod textures;
mod types;

mod renderer;
pub(crate) use renderer::Renderer;
mod tone_map;

/// Object-safe renderer backend trait.
///
/// Exists to allow the main binary to hold a `Box<dyn RendererBackend + '_>` without
/// being coupled to the concrete wgpu type. This makes headless test renderers and
/// future alternative backends possible without changing call sites.
pub trait RendererBackend {
    fn resize(&mut self, width: u32, height: u32);
    /// Register a mesh represented by an array of positions. Returns a handle
    /// that can be used with `enqueue_draw` to render that mesh without
    /// re-supplying the vertex data every frame.
    fn register_mesh(&mut self, vertices: &[[f32; 3]]) -> u32;
    /// Register a mesh with an index buffer. `indices` are 32-bit indices.
    #[allow(clippy::too_many_arguments)]
    fn register_indexed_mesh(
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
    ) -> u32;
    /// Register a contract mesh, with `material` as the per-vertex material channel
    /// (empty defers every vertex to the instance material). The default uploads
    /// through `register_indexed_mesh`; a backend that draws textures overrides it to
    /// upload the mesh's UVs too.
    fn register_world_mesh(&mut self, mesh: &WorldMesh, material: &[u32]) -> u32 {
        let light_level = renderer::world_mesh_light_level(mesh);
        self.register_indexed_mesh(
            mesh.positions(),
            mesh.normals(),
            mesh.ao(),
            mesh.surface(),
            &light_level,
            mesh.light_rgb(),
            mesh.sky_exposure(),
            material,
            mesh.indices(),
        )
    }
    /// Upload `image` and return its handle; handle 0 is the white default texture, which
    /// is all the default returns.
    fn register_texture(&mut self, image: &Rgba8Image) -> u32 {
        let _ = image;
        0
    }
    /// Free a texture handle from `register_texture`. No-op by default.
    fn unregister_texture(&mut self, texture: u32) {
        let _ = texture;
    }
    /// Queue a previously-registered mesh for drawing with `texture` bound. The default
    /// ignores the texture and calls `enqueue_draw`.
    fn enqueue_textured_draw(&mut self, mesh: u32, texture: u32, instances: &[InstanceGpu]) {
        let _ = texture;
        self.enqueue_draw(mesh, instances);
    }
    /// Unregister a previously-registered mesh handle and free GPU resources.
    fn unregister_mesh(&mut self, mesh: u32);
    /// Begin a new frame: uploads the camera, culls/updates dynamic lights,
    /// updates shadow matrices, and acquires the swapchain surface texture.
    /// Must be called once before any `enqueue_draw` calls for the frame.
    fn begin_frame(
        &mut self,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), FrameError>;
    /// Queue a previously-registered mesh for drawing this frame with the
    /// given instances. Must be called after `begin_frame` and before
    /// `submit_frame`.
    fn enqueue_draw(&mut self, mesh: u32, instances: &[moho_render_api::InstanceGpu]);
    /// Flush all queued draws (shadow passes, main pass, markers, SSAO) and present
    /// the frame. No-op if `begin_frame` wasn't called or didn't acquire a
    /// surface texture.
    fn submit_frame(&mut self);
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

    /// Set the provider of UI paint data, pulled in `submit_frame` and drawn
    /// over the scene; `None` clears it. No-op by default.
    fn set_ui_source(&mut self, _source: Option<UiSource>) {}
    /// Choose whether presented frames wait for the display refresh. No-op by default.
    fn set_vsync(&mut self, _on: bool) {}
    /// Override the debug view mode in the lighting the shader reads; `None` leaves the
    /// game's own value. No-op by default.
    fn set_debug_view(&mut self, _mode: Option<u32>) {}
    /// Draw nothing in the world above `height` metres (`None` clears the cut).
    /// A non-finite height counts as `None`. Sticky until changed. No-op by
    /// default.
    fn set_view_height(&mut self, _height: Option<f32>) {}
    /// Set the Brightness exposure multiplier applied by the tone-map pass (clamped
    /// to `[0.5, 2.0]`, NaN gives 1.0). No-op by default.
    fn set_brightness(&mut self, _brightness: f32) {}
    /// Turn the tone-map curve on or off; off skips the curve and passes the
    /// HDR scene through, still adding bloom when it is enabled. Session-only. No-op by default.
    fn set_tone_map(&mut self, _enabled: bool) {}
    /// Set the bloom quality: 0 Off, 1 Low, 2 High; any other value gives High. Off adds
    /// no bloom at all. No-op by default.
    fn set_bloom_quality(&mut self, _quality: u8) {}
    /// Queue the frame's box markers to draw over the world: translucent
    /// `fills` and edge-only `outlines`, each sorted far to near. Called once
    /// per frame after the transparent draws and before `submit_frame`. No-op
    /// by default.
    fn enqueue_markers(
        &mut self,
        _fills: &[moho_render_api::MarkerGpu],
        _outlines: &[moho_render_api::MarkerGpu],
    ) {
    }
}

pub use moho_render_api::UiSource;

impl RendererBackend for Renderer<'_> {
    fn resize(&mut self, width: u32, height: u32) {
        self.resize(width, height);
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
        block_light_rgb: &[[f32; 3]],
        sky_exposed: &[f32],
        material: &[u32],
        indices: &[u32],
    ) -> u32 {
        self.register_indexed_mesh(
            vertices,
            normals,
            ao,
            geometry_type,
            light_level,
            block_light_rgb,
            sky_exposed,
            material,
            indices,
        )
    }
    fn register_world_mesh(&mut self, mesh: &WorldMesh, material: &[u32]) -> u32 {
        self.register_world_mesh(mesh, material)
    }
    fn register_texture(&mut self, image: &Rgba8Image) -> u32 {
        self.register_texture(image)
    }
    fn unregister_texture(&mut self, texture: u32) {
        self.unregister_texture(texture);
    }
    fn enqueue_textured_draw(&mut self, mesh: u32, texture: u32, instances: &[InstanceGpu]) {
        self.enqueue_textured_draw(mesh, texture, instances);
    }
    fn unregister_mesh(&mut self, mesh: u32) {
        self.unregister_mesh(mesh);
    }
    fn begin_frame(
        &mut self,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), FrameError> {
        self.begin_frame(camera)
    }
    fn enqueue_draw(&mut self, mesh: u32, instances: &[moho_render_api::InstanceGpu]) {
        self.enqueue_draw(mesh, instances);
    }
    fn submit_frame(&mut self) {
        self.submit_frame();
    }
    fn set_materials(&mut self, materials: &[crate::MaterialGpu]) {
        self.set_material_table(materials);
    }
    fn update_lighting(&mut self, lighting: crate::gpu_types::LightingGpu) {
        self.update_lighting(lighting);
    }
    fn set_ui_source(&mut self, source: Option<UiSource>) {
        self.set_ui_source(source);
    }
    fn set_vsync(&mut self, on: bool) {
        self.set_vsync(on);
    }
    fn set_debug_view(&mut self, mode: Option<u32>) {
        self.set_debug_view(mode);
    }
    fn set_view_height(&mut self, height: Option<f32>) {
        self.set_view_height(height);
    }
    fn set_brightness(&mut self, brightness: f32) {
        self.set_brightness(brightness);
    }
    fn set_tone_map(&mut self, enabled: bool) {
        self.set_tone_map(enabled);
    }
    fn set_bloom_quality(&mut self, quality: u8) {
        self.set_bloom_quality(quality);
    }
    fn enqueue_markers(
        &mut self,
        fills: &[moho_render_api::MarkerGpu],
        outlines: &[moho_render_api::MarkerGpu],
    ) {
        self.enqueue_markers(fills, outlines);
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
        let msg = format!("{e}");
        Box::new(RendererInitError::WgpuInit(msg)) as Box<dyn std::error::Error>
    })?;
    Ok(Box::new(r))
}

/// Errors that can occur during renderer initialization.
#[derive(thiserror::Error, Debug)]
pub enum RendererInitError {
    #[error("missing window for wgpu backend initialization")]
    MissingWindow,
    #[error("wgpu backend error: {0}")]
    WgpuInit(String),
}

/// Errors that can occur while beginning a frame.
#[derive(thiserror::Error, Debug)]
pub enum FrameError {
    /// The swapchain surface texture could not be acquired this frame (e.g.
    /// surface lost/outdated). The surface has already been reconfigured;
    /// callers should skip this frame and try again on the next one.
    #[error("surface texture unavailable this frame")]
    SurfaceUnavailable,
}
