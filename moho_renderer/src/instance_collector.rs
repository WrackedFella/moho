use crate::MaterialTable;
use moho_render_api::{InstanceGpu, Renderable};

/// Per-frame collection of renderable instances, deduplicating materials.
#[derive(Debug)]
pub struct InstanceCollector {
    sphere_instances: Vec<InstanceGpu>,
    cube_instances: Vec<InstanceGpu>,
}

impl InstanceCollector {
    /// Create a new InstanceCollector.
    pub fn new() -> Self {
        Self {
            sphere_instances: Vec::new(),
            cube_instances: Vec::new(),
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

    /// Clear all collected instances.
    pub fn clear(&mut self) {
        self.sphere_instances.clear();
        self.cube_instances.clear();
    }

    /// Get the collected sphere instances.
    pub fn sphere_instances(&self) -> &[InstanceGpu] {
        &self.sphere_instances
    }

    /// Get the collected cube instances.
    pub fn cube_instances(&self) -> &[InstanceGpu] {
        &self.cube_instances
    }

    /// Get the total number of collected instances.
    pub fn total_instances(&self) -> usize {
        self.sphere_instances.len() + self.cube_instances.len()
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
    use moho_render_api::{MaterialGpu, MaterialKey, RenderMaterial};

    /// Flat-colour material stand-in; the renderer must not name game material types.
    #[derive(Copy, Clone, Debug)]
    struct TestMaterial([f32; 3]);

    impl RenderMaterial for TestMaterial {
        fn to_gpu(&self) -> MaterialGpu {
            MaterialGpu {
                albedo: [self.0[0], self.0[1], self.0[2], 1.0],
                params: [0.0; 4],
            }
        }

        fn dedup_key(&self) -> MaterialKey {
            MaterialKey {
                variant: 0,
                albedo_bits: self.0.map(f32::to_bits),
                fuzz_bits: 0,
                ref_idx_bits: 0,
                extra_bits: [0; 3],
            }
        }
    }

    /// Minimal sphere-like `Renderable` test fixture. `moho_renderer` must not
    /// depend on `moho_game`, so tests use local stand-ins instead of the real
    /// `Sphere`/`Cube` actor types.
    #[derive(Copy, Clone, Debug)]
    struct TestSphere {
        center: glam::Vec3,
        material: TestMaterial,
    }

    impl TestSphere {
        fn new(center: glam::Vec3, _radius: f32, material: TestMaterial) -> Self {
            Self { center, material }
        }
    }

    impl Renderable for TestSphere {
        type Material = TestMaterial;

        fn material(&self) -> &TestMaterial {
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
        material: TestMaterial,
    }

    impl TestCube {
        fn new(
            center: glam::Vec3,
            _length: f32,
            _width: f32,
            _height: f32,
            material: TestMaterial,
        ) -> Self {
            Self { center, material }
        }
    }

    impl Renderable for TestCube {
        type Material = TestMaterial;

        fn material(&self) -> &TestMaterial {
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

    #[test]
    fn test_instance_collector_new() {
        let collector = InstanceCollector::new();
        assert_eq!(collector.total_instances(), 0);
        assert_eq!(collector.sphere_instances().len(), 0);
        assert_eq!(collector.cube_instances().len(), 0);
    }

    #[test]
    fn test_collect_spheres() {
        let mut collector = InstanceCollector::new();
        let mut material_table = MaterialTable::new();

        let mat = TestMaterial([1.0, 0.0, 0.0]);
        let spheres = vec![
            TestSphere::new(glam::Vec3::ZERO, 1.0, mat),
            TestSphere::new(glam::Vec3::new(5.0, 0.0, 0.0), 2.0, mat),
        ];
        let cubes: Vec<TestCube> = Vec::new();

        collector.collect_actors(&spheres, &cubes, &mut material_table);

        assert_eq!(collector.sphere_instances().len(), 2);
        assert_eq!(collector.cube_instances().len(), 0);
        assert_eq!(collector.total_instances(), 2);
    }

    #[test]
    fn collect_routes_spheres_and_cubes_to_their_lists() {
        let mut collector = InstanceCollector::new();
        let mut material_table = MaterialTable::new();
        let mat1 = TestMaterial([1.0, 0.0, 0.0]);
        let mat2 = TestMaterial([0.8, 0.8, 0.8]);
        let spheres = vec![TestSphere::new(glam::Vec3::ZERO, 1.0, mat1)];
        let cubes = vec![
            TestCube::new(glam::Vec3::new(3.0, 0.0, 0.0), 1.0, 1.0, 1.0, mat2),
            TestCube::new(glam::Vec3::new(6.0, 0.0, 0.0), 2.0, 2.0, 2.0, mat2),
        ];

        collector.collect_actors(&spheres, &cubes, &mut material_table);

        assert_eq!(collector.sphere_instances().len(), 1);
        assert_eq!(collector.cube_instances().len(), 2);
        assert_eq!(collector.total_instances(), 3);
    }

    #[test]
    fn test_clear() {
        let mut collector = InstanceCollector::new();
        let mut material_table = MaterialTable::new();

        // Add some objects
        let mat = TestMaterial(glam::Vec3::ONE.to_array());
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
        let mat = TestMaterial(glam::Vec3::ONE.to_array());
        let first = vec![TestSphere::new(glam::Vec3::ZERO, 1.0, mat)];
        let second = vec![TestSphere::new(glam::Vec3::new(2.0, 0.0, 0.0), 1.0, mat)];
        let no_cubes: Vec<TestCube> = Vec::new();

        collector.collect_actors(&first, &no_cubes, &mut material_table);
        collector.collect_actors(&second, &no_cubes, &mut material_table);

        assert_eq!(collector.sphere_instances().len(), 2);

        collector.clear();

        assert_eq!(collector.sphere_instances().len(), 0);
    }
}
