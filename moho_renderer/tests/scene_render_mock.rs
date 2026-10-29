use glam::Vec3;
use moho_render_api::{
    InstanceGpu, MaterialGpu, MaterialKey, RenderMaterial, Renderable, WorldMesh, WorldMeshId,
};
use moho_renderer::RendererBackend;

/// Flat-colour material stand-in; the renderer must not name game material types.
#[derive(Copy, Clone, Debug)]
struct FlatMaterial([f32; 3], bool);

impl RenderMaterial for FlatMaterial {
    fn to_gpu(&self) -> MaterialGpu {
        MaterialGpu {
            albedo: [self.0[0], self.0[1], self.0[2], 1.0],
            params: [0.0, 0.0, if self.1 { 0.5 } else { 0.0 }, 0.0],
        }
    }

    fn dedup_key(&self) -> MaterialKey {
        MaterialKey {
            variant: u8::from(self.1),
            albedo_bits: self.0.map(f32::to_bits),
            fuzz_bits: 0,
            ref_idx_bits: 0,
            extra_bits: [0; 3],
        }
    }
}

/// Renderable stand-in for a sphere (object type 2) or cube (object type 1).
#[derive(Copy, Clone, Debug)]
struct Prop {
    center: Vec3,
    material: FlatMaterial,
    object_type: u32,
}

impl Renderable for Prop {
    type Material = FlatMaterial;

    fn material(&self) -> &FlatMaterial {
        &self.material
    }

    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        InstanceGpu {
            model: glam::Mat4::from_translation(self.center).to_cols_array_2d(),
            material: material_index,
            object_type: self.object_type,
            padding: [0, 0],
        }
    }
}

// A tiny mock RendererBackend that records calls for assertions.
struct MockRenderer {
    pub renders: std::cell::RefCell<Vec<String>>,
    pub drawn_handles: std::cell::RefCell<Vec<u32>>,
    /// Each enqueue as `(unshadowed, mesh handle, instances)`, in call order.
    pub enqueues: Vec<(bool, u32, Vec<InstanceGpu>)>,
    /// How many indexed meshes were registered.
    pub uploads: u32,
    next_handle: u32,
}

impl MockRenderer {
    fn new() -> Self {
        Self {
            renders: std::cell::RefCell::new(Vec::new()),
            drawn_handles: std::cell::RefCell::new(Vec::new()),
            enqueues: Vec::new(),
            uploads: 0,
            next_handle: 100,
        }
    }
}

impl RendererBackend for MockRenderer {
    fn update_lighting(&mut self, _lighting: moho_renderer::LightingGpu) {}
    fn resize(&mut self, _width: u32, _height: u32) {}
    fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
        0
    }
    fn register_indexed_mesh(
        &mut self,
        _vertices: &[[f32; 3]],
        _normals: &[[f32; 3]],
        _ao: &[f32],
        _geometry_type: &[u32],
        _light_level: &[f32],
        _block_light_rgb: &[[f32; 3]],
        _sky_exposed: &[f32],
        _material: &[u32],
        _indices: &[u32],
    ) -> u32 {
        self.uploads += 1;
        self.next_handle += 1;
        self.next_handle
    }
    fn unregister_mesh(&mut self, _mesh: u32) {}
    fn add_point_light(
        &mut self,
        _position: glam::Vec3,
        _color: glam::Vec3,
        _intensity: f32,
        _range: f32,
    ) -> u32 {
        0
    }
    fn remove_light(&mut self, _id: u32) -> bool {
        true
    }
    fn set_light_position(&mut self, _id: u32, _position: glam::Vec3) {}
    fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
    fn set_shadow_quality(&mut self, _quality: u8) {}
    fn set_ssao_quality(&mut self, _quality: u8) {}
    fn begin_frame(
        &mut self,
        _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) -> Result<(), moho_renderer::FrameError> {
        Ok(())
    }
    fn enqueue_draw(&mut self, mesh: u32, instances: &[moho_renderer::InstanceGpu]) {
        self.enqueues.push((false, mesh, instances.to_vec()));
        self.drawn_handles.borrow_mut().push(mesh);
        self.renders
            .borrow_mut()
            .push("render_mesh_called".to_string());
    }
    fn enqueue_unshadowed_draw(&mut self, mesh: u32, instances: &[moho_renderer::InstanceGpu]) {
        self.enqueues.push((true, mesh, instances.to_vec()));
    }
    fn submit_frame(&mut self) {}
    fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
}

fn camera() -> (glam::Mat4, glam::Mat4, Vec3) {
    (
        glam::Mat4::IDENTITY,
        glam::Mat4::IDENTITY,
        Vec3::new(0.0, 0.0, 5.0),
    )
}

#[test]
fn scene_render_invokes_renderer_backend_calls() {
    let s = Prop {
        center: Vec3::ZERO,
        material: FlatMaterial([0.2, 0.3, 0.4], false),
        object_type: 2,
    };
    let c = Prop {
        center: Vec3::new(1.0, 0.0, 0.0),
        material: FlatMaterial([0.6, 0.6, 0.6], false),
        object_type: 1,
    };
    let spheres = [s];
    let cubes = [c];

    let mut scene = moho_renderer::Scene::new();
    let mut mock = MockRenderer::new();

    // Register meshes (mock returns handles 0)
    let mesh_handle = mock.register_mesh(&[[0.0f32; 3]]);
    let cube_mesh_handle = mock.register_mesh(&[[0.0f32; 3]]);

    // Call render — it should call into the mock's enqueue_draw several times.
    scene
        .render::<Prop, Prop>(
            &mut mock,
            &spheres,
            &cubes,
            mesh_handle,
            cube_mesh_handle,
            camera(),
        )
        .expect("mock renderer's begin_frame always succeeds");

    let calls = mock.renders.borrow();
    assert!(
        calls.iter().any(|s| s == "render_mesh_called"),
        "expected render_mesh to be called"
    );
}

#[test]
fn non_voxel_mesh_is_in_draw_list() {
    let triangle = WorldMesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        vec![[0.0, 1.0, 0.0]; 3],
        vec![1.0; 3],
        vec![[1.0; 3]; 3],
        vec![1.0; 3],
        vec![0; 3],
        vec![0, 1, 2],
    )
    .expect("well-formed triangle");
    let mut scene = moho_renderer::Scene::new();
    let mut mock = MockRenderer::new();
    let sphere_handle = mock.register_mesh(&[[0.0f32; 3]]);
    let cube_handle = mock.register_mesh(&[[0.0f32; 3]]);
    scene
        .world_meshes_mut()
        .upsert(WorldMeshId(42), triangle, 0);

    scene
        .render::<Prop, Prop>(&mut mock, &[], &[], sphere_handle, cube_handle, camera())
        .expect("mock renderer's begin_frame always succeeds");

    let drawn = mock.drawn_handles.borrow();
    let world_handles: Vec<u32> = drawn
        .iter()
        .copied()
        .filter(|h| *h != sphere_handle && *h != cube_handle)
        .collect();
    assert_eq!(
        world_handles.len(),
        1,
        "the static triangle is drawn once, drawn = {drawn:?}"
    );
    assert!(
        world_handles[0] > 100,
        "the drawn handle is the one the backend returned for the triangle"
    );
}

#[test]
fn a_streak_is_enqueued_unshadowed_after_the_world_draws() {
    let triangle = WorldMesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        vec![[0.0, 1.0, 0.0]; 3],
        vec![1.0; 3],
        vec![[1.0; 3]; 3],
        vec![1.0; 3],
        vec![0; 3],
        vec![0, 1, 2],
    )
    .expect("well-formed triangle");
    let mut scene = moho_renderer::Scene::new();
    let mut mock = MockRenderer::new();
    let sphere_handle = mock.register_mesh(&[[0.0f32; 3]]);
    let cube_handle = mock.register_mesh(&[[0.0f32; 3]]);
    scene
        .world_meshes_mut()
        .upsert(WorldMeshId(42), triangle, 0);
    let effects = scene.effects_mut();
    effects.set_quality(4);
    let streak = effects.begin_streak(moho_renderer::StreakStyle::new(2.0, 0.1, 0));
    let glass = [Prop {
        center: Vec3::new(0.0, 0.0, 1.0),
        material: FlatMaterial([0.5, 0.5, 0.5], true),
        object_type: 2,
    }];

    for x in [10.0, 11.0, 12.0] {
        scene
            .effects_mut()
            .place_streak(streak, Vec3::new(x, 1.0, 0.0), Vec3::X);
        scene
            .render::<Prop, Prop>(&mut mock, &glass, &[], sphere_handle, cube_handle, camera())
            .expect("mock renderer's begin_frame always succeeds");
        scene
            .effects_mut()
            .advance(std::time::Duration::from_millis(16));
    }

    assert_eq!(
        mock.uploads, 2,
        "the triangle and the streak mesh are registered once over three frames"
    );
    let frames: Vec<&(bool, u32, Vec<InstanceGpu>)> = mock
        .enqueues
        .iter()
        .filter(|(_, mesh, _)| *mesh != sphere_handle && *mesh != cube_handle)
        .collect();
    assert_eq!(frames.len(), 6, "a world draw and a streak draw per frame");
    for (frame, x) in [10.0f32, 11.0, 12.0].into_iter().enumerate() {
        let (world, streak) = (frames[frame * 2], frames[frame * 2 + 1]);
        assert!(!world.0, "the triangle is an ordinary draw");
        assert!(streak.0, "the streak is an unshadowed draw");
        assert_ne!(streak.1, world.1);
        assert_eq!(streak.2.len(), 1);
        assert_eq!(streak.2[0].model[3][..3], [x - 2.0, 1.0, 0.0], "the tail");
    }
    let positions = |matches: &dyn Fn(&(bool, u32, Vec<InstanceGpu>)) -> bool| -> Vec<usize> {
        (0..mock.enqueues.len())
            .filter(|i| matches(&mock.enqueues[*i]))
            .collect()
    };
    let streaks = positions(&|e| e.0);
    let transparent = positions(&|e| !e.0 && e.1 == sphere_handle && !e.2.is_empty());
    let worlds = positions(&|e| !e.0 && e.1 != sphere_handle && e.1 != cube_handle);
    assert_eq!(transparent.len(), 3, "one transparent entry per frame");
    for frame in 0..3 {
        assert!(
            worlds[frame] < streaks[frame] && streaks[frame] < transparent[frame],
            "frame {frame}: world, then streak, then transparent; got {:?}",
            mock.enqueues.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>()
        );
    }
    assert!(
        mock.enqueues.iter().all(|(unshadowed, mesh, _)| {
            *unshadowed == frames.iter().any(|f| f.0 && f.1 == *mesh)
        }),
        "the streak mesh is never queued as an ordinary draw"
    );
}
