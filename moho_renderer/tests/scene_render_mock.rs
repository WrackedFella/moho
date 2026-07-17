use glam::Vec3;
use legion::World;
use moho_game::actors::{Cube, Sphere};
use moho_renderer::FrameCallback;
use moho_renderer::{MaterialGpu, RendererBackend};

// A tiny mock RendererBackend that records calls for assertions.
struct MockRenderer {
    pub renders: std::cell::RefCell<Vec<String>>,
}

impl MockRenderer {
    fn new() -> Self {
        Self {
            renders: std::cell::RefCell::new(Vec::new()),
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
        _indices: &[u32],
    ) -> u32 {
        0
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
    fn enqueue_draw(&mut self, _mesh: u32, _instances: &[moho_renderer::InstanceGpu]) {
        self.renders
            .borrow_mut()
            .push("render_mesh_called".to_string());
    }
    fn submit_frame(&mut self) {}
    fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
    fn set_frame_callback_raw(&mut self, _ptr: Option<*mut dyn FrameCallback>) {}
    fn set_frame_callback_arc(
        &mut self,
        _cb: Option<std::sync::Arc<std::sync::Mutex<dyn FrameCallback>>>,
    ) {
    }
}

#[test]
fn scene_render_invokes_renderer_backend_calls() {
    // Build a simple world with one sphere and one cube.
    let mut world = World::default();
    let s = Sphere::new(
        Vec3::new(0.0, 0.0, 0.0),
        1.0,
        moho_core::materials::MaterialType::Lambertian {
            albedo: Vec3::new(0.2, 0.3, 0.4),
        },
    );
    let c = Cube::new(
        Vec3::new(1.0, 0.0, 0.0),
        1.0,
        1.0,
        1.0,
        moho_core::materials::MaterialType::Metal {
            albedo: Vec3::new(0.6, 0.6, 0.6),
            fuzz: 0.1,
        },
    );
    world.push((s,));
    world.push((c,));

    let mut scene = moho_renderer::Scene::new();
    let mut mock = MockRenderer::new();

    // Register meshes (mock returns handles 0)
    let mesh_handle = mock.register_mesh(&[[0.0f32; 3]]);
    let cube_mesh_handle = mock.register_mesh(&[[0.0f32; 3]]);

    // Compose a camera
    let camera = (
        glam::Mat4::IDENTITY,
        glam::Mat4::IDENTITY,
        Vec3::new(0.0, 0.0, 5.0),
    );

    // Call render — it should call into the mock's enqueue_draw several times.
    scene
        .render::<Sphere, Cube>(
            &mut mock,
            &mut world,
            mesh_handle,
            cube_mesh_handle,
            0,
            camera,
        )
        .expect("mock renderer's begin_frame always succeeds");

    let calls = mock.renders.borrow();
    assert!(
        calls.iter().any(|s| s == "render_mesh_called"),
        "expected render_mesh to be called"
    );
}
