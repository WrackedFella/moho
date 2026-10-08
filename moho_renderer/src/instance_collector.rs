use crate::{BufferManager, MaterialTable, RendererBackend};
use moho_core::voxel::VoxelChunk;
use moho_render_api::{InstanceGpu, Renderable};

/// Per-frame collection of renderable instances, deduplicating materials.
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

    /// Append sphere and cube instances, deduplicating their materials in
    /// `material_table`.
    ///
    /// Appends only; the caller clears per frame via [`InstanceCollector::clear`].
    /// `S`/`C` are generic so this crate doesn't need to name the concrete
    /// game-domain actor types — the caller supplies them.
    pub fn collect_actors<S, C>(
        &mut self,
        spheres: &[S],
        cubes: &[C],
        material_table: &mut MaterialTable,
    ) where
        S: Renderable,
        C: Renderable,
    {
        for s in spheres {
            let midx = material_table.find_or_push(s.material());
            self.sphere_instances
                .push(s.to_instance_with_material(midx));
        }

        for c in cubes {
            let midx = material_table.find_or_push(c.material());
            self.cube_instances.push(c.to_instance_with_material(midx));
        }
    }

    /// Register chunk meshes with the backend and append one render entry per
    /// registered chunk.
    ///
    /// Appends only; the caller clears per frame via [`InstanceCollector::clear`].
    /// `terrain_material_idx` is the material table index for terrain chunks,
    /// registered once by the caller at startup. All terrain chunks share that
    /// entry; colours come from the material buffer, not the shader.
    pub fn collect_chunks<'a>(
        &mut self,
        chunks: impl IntoIterator<Item = &'a mut VoxelChunk>,
        buffer_manager: &mut BufferManager,
        renderer: &mut dyn RendererBackend,
        terrain_material_idx: u32,
    ) {
        for chunk in chunks {
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
    use moho_core::voxel::ChunkStore;

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
        let mut material_table = MaterialTable::new();

        let mat = MaterialType::Lambertian {
            albedo: glam::Vec3::new(1.0, 0.0, 0.0),
        };
        let spheres = vec![
            TestSphere::new(glam::Vec3::ZERO, 1.0, mat),
            TestSphere::new(glam::Vec3::new(5.0, 0.0, 0.0), 2.0, mat),
        ];
        let cubes: Vec<TestCube> = Vec::new();

        collector.collect_actors(&spheres, &cubes, &mut material_table);

        assert_eq!(collector.sphere_instances().len(), 2);
        assert_eq!(collector.cube_instances().len(), 0);
        assert_eq!(collector.chunk_renders().len(), 0);
        assert_eq!(collector.total_instances(), 2);
    }

    #[test]
    fn test_collect_chunks() {
        let mut collector = InstanceCollector::new();
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
        let mut store = ChunkStore::new();
        store.insert(chunk);

        collector.collect_chunks(store.iter_mut(), &mut buffer_manager, &mut renderer, 0);

        assert_eq!(collector.sphere_instances().len(), 0);
        assert_eq!(collector.cube_instances().len(), 0);
        assert_eq!(collector.chunk_renders().len(), 1);
        assert_eq!(collector.total_instances(), 1);

        // Check that the chunk was registered
        let (handle, _inst) = collector.chunk_renders()[0];
        assert_eq!(handle, 1); // First mesh handle from mock renderer
    }

    #[test]
    fn collect_routes_spheres_cubes_and_chunks_to_their_lists() {
        let mut collector = InstanceCollector::new();
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

        let spheres = vec![TestSphere::new(glam::Vec3::ZERO, 1.0, mat1)];
        let cubes = vec![
            TestCube::new(glam::Vec3::new(3.0, 0.0, 0.0), 1.0, 1.0, 1.0, mat2),
            TestCube::new(glam::Vec3::new(6.0, 0.0, 0.0), 2.0, 2.0, 2.0, mat2),
        ];

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
        let mut store = ChunkStore::new();
        store.insert(chunk);

        collector.collect_actors(&spheres, &cubes, &mut material_table);
        collector.collect_chunks(store.iter_mut(), &mut buffer_manager, &mut renderer, 0);

        assert_eq!(collector.sphere_instances().len(), 1);
        assert_eq!(collector.cube_instances().len(), 2);
        assert_eq!(collector.chunk_renders().len(), 1);
        assert_eq!(collector.total_instances(), 4);
    }

    #[test]
    fn test_clear() {
        let mut collector = InstanceCollector::new();
        let mut material_table = MaterialTable::new();

        // Add some objects
        let mat = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        let spheres = vec![TestSphere::new(glam::Vec3::ZERO, 1.0, mat)];
        let cubes: Vec<TestCube> = Vec::new();

        collector.collect_actors(&spheres, &cubes, &mut material_table);
        assert_eq!(collector.total_instances(), 1);

        collector.clear();
        assert_eq!(collector.total_instances(), 0);
        assert_eq!(collector.sphere_instances().len(), 0);
    }

    #[test]
    fn collect_actors_appends_without_clearing() {
        let mut collector = InstanceCollector::new();
        let mut material_table = MaterialTable::new();
        let mat = MaterialType::Lambertian {
            albedo: glam::Vec3::ONE,
        };
        let first = vec![TestSphere::new(glam::Vec3::ZERO, 1.0, mat)];
        let second = vec![TestSphere::new(glam::Vec3::new(2.0, 0.0, 0.0), 1.0, mat)];
        let no_cubes: Vec<TestCube> = Vec::new();

        collector.collect_actors(&first, &no_cubes, &mut material_table);
        collector.collect_actors(&second, &no_cubes, &mut material_table);

        assert_eq!(collector.sphere_instances().len(), 2);

        collector.clear();

        assert_eq!(collector.sphere_instances().len(), 0);
    }

    #[test]
    fn collect_chunks_renders_every_chunk_in_ascending_position_order() {
        let mut collector = InstanceCollector::new();
        let mut buffer_manager = BufferManager::new();
        let mut renderer = MockRenderer::new();
        let mut store = ChunkStore::new();
        for pos in [
            glam::IVec3::new(2, 0, 0),
            glam::IVec3::new(-1, 0, 3),
            glam::IVec3::new(0, 0, 0),
        ] {
            store.insert(VoxelChunk::new(
                pos,
                vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                vec![[0.0, 0.0, 1.0]; 3],
                vec![1.0; 3],
                vec![0; 3],
                vec![1.0; 3],
                vec![[1.0, 1.0, 1.0]; 3],
                vec![1.0; 3],
                vec![0, 1, 2],
                0,
            ));
        }

        collector.collect_chunks(store.iter_mut(), &mut buffer_manager, &mut renderer, 0);

        let handles: Vec<u32> = collector.chunk_renders().iter().map(|(h, _)| *h).collect();
        assert_eq!(handles, vec![1, 2, 3]);
        let ascending = [
            glam::IVec3::new(-1, 0, 3),
            glam::IVec3::new(0, 0, 0),
            glam::IVec3::new(2, 0, 0),
        ];
        for (pos, handle) in ascending.iter().zip(&handles) {
            let chunk = store.get(*pos).expect("chunk present");
            assert_eq!(chunk.get_mesh_handle(), Some(*handle));
        }
    }
}
