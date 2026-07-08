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
    #[allow(clippy::too_many_arguments)]
    pub fn render<S, C>(
        &mut self,
        renderer: &mut dyn RendererBackend,
        world: &mut World,
        mesh_handle: u32,
        cube_mesh_handle: u32,
        terrain_material_idx: u32,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) where
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

        // Render opaque geometry first (no finalize)
        renderer.render_mesh(cube_mesh_handle, &prepared.cube_opaque, camera, false);
        renderer.render_mesh(mesh_handle, &prepared.sphere_opaque, camera, false);

        // Render VoxelChunks (opaque, each chunk as separate draw)
        for (chunk_handle, chunk_inst) in self.instance_collector.chunk_renders() {
            renderer.render_mesh(*chunk_handle, &[*chunk_inst], camera, false);
        }

        // Render transparent instances (back-to-front sorted)
        self.render_transparent(
            renderer,
            prepared.transparent_entries,
            camera,
            mesh_handle,
            self.instance_collector.chunk_renders(),
        );
    }

    /// Render transparent instances sorted back-to-front.
    fn render_transparent(
        &self,
        renderer: &mut dyn RendererBackend,
        transparent_entries: Vec<(u32, InstanceGpu)>,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        mesh_handle: u32,
        chunk_renders: &[(u32, InstanceGpu)],
    ) {
        // If no transparent draws, finalize with empty draw
        if transparent_entries.is_empty() {
            let finalize_handle = chunk_renders
                .first()
                .map(|(h, _)| *h)
                .unwrap_or(mesh_handle);
            renderer.render_mesh(finalize_handle, &Vec::new(), camera, true);
            return;
        }

        let cam_eye = camera.2;

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

        // Render each group, marking the last call for finalization
        for (i, (mesh_h, insts)) in groups.iter().enumerate() {
            let final_call = i + 1 == groups.len();
            renderer.render_mesh(*mesh_h, insts, camera, final_call);
        }
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}
