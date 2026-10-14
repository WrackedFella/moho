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

use crate::hud;
use crate::input::{FpsAction, FpsCommand};
use crate::sim::{Feedback, FpsSim, STAND_HEIGHT, dummy_hit_volumes};
use crate::tag::SurfaceTag;

/// Ticks per second the prototype runs at.
pub const TICK_HZ: u32 = 60;

/// Ticks a struck dummy segment stays lit.
const FLASH_TICKS: u32 = TICK_HZ / 6;
/// Dummy segment mesh ids start here, clear of the arena's (which count up from 0).
const DUMMY_MESH_BASE: u64 = 1 << 32;
/// A mesh handle the renderer never issues, so the empty actor draws are skipped.
const NO_MESH: u32 = u32::MAX;
const WINDOW_TITLE: &str = "moho_fps";
const FOV_DEGREES: f32 = 75.0;
const NEAR: f32 = 0.05;
const FAR: f32 = 500.0;
const SHOT_VOLUME: f32 = 0.8;

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
    shown_struck: bool,
}

/// The dummies' segments, in the frame the world draws them in.
fn dummy_segments(spawns: &[Vec3]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (index, &spawn) in spawns.iter().enumerate() {
        let target = u16::try_from(index).expect("an arena has fewer than 65536 dummies");
        let centre = spawn + Vec3::Y * STAND_HEIGHT;
        for volume in dummy_hit_volumes(target) {
            segments.push(Segment {
                tag: SurfaceTag::try_from(volume.tag).expect("dummy volumes carry part tags"),
                id: WorldMeshId(DUMMY_MESH_BASE + segments.len() as u64),
                mesh: box_mesh(centre + volume.offset, volume.half_extents),
                flash_left: 0,
                shown_struck: false,
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
        sun_color: [1.0, 0.95, 0.8, 0.0],
        moon_direction: [0.0, -1.0, 0.0, 0.0],
        moon_color: [0.7, 0.8, 0.9, 0.0],
        ambient: [0.4, 0.5, 0.6, 0.15],
        params: [12.0, 0.0, 0.0, 0.0],
    }
}

/// The status line a piece of feedback puts in the window title, if it changes it.
fn status_text(feedback: &Feedback) -> Option<String> {
    match feedback {
        Feedback::Fired => None,
        Feedback::Dry => Some("click, the rifle is empty".to_owned()),
        Feedback::Hit { part, report, .. } => Some(format!(
            "hit the {} for {}",
            hud::part_text(*part),
            report.damage
        )),
        Feedback::MagazineLevel(level) => {
            Some(format!("magazine is {}", hud::magazine_level_text(*level)))
        }
        Feedback::ReloadDone => Some("reloaded".to_owned()),
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

    /// Reacts to one piece of feedback: sound, status line and flash.
    fn react(&mut self, feedback: &Feedback, audio: Option<&mut AudioSystem>) {
        if let Some(text) = status_text(feedback) {
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
            segment.flash_left = segment.flash_left.saturating_sub(1);
        }
    }

    fn refresh_title(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let title = if self.status.is_empty() {
            WINDOW_TITLE.to_owned()
        } else {
            format!("{WINDOW_TITLE} | {}", self.status)
        };
        if title != self.title {
            window.set_title(&title);
            self.title = title;
        }
    }

    /// Re-queues the segments whose lit state changed since they were last queued.
    fn queue_flashes(&mut self) {
        for segment in &mut self.segments {
            let struck = segment.flash_left > 0;
            if struck != segment.shown_struck {
                segment.shown_struck = struck;
                let material = if struck {
                    self.struck_material
                } else {
                    self.dummy_material
                };
                self.scene
                    .world_meshes_mut()
                    .upsert(segment.id, segment.mesh.clone(), material);
            }
        }
    }

    fn aspect(&self) -> f32 {
        self.window.as_ref().map_or(16.0 / 9.0, |window| {
            let size = window.inner_size();
            size.width as f32 / size.height.max(1) as f32
        })
    }

    fn draw(&mut self, renderer: &mut dyn RendererBackend, eye: (Mat4, Vec3)) {
        self.queue_flashes();
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
    use moho_fps_rules::Rifle;

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
    use moho_fps_rules::BodyPart;

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
            report: moho_fps_rules::Body::new(moho_fps_rules::BodyProfile {
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
