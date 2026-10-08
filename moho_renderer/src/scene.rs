use crate::{InstanceCollector, MaterialTable, RendererBackend, WorldMeshes};
use moho_render_api::{InstanceGpu, Renderable};

mod preparation;
#[allow(unused_imports)] // Public API, used externally
pub use preparation::PreparedScene;
pub use preparation::ScenePreparation;

/// Scene manager that owns the `MaterialTable` and provides a simple
/// `render` API to submit actors and world meshes for drawing. This centralizes
/// material deduplication, instance collection, and transparent sorting.
///
/// Scene *persistence* (save/load) lives in `moho_game::scene_persistence` —
/// it constructs/reads game-domain types this crate must not name.
#[derive(Debug)]
pub struct Scene {
    pub material_table: MaterialTable,
    instance_collector: InstanceCollector,
    world_meshes: WorldMeshes,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            material_table: MaterialTable::new(),
            instance_collector: InstanceCollector::new(),
            world_meshes: WorldMeshes::new(),
        }
    }

    /// Queue world-geometry changes; they reach the backend on the next `render`.
    pub fn world_meshes_mut(&mut self) -> &mut WorldMeshes {
        &mut self.world_meshes
    }

    /// Render the provided actors and the world meshes using `renderer`.
    /// `mesh_handle` is the spherical mesh handle and `cube_mesh_handle` is the
    /// cube mesh handle, both previously registered with the renderer at
    /// startup. Queued world-mesh changes are flushed first. `S`/`C` are the
    /// concrete sphere-like/cube-like actor types (supplied by the caller so
    /// this crate doesn't need to name game-domain types).
    ///
    /// Returns `Err` if the frame's surface texture couldn't be acquired
    /// (e.g. surface lost/outdated); the surface has already been
    /// reconfigured in that case, so callers should just skip the frame.
    pub fn render<S, C>(
        &mut self,
        renderer: &mut dyn RendererBackend,
        spheres: &[S],
        cubes: &[C],
        mesh_handle: u32,
        cube_mesh_handle: u32,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), crate::FrameError>
    where
        S: Renderable,
        C: Renderable,
    {
        self.world_meshes.flush(renderer);

        // Prepare scene: collect instances, process materials, separate by transparency
        let prepared = ScenePreparation::prepare(
            spheres,
            cubes,
            &mut self.material_table,
            &mut self.instance_collector,
            renderer,
            mesh_handle,
            cube_mesh_handle,
        );

        renderer.begin_frame(camera)?;

        // Render opaque geometry first
        renderer.enqueue_draw(cube_mesh_handle, &prepared.cube_opaque);
        renderer.enqueue_draw(mesh_handle, &prepared.sphere_opaque);

        // World meshes (opaque, one draw per mesh)
        for (handle, instance) in self.world_meshes.draws() {
            renderer.enqueue_draw(handle, &[instance]);
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
