use crate::{BufferManager, InstanceCollector, MaterialTable, RendererBackend};
use legion::World;
use legion::storage::Component;
use moho_render_api::{InstanceGpu, Renderable};

mod preparation;
#[allow(unused_imports)] // Public API, used externally
pub use preparation::PreparedScene;
pub use preparation::ScenePreparation;

/// Scene manager that owns the `MaterialTable` and provides a simple
/// `render` API to submit an ECS world for drawing. This centralizes
/// material deduplication, instance collection, and transparent sorting.
///
/// Scene *persistence* (save/load) lives in `moho_game::scene_persistence` —
/// it constructs/reads game-domain types this crate must not name.
#[derive(Debug)]
pub struct Scene {
    pub material_table: MaterialTable,
    buffer_manager: BufferManager,
    instance_collector: InstanceCollector,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            material_table: MaterialTable::new(),
            buffer_manager: BufferManager::new(),
            instance_collector: InstanceCollector::new(),
        }
    }

    /// Render the provided `world` using `renderer`. `mesh_handle` is the
    /// spherical mesh handle, `cube_mesh_handle` is the cube mesh handle, and
    /// `terrain_material_idx` is the material table index for terrain chunks
    /// — all previously registered with the renderer/material table at
    /// startup. `S`/`C` are the concrete sphere-like/cube-like component
    /// types to query for (supplied by the caller so this crate doesn't need
    /// to name game-domain types).
    ///
    /// Returns `Err` if the frame's surface texture couldn't be acquired
    /// (e.g. surface lost/outdated); the surface has already been
    /// reconfigured in that case, so callers should just skip the frame.
    #[allow(clippy::too_many_arguments)]
    pub fn render<S, C>(
        &mut self,
        renderer: &mut dyn RendererBackend,
        world: &mut World,
        mesh_handle: u32,
        cube_mesh_handle: u32,
        terrain_material_idx: u32,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), crate::FrameError>
    where
        S: Renderable + Component,
        C: Renderable + Component,
    {
        // Prepare scene: collect instances, process materials, separate by transparency
        let prepared = ScenePreparation::prepare::<S, C>(
            world,
            &mut self.material_table,
            &mut self.buffer_manager,
            &mut self.instance_collector,
            renderer,
            mesh_handle,
            cube_mesh_handle,
            terrain_material_idx,
        );

        renderer.begin_frame(camera)?;

        // Render opaque geometry first
        renderer.enqueue_draw(cube_mesh_handle, &prepared.cube_opaque);
        renderer.enqueue_draw(mesh_handle, &prepared.sphere_opaque);

        // Render VoxelChunks (opaque, each chunk as separate draw)
        for (chunk_handle, chunk_inst) in self.instance_collector.chunk_renders() {
            renderer.enqueue_draw(*chunk_handle, &[*chunk_inst]);
        }

        // Render transparent instances (back-to-front sorted)
        Self::enqueue_transparent(renderer, prepared.transparent_entries, camera.2);

        renderer.submit_frame();
        Ok(())
    }

    /// Queue transparent instances sorted back-to-front for drawing.
    fn enqueue_transparent(
        renderer: &mut dyn RendererBackend,
        transparent_entries: Vec<(u32, InstanceGpu)>,
        cam_eye: glam::Vec3,
    ) {
        if transparent_entries.is_empty() {
            return;
        }

        // Sort transparent entries by depth (back-to-front)
        let mut by_depth: Vec<(f32, u32, InstanceGpu)> =
            Vec::with_capacity(transparent_entries.len());
        for (mesh_h, inst) in transparent_entries {
            let pos = glam::Vec3::new(inst.model[3][0], inst.model[3][1], inst.model[3][2]);
            let dist2 = (pos - cam_eye).length_squared();
            by_depth.push((dist2, mesh_h, inst));
        }

        by_depth.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Group consecutive entries with the same mesh handle
        let mut groups: Vec<(u32, Vec<InstanceGpu>)> = Vec::new();
        for (_d, mesh_h, inst) in by_depth {
            if let Some((last_mesh, vec)) = groups.last_mut()
                && *last_mesh == mesh_h
            {
                vec.push(inst);
                continue;
            }
            groups.push((mesh_h, vec![inst]));
        }

        for (mesh_h, insts) in &groups {
            renderer.enqueue_draw(*mesh_h, insts);
        }
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}
