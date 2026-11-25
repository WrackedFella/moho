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

/// One recorded enqueue: `(unshadowed, mesh handle, instances)`.
type Enqueue = (bool, u32, Vec<InstanceGpu>);

// A tiny mock RendererBackend that records calls for assertions.
struct MockRenderer {
    pub renders: std::cell::RefCell<Vec<String>>,
    pub drawn_handles: std::cell::RefCell<Vec<u32>>,
    pub enqueues: Vec<Enqueue>,
    /// Each `enqueue_shadow_only_draw` call as `(mesh handle, instances)`.
    pub shadow_only: Vec<(u32, Vec<InstanceGpu>)>,
    pub uploads: u32,
    next_handle: u32,
}

impl MockRenderer {
    fn new() -> Self {
        Self {
            renders: std::cell::RefCell::new(Vec::new()),
            drawn_handles: std::cell::RefCell::new(Vec::new()),
            enqueues: Vec::new(),
            shadow_only: Vec::new(),
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
    fn set_shadow_quality(&mut self, quality: u8) -> u8 {
        quality
    }
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
    fn enqueue_shadow_only_draw(&mut self, mesh: u32, instances: &[moho_renderer::InstanceGpu]) {
        self.shadow_only.push((mesh, instances.to_vec()));
    }
    fn submit_frame(&mut self) {}
    fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
}

/// A backend that records through a [`MockRenderer`] but keeps the trait's default
/// `enqueue_shadow_only_draw`.
struct DefaultShadowBackend(MockRenderer);

impl RendererBackend for DefaultShadowBackend {
    fn update_lighting(&mut self, lighting: moho_renderer::LightingGpu) {
        self.0.update_lighting(lighting);
    }
    fn resize(&mut self, width: u32, height: u32) {
        self.0.resize(width, height);
    }
    fn register_mesh(&mut self, vertices: &[[f32; 3]]) -> u32 {
        self.0.register_mesh(vertices)
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
        self.0.register_indexed_mesh(
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
    fn unregister_mesh(&mut self, mesh: u32) {
        self.0.unregister_mesh(mesh);
    }
    fn add_point_light(
        &mut self,
        position: glam::Vec3,
        color: glam::Vec3,
        intensity: f32,
        range: f32,
    ) -> u32 {
        self.0.add_point_light(position, color, intensity, range)
    }
    fn remove_light(&mut self, id: u32) -> bool {
        self.0.remove_light(id)
    }
    fn set_light_position(&mut self, id: u32, position: glam::Vec3) {
        self.0.set_light_position(id, position);
    }
    fn set_light_enabled(&mut self, id: u32, enabled: bool) {
        self.0.set_light_enabled(id, enabled);
    }
    fn set_shadow_quality(&mut self, quality: u8) -> u8 {
        self.0.set_shadow_quality(quality)
    }
    fn set_ssao_quality(&mut self, quality: u8) {
        self.0.set_ssao_quality(quality);
    }
    fn begin_frame(
        &mut self,
        camera: (glam::Mat4, glam::Mat4, Vec3),
    ) -> Result<(), moho_renderer::FrameError> {
        self.0.begin_frame(camera)
    }
    fn enqueue_draw(&mut self, mesh: u32, instances: &[moho_renderer::InstanceGpu]) {
        self.0.enqueue_draw(mesh, instances);
    }
    fn submit_frame(&mut self) {
        self.0.submit_frame();
    }
    fn set_materials(&mut self, materials: &[MaterialGpu]) {
        self.0.set_materials(materials);
    }
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
    let frames: Vec<&Enqueue> = mock
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
    let positions = |matches: &dyn Fn(&Enqueue) -> bool| -> Vec<usize> {
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

const BOX: WorldMeshId = WorldMeshId(7);

/// The handle `MockRenderer` returns for its first indexed upload.
const FIRST_UPLOAD_HANDLE: u32 = 101;

fn box_mesh() -> WorldMesh {
    WorldMesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        vec![[0.0, 1.0, 0.0]; 3],
        vec![1.0; 3],
        vec![[1.0; 3]; 3],
        vec![1.0; 3],
        vec![0; 3],
        vec![0, 1, 2],
    )
    .expect("well-formed triangle")
}

fn at(x: f32) -> glam::Affine3A {
    glam::Affine3A::from_translation(Vec3::new(x, 0.0, 0.0))
}

#[test]
fn a_shadow_only_placement_is_enqueued_shadow_only_and_never_drawn() {
    let mut scene = moho_renderer::Scene::new();
    let mut mock = MockRenderer::new();
    let sphere_handle = mock.register_mesh(&[[0.0f32; 3]]);
    let cube_handle = mock.register_mesh(&[[0.0f32; 3]]);
    scene.world_meshes_mut().upsert_movable(BOX, box_mesh());
    scene.world_meshes_mut().place_shadow_only(BOX, at(2.0));

    scene
        .render::<Prop, Prop>(&mut mock, &[], &[], sphere_handle, cube_handle, camera())
        .expect("mock renderer's begin_frame always succeeds");

    assert_eq!(mock.uploads, 1, "the box is uploaded once");
    assert_eq!(mock.shadow_only.len(), 1, "one shadow-only enqueue");
    let (handle, instances) = &mock.shadow_only[0];
    assert_eq!(*handle, FIRST_UPLOAD_HANDLE);
    assert_eq!(instances.len(), 1);
    assert_eq!(
        instances[0].model,
        glam::Mat4::from(at(2.0)).to_cols_array_2d()
    );
    assert!(
        mock.enqueues
            .iter()
            .all(|(_, mesh, _)| *mesh != FIRST_UPLOAD_HANDLE),
        "no ordinary or unshadowed draw carries the box, enqueues = {:?}",
        mock.enqueues.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>()
    );
}

#[test]
fn a_shadow_only_placement_moves_by_1_m_without_an_upload() {
    let mut scene = moho_renderer::Scene::new();
    let mut mock = MockRenderer::new();
    let sphere_handle = mock.register_mesh(&[[0.0f32; 3]]);
    let cube_handle = mock.register_mesh(&[[0.0f32; 3]]);
    scene.world_meshes_mut().upsert_movable(BOX, box_mesh());

    scene.world_meshes_mut().place_shadow_only(BOX, at(2.0));
    scene
        .render::<Prop, Prop>(&mut mock, &[], &[], sphere_handle, cube_handle, camera())
        .expect("mock renderer's begin_frame always succeeds");
    let uploads_after_first = mock.uploads;
    scene.world_meshes_mut().place_shadow_only(BOX, at(3.0));
    scene
        .render::<Prop, Prop>(&mut mock, &[], &[], sphere_handle, cube_handle, camera())
        .expect("mock renderer's begin_frame always succeeds");

    assert_eq!(uploads_after_first, 1);
    assert_eq!(mock.uploads, uploads_after_first, "moving never uploads");
    assert_eq!(mock.shadow_only.len(), 2, "one shadow-only enqueue a frame");
    let (first, second) = (&mock.shadow_only[0], &mock.shadow_only[1]);
    assert_eq!(first.0, second.0, "the same mesh both frames");
    assert_eq!(first.1.len(), 1);
    assert_eq!(second.1.len(), 1);
    let moved = second.1[0].model[3][0] - first.1[0].model[3][0];
    assert!((moved - 1.0).abs() < f32::EPSILON, "moved {moved} m");
}

#[test]
fn the_default_backend_drops_a_shadow_only_draw() {
    let mut scene = moho_renderer::Scene::new();
    let mut backend = DefaultShadowBackend(MockRenderer::new());
    let sphere_handle = backend.register_mesh(&[[0.0f32; 3]]);
    let cube_handle = backend.register_mesh(&[[0.0f32; 3]]);
    scene.world_meshes_mut().upsert_movable(BOX, box_mesh());
    scene.world_meshes_mut().place_shadow_only(BOX, at(2.0));

    scene
        .render::<Prop, Prop>(&mut backend, &[], &[], sphere_handle, cube_handle, camera())
        .expect("mock renderer's begin_frame always succeeds");

    assert_eq!(backend.0.uploads, 1, "the box is registered");
    assert!(
        backend.0.shadow_only.is_empty(),
        "the default method records nothing"
    );
    assert!(
        backend
            .0
            .enqueues
            .iter()
            .all(|(_, mesh, _)| *mesh != FIRST_UPLOAD_HANDLE),
        "the default must not fall back to a drawn enqueue"
    );
}
