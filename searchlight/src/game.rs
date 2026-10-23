//! The prototype as a [`moho_app::Game`]: samples input, ticks the sim, and turns its
//! feedback into sound, a title status line, a hit flash and a crosshair.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use glam::{Mat4, Quat, Vec3};
use moho_app::{Event, EventContext, FrameContext, Game, InitContext, TickContext, WindowMode};
use moho_audio::AudioSystem;
use moho_input::action_map::{ActionMap, handle_device_event, handle_window_event};
use moho_input::bindings::Input;
use moho_input::key::{Key, MouseButton};
use moho_physics::{CharacterState, first_person_view};
use moho_render_api::{
    InstanceGpu, MaterialGpu, MaterialKey, RenderMaterial, Renderable, UiFrame, UiFrameSource,
    UiImage, UiMesh, UiRect, UiSampler, UiSource, UiTextureId, UiTextureSet, UiVertex, WorldMesh,
    WorldMeshId,
};
use moho_renderer::{LightingGpu, RendererBackend, Scene, WorldMeshes};
use winit::event::WindowEvent;
use winit::window::{CursorGrabMode, Window};

use moho_ui_shell::host::input_of;
use moho_ui_shell::{Routing, UiLayers, UiShell, apply_engine_command};

use crate::console_commands::{self, ConsoleCommand, GodModeArg};
use crate::debug_settings::DebugSettings;
use crate::hud::Edge;
use crate::input::{SearchlightAction, SearchlightCommand, look_to_turn};
use crate::menu::{Menu, MenuAction};
use crate::mode::ArenaMode;
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

/// The glTF material name of the arena meshes that draw glossy.
const FLOOR_MATERIAL: &str = "floor";

/// Gloss of the arena floor's paint.
const ARENA_FLOOR_GLOSS: f32 = 0.6;

/// Surface colours of the greybox world.
#[derive(Clone, Copy, Debug)]
enum Paint {
    Arena,
    ArenaFloor,
    Dummy,
    Enemy,
    Struck,
}

impl RenderMaterial for Paint {
    fn to_gpu(&self) -> MaterialGpu {
        // World meshes draw as terrain: `albedo` colours up-facing surfaces, `params.xyz`
        // the rest, and `params.w > 0` makes the surface glow its colour.
        // `albedo.w` is a world mesh's gloss.
        let (top, side, glow, gloss) = match self {
            Paint::Arena => ([0.62, 0.62, 0.64], [0.42, 0.43, 0.46], 0.0, 0.0),
            Paint::ArenaFloor => (
                [0.62, 0.62, 0.64],
                [0.42, 0.43, 0.46],
                0.0,
                ARENA_FLOOR_GLOSS,
            ),
            Paint::Dummy => ([0.85, 0.6, 0.2], [0.7, 0.48, 0.16], 0.0, 0.0),
            Paint::Enemy => ([0.3, 0.4, 0.62], [0.22, 0.3, 0.5], 0.0, 0.0),
            Paint::Struck => ([1.0, 0.15, 0.1], [1.0, 0.15, 0.1], 1.0, 0.0),
        };
        MaterialGpu {
            albedo: [top[0], top[1], top[2], gloss],
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

    /// Takes the segments out of the scene and clears the gun flash.
    fn hide(&mut self, meshes: &mut WorldMeshes) {
        for slot in 0..enemy_segments().len() {
            meshes.remove(self.mesh_id(slot));
        }
        self.shown = false;
        self.gun_flash = 0;
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
    /// Whether a menu owns the view's overlay: nothing is drawn.
    hidden: bool,
}

impl Crosshair {
    const TEXTURE: UiTextureId = UiTextureId(0);
    const GAP: f32 = 4.0;
    const LENGTH: f32 = 9.0;
    const THICKNESS: f32 = 2.0;
}

impl UiFrameSource for Crosshair {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        if self.hidden {
            return None;
        }
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
        ambient: [0.0, 0.0, 0.0, 0.15],
        params: [12.0, 0.0, 0.0, 0.0],
        ..LightingGpu::default()
    }
}

/// Keeps the pointer inside the window for mouse look; a platform that allows neither
/// grab leaves it free.
fn grab_cursor(window: &Window) {
    window.set_cursor_visible(false);
    let grabbed = window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
    if let Err(error) = grabbed {
        tracing::warn!(%error, "cursor could not be captured, mouse look may leave the window");
    }
}

/// A window size of `physical` pixels in logical points, at `scale` pixels per point.
fn logical_size(physical: winit::dpi::PhysicalSize<u32>, scale: f64) -> [f32; 2] {
    let size = physical.to_logical::<f32>(scale);
    [size.width, size.height]
}

/// A position of `physical` pixels in logical points, at `scale` pixels per point.
fn logical_point(physical: [f64; 2], scale: f64) -> [f32; 2] {
    [(physical[0] / scale) as f32, (physical[1] / scale) as f32]
}

/// Everything drawn or heard; the sim knows nothing of it.
#[derive(Debug)]
struct Stage {
    scene: Scene,
    /// Drawn instead of `scene` behind the main menu, so the dormant arena stays untouched.
    backdrop: Scene,
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
    shell: Arc<Mutex<UiShell<ConsoleCommand>>>,
    /// Whether the cursor is captured for mouse look.
    cursor_captured: bool,
    /// Times the cursor was captured, windowed or not, so tests can see it.
    #[cfg(test)]
    cursor_captures: u32,
}

impl Stage {
    fn new() -> Self {
        let mut scene = Scene::new();
        let dummy_material = scene.material_table.find_or_push(&Paint::Dummy);
        let struck_material = scene.material_table.find_or_push(&Paint::Struck);
        let enemy_material = scene.material_table.find_or_push(&Paint::Enemy);
        Self {
            scene,
            backdrop: Scene::new(),
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
            shell: Arc::new(Mutex::new(UiShell::new(
                console_commands::console(&BTreeMap::new()).0,
            ))),
            cursor_captured: false,
            #[cfg(test)]
            cursor_captures: 0,
        }
    }

    /// The window size in logical points; a windowless stage reports 1280 by 720.
    fn screen_points(&self) -> [f32; 2] {
        self.window.as_ref().map_or([1280.0, 720.0], |window| {
            logical_size(window.inner_size(), window.scale_factor())
        })
    }

    /// Captures the cursor for mouse look, if there is a window to capture it in.
    fn capture_cursor(&mut self) {
        #[cfg(test)]
        {
            self.cursor_captures += 1;
        }
        if let Some(window) = &self.window {
            grab_cursor(window);
        }
        self.cursor_captured = true;
    }

    fn release_cursor(&mut self) {
        if let Some(window) = &self.window {
            window.set_cursor_visible(true);
            if let Err(error) = window.set_cursor_grab(CursorGrabMode::None) {
                tracing::warn!(%error, "cursor could not be released");
            }
        }
        self.cursor_captured = false;
    }

    /// The console shell. Never hold the guard across `draw`: the renderer locks the shell
    /// itself when it pulls the UI frame.
    fn shell(&self) -> MutexGuard<'_, UiShell<ConsoleCommand>> {
        self.shell.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn add_arena(&mut self, meshes: Vec<WorldMesh>, dummy_spawns: &[Vec3]) {
        self.add_named_arena(meshes, &[], dummy_spawns);
    }

    /// Draws `meshes` as the arena, the one named `floor` in the glossy [`Paint::ArenaFloor`]
    /// and the rest, or any past the end of `material_names`, in [`Paint::Arena`].
    fn add_named_arena(
        &mut self,
        meshes: Vec<WorldMesh>,
        material_names: &[Option<String>],
        dummy_spawns: &[Vec3],
    ) {
        let arena_material = self.scene.material_table.find_or_push(&Paint::Arena);
        let floor_material = self.scene.material_table.find_or_push(&Paint::ArenaFloor);
        for (index, mesh) in meshes.into_iter().enumerate() {
            let is_floor = material_names
                .get(index)
                .is_some_and(|name| name.as_deref() == Some(FLOOR_MATERIAL));
            self.scene.world_meshes_mut().upsert(
                crate::arena::arena_mesh_id(index),
                mesh,
                if is_floor {
                    floor_material
                } else {
                    arena_material
                },
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

    fn rig_index(&mut self, target: u16) -> usize {
        self.enemies
            .iter()
            .position(|rig| rig.target == target)
            .unwrap_or_else(|| {
                self.enemies.push(EnemyRig {
                    target,
                    shown: false,
                    gun_flash: 0,
                });
                self.enemies.len() - 1
            })
    }

    fn enemy_rig(&mut self, target: u16) -> &mut EnemyRig {
        let index = self.rig_index(target);
        &mut self.enemies[index]
    }

    /// Lights the damage bar on `edge`; it fades over [`EDGE_TICKS`].
    fn show_hit_edge(&mut self, edge: Edge) {
        self.edge_left[edge as usize] = EDGE_TICKS;
    }

    /// Puts every living enemy's segments where the sim has it, and takes the dead ones out.
    fn sync_enemies(&mut self, sim: &SearchlightSim) {
        let known = self.dummy_count + sim.enemy_count();
        for rig in &mut self.enemies {
            if usize::from(rig.target) >= known && rig.shown {
                rig.hide(self.scene.world_meshes_mut());
            }
        }
        for index in 0..sim.enemy_count() {
            let target = u16::try_from(self.dummy_count + index)
                .expect("the sim numbers its targets with a u16");
            let rig_index = self.rig_index(target);
            let rig = &mut self.enemies[rig_index];
            let meshes = self.scene.world_meshes_mut();
            let Some(state) = sim.enemy_state(index) else {
                if rig.shown {
                    rig.hide(meshes);
                }
                continue;
            };
            rig.shown = true;
            let gun_flash = rig.gun_flash;
            let ids: [WorldMeshId; 7] = std::array::from_fn(|slot| rig.mesh_id(slot));
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
        hud.hidden = false;
        hud.dead = dead;
        hud.edges = self
            .edge_left
            .map(|left| (EDGE_ALPHA * left / EDGE_TICKS) as u8);
    }

    fn hide_hud(&self) {
        self.crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .hidden = true;
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
        let aspect = self.aspect();
        draw_scene(&mut self.scene, renderer, eye, aspect);
    }

    fn draw_backdrop(&mut self, renderer: &mut dyn RendererBackend) {
        let aspect = self.aspect();
        draw_scene(
            &mut self.backdrop,
            renderer,
            (Mat4::IDENTITY, Vec3::ZERO),
            aspect,
        );
    }
}

fn draw_scene(
    scene: &mut Scene,
    renderer: &mut dyn RendererBackend,
    eye: (Mat4, Vec3),
    aspect: f32,
) {
    renderer.update_lighting(lighting());
    let (view, eye) = eye;
    let projection =
        glam::camera::rh::proj::directx::perspective(FOV_DEGREES.to_radians(), aspect, NEAR, FAR);
    let no_actors: &[NoActor] = &[];
    if let Err(error) = scene.render(
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

/// What the player is looking at: the main menu, or the arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    MainMenu,
    Playing,
    Paused,
}

/// The mode the window opens in for a saved `mode`: both fullscreen kinds open borderless.
pub fn start_window_mode(mode: moho_core::prefs::WindowMode) -> WindowMode {
    match mode {
        moho_core::prefs::WindowMode::Windowed => WindowMode::Windowed,
        moho_core::prefs::WindowMode::Fullscreen | moho_core::prefs::WindowMode::Borderless => {
            WindowMode::Borderless
        }
    }
}

/// The FPS prototype's game: one arena mode and the input that drives it.
#[derive(Debug)]
pub struct SearchlightGame {
    screen: Screen,
    menu: Menu,
    /// The cursor's last position, in logical points from the window's top left.
    cursor: [f32; 2],
    mode: ArenaMode,
    input: ActionMap<SearchlightAction>,
    /// The player as the tick before the last left them, for blending the view.
    previous: CharacterState,
    /// Whether the window has focus; look is sampled only while it does.
    focused: bool,
    stage: Stage,
    god_mode: bool,
    debug_settings: Option<DebugSettings>,
}

impl SearchlightGame {
    /// A game ticking `mode` with commands sampled from `input`.
    pub fn new(mode: ArenaMode, input: ActionMap<SearchlightAction>) -> Self {
        let previous = mode.sim().player();
        Self {
            screen: Screen::Playing,
            menu: Menu::main(),
            cursor: [0.0, 0.0],
            mode,
            input,
            previous,
            focused: true,
            stage: Stage::new(),
            god_mode: false,
            debug_settings: None,
        }
    }

    #[must_use]
    pub fn with_main_menu(mut self) -> Self {
        self.screen = Screen::MainMenu;
        self
    }

    /// Whether input goes to the console or the menu instead of the game: the loop pauses,
    /// the action map is fed nothing and no look is sampled.
    fn input_captured(&self) -> bool {
        self.screen != Screen::Playing || self.stage.shell().captures_input()
    }

    /// Whether the window opens with the cursor captured: only when it opens in play, since the
    /// main menu needs a free pointer.
    fn captures_cursor_on_start(&self) -> bool {
        self.screen == Screen::Playing
    }

    /// The cursor moved to `physical`, in physical pixels from the window's top left.
    fn cursor_moved(&mut self, physical: [f64; 2]) {
        // A windowless stage reports a scale of 1.
        let scale = self
            .stage
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor());
        self.cursor = logical_point(physical, scale);
        if self.in_menu() {
            self.menu.hover(self.cursor, self.stage.screen_points());
        }
    }

    fn in_menu(&self) -> bool {
        matches!(self.screen, Screen::MainMenu | Screen::Paused)
    }

    fn enter_play(&mut self) {
        self.screen = Screen::Playing;
        // Whatever was pressed or moved over the menu must not act in the arena.
        self.input.release_all();
        self.input.discard_edges();
        self.input.reset_look();
        self.stage.capture_cursor();
    }

    fn pause(&mut self) {
        self.screen = Screen::Paused;
        self.menu = Menu::pause();
        self.input.release_all();
        self.input.discard_edges();
        self.input.reset_look();
        self.stage.release_cursor();
    }

    fn choose(&mut self, ctx: &mut EventContext<'_>, action: MenuAction) {
        match action {
            MenuAction::Play | MenuAction::Resume => self.enter_play(),
            MenuAction::Settings => {}
            MenuAction::Quit => ctx.request_exit(),
        }
    }

    /// A key or button press on a menu; releases do nothing, and Escape resumes the pause menu.
    fn menu_input(
        &mut self,
        ctx: &mut EventContext<'_>,
        input: Input,
        pressed: bool,
        repeat: bool,
    ) {
        if !pressed {
            return;
        }
        match input {
            Input::Key(Key::ArrowUp) => self.menu.up(),
            Input::Key(Key::ArrowDown) => self.menu.down(),
            Input::Key(Key::Enter) if !repeat => {
                let action = self.menu.activate();
                self.choose(ctx, action);
            }
            Input::Key(Key::Escape) if !repeat && self.screen == Screen::Paused => {
                self.enter_play();
            }
            Input::Mouse(MouseButton::Left) if !repeat => {
                if let Some(action) = self.menu.click(self.cursor, self.stage.screen_points()) {
                    self.choose(ctx, action);
                }
            }
            _ => {}
        }
    }

    /// Starts in the god mode `settings` holds and saves every change to it.
    #[must_use]
    pub fn with_debug_settings(mut self, settings: DebugSettings) -> Self {
        self.god_mode = settings.god_mode();
        self.debug_settings = Some(settings);
        self
    }

    /// Applies the console's queued commands in order, saves god mode once if they changed it,
    /// and reports the value after each bare `tgm`.
    fn apply_console_commands(&mut self) {
        let queued = self.stage.shell().console_mut().take_commands();
        if queued.is_empty() {
            return;
        }
        let before = self.god_mode;
        for ConsoleCommand::GodMode(arg) in queued {
            match arg {
                GodModeArg::Toggle => {
                    self.god_mode = !self.god_mode;
                    let text = if self.god_mode {
                        "God mode: on"
                    } else {
                        "God mode: off"
                    };
                    self.stage.shell().console_mut().print(text);
                }
                GodModeArg::Set(on) => self.god_mode = on,
            }
        }
        if self.god_mode != before
            && let Some(settings) = &mut self.debug_settings
            && let Err(error) = settings.set_god_mode(self.god_mode)
        {
            tracing::warn!(%error, god_mode = self.god_mode, "god mode not saved");
        }
    }

    /// Draws `meshes` as the arena and one dummy at each of `dummy_spawns`.
    #[must_use]
    pub fn with_arena(mut self, meshes: Vec<WorldMesh>, dummy_spawns: &[Vec3]) -> Self {
        self.stage.add_arena(meshes, dummy_spawns);
        self
    }

    /// Like [`Self::with_arena`], with `material_names` (one per mesh) choosing each mesh's
    /// paint: meshes named `floor` draw glossy.
    #[must_use]
    pub fn with_named_arena(
        mut self,
        meshes: Vec<WorldMesh>,
        material_names: &[Option<String>],
        dummy_spawns: &[Vec3],
    ) -> Self {
        self.stage
            .add_named_arena(meshes, material_names, dummy_spawns);
        self
    }

    /// Plays the sound file at `path` for every shot fired.
    #[must_use]
    pub fn with_shot_sound(mut self, path: &Path) -> Self {
        self.stage.shot_sound = Some(path.to_owned());
        self
    }

    pub fn sim(&self) -> &SearchlightSim {
        self.mode.sim()
    }

    /// The input map, for feeding it device events.
    pub fn input_mut(&mut self) -> &mut ActionMap<SearchlightAction> {
        &mut self.input
    }

    /// Replaces the default console shell.
    #[must_use]
    pub fn with_shell(mut self, shell: UiShell<ConsoleCommand>) -> Self {
        self.stage.shell = Arc::new(Mutex::new(shell));
        self
    }

    fn show_menu_texts(&self) {
        if let Err(error) = self.stage.shell().set_texts(self.menu.texts()) {
            tracing::warn!(%error, "menu text not shown");
        }
    }

    /// A frame of the main menu: the title bare, the HUD hidden, an empty world and the
    /// menu text for the shell to paint.
    fn menu_frame(&mut self, ctx: &mut FrameContext<'_>) {
        self.stage.refresh_title("");
        self.stage.hide_hud();
        self.show_menu_texts();
        if let Some(renderer) = ctx.renderer() {
            self.stage.draw_backdrop(renderer);
        }
    }

    /// Where one key or button press or release goes: the console sees it first, so a game
    /// action bound to the console key never shadows it. The window-event path calls this once
    /// it has an `Input`, so it is testable without a winit `KeyEvent`.
    fn route_input(
        &mut self,
        ctx: &mut EventContext<'_>,
        input: Input,
        pressed: bool,
        repeat: bool,
    ) -> Routing {
        let routing = self.stage.shell().route(input, pressed, repeat);
        match routing {
            // A key held as the console opened would otherwise stay held: its release goes to
            // the console.
            Routing::Opened => self.input.release_all(),
            Routing::Console | Routing::Closed => {}
            Routing::Game if self.in_menu() => {
                self.menu_input(ctx, input, pressed, repeat);
            }
            Routing::Game => {
                if pressed && !repeat && input == Input::Key(Key::Escape) {
                    self.pause();
                    return routing;
                }
                match input {
                    Input::Key(key) => self.input.key(key, pressed),
                    Input::Mouse(button) => self.input.mouse_button(button, pressed),
                    Input::Pad(_) => {}
                }
            }
        }
        routing
    }
}

impl Game for SearchlightGame {
    type Command = SearchlightCommand;

    fn command(&mut self) -> SearchlightCommand {
        self.apply_console_commands();
        // The paused loop asks every frame and drops the command; ending the tick would
        // consume the look the frames showed and snap the view back. Edges are dropped
        // instead, so a press made as the console opened does not act when it closes.
        if self.input_captured() {
            self.input.discard_edges();
            return SearchlightCommand::default();
        }
        SearchlightCommand {
            god_mode: self.god_mode,
            ..SearchlightCommand::from_frame(&self.input.end_tick())
        }
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>, command: &SearchlightCommand) {
        self.previous = self.mode.sim().player();
        let was_dead = self.mode.sim().player_body().is_dead();
        self.mode.tick(command, ctx.tick_length.as_secs_f32());
        if was_dead && !self.mode.sim().player_body().is_dead() {
            self.previous = self.mode.sim().player();
            self.stage.status.clear();
        }
        self.stage.tick_flashes();
    }

    fn init(&mut self, ctx: &mut InitContext<'_>) {
        self.stage.window = Some(ctx.window.clone());
        if self.captures_cursor_on_start() {
            self.stage.capture_cursor();
        }
        self.stage.shell().attach(ctx.window.clone());
        let crosshair: UiSource = self.stage.crosshair.clone();
        let shell: UiSource = self.stage.shell.clone();
        let layers: UiSource = Arc::new(Mutex::new(UiLayers::new(vec![crosshair, shell])));
        ctx.renderer.set_ui_source(Some(layers));
    }

    fn window_mode_changed(&mut self, _ctx: &mut EventContext<'_>, mode: WindowMode) {
        if let Some(settings) = &mut self.debug_settings
            && start_window_mode(settings.window_mode()) != mode
        {
            let saved = match mode {
                WindowMode::Windowed => moho_core::prefs::WindowMode::Windowed,
                WindowMode::Borderless => moho_core::prefs::WindowMode::Borderless,
            };
            if let Err(error) = settings.set_window_mode(saved) {
                tracing::warn!(%error, ?mode, "window mode not saved");
            }
        }
        // The platform drops held keys' releases and the grab across a mode change.
        self.input.release_all();
        self.input.reset_look();
        if !self.input_captured() {
            self.stage.capture_cursor();
        }
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
                    if let WindowEvent::CursorMoved { position, .. } = event {
                        self.cursor_moved([position.x, position.y]);
                    }
                    if self.screen == Screen::Playing {
                        handle_window_event(&mut self.input, &event);
                    }
                }
            }
            Event::Device(event) => {
                if self.focused && !self.input_captured() {
                    handle_device_event(&mut self.input, &event);
                }
            }
        }
        ctx.set_paused(self.input_captured());
    }

    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32) {
        let captured = self.input_captured();
        ctx.set_paused(captured);
        if self.focused && !captured {
            self.input.sample_frame(ctx.frame_dt);
        }
        let engine_commands = self.stage.shell().console_mut().take_engine_commands();
        for command in &engine_commands {
            apply_engine_command(command, ctx);
        }
        for feedback in self.mode.drain_feedback() {
            if let Feedback::PlayerHit { from, .. } = feedback {
                let player = self.mode.sim().player();
                self.stage
                    .show_hit_edge(hud::hit_edge(player.position, player.yaw, from));
            }
            self.stage.react(&feedback, ctx.audio());
        }
        if self.screen == Screen::MainMenu {
            self.menu_frame(ctx);
            return;
        }
        let sim = self.mode.sim();
        let dead = sim.player_body().is_dead();
        let vitals = hud::death_text(dead).map_or_else(
            || hud::health_text(sim.player_body().health(), sim.player_max_health()),
            str::to_owned,
        );
        let vitals = hud::vitals_text(&vitals, self.mode.wave(), self.mode.score());
        let vitals = hud::god_mode_text(&vitals, self.god_mode);
        self.stage.refresh_title(&vitals);
        self.stage.refresh_hud(dead);
        if self.screen == Screen::Paused {
            self.stage.hide_hud();
            self.show_menu_texts();
        }
        self.stage.sync_enemies(self.mode.sim());
        if let Some(renderer) = ctx.renderer() {
            let view = first_person_view(
                &self.previous,
                &self.mode.sim().player(),
                self.mode.sim().eye_height(),
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
    use moho_app::{GameClock, HeadlessLoop, LoopConfig, WindowMode};
    use moho_core::prefs::Prefs;
    use moho_input::bindings::ActionBindings;
    use winit::event::{DeviceEvent, WindowEvent};

    use super::*;
    use crate::sim::Feedback;
    use crate::test_support::{arena_with, mag, press, test_config, wave_tuning};
    use searchlight_rules::Rifle;

    fn game() -> SearchlightGame {
        let mode = ArenaMode::new(
            arena_with(&[], Vec3::ZERO, &[], &[]),
            test_config(Rifle::loaded(mag(30, 5)), vec![]),
            wave_tuning(1),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
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
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
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
    fn a_dead_player_restarts_with_f8_and_enter_does_nothing() {
        let (mut game, mut clock) = game_with_a_player_dead_away_from_the_spawn();
        let spawn = SearchlightSim::start(arena_arena(), arena_config()).player();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::Enter), true, false);
        game.route_input(&mut ctx, Input::Key(Key::Enter), false, false);
        let enter = game.command();
        tick_with(&mut game, &mut clock, &enter);
        let dead_after_enter = game.sim().player_body().is_dead();

        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::F8), true, false);
        game.route_input(&mut ctx, Input::Key(Key::F8), false, false);
        let f8 = game.command();
        tick_with(&mut game, &mut clock, &f8);

        assert!(!enter.restart);
        assert!(dead_after_enter, "Enter restarted the game");
        assert!(f8.restart);
        assert!(!game.sim().player_body().is_dead());
        assert_eq!(game.sim().player(), spawn);
    }

    #[test]
    fn restarting_clears_the_status_line() {
        let (mut game, mut clock) = game_with_a_player_dead_away_from_the_spawn();

        tick_with(&mut game, &mut clock, &restart_command());

        assert_eq!(game.stage.status, "");
    }

    #[test]
    fn an_ordinary_alive_tick_blends_from_the_previous_pose_and_keeps_the_status() {
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
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

    const BACKTICK: Input = Input::Key(Key::Backtick);
    const TICK: Duration = Duration::from_millis(16);

    fn open_the_console(game: &mut SearchlightGame, clock: &GameClock) {
        let mut ctx = EventContext::new(clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        assert_eq!(routing, Routing::Opened);
    }

    /// A game whose enemy is walking toward the player, so every sim tick moves it.
    fn game_with_a_moving_enemy() -> SearchlightGame {
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
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
            _material: &[u32],
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
    fn escape_with_the_console_open_closes_only_the_console() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        let mut ctx = EventContext::new(&clock, None, None);

        let routing = game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);

        assert_eq!(routing, Routing::Closed);
        assert!(!ctx.exit_requested());
        assert!(!game.stage.shell.lock().unwrap().captures_input());
        assert_eq!(game.screen, Screen::Playing);
    }

    #[test]
    fn an_escape_repeat_in_play_does_not_pause() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        let routing = game.route_input(&mut ctx, Input::Key(Key::Escape), true, true);

        assert_eq!(routing, Routing::Game);
        assert_eq!(game.screen, Screen::Playing);
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn escape_in_play_pauses_frees_the_cursor_and_does_not_exit() {
        let mut game = game();
        game.stage.capture_cursor();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        let routing = game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);
        game.event(&mut ctx, motion());

        assert_eq!(routing, Routing::Game);
        assert_eq!(game.screen, Screen::Paused);
        assert!(!game.stage.cursor_captured);
        assert!(!ctx.exit_requested());
        assert_eq!(ctx.requested_paused(), Some(true));
        assert_eq!(game.menu.selected(), Some(MenuAction::Resume));
        let labels: Vec<String> = game.menu.texts().into_iter().map(|t| t.text).collect();
        assert_eq!(labels, ["Resume", "Settings", "Quit"]);
    }

    #[test]
    fn an_open_console_stops_movement_and_look() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);

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
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);

        game.event(&mut ctx, Event::Window(WindowEvent::Focused(false)));
        let command = game.command();

        assert_eq!(command.movement.forward, 0.0);
    }

    #[test]
    fn a_held_movement_key_moves_forward_until_focus_is_lost() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);

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

    #[test]
    fn window_mode_changed_releases_held_keys_and_recaptures_the_cursor_in_play() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        mouse(&mut game, 40.0, 30.0);
        draw_frame(&mut game, &mut SettingsRenderer::default());
        assert_ne!(game.input.pending_look(), (0.0, 0.0));

        game.window_mode_changed(&mut ctx, WindowMode::Borderless);
        let command = game.command();

        assert_eq!(command.movement.forward, 0.0);
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
        assert_eq!(game.stage.cursor_captures, 1);
    }

    #[test]
    fn window_mode_changed_leaves_the_cursor_free_in_the_pause_menu() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        mouse(&mut game, 40.0, 30.0);
        draw_frame(&mut game, &mut SettingsRenderer::default());
        open_the_console(&mut game, &clock);
        let mut ctx = EventContext::new(&clock, None, None);

        game.window_mode_changed(&mut ctx, WindowMode::Windowed);

        assert_eq!(game.input.pending_look(), (0.0, 0.0));
        assert_eq!(game.stage.cursor_captures, 0);
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
                invert_y: false,
                acceleration: 0.0,
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
            &game.sim().player(),
            game.sim().eye_height(),
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
    fn a_press_made_as_the_console_opens_does_not_act_on_close() {
        for press_binding in [
            Input::Mouse(moho_input::key::MouseButton::Left),
            Input::Key(Key::C),
            // Reload taps on R's release, so the release after close must not tap either.
            Input::Key(Key::R),
        ] {
            let mut game = turning_game(false);
            let clock = GameClock::default();
            let mut renderer = SettingsRenderer::default();
            let mut ctx = EventContext::new(&clock, None, None);
            let routing = game.route_input(&mut ctx, press_binding, true, false);
            assert_eq!(routing, Routing::Game);

            open_the_console(&mut game, &clock);
            for _ in 0..3 {
                // A paused loop asks for the command on every frame.
                game.command();
                draw_frame(&mut game, &mut renderer);
            }
            let routing = game.route_input(&mut ctx, BACKTICK, true, false);
            assert_eq!(routing, Routing::Closed);
            let command = game.command();

            assert!(!command.fire, "{press_binding:?}: fire");
            assert!(!command.reload, "{press_binding:?}: reload");
            assert!(!command.check, "{press_binding:?}: check");

            game.route_input(&mut ctx, press_binding, false, false);
            let command = game.command();

            assert!(!command.reload, "{press_binding:?}: reload on release");
            assert!(!command.check, "{press_binding:?}: check on release");
        }
    }

    #[test]
    fn motion_just_before_the_console_opens_waits_for_it_to_close() {
        for stop in ["console", "focus lost"] {
            let mut game = turning_game(false);
            let clock = GameClock::default();
            let mut renderer = SettingsRenderer::default();
            let level = draw_frame(&mut game, &mut renderer);
            mouse(&mut game, 300.0, 150.0);

            if stop == "console" {
                open_the_console(&mut game, &clock);
            } else {
                let mut ctx = EventContext::new(&clock, None, None);
                game.event(&mut ctx, Event::Window(WindowEvent::Focused(false)));
            }
            for _ in 0..3 {
                let view = draw_frame(&mut game, &mut renderer);

                assert_eq!(view, level, "{stop}: the view turned while stopped");
            }
            let mut ctx = EventContext::new(&clock, None, None);
            match stop {
                "console" => {
                    let routing = game.route_input(&mut ctx, BACKTICK, true, false);
                    assert_eq!(routing, Routing::Closed);
                }
                _ => game.event(&mut ctx, Event::Window(WindowEvent::Focused(true))),
            }
            let resumed = draw_frame(&mut game, &mut renderer);

            assert_ne!(
                resumed, level,
                "{stop}: the waiting motion shows once play resumes"
            );
        }
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

    fn type_into_console(game: &SearchlightGame, line: &str) {
        game.stage.shell().console_mut().submit(line);
    }

    fn console_lines(game: &SearchlightGame) -> Vec<String> {
        game.stage
            .shell()
            .console_mut()
            .output()
            .lines()
            .iter()
            .cloned()
            .collect()
    }

    /// Types `line` and runs one `command()`, returning the god mode it carries.
    fn tgm_tick(game: &mut SearchlightGame, line: &str) -> bool {
        type_into_console(game, line);
        game.command().god_mode
    }

    fn settings_in(dir: &tempfile::TempDir) -> (DebugSettings, std::path::PathBuf) {
        let path = dir.path().join("searchlight.ini");
        (DebugSettings::new(Prefs::default(), path.clone()), path)
    }

    fn saved_god_mode(path: &std::path::Path) -> Option<String> {
        let (saved, _) = Prefs::load_from(path);
        saved.debug_value("god_mode").map(str::to_owned)
    }

    #[test]
    fn tgm_flips_god_mode_each_time() {
        let mut game = game();

        let start = game.command().god_mode;
        let first = tgm_tick(&mut game, "tgm");
        let second = tgm_tick(&mut game, "tgm");
        let third = tgm_tick(&mut game, "tgm");

        assert_eq!([start, first, second, third], [false, true, false, true]);
        assert!(game.god_mode);
    }

    #[test]
    fn bare_tgm_prints_the_value_it_set() {
        let mut game = game();

        type_into_console(&game, "tgm");
        game.command();
        type_into_console(&game, "tgm");
        game.command();

        let lines = console_lines(&game);
        let tail = &lines[lines.len() - 4..];
        assert_eq!(tail, ["> tgm", "God mode: on", "> tgm", "God mode: off"]);
    }

    #[test]
    fn tgm_1_twice_leaves_god_mode_on() {
        let mut game = game();

        let first = tgm_tick(&mut game, "tgm 1");
        let second = tgm_tick(&mut game, "tgm 1");

        assert!(first && second);
        assert!(game.god_mode);
    }

    #[test]
    fn tgm_0_turns_god_mode_off() {
        let mut game = game();
        let on = tgm_tick(&mut game, "tgm 1");

        let off = tgm_tick(&mut game, "tgm 0");

        assert!(on);
        assert!(!off);
        assert!(!game.god_mode);
    }

    #[test]
    fn queued_commands_apply_in_order_within_one_command_call() {
        let mut game = game();
        type_into_console(&game, "tgm 1");
        type_into_console(&game, "tgm");
        type_into_console(&game, "tgm");

        let command = game.command();

        assert!(command.god_mode);
        assert!(game.god_mode);
    }

    #[test]
    fn a_refused_tgm_leaves_god_mode_on() {
        let mut game = game();
        tgm_tick(&mut game, "tgm 1");

        let after_two = tgm_tick(&mut game, "tgm 2");
        let after_on = tgm_tick(&mut game, "tgm on");
        let after_two_args = tgm_tick(&mut game, "tgm 1 0");

        assert!(after_two && after_on && after_two_args);
        assert!(game.god_mode);
    }

    #[test]
    fn the_command_carries_god_mode_every_tick() {
        let mut game = game();
        tgm_tick(&mut game, "tgm 1");

        let later: Vec<bool> = (0..3).map(|_| game.command().god_mode).collect();
        tgm_tick(&mut game, "tgm 0");
        let after_off: Vec<bool> = (0..3).map(|_| game.command().god_mode).collect();

        assert_eq!(later, [true, true, true]);
        assert_eq!(after_off, [false, false, false]);
    }

    #[test]
    fn god_mode_survives_a_restart() {
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        game.mode.sim_mut().kill_player();
        assert!(game.sim().player_body().is_dead());
        tgm_tick(&mut game, "tgm 1");

        let restart = SearchlightCommand {
            restart: true,
            ..game.command()
        };
        tick_with(&mut game, &mut clock, &restart);
        let mut hits = 0;
        for _ in 0..300 {
            let command = game.command();
            tick_with(&mut game, &mut clock, &command);
            hits += game
                .mode
                .drain_feedback()
                .iter()
                .filter(|fb| matches!(fb, Feedback::PlayerHit { .. }))
                .count();
        }

        assert!(hits > 0, "no enemy ever hit the player");
        assert!(!game.sim().player_body().is_dead());
        assert_eq!(
            game.sim().player_body().health(),
            game.sim().player_max_health()
        );
    }

    fn saved_window_mode(path: &std::path::Path) -> Option<moho_core::prefs::WindowMode> {
        // Loading a missing file writes defaults, so look before loading.
        if !path.exists() {
            return None;
        }
        let (saved, _) = Prefs::load_from(path);
        Some(saved.window_mode())
    }

    #[test]
    fn a_saved_borderless_or_fullscreen_mode_starts_borderless_and_windowed_starts_windowed() {
        use moho_core::prefs::WindowMode as Saved;

        let table = [
            (Saved::Windowed, WindowMode::Windowed),
            (Saved::Fullscreen, WindowMode::Borderless),
            (Saved::Borderless, WindowMode::Borderless),
        ];

        for (saved, expected) in table {
            assert_eq!(start_window_mode(saved), expected, "{saved:?}");
        }
    }

    #[test]
    fn a_flip_saves_the_new_mode() {
        use moho_core::prefs::WindowMode as Saved;

        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        game.window_mode_changed(&mut ctx, WindowMode::Borderless);
        let borderless = saved_window_mode(&path);
        game.window_mode_changed(&mut ctx, WindowMode::Windowed);
        let windowed = saved_window_mode(&path);

        assert_eq!(borderless, Some(Saved::Borderless));
        assert_eq!(windowed, Some(Saved::Windowed));
    }

    #[test]
    fn a_requested_mode_is_not_saved_twice_or_looped() {
        use moho_core::prefs::WindowMode as Saved;

        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.window_mode_changed(&mut ctx, WindowMode::Borderless);
        assert_eq!(saved_window_mode(&path), Some(Saved::Borderless));
        std::fs::remove_file(&path).unwrap();

        game.window_mode_changed(&mut ctx, WindowMode::Borderless);

        assert_eq!(saved_window_mode(&path), None, "saved a mode already saved");
        assert_eq!(ctx.requested_window_mode(), None, "asked for a mode again");
    }

    #[test]
    fn a_saved_god_mode_starts_the_game_in_god_mode() {
        let dir = tempfile::tempdir().unwrap();
        let (mut settings, path) = settings_in(&dir);
        settings.set_god_mode(true).unwrap();
        let mut game = game().with_debug_settings(settings);

        let command = game.command();

        assert!(command.god_mode);
        assert!(game.god_mode);
        assert_eq!(saved_god_mode(&path).as_deref(), Some("1"));
    }

    #[test]
    fn tgm_saves_the_setting() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);

        tgm_tick(&mut game, "tgm 1");
        let on = saved_god_mode(&path);
        tgm_tick(&mut game, "tgm");
        let off = saved_god_mode(&path);

        assert_eq!(on.as_deref(), Some("1"));
        assert_eq!(off.as_deref(), Some("0"));
    }

    #[test]
    fn a_tgm_that_changes_nothing_does_not_save() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);

        tgm_tick(&mut game, "tgm 0");
        tgm_tick(&mut game, "tgm 2");

        assert!(!path.exists());
    }

    #[test]
    fn a_failed_save_keeps_god_mode_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));
        let mut game = game().with_debug_settings(settings);

        let on = tgm_tick(&mut game, "tgm 1");
        let later = game.command().god_mode;
        let off = tgm_tick(&mut game, "tgm");

        assert!(on && later);
        assert!(!off);
    }

    use crate::menu::MenuAction;
    use moho_input::key::MouseButton;

    const FRAME: Duration = Duration::from_micros(16_667);

    fn menu_game() -> SearchlightGame {
        game_with_a_moving_enemy().with_main_menu()
    }

    /// A menu game whose look is set up so mouse motion would turn the view.
    fn turning_menu_game() -> SearchlightGame {
        let mut game = menu_game();
        game.input_mut()
            .set_look(moho_input::action_map::LookSettings {
                look_per_count: 0.002,
                smoothing: false,
                invert_y: false,
                acceleration: 0.0,
            });
        game
    }

    fn press_keys(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, keys: &[Key]) {
        for &key in keys {
            game.route_input(ctx, Input::Key(key), true, false);
            game.route_input(ctx, Input::Key(key), false, false);
        }
    }

    /// Where menu row `row` is drawn, in physical pixels; the test window has scale 1.
    fn row_point(game: &SearchlightGame, row: usize) -> [f64; 2] {
        let texts = game.menu.texts();
        assert!(row < texts.len(), "no menu row {row} in {texts:?}");
        let [width, height] = game.stage.screen_points();
        [
            f64::from(width / 2.0 + texts[row].offset[0]),
            f64::from(height / 2.0 + texts[row].offset[1]),
        ]
    }

    fn click_at(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, at: [f64; 2]) {
        let left = Input::Mouse(MouseButton::Left);
        game.cursor_moved(at);
        game.route_input(ctx, left, true, false);
        game.route_input(ctx, left, false, false);
    }

    fn motion() -> Event {
        Event::Device(DeviceEvent::MouseMotion { delta: (1.0, 1.0) })
    }

    /// Runs `frames` frames of the headless loop and returns the enemy's pose afterwards.
    fn run_frames(
        game: &mut SearchlightGame,
        headless: &mut HeadlessLoop<SearchlightGame>,
        frames: u32,
    ) -> Option<CharacterState> {
        for _ in 0..frames {
            headless.advance(game, FRAME);
        }
        game.sim().enemy_state(0)
    }

    #[test]
    fn a_game_with_a_main_menu_asks_the_loop_to_pause_and_runs_no_ticks() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);
        let (enemy, player, next_tick) = (
            game.sim().enemy_state(0),
            game.sim().player(),
            headless.next_tick(),
        );

        game.event(&mut ctx, motion());
        let after = run_frames(&mut game, &mut headless, 120);

        assert_eq!(game.screen, Screen::MainMenu);
        assert_eq!(ctx.requested_paused(), Some(true));
        assert!(headless.paused());
        assert_eq!(headless.next_tick(), next_tick);
        assert_eq!(after, enemy);
        assert_eq!(game.sim().player(), player);
        assert!(!game.stage.cursor_captured);
    }

    #[test]
    fn a_game_built_with_new_still_starts_in_play() {
        let mut game = game_with_a_moving_enemy();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        let enemy = game.sim().enemy_state(0);

        game.event(&mut ctx, motion());
        let after = run_frames(&mut game, &mut headless, 30);

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(ctx.requested_paused(), Some(false));
        assert!(!headless.paused());
        assert_ne!(after, enemy);
    }

    #[test]
    fn play_starts_the_arena_and_captures_the_cursor() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);
        let enemy = game.sim().enemy_state(0);

        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        game.event(&mut ctx, motion());
        let after = run_frames(&mut game, &mut headless, 30);

        assert_eq!(game.screen, Screen::Playing);
        assert!(game.stage.cursor_captured);
        assert_eq!(ctx.requested_paused(), Some(false));
        assert!(!headless.paused());
        assert_ne!(after, enemy, "the arena runs once Play is chosen");
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn play_from_the_menu_is_a_fresh_first_wave() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);
        let standing = game.sim().enemy_state(0);
        let on_the_menu = run_frames(&mut game, &mut headless, 60);

        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        game.event(&mut ctx, motion());

        assert_eq!(on_the_menu, standing, "no tick ran before Play");
        assert_eq!(game.screen, Screen::Playing);
        assert!(game.stage.cursor_captured);
        assert_eq!(
            game.sim().enemy_state(0),
            standing,
            "Play ticks nothing itself"
        );
        assert_eq!((game.mode.wave(), game.mode.score()), (1, 0));
        assert!(!game.sim().player_body().is_dead());
        assert_eq!(
            game.sim().player_body().health(),
            game.sim().player_max_health()
        );
        assert_eq!(game.sim().enemies_alive(), 1);
    }

    #[test]
    fn quit_on_the_main_menu_requests_exit() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        press_keys(
            &mut game,
            &mut ctx,
            &[Key::ArrowDown, Key::ArrowDown, Key::Enter],
        );

        assert_eq!(game.menu.selected(), Some(MenuAction::Quit));
        assert!(ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
    }

    #[test]
    fn the_arrow_keys_and_enter_choose_play() {
        let sequences: [&[Key]; 3] = [
            &[Key::Enter],
            &[Key::ArrowDown, Key::ArrowUp, Key::Enter],
            &[Key::ArrowDown, Key::ArrowDown, Key::ArrowDown, Key::Enter],
        ];

        for keys in sequences {
            let mut game = menu_game();
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);

            press_keys(&mut game, &mut ctx, keys);

            assert_eq!(game.screen, Screen::Playing, "{keys:?}");
            assert!(game.stage.cursor_captured, "{keys:?}");
            assert!(!ctx.exit_requested(), "{keys:?}");
        }
    }

    #[test]
    fn a_click_on_play_starts_the_arena() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        press_keys(&mut game, &mut ctx, &[Key::ArrowUp]);
        assert_eq!(game.menu.selected(), Some(MenuAction::Quit));

        click_at(&mut game, &mut ctx, [0.0, 0.0]);
        let after_a_miss = (game.screen, ctx.exit_requested());
        let play = row_point(&game, 0);
        click_at(&mut game, &mut ctx, play);
        let command = game.command();

        assert_eq!(after_a_miss, (Screen::MainMenu, false));
        assert_eq!(game.screen, Screen::Playing);
        assert!(game.stage.cursor_captured);
        assert!(!ctx.exit_requested());
        assert!(!command.fire, "the click that chose Play did not fire");
    }

    #[test]
    fn a_click_acts_on_the_item_the_cursor_last_moved_over() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let quit = row_point(&game, 2);

        click_at(&mut game, &mut ctx, quit);

        assert!(ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
    }

    #[test]
    fn movement_look_and_fire_do_not_reach_the_sim_at_the_main_menu() {
        let mut game = turning_menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));

        game.cursor_moved([0.0, 0.0]);
        for input in [
            Input::Key(Key::W),
            Input::Key(Key::Space),
            Input::Key(Key::R),
            Input::Mouse(MouseButton::Left),
        ] {
            game.route_input(&mut ctx, input, true, false);
        }
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion {
                delta: (40.0, 30.0),
            }),
        );
        let command = game.command();

        assert_eq!(game.screen, Screen::MainMenu);
        assert_eq!(command.movement.forward, 0.0);
        assert!(!command.movement.jump);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert!(!command.fire);
        assert!(!command.reload);
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn the_action_map_sees_no_input_while_the_main_menu_shows() {
        use moho_input::Action;
        use winit::event::DeviceId;

        let mut game = turning_menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));

        for input in [
            Input::Key(Key::W),
            Input::Key(Key::Space),
            Input::Key(Key::R),
            Input::Mouse(MouseButton::Left),
        ] {
            game.route_input(&mut ctx, input, true, false);
        }
        game.event(
            &mut ctx,
            Event::Window(WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: winit::dpi::PhysicalPosition::new(12.0, 34.0),
            }),
        );
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion {
                delta: (40.0, 30.0),
            }),
        );
        game.input.sample_frame(Duration::from_millis(16));
        let frame = game.input.end_tick();

        assert_eq!(game.screen, Screen::MainMenu);
        for &action in SearchlightAction::ALL {
            assert!(!frame.held(action), "{action:?} held");
            assert!(!frame.pressed(action), "{action:?} pressed");
            assert!(!frame.released(action), "{action:?} released");
        }
        assert_eq!(frame.look(), (0.0, 0.0));
        assert_eq!(frame.pointer(), None);
        assert_eq!(frame.press_pointer(), None);
    }

    #[test]
    fn escape_at_the_main_menu_does_nothing() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        let routing = game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);
        game.route_input(&mut ctx, Input::Key(Key::Escape), false, false);

        assert_eq!(routing, Routing::Game);
        assert!(!ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
        assert_eq!(game.menu.selected(), Some(MenuAction::Play));
        assert!(!game.stage.cursor_captured);
    }

    #[test]
    fn settings_on_the_main_menu_changes_nothing() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let (player, enemy, wave, score) = (
            game.sim().player(),
            game.sim().enemy_state(0),
            game.mode.wave(),
            game.mode.score(),
        );

        press_keys(&mut game, &mut ctx, &[Key::ArrowDown, Key::Enter]);
        let settings = row_point(&game, 1);
        click_at(&mut game, &mut ctx, settings);
        game.event(&mut ctx, motion());

        assert_eq!(game.menu.selected(), Some(MenuAction::Settings));
        assert_eq!(game.screen, Screen::MainMenu);
        assert!(!ctx.exit_requested());
        assert!(!game.stage.cursor_captured);
        assert_eq!(ctx.requested_paused(), Some(true));
        assert_eq!(game.sim().player(), player);
        assert_eq!(game.sim().enemy_state(0), enemy);
        assert_eq!((game.mode.wave(), game.mode.score()), (wave, score));
    }

    #[test]
    fn a_repeated_enter_on_the_main_menu_chooses_nothing() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        press_keys(&mut game, &mut ctx, &[Key::ArrowUp]);
        assert_eq!(game.menu.selected(), Some(MenuAction::Quit));

        game.route_input(&mut ctx, Input::Key(Key::Enter), true, true);

        assert!(!ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
    }

    #[test]
    fn a_repeated_left_button_press_over_an_item_chooses_nothing() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let quit = row_point(&game, 2);
        game.cursor_moved(quit);

        game.route_input(&mut ctx, Input::Mouse(MouseButton::Left), true, true);

        assert!(!ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
        assert!(!game.stage.cursor_captured);
    }

    #[test]
    fn a_repeated_arrow_key_still_moves_the_selection() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        game.route_input(&mut ctx, Input::Key(Key::ArrowDown), true, true);

        assert_eq!(game.menu.selected(), Some(MenuAction::Settings));
    }

    #[test]
    fn moving_the_cursor_during_play_does_not_select_a_menu_item() {
        let mut game = game_with_a_moving_enemy();
        let quit = [640.0, 360.0 + 56.0];

        game.cursor_moved(quit);

        assert_eq!(game.menu.selected(), Some(MenuAction::Play));
        assert_eq!(game.cursor, [640.0, 416.0]);
    }

    #[test]
    fn moving_the_cursor_over_a_menu_item_selects_it() {
        let mut game = menu_game();
        let quit = row_point(&game, 2);

        game.cursor_moved(quit);

        assert_eq!(game.menu.selected(), Some(MenuAction::Quit));
    }

    #[test]
    fn physical_cursor_positions_are_divided_by_the_scale_factor() {
        assert_eq!(logical_point([200.0, 100.0], 2.0), [100.0, 50.0]);
        assert_eq!(logical_point([30.0, 45.0], 1.5), [20.0, 30.0]);
        assert_eq!(logical_point([7.0, 9.0], 1.0), [7.0, 9.0]);
    }

    #[test]
    fn a_physical_window_size_is_divided_by_the_scale_factor() {
        let size = winit::dpi::PhysicalSize::new(2560, 1440);

        assert_eq!(logical_size(size, 2.0), [1280.0, 720.0]);
        assert_eq!(logical_size(size, 1.0), [2560.0, 1440.0]);
    }

    #[test]
    fn a_windowless_stage_reports_the_default_size() {
        let stage = Stage::new();

        assert_eq!(stage.screen_points(), [1280.0, 720.0]);
    }

    #[test]
    fn only_a_game_that_opens_in_play_captures_the_cursor_on_start() {
        assert!(game_with_a_moving_enemy().captures_cursor_on_start());
        assert!(!menu_game().captures_cursor_on_start());
    }

    #[test]
    fn a_key_held_into_play_does_not_stick_after_play() {
        let mut game = turning_menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));
        game.input.key(Key::W, true);
        game.input.mouse_button(MouseButton::Left, true);
        game.input.mouse_motion(400.0, 300.0);

        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let command = game.command();
        game.route_input(&mut ctx, Input::Key(Key::W), false, false);
        let after_release = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(command.movement.forward, 0.0);
        assert!(!command.fire);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
        assert_eq!(after_release.movement.forward, 0.0);
    }

    fn playing_game() -> SearchlightGame {
        let mut game = game_with_a_moving_enemy();
        game.input_mut()
            .set_look(moho_input::action_map::LookSettings {
                look_per_count: 0.002,
                smoothing: false,
                invert_y: false,
                acceleration: 0.0,
            });
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));
        game.stage.capture_cursor();
        game
    }

    fn pause(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        press_keys(game, ctx, &[Key::Escape]);
        assert_eq!(game.screen, Screen::Paused, "Escape did not pause");
    }

    /// Everything a pause must leave as it found it.
    fn world_of(game: &SearchlightGame) -> (CharacterState, Option<CharacterState>, u32, u32, u32) {
        (
            game.sim().player(),
            game.sim().enemy_state(0),
            game.mode.wave(),
            game.mode.score(),
            game.sim().rifle().magazine().rounds(),
        )
    }

    #[test]
    fn resuming_by_the_menu_item_or_escape_restores_play_and_recaptures_the_cursor() {
        let ways: [&[Key]; 2] = [&[Key::Enter], &[Key::Escape]];

        for way in ways {
            let mut game = playing_game();
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);
            pause(&mut game, &mut ctx);
            assert!(!game.stage.cursor_captured, "{way:?}");

            press_keys(&mut game, &mut ctx, way);
            game.event(&mut ctx, motion());

            assert_eq!(game.screen, Screen::Playing, "{way:?}");
            assert!(game.stage.cursor_captured, "{way:?}");
            assert_eq!(ctx.requested_paused(), Some(false), "{way:?}");
            assert!(!ctx.exit_requested(), "{way:?}");
        }
    }

    #[test]
    fn the_world_is_unchanged_across_a_pause_and_resume() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.step(&mut game, 10);
        let before = world_of(&game);
        let health = game.sim().player_body().health();

        pause(&mut game, &mut ctx);
        headless.advance(&mut game, Duration::ZERO);
        game.event(&mut ctx, motion());
        run_frames(&mut game, &mut headless, 10);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(world_of(&game), before);
        assert_eq!(game.sim().player_body().health(), health);
    }

    #[test]
    fn thirty_seconds_while_paused_run_no_tick() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.step(&mut game, 1);
        pause(&mut game, &mut ctx);
        headless.advance(&mut game, Duration::ZERO);
        let (world, next_tick) = (world_of(&game), headless.next_tick());

        for _ in 0..1800 {
            headless.advance(&mut game, Duration::from_secs(1) / 60);
        }

        assert!(headless.paused());
        assert_eq!(headless.next_tick(), next_tick);
        assert_eq!(world_of(&game), world);
    }

    #[test]
    fn movement_look_and_fire_while_paused_change_nothing() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);

        game.cursor_moved([0.0, 0.0]);
        for input in [
            Input::Key(Key::W),
            Input::Key(Key::Space),
            Input::Key(Key::R),
            Input::Mouse(MouseButton::Left),
        ] {
            game.route_input(&mut ctx, input, true, false);
        }
        mouse(&mut game, 40.0, 30.0);
        let frame = game.input_mut().end_tick();
        let command = game.command();

        assert_eq!(game.screen, Screen::Paused);
        for action in [
            SearchlightAction::MoveForward,
            SearchlightAction::Jump,
            SearchlightAction::Reload,
            SearchlightAction::Fire,
        ] {
            assert!(!frame.held(action), "{action:?} held while paused");
            assert!(!frame.pressed(action), "{action:?} pressed while paused");
        }
        assert_eq!(command.movement.forward, 0.0);
        assert!(!command.movement.jump);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert!(!command.fire);
        assert!(!command.reload);
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn a_key_held_at_the_pause_does_not_move_the_player_after_resume() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        game.route_input(&mut ctx, Input::Mouse(MouseButton::Left), true, false);
        assert_eq!(game.command().movement.forward, 1.0);

        pause(&mut game, &mut ctx);
        game.route_input(&mut ctx, Input::Key(Key::W), false, false);
        game.route_input(&mut ctx, Input::Mouse(MouseButton::Left), false, false);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let resumed = game.command();
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        let pressed_again = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(resumed.movement.forward, 0.0);
        assert!(!resumed.fire);
        assert_eq!(pressed_again.movement.forward, 1.0);
    }

    #[test]
    fn a_key_still_held_at_the_resume_does_not_stick() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);

        pause(&mut game, &mut ctx);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let resumed = game.command();
        game.route_input(&mut ctx, Input::Key(Key::W), false, false);
        let after_release = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(resumed.movement.forward, 0.0);
        assert_eq!(after_release.movement.forward, 0.0);
    }

    #[test]
    fn mouse_motion_while_paused_does_not_turn_the_resumed_view() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);

        mouse(&mut game, 400.0, 300.0);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let command = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
    }

    #[test]
    fn quit_on_the_pause_menu_requests_exit() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);
        assert!(!ctx.exit_requested());

        press_keys(
            &mut game,
            &mut ctx,
            &[Key::ArrowDown, Key::ArrowDown, Key::Enter],
        );

        assert_eq!(game.menu.selected(), Some(MenuAction::Quit));
        assert!(ctx.exit_requested());
        assert_eq!(game.screen, Screen::Paused);
    }

    #[test]
    fn settings_on_the_pause_menu_changes_nothing_and_stays_paused() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);
        let world = world_of(&game);

        press_keys(&mut game, &mut ctx, &[Key::ArrowDown, Key::Enter]);
        game.event(&mut ctx, motion());

        assert_eq!(game.menu.selected(), Some(MenuAction::Settings));
        assert_eq!(game.screen, Screen::Paused);
        assert!(!ctx.exit_requested());
        assert!(!game.stage.cursor_captured);
        assert_eq!(ctx.requested_paused(), Some(true));
        assert_eq!(world_of(&game), world);
    }

    #[test]
    fn escape_while_dead_pauses_and_enter_restarts_only_after_resume() {
        let (mut game, clock) = game_with_a_player_dead_away_from_the_spawn();
        let mut ctx = EventContext::new(&clock, None, None);

        pause(&mut game, &mut ctx);
        let enter = Input::Key(Key::Enter);
        game.route_input(&mut ctx, enter, true, false);
        game.route_input(&mut ctx, enter, false, false);
        let after_resume = game.command();
        game.route_input(&mut ctx, enter, true, false);
        let restart = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert!(
            !after_resume.restart,
            "the Enter that resumed also restarted"
        );
        assert!(restart.restart, "Enter restarts once playing again");
        assert!(!ctx.exit_requested());
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
            _material: &[u32],
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
        assert_eq!(light.ambient, [0.0, 0.0, 0.0, 0.15]);
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

    fn game_with_an_arena_on_the_main_menu() -> SearchlightGame {
        use crate::test_support::{arena_with, mag, test_config, wave_tuning};
        use searchlight_rules::Rifle;

        let mode = ArenaMode::new(
            arena_with(
                &[],
                Vec3::ZERO,
                &[Vec3::new(0.0, 0.0, 10.0)],
                &[Vec3::new(0.0, 0.0, 6.0)],
            ),
            test_config(Rifle::loaded(mag(30, 30)), vec![]),
            wave_tuning(1),
        );
        let (bindings, _) = moho_input::bindings::ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
            .with_arena(
                vec![box_mesh(Vec3::ZERO, Vec3::ONE)],
                &[Vec3::new(5.0, 0.0, 5.0)],
            )
            .with_main_menu()
    }

    fn frame_with(game: &mut SearchlightGame, renderer: &mut FakeRenderer) {
        let clock = moho_app::GameClock::default();
        let backend: &mut dyn RendererBackend = renderer;
        let mut ctx = FrameContext::new(
            std::time::Duration::from_millis(16),
            &clock,
            Some(backend),
            None,
        );
        game.frame(&mut ctx, 0.5);
    }

    fn crosshair_vertices(game: &SearchlightGame) -> usize {
        let mut crosshair = game
            .stage
            .crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        crosshair.ui_frame([800, 600]).map_or(0, |frame| {
            frame.meshes.iter().map(|m| m.vertices.len()).sum()
        })
    }

    #[test]
    fn the_main_menu_frame_draws_no_world_mesh_or_crosshair() {
        let mut game = game_with_an_arena_on_the_main_menu();
        let mut renderer = FakeRenderer::default();

        frame_with(&mut game, &mut renderer);
        let menu_draws = renderer.draws.len();
        let menu_crosshair = crosshair_vertices(&game);
        let menu_enemies = game.stage.enemies.iter().filter(|rig| rig.shown).count();

        assert_eq!(menu_draws, 0, "world meshes drawn behind the menu");
        assert_eq!(menu_crosshair, 0, "crosshair drawn on the menu");
        assert_eq!(menu_enemies, 0, "enemies synced behind the menu");
    }

    #[test]
    fn the_arena_and_crosshair_appear_on_the_first_frame_after_play() {
        let mut game = game_with_an_arena_on_the_main_menu();
        let mut renderer = FakeRenderer::default();
        frame_with(&mut game, &mut renderer);
        let clock = moho_app::GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        game.route_input(&mut ctx, Input::Key(Key::Enter), true, false);
        frame_with(&mut game, &mut renderer);

        assert_eq!(
            renderer.draws.len(),
            1 + 6 + 7,
            "arena, dummy and enemy boxes"
        );
        assert_eq!(crosshair_vertices(&game), 16);
    }

    #[test]
    fn a_paused_frame_still_draws_the_world_but_not_the_crosshair() {
        let mut game = game_with_an_arena_on_the_main_menu();
        let mut renderer = FakeRenderer::default();
        let clock = moho_app::GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::Enter), true, false);
        frame_with(&mut game, &mut renderer);
        assert_eq!(crosshair_vertices(&game), 16);

        game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);
        frame_with(&mut game, &mut renderer);
        let shown = game.stage.enemies.iter().filter(|rig| rig.shown).count();
        let menu = game.stage.shell().ui_frame([1280, 720]);

        assert_eq!(game.screen, Screen::Paused);
        assert_eq!(
            renderer.draws.len(),
            1 + 6 + 7,
            "arena, dummy and enemy boxes"
        );
        assert_eq!(shown, 1);
        assert_eq!(crosshair_vertices(&game), 0, "crosshair drawn while paused");
        let menu = menu.expect("the pause menu text makes the shell draw");
        assert!(!menu.meshes.is_empty());
    }

    #[test]
    fn a_frame_after_resuming_draws_the_crosshair_again() {
        let mut game = game_with_an_arena_on_the_main_menu();
        let mut renderer = FakeRenderer::default();
        let clock = moho_app::GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::Enter), true, false);
        game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);
        frame_with(&mut game, &mut renderer);
        assert_eq!(crosshair_vertices(&game), 0);

        game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);
        frame_with(&mut game, &mut renderer);

        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(crosshair_vertices(&game), 16);
    }

    #[test]
    fn the_main_menu_frame_hands_the_menu_text_to_the_shell() {
        let mut game = game_with_an_arena_on_the_main_menu();
        let mut renderer = FakeRenderer::default();

        frame_with(&mut game, &mut renderer);
        let drawn = game.stage.shell().ui_frame([1280, 720]);

        let frame = drawn.expect("the menu text makes the shell draw with the console closed");
        assert!(!frame.meshes.is_empty());
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
    use crate::sim::Start;
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
            _material: &[u32],
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

    #[test]
    fn the_backdrop_draws_its_own_scene_and_not_the_arena() {
        let mut stage = Stage::new();
        stage.add_arena(vec![box_mesh(Vec3::new(50.0, 0.0, 0.0), Vec3::ONE)], &[]);
        let backdrop_material = stage.backdrop.material_table.find_or_push(&Paint::Arena);
        stage.backdrop.world_meshes_mut().upsert(
            WorldMeshId(1),
            box_mesh(Vec3::new(-7.0, 0.0, 0.0), Vec3::ONE),
            backdrop_material,
        );
        let mut renderer = RecordingRenderer::default();

        stage.draw_backdrop(&mut renderer);
        let drawn = renderer.drawn();

        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, Vec3::new(-7.0, 0.0, 0.0));
    }

    fn is_paint(material: MaterialGpu, paint: Paint) -> bool {
        let expected = paint.to_gpu();
        (material.albedo, material.params) == (expected.albedo, expected.params)
    }

    #[test]
    fn the_arena_floor_draws_in_the_glossy_paint() {
        let floor_at = Vec3::new(0.0, -0.5, 0.0);
        let wall_at = Vec3::new(10.0, 1.0, 0.0);
        let mut stage = Stage::new();
        stage.add_named_arena(
            vec![
                box_mesh(floor_at, Vec3::new(30.0, 0.5, 30.0)),
                box_mesh(wall_at, Vec3::new(0.5, 1.0, 5.0)),
            ],
            &[Some("floor".to_owned()), Some("wall".to_owned())],
            &[],
        );
        let mut renderer = RecordingRenderer::default();

        stage.draw(&mut renderer, (Mat4::IDENTITY, Vec3::ZERO));
        let drawn = renderer.drawn();

        let material_at = |centre: Vec3| {
            drawn
                .iter()
                .find(|d| d.0.abs_diff_eq(centre, 1e-4))
                .map(|d| d.2)
                .expect("the mesh is drawn")
        };
        let floor = material_at(floor_at);
        let wall = material_at(wall_at);
        assert_eq!(floor.albedo[3], 0.6, "floor gloss");
        assert_eq!(wall.albedo[3], 0.0, "wall gloss");
        let flat = Paint::Arena.to_gpu();
        assert_eq!(floor.albedo[..3], flat.albedo[..3], "floor top colour");
        assert_eq!(floor.params, flat.params, "floor side colour");
        assert!(is_paint(wall, Paint::Arena));
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

    #[test]
    fn restarting_removes_the_drawn_models_of_enemies_that_no_longer_exist() {
        let markers = [
            Vec3::new(-3.0, 0.0, 4.0),
            Vec3::new(3.0, 0.0, 4.0),
            Vec3::new(9.0, 0.0, 4.0),
        ];
        let arena = arena_with(
            &[],
            Vec3::new(0.0, 0.0, -20.0),
            &markers,
            &[Vec3::new(13.0, 0.0, 4.0)],
        );
        let config = test_config(Rifle::loaded(mag(30, 30)), vec![]);
        let mut sim = SearchlightSim::with_start(arena, config, Start::Empty);
        for marker in 0..markers.len() {
            sim.spawn_enemy(marker).expect("the marker is free");
        }
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();
        let before = drawn_after_sync(&mut stage, &mut renderer, &sim).len();
        sim.kill_player();
        sim.tick(
            &SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            },
            1.0 / 60.0,
        );
        sim.spawn_enemy(0).expect("the restart emptied the markers");
        assert_eq!(sim.enemy_count(), 1);

        let after = drawn_after_sync(&mut stage, &mut renderer, &sim).len();

        assert_eq!(before, 3 * 7);
        assert_eq!(after, 7, "stale enemy models are still drawn");
        assert_eq!(stage.enemies.iter().filter(|rig| rig.shown).count(), 1);
    }

    #[test]
    fn syncing_keeps_the_gun_flash_of_a_shown_living_enemy_numbered_after_the_dummies() {
        let mut arena = (*arena_with(
            &[],
            Vec3::new(0.0, 0.0, -20.0),
            &[Vec3::new(3.0, 0.0, 4.0)],
            &[Vec3::new(13.0, 0.0, 4.0)],
        ))
        .clone();
        arena.dummy_spawns = vec![Vec3::new(-8.0, 0.0, 4.0), Vec3::new(-4.0, 0.0, 4.0)];
        let sim = sim_of(Arc::new(arena));
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();
        stage.add_arena(Vec::new(), &sim_dummy_spawns());
        drawn_after_sync(&mut stage, &mut renderer, &sim);

        stage.react(&Feedback::EnemyFired { target: 2 }, None);
        let drawn = drawn_after_sync(&mut stage, &mut renderer, &sim);

        assert_eq!(stage.enemies[0].gun_flash, FLASH_TICKS);
        let enemy_draws = &drawn[drawn.len() - 7..];
        assert!(is_paint(enemy_draws[6].2, Paint::Struck));
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
