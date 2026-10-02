/// Instance collection from ECS world for scene rendering.
///
/// This module handles collecting instances from the ECS world and
/// managing material deduplication through the material table.
use crate::{BufferManager, MaterialTable, RendererBackend};
use legion::World;
use legion::query::IntoQuery;
use legion::storage::Component;
use moho_core::voxel::VoxelChunk;
use moho_render_api::{InstanceGpu, Renderable};

/// Collects instances from the ECS world for rendering.
///
/// This struct handles the per-frame collection of renderable objects
/// from the ECS world, including material deduplication and mesh registration.
#[derive(Debug)]
pub struct InstanceCollector {
    sphere_instances: Vec<InstanceGpu>,
    cube_instances: Vec<InstanceGpu>,
    chunk_renders: Vec<(u32, InstanceGpu)>,
}

impl InstanceCollector {
    /// Create a new InstanceCollector.
    pub fn new() -> Self {
        Self {
            sphere_instances: Vec::new(),
            cube_instances: Vec::new(),
            chunk_renders: Vec::new(),
        }
    }

    /// Collect all instances from the world.
    ///
    /// This method queries the ECS world for `S` (spheres), `C` (cubes), and
    /// `VoxelChunk`s, deduplicates materials, registers chunk meshes, and
    /// prepares instance data for rendering. `S`/`C` are generic so this crate
    /// doesn't need to name the concrete game-domain actor types — the caller
    /// supplies them.
    ///
    /// # Arguments
    /// * `world` - The ECS world to query
    /// * `material_table` - Material table for deduplication
    /// * `buffer_manager` - Buffer manager for chunk mesh registration
    /// * `renderer` - Backend renderer for mesh registration
    /// * `terrain_material_idx` - Material table index for terrain chunks,
    ///   registered once by the caller at startup (mirrors the
    ///   `mesh_handle`/`cube_mesh_handle` pattern already used for meshes)
    pub fn collect_from_world<S, C>(
        &mut self,
        world: &mut World,
        material_table: &mut MaterialTable,
        buffer_manager: &mut BufferManager,
        renderer: &mut dyn RendererBackend,
        terrain_material_idx: u32,
    ) where
        S: Renderable + Component,
        C: Renderable + Component,
    {
        // Clear previous frame data
        self.clear();

        // Collect sphere-like instances and deduplicate materials
        let mut q_s = <&S>::query();
        for s in q_s.iter(world) {
            let midx = material_table.find_or_push(s.material());
            self.sphere_instances
                .push(s.to_instance_with_material(midx));
        }

        // Collect cube-like instances and deduplicate materials
        let mut q_c = <&C>::query();
        for c in q_c.iter(world) {
            let midx = material_table.find_or_push(c.material());
            self.cube_instances.push(c.to_instance_with_material(midx));
        }

        // Register VoxelChunk meshes and collect instances. All terrain chunks
        // share a single pre-registered GPU material entry sourcing colours
        // from the material buffer rather than hardcoding them in the shader.
        let mut q_chunks_mut = <&mut VoxelChunk>::query();
        for chunk in q_chunks_mut.iter_mut(world) {
            if let Some(handle) = buffer_manager.ensure_chunk_registered(chunk, renderer) {
                // VoxelChunk uses identity transform (mesh in world space)
                let inst = InstanceGpu {
                    model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                    material: terrain_material_idx,
                    object_type: 0, // terrain — fragment shader applies normal-based colour
                    padding: [0, 0],
                };
                self.chunk_renders.push((handle, inst));
            }
        }

        log::debug!(
            "[InstanceCollector] Collected {} spheres, {} cubes, {} chunks",
            self.sphere_instances.len(),
            self.cube_instances.len(),
            self.chunk_renders.len()
        );
    }

    /// Clear all collected instances.
    pub fn clear(&mut self) {
        self.sphere_instances.clear();
        self.cube_instances.clear();
        self.chunk_renders.clear();
    }

    /// Get the collected sphere instances.
    pub fn sphere_instances(&self) -> &[InstanceGpu] {
        &self.sphere_instances
    }

    /// Get the collected cube instances.
    pub fn cube_instances(&self) -> &[InstanceGpu] {
        &self.cube_instances
    }

    /// Get the collected chunk renders (mesh handle + instance pairs).
    pub fn chunk_renders(&self) -> &[(u32, InstanceGpu)] {
        &self.chunk_renders
    }

    /// Get the total number of collected instances.
    pub fn total_instances(&self) -> usize {
        self.sphere_instances.len() + self.cube_instances.len() + self.chunk_renders.len()
    }
}

impl Default for InstanceCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::materials::MaterialType;

    /// Minimal sphere-like `Renderable` test fixture. `moho_renderer` must not
    /// depend on `moho_game`, so tests use local stand-ins instead of the real
    /// `Sphere`/`Cube` actor types.
    #[derive(Copy, Clone, Debug)]
    struct TestSphere {
        center: glam::Vec3,
        material: MaterialType,
    }

    impl TestSphere {
        fn new(center: glam::Vec3, _radius: f32, material: MaterialType) -> Self {
            Self { center, material }
        }
    }

    impl Renderable for TestSphere {
        type Material = MaterialType;

        fn material(&self) -> &MaterialType {
            &self.material
        }

        fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
            InstanceGpu {
                model: glam::Mat4::from_translation(self.center).to_cols_array_2d(),
                material: material_index,
                object_type: 2,
                padding: [0, 0],
            }
        }
    }

    /// Minimal cube-like `Renderable` test fixture (see `TestSphere`).
    #[derive(Copy, Clone, Debug)]
    struct TestCube {
        center: glam::Vec3,
        material: MaterialType,
    }

    impl TestCube {
        fn new(
            center: glam::Vec3,
            _length: f32,
            _width: f32,
            _height: f32,
            material: MaterialType,
        ) -> Self {
            Self { center, material }
        }
    }

    impl Renderable for TestCube {
        type Material = MaterialType;

        fn material(&self) -> &MaterialType {
            &self.material
        }

        fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
            InstanceGpu {
                model: glam::Mat4::from_translation(self.center).to_cols_array_2d(),
                material: material_index,
                object_type: 1,
                padding: [0, 0],
            }
        }
    }

    /// Mock renderer for testing instance collection
    struct MockRenderer {
        next_handle: u32,
    }

    impl MockRenderer {
        fn new() -> Self {
            Self { next_handle: 1 }
        }
    }

    impl RendererBackend for MockRenderer {
        fn register_indexed_mesh(
            &mut self,
            _vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            _indices: &[u32],
        ) -> u32 {
            let handle = self.next_handle;
            self.next_handle += 1;
            handle
        }

        fn add_point_light(
            &mut self,
            _pos: glam::Vec3,
            _col: glam::Vec3,
            _int: f32,
            _rng: f32,
        ) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _pos: glam::Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, _quality: u8) {}
        fn set_ssao_quality(&mut self, _quality: u8) {}

        fn unregister_mesh(&mut self, _handle: u32) {}

        fn begin_frame(
            &mut self,
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), crate::FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[moho_render_api::InstanceGpu]) {}
        fn submit_frame(&mut self) {}

        fn set_materials(&mut self, _materials: &[crate::Material]) {}

        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            0
        }
        fn update_lighting(&mut self, _lighting: crate::gpu_types::LightingGpu) {}
        fn set_frame_callback_raw(&mut self, _ptr: Option<*mut dyn crate::FrameCallback>) {}
        fn set_frame_callback_arc(
            &mut self,
            _cb: Option<std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
        ) {
        }
    }

    #[test]
    fn test_instance_collector_new() {
        let collector = InstanceCollector::new();
        assert_eq!(collector.total_instances(), 0);
        assert_eq!(collector.sphere_instances().len(), 0);
        assert_eq!(collector.cube_instances().len(), 0);
        assert_eq!(collector.chunk_renders().len(), 0);
    }

    #[test]
    fn test_collect_spheres() {
        let mut collector = InstanceCollector::new();
        let mut world = World::default();
        let mut material_table = MaterialTable::new();
        let mut buffer_manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        // Add some spheres to the world
        let mat = MaterialType::Lambertian {
            albedo: glam::Vec3::new(1.0, 0.0, 0.0),
        };
        world.push((TestSphere::new(glam::Vec3::ZERO, 1.0, mat),));
        world.push((TestSphere::new(glam::Vec3::new(5.0, 0.0, 0.0), 2.0, mat),));

        collector.collect_from_world::<TestSphere, TestCube>(
            &mut world,
            &mut material_table,
            &mut buffer_manager,
            &mut renderer,
            0,
        );

        assert_eq!(collector.sphere_instances().len(), 2);
        assert_eq!(collector.cube_instances().len(), 0);
        assert_eq!(collector.chunk_renders().len(), 0);
        assert_eq!(collector.total_instances(), 2);
    }

    #[test]
    fn test_collect_cubes() {
        let mut collector = InstanceCollector::new();
        let mut world = World::default();
        let mut material_table = MaterialTable::new();
        let mut buffer_manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        // Add some cubes to the world
        let mat = MaterialType::Metal {
            albedo: glam::Vec3::new(0.8, 0.8, 0.8),
            fuzz: 0.1,
        };
        world.push((TestCube::new(glam::Vec3::ZERO, 1.0, 1.0, 1.0, mat),));
        world.push((TestCube::new(
            glam::Vec3::new(3.0, 0.0, 0.0),
            2.0,
            2.0,
            2.0,
            mat,
        ),));

        collector.collect_from_world::<TestSphere, TestCube>(
            &mut world,
            &mut material_table,
            &mut buffer_manager,
            &mut renderer,
            0,
        );

        assert_eq!(collector.sphere_instances().len(), 0);
        assert_eq!(collector.cube_instances().len(), 2);
        assert_eq!(collector.chunk_renders().len(), 0);
        assert_eq!(collector.total_instances(), 2);
    }

    #[test]
    fn test_collect_chunks() {
        let mut collector = InstanceCollector::new();
        let mut world = World::default();
        let mut material_table = MaterialTable::new();
        let mut buffer_manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        // Add a voxel chunk with geometry
        let chunk = VoxelChunk::new(
            glam::IVec3::new(0, 0, 0),
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            vec![1.0; 3],
            vec![0; 3],
            vec![1.0; 3],
            vec![[1.0, 1.0, 1.0]; 3],
            vec![1.0; 3],
            vec![0, 1, 2],
            0,
        );
        world.push((chunk,));

        collector.collect_from_world::<TestSphere, TestCube>(
            &mut world,
            &mut material_table,
            &mut buffer_manager,
            &mut renderer,
            0,
        );

        assert_eq!(collector.sphere_instances().len(), 0);
        assert_eq!(collector.cube_instances().len(), 0);
        assert_eq!(collector.chunk_renders().len(), 1);
        assert_eq!(collector.total_instances(), 1);

        // Check that the chunk was registered
        let (handle, _inst) = collector.chunk_renders()[0];
        assert_eq!(handle, 1); // First mesh handle from mock renderer
    }

    #[test]
    fn test_collect_mixed_objects() {
        let mut collector = InstanceCollector::new();
        let mut world = World::default();
        let mut material_table = MaterialTable::new();
        let mut buffer_manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        // Add various objects
        let mat1 = MaterialType::Lambertian {
            albedo: glam::Vec3::new(1.0, 0.0, 0.0),
        };
        let mat2 = MaterialType::Metal {
            albedo: glam::Vec3::new(0.8, 0.8, 0.8),
            fuzz: 0.1,
        };

        world.push((TestSphere::new(glam::Vec3::ZERO, 1.0, mat1),));
        world.push((TestCube::new(
            glam::Vec3::new(3.0, 0.0, 0.0),
            1.0,
            1.0,
            1.0,
            mat2,
        ),));

        let chunk = VoxelChunk::new(
            glam::IVec3::new(0, 0, 0),
            vec![[0.0, 0.0, 0.0]],
            vec![[0.0, 0.0, 1.0]],
            vec![1.0],
            vec![0],
            vec![1.0],
            vec![[1.0, 1.0, 1.0]],
            vec![1.0],
            vec![0],
            0,
        );
        world.push((chunk,));

        collector.collect_from_world::<TestSphere, TestCube>(
            &mut world,
            &mut material_table,
            &mut buffer_manager,
            &mut renderer,
            0,
        );

        assert_eq!(collector.sphere_instances().len(), 1);
        assert_eq!(collector.cube_instances().len(), 1);
        assert_eq!(collector.chunk_renders().len(), 1);
        assert_eq!(collector.total_instances(), 3);
    }

    #[test]
    fn test_clear() {
        let mut collector = InstanceCollector::new();
        let mut world = World::default();
        let mut material_table = MaterialTable::new();
        let mut buffer_manager = BufferManager::new();
        let mut renderer = MockRenderer::new();

        // Add some objects
        let mat = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        world.push((TestSphere::new(glam::Vec3::ZERO, 1.0, mat),));

        collector.collect_from_world::<TestSphere, TestCube>(
            &mut world,
            &mut material_table,
            &mut buffer_manager,
            &mut renderer,
            0,
        );
        assert_eq!(collector.total_instances(), 1);

        collector.clear();
        assert_eq!(collector.total_instances(), 0);
        assert_eq!(collector.sphere_instances().len(), 0);
    }
}
