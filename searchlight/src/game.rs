//! The prototype as a [`moho_app::Game`]: samples input, ticks the sim, and turns its
//! feedback into sound, a title status line, a hit flash and a crosshair.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use glam::{Mat4, Vec3};
use moho_app::{Event, EventContext, FrameContext, Game, InitContext, TickContext};
use moho_audio::AudioSystem;
use moho_input::action_map::{ActionMap, handle_device_event, handle_window_event};
use moho_physics::{CharacterState, first_person_view};
use moho_render_api::{
    InstanceGpu, MaterialGpu, MaterialKey, RenderMaterial, Renderable, UiFrame, UiFrameSource,
    UiImage, UiMesh, UiRect, UiSampler, UiSource, UiTextureId, UiTextureSet, UiVertex, WorldMesh,
    WorldMeshId,
};
use moho_renderer::{LightingGpu, RendererBackend, Scene};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::input::{FpsAction, FpsCommand};
use crate::sim::{Feedback, FpsSim, dummy_at, dummy_hit_volumes};
use crate::tag::SurfaceTag;
use crate::{TICK_HZ, hud};

/// Ticks a struck dummy segment stays lit.
const FLASH_TICKS: u32 = TICK_HZ / 6;
/// Dummy segment mesh ids start here, clear of the arena's (which count up from 0).
const DUMMY_MESH_BASE: u64 = 1 << 32;
/// A mesh handle the renderer never issues, so the empty actor draws are skipped.
const NO_MESH: u32 = u32::MAX;
/// The window title before any status line is appended.
pub const WINDOW_TITLE: &str = "Searchlight";
const FOV_DEGREES: f32 = 75.0;
const NEAR: f32 = 0.05;
const FAR: f32 = 500.0;
const SHOT_VOLUME: f32 = 0.8;

/// Width over height, treating a zero height as one so a minimised window cannot divide by zero.
fn aspect_ratio(width: u32, height: u32) -> f32 {
    width as f32 / height.max(1) as f32
}

fn window_title(status: &str) -> String {
    if status.is_empty() {
        WINDOW_TITLE.to_owned()
    } else {
        format!("{WINDOW_TITLE} | {status}")
    }
}

/// Surface colours of the greybox world.
#[derive(Clone, Copy, Debug)]
enum Paint {
    Arena,
    Dummy,
    Struck,
}

impl RenderMaterial for Paint {
    fn to_gpu(&self) -> MaterialGpu {
        // World meshes draw as terrain: `albedo` colours up-facing surfaces, `params.xyz`
        // the rest, and `params.w > 0` makes the surface glow its colour.
        let (top, side, glow) = match self {
            Paint::Arena => ([0.62, 0.62, 0.64], [0.42, 0.43, 0.46], 0.0),
            Paint::Dummy => ([0.85, 0.6, 0.2], [0.7, 0.48, 0.16], 0.0),
            Paint::Struck => ([1.0, 0.15, 0.1], [1.0, 0.15, 0.1], 1.0),
        };
        MaterialGpu {
            albedo: [top[0], top[1], top[2], 0.0],
            params: [side[0], side[1], side[2], glow],
        }
    }

    fn dedup_key(&self) -> MaterialKey {
        MaterialKey {
            variant: *self as u8,
            albedo_bits: [0; 3],
            fuzz_bits: 0,
            ref_idx_bits: 0,
            extra_bits: [0; 3],
        }
    }
}

/// An actor type with no values: the scene draws its world meshes and no actors.
enum NoActor {}

impl Renderable for NoActor {
    type Material = Paint;

    fn material(&self) -> &Paint {
        match *self {}
    }

    fn to_instance_with_material(&self, _material_index: u32) -> InstanceGpu {
        match *self {}
    }
}

/// A closed box with outward-facing triangles and a normal per face.
fn box_mesh(centre: Vec3, half: Vec3) -> WorldMesh {
    // (normal, u, v) with u x v = normal, so (-u-v, +u-v, +u+v, -u+v) winds counter-clockwise
    // seen from outside.
    const FACES: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (Vec3::NEG_X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (Vec3::NEG_Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (Vec3::NEG_Z, Vec3::Y, Vec3::X),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, u, v) in FACES {
        let first = positions.len() as u32;
        let (du, dv) = (u * half, v * half);
        let face_centre = centre + normal * half;
        for corner in [-du - dv, du - dv, du + dv, -du + dv] {
            positions.push((face_centre + corner).to_array());
            normals.push(normal.to_array());
        }
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    let count = positions.len();
    WorldMesh::new(
        positions,
        normals,
        vec![1.0; count],
        vec![[0.0; 3]; count],
        vec![1.0; count],
        vec![0; count],
        indices,
    )
    .expect("a box has matching channels and whole triangles")
}

/// One drawn body part of a dummy, lit while it flashes.
#[derive(Debug)]
struct Segment {
    tag: SurfaceTag,
    id: WorldMeshId,
    mesh: WorldMesh,
    flash_left: u32,
}

/// The dummies' segments, in the frame the world draws them in.
fn dummy_segments(spawns: &[Vec3]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (index, &spawn) in spawns.iter().enumerate() {
        let (target, centre) = dummy_at(index, spawn);
        for volume in dummy_hit_volumes(target) {
            segments.push(Segment {
                tag: SurfaceTag::try_from(volume.tag).expect("dummy volumes carry part tags"),
                id: WorldMeshId(DUMMY_MESH_BASE + segments.len() as u64),
                mesh: box_mesh(centre + volume.offset, volume.half_extents),
                flash_left: 0,
            });
        }
    }
    segments
}

/// Four short bars around the screen centre, drawn as solid rects from a 1x1 white texture.
#[derive(Debug, Default)]
struct Crosshair {
    texture_sent: bool,
}

impl Crosshair {
    const TEXTURE: UiTextureId = UiTextureId(0);
    const GAP: f32 = 4.0;
    const LENGTH: f32 = 9.0;
    const THICKNESS: f32 = 2.0;
}

impl UiFrameSource for Crosshair {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        let size = [size_in_pixels[0] as f32, size_in_pixels[1] as f32];
        let (cx, cy) = (size[0] / 2.0, size[1] / 2.0);
        let (near, far, half) = (Self::GAP, Self::GAP + Self::LENGTH, Self::THICKNESS / 2.0);
        let bars = [
            ([cx - far, cy - half], [cx - near, cy + half]),
            ([cx + near, cy - half], [cx + far, cy + half]),
            ([cx - half, cy - far], [cx + half, cy - near]),
            ([cx - half, cy + near], [cx + half, cy + far]),
        ];
        let mut vertices = Vec::with_capacity(16);
        let mut indices = Vec::with_capacity(24);
        for (min, max) in bars {
            let first = vertices.len() as u32;
            for pos in [min, [max[0], min[1]], max, [min[0], max[1]]] {
                vertices.push(UiVertex {
                    pos,
                    uv: [0.5, 0.5],
                    color: [255; 4],
                });
            }
            indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
        let textures_set = if self.texture_sent {
            Vec::new()
        } else {
            self.texture_sent = true;
            vec![UiTextureSet {
                id: Self::TEXTURE,
                pos: None,
                image: UiImage {
                    size: [1, 1],
                    pixels: vec![[255; 4]],
                },
                sampler: UiSampler::default(),
            }]
        };
        Some(UiFrame {
            pixels_per_point: 1.0,
            textures_set,
            meshes: vec![UiMesh {
                clip_rect: UiRect {
                    min: [0.0, 0.0],
                    max: size,
                },
                texture: Self::TEXTURE,
                vertices,
                indices,
            }],
            textures_free: Vec::new(),
        })
    }
}

/// Daylight from above and to one side.
fn lighting() -> LightingGpu {
    let sun = Vec3::new(0.4, 0.8, 0.45).normalize();
    LightingGpu {
        sun_direction: [sun.x, sun.y, sun.z, 1.0],
        moon_direction: [0.0, -1.0, 0.0, 0.0],
        ambient: [0.4, 0.5, 0.6, 0.15],
        params: [12.0, 0.0, 0.0, 0.0],
        ..LightingGpu::default()
    }
}

/// Keeps the pointer inside the window for mouse look; a platform that allows neither
/// grab leaves it free.
fn capture_cursor(window: &Window) {
    window.set_cursor_visible(false);
    let grabbed = window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
    if let Err(error) = grabbed {
        tracing::warn!(%error, "cursor could not be captured, mouse look may leave the window");
    }
}

/// Everything drawn or heard; the sim knows nothing of it.
#[derive(Debug)]
struct Stage {
    scene: Scene,
    segments: Vec<Segment>,
    dummy_material: u32,
    struck_material: u32,
    crosshair: Arc<Mutex<Crosshair>>,
    shot_sound: Option<PathBuf>,
    window: Option<Arc<Window>>,
    status: String,
    title: String,
}

impl Stage {
    fn new() -> Self {
        let mut scene = Scene::new();
        let dummy_material = scene.material_table.find_or_push(&Paint::Dummy);
        let struck_material = scene.material_table.find_or_push(&Paint::Struck);
        Self {
            scene,
            segments: Vec::new(),
            dummy_material,
            struck_material,
            crosshair: Arc::new(Mutex::new(Crosshair::default())),
            shot_sound: None,
            window: None,
            status: String::new(),
            title: String::new(),
        }
    }

    fn add_arena(&mut self, meshes: Vec<WorldMesh>, dummy_spawns: &[Vec3]) {
        let arena_material = self.scene.material_table.find_or_push(&Paint::Arena);
        for (index, mesh) in meshes.into_iter().enumerate() {
            self.scene.world_meshes_mut().upsert(
                crate::arena::arena_mesh_id(index),
                mesh,
                arena_material,
            );
        }
        self.segments = dummy_segments(dummy_spawns);
        for segment in &self.segments {
            self.scene.world_meshes_mut().upsert(
                segment.id,
                segment.mesh.clone(),
                self.dummy_material,
            );
        }
    }

    fn react(&mut self, feedback: &Feedback, audio: Option<&mut AudioSystem>) {
        if let Some(text) = hud::status_text(feedback) {
            self.status = text;
        }
        match feedback {
            Feedback::Fired => self.play_shot(audio),
            Feedback::Hit { target, part, .. } => {
                let tag = SurfaceTag::Part {
                    target: *target,
                    part: *part,
                };
                if let Some(segment) = self.segments.iter_mut().find(|s| s.tag == tag) {
                    segment.flash_left = FLASH_TICKS;
                    self.scene.world_meshes_mut().upsert(
                        segment.id,
                        segment.mesh.clone(),
                        self.struck_material,
                    );
                }
            }
            _ => {}
        }
    }

    fn play_shot(&self, audio: Option<&mut AudioSystem>) {
        let (Some(audio), Some(path)) = (audio, &self.shot_sound) else {
            return;
        };
        if let Err(error) = audio.play_sound(path, SHOT_VOLUME) {
            tracing::warn!(%error, path = %path.display(), "rifle shot could not be played");
        }
    }

    fn tick_flashes(&mut self) {
        for segment in &mut self.segments {
            if segment.flash_left == 0 {
                continue;
            }
            segment.flash_left -= 1;
            if segment.flash_left == 0 {
                self.scene.world_meshes_mut().upsert(
                    segment.id,
                    segment.mesh.clone(),
                    self.dummy_material,
                );
            }
        }
    }

    fn refresh_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let title = window_title(&self.status);
        if title != self.title {
            window.set_title(&title);
            self.title = title;
        }
    }

    fn aspect(&self) -> f32 {
        self.window.as_ref().map_or(16.0 / 9.0, |window| {
            let size = window.inner_size();
            aspect_ratio(size.width, size.height)
        })
    }

    fn draw(&mut self, renderer: &mut dyn RendererBackend, eye: (Mat4, Vec3)) {
        renderer.update_lighting(lighting());
        let (view, eye) = eye;
        let projection = glam::camera::rh::proj::directx::perspective(
            FOV_DEGREES.to_radians(),
            self.aspect(),
            NEAR,
            FAR,
        );
        let no_actors: &[NoActor] = &[];
        if let Err(error) = self.scene.render(
            renderer,
            no_actors,
            no_actors,
            NO_MESH,
            NO_MESH,
            (view, projection, eye),
        ) {
            tracing::warn!(%error, "skipped frame");
        }
    }
}

/// The FPS prototype's game: one sim and the input that drives it.
#[derive(Debug)]
pub struct FpsGame {
    sim: FpsSim,
    input: ActionMap<FpsAction>,
    /// The player as the tick before the last left them, for blending the view.
    previous: CharacterState,
    stage: Stage,
}

impl FpsGame {
    /// A game ticking `sim` with commands sampled from `input`.
    pub fn new(sim: FpsSim, input: ActionMap<FpsAction>) -> Self {
        let previous = sim.player();
        Self {
            sim,
            input,
            previous,
            stage: Stage::new(),
        }
    }

    /// Draws `meshes` as the arena and one dummy at each of `dummy_spawns`.
    #[must_use]
    pub fn with_arena(mut self, meshes: Vec<WorldMesh>, dummy_spawns: &[Vec3]) -> Self {
        self.stage.add_arena(meshes, dummy_spawns);
        self
    }

    /// Plays the sound file at `path` for every shot fired.
    #[must_use]
    pub fn with_shot_sound(mut self, path: &Path) -> Self {
        self.stage.shot_sound = Some(path.to_owned());
        self
    }

    pub fn sim(&self) -> &FpsSim {
        &self.sim
    }

    /// The input map, for feeding it device events.
    pub fn input_mut(&mut self) -> &mut ActionMap<FpsAction> {
        &mut self.input
    }
}

impl Game for FpsGame {
    type Command = FpsCommand;

    fn command(&mut self) -> FpsCommand {
        FpsCommand::from_frame(&self.input.end_tick())
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>, command: &FpsCommand) {
        self.previous = self.sim.player();
        self.sim.tick(command, ctx.tick_length.as_secs_f32());
        self.stage.tick_flashes();
    }

    fn init(&mut self, ctx: &mut InitContext<'_>) {
        capture_cursor(ctx.window);
        let crosshair: UiSource = self.stage.crosshair.clone();
        ctx.renderer.set_ui_source(Some(crosshair));
        self.stage.window = Some(ctx.window.clone());
    }

    fn event(&mut self, ctx: &mut EventContext<'_>, event: Event) {
        match event {
            Event::Window(event) => {
                if let WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key: PhysicalKey::Code(KeyCode::Escape),
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } = event
                {
                    ctx.request_exit();
                }
                handle_window_event(&mut self.input, &event);
            }
            Event::Device(event) => handle_device_event(&mut self.input, &event),
        }
    }

    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32) {
        for feedback in self.sim.drain_feedback() {
            self.stage.react(&feedback, ctx.audio());
        }
        self.stage.refresh_title();
        if let Some(renderer) = ctx.renderer() {
            let view = first_person_view(
                &self.previous,
                &self.sim.player(),
                self.sim.eye_height(),
                alpha,
            );
            self.stage.draw(renderer, (view.view_matrix(), view.eye));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use glam::Vec3;
    use moho_app::{HeadlessLoop, LoopConfig};
    use moho_input::bindings::ActionBindings;

    use super::*;
    use crate::sim::Feedback;
    use crate::test_support::{flat_floor, mag, press, test_config};
    use searchlight_rules::Rifle;

    fn game() -> FpsGame {
        let sim = FpsSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(Rifle::loaded(mag(30, 5)), vec![]),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        FpsGame::new(sim, ActionMap::new(bindings, 1.0))
    }

    #[test]
    fn a_headless_tick_turns_a_trigger_press_into_a_shot() {
        let mut game = game();
        press(game.input_mut(), FpsAction::Fire);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));

        headless.step(&mut game, 1);

        assert_eq!(game.sim().pending_feedback(), [Feedback::Fired]);
        assert_eq!(game.sim().rifle().magazine().rounds(), 3);
    }

    #[test]
    fn a_headless_frame_drains_the_feedback_without_audio_or_window() {
        let mut game = game();
        press(game.input_mut(), FpsAction::Fire);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.step(&mut game, 1);

        headless.advance(&mut game, Duration::ZERO);

        assert!(game.sim().pending_feedback().is_empty());
    }
}

#[cfg(test)]
mod presentation_tests {
    use searchlight_rules::BodyPart;

    use super::*;

    #[test]
    fn box_triangles_wind_counter_clockwise_seen_from_outside() {
        let centre = Vec3::new(1.0, 2.0, 3.0);
        let mesh = box_mesh(centre, Vec3::new(0.5, 1.0, 0.25));

        for (tri, indices) in mesh.indices().chunks(3).enumerate() {
            let at = |i: u32| Vec3::from(mesh.positions()[i as usize]);
            let (a, b, c) = (at(indices[0]), at(indices[1]), at(indices[2]));
            let facing = (b - a).cross(c - a);
            let outward = (a + b + c) / 3.0 - centre;
            let stated = Vec3::from(mesh.normals()[indices[0] as usize]);

            assert!(facing.dot(outward) > 0.0, "triangle {tri} faces inward");
            assert!(facing.normalize().dot(stated) > 0.999, "triangle {tri}");
        }
    }

    #[test]
    fn the_crosshair_sends_its_texture_once_and_centres_four_bars() {
        let mut crosshair = Crosshair::default();

        let first = crosshair.ui_frame([800, 600]).expect("a frame");
        let second = crosshair.ui_frame([800, 600]).expect("a frame");

        assert_eq!(first.textures_set.len(), 1);
        assert!(second.textures_set.is_empty());
        let mesh = &first.meshes[0];
        assert_eq!(mesh.vertices.len(), 16);
        assert_eq!(mesh.indices.len(), 24);
        assert!(mesh.indices.iter().all(|&i| (i as usize) < 16));
        let xs: Vec<f32> = mesh.vertices.iter().map(|v| v.pos[0]).collect();
        let ys: Vec<f32> = mesh.vertices.iter().map(|v| v.pos[1]).collect();
        let centre = |v: &[f32]| {
            (v.iter().copied().fold(f32::MAX, f32::min)
                + v.iter().copied().fold(f32::MIN, f32::max))
                / 2.0
        };
        assert_eq!((centre(&xs), centre(&ys)), (400.0, 300.0));
    }

    #[test]
    fn a_hit_lights_only_the_struck_segment_until_the_flash_runs_out() {
        let mut stage = Stage::new();
        stage.add_arena(Vec::new(), &[Vec3::ZERO]);
        let hit = Feedback::Hit {
            target: 0,
            part: BodyPart::Head,
            report: searchlight_rules::Body::new(searchlight_rules::BodyProfile {
                max_health: 10,
                head_status: 10,
                arm_status: 10,
                leg_status: 10,
            })
            .hit(BodyPart::Head, 1),
        };

        stage.react(&hit, None);
        let lit: Vec<bool> = stage.segments.iter().map(|s| s.flash_left > 0).collect();
        for _ in 0..FLASH_TICKS {
            stage.tick_flashes();
        }

        assert_eq!(lit, [true, false, false, false, false, false]);
        assert!(stage.segments.iter().all(|s| s.flash_left == 0));
        assert_eq!(stage.status, "hit the head for 1");
    }
}

#[cfg(test)]
mod mutation_tests {
    use searchlight_rules::{Body, BodyPart, BodyProfile};
    use moho_renderer::FrameError;

    use crate::sim::STAND_HEIGHT;

    use super::*;

    /// Records what the stage hands the backend; mesh handles count up from 1.
    #[derive(Default)]
    struct FakeRenderer {
        next_handle: u32,
        lighting: Option<LightingGpu>,
        materials: Vec<MaterialGpu>,
        camera: Option<(Mat4, Mat4, Vec3)>,
        draws: Vec<(u32, InstanceGpu)>,
    }

    impl FakeRenderer {
        /// The material each world draw uses, in draw order.
        fn drawn_materials(&self) -> Vec<MaterialGpu> {
            self.draws
                .iter()
                .map(|(_, instance)| self.materials[instance.material as usize])
                .collect()
        }
    }

    impl RendererBackend for FakeRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("world meshes upload indexed meshes")
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
            self.next_handle += 1;
            self.next_handle
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(&mut self, camera: (Mat4, Mat4, Vec3)) -> Result<(), FrameError> {
            self.camera = Some(camera);
            self.draws.clear();
            Ok(())
        }
        fn enqueue_draw(&mut self, mesh: u32, instances: &[InstanceGpu]) {
            self.draws
                .extend(instances.iter().map(|instance| (mesh, *instance)));
        }
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, materials: &[MaterialGpu]) {
            self.materials = materials.to_vec();
        }
        fn update_lighting(&mut self, lighting: LightingGpu) {
            self.lighting = Some(lighting);
        }
        fn add_point_light(&mut self, _p: Vec3, _c: Vec3, _intensity: f32, _range: f32) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, _quality: u8) {}
        fn set_ssao_quality(&mut self, _quality: u8) {}
    }

    fn head_hit() -> Feedback {
        let report = Body::new(BodyProfile {
            max_health: 10,
            head_status: 10,
            arm_status: 10,
            leg_status: 10,
        })
        .hit(BodyPart::Head, 1);
        Feedback::Hit {
            target: 0,
            part: BodyPart::Head,
            report,
        }
    }

    fn assert_material(actual: MaterialGpu, expected: Paint) {
        let expected = expected.to_gpu();
        assert_eq!(
            (actual.albedo, actual.params),
            (expected.albedo, expected.params)
        );
    }

    fn draw_once(stage: &mut Stage, renderer: &mut FakeRenderer) {
        stage.draw(renderer, (Mat4::IDENTITY, Vec3::ZERO));
    }

    #[test]
    fn box_mesh_emits_exact_corners_normals_and_indices() {
        let mesh = box_mesh(Vec3::new(1.0, 2.0, 3.0), Vec3::new(0.5, 1.0, 0.25));

        let expected_positions: [[f32; 3]; 24] = [
            [1.5, 1.0, 2.75],
            [1.5, 3.0, 2.75],
            [1.5, 3.0, 3.25],
            [1.5, 1.0, 3.25],
            [0.5, 1.0, 2.75],
            [0.5, 1.0, 3.25],
            [0.5, 3.0, 3.25],
            [0.5, 3.0, 2.75],
            [0.5, 3.0, 2.75],
            [0.5, 3.0, 3.25],
            [1.5, 3.0, 3.25],
            [1.5, 3.0, 2.75],
            [0.5, 1.0, 2.75],
            [1.5, 1.0, 2.75],
            [1.5, 1.0, 3.25],
            [0.5, 1.0, 3.25],
            [0.5, 1.0, 3.25],
            [1.5, 1.0, 3.25],
            [1.5, 3.0, 3.25],
            [0.5, 3.0, 3.25],
            [0.5, 1.0, 2.75],
            [0.5, 3.0, 2.75],
            [1.5, 3.0, 2.75],
            [1.5, 1.0, 2.75],
        ];
        let face_normals = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ];
        let expected_normals: Vec<[f32; 3]> = face_normals
            .iter()
            .flat_map(|n| [n.to_array(); 4])
            .collect();
        let expected_indices: Vec<u32> = (0..6)
            .flat_map(|face| {
                let f = face * 4;
                [f, f + 1, f + 2, f, f + 2, f + 3]
            })
            .collect();
        assert_eq!(mesh.positions(), expected_positions);
        assert_eq!(mesh.normals(), expected_normals);
        assert_eq!(mesh.indices(), expected_indices);
    }

    #[test]
    fn dummy_segments_sit_at_the_standing_centre_plus_each_volume_offset() {
        let spawns = [Vec3::new(9.0, 0.0, 9.0), Vec3::new(3.0, 0.5, -2.0)];

        let segments = dummy_segments(&spawns);

        assert_eq!(segments.len(), 12);
        let centre = Vec3::new(3.0, 0.5 + STAND_HEIGHT, -2.0);
        for (i, segment) in segments[6..].iter().enumerate() {
            let volume = dummy_hit_volumes(1)[i];
            let positions: Vec<Vec3> = segment
                .mesh
                .positions()
                .iter()
                .map(|&p| Vec3::from(p))
                .collect();
            let min = positions.iter().fold(Vec3::MAX, |a, &p| a.min(p));
            let max = positions.iter().fold(Vec3::MIN, |a, &p| a.max(p));
            let want = centre + volume.offset;
            assert!(
                ((min + max) / 2.0).abs_diff_eq(want, 1e-5),
                "segment {i} centre"
            );
            assert!(
                ((max - min) / 2.0).abs_diff_eq(volume.half_extents, 1e-5),
                "segment {i} size"
            );
            assert_eq!(segment.id, WorldMeshId((1u64 << 32) + 6 + i as u64));
            assert_eq!(segment.tag, SurfaceTag::try_from(volume.tag).unwrap());
        }
    }

    fn expected_bar_vertices(min: [f32; 2], max: [f32; 2]) -> [[f32; 2]; 4] {
        [min, [max[0], min[1]], max, [min[0], max[1]]]
    }

    #[test]
    fn crosshair_bars_leave_a_gap_around_the_centre_of_an_800_by_600_target() {
        let mut crosshair = Crosshair::default();

        let frame = crosshair.ui_frame([800, 600]).expect("a frame");

        let mesh = &frame.meshes[0];
        let positions: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.pos).collect();
        let expected: Vec<[f32; 2]> = [
            ([387.0, 299.0], [396.0, 301.0]),
            ([404.0, 299.0], [413.0, 301.0]),
            ([399.0, 287.0], [401.0, 296.0]),
            ([399.0, 304.0], [401.0, 313.0]),
        ]
        .into_iter()
        .flat_map(|(min, max)| expected_bar_vertices(min, max))
        .collect();
        assert_eq!(positions, expected);
        let expected_indices: Vec<u32> = (0..4)
            .flat_map(|bar| {
                let f = bar * 4;
                [f, f + 1, f + 2, f, f + 2, f + 3]
            })
            .collect();
        assert_eq!(mesh.indices, expected_indices);
        assert_eq!(frame.pixels_per_point, 1.0);
        assert_eq!(mesh.clip_rect.min, [0.0, 0.0]);
        assert_eq!(mesh.clip_rect.max, [800.0, 600.0]);
        assert!(mesh.vertices.iter().all(|v| v.color == [255; 4]));
    }

    #[test]
    fn crosshair_centres_on_each_axis_separately_for_a_tall_odd_target() {
        let mut crosshair = Crosshair::default();

        let frame = crosshair.ui_frame([101, 333]).expect("a frame");

        let positions: Vec<[f32; 2]> = frame.meshes[0].vertices.iter().map(|v| v.pos).collect();
        let (cx, cy) = (50.5, 166.5);
        let expected: Vec<[f32; 2]> = [
            ([cx - 13.0, cy - 1.0], [cx - 4.0, cy + 1.0]),
            ([cx + 4.0, cy - 1.0], [cx + 13.0, cy + 1.0]),
            ([cx - 1.0, cy - 13.0], [cx + 1.0, cy - 4.0]),
            ([cx - 1.0, cy + 4.0], [cx + 1.0, cy + 13.0]),
        ]
        .into_iter()
        .flat_map(|(min, max)| expected_bar_vertices(min, max))
        .collect();
        assert_eq!(positions, expected);
    }

    #[test]
    fn lighting_is_a_normalised_sun_overhead_and_a_moon_below_with_fixed_ambient() {
        let light = lighting();

        let sun = Vec3::new(0.4, 0.8, 0.45).normalize();
        assert_eq!(light.sun_direction, [sun.x, sun.y, sun.z, 1.0]);
        assert_eq!(light.moon_direction, [0.0, -1.0, 0.0, 0.0]);
        assert_eq!(light.ambient, [0.4, 0.5, 0.6, 0.15]);
        assert_eq!(light.params, [12.0, 0.0, 0.0, 0.0]);
        assert_eq!(light.sun_color, LightingGpu::default().sun_color);
        assert_eq!(light.moon_color, LightingGpu::default().moon_color);
    }

    #[test]
    fn drawing_without_a_window_sends_lighting_and_a_widescreen_projection() {
        let mut stage = Stage::new();
        let mut renderer = FakeRenderer::default();
        let eye = Vec3::new(1.0, 2.0, 3.0);

        stage.draw(&mut renderer, (Mat4::IDENTITY, eye));

        let sent = renderer.lighting.expect("lighting is sent every draw");
        assert_eq!(sent.ambient, lighting().ambient);
        let (view, projection, camera_eye) = renderer.camera.expect("a frame began");
        assert_eq!(view, Mat4::IDENTITY);
        assert_eq!(camera_eye, eye);
        let aspect = projection.y_axis.y / projection.x_axis.x;
        assert!((aspect - 16.0 / 9.0).abs() < 1e-4, "aspect {aspect}");
    }

    #[test]
    fn dummy_segments_do_not_share_mesh_ids_with_arena_meshes() {
        let mut stage = Stage::new();
        let arena = box_mesh(Vec3::ZERO, Vec3::ONE);
        stage.add_arena(vec![arena], &[Vec3::new(5.0, 0.0, 5.0)]);
        let mut renderer = FakeRenderer::default();

        draw_once(&mut stage, &mut renderer);

        assert_eq!(renderer.draws.len(), 7);
        assert_material(renderer.drawn_materials()[0], Paint::Arena);
        assert!(
            renderer.drawn_materials()[1..]
                .iter()
                .all(|m| m.params == Paint::Dummy.to_gpu().params)
        );
    }

    #[test]
    fn a_struck_segment_is_drawn_lit_for_exactly_the_flash_duration() {
        let mut stage = Stage::new();
        stage.add_arena(Vec::new(), &[Vec3::ZERO]);
        let mut renderer = FakeRenderer::default();
        stage.react(&head_hit(), None);

        draw_once(&mut stage, &mut renderer);
        let at_hit = renderer.drawn_materials();
        for _ in 0..9 {
            stage.tick_flashes();
        }
        draw_once(&mut stage, &mut renderer);
        let one_tick_left = renderer.drawn_materials();
        stage.tick_flashes();
        draw_once(&mut stage, &mut renderer);
        let expired = renderer.drawn_materials();

        assert_material(at_hit[0], Paint::Struck);
        assert_material(one_tick_left[0], Paint::Struck);
        assert_material(expired[0], Paint::Dummy);
        for materials in [&at_hit, &one_tick_left, &expired] {
            assert_eq!(materials.len(), 6);
            for &other in &materials[1..] {
                assert_material(other, Paint::Dummy);
            }
        }
    }

    #[test]
    fn a_hit_keeps_a_segment_lit_for_ten_ticks_at_the_sixty_hertz_tick() {
        let mut stage = Stage::new();
        stage.add_arena(Vec::new(), &[Vec3::ZERO]);

        stage.react(&head_hit(), None);

        assert_eq!(stage.segments[0].flash_left, 10);
        assert_eq!(FLASH_TICKS, 10);
    }
}

#[cfg(test)]
mod title_and_aspect_tests {
    use super::*;

    const EPSILON: f32 = 1e-6;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn aspect_ratio_of_full_hd_is_sixteen_ninths() {
        assert_close(aspect_ratio(1920, 1080), 16.0 / 9.0);
    }

    #[test]
    fn aspect_ratio_of_800_by_600_is_four_thirds() {
        assert_close(aspect_ratio(800, 600), 4.0 / 3.0);
    }

    #[test]
    fn aspect_ratio_of_a_tall_non_square_window_is_below_one() {
        assert_close(aspect_ratio(101, 333), 101.0 / 333.0);
    }

    #[test]
    fn aspect_ratio_with_zero_width_is_zero() {
        assert_close(aspect_ratio(0, 480), 0.0);
    }

    #[test]
    fn aspect_ratio_with_zero_height_is_finite_and_equals_the_width() {
        let ratio = aspect_ratio(7, 0);

        assert!(ratio.is_finite());
        assert_close(ratio, 7.0);
    }

    #[test]
    fn aspect_ratio_with_height_one_equals_the_width() {
        assert_close(aspect_ratio(9, 1), 9.0);
    }

    #[test]
    fn window_title_is_searchlight() {
        assert_eq!(WINDOW_TITLE, "Searchlight");
    }

    #[test]
    fn window_title_with_empty_status_is_just_the_base_title() {
        assert_eq!(window_title(""), "Searchlight");
        assert_eq!(window_title(""), WINDOW_TITLE);
    }

    #[test]
    fn window_title_with_a_status_appends_it_after_a_separator() {
        let title = window_title("Ammo 12/30");

        assert_eq!(title, "Searchlight | Ammo 12/30");
        assert!(title.starts_with(WINDOW_TITLE));
    }

    #[test]
    fn stage_without_a_window_reports_the_default_sixteen_ninths_aspect() {
        let stage = Stage::new();

        assert_close(stage.aspect(), 16.0 / 9.0);
    }
}
