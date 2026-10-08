//! Draw ordering, opaque/transparent routing and material upload of `Scene::render`,
//! observed through a recording `RendererBackend`.

use glam::Vec3;
use moho_render_api::{InstanceGpu, MaterialGpu, MaterialKey, RenderMaterial, Renderable};
use moho_renderer::{FrameCallback, FrameError, LightingGpu, RendererBackend, Scene};

const SPHERE_MESH: u32 = 1;
const CUBE_MESH: u32 = 2;

#[derive(Copy, Clone, Debug)]
struct TestMaterial {
    id: u8,
    transparent: bool,
}

const OPAQUE: TestMaterial = TestMaterial {
    id: 0,
    transparent: false,
};
const GLASS: TestMaterial = TestMaterial {
    id: 1,
    transparent: true,
};
const OTHER_OPAQUE: TestMaterial = TestMaterial {
    id: 2,
    transparent: false,
};

impl RenderMaterial for TestMaterial {
    fn to_gpu(&self) -> MaterialGpu {
        MaterialGpu {
            albedo: [1.0; 4],
            params: [0.0, 0.0, if self.transparent { 0.5 } else { 0.0 }, 0.0],
        }
    }

    fn dedup_key(&self) -> MaterialKey {
        MaterialKey {
            variant: self.id,
            albedo_bits: [0; 3],
            fuzz_bits: 0,
            ref_idx_bits: 0,
            extra_bits: [0; 3],
        }
    }
}

/// Renderable placed on the x axis. `forced_index` overrides the material index
/// the table handed out, to model an index outside the table.
#[derive(Copy, Clone, Debug)]
struct Prop {
    x: f32,
    material: TestMaterial,
    forced_index: Option<u32>,
}

fn prop(x: f32, material: TestMaterial) -> Prop {
    Prop {
        x,
        material,
        forced_index: None,
    }
}

impl Renderable for Prop {
    type Material = TestMaterial;

    fn material(&self) -> &TestMaterial {
        &self.material
    }

    fn to_instance_with_material(&self, material_index: u32) -> InstanceGpu {
        InstanceGpu {
            model: glam::Mat4::from_translation(Vec3::new(self.x, 0.0, 0.0)).to_cols_array_2d(),
            material: self.forced_index.unwrap_or(material_index),
            object_type: 0,
            padding: [0, 0],
        }
    }
}

#[derive(Debug, PartialEq)]
struct Draw {
    mesh: u32,
    xs: Vec<f32>,
}

#[derive(Default)]
struct RecordingRenderer {
    draws: Vec<Draw>,
    material_uploads: Vec<usize>,
}

impl RecordingRenderer {
    /// Draw calls that carried at least one instance, in submission order.
    fn non_empty_draws(&self) -> Vec<&Draw> {
        self.draws.iter().filter(|d| !d.xs.is_empty()).collect()
    }
}

impl RendererBackend for RecordingRenderer {
    fn update_lighting(&mut self, _lighting: LightingGpu) {}
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
        _position: Vec3,
        _color: Vec3,
        _intensity: f32,
        _range: f32,
    ) -> u32 {
        0
    }
    fn remove_light(&mut self, _id: u32) -> bool {
        true
    }
    fn set_light_position(&mut self, _id: u32, _position: Vec3) {}
    fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
    fn set_shadow_quality(&mut self, _quality: u8) {}
    fn set_ssao_quality(&mut self, _quality: u8) {}
    fn begin_frame(&mut self, _camera: (glam::Mat4, glam::Mat4, Vec3)) -> Result<(), FrameError> {
        Ok(())
    }
    fn enqueue_draw(&mut self, mesh: u32, instances: &[InstanceGpu]) {
        self.draws.push(Draw {
            mesh,
            xs: instances.iter().map(|i| i.model[3][0]).collect(),
        });
    }
    fn submit_frame(&mut self) {}
    fn set_materials(&mut self, materials: &[MaterialGpu]) {
        self.material_uploads.push(materials.len());
    }
    fn set_frame_callback_arc(
        &mut self,
        _cb: Option<std::sync::Arc<std::sync::Mutex<dyn FrameCallback>>>,
    ) {
    }
}

fn render_frame(
    scene: &mut Scene,
    renderer: &mut RecordingRenderer,
    spheres: &[Prop],
    cubes: &[Prop],
) {
    let eye_at_origin = (glam::Mat4::IDENTITY, glam::Mat4::IDENTITY, Vec3::ZERO);
    scene
        .render::<Prop, Prop>(
            renderer,
            spheres,
            cubes,
            SPHERE_MESH,
            CUBE_MESH,
            eye_at_origin,
        )
        .expect("recording renderer's begin_frame always succeeds");
}

fn draw(mesh: u32, xs: &[f32]) -> Draw {
    Draw {
        mesh,
        xs: xs.to_vec(),
    }
}

#[test]
fn transparent_instances_draw_back_to_front_grouped_by_mesh() {
    let mut scene = Scene::new();
    let mut renderer = RecordingRenderer::default();
    let spheres = [prop(5.0, GLASS), prop(1.0, GLASS)];
    let cubes = [prop(4.0, GLASS), prop(3.0, GLASS)];

    render_frame(&mut scene, &mut renderer, &spheres, &cubes);

    let calls = renderer.non_empty_draws();
    assert_eq!(
        calls,
        vec![
            &draw(SPHERE_MESH, &[5.0]),
            &draw(CUBE_MESH, &[4.0, 3.0]),
            &draw(SPHERE_MESH, &[1.0]),
        ]
    );
}

#[test]
fn transparent_material_routes_to_transparent_pass() {
    let mut scene = Scene::new();
    let mut renderer = RecordingRenderer::default();
    let spheres = [prop(1.0, OPAQUE), prop(4.0, GLASS)];
    let cubes = [prop(3.0, OPAQUE), prop(2.0, GLASS)];

    render_frame(&mut scene, &mut renderer, &spheres, &cubes);

    let calls = renderer.non_empty_draws();
    assert_eq!(
        calls,
        vec![
            &draw(CUBE_MESH, &[3.0]),
            &draw(SPHERE_MESH, &[1.0]),
            &draw(SPHERE_MESH, &[4.0]),
            &draw(CUBE_MESH, &[2.0]),
        ]
    );
}

#[test]
fn out_of_range_material_index_draws_opaque() {
    let mut scene = Scene::new();
    let mut renderer = RecordingRenderer::default();
    let stray = Prop {
        x: 3.0,
        material: OPAQUE,
        forced_index: Some(999),
    };
    let spheres = [stray, prop(7.0, GLASS)];

    render_frame(&mut scene, &mut renderer, &spheres, &[]);

    let calls = renderer.non_empty_draws();
    assert_eq!(
        calls,
        vec![&draw(SPHERE_MESH, &[3.0]), &draw(SPHERE_MESH, &[7.0])],
        "the stray instance is an opaque call ahead of the transparent one"
    );
}

#[test]
fn materials_upload_only_when_table_changed() {
    let mut scene = Scene::new();
    let mut renderer = RecordingRenderer::default();
    let spheres = [prop(1.0, OPAQUE)];

    render_frame(&mut scene, &mut renderer, &spheres, &[]);
    render_frame(&mut scene, &mut renderer, &spheres, &[]);

    assert_eq!(renderer.material_uploads, vec![1]);

    let with_new_material = [prop(1.0, OPAQUE), prop(2.0, OTHER_OPAQUE)];
    render_frame(&mut scene, &mut renderer, &with_new_material, &[]);

    assert_eq!(renderer.material_uploads, vec![1, 2]);
}
