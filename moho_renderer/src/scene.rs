use crate::{InstanceCollector, Markers, MaterialTable, RendererBackend, WorldMeshes};
use moho_render_api::{InstanceGpu, Renderable};

mod preparation;
#[allow(unused_imports)] // Public API, used externally
pub use preparation::PreparedScene;
pub use preparation::ScenePreparation;

/// Scene manager that owns the `MaterialTable` and provides a simple
/// `render` API to submit actors and world meshes for drawing. This centralizes
/// material deduplication, instance collection, and transparent sorting.
///
/// Scene *persistence* (save/load) lives in `flagstone_game::scene_persistence` —
/// it constructs/reads game-domain types this crate must not name.
#[derive(Debug)]
pub struct Scene {
    pub material_table: MaterialTable,
    instance_collector: InstanceCollector,
    world_meshes: WorldMeshes,
    markers: Markers,
}

impl Scene {
    pub fn new() -> Self {
        Scene {
            material_table: MaterialTable::new(),
            instance_collector: InstanceCollector::new(),
            world_meshes: WorldMeshes::new(),
            markers: Markers::new(),
        }
    }

    /// Add, move and remove box markers; they draw on the next `render`.
    pub fn markers_mut(&mut self) -> &mut Markers {
        &mut self.markers
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

        let mut world_draws: Vec<((u32, u32), Vec<InstanceGpu>)> = Vec::new();
        for (mesh, texture, instance) in self.world_meshes.textured_draws() {
            let key = (mesh, texture);
            match world_draws.last_mut() {
                Some((last, instances)) if *last == key => instances.push(instance),
                _ => world_draws.push((key, vec![instance])),
            }
        }
        let skinned_draws: Vec<_> = self
            .world_meshes
            .skinned_draws()
            .map(|(mesh, texture, instance, palette)| {
                let rows: Vec<[[f32; 4]; 3]> = palette.iter().map(joint_rows).collect();
                (mesh, texture, instance, rows)
            })
            .collect();
        // Cleared before begin_frame so a skipped frame consumes its placements too.
        self.world_meshes.clear_placements();

        renderer.begin_frame(camera)?;

        // Render opaque geometry first
        renderer.enqueue_draw(cube_mesh_handle, &prepared.cube_opaque);
        renderer.enqueue_draw(mesh_handle, &prepared.sphere_opaque);

        for ((mesh, texture), instances) in &world_draws {
            renderer.enqueue_textured_draw(*mesh, *texture, instances);
        }

        for (mesh, texture, instance, palette) in skinned_draws {
            renderer.enqueue_skinned_draw(mesh, texture, &palette, instance);
        }

        // Render transparent instances (back-to-front sorted)
        Self::enqueue_transparent(renderer, prepared.transparent_entries, camera.2);

        let markers = self.markers.draws(camera.2);
        renderer.enqueue_markers(&markers.fills, &markers.outlines);

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

/// A joint matrix as the GPU palette holds it: three rows of `[m_r0, m_r1, m_r2, t_r]`.
fn joint_rows(joint: &glam::Affine3A) -> [[f32; 4]; 3] {
    let (x, y, z, t) = (
        joint.matrix3.x_axis,
        joint.matrix3.y_axis,
        joint.matrix3.z_axis,
        joint.translation,
    );
    [
        [x.x, y.x, z.x, t.x],
        [x.y, y.y, z.y, t.y],
        [x.z, y.z, z.z, t.z],
    ]
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}
