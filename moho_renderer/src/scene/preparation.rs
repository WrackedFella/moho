//! Scene preparation module - handles instance collection, material processing, and geometry separation.
//!
//! This module contains the preparation logic extracted from `Scene::render()` to improve
//! separation of concerns. Preparation (collecting instances, separating opaque/transparent)
//! is now distinct from rendering (GPU command submission).

use crate::{BufferManager, InstanceCollector, MaterialTable, RendererBackend};
use legion::World;
use legion::storage::Component;
use moho_render_api::{InstanceGpu, Renderable};

/// Prepared scene data ready for rendering.
///
/// Contains all instances separated into opaque and transparent groups,
/// ready to be submitted to the GPU via render calls.
pub struct PreparedScene {
    /// Opaque cube instances (rendered first, no depth sorting needed)
    pub cube_opaque: Vec<InstanceGpu>,
    /// Opaque sphere instances (rendered first, no depth sorting needed)
    pub sphere_opaque: Vec<InstanceGpu>,
    /// Transparent instances with their mesh handles (requires depth sorting)
    pub transparent_entries: Vec<(u32, InstanceGpu)>,
    /// Whether materials were uploaded to GPU this frame
    #[allow(dead_code)] // Will be used for conditional material updates
    pub materials_uploaded: bool,
}

impl PreparedScene {
    /// Create an empty prepared scene (useful for testing or when no geometry exists).
    #[allow(dead_code)] // Used in tests
    pub fn empty() -> Self {
        Self {
            cube_opaque: Vec::new(),
            sphere_opaque: Vec::new(),
            transparent_entries: Vec::new(),
            materials_uploaded: false,
        }
    }

    /// Check if this prepared scene has any geometry to render.
    #[allow(dead_code)] // Used in tests
    pub fn is_empty(&self) -> bool {
        self.cube_opaque.is_empty()
            && self.sphere_opaque.is_empty()
            && self.transparent_entries.is_empty()
    }
}

/// Scene preparation coordinator.
///
/// Handles the complex logic of collecting instances from the ECS world,
/// processing materials, and separating geometry into opaque and transparent groups.
pub struct ScenePreparation;

impl ScenePreparation {
    /// Prepare a scene for rendering by collecting instances and separating by transparency.
    ///
    /// This method:
    /// 1. Collects all renderable instances from the ECS world
    /// 2. Logs debug information about materials and instances
    /// 3. Uploads materials to GPU if they've changed
    /// 4. Separates instances into opaque and transparent groups
    ///
    /// # Arguments
    /// * `world` - ECS world containing renderable entities
    /// * `material_table` - Table of all materials (may be updated)
    /// * `buffer_manager` - Manages GPU buffers for voxel chunks
    /// * `instance_collector` - Collects instances from world
    /// * `renderer` - Backend for GPU operations
    /// * `mesh_handle` - Handle for sphere mesh
    /// * `cube_mesh_handle` - Handle for cube mesh
    /// * `terrain_material_idx` - Pre-registered material index for terrain chunks
    ///
    /// # Returns
    /// PreparedScene containing separated geometry ready for rendering
    #[allow(clippy::too_many_arguments)]
    pub fn prepare<S, C>(
        world: &mut World,
        material_table: &mut MaterialTable,
        buffer_manager: &mut BufferManager,
        instance_collector: &mut InstanceCollector,
        renderer: &mut dyn RendererBackend,
        mesh_handle: u32,
        cube_mesh_handle: u32,
        terrain_material_idx: u32,
    ) -> PreparedScene
    where
        S: Renderable + Component,
        C: Renderable + Component,
    {
        // Step 1: Collect instances from world
        instance_collector.collect_from_world::<S, C>(
            world,
            material_table,
            buffer_manager,
            renderer,
            terrain_material_idx,
        );

        // Step 2: Debug logging (optional, can be feature-gated in future)
        Self::log_debug_info(material_table, instance_collector);

        // Step 3: Upload materials to GPU if changed
        let materials_uploaded = if material_table.is_dirty() {
            renderer.set_materials(material_table.as_slice());
            material_table.clear_dirty();
            true
        } else {
            false
        };

        log::debug!(
            "[ScenePreparation] VoxelChunks to render: {}",
            instance_collector.chunk_renders().len()
        );

        // Step 4: Separate instances by transparency
        let (cube_opaque, sphere_opaque, transparent_entries) = Self::separate_by_transparency(
            material_table,
            instance_collector,
            mesh_handle,
            cube_mesh_handle,
        );

        PreparedScene {
            cube_opaque,
            sphere_opaque,
            transparent_entries,
            materials_uploaded,
        }
    }

    /// Log debug information about materials and instances.
    ///
    /// This helps with debugging material assignments and instance counts.
    fn log_debug_info(material_table: &MaterialTable, instance_collector: &InstanceCollector) {
        if !material_table.as_slice().is_empty() {
            log::debug!(
                "[debug] material_table.len={} ",
                material_table.as_slice().len()
            );
            for (i, m) in material_table.as_slice().iter().enumerate().take(8) {
                log::debug!(
                    "[debug] mat[{}] albedo=({:.3},{:.3},{:.3}) fuzz={:.3} ref={:.3}",
                    i,
                    m.albedo[0],
                    m.albedo[1],
                    m.albedo[2],
                    m.params[0],
                    m.params[1]
                );
            }
        }

        for (i, inst) in instance_collector
            .sphere_instances()
            .iter()
            .enumerate()
            .take(8)
        {
            log::debug!("[debug] sphere_inst[{}].material={}", i, inst.material);
        }

        for (i, inst) in instance_collector
            .cube_instances()
            .iter()
            .enumerate()
            .take(8)
        {
            log::debug!("[debug] cube_inst[{}].material={}", i, inst.material);
        }
    }

    /// Separate instances into opaque and transparent groups based on material properties.
    ///
    /// Opaque geometry can be rendered in any order (usually front-to-back for early-Z).
    /// Transparent geometry must be sorted back-to-front for correct alpha blending.
    ///
    /// # Returns
    /// Tuple of (opaque_cubes, opaque_spheres, transparent_entries)
    fn separate_by_transparency(
        material_table: &MaterialTable,
        instance_collector: &InstanceCollector,
        mesh_handle: u32,
        cube_mesh_handle: u32,
    ) -> (Vec<InstanceGpu>, Vec<InstanceGpu>, Vec<(u32, InstanceGpu)>) {
        let mats = material_table.as_slice();
        let mut cube_opaque = Vec::new();
        let mut sphere_opaque = Vec::new();
        let mut transparent_entries = Vec::new();

        // Separate cubes by transparency
        for inst in instance_collector.cube_instances() {
            let idx = inst.material as usize;
            let is_transparent = if idx < mats.len() {
                mats[idx].is_transparent()
            } else {
                false
            };

            if is_transparent {
                transparent_entries.push((cube_mesh_handle, *inst));
            } else {
                cube_opaque.push(*inst);
            }
        }

        // Separate spheres by transparency
        for inst in instance_collector.sphere_instances() {
            let idx = inst.material as usize;
            let is_transparent = if idx < mats.len() {
                mats[idx].is_transparent()
            } else {
                false
            };

            if is_transparent {
                transparent_entries.push((mesh_handle, *inst));
            } else {
                sphere_opaque.push(*inst);
            }
        }

        (cube_opaque, sphere_opaque, transparent_entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test instance with identity transform.
    fn test_instance() -> InstanceGpu {
        InstanceGpu {
            model: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            material: 0,
            object_type: 0,
            padding: [0, 0],
        }
    }

    #[test]
    fn test_prepared_scene_empty() {
        let prepared = PreparedScene::empty();
        assert!(prepared.is_empty());
        assert_eq!(prepared.cube_opaque.len(), 0);
        assert_eq!(prepared.sphere_opaque.len(), 0);
        assert_eq!(prepared.transparent_entries.len(), 0);
        assert!(!prepared.materials_uploaded);
    }

    #[test]
    fn test_prepared_scene_is_empty_with_cubes() {
        let mut prepared = PreparedScene::empty();
        prepared.cube_opaque.push(test_instance());
        assert!(!prepared.is_empty());
    }

    #[test]
    fn test_prepared_scene_is_empty_with_spheres() {
        let mut prepared = PreparedScene::empty();
        prepared.sphere_opaque.push(test_instance());
        assert!(!prepared.is_empty());
    }

    #[test]
    fn test_prepared_scene_is_empty_with_transparent() {
        let mut prepared = PreparedScene::empty();
        prepared.transparent_entries.push((0, test_instance()));
        assert!(!prepared.is_empty());
    }
}
