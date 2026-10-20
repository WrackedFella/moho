//! The prototype as a [`moho_app::Game`]: samples input, ticks the sim, and turns its
//! feedback into sound, a title status line, a hit flash and a crosshair.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use glam::{Mat4, Quat, Vec3};
use moho_app::{Event, EventContext, FrameContext, Game, InitContext, TickContext};
use moho_audio::AudioSystem;
use moho_input::action_map::{ActionMap, handle_device_event, handle_window_event};
use moho_input::bindings::Binding;
use moho_input::key::Key;
use moho_physics::{CharacterState, first_person_view};
use moho_render_api::{
    InstanceGpu, MaterialGpu, MaterialKey, RenderMaterial, Renderable, UiFrame, UiFrameSource,
    UiImage, UiMesh, UiRect, UiSampler, UiSource, UiTextureId, UiTextureSet, UiVertex, WorldMesh,
    WorldMeshId,
};
use moho_renderer::{LightingGpu, RendererBackend, Scene};
use winit::event::WindowEvent;
use winit::window::{CursorGrabMode, Window};

use moho_ui_shell::host::input_of;
use moho_ui_shell::{Console, Routing, UiLayers, UiShell, apply_engine_command};

use crate::hud::Edge;
use crate::input::{SearchlightAction, SearchlightCommand, look_to_turn};
use crate::sim::{Feedback, SearchlightSim, dummy_at, dummy_hit_volumes};
use crate::tag::SurfaceTag;
use crate::{TICK_HZ, hud};

/// Ticks a struck dummy segment stays lit.
const FLASH_TICKS: u32 = TICK_HZ / 6;
/// Dummy segment mesh ids start here, clear of the arena's (which count up from 0).
const DUMMY_MESH_BASE: u64 = 1 << 32;
/// Enemy segment mesh ids start here, clear of the arena's and the dummies'.
const ENEMY_MESH_BASE: u64 = 2 << 32;
/// Mesh ids reserved per enemy: six body parts and the gun.
const ENEMY_MESH_SLOTS: u64 = 8;
/// Ticks a damage edge bar takes to fade out.
const EDGE_TICKS: u32 = TICK_HZ / 4;
/// Width of a damage edge bar, in pixels.
const EDGE_THICKNESS: f32 = 28.0;
/// Opacity of a damage edge bar at the moment of the hit.
const EDGE_ALPHA: u32 = 190;
const EDGE_RGB: [u8; 3] = [200, 20, 20];
/// The enemy's gun in its body frame: a long box in the right hand, pointing forward.
const GUN_OFFSET: Vec3 = Vec3::new(-0.3, 0.25, 0.4);
const GUN_HALF: Vec3 = Vec3::new(0.04, 0.04, 0.3);
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
    Enemy,
    Struck,
}

impl RenderMaterial for Paint {
    fn to_gpu(&self) -> MaterialGpu {
        // World meshes draw as terrain: `albedo` colours up-facing surfaces, `params.xyz`
        // the rest, and `params.w > 0` makes the surface glow its colour.
        let (top, side, glow) = match self {
            Paint::Arena => ([0.62, 0.62, 0.64], [0.42, 0.43, 0.46], 0.0),
            Paint::Dummy => ([0.85, 0.6, 0.2], [0.7, 0.48, 0.16], 0.0),
            Paint::Enemy => ([0.3, 0.4, 0.62], [0.22, 0.3, 0.5], 0.0),
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
    oriented_box_mesh(centre, half, Quat::IDENTITY)
}

/// [`box_mesh`] with its axes turned by `rotation` about its centre.
fn oriented_box_mesh(centre: Vec3, half: Vec3, rotation: Quat) -> WorldMesh {
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
        let face_centre = centre + rotation * (normal * half);
        for corner in [-du - dv, du - dv, du + dv, -du + dv] {
            positions.push((face_centre + rotation * corner).to_array());
            normals.push((rotation * normal).to_array());
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

/// One enemy as box segments: the six body parts and the gun, which flashes when it fires.
#[derive(Debug)]
struct EnemyRig {
    target: u16,
    /// Whether the segments are in the scene.
    shown: bool,
    gun_flash: u32,
}

impl EnemyRig {
    fn mesh_id(&self, slot: usize) -> WorldMeshId {
        WorldMeshId(ENEMY_MESH_BASE + u64::from(self.target) * ENEMY_MESH_SLOTS + slot as u64)
    }
}

/// The enemy's segments as `(offset, half extents)` in its body frame, gun last.
fn enemy_segments() -> [(Vec3, Vec3); 7] {
    let volumes = dummy_hit_volumes(0);
    let mut segments = [(GUN_OFFSET, GUN_HALF); 7];
    for (segment, volume) in segments.iter_mut().zip(volumes) {
        *segment = (volume.offset, volume.half_extents);
    }
    segments
}

fn push_quad(
    vertices: &mut Vec<UiVertex>,
    indices: &mut Vec<u32>,
    min: [f32; 2],
    max: [f32; 2],
    color: [u8; 4],
) {
    let first = vertices.len() as u32;
    for pos in [min, [max[0], min[1]], max, [min[0], max[1]]] {
        vertices.push(UiVertex {
            pos,
            uv: [0.5, 0.5],
            color,
        });
    }
    indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
}

/// Four short bars around the screen centre, drawn as solid rects from a 1x1 white texture,
/// under the damage edge bars and the death tint when they are showing.
#[derive(Debug, Default)]
struct Crosshair {
    texture_sent: bool,
    dead: bool,
    /// Opacity of the damage bar on each screen edge, indexed front, right, behind, left.
    edges: [u8; 4],
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
        let thick = EDGE_THICKNESS;
        let edge_rects = [
            ([0.0, 0.0], [size[0], thick]),
            ([size[0] - thick, 0.0], size),
            ([0.0, size[1] - thick], size),
            ([0.0, 0.0], [thick, size[1]]),
        ];
        for (alpha, (min, max)) in self.edges.into_iter().zip(edge_rects) {
            if alpha > 0 {
                let [r, g, b] = EDGE_RGB;
                push_quad(&mut vertices, &mut indices, min, max, [r, g, b, alpha]);
            }
        }
        if let Some(tint) = hud::death_overlay(self.dead, size_in_pixels) {
            push_quad(&mut vertices, &mut indices, tint.min, tint.max, tint.rgba);
        }
        for (min, max) in bars {
            push_quad(&mut vertices, &mut indices, min, max, [255; 4]);
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
    enemy_material: u32,
    dummy_count: usize,
    enemies: Vec<EnemyRig>,
    edge_left: [u32; 4],
    crosshair: Arc<Mutex<Crosshair>>,
    shot_sound: Option<PathBuf>,
    window: Option<Arc<Window>>,
    status: String,
    title: String,
    shell: Arc<Mutex<UiShell<Infallible>>>,
}

impl Stage {
    fn new() -> Self {
        let mut scene = Scene::new();
        let dummy_material = scene.material_table.find_or_push(&Paint::Dummy);
        let struck_material = scene.material_table.find_or_push(&Paint::Struck);
        let enemy_material = scene.material_table.find_or_push(&Paint::Enemy);
        Self {
            scene,
            segments: Vec::new(),
            dummy_material,
            struck_material,
            enemy_material,
            dummy_count: 0,
            enemies: Vec::new(),
            edge_left: [0; 4],
            crosshair: Arc::new(Mutex::new(Crosshair::default())),
            shot_sound: None,
            window: None,
            status: String::new(),
            title: String::new(),
            shell: Arc::new(Mutex::new(UiShell::new(Console::new(&BTreeMap::new()).0))),
        }
    }

    /// The console shell. Never hold the guard across `draw`: the renderer locks the shell
    /// itself when it pulls the UI frame.
    fn shell(&self) -> MutexGuard<'_, UiShell<Infallible>> {
        self.shell.lock().unwrap_or_else(PoisonError::into_inner)
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
        self.dummy_count = dummy_spawns.len();
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
            Feedback::EnemyFired { target } => {
                self.enemy_rig(*target).gun_flash = FLASH_TICKS;
                self.play_shot(audio);
            }
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

    fn enemy_rig(&mut self, target: u16) -> &mut EnemyRig {
        let index = self
            .enemies
            .iter()
            .position(|rig| rig.target == target)
            .unwrap_or_else(|| {
                self.enemies.push(EnemyRig {
                    target,
                    shown: false,
                    gun_flash: 0,
                });
                self.enemies.len() - 1
            });
        &mut self.enemies[index]
    }

    /// Lights the damage bar on `edge`; it fades over [`EDGE_TICKS`].
    fn show_hit_edge(&mut self, edge: Edge) {
        self.edge_left[edge as usize] = EDGE_TICKS;
    }

    /// Puts every living enemy's segments where the sim has it, and takes the dead ones out.
    fn sync_enemies(&mut self, sim: &SearchlightSim) {
        for index in 0..sim.enemy_count() {
            let target = u16::try_from(self.dummy_count + index)
                .expect("the sim numbers its targets with a u16");
            let rig = self.enemy_rig(target);
            let state = sim.enemy_state(index);
            let (shown, gun_flash) = (rig.shown, rig.gun_flash);
            let ids: [WorldMeshId; 7] = std::array::from_fn(|slot| rig.mesh_id(slot));
            rig.shown = state.is_some();
            let meshes = self.scene.world_meshes_mut();
            let Some(state) = state else {
                if shown {
                    for id in ids {
                        meshes.remove(id);
                    }
                }
                continue;
            };
            let facing = Quat::from_rotation_y(state.yaw);
            for (slot, (offset, half)) in enemy_segments().into_iter().enumerate() {
                let material = if slot == 6 && gun_flash > 0 {
                    self.struck_material
                } else {
                    self.enemy_material
                };
                meshes.upsert(
                    ids[slot],
                    oriented_box_mesh(state.position + facing * offset, half, facing),
                    material,
                );
            }
        }
    }

    /// Pushes the death tint and damage bars to the UI.
    fn refresh_hud(&self, dead: bool) {
        let mut hud = self
            .crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        hud.dead = dead;
        hud.edges = self
            .edge_left
            .map(|left| (EDGE_ALPHA * left / EDGE_TICKS) as u8);
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
        for left in &mut self.edge_left {
            *left = left.saturating_sub(1);
        }
        for rig in &mut self.enemies {
            rig.gun_flash = rig.gun_flash.saturating_sub(1);
        }
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

    /// Sets the window title to `vitals` and the latest status line.
    fn refresh_title(&mut self, vitals: &str) {
        let Some(window) = &self.window else {
            return;
        };
        let title = if self.status.is_empty() {
            window_title(vitals)
        } else {
            window_title(&format!("{vitals} | {}", self.status))
        };
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
pub struct SearchlightGame {
    sim: SearchlightSim,
    input: ActionMap<SearchlightAction>,
    /// The player as the tick before the last left them, for blending the view.
    previous: CharacterState,
    /// Whether the window has focus; look is sampled only while it does.
    focused: bool,
    stage: Stage,
}

impl SearchlightGame {
    /// A game ticking `sim` with commands sampled from `input`.
    pub fn new(sim: SearchlightSim, input: ActionMap<SearchlightAction>) -> Self {
        let previous = sim.player();
        Self {
            sim,
            input,
            previous,
            focused: true,
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

    pub fn sim(&self) -> &SearchlightSim {
        &self.sim
    }

    /// The input map, for feeding it device events.
    pub fn input_mut(&mut self) -> &mut ActionMap<SearchlightAction> {
        &mut self.input
    }

    /// Replaces the default console shell, which has no game commands.
    #[must_use]
    pub fn with_shell(mut self, shell: UiShell<Infallible>) -> Self {
        self.stage.shell = Arc::new(Mutex::new(shell));
        self
    }

    /// Where one key or button press or release goes: the console sees it first, so a game
    /// action bound to the console key never shadows it. The window-event path calls this once
    /// it has a `Binding`, so it is testable without a winit `KeyEvent`.
    fn route_input(
        &mut self,
        ctx: &mut EventContext<'_>,
        input: Binding,
        pressed: bool,
        repeat: bool,
    ) -> Routing {
        let routing = self.stage.shell().route(input, pressed, repeat);
        match routing {
            // A key held as the console opened would otherwise stay held: its release goes to
            // the console.
            Routing::Opened => self.input.release_all(),
            Routing::Console | Routing::Closed => {}
            Routing::Game => {
                if pressed && !repeat && input == Binding::Key(Key::Escape) {
                    ctx.request_exit();
                }
                match input {
                    Binding::Key(key) => self.input.key(key, pressed),
                    Binding::Mouse(button) => self.input.mouse_button(button, pressed),
                    Binding::Pad(_) => {}
                }
            }
        }
        routing
    }
}

impl Game for SearchlightGame {
    type Command = SearchlightCommand;

    fn command(&mut self) -> SearchlightCommand {
        // The paused loop asks every frame; ending the tick would consume the look the
        // frames showed and snap the view back.
        if self.stage.shell().captures_input() {
            return SearchlightCommand::default();
        }
        SearchlightCommand::from_frame(&self.input.end_tick())
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>, command: &SearchlightCommand) {
        self.previous = self.sim.player();
        let was_dead = self.sim.player_body().is_dead();
        self.sim.tick(command, ctx.tick_length.as_secs_f32());
        if was_dead && !self.sim.player_body().is_dead() {
            self.previous = self.sim.player();
            self.stage.status.clear();
        }
        self.stage.tick_flashes();
    }

    fn init(&mut self, ctx: &mut InitContext<'_>) {
        capture_cursor(ctx.window);
        self.stage.shell().attach(ctx.window.clone());
        let crosshair: UiSource = self.stage.crosshair.clone();
        let shell: UiSource = self.stage.shell.clone();
        let layers: UiSource = Arc::new(Mutex::new(UiLayers::new(vec![crosshair, shell])));
        ctx.renderer.set_ui_source(Some(layers));
        self.stage.window = Some(ctx.window.clone());
    }

    fn event(&mut self, ctx: &mut EventContext<'_>, event: Event) {
        match event {
            Event::Window(event) => {
                if let WindowEvent::Focused(focused) = event {
                    self.focused = focused;
                }
                if let Some((input, pressed, repeat)) = input_of(&event) {
                    let routing = self.route_input(ctx, input, pressed, repeat);
                    self.stage.shell().feed(&event, routing);
                } else if self.stage.shell().on_window_event(&event) == Routing::Game {
                    handle_window_event(&mut self.input, &event);
                }
            }
            Event::Device(event) => {
                if self.focused && !self.stage.shell().captures_input() {
                    handle_device_event(&mut self.input, &event);
                }
            }
        }
        ctx.set_paused(self.stage.shell().captures_input());
    }

    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32) {
        let captured = self.stage.shell().captures_input();
        ctx.set_paused(captured);
        if self.focused && !captured {
            self.input.sample_frame(ctx.frame_dt);
        }
        let engine_commands = self.stage.shell().console_mut().take_engine_commands();
        for command in &engine_commands {
            apply_engine_command(command, ctx);
        }
        for feedback in self.sim.drain_feedback() {
            if let Feedback::PlayerHit { from, .. } = feedback {
                let player = self.sim.player();
                self.stage
                    .show_hit_edge(hud::hit_edge(player.position, player.yaw, from));
            }
            self.stage.react(&feedback, ctx.audio());
        }
        let dead = self.sim.player_body().is_dead();
        let vitals = hud::death_text(dead).map_or_else(
            || {
                hud::health_text(
                    self.sim.player_body().health(),
                    self.sim.player_max_health(),
                )
            },
            str::to_owned,
        );
        self.stage.refresh_title(&vitals);
        self.stage.refresh_hud(dead);
        self.stage.sync_enemies(&self.sim);
        if let Some(renderer) = ctx.renderer() {
            let view = first_person_view(
                &self.previous,
                &self.sim.player(),
                self.sim.eye_height(),
                alpha,
                look_to_turn(self.input.pending_look()),
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
    use moho_app::{GameClock, HeadlessLoop, LoopConfig};
    use moho_input::bindings::ActionBindings;
    use winit::event::{DeviceEvent, WindowEvent};

    use super::*;
    use crate::sim::Feedback;
    use crate::test_support::{arena_with, flat_floor, mag, press, test_config};
    use searchlight_rules::Rifle;

    fn game() -> SearchlightGame {
        let sim = SearchlightSim::new(
            flat_floor(),
            Vec3::ZERO,
            &[],
            test_config(Rifle::loaded(mag(30, 5)), vec![]),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(sim, ActionMap::new(bindings, 1.0))
    }

    #[test]
    fn a_headless_tick_turns_a_trigger_press_into_a_shot() {
        let mut game = game();
        press(game.input_mut(), SearchlightAction::Fire);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));

        headless.step(&mut game, 1);

        assert_eq!(game.sim().pending_feedback(), [Feedback::Fired]);
        assert_eq!(game.sim().rifle().magazine().rounds(), 3);
    }

    fn arena_config() -> crate::sim::SimConfig {
        let mut config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        config.eye_height = 0.72;
        config.enemy.damage = 100;
        config.enemy.reaction_ticks = 30;
        config
    }

    fn arena_arena() -> Arc<crate::arena::Arena> {
        arena_with(
            &[],
            Vec3::ZERO,
            &[Vec3::new(0.0, 0.0, 10.0)],
            &[Vec3::new(0.0, 0.0, 6.0)],
        )
    }

    fn tick_with(game: &mut SearchlightGame, clock: &mut GameClock, command: &SearchlightCommand) {
        let mut ctx = TickContext {
            tick: 0,
            tick_length: Duration::from_secs_f32(1.0 / 60.0),
            clock,
        };
        game.tick(&mut ctx, command);
    }

    /// A game whose player walked toward an enemy and was shot dead there, with a status shown.
    fn game_with_a_player_dead_away_from_the_spawn() -> (SearchlightGame, GameClock) {
        let sim = SearchlightSim::start(arena_arena(), arena_config());
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(sim, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let walk = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                forward: 1.0,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        for _ in 0..600 {
            tick_with(&mut game, &mut clock, &walk);
            if game.sim().player_body().is_dead() {
                break;
            }
        }
        assert!(game.sim().player_body().is_dead(), "the player never died");
        assert!(
            game.sim().player().position.z > 0.5,
            "the player never left the spawn"
        );
        game.stage.react(&Feedback::Dry, None);
        assert!(!game.stage.status.is_empty());
        (game, clock)
    }

    fn restart_command() -> SearchlightCommand {
        SearchlightCommand {
            restart: true,
            ..SearchlightCommand::default()
        }
    }

    #[test]
    fn the_view_does_not_blend_from_the_death_position_on_the_restart_tick() {
        let (mut game, mut clock) = game_with_a_player_dead_away_from_the_spawn();
        let spawn = SearchlightSim::start(arena_arena(), arena_config()).player();

        tick_with(&mut game, &mut clock, &restart_command());

        assert_eq!(game.sim().player(), spawn);
        assert_eq!(game.previous, spawn);
    }

    #[test]
    fn restarting_clears_the_status_line() {
        let (mut game, mut clock) = game_with_a_player_dead_away_from_the_spawn();

        tick_with(&mut game, &mut clock, &restart_command());

        assert_eq!(game.stage.status, "");
    }

    #[test]
    fn an_ordinary_alive_tick_blends_from_the_previous_pose_and_keeps_the_status() {
        let sim = SearchlightSim::start(arena_arena(), arena_config());
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(sim, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let walk = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                forward: 1.0,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        for _ in 0..5 {
            tick_with(&mut game, &mut clock, &walk);
        }
        assert!(!game.sim().player_body().is_dead());
        game.stage.react(&Feedback::Dry, None);
        assert!(!game.stage.status.is_empty());
        let before = game.sim().player();

        tick_with(&mut game, &mut clock, &walk);

        assert_eq!(game.previous, before);
        assert_ne!(game.previous, game.sim().player());
        assert!(!game.stage.status.is_empty());
    }

    const BACKTICK: Binding = Binding::Key(Key::Backtick);
    const TICK: Duration = Duration::from_millis(16);

    fn open_the_console(game: &mut SearchlightGame, clock: &GameClock) {
        let mut ctx = EventContext::new(clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        assert_eq!(routing, Routing::Opened);
    }

    /// A game whose enemy is walking toward the player, so every sim tick moves it.
    fn game_with_a_moving_enemy() -> SearchlightGame {
        let sim = SearchlightSim::start(arena_arena(), arena_config());
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(sim, ActionMap::new(bindings, 1.0))
    }

    #[test]
    fn an_open_console_freezes_the_simulation() {
        let mut game = game_with_a_moving_enemy();
        let clock = GameClock::default();
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.step(&mut game, 1);
        open_the_console(&mut game, &clock);
        headless.advance(&mut game, Duration::ZERO);
        let (enemy, player, health, next_tick) = (
            game.sim().enemy_state(0),
            game.sim().player(),
            game.sim().player_body().health(),
            headless.next_tick(),
        );

        for _ in 0..120 {
            headless.advance(&mut game, Duration::from_secs(1) / 60);
        }

        assert!(headless.paused());
        assert_eq!(headless.next_tick(), next_tick);
        assert_eq!(game.sim().enemy_state(0), enemy);
        assert_eq!(game.sim().player(), player);
        assert_eq!(game.sim().player_body().health(), health);
    }

    #[test]
    fn opening_the_console_asks_the_loop_to_pause() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        let mut ctx = EventContext::new(&clock, None, None);

        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion { delta: (1.0, 1.0) }),
        );

        assert_eq!(ctx.requested_paused(), Some(true));
    }

    #[test]
    fn closing_the_console_asks_the_loop_to_resume() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        let mut ctx = EventContext::new(&clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        assert_eq!(routing, Routing::Closed);

        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion { delta: (1.0, 1.0) }),
        );

        assert_eq!(ctx.requested_paused(), Some(false));
    }

    #[test]
    fn closing_the_console_resumes_the_simulation() {
        let mut game = game_with_a_moving_enemy();
        let clock = GameClock::default();
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        open_the_console(&mut game, &clock);
        headless.step(&mut game, 1);
        let mut ctx = EventContext::new(&clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        assert_eq!(routing, Routing::Closed);
        let frozen = game.sim().enemy_state(0);

        headless.step(&mut game, 30);

        assert_ne!(game.sim().enemy_state(0), frozen);
    }

    #[test]
    fn quit_typed_in_the_console_exits_on_the_next_frame() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        game.stage
            .shell
            .lock()
            .unwrap()
            .console_mut()
            .submit("quit");
        let mut ctx = FrameContext::new(TICK, &clock, None, None);

        game.frame(&mut ctx, 0.0);

        assert!(ctx.exit_requested());
    }

    /// Records the engine's renderer settings; the draw calls do nothing.
    #[derive(Default)]
    struct SettingsRenderer {
        shadow: Vec<u8>,
        ssao: Vec<u8>,
        debug_view: Vec<Option<u32>>,
        /// The view matrix of every frame begun.
        views: Vec<Mat4>,
    }

    impl RendererBackend for SettingsRenderer {
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
        fn begin_frame(
            &mut self,
            camera: (Mat4, Mat4, Vec3),
        ) -> Result<(), moho_renderer::FrameError> {
            self.views.push(camera.0);
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: LightingGpu) {}
        fn add_point_light(&mut self, _p: Vec3, _c: Vec3, _intensity: f32, _range: f32) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, quality: u8) {
            self.shadow.push(quality);
        }
        fn set_ssao_quality(&mut self, quality: u8) {
            self.ssao.push(quality);
        }
        fn set_debug_view(&mut self, mode: Option<u32>) {
            self.debug_view.push(mode);
        }
    }

    #[test]
    fn a_renderer_command_applies_on_the_next_frame() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        {
            let mut shell = game.stage.shell.lock().unwrap();
            shell.console_mut().submit("r_shadow_quality 0");
            shell.console_mut().submit("r_ssao_quality 4");
            shell.console_mut().submit("r_debug_view 3");
        }
        let mut renderer = SettingsRenderer::default();
        let before = (
            renderer.shadow.clone(),
            renderer.ssao.clone(),
            renderer.debug_view.clone(),
        );
        {
            let backend: &mut dyn RendererBackend = &mut renderer;
            let mut ctx = FrameContext::new(TICK, &clock, Some(backend), None);

            game.frame(&mut ctx, 0.0);
        }

        assert_eq!(before, (vec![], vec![], vec![]));
        assert_eq!(renderer.shadow, vec![0]);
        assert_eq!(renderer.ssao, vec![4]);
        assert_eq!(renderer.debug_view, vec![Some(3)]);
    }

    #[test]
    fn escape_with_the_console_open_does_not_exit() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        let mut ctx = EventContext::new(&clock, None, None);

        let routing = game.route_input(&mut ctx, Binding::Key(Key::Escape), true, false);

        assert_eq!(routing, Routing::Closed);
        assert!(!ctx.exit_requested());
        assert!(!game.stage.shell.lock().unwrap().captures_input());
    }

    #[test]
    fn an_escape_repeat_after_the_console_closed_does_not_exit() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        let mut ctx = EventContext::new(&clock, None, None);
        let escape = Binding::Key(Key::Escape);

        let closing = game.route_input(&mut ctx, escape, true, false);
        let repeated = game.route_input(&mut ctx, escape, true, true);

        assert_eq!(closing, Routing::Closed);
        assert_eq!(repeated, Routing::Game);
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn escape_with_the_console_closed_exits() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        let routing = game.route_input(&mut ctx, Binding::Key(Key::Escape), true, false);

        assert_eq!(routing, Routing::Game);
        assert!(ctx.exit_requested());
    }

    #[test]
    fn an_open_console_stops_movement_and_look() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Binding::Key(Key::W), true, false);

        open_the_console(&mut game, &clock);
        let held = game.command();
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion {
                delta: (40.0, 30.0),
            }),
        );
        let turned = game.command();

        assert_eq!(held.movement.forward, 0.0);
        assert_eq!(turned.movement.yaw_delta, 0.0);
        assert_eq!(turned.movement.pitch_delta, 0.0);
    }

    #[test]
    fn losing_window_focus_releases_a_held_movement_key() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Binding::Key(Key::W), true, false);

        game.event(&mut ctx, Event::Window(WindowEvent::Focused(false)));
        let command = game.command();

        assert_eq!(command.movement.forward, 0.0);
    }

    #[test]
    fn a_held_movement_key_moves_forward_until_focus_is_lost() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Binding::Key(Key::W), true, false);

        let command = game.command();

        assert_eq!(command.movement.forward, 1.0);
    }

    #[test]
    fn a_headless_frame_drains_the_feedback_without_audio_or_window() {
        let mut game = game();
        press(game.input_mut(), SearchlightAction::Fire);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.step(&mut game, 1);

        headless.advance(&mut game, Duration::ZERO);

        assert!(game.sim().pending_feedback().is_empty());
    }

    /// The `alpha` the view tests draw with.
    const ALPHA: f32 = 0.5;

    /// A game with a visible-size sensitivity, the window focused and the given smoothing.
    fn turning_game(smoothing: bool) -> SearchlightGame {
        let mut game = game();
        game.input_mut()
            .set_look(moho_input::action_map::LookSettings {
                look_per_count: 0.002,
                smoothing,
            });
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));
        game
    }

    fn mouse(game: &mut SearchlightGame, dx: f64, dy: f64) {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion { delta: (dx, dy) }),
        );
    }

    /// Runs one frame and returns the view it drew.
    fn draw_frame(game: &mut SearchlightGame, renderer: &mut SettingsRenderer) -> Mat4 {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &clock, Some(renderer), None);
        game.frame(&mut ctx, ALPHA);
        *renderer.views.last().expect("a frame began")
    }

    /// The view a frame must draw: the sim's player turned by the look shown but not yet ticked.
    fn expected_view(game: &SearchlightGame) -> Mat4 {
        let (look_x, look_y) = game.input.pending_look();
        first_person_view(
            &game.previous,
            &game.sim.player(),
            game.sim.eye_height(),
            ALPHA,
            (-look_x, -look_y),
        )
        .view_matrix()
    }

    fn forward_of(view: Mat4) -> Vec3 {
        -view.transpose().col(2).truncate()
    }

    fn tick_once(game: &mut SearchlightGame) {
        let command = game.command();
        let mut clock = GameClock::default();
        tick_with(game, &mut clock, &command);
    }

    #[test]
    fn mouse_motion_turns_the_next_frame_before_any_tick() {
        let mut game = turning_game(false);
        let mut renderer = SettingsRenderer::default();
        let before = draw_frame(&mut game, &mut renderer);
        let player = game.sim().player();

        mouse(&mut game, 400.0, 200.0);
        let after = draw_frame(&mut game, &mut renderer);

        let (was, now) = (forward_of(before), forward_of(after));
        assert!(now.distance(was) > 1e-3, "the frame did not turn");
        assert!(
            now.x.atan2(now.z) < was.x.atan2(was.z),
            "right is negative yaw"
        );
        assert!(now.y < was.y, "down looks down");
        assert_eq!(game.sim().player(), player, "no tick ran");
        assert_eq!(after, expected_view(&game));
    }

    #[test]
    fn tick_lands_on_the_last_frames_view() {
        const MOTION: [(f64, f64); 3] = [(120.0, 40.0), (-30.0, 80.0), (200.0, -150.0)];

        for smoothing in [false, true] {
            let mut game = turning_game(smoothing);
            let mut renderer = SettingsRenderer::default();
            let level = draw_frame(&mut game, &mut renderer);
            let start = game.sim().player();

            for (dx, dy) in MOTION {
                mouse(&mut game, dx, dy);
                draw_frame(&mut game, &mut renderer);
            }
            let last_frame = *renderer.views.last().expect("a frame began");
            assert_ne!(
                last_frame, level,
                "smoothing {smoothing}: the frames turned"
            );
            let (look_x, look_y) = game.input.pending_look();
            tick_once(&mut game);

            let landed = game.sim().player();
            assert_eq!(landed.yaw, start.yaw + (-look_x), "smoothing {smoothing}");
            assert_eq!(
                landed.pitch,
                (start.pitch + (-look_y))
                    .clamp(-moho_physics::PITCH_LIMIT, moho_physics::PITCH_LIMIT),
                "smoothing {smoothing}"
            );
            assert_eq!(
                game.input.pending_look(),
                (0.0, 0.0),
                "smoothing {smoothing}"
            );
        }
    }

    #[test]
    fn drawn_view_is_the_simulation_turned_by_the_pending_look() {
        for smoothing in [false, true] {
            let mut game = turning_game(smoothing);
            let mut renderer = SettingsRenderer::default();
            let mut previous = draw_frame(&mut game, &mut renderer);

            for (dx, dy) in [(120.0, 40.0), (-30.0, 80.0), (200.0, -150.0)] {
                mouse(&mut game, dx, dy);
                let view = draw_frame(&mut game, &mut renderer);

                assert_ne!(view, previous, "smoothing {smoothing}: the frame turned");
                assert_eq!(view, expected_view(&game), "smoothing {smoothing}");
                previous = view;
            }
        }
    }

    #[test]
    fn pitch_held_at_the_limit_in_the_view_and_after_the_tick() {
        let mut game = turning_game(false);
        let mut renderer = SettingsRenderer::default();
        let limit = moho_physics::PITCH_LIMIT;

        for up in [100_000.0, 5_000.0] {
            mouse(&mut game, 0.0, -up);
            let view = draw_frame(&mut game, &mut renderer);
            tick_once(&mut game);

            assert!(
                (forward_of(view).y - limit.sin()).abs() < 1e-5,
                "view at {}",
                forward_of(view).y
            );
            assert_eq!(game.sim().player().pitch, limit);
        }
    }

    #[test]
    fn unfocused_mouse_motion_does_not_turn() {
        let mut game = turning_game(false);
        let mut renderer = SettingsRenderer::default();
        let level = draw_frame(&mut game, &mut renderer);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(false)));

        mouse(&mut game, 400.0, 200.0);
        let unfocused = draw_frame(&mut game, &mut renderer);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));
        let refocused = draw_frame(&mut game, &mut renderer);
        let command = game.command();

        assert!(forward_of(unfocused).distance(forward_of(level)) < 1e-6);
        assert!(forward_of(refocused).distance(forward_of(level)) < 1e-6);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
    }

    #[test]
    fn open_console_freezes_the_view() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        let mut renderer = SettingsRenderer::default();
        let level = draw_frame(&mut game, &mut renderer);
        mouse(&mut game, 300.0, 150.0);
        let shown = draw_frame(&mut game, &mut renderer);
        assert_ne!(shown, level, "the look showed before the console opened");
        let (look_x, look_y) = game.input.pending_look();
        let start = game.sim().player();

        open_the_console(&mut game, &clock);
        let mut open_views = Vec::new();
        for _ in 0..3 {
            // A paused loop asks for the command on every frame.
            game.command();
            mouse(&mut game, 500.0, 500.0);
            open_views.push(draw_frame(&mut game, &mut renderer));
        }
        let mut ctx = EventContext::new(&clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        assert_eq!(routing, Routing::Closed);
        let resumed = draw_frame(&mut game, &mut renderer);
        tick_once(&mut game);

        assert_eq!(resumed, shown, "the first frame after closing the console");
        for view in open_views {
            assert_eq!(view, shown, "the open console changed the view");
        }
        let landed = game.sim().player();
        assert_eq!(landed.yaw, start.yaw + (-look_x));
        assert_eq!(
            landed.pitch,
            (start.pitch + (-look_y)).clamp(-moho_physics::PITCH_LIMIT, moho_physics::PITCH_LIMIT)
        );
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
    use moho_renderer::FrameError;
    use searchlight_rules::{Body, BodyPart, BodyProfile};

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

#[cfg(test)]
mod enemy_and_hud_tests {
    use std::collections::HashMap;

    use moho_renderer::FrameError;
    use searchlight_rules::Rifle;

    use super::*;
    use crate::test_support::{arena_with, mag, test_config};

    /// Keeps the vertices of every mesh uploaded and the draws of the last frame.
    #[derive(Default)]
    struct RecordingRenderer {
        next_handle: u32,
        uploads: HashMap<u32, Vec<[f32; 3]>>,
        materials: Vec<MaterialGpu>,
        draws: Vec<(u32, InstanceGpu)>,
    }

    impl RecordingRenderer {
        /// Each draw as its mesh's centre and half extents, then its material, in draw order.
        fn drawn(&self) -> Vec<(Vec3, Vec3, MaterialGpu)> {
            self.draws
                .iter()
                .map(|(handle, instance)| {
                    let points: Vec<Vec3> = self.uploads[handle]
                        .iter()
                        .map(|&p| Vec3::from(p))
                        .collect();
                    let min = points.iter().fold(Vec3::MAX, |a, &p| a.min(p));
                    let max = points.iter().fold(Vec3::MIN, |a, &p| a.max(p));
                    (
                        (min + max) / 2.0,
                        (max - min) / 2.0,
                        self.materials[instance.material as usize],
                    )
                })
                .collect()
        }
    }

    impl RendererBackend for RecordingRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("world meshes upload indexed meshes")
        }
        fn register_indexed_mesh(
            &mut self,
            vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            _indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.uploads.insert(self.next_handle, vertices.to_vec());
            self.next_handle
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(&mut self, _camera: (Mat4, Mat4, Vec3)) -> Result<(), FrameError> {
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
        fn update_lighting(&mut self, _lighting: LightingGpu) {}
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

    fn sim_of(arena: Arc<crate::arena::Arena>) -> SearchlightSim {
        SearchlightSim::start(arena, test_config(Rifle::loaded(mag(30, 30)), vec![]))
    }

    /// One enemy at (3, 0, 4) whose only waypoint is ten metres to +X, so it faces +X.
    fn lone_enemy() -> SearchlightSim {
        sim_of(arena_with(
            &[],
            Vec3::new(0.0, 0.0, -20.0),
            &[Vec3::new(3.0, 0.0, 4.0)],
            &[Vec3::new(13.0, 0.0, 4.0)],
        ))
    }

    /// The backend keeps its material table between frames, so one renderer serves a whole test.
    fn drawn_after_sync(
        stage: &mut Stage,
        renderer: &mut RecordingRenderer,
        sim: &SearchlightSim,
    ) -> Vec<(Vec3, Vec3, MaterialGpu)> {
        stage.sync_enemies(sim);
        stage.draw(renderer, (Mat4::IDENTITY, Vec3::ZERO));
        renderer.drawn()
    }

    fn is_paint(material: MaterialGpu, paint: Paint) -> bool {
        let expected = paint.to_gpu();
        (material.albedo, material.params) == (expected.albedo, expected.params)
    }

    #[test]
    fn the_enemy_segment_table_is_the_six_body_volumes_then_the_gun() {
        let segments = enemy_segments();

        for (segment, volume) in segments.iter().zip(dummy_hit_volumes(0)) {
            assert_eq!(*segment, (volume.offset, volume.half_extents));
        }
        assert_eq!(
            segments[6],
            (Vec3::new(-0.3, 0.25, 0.4), Vec3::new(0.04, 0.04, 0.3))
        );
    }

    #[test]
    fn enemy_mesh_ids_start_at_two_times_two_to_the_32_and_hold_eight_slots_per_enemy() {
        let rig = EnemyRig {
            target: 5,
            shown: false,
            gun_flash: 0,
        };

        let ids: Vec<WorldMeshId> = [0, 3, 6]
            .into_iter()
            .map(|slot| rig.mesh_id(slot))
            .collect();

        let base = 2 * (1u64 << 32) + 5 * 8;
        assert_eq!(
            ids,
            [
                WorldMeshId(base),
                WorldMeshId(base + 3),
                WorldMeshId(base + 6)
            ]
        );
    }

    #[test]
    fn syncing_draws_each_living_enemy_as_seven_boxes_at_its_pose() {
        let sim = lone_enemy();
        let state = sim.enemy_state(0).expect("the enemy spawned");
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();

        let drawn = drawn_after_sync(&mut stage, &mut renderer, &sim);

        assert_eq!(drawn.len(), 7);
        let facing = Quat::from_rotation_y(state.yaw);
        let mut offsets: Vec<Vec3> = dummy_hit_volumes(0).iter().map(|v| v.offset).collect();
        offsets.push(Vec3::new(-0.3, 0.25, 0.4));
        for (slot, (centre, _, material)) in drawn.iter().enumerate() {
            let want = state.position + facing * offsets[slot];
            assert!(centre.abs_diff_eq(want, 1e-4), "slot {slot}: {centre:?}");
            assert!(is_paint(*material, Paint::Enemy), "slot {slot}");
        }
        // Facing +X turns the gun's long axis from Z onto X.
        assert!(drawn[6].1.abs_diff_eq(Vec3::new(0.3, 0.04, 0.04), 1e-4));
    }

    #[test]
    fn syncing_numbers_enemy_rigs_after_the_dummies() {
        let mut arena = (*arena_with(
            &[],
            Vec3::new(0.0, 0.0, -20.0),
            &[Vec3::new(3.0, 0.0, 4.0), Vec3::new(8.0, 0.0, 4.0)],
            &[Vec3::new(13.0, 0.0, 4.0)],
        ))
        .clone();
        arena.dummy_spawns = vec![Vec3::new(-8.0, 0.0, 4.0), Vec3::new(-4.0, 0.0, 4.0)];
        let sim = sim_of(Arc::new(arena));
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();
        stage.add_arena(Vec::new(), &sim_dummy_spawns());

        let drawn = drawn_after_sync(&mut stage, &mut renderer, &sim);

        let targets: Vec<u16> = stage.enemies.iter().map(|rig| rig.target).collect();
        assert_eq!(targets, [2, 3]);
        assert_eq!(drawn.len(), 12 + 14);
    }

    fn sim_dummy_spawns() -> [Vec3; 2] {
        [Vec3::new(-8.0, 0.0, 4.0), Vec3::new(-4.0, 0.0, 4.0)]
    }

    #[test]
    fn an_enemy_shot_lights_only_its_gun_until_the_flash_runs_out() {
        let sim = lone_enemy();
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();

        stage.react(&Feedback::EnemyFired { target: 0 }, None);
        let flashing = drawn_after_sync(&mut stage, &mut renderer, &sim);
        for _ in 0..FLASH_TICKS {
            stage.tick_flashes();
        }
        let settled = drawn_after_sync(&mut stage, &mut renderer, &sim);

        assert_eq!(stage.enemies[0].gun_flash, 0);
        let lit: Vec<bool> = flashing
            .iter()
            .map(|d| is_paint(d.2, Paint::Struck))
            .collect();
        assert_eq!(lit, [false, false, false, false, false, false, true]);
        assert!(settled.iter().all(|d| is_paint(d.2, Paint::Enemy)));
    }

    #[test]
    fn an_enemy_shot_starts_a_ten_tick_gun_flash_on_that_enemys_rig() {
        let mut stage = Stage::new();

        stage.react(&Feedback::EnemyFired { target: 4 }, None);
        stage.react(&Feedback::EnemyFired { target: 9 }, None);

        let flashes: Vec<(u16, u32)> = stage
            .enemies
            .iter()
            .map(|rig| (rig.target, rig.gun_flash))
            .collect();
        assert_eq!(flashes, [(4, 10), (9, 10)]);
        assert_eq!(stage.status, "");
    }

    #[test]
    fn looking_up_a_rig_twice_returns_the_same_rig_and_a_new_target_adds_one() {
        let mut stage = Stage::new();

        stage.enemy_rig(3).gun_flash = 7;
        let again = stage.enemy_rig(3).gun_flash;
        let other = stage.enemy_rig(8).gun_flash;

        assert_eq!((again, other), (7, 0));
        assert_eq!(stage.enemies.len(), 2);
        assert_eq!(stage.enemy_rig(8).target, 8);
        assert_eq!(stage.enemy_rig(3).target, 3);
    }

    #[test]
    fn a_hit_edge_lights_its_bar_for_fifteen_ticks_and_fades_one_per_tick() {
        let mut stage = Stage::new();

        stage.show_hit_edge(Edge::Right);
        stage.tick_flashes();

        assert_eq!(stage.edge_left, [0, 14, 0, 0]);
        assert_eq!(EDGE_TICKS, 15);
    }

    #[test]
    fn refreshing_the_hud_hands_the_crosshair_the_death_flag_and_scaled_edge_opacity() {
        let mut stage = Stage::new();
        stage.edge_left = [15, 5, 0, 1];

        stage.refresh_hud(true);

        let hud = stage.crosshair.lock().expect("not poisoned");
        assert!(hud.dead);
        assert_eq!(hud.edges, [190, 63, 0, 12]);
    }

    #[test]
    fn edge_bars_hug_their_own_sides_of_the_screen_in_the_edge_colour() {
        let mut crosshair = Crosshair {
            edges: [10, 20, 30, 40],
            ..Crosshair::default()
        };

        let frame = crosshair.ui_frame([800, 600]).expect("a frame");

        let mesh = &frame.meshes[0];
        assert_eq!(mesh.vertices.len(), 16 + 16);
        let quad = |index: usize| -> Vec<([f32; 2], [u8; 4])> {
            mesh.vertices[index * 4..index * 4 + 4]
                .iter()
                .map(|v| (v.pos, v.color))
                .collect()
        };
        let expect = |min: [f32; 2], max: [f32; 2], alpha: u8| {
            let color = [200, 20, 20, alpha];
            vec![
                (min, color),
                ([max[0], min[1]], color),
                (max, color),
                ([min[0], max[1]], color),
            ]
        };
        assert_eq!(quad(0), expect([0.0, 0.0], [800.0, 28.0], 10));
        assert_eq!(quad(1), expect([772.0, 0.0], [800.0, 600.0], 20));
        assert_eq!(quad(2), expect([0.0, 572.0], [800.0, 600.0], 30));
        assert_eq!(quad(3), expect([0.0, 0.0], [28.0, 600.0], 40));
        assert_eq!(mesh.indices.len(), 48);
    }

    #[test]
    fn a_bar_with_no_opacity_is_not_drawn_and_the_rest_keep_their_order() {
        let mut crosshair = Crosshair {
            edges: [0, 0, 77, 0],
            ..Crosshair::default()
        };

        let frame = crosshair.ui_frame([300, 200]).expect("a frame");

        let mesh = &frame.meshes[0];
        assert_eq!(mesh.vertices.len(), 4 + 16);
        assert_eq!(mesh.vertices[0].pos, [0.0, 172.0]);
        assert_eq!(mesh.vertices[2].pos, [300.0, 200.0]);
        assert_eq!(mesh.vertices[0].color, [200, 20, 20, 77]);
    }

    #[test]
    fn the_death_tint_is_drawn_under_the_crosshair_bars() {
        let mut crosshair = Crosshair {
            dead: true,
            ..Crosshair::default()
        };

        let frame = crosshair.ui_frame([800, 600]).expect("a frame");

        let mesh = &frame.meshes[0];
        assert_eq!(mesh.vertices.len(), 4 + 16);
        assert_eq!(mesh.vertices[0].color, [120, 0, 0, 140]);
        assert_eq!(mesh.vertices[2].pos, [800.0, 600.0]);
    }
}
