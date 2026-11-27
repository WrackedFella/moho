//! The prototype as a [`moho_app::Game`]: samples input, ticks the sim, and turns its
//! feedback into sound, a title status line, a hit flash and a crosshair.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use glam::{Affine3A, Mat3, Mat4, Quat, Vec3};
use moho_app::{Event, EventContext, FrameContext, Game, InitContext, TickContext, WindowMode};
use moho_audio::AudioSystem;
use moho_input::action_map::{ActionMap, handle_device_event, handle_window_event};
use moho_input::bindings::{Binding, Input};
use moho_input::key::Key;
use moho_physics::{CharacterBody, CharacterState, HitVolume, first_person_view};
use moho_render_api::{
    InstanceGpu, Marker, MarkerId, MarkerStyle, MaterialGpu, MaterialKey, RenderMaterial,
    Renderable, UiFrame, UiFrameSource, UiImage, UiMesh, UiRect, UiSampler, UiSource, UiTextureId,
    UiTextureSet, UiVertex, WorldMesh, WorldMeshId,
};
use moho_renderer::{EffectHandle, LightingGpu, MarkStyle, RendererBackend, Scene, StreakStyle};
use searchlight_rules::{Grade, GravityScale, Lean, LeanSide};
use winit::event::WindowEvent;
use winit::window::{CursorGrabMode, Window};

use moho_core::prefs::Prefs;
use moho_ui_shell::host::input_of;
use moho_ui_shell::{
    Anchor, EngineSettings, ItemLists, ListId, MenuEvent, Routing, ScreenText, Slot, UiLayers,
    UiShell, apply_engine_command, apply_engine_settings,
};

use crate::aim::{Magnification, aimed_fov_y, look_scale};
use crate::arena::{box_mesh, control_mesh_id};
use crate::character;
use crate::console_commands::{self, ConsoleCommand, GodModeArg, HeadBobArg, SimpleArg};
use crate::control::ControlAction;
use crate::controls::{Capture, ControlsDraft, label};
use crate::debug_settings::{DebugSettings, GameRows};
use crate::head_bob::{Bob, HeadBobLevel};
use crate::hud::Edge;
use crate::input::{
    InventoryCommand, SearchlightAction, SearchlightCommand, build_action_map, look_from,
    look_to_turn, resolve_lean, scale_look,
};
use crate::inventory;
use crate::lean::{lean_offset, rolled_view_matrix};
use crate::menu::{
    MenuAction, conflict_screen, controls_rows, controls_screen, main_screen, pause_screen,
    settings_categories_screen, settings_page,
};
use crate::menu_feed::{MenuCamera, MenuFeed};
use crate::mode::ArenaMode;
use crate::sim::{
    CarrySheet, Feedback, RoundTrace, SearchlightSim, UseTarget, dummy_at, dummy_hit_volumes_of,
};
use crate::tag::SurfaceTag;
use crate::tuning::{SHIPPED_BODY, STRIP_FADE_TICKS};
use crate::{TICK_HZ, hud};

/// Ticks a struck dummy segment stays lit.
const FLASH_TICKS: u32 = TICK_HZ / 6;
/// Dummy segment mesh ids start here, clear of the arena's (which count up from 0).
const DUMMY_MESH_BASE: u64 = 1 << 32;
/// The unit box every enemy segment is placed from, clear of the arena's, the dummies' and the
/// controls' ids.
const ENEMY_BOX_MESH_ID: WorldMeshId = WorldMeshId(2 << 32);
/// Body box mesh ids start here, clear of the enemies'; a body's id is its target number.
const BODY_MESH_BASE: u64 = 3 << 32;
/// Half extents of the box a dead enemy is drawn as, lying on the floor.
const BODY_BOX_HALF: Vec3 = Vec3::new(0.9, 0.15, 0.3);

fn body_mesh_id(number: u16) -> WorldMeshId {
    WorldMeshId(BODY_MESH_BASE + u64::from(number))
}
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
/// The vertical field of view before a saved one is applied.
const FOV_DEGREES: f32 = 75.0;
const NEAR: f32 = 0.05;
const FAR: f32 = 500.0;
const SHOT_VOLUME: f32 = 0.8;
/// The use prompt's distance below the crosshair, in logical points, and its size and colour.
const PROMPT_OFFSET_Y: f32 = 48.0;
const PROMPT_SIZE: f32 = 20.0;
const PROMPT_COLOR: [u8; 4] = [255, 255, 255, 230];
/// The rig strip's distance from the bottom edge, in logical points, and its size and colour.
const STRIP_OFFSET_Y: f32 = -96.0;
const STRIP_SIZE: f32 = 20.0;
const STRIP_COLOR: [u8; 4] = [220, 220, 220, 230];
/// The load icon's distance from the bottom left corner, in logical points. It is drawn at the
/// strip's size.
const LOAD_ICON_OFFSET: [f32; 2] = [24.0, -24.0];
/// The inventory's load header's distance below the top edge, in logical points.
const LOAD_HEADER_OFFSET_Y: f32 = 32.0;

/// The colour the load icon is drawn in.
fn load_icon_color(icon: hud::LoadIcon) -> [u8; 4] {
    match icon {
        hud::LoadIcon::White => [255, 255, 255, 230],
        hud::LoadIcon::Yellow => [255, 214, 51, 230],
        hud::LoadIcon::Red => [230, 51, 51, 230],
    }
}

/// The load line the inventory screen shows for `sheet`.
fn header_of(sheet: &CarrySheet) -> inventory::LoadHeader {
    inventory::load_header(sheet.load_kg, sheet.capacity, sheet.thresholds())
}

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
const MARK_MATERIAL: &str = "mark";
const MARK_RUN_MATERIAL: &str = "mark_run";

const ARENA_FLOOR_GLOSS: f32 = 0.6;

/// How long a tracer streak is, in metres.
const TRACER_LENGTH: f32 = 3.0;
/// How wide a tracer streak is, in metres.
const TRACER_WIDTH: f32 = 0.03;
/// How long an ended tracer takes to fade, so a flight of two to four ticks still reads.
const TRACER_FADE: Duration = Duration::from_millis(150);

/// The streak a round in flight is drawn as.
fn tracer_style(material: u32) -> StreakStyle {
    StreakStyle {
        fade: TRACER_FADE,
        ..StreakStyle::new(TRACER_LENGTH, TRACER_WIDTH, material)
    }
}

/// Bullet mark appearance; `material` is the stage's own.
fn impact_style(material: u32) -> MarkStyle {
    MarkStyle {
        size: 0.05,
        material,
        lifetime: Duration::from_secs(20),
        fade: Duration::from_secs(2),
    }
}

/// Roll step between marks: the golden angle (radians), so marks in a row never line up.
const IMPACT_ROLL_STEP: f32 = 2.399_963_2;

/// Surface colours of the greybox world.
#[derive(Clone, Copy, Debug)]
enum Paint {
    Arena,
    ArenaFloor,
    Dummy,
    Enemy,
    Struck,
    Control,
    ArenaMark,
    ArenaMarkRun,
    Tracer,
    ImpactMark,
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
            Paint::Control => ([0.25, 0.75, 0.35], [0.18, 0.55, 0.26], 0.0, 0.0),
            Paint::ArenaMark => ([0.95, 0.9, 0.2], [0.8, 0.75, 0.15], 0.0, 0.0),
            Paint::ArenaMarkRun => ([0.3, 0.85, 0.35], [0.22, 0.65, 0.26], 0.0, 0.0),
            Paint::Tracer => ([1.0, 0.85, 0.4], [1.0, 0.85, 0.4], 1.0, 0.0),
            Paint::ImpactMark => ([0.05, 0.05, 0.05], [0.05, 0.05, 0.05], 0.0, 0.0),
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

/// One drawn body part of a dummy, lit while it flashes.
#[derive(Debug)]
struct Segment {
    tag: SurfaceTag,
    id: WorldMeshId,
    mesh: WorldMesh,
    flash_left: u32,
    /// World-axis bounds `(min, max)` of the part's hit volume.
    bounds: (Vec3, Vec3),
}

/// The dummies' segments, in the frame the world draws them in.
fn dummy_segments(body: &CharacterBody, spawns: &[Vec3]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (index, &spawn) in spawns.iter().enumerate() {
        let (target, centre) = dummy_at(body, index, spawn);
        for volume in dummy_hit_volumes_of(body, target) {
            segments.push(Segment {
                tag: SurfaceTag::try_from(volume.tag).expect("dummy volumes carry part tags"),
                id: WorldMeshId(DUMMY_MESH_BASE + segments.len() as u64),
                mesh: box_mesh(centre + volume.offset, volume.half_extents),
                flash_left: 0,
                bounds: volume_bounds(centre, 0.0, &volume),
            });
        }
    }
    segments
}

/// The world-axis bounds `(min, max)` of `volume` on a body standing at `position` turned
/// `yaw` about the vertical: the volume's oriented box, axis-aligned.
fn volume_bounds(position: Vec3, yaw: f32, volume: &HitVolume) -> (Vec3, Vec3) {
    let facing = Quat::from_rotation_y(yaw);
    let axes = Mat3::from_quat(facing * volume.rotation);
    let half = axes.x_axis.abs() * volume.half_extents.x
        + axes.y_axis.abs() * volume.half_extents.y
        + axes.z_axis.abs() * volume.half_extents.z;
    let centre = position + facing * volume.offset;
    (centre - half, centre + half)
}

/// Linear RGBA of the outlines of the player's, an enemy's and a dummy's hit volumes.
const PLAYER_OUTLINE: [f32; 4] = [0.2, 1.0, 0.3, 1.0];
const ENEMY_OUTLINE: [f32; 4] = [1.0, 0.2, 0.2, 1.0];
const DUMMY_OUTLINE: [f32; 4] = [1.0, 0.9, 0.2, 1.0];

/// The hit-volume outlines take marker ids counting up from here, renumbered every frame
/// because the markers are cleared before each placement.
const OUTLINE_ID_BASE: u64 = 1 << 32;

/// One enemy's gun flash: its box segments are placed each frame, the gun struck while it flashes.
#[derive(Debug)]
struct EnemyRig {
    target: u16,
    gun_flash: u32,
}

/// The six body parts and the gun.
const ENEMY_SEGMENT_COUNT: usize = 7;

/// The gun's place in [`enemy_segments`].
const GUN_SLOT: usize = ENEMY_SEGMENT_COUNT - 1;

/// The enemy's segments as `(offset, half extents)` in its body frame, gun last.
fn enemy_segments(body: &CharacterBody) -> [(Vec3, Vec3); ENEMY_SEGMENT_COUNT] {
    let volumes = dummy_hit_volumes_of(body, 0);
    let mut segments = [(GUN_OFFSET, GUN_HALF); ENEMY_SEGMENT_COUNT];
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
#[derive(Debug)]
struct Crosshair {
    texture_sent: bool,
    /// Opacity of the four bars in percent; at 0 they are not drawn.
    opacity: i32,
    /// Vertical field of view in degrees of the view the gap is measured against.
    fov_degrees: f32,
    dead: bool,
    /// Opacity of the damage bar on each screen edge, indexed front, right, behind, left.
    edges: [u8; 4],
    /// Whether a menu owns the view's overlay: nothing is drawn.
    hidden: bool,
    /// Half-angle in radians of the cone the next shot leaves in; the gap opens to mark it.
    spread: f32,
}

impl Default for Crosshair {
    fn default() -> Self {
        Self {
            texture_sent: false,
            opacity: GameRows::CROSSHAIR_OPACITY_MAX,
            fov_degrees: FOV_DEGREES,
            dead: false,
            edges: [0; 4],
            hidden: false,
            spread: 0.0,
        }
    }
}

impl Crosshair {
    const TEXTURE: UiTextureId = UiTextureId(0);
    const GAP: f32 = 4.0;
    const LENGTH: f32 = 9.0;
    const THICKNESS: f32 = 2.0;

    /// Pixels from the screen centre to the inner end of each bar, for a view `fov_y` radians
    /// tall drawn `height` pixels high: the resting gap plus the cone's circle on screen, by a
    /// pinhole projection.
    fn gap(spread: f32, fov_y: f32, height: f32) -> f32 {
        Self::GAP + spread.tan() / (fov_y / 2.0).tan() * (height / 2.0)
    }
}

impl UiFrameSource for Crosshair {
    fn ui_frame(&mut self, size_in_pixels: [u32; 2]) -> Option<UiFrame> {
        if self.hidden {
            return None;
        }
        let size = [size_in_pixels[0] as f32, size_in_pixels[1] as f32];
        let (cx, cy) = (size[0] / 2.0, size[1] / 2.0);
        let gap = Self::gap(self.spread, self.fov_degrees.to_radians(), size[1]);
        let (near, far, half) = (gap, gap + Self::LENGTH, Self::THICKNESS / 2.0);
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
        if self.opacity > 0 {
            let alpha = (255.0 * self.opacity as f32 / 100.0).round() as u8;
            for (min, max) in bars {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    min,
                    max,
                    [255, 255, 255, alpha],
                );
            }
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

/// Everything drawn or heard; the sim knows nothing of it.
#[derive(Debug)]
struct Stage {
    scene: Scene,
    /// Drawn instead of `scene` behind the main menu of a level without menu cameras.
    backdrop: Scene,
    segments: Vec<Segment>,
    dummy_material: u32,
    struck_material: u32,
    enemy_material: u32,
    tracer_material: u32,
    impact_material: u32,
    impact_roll: f32,
    /// The streak drawn for each round, by the sim's round id.
    tracers: BTreeMap<u32, EffectHandle>,
    /// The traces of the ticks since the last frame, or of the last tick if a frame has drawn
    /// them.
    traces: Vec<RoundTrace>,
    /// Whether a frame has drawn `traces`.
    traces_shown: bool,
    /// The hull and eye the dummies and enemies are drawn from, the sim's own.
    body: CharacterBody,
    dummy_count: usize,
    enemies: Vec<EnemyRig>,
    /// The numbers of the bodies whose placeholder box is in the scene.
    shown_bodies: BTreeSet<u16>,
    edge_left: [u32; 4],
    crosshair: Arc<Mutex<Crosshair>>,
    /// Vertical field of view in degrees of every scene drawn.
    fov_degrees: f32,
    shot_sound: Option<PathBuf>,
    window: Option<Arc<Window>>,
    status: String,
    title: String,
    shell: Arc<Mutex<UiShell<ConsoleCommand, MenuAction>>>,
    /// Whether the cursor is captured for mouse look.
    cursor_captured: bool,
    /// Hit-volume outlines are in the scene.
    outlines_placed: bool,
    /// Times the cursor was captured, windowed or not, so tests can see it.
    #[cfg(test)]
    cursor_captures: u32,
}

impl Stage {
    /// Draws every scene at `fov` degrees (vertical) and sizes the crosshair's gap for it, and
    /// gives the crosshair `opacity` percent.
    fn set_view_rows(&mut self, fov: i32, opacity: i32) {
        self.fov_degrees = fov as f32;
        let mut crosshair = self
            .crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        crosshair.fov_degrees = self.fov_degrees;
        crosshair.opacity = opacity;
    }

    fn set_effects_quality(&mut self, quality: u32) {
        self.scene.effects_mut().set_quality(quality);
    }

    /// Takes in the traces of one tick. Those of ticks no frame has drawn yet merge by round id,
    /// the later tick winning, so a round that stopped between two frames keeps its end; the
    /// first tick after a frame starts afresh.
    fn absorb_traces(&mut self, traces: &[RoundTrace]) {
        if std::mem::take(&mut self.traces_shown) {
            self.traces.clear();
        }
        for trace in traces {
            match self.traces.iter_mut().find(|held| held.id == trace.id) {
                Some(held) => *held = *trace,
                None => self.traces.push(*trace),
            }
        }
    }

    /// Ends every tracer and forgets the traces, for a sim whose rounds are gone or frozen.
    fn end_tracers(&mut self) {
        let effects = self.scene.effects_mut();
        for handle in std::mem::take(&mut self.tracers).into_values() {
            effects.end(handle);
        }
        self.traces.clear();
        self.traces_shown = false;
    }

    /// Draws one streak per absorbed trace, placed `alpha` of the way along its step, and ends
    /// the streak of each round that stopped or is no longer traced. An ended trace stays held
    /// until the next tick, so its streak is kept, ended, until it goes.
    fn show_tracers(&mut self, alpha: f32) {
        let style = tracer_style(self.tracer_material);
        let effects = self.scene.effects_mut();
        let traces = &self.traces;
        self.traces_shown = true;
        for trace in traces {
            let handle = *self
                .tracers
                .entry(trace.id)
                .or_insert_with(|| effects.begin_streak(style));
            let at = if trace.ended {
                trace.to
            } else {
                trace.from.lerp(trace.to, alpha)
            };
            effects.place_streak(handle, at, trace.heading);
            if trace.ended {
                effects.end(handle);
            }
        }
        self.tracers.retain(|id, handle| {
            let traced = traces.iter().any(|trace| trace.id == *id);
            if !traced {
                effects.end(*handle);
            }
            traced
        });
    }

    fn new() -> Self {
        let mut scene = Scene::new();
        let dummy_material = scene.material_table.find_or_push(&Paint::Dummy);
        let struck_material = scene.material_table.find_or_push(&Paint::Struck);
        let enemy_material = scene.material_table.find_or_push(&Paint::Enemy);
        let tracer_material = scene.material_table.find_or_push(&Paint::Tracer);
        let impact_material = scene.material_table.find_or_push(&Paint::ImpactMark);
        scene
            .world_meshes_mut()
            .upsert_movable(ENEMY_BOX_MESH_ID, box_mesh(Vec3::ZERO, Vec3::splat(0.5)));
        Self {
            scene,
            backdrop: Scene::new(),
            segments: Vec::new(),
            dummy_material,
            struck_material,
            enemy_material,
            tracer_material,
            impact_material,
            impact_roll: 0.0,
            tracers: BTreeMap::new(),
            traces: Vec::new(),
            traces_shown: false,
            body: SHIPPED_BODY,
            dummy_count: 0,
            enemies: Vec::new(),
            shown_bodies: BTreeSet::new(),
            edge_left: [0; 4],
            crosshair: Arc::new(Mutex::new(Crosshair::default())),
            fov_degrees: FOV_DEGREES,
            shot_sound: None,
            window: None,
            status: String::new(),
            title: String::new(),
            shell: Arc::new(Mutex::new(UiShell::new(
                console_commands::console(&BTreeMap::new()).0,
            ))),
            cursor_captured: false,
            outlines_placed: false,
            #[cfg(test)]
            cursor_captures: 0,
        }
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
    fn shell(&self) -> MutexGuard<'_, UiShell<ConsoleCommand, MenuAction>> {
        self.shell.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn add_arena(&mut self, meshes: Vec<WorldMesh>, dummy_spawns: &[Vec3]) {
        self.add_named_arena(meshes, &[], dummy_spawns);
    }

    /// Draws `meshes` as the arena, the one named `floor` in the glossy [`Paint::ArenaFloor`],
    /// those named `mark` in [`Paint::ArenaMark`], those named `mark_run` in
    /// [`Paint::ArenaMarkRun`] and the rest, or any past the end of `material_names`, in
    /// [`Paint::Arena`].
    fn add_named_arena(
        &mut self,
        meshes: Vec<WorldMesh>,
        material_names: &[Option<String>],
        dummy_spawns: &[Vec3],
    ) {
        let arena_material = self.scene.material_table.find_or_push(&Paint::Arena);
        let floor_material = self.scene.material_table.find_or_push(&Paint::ArenaFloor);
        let mark_material = self.scene.material_table.find_or_push(&Paint::ArenaMark);
        let mark_run_material = self.scene.material_table.find_or_push(&Paint::ArenaMarkRun);
        for (index, mesh) in meshes.into_iter().enumerate() {
            let material = match material_names.get(index).and_then(Option::as_deref) {
                Some(FLOOR_MATERIAL) => floor_material,
                Some(MARK_MATERIAL) => mark_material,
                Some(MARK_RUN_MATERIAL) => mark_run_material,
                _ => arena_material,
            };
            self.scene.world_meshes_mut().upsert(
                crate::arena::arena_mesh_id(index),
                mesh,
                material,
            );
        }
        self.dummy_count = dummy_spawns.len();
        self.segments = dummy_segments(&self.body, dummy_spawns);
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
            Feedback::Fired => self.play_shot(feedback, audio),
            Feedback::EnemyFired { target, .. } => {
                self.enemy_rig(*target).gun_flash = FLASH_TICKS;
                self.play_shot(feedback, audio);
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
            Feedback::Impact { point, normal } => {
                let style = impact_style(self.impact_material);
                self.scene
                    .effects_mut()
                    .mark(*point, *normal, self.impact_roll, style);
                self.impact_roll = (self.impact_roll + IMPACT_ROLL_STEP) % std::f32::consts::TAU;
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

    /// Places every living enemy's segments where the sim has it for this frame; a dead or
    /// missing enemy is not placed.
    fn sync_enemies(&mut self, sim: &SearchlightSim) {
        let segments = enemy_segments(sim.character_body());
        let known = self.dummy_count + sim.enemy_count();
        for rig in &mut self.enemies {
            if usize::from(rig.target) >= known {
                rig.gun_flash = 0;
            }
        }
        for index in 0..sim.enemy_count() {
            let target = u16::try_from(self.dummy_count + index)
                .expect("the sim numbers its targets with a u16");
            let rig = self.enemy_rig(target);
            let Some(state) = sim.enemy_state(index) else {
                // The slot may be refilled by a new enemy, which must not inherit the flash.
                rig.gun_flash = 0;
                continue;
            };
            let gun_flash = rig.gun_flash;
            let facing = Quat::from_rotation_y(state.yaw);
            let meshes = self.scene.world_meshes_mut();
            for (slot, (offset, half)) in segments.into_iter().enumerate() {
                let material = if slot == GUN_SLOT && gun_flash > 0 {
                    self.struck_material
                } else {
                    self.enemy_material
                };
                let transform = Affine3A::from_scale_rotation_translation(
                    half * 2.0,
                    facing,
                    state.position + facing * offset,
                );
                meshes.place(ENEMY_BOX_MESH_ID, transform, material);
            }
        }
    }

    /// Draws each body the sim holds as one box lying at its point in the enemy material, and
    /// takes out the boxes of bodies a restart cleared.
    fn sync_bodies(&mut self, sim: &SearchlightSim) {
        let meshes = self.scene.world_meshes_mut();
        self.shown_bodies.retain(|number| {
            let kept = sim.bodies().contains_key(number);
            if !kept {
                meshes.remove(body_mesh_id(*number));
            }
            kept
        });
        for (&number, body) in sim.bodies() {
            if self.shown_bodies.insert(number) {
                meshes.upsert(
                    body_mesh_id(number),
                    box_mesh(body.point + Vec3::Y * BODY_BOX_HALF.y, BODY_BOX_HALF),
                    self.enemy_material,
                );
            }
        }
    }

    /// With `shown`, outlines every hit volume of the player and of every living enemy and
    /// dummy where the sim has them; otherwise removes the outlines.
    fn sync_hit_volumes(&mut self, sim: &SearchlightSim, shown: bool) {
        if !shown {
            if std::mem::take(&mut self.outlines_placed) {
                self.scene.markers_mut().clear();
            }
            return;
        }
        self.outlines_placed = true;
        let enemy_volumes = dummy_hit_volumes_of(sim.character_body(), 0);
        let player = sim.player();
        let players = sim.player_hit_volumes().iter().map(|volume| {
            let bounds = volume_bounds(player.position, player.yaw, volume);
            (bounds, PLAYER_OUTLINE)
        });
        let enemies = (0..sim.enemy_count())
            .filter_map(|index| sim.enemy_state(index))
            .flat_map(|state| {
                enemy_volumes.iter().map(move |volume| {
                    let bounds = volume_bounds(state.position, state.yaw, volume);
                    (bounds, ENEMY_OUTLINE)
                })
            });
        let dummies = self
            .segments
            .iter()
            .map(|segment| (segment.bounds, DUMMY_OUTLINE));
        let markers = self.scene.markers_mut();
        markers.clear();
        for (next, (bounds, color)) in
            (OUTLINE_ID_BASE..).zip(players.chain(enemies).chain(dummies))
        {
            let outline = Marker::new(bounds.0.to_array(), bounds.1.to_array(), color)
                .with_style(MarkerStyle::Outline);
            markers.upsert(MarkerId(next), outline);
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

    /// Hands the crosshair the cone its next shot leaves in, so the gap can mark it.
    fn show_spread(&self, spread: f32) {
        self.crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .spread = spread;
    }

    fn hide_hud(&self) {
        self.crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .hidden = true;
    }

    /// The rifle sound for a shot: the player's own from no position, an enemy's from where
    /// it fired. `None` for any other feedback or when no shot file is configured.
    fn shot_sound(&self, feedback: &Feedback) -> Option<moho_audio::AudioSource> {
        let path = self.shot_sound.as_ref()?;
        let source = moho_audio::AudioSource::new(path).with_volume(SHOT_VOLUME);
        match feedback {
            Feedback::Fired => Some(source),
            Feedback::EnemyFired { from, .. } => Some(source.with_position(from.to_array())),
            _ => None,
        }
    }

    fn play_shot(&self, feedback: &Feedback, audio: Option<&mut AudioSystem>) {
        let (Some(audio), Some(source)) = (audio, self.shot_sound(feedback)) else {
            return;
        };
        if let Err(error) = audio.play_audio_source(&source) {
            tracing::warn!(%error, path = source.path(), "rifle shot could not be played");
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
        self.draw_at(renderer, eye, self.fov_degrees);
    }

    /// Draws the arena from `eye` with a view `fov_degrees` tall.
    fn draw_at(&mut self, renderer: &mut dyn RendererBackend, eye: (Mat4, Vec3), fov_degrees: f32) {
        let aspect = self.aspect();
        draw_scene(&mut self.scene, renderer, eye, (fov_degrees, aspect));
    }

    fn draw_backdrop(&mut self, renderer: &mut dyn RendererBackend) {
        let aspect = self.aspect();
        draw_scene(
            &mut self.backdrop,
            renderer,
            (Mat4::IDENTITY, Vec3::ZERO),
            (self.fov_degrees, aspect),
        );
    }
}

/// Draws `scene` from `eye` with a view `fov_degrees` tall and `aspect` wide per tall.
fn draw_scene(
    scene: &mut Scene,
    renderer: &mut dyn RendererBackend,
    eye: (Mat4, Vec3),
    (fov_degrees, aspect): (f32, f32),
) {
    renderer.update_lighting(lighting());
    let (view, eye) = eye;
    let projection =
        glam::camera::rh::proj::directx::perspective(fov_degrees.to_radians(), aspect, NEAR, FAR);
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
    /// The inventory: takes input like a menu while the loop keeps running.
    Inventory,
    /// The character screen: the player's stats and what contributes to them; takes input like
    /// a menu while the loop keeps running.
    Character,
}

impl Screen {
    /// Whether this is a screen over play that takes input like a menu while the loop keeps
    /// running: the inventory or the character screen.
    fn is_overlay(self) -> bool {
        matches!(self, Self::Inventory | Self::Character)
    }
}

/// The mode the window opens in for a saved `mode`: both fullscreen kinds open borderless.
pub fn start_window_mode(mode: moho_core::prefs::WindowMode) -> WindowMode {
    moho_ui_shell::runtime_window_mode(mode)
}

/// The FPS prototype's game: one level's rules and the input that drives it.
#[derive(Debug)]
pub struct SearchlightGame {
    screen: Screen,
    mode: ArenaMode,
    input: ActionMap<SearchlightAction>,
    /// The player as the tick before the last left them, for blending the view.
    previous: CharacterState,
    /// The player's lean as the tick before the last left it, blended like `previous`.
    previous_lean: Lean,
    /// The side the last command held a lean toward, for the most-recent-press rule.
    lean_side: Option<LeanSide>,
    /// Whether the window has focus; look is sampled only while it does.
    focused: bool,
    stage: Stage,
    /// The views the main menu cuts between behind itself.
    menu_feed: MenuFeed,
    god_mode: bool,
    /// The grade the `grade` command forces on new enemies; not saved, a force is a tuning
    /// session's lever. A host action like `god_mode`: once commands are per player slot, only
    /// slot 0's is read.
    forced_grade: Option<Grade>,
    /// Simple is on: shots are hitscan. Saved with the debug settings. A host action like
    /// `god_mode`.
    simple: bool,
    /// How hard gravity pulls on rounds; session state, standard at every launch. A host action
    /// like `god_mode`.
    round_gravity: GravityScale,
    /// How much the view bobs while moving. Saved with the game's settings.
    head_bob: HeadBobLevel,
    /// Aim was held as of the last tick: view state, read from the tick's frame once per tick.
    aiming: bool,
    /// The scale the pending look was sampled under: 1 unless `aiming` as of the last tick.
    /// Kept while the console drops `aiming`, and fixed when sampled so a later zoom change
    /// cannot rescale a look already drawn.
    look_scale: f32,
    /// How much a held Aim magnifies the view. Saved with the debug settings.
    zoom: Magnification,
    /// The view bob after the last tick, and the tick before it for blending.
    bob: Bob,
    previous_bob: Bob,
    /// The listener of the last frame, read from the view that frame draws.
    listener: moho_audio::Listener,
    debug_settings: Option<DebugSettings>,
    /// A `fullscreen` command waiting for a frame, the first place a window mode can be
    /// requested.
    flip_window_pending: bool,
    /// Engine settings waiting for a frame, the first place the renderer, audio and window can
    /// be reached.
    pending_settings: Option<EngineSettings>,
    /// The move the inventory screen asked for since the last tick, sent with the next command.
    queued_inventory: Option<InventoryCommand>,
    /// The bindings being edited while the Controls page is open.
    controls: Option<ControlsDraft>,
    /// Ticks the rig strip stays up after the last trigger.
    strip_ticks_left: u32,
    /// The check-gear key was held on the last tick, which keeps the strip up.
    check_gear_held: bool,
    /// Outlines of every character's hit volumes are drawn; session state, off at every launch.
    hit_volumes: bool,
}

impl SearchlightGame {
    /// A game ticking `mode` with commands sampled from `input`.
    pub fn new(mode: ArenaMode, input: ActionMap<SearchlightAction>) -> Self {
        let previous = mode.sim().player();
        Self {
            screen: Screen::Playing,
            mode,
            input,
            previous,
            previous_lean: Lean::default(),
            lean_side: None,
            focused: true,
            stage: Stage::new(),
            menu_feed: MenuFeed::new(Vec::new()),
            god_mode: false,
            forced_grade: None,
            simple: false,
            round_gravity: GravityScale::default(),
            head_bob: HeadBobLevel::DEFAULT,
            aiming: false,
            look_scale: 1.0,
            zoom: Magnification::DEFAULT,
            bob: Bob::REST,
            previous_bob: Bob::REST,
            listener: moho_audio::Listener::default(),
            debug_settings: None,
            flip_window_pending: false,
            pending_settings: None,
            queued_inventory: None,
            controls: None,
            strip_ticks_left: 0,
            check_gear_held: false,
            hit_volumes: false,
        }
    }

    #[must_use]
    pub fn with_main_menu(mut self) -> Self {
        self.screen = Screen::MainMenu;
        self.stage.shell().menu_mut().push(main_screen());
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

    fn enter_play(&mut self) {
        self.screen = Screen::Playing;
        self.stage.shell().menu_mut().clear();
        // Whatever was pressed or moved over the menu must not act in the arena.
        self.release_held_input();
        self.input.discard_edges();
        self.input.reset_look();
        self.stage.capture_cursor();
    }

    /// Lets go of every held input and the check-gear hold that mirrors it.
    fn release_held_input(&mut self) {
        self.input.release_all();
        self.check_gear_held = false;
    }

    fn pause(&mut self) {
        self.screen = Screen::Paused;
        self.stage.shell().menu_mut().push(pause_screen());
        self.hand_input_to_screen();
    }

    /// Whether `input` is bound to `action`.
    fn is_bound(&self, action: SearchlightAction, input: Input) -> bool {
        self.input
            .bindings()
            .get(action)
            .iter()
            .any(|binding| binding.input == input)
    }

    /// Whether the loop stands still: on the main menu or the pause menu, or with the console
    /// open. The inventory and character screens take input but leave the world running.
    fn loop_paused(&self) -> bool {
        !(self.screen == Screen::Playing || self.screen.is_overlay())
            || self.stage.shell().console_mut().is_open()
    }

    /// The lists the inventory screen shows: the loot screen while a body is open, otherwise
    /// the gear screen.
    fn inventory_lists(&self) -> ItemLists<MenuAction> {
        let sim = self.mode.sim();
        match sim.looted_remains() {
            Some(remains) => inventory::loot_lists(sim.loadout(), remains),
            None => inventory::gear_lists(sim.loadout()),
        }
    }

    /// The colour of the HUD's load icon, from the sim's load state.
    pub fn load_icon(&self) -> hud::LoadIcon {
        hud::load_icon(self.mode.sim().carry_sheet().state)
    }

    /// The load line the open inventory screen shows; `None` while it is closed.
    pub fn inventory_header(&self) -> Option<inventory::LoadHeader> {
        (self.screen == Screen::Inventory).then(|| header_of(&self.mode.sim().carry_sheet()))
    }

    /// Opens the character screen over play, the world still running behind it.
    fn open_character(&mut self) {
        self.screen = Screen::Character;
        let lists = character::character_lists(self.mode.sim());
        self.stage.shell().menu_mut().push_lists(lists);
        self.hand_input_to_screen();
    }

    /// `command` with the host's levels and the inventory command the screen queued, the parts
    /// of a tick's command that do not come from the player's hands.
    fn with_host_levels(&mut self, command: SearchlightCommand) -> SearchlightCommand {
        SearchlightCommand {
            god_mode: self.god_mode,
            forced_grade: self.forced_grade,
            simple: self.simple,
            round_gravity: self.round_gravity,
            inventory: self.queued_inventory.take(),
            ..command
        }
    }

    /// Opens the inventory screen over play, the world still running behind it.
    fn open_inventory(&mut self) {
        self.screen = Screen::Inventory;
        let lists = self.inventory_lists();
        self.stage.shell().menu_mut().push_lists(lists);
        self.hand_input_to_screen();
    }

    /// Closes the inventory screen and tells the sim, which ends the looting.
    fn close_inventory(&mut self) {
        if self.queued_inventory.is_some() {
            tracing::debug!("inventory move dropped by a close in the same tick");
        }
        self.queued_inventory = Some(InventoryCommand::Close);
        self.enter_play();
    }

    /// Drops what play held or looked at and frees the cursor for a screen to use.
    fn hand_input_to_screen(&mut self) {
        self.release_held_input();
        self.input.discard_edges();
        self.input.reset_look();
        self.stage.release_cursor();
    }

    /// Queues the move of row `row` of list `from` onto list `to` for the next tick. One move is
    /// sent per tick: a second request before it is sent is dropped, as is one that names worn
    /// equipment or a row that is gone.
    fn queue_move(&mut self, row: u32, from: ListId, to: ListId) {
        let sim = self.mode.sim();
        let row = usize::try_from(row).expect("a row index fits usize");
        let Some(source) = inventory::source_of(from, row, sim.loadout(), sim.looted_remains())
        else {
            tracing::debug!(
                list = from.0,
                row,
                "inventory move dropped: nothing movable there"
            );
            return;
        };
        let Some(place) = inventory::place_of(to) else {
            tracing::debug!(
                list = to.0,
                "inventory move dropped: nothing drops on that list"
            );
            return;
        };
        if let Some(pending) = self.queued_inventory {
            tracing::debug!(
                ?pending,
                ?source,
                ?place,
                "inventory move dropped: one is already queued"
            );
            return;
        }
        self.queued_inventory = Some(InventoryCommand::Move {
            from: source,
            to: place,
        });
    }

    /// Answers what the player chose on the menu screens since the last call.
    fn answer_menu(&mut self, ctx: &mut EventContext<'_>) {
        let events = self.stage.shell().menu_mut().take_events();
        for event in events {
            match event {
                MenuEvent::Chosen(MenuAction::Play | MenuAction::Resume) => self.enter_play(),
                MenuEvent::Chosen(MenuAction::Settings) => {
                    self.stage
                        .shell()
                        .menu_mut()
                        .push(settings_categories_screen());
                }
                MenuEvent::Chosen(MenuAction::Page(page)) => {
                    let settings = self.debug_settings.as_ref().map_or_else(
                        || EngineSettings::from_prefs(&Prefs::default()),
                        |saved| EngineSettings::from_prefs(saved.prefs()),
                    );
                    let rows = self.game_rows();
                    self.stage
                        .shell()
                        .menu_mut()
                        .push(settings_page(page, settings, &rows));
                }
                MenuEvent::Chosen(MenuAction::Controls) => {
                    let draft = ControlsDraft::new(self.input.bindings());
                    self.stage.shell().menu_mut().push(controls_screen(&draft));
                    self.controls = Some(draft);
                }
                MenuEvent::Chosen(MenuAction::ApplyControls) => self.apply_controls(),
                MenuEvent::Chosen(MenuAction::ResetControls) => {
                    if let Some(draft) = &mut self.controls {
                        draft.reset_all();
                    }
                    self.show_controls();
                }
                MenuEvent::Chosen(MenuAction::Rebind(_)) => {}
                MenuEvent::Chosen(MenuAction::Back) if self.screen == Screen::Inventory => {
                    self.close_inventory();
                }
                MenuEvent::Chosen(MenuAction::Back) if self.screen == Screen::Character => {
                    self.enter_play();
                }
                MenuEvent::Chosen(MenuAction::Back) => {
                    self.controls = None;
                    self.stage.shell().menu_mut().pop();
                }
                MenuEvent::Chosen(MenuAction::Replace) => self.answer_conflict(true),
                MenuEvent::Chosen(MenuAction::Cancel) => self.answer_conflict(false),
                MenuEvent::Chosen(MenuAction::Quit) => ctx.request_exit(),
                MenuEvent::SettingsApplied(settings) => self.settings_applied(settings),
                MenuEvent::GameSettingsApplied(values) => self.game_settings_applied(&values),
                MenuEvent::MoveRequested {
                    item: MenuAction::Item(row),
                    from,
                    to,
                } => self.queue_move(row, from, to),
                MenuEvent::Captured {
                    action: MenuAction::Rebind(action),
                    slot,
                    input,
                } => self.captured(action, slot, input),
                MenuEvent::Cleared {
                    action: MenuAction::Rebind(action),
                    slot,
                } => {
                    if let Some(draft) = &mut self.controls {
                        draft.clear(action, slot);
                    }
                    self.show_controls();
                }
                MenuEvent::Captured { .. } | MenuEvent::Cleared { .. } => self.show_controls(),
                MenuEvent::DetailsToggled { .. }
                | MenuEvent::Picked { .. }
                | MenuEvent::ModeChosen { .. } => {}
                MenuEvent::ActionPicked {
                    action: MenuAction::ItemAction(action),
                    ..
                } => match action {},
                MenuEvent::MoveRequested { .. }
                | MenuEvent::ActionPicked { .. }
                | MenuEvent::Chosen(MenuAction::Item(_) | MenuAction::ItemAction(_)) => {}
            }
        }
    }

    /// Shows the Controls page's rows from the draft.
    fn show_controls(&mut self) {
        if let Some(draft) = &self.controls {
            let rows = controls_rows(draft);
            self.stage.shell().menu_mut().set_items(rows);
        }
    }

    /// Stages a captured `input` for `action`'s `slot` on the Controls page, or asks about the
    /// action that holds it.
    fn captured(&mut self, action: SearchlightAction, slot: Slot, input: Input) {
        let Some(draft) = &mut self.controls else {
            return;
        };
        if let Capture::Conflict { by, .. } = draft.capture(action, slot, input) {
            let dialog = conflict_screen(&Binding::from(input).label(), label(by));
            self.stage.shell().menu_mut().push(dialog);
            return;
        }
        self.show_controls();
    }

    /// Closes the conflict dialog, answers it through the draft and refreshes the Controls rows.
    fn answer_conflict(&mut self, replace: bool) {
        if let Some(draft) = &mut self.controls {
            if replace {
                draft.replace();
            } else {
                draft.cancel();
            }
        }
        self.stage.shell().menu_mut().pop();
        self.show_controls();
    }

    /// Makes the drafted bindings live, replacing the action map, and saves them; a failed save
    /// warns and the bindings stay live for the session.
    fn apply_controls(&mut self) {
        let Some(draft) = &self.controls else {
            return;
        };
        let bindings = draft.bindings();
        let prefs = self
            .debug_settings
            .as_ref()
            .map_or_else(Prefs::default, |saved| saved.prefs().clone());
        if let Some(saved) = &mut self.debug_settings
            && let Err(error) = saved.set_bindings(&bindings)
        {
            tracing::warn!(%error, "bindings not saved");
        }
        self.input = build_action_map(bindings, &prefs);
    }

    /// The game rows as the game holds them now.
    fn game_rows(&self) -> GameRows {
        let (fov, opacity) = {
            let crosshair = self
                .stage
                .crosshair
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            (self.stage.fov_degrees as i32, crosshair.opacity)
        };
        GameRows {
            fov,
            crosshair_opacity: opacity,
            head_bob: self.head_bob,
        }
    }

    /// Makes `rows` live.
    fn apply_game_rows(&mut self, rows: GameRows) {
        self.head_bob = rows.head_bob;
        self.stage.set_view_rows(rows.fov, rows.crosshair_opacity);
    }

    /// Applies and saves the game rows a page sent. A row the page does not carry keeps the
    /// value the game holds; a failed save warns and the values stay for the session.
    fn game_settings_applied(&mut self, values: &BTreeMap<String, i32>) {
        let mut rows = self.game_rows();
        if let Some(&fov) = values.get(GameRows::FOV_KEY) {
            rows.fov = fov.clamp(GameRows::FOV_MIN, GameRows::FOV_MAX);
        }
        if let Some(&opacity) = values.get(GameRows::CROSSHAIR_OPACITY_KEY) {
            rows.crosshair_opacity = opacity.clamp(
                GameRows::CROSSHAIR_OPACITY_MIN,
                GameRows::CROSSHAIR_OPACITY_MAX,
            );
        }
        if let Some(&percent) = values.get(HeadBobLevel::KEY) {
            rows.head_bob = HeadBobLevel::nearest(percent);
        }
        self.apply_game_rows(rows);
        if let Some(saved) = &mut self.debug_settings
            && let Err(error) = saved.set_game_rows(rows)
        {
            tracing::warn!(%error, "game settings not saved");
        }
    }

    /// Saves the head bob; a failed save warns and the value stays for the session.
    fn save_head_bob(&mut self) {
        if let Some(saved) = &mut self.debug_settings
            && let Err(error) = saved.set_head_bob(self.head_bob)
        {
            tracing::warn!(%error, head_bob = self.head_bob.percent(), "head bob not saved");
        }
    }

    fn save_zoom(&mut self) {
        if let Some(saved) = &mut self.debug_settings
            && let Err(error) = saved.set_zoom(self.zoom)
        {
            tracing::warn!(%error, zoom = self.zoom.get(), "zoom not saved");
        }
    }

    /// The scale of the pending look, by the aim state it was sampled under: 1 unzoomed.
    fn aim_look_scale(&self) -> f32 {
        self.look_scale
    }

    /// The vertical field of view the play view draws with, in degrees.
    fn play_fov_degrees(&self) -> f32 {
        if self.aiming {
            aimed_fov_y(self.stage.fov_degrees.to_radians(), self.zoom).to_degrees()
        } else {
            self.stage.fov_degrees
        }
    }

    /// Saves the applied `settings`, sets the look from them and queues them for the next frame,
    /// which hands the renderer, audio and window theirs.
    fn settings_applied(&mut self, settings: EngineSettings) {
        if let Some(saved) = &mut self.debug_settings
            && let Err(error) = saved.set_engine_settings(settings)
        {
            tracing::warn!(%error, "settings not saved");
        }
        self.input.set_look(look_from(
            settings.mouse_sensitivity,
            settings.mouse_smoothing,
            settings.invert_mouse_y,
        ));
        self.pending_settings = Some(settings);
    }

    /// Saves `applied`, whose shadow quality the renderer lowered because the GPU could not
    /// allocate the asked level, so the next start does not ask for it again.
    fn save_fallen_back_settings(&mut self, applied: EngineSettings) {
        if let Some(saved) = &mut self.debug_settings
            && let Err(error) = saved.set_engine_settings(applied)
        {
            tracing::warn!(%error, "lowered shadow quality not saved");
        }
    }

    /// Starts in the god mode `settings` holds and saves every change to it. The engine settings
    /// it holds apply on the first frame.
    #[must_use]
    pub fn with_debug_settings(mut self, settings: DebugSettings) -> Self {
        self.god_mode = settings.god_mode();
        self.simple = settings.simple();
        self.zoom = settings.zoom();
        self.apply_game_rows(settings.game_rows());
        self.pending_settings = Some(EngineSettings::from_prefs(settings.prefs()));
        self.debug_settings = Some(settings);
        self
    }

    /// Hands the renderer the Brightness the debug settings' prefs hold.
    fn apply_saved_brightness(&self, renderer: &mut dyn RendererBackend) {
        if let Some(settings) = &self.debug_settings {
            renderer.set_brightness(settings.prefs().brightness());
        }
    }

    fn apply_saved_bloom(&self, renderer: &mut dyn RendererBackend) {
        if let Some(settings) = &self.debug_settings {
            renderer.set_bloom_quality(settings.prefs().bloom() as u8);
        }
    }

    /// Applies the console's queued commands in order, saves god mode once if they changed it,
    /// and reports the value after each bare `tgm`.
    fn apply_console_commands(&mut self) {
        let queued = self.stage.shell().console_mut().take_commands();
        if queued.is_empty() {
            return;
        }
        let before = self.god_mode;
        let simple_before = self.simple;
        for command in queued {
            match command {
                ConsoleCommand::GodMode(GodModeArg::Toggle) => {
                    self.god_mode = !self.god_mode;
                    let text = if self.god_mode {
                        "God mode: on"
                    } else {
                        "God mode: off"
                    };
                    self.stage.shell().console_mut().print(text);
                }
                ConsoleCommand::GodMode(GodModeArg::Set(on)) => self.god_mode = on,
                ConsoleCommand::Fullscreen => self.flip_window_pending = true,
                ConsoleCommand::ForceGrade(grade) => self.forced_grade = grade,
                ConsoleCommand::Simple(SimpleArg::Toggle) => {
                    self.simple = !self.simple;
                    let text = if self.simple {
                        "Simple: on"
                    } else {
                        "Simple: off"
                    };
                    self.stage.shell().console_mut().print(text);
                }
                ConsoleCommand::Simple(SimpleArg::Set(on)) => self.simple = on,
                ConsoleCommand::RoundGravity(scale) => self.round_gravity = scale,
                ConsoleCommand::HeadBob(HeadBobArg::Show) => {
                    let text = format!("Head bob: {}", self.head_bob);
                    self.stage.shell().console_mut().print(&text);
                }
                ConsoleCommand::HeadBob(HeadBobArg::Set(level)) => {
                    self.head_bob = level;
                    self.save_head_bob();
                }
                ConsoleCommand::Zoom(zoom) => {
                    self.zoom = zoom;
                    self.save_zoom();
                }
                ConsoleCommand::HitVolumes => {
                    self.hit_volumes = !self.hit_volumes;
                    let text = if self.hit_volumes {
                        "Hit volumes: on"
                    } else {
                        "Hit volumes: off"
                    };
                    self.stage.shell().console_mut().print(text);
                }
            }
        }
        if self.simple != simple_before
            && let Some(settings) = &mut self.debug_settings
            && let Err(error) = settings.set_simple(self.simple)
        {
            tracing::warn!(%error, simple = self.simple, "simple not saved");
        }
        if self.god_mode != before
            && let Some(settings) = &mut self.debug_settings
            && let Err(error) = settings.set_god_mode(self.god_mode)
        {
            tracing::warn!(%error, god_mode = self.god_mode, "god mode not saved");
        }
    }

    /// Saves `mode`, releases held keys and re-grabs the cursor after the window changed mode.
    fn window_mode_applied(&mut self, mode: WindowMode) {
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

    /// Draws `meshes` as the arena and one dummy at each of `dummy_spawns`.
    #[must_use]
    pub fn with_arena(mut self, meshes: Vec<WorldMesh>, dummy_spawns: &[Vec3]) -> Self {
        self.stage.body = *self.mode.sim().character_body();
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
        self.stage.body = *self.mode.sim().character_body();
        self.stage
            .add_named_arena(meshes, material_names, dummy_spawns);
        self
    }

    /// Shows the main menu over the level from `cameras`, cutting between them.
    #[must_use]
    pub fn with_menu_cameras(mut self, cameras: Vec<MenuCamera>) -> Self {
        self.menu_feed = MenuFeed::new(cameras);
        self
    }

    /// Draws `meshes` as the arena's control posts, the `i`th being control `i`.
    #[must_use]
    pub fn with_controls(mut self, meshes: impl IntoIterator<Item = WorldMesh>) -> Self {
        let material = self
            .stage
            .scene
            .material_table
            .find_or_push(&Paint::Control);
        for (index, mesh) in meshes.into_iter().enumerate() {
            self.stage
                .scene
                .world_meshes_mut()
                .upsert(control_mesh_id(index), mesh, material);
        }
        self
    }

    /// Shows the game's screen text: the prompt of the control in reach below the crosshair, and
    /// the rig strip while it is up. Only while playing and alive; otherwise shows none.
    fn refresh_screen_texts(&mut self, dead: bool) {
        /// The use prompt that shows below the crosshair.
        enum Prompt {
            Use(ControlAction),
            Search { searching: bool },
        }

        let sim = self.mode.sim();
        let live = self.screen == Screen::Playing && !dead;
        let searching = live && sim.searching().is_some();
        let prompt = match live.then(|| sim.use_target()).flatten() {
            Some(UseTarget::Control(index)) => self.mode.control_action(index).map(Prompt::Use),
            Some(UseTarget::Body(_)) => Some(Prompt::Search { searching }),
            None => searching.then_some(Prompt::Search { searching: true }),
        };
        let mut texts: Vec<ScreenText> = prompt
            .map(|prompt| {
                let key = self
                    .input
                    .bindings()
                    .get(SearchlightAction::Use)
                    .first()
                    .map_or_else(|| "unbound".to_owned(), |binding| binding.label());
                match prompt {
                    Prompt::Use(action) => hud::use_prompt(&key, action),
                    Prompt::Search { searching } => hud::search_prompt(&key, searching),
                }
            })
            .map(|text| ScreenText {
                text,
                anchor: Anchor::Center,
                offset: [0.0, PROMPT_OFFSET_Y],
                size: PROMPT_SIZE,
                color: PROMPT_COLOR,
            })
            .into_iter()
            .collect();
        let sheet = (live || self.screen == Screen::Inventory).then(|| sim.carry_sheet());
        if let Some(sheet) = sheet.as_ref().filter(|_| live) {
            texts.push(ScreenText {
                text: "LOAD".to_owned(),
                anchor: Anchor::BottomLeft,
                offset: LOAD_ICON_OFFSET,
                size: STRIP_SIZE,
                color: load_icon_color(hud::load_icon(sheet.state)),
            });
        }
        let header = sheet
            .as_ref()
            .filter(|_| self.screen == Screen::Inventory && !dead)
            .map(header_of);
        if let Some(header) = header {
            let [slowed, no_sprint, cannot_move] = header.marks;
            texts.push(ScreenText {
                text: format!(
                    "{}  (slowed {slowed:.1}, no sprint {no_sprint:.1}, cannot move {cannot_move:.1})",
                    header.text
                ),
                anchor: Anchor::Top,
                offset: [0.0, LOAD_HEADER_OFFSET_Y],
                size: PROMPT_SIZE,
                color: PROMPT_COLOR,
            });
        }
        if let Some(marks) = self.strip_marks().filter(|_| !dead) {
            texts.push(ScreenText {
                text: hud::strip_text(&marks),
                anchor: Anchor::Bottom,
                offset: [0.0, STRIP_OFFSET_Y],
                size: STRIP_SIZE,
                color: STRIP_COLOR,
            });
        }
        if let Err(error) = self.stage.shell().set_texts(texts) {
            tracing::warn!(%error, "screen text not shown");
        }
    }

    /// The marks the rig strip shows, or `None` while it is hidden: shown for the fade ticks
    /// after a trigger and while check gear is held, and never off the playing screen.
    fn strip_marks(&self) -> Option<Vec<hud::SlotMark>> {
        let up = self.strip_ticks_left > 0 || self.check_gear_held;
        (self.screen == Screen::Playing && up).then(|| hud::rig_strip(self.mode.sim().rig()))
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

    #[cfg(test)]
    fn frame_listener(&self) -> moho_audio::Listener {
        self.listener
    }

    /// The view matrix and eye the frame draws at `alpha` between ticks: the interpolated
    /// pose with lean shift, head bob and lean roll.
    fn frame_view(&self, alpha: f32) -> (Mat4, Vec3) {
        let sim = self.mode.sim();
        let view = first_person_view(
            &self.previous,
            &sim.player(),
            alpha,
            look_to_turn(scale_look(self.input.pending_look(), self.aim_look_scale())),
        );
        let tuning = sim.lean_tuning();
        let from = self.previous_lean.amount();
        let amount = from + alpha * (sim.player_lean().amount() - from);
        let yaw = view.forward.x.atan2(view.forward.z);
        let bob = self.previous_bob.blend(self.bob, alpha);
        let eye =
            view.eye + lean_offset(yaw, amount * tuning.shift) + bob.offset(self.head_bob, yaw);
        (
            rolled_view_matrix(eye, view.forward, amount * tuning.roll),
            eye,
        )
    }

    /// The input map, for feeding it device events.
    pub fn input_mut(&mut self) -> &mut ActionMap<SearchlightAction> {
        &mut self.input
    }

    /// Replaces the default console shell; a game built to open on the main menu shows it on
    /// the new shell.
    #[must_use]
    pub fn with_shell(mut self, shell: UiShell<ConsoleCommand, MenuAction>) -> Self {
        self.stage.shell = Arc::new(Mutex::new(shell));
        if self.screen == Screen::MainMenu {
            self.stage.shell().menu_mut().push(main_screen());
        }
        self
    }

    /// A frame of the main menu: the title bare, the HUD hidden and the level seen from the
    /// feed's camera, or an empty world on a level without menu cameras; the shell paints the
    /// menu. The feed follows frame time: the sim is paused here.
    /// Opens the inventory on a finished search and leaves it on death, before the frame reads
    /// its view: both discard the pending look, so the listener and the draw share one view.
    fn settle_screen(&mut self, feedback: &[Feedback]) {
        let searched = feedback
            .iter()
            .any(|f| matches!(f, Feedback::SearchOpened { .. }));
        if searched && self.screen == Screen::Playing {
            self.open_inventory();
        }
        if self.screen == Screen::Inventory && self.mode.sim().player_body().is_dead() {
            self.enter_play();
        }
    }

    fn menu_frame(&mut self, ctx: &mut FrameContext<'_>) {
        self.stage.refresh_title("");
        self.stage.hide_hud();
        self.menu_feed.advance(ctx.frame_dt);
        let view = self.menu_feed.view();
        if let Some(renderer) = ctx.renderer() {
            match view {
                Some(view) => self.stage.draw(renderer, view),
                None => self.stage.draw_backdrop(renderer),
            }
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
            Routing::Opened => self.release_held_input(),
            Routing::Console | Routing::Closed => {}
            Routing::Menu
                if self.screen == Screen::Inventory
                    && pressed
                    && !repeat
                    && self.is_bound(SearchlightAction::Inventory, input) =>
            {
                self.close_inventory();
            }
            Routing::Menu
                if self.screen == Screen::Character
                    && pressed
                    && !repeat
                    && self.is_bound(SearchlightAction::Character, input) =>
            {
                self.enter_play();
            }
            Routing::Menu => self.answer_menu(ctx),
            Routing::Game if self.screen != Screen::Playing => {}
            Routing::Game => {
                if pressed && !repeat && input == Input::Key(Key::Escape) {
                    self.pause();
                    return routing;
                }
                if pressed
                    && !repeat
                    && self.is_bound(SearchlightAction::Inventory, input)
                    && !self.mode.sim().player_body().is_dead()
                {
                    self.open_inventory();
                    return routing;
                }
                if pressed
                    && !repeat
                    && self.is_bound(SearchlightAction::Character, input)
                    && !self.mode.sim().player_body().is_dead()
                {
                    self.open_character();
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

    /// The window title's vitals line: health or death, the wave and score when the mode
    /// has them, and god mode.
    fn vitals_line(&self) -> String {
        let sim = self.mode.sim();
        let vitals = hud::death_text(sim.player_body().is_dead()).map_or_else(
            || hud::health_text(sim.player_body().health(), sim.player_max_health()),
            str::to_owned,
        );
        let vitals = hud::vitals_text(&vitals, self.mode.tally());
        hud::god_mode_text(&vitals, self.god_mode)
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
            self.lean_side = None;
            self.aiming = false;
            // The inventory screen leaves the world running: it keeps the host levels and sends
            // what the player asked of it, and nothing the player's hands would do.
            return if self.screen.is_overlay() && !self.loop_paused() {
                self.with_host_levels(SearchlightCommand::default())
            } else {
                SearchlightCommand::default()
            };
        }
        let frame = self.input.end_tick();
        self.lean_side = resolve_lean(&frame, self.lean_side);
        let mut command = SearchlightCommand::from_frame(&frame);
        // The tick's look was sampled under the aim state before this read.
        (command.movement.yaw_delta, command.movement.pitch_delta) =
            look_to_turn(scale_look(frame.look(), self.aim_look_scale()));
        self.aiming = frame.held(SearchlightAction::Aim);
        self.look_scale = if self.aiming {
            look_scale(self.zoom)
        } else {
            1.0
        };
        self.with_host_levels(SearchlightCommand {
            lean: self.lean_side,
            ..command
        })
    }

    fn tick(&mut self, ctx: &mut TickContext<'_>, command: &SearchlightCommand) {
        self.previous = self.mode.sim().player();
        self.previous_lean = self.mode.sim().player_lean();
        let was_dead = self.mode.sim().player_body().is_dead();
        self.strip_ticks_left = self.strip_ticks_left.saturating_sub(1);
        self.check_gear_held = command.check_gear;
        let dt = ctx.tick_length.as_secs_f32();
        self.mode.tick(command, dt);
        self.previous_bob = self.bob;
        let player = self.mode.sim().player();
        // A dead player's sim stops moving them, so their last velocity must not keep the view bobbing.
        self.bob = if self.mode.sim().player_body().is_dead() {
            Bob::REST
        } else {
            self.bob.step(
                player.grounded,
                player.velocity.x.hypot(player.velocity.z),
                self.mode.sim().max_speed(),
                dt,
            )
        };
        if was_dead && !self.mode.sim().player_body().is_dead() {
            self.previous = self.mode.sim().player();
            self.previous_lean = self.mode.sim().player_lean();
            self.previous_bob = Bob::REST;
            self.bob = Bob::REST;
            self.stage.status.clear();
            self.strip_ticks_left = 0;
        }
        // A dead player's rounds are frozen and a restart numbers rounds from 0 again, so the
        // tracers of either end rather than keep a trace to place.
        if was_dead || self.mode.sim().player_body().is_dead() {
            self.stage.end_tracers();
        } else {
            self.stage.absorb_traces(self.mode.sim().round_traces());
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
        self.apply_saved_brightness(&mut *ctx.renderer);
        self.apply_saved_bloom(&mut *ctx.renderer);
    }

    fn window_mode_changed(&mut self, _ctx: &mut EventContext<'_>, mode: WindowMode) {
        self.window_mode_applied(mode);
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
                } else if self.stage.shell().on_window_event(&event) == Routing::Game
                    && self.screen == Screen::Playing
                {
                    handle_window_event(&mut self.input, &event);
                }
            }
            Event::Device(event) => {
                if self.focused && !self.input_captured() {
                    handle_device_event(&mut self.input, &event);
                }
            }
        }
        ctx.set_paused(self.loop_paused());
    }

    fn frame(&mut self, ctx: &mut FrameContext<'_>, alpha: f32) {
        let captured = self.input_captured();
        ctx.set_paused(self.loop_paused());
        if self.focused && !captured {
            self.input.sample_frame(ctx.frame_dt);
        }
        let feedback = self.mode.drain_feedback();
        self.settle_screen(&feedback);
        let (view, eye) = self.frame_view(alpha);
        self.listener = moho_audio::Listener::from_view(view);
        if let Some(audio) = ctx.audio() {
            audio.set_listener(self.listener);
        }
        let engine_commands = self.stage.shell().console_mut().take_engine_commands();
        for command in &engine_commands {
            apply_engine_command(command, ctx);
        }
        if let Some(settings) = self.pending_settings.take() {
            let applied = apply_engine_settings(&settings, ctx);
            if applied.shadow_quality != settings.shadow_quality {
                self.save_fallen_back_settings(applied);
            }
            self.stage.set_effects_quality(settings.effects_quality);
            let mode = start_window_mode(settings.window_mode);
            if mode != ctx.window_mode() {
                self.window_mode_applied(mode);
            }
        }
        if std::mem::take(&mut self.flip_window_pending) {
            let mode = ctx.window_mode().toggled();
            ctx.set_window_mode(mode);
            self.stage.shell().console_mut().print(match mode {
                WindowMode::Windowed => "Window mode: windowed",
                WindowMode::Borderless => "Window mode: fullscreen",
            });
            self.window_mode_applied(mode);
        }
        for feedback in feedback {
            if let Feedback::PlayerHit { from, .. } = feedback {
                let player = self.mode.sim().player();
                self.stage
                    .show_hit_edge(hud::hit_edge(player.position, player.yaw, from));
            }
            if hud::shows_strip(&feedback) {
                self.strip_ticks_left = STRIP_FADE_TICKS;
            }
            self.stage.react(&feedback, ctx.audio());
        }
        if self.screen == Screen::MainMenu {
            self.menu_frame(ctx);
            return;
        }
        let dead = self.mode.sim().player_body().is_dead();
        if self.screen == Screen::Inventory {
            let lists = self.inventory_lists();
            self.stage.shell().menu_mut().set_lists(lists);
        }
        if self.screen == Screen::Character {
            if dead {
                self.enter_play();
            } else {
                let lists = character::character_lists(self.mode.sim());
                self.stage.shell().menu_mut().set_lists(lists);
            }
        }
        let vitals = self.vitals_line();
        self.stage.refresh_title(&vitals);
        self.stage.refresh_hud(dead);
        self.stage.show_spread(self.mode.sim().player_spread());
        if self.screen == Screen::Paused {
            self.stage.hide_hud();
        }
        self.refresh_screen_texts(dead);
        self.stage.sync_enemies(self.mode.sim());
        self.stage.sync_bodies(self.mode.sim());
        self.stage
            .sync_hit_volumes(self.mode.sim(), self.hit_volumes);
        self.stage.show_tracers(alpha);
        self.stage.scene.effects_mut().advance(ctx.frame_dt);
        if let Some(renderer) = ctx.renderer() {
            let fov = self.play_fov_degrees();
            self.stage.draw_at(renderer, (view, eye), fov);
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
    use moho_physics::{Hull, HullShape, Stature};
    use winit::event::{DeviceEvent, WindowEvent};

    use super::*;
    use crate::sim::Feedback;
    use crate::test_support::{
        WaveScore, arena_with, arena_with_controls, mag, press, rifle_from, test_config,
        wave_tuning,
    };
    use searchlight_rules::{Grade, LeanSide};

    #[test]
    fn a_title_line_without_waves_has_no_wave() {
        let arena = arena_with(&[], Vec3::ZERO, &[], &[]);
        let config = || test_config(rifle_from(mag(30, 30)), vec![]);
        let actions = || {
            let (bindings, _) = ActionBindings::load(&BTreeMap::new());
            ActionMap::new(bindings, 1.0)
        };
        let waves = SearchlightGame::new(
            ArenaMode::new(arena.clone(), config(), wave_tuning(1)),
            actions(),
        );
        let quiet = SearchlightGame::new(ArenaMode::without_waves(arena, config()), actions());

        let quiet_line = quiet.vitals_line();

        assert_eq!(waves.vitals_line(), "HP 100/100 | Wave 1  Score 0");
        assert_eq!(quiet_line, "HP 100/100");
    }

    fn game() -> SearchlightGame {
        let mode = ArenaMode::new(
            arena_with(&[], Vec3::ZERO, &[], &[]),
            test_config(rifle_from(mag(30, 5)), vec![]),
            wave_tuning(1),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
    }

    fn shot_file() -> &'static Path {
        Path::new("rifle.wav")
    }

    fn enemy_shot_from(from: Vec3) -> Feedback {
        Feedback::EnemyFired { target: 0, from }
    }

    #[test]
    fn an_enemy_shot_is_louder_in_the_ear_on_its_side() {
        let mut game = game().with_shot_sound(shot_file());
        let mut renderer = SettingsRenderer::default();
        let view = draw_frame(&mut game, &mut renderer);
        let eye = moho_audio::Listener::from_view(view);
        let listener = game.frame_listener();

        for (side, louder_right) in [(1.0, true), (-1.0, false)] {
            let from = eye.position + eye.right() * (10.0 * side);
            let source = game
                .stage
                .shot_sound(&enemy_shot_from(from))
                .expect("an enemy shot with a shot file has a sound");
            let at = Vec3::from(source.position().expect("an enemy shot is positioned"));

            let pan = moho_audio::spatial::stereo_pan(&listener, at);

            assert_eq!(pan.right > pan.left, louder_right, "side {side}: {pan:?}");
            assert_ne!(pan.right, pan.left, "side {side}: {pan:?}");
        }
    }

    #[test]
    fn a_far_enemy_shot_plays_quieter_than_a_near_one() {
        let mut stage = Stage::new();
        stage.shot_sound = Some(shot_file().to_owned());
        let listener = moho_audio::Listener::default();
        let heard = |distance: f32| {
            let source = stage
                .shot_sound(&enemy_shot_from(Vec3::new(0.0, 0.0, -distance)))
                .expect("an enemy shot with a shot file has a sound");
            let at = Vec3::from(source.position().expect("an enemy shot is positioned"));
            let gain = moho_audio::spatial::distance_gain(listener.position.distance(at))
                .expect("within audible range");
            source.volume() * gain
        };

        let near = heard(5.0);
        let far = heard(40.0);

        assert!(far < near, "far {far}, near {near}");
        assert!(far > 0.0, "far {far}");
    }

    #[test]
    fn the_players_own_shot_plays_from_no_position() {
        let mut stage = Stage::new();
        stage.shot_sound = Some(shot_file().to_owned());

        let source = stage
            .shot_sound(&Feedback::Fired)
            .expect("the player's shot has a sound");

        assert_eq!(source.position(), None);
        assert_eq!(source.volume(), SHOT_VOLUME);
        assert_eq!(*source.category(), moho_audio::AudioCategory::SoundEffect);
        assert_eq!(source.path(), shot_file().to_string_lossy());
    }

    #[test]
    fn the_listener_is_the_eye_the_frame_draws() {
        let mut game = game();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let lean_left = SearchlightCommand {
            lean: Some(LeanSide::Left),
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &lean_left);
        tick_with(&mut game, &mut clock, &lean_left);
        let mut renderer = SettingsRenderer::default();

        let view = draw_frame(&mut game, &mut renderer);

        let standing = game.sim().player().eye_position();
        let listener = game.frame_listener();
        assert_eq!(listener, moho_audio::Listener::from_view(view));
        assert!(
            listener.position.distance(standing) > 0.01,
            "the lean moved the eye off {standing:?}: {listener:?}"
        );
    }

    #[test]
    fn a_frame_without_audio_or_renderer_still_stores_the_listener_at_its_alpha() {
        let mut game = game();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let lean_left = SearchlightCommand {
            lean: Some(LeanSide::Left),
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &lean_left);
        tick_with(&mut game, &mut clock, &lean_left);
        let frame_clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &frame_clock, None, None);

        game.frame(&mut ctx, 0.25);

        let (view, _) = game.frame_view(0.25);
        assert_eq!(game.frame_listener(), moho_audio::Listener::from_view(view));
        assert_ne!(
            game.frame_listener(),
            moho_audio::Listener::from_view(game.frame_view(1.0).0)
        );
    }

    #[test]
    fn no_shot_file_plays_no_shot_sound() {
        let stage = Stage::new();

        let own = stage.shot_sound(&Feedback::Fired);
        let enemy = stage.shot_sound(&enemy_shot_from(Vec3::new(0.0, 0.0, 10.0)));

        assert!(own.is_none(), "{own:?}");
        assert!(enemy.is_none(), "{enemy:?}");
    }

    #[test]
    fn only_shots_have_a_shot_sound() {
        let mut stage = Stage::new();
        stage.shot_sound = Some(shot_file().to_owned());
        let hit = Feedback::Hit {
            target: 0,
            part: searchlight_rules::BodyPart::Head,
            report: searchlight_rules::Body::new(searchlight_rules::BodyProfile {
                max_health: 10,
                head_status: 10,
                arm_status: 10,
                leg_status: 10,
            })
            .hit(searchlight_rules::BodyPart::Head, 1),
            point: Vec3::ZERO,
        };
        let impact = Feedback::Impact {
            point: Vec3::ZERO,
            normal: Vec3::Y,
        };

        for feedback in [Feedback::Dry, hit, impact] {
            let sound = stage.shot_sound(&feedback);

            assert!(sound.is_none(), "{feedback:?}: {sound:?}");
        }
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
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        crate::test_support::set_eye_above_centre(&mut config, 0.72);
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

    #[test]
    fn holding_q_puts_lean_left_on_the_command() {
        let mut game = game();
        press(game.input_mut(), SearchlightAction::LeanLeft);

        let first = game.command();
        let held = game.command();
        press(game.input_mut(), SearchlightAction::LeanRight);
        let switched = game.command();
        let both_held = game.command();
        game.input_mut().key(Key::E, false);
        let back = game.command();
        game.input_mut().key(Key::Q, false);
        let released = game.command();

        assert_eq!(
            [
                first.lean,
                held.lean,
                switched.lean,
                both_held.lean,
                back.lean,
                released.lean
            ],
            [
                Some(LeanSide::Left),
                Some(LeanSide::Left),
                Some(LeanSide::Right),
                Some(LeanSide::Right),
                Some(LeanSide::Left),
                None
            ]
        );
    }

    #[test]
    fn a_captured_input_command_leans_nowhere() {
        let mut game = game();
        let clock = GameClock::default();
        press(game.input_mut(), SearchlightAction::LeanLeft);
        assert_eq!(game.command().lean, Some(LeanSide::Left));
        open_the_console(&mut game, &clock);

        let command = game.command();

        assert_eq!(command.lean, None);
    }

    #[test]
    fn the_drawn_eye_blends_the_lean_between_ticks() {
        let mut game = game();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let lean_left = SearchlightCommand {
            lean: Some(LeanSide::Left),
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &lean_left);
        tick_with(&mut game, &mut clock, &lean_left);
        let mut renderer = SettingsRenderer::default();

        let view = draw_frame(&mut game, &mut renderer);

        let tick = 1.0 / 60.0 / 0.25;
        let blended = tick + ALPHA * (2.0 * tick - tick);
        let standing = {
            let player = game.sim().player();
            player.eye_position()
        };
        let eye = view.inverse().w_axis.truncate();
        assert!(
            (eye.x - standing.x - 0.35 * blended).abs() < 1e-3,
            "the eye should sit {} m left, at {eye:?}",
            0.35 * blended
        );
        assert!((eye.y - standing.y).abs() < 1e-3 && (eye.z - standing.z).abs() < 1e-3);
        let up = view.transpose().col(1).truncate();
        let roll = blended * 12.0_f32.to_radians();
        assert!(
            (up.x - roll.sin()).abs() < 1e-3,
            "the view should roll {roll} rad left, up is {up:?}"
        );
    }

    #[test]
    fn a_restart_after_dying_while_leaned_draws_the_first_frame_upright() {
        let mut game = game();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let lean_right = SearchlightCommand {
            lean: Some(LeanSide::Right),
            ..SearchlightCommand::default()
        };
        for _ in 0..30 {
            tick_with(&mut game, &mut clock, &lean_right);
        }
        assert!(game.sim().player_lean().amount() > 0.99);
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &lean_right);
        assert!(game.sim().player_body().is_dead());

        tick_with(&mut game, &mut clock, &restart_command());

        assert!(!game.sim().player_body().is_dead());
        let standing = {
            let player = game.sim().player();
            player.eye_position()
        };
        for alpha in [0.0, 0.5] {
            let mut renderer = SettingsRenderer::default();
            let ctx_clock = GameClock::default();
            let mut ctx = FrameContext::new(TICK, &ctx_clock, Some(&mut renderer), None);
            game.frame(&mut ctx, alpha);
            let view = *renderer.views.last().expect("a frame began");

            let eye = view.inverse().w_axis.truncate();
            let up = view.transpose().col(1).truncate();
            assert!(
                eye.distance(standing) < 1e-3,
                "alpha {alpha}: the eye is {eye:?}, standing is {standing:?}"
            );
            assert!(
                up.distance(Vec3::Y) < 1e-3,
                "alpha {alpha}: the view is rolled, up is {up:?}"
            );
        }
    }

    const BACKTICK: Input = Input::Key(Key::Backtick);
    const TICK: Duration = Duration::from_millis(16);

    fn open_the_console(game: &mut SearchlightGame, clock: &GameClock) {
        let mut ctx = EventContext::new(clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        assert_eq!(routing, Routing::Opened);
    }

    /// A game whose enemy is walking toward the player, so every sim tick moves it.
    pub(super) fn game_with_a_moving_enemy() -> SearchlightGame {
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
    }

    /// Keeps the unshadowed instances of the last frame, which is where the engine draws streaks.
    #[derive(Default)]
    struct TracerRenderer {
        tracers: Vec<InstanceGpu>,
    }

    impl RendererBackend for TracerRenderer {
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
            _camera: (Mat4, Mat4, Vec3),
        ) -> Result<(), moho_renderer::FrameError> {
            self.tracers.clear();
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn enqueue_unshadowed_draw(&mut self, _mesh: u32, instances: &[InstanceGpu]) {
            self.tracers.extend_from_slice(instances);
        }
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            quality
        }
        fn set_ssao_quality(&mut self, _quality: u8) {}
    }

    /// Frames enough to outlast a tracer's fade at `TICK` a frame.
    const FADE_FRAMES: usize = 20;

    /// A game on a floor with `boxes` on it, the player at the origin facing +Z, and no waves.
    fn tracer_game(boxes: &[(Vec3, Vec3)], config: crate::sim::SimConfig) -> SearchlightGame {
        let mode = ArenaMode::without_waves(arena_with(boxes, Vec3::ZERO, &[], &[]), config);
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
    }

    fn tracer_config() -> crate::sim::SimConfig {
        test_config(rifle_from(mag(30, 30)), vec![])
    }

    fn pull_command() -> SearchlightCommand {
        SearchlightCommand {
            fire: true,
            ..SearchlightCommand::default()
        }
    }

    fn tracer_frame(game: &mut SearchlightGame, renderer: &mut TracerRenderer, alpha: f32) {
        let clock = GameClock::default();
        let backend: &mut dyn RendererBackend = renderer;
        let mut ctx = FrameContext::new(TICK, &clock, Some(backend), None);
        game.frame(&mut ctx, alpha);
    }

    /// Draws `count` frames with no tick between; returns the most effects live in any of them.
    fn tracer_frames(
        game: &mut SearchlightGame,
        renderer: &mut TracerRenderer,
        count: usize,
    ) -> usize {
        let mut most = 0;
        for _ in 0..count {
            tracer_frame(game, renderer, 0.5);
            most = most.max(live_tracers(game, renderer));
        }
        most
    }

    /// Whether `instance` is a bullet mark: drawn in the stage's mark material, not a tracer's.
    /// A fresh stage hands out its materials in the same order as every other.
    fn is_mark(instance: &InstanceGpu) -> bool {
        instance.material == Stage::new().impact_material
    }

    fn tracers_in(renderer: &TracerRenderer) -> Vec<InstanceGpu> {
        renderer
            .tracers
            .iter()
            .filter(|instance| !is_mark(instance))
            .copied()
            .collect()
    }

    /// The live effects that are not marks, which are those the last frame did not draw as one.
    fn live_tracers(game: &mut SearchlightGame, renderer: &TracerRenderer) -> usize {
        let marks = renderer.tracers.iter().filter(|i| is_mark(i)).count();
        live_effects(game).saturating_sub(marks)
    }

    fn live_effects(game: &mut SearchlightGame) -> usize {
        game.stage.scene.effects_mut().live()
    }

    fn column(instance: &InstanceGpu, index: usize) -> Vec3 {
        let [x, y, z, _] = instance.model[index];
        Vec3::new(x, y, z)
    }

    /// Where the drawn streak's head is: its tail plus its length along its heading.
    fn tracer_head(instance: &InstanceGpu) -> Vec3 {
        column(instance, 3) + column(instance, 0)
    }

    #[test]
    fn a_round_in_flight_has_one_tracer_at_its_interpolated_position() {
        let mut game = tracer_game(&[], tracer_config());
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();
        tick_with(&mut game, &mut clock, &pull_command());

        tracer_frame(&mut game, &mut renderer, 0.5);

        let muzzle = game.sim().player_eye();
        let [at_muzzle] = renderer.tracers.as_slice() else {
            panic!("{} tracers after the shot", renderer.tracers.len());
        };
        assert!(tracer_head(at_muzzle).distance(muzzle) < 1e-3);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        let &[trace] = game.sim().round_traces() else {
            panic!("expected one trace: {:?}", game.sim().round_traces());
        };
        assert!(
            trace.from.distance(trace.to) > 5.0,
            "the round did not move: {trace:?}"
        );
        for alpha in [0.0, 0.25, 0.5, 1.0] {
            tracer_frame(&mut game, &mut renderer, alpha);

            let [tracer] = renderer.tracers.as_slice() else {
                panic!("alpha {alpha}: {} tracers", renderer.tracers.len());
            };
            let expected = trace.from.lerp(trace.to, alpha);
            assert!(
                tracer_head(tracer).distance(expected) < 1e-3,
                "alpha {alpha}: head {}, expected {expected}",
                tracer_head(tracer)
            );
            let along = column(tracer, 0);
            assert!((along.length() - 3.0).abs() < 1e-3, "length {along}");
            assert!(along.normalize().distance(trace.heading.normalize()) < 1e-3);
            assert!((column(tracer, 1).length() - 0.03).abs() < 1e-4);
            assert_eq!(live_effects(&mut game), 1, "alpha {alpha}");
        }
    }

    #[test]
    fn a_tracer_ends_when_its_round_hits() {
        let wall = (Vec3::new(-5.0, 0.0, 3.0), Vec3::new(5.0, 4.0, 3.2));
        for frame_between in [false, true] {
            let mut game = tracer_game(&[wall], tracer_config());
            let mut clock = GameClock::new(0.0, 1.0, 1.0);
            let mut renderer = TracerRenderer::default();
            tick_with(&mut game, &mut clock, &pull_command());
            if frame_between {
                tracer_frame(&mut game, &mut renderer, 0.5);
                assert_eq!(
                    tracers_in(&renderer).len(),
                    1,
                    "no tracer flew before the hit"
                );
            }

            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
            tracer_frame(&mut game, &mut renderer, 0.5);

            let &[trace] = game.sim().round_traces() else {
                panic!("expected one trace: {:?}", game.sim().round_traces());
            };
            assert!(trace.ended, "the round did not hit the wall");
            let tracers = tracers_in(&renderer);
            let [tracer] = tracers.as_slice() else {
                panic!(
                    "between {frame_between}: {} tracers on the hit",
                    tracers.len()
                );
            };
            assert!(
                tracer_head(tracer).distance(trace.to) < 1e-3,
                "between {frame_between}: the tracer ended at {}, not on the wall at {}",
                tracer_head(tracer),
                trace.to
            );
            let most = tracer_frames(&mut game, &mut renderer, FADE_FRAMES);
            assert!(most <= 1, "between {frame_between}: {most} tracers lived");
            assert!(tracers_in(&renderer).is_empty(), "between {frame_between}");
            assert_eq!(
                live_tracers(&mut game, &renderer),
                0,
                "between {frame_between}"
            );
        }
    }

    #[test]
    fn a_tracer_ends_when_it_is_spent_or_leaves() {
        let spent_at_once = searchlight_rules::Ballistics {
            muzzle_velocity: 150.0,
            drag: 30.0,
        };
        for (label, ballistics) in [
            ("leaves the level", crate::test_support::TEST_BALLISTICS),
            ("slows too far", spent_at_once),
        ] {
            let mut config = tracer_config();
            config.rounds = BTreeMap::from([(
                searchlight_rules::ItemId::new("test_round"),
                searchlight_rules::RoundStats {
                    ballistics,
                    damage: 30,
                },
            )]);
            let mut game = tracer_game(&[], config);
            let mut clock = GameClock::new(0.0, 1.0, 1.0);
            let mut renderer = TracerRenderer::default();
            let look_up = SearchlightCommand {
                movement: moho_physics::CharacterCommand {
                    pitch_delta: 0.3,
                    ..moho_physics::CharacterCommand::default()
                },
                ..SearchlightCommand::default()
            };
            tick_with(&mut game, &mut clock, &look_up);
            tick_with(&mut game, &mut clock, &pull_command());
            tracer_frame(&mut game, &mut renderer, 0.5);
            let mut flew = !renderer.tracers.is_empty();

            for _ in 0..40 {
                tick_with(&mut game, &mut clock, &SearchlightCommand::default());
                tracer_frame(&mut game, &mut renderer, 0.5);
                flew |= !renderer.tracers.is_empty();
                if game.sim().round_traces().is_empty() {
                    break;
                }
            }

            assert!(flew, "{label}: no tracer was drawn in flight");
            assert!(
                game.sim().round_traces().is_empty(),
                "{label}: the round never ended"
            );
            let most = tracer_frames(&mut game, &mut renderer, FADE_FRAMES);
            assert!(most <= 1, "{label}: {most} tracers lived");
            assert!(renderer.tracers.is_empty(), "{label}");
            assert_eq!(live_effects(&mut game), 0, "{label}");
        }
    }

    #[test]
    fn restart_ends_every_tracer() {
        let mut game = tracer_game(&[], tracer_config());
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();
        for tick in 0..6 {
            let command = if tick % 4 == 0 {
                pull_command()
            } else {
                SearchlightCommand::default()
            };
            tick_with(&mut game, &mut clock, &command);
            tracer_frame(&mut game, &mut renderer, 0.5);
        }
        assert_eq!(renderer.tracers.len(), 2, "two rounds are in flight");
        game.mode.sim_mut().kill_player();

        tick_with(&mut game, &mut clock, &restart_command());
        let most = tracer_frames(&mut game, &mut renderer, FADE_FRAMES);

        assert!(game.sim().round_traces().is_empty());
        assert!(most <= 2, "{most} tracers lived");
        assert!(renderer.tracers.is_empty());
        assert_eq!(live_effects(&mut game), 0);
    }

    #[test]
    fn a_dead_players_tracer_holds_still_where_it_was_last_placed() {
        let mut game = tracer_game(&[], tracer_config());
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();
        tick_with(&mut game, &mut clock, &pull_command());
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        tracer_frame(&mut game, &mut renderer, 0.5);
        let placed = tracer_head(&renderer.tracers[0]);
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());

        for alpha in [0.0, 1.0, 0.25, 0.75] {
            tracer_frame(&mut game, &mut renderer, alpha);

            for tracer in &renderer.tracers {
                assert!(
                    tracer_head(tracer).distance(placed) < 1e-3,
                    "alpha {alpha}: the head moved to {}",
                    tracer_head(tracer)
                );
            }
        }
        assert!(game.sim().round_traces().is_empty());
    }

    #[test]
    fn a_tracer_ends_at_the_strike_when_later_ticks_pass_before_the_next_frame() {
        let wall = (Vec3::new(-5.0, 0.0, 3.0), Vec3::new(5.0, 4.0, 3.2));
        for frame_before_hit in [false, true] {
            let mut game = tracer_game(&[wall], tracer_config());
            let mut clock = GameClock::new(0.0, 1.0, 1.0);
            let mut renderer = TracerRenderer::default();
            tick_with(&mut game, &mut clock, &pull_command());
            if frame_before_hit {
                tracer_frame(&mut game, &mut renderer, 0.5);
            }
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
            let &[trace] = game.sim().round_traces() else {
                panic!("expected one trace: {:?}", game.sim().round_traces());
            };
            assert!(trace.ended, "the round did not hit the wall");
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());

            tracer_frame(&mut game, &mut renderer, 0.5);

            let tracers = tracers_in(&renderer);
            let [tracer] = tracers.as_slice() else {
                panic!("before {frame_before_hit}: {} tracers", tracers.len());
            };
            assert!(
                tracer_head(tracer).distance(trace.to) < 1e-3,
                "before {frame_before_hit}: ended at {}, not at the strike {}",
                tracer_head(tracer),
                trace.to
            );
        }
    }

    #[test]
    fn a_round_fired_after_a_restart_gets_a_fresh_tracer_not_its_predecessors() {
        let wall = (Vec3::new(-5.0, 0.0, 3.0), Vec3::new(5.0, 4.0, 3.2));
        let mut game = tracer_game(&[wall], tracer_config());
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();
        tick_with(&mut game, &mut clock, &pull_command());
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        tracer_frames(&mut game, &mut renderer, 4);
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &restart_command());

        tick_with(&mut game, &mut clock, &pull_command());
        tracer_frame(&mut game, &mut renderer, 0.5);

        let muzzle = game.sim().player_eye();
        let fresh: Vec<&InstanceGpu> = renderer
            .tracers
            .iter()
            .filter(|tracer| tracer_head(tracer).distance(muzzle) < 1e-3)
            .collect();
        let [tracer] = fresh.as_slice() else {
            panic!("{} tracers at the muzzle", fresh.len());
        };
        assert!(
            (column(tracer, 1).length() - 0.03).abs() < 1e-4,
            "the new round's tracer is fading"
        );
    }

    #[test]
    fn under_simple_no_tracer_starts() {
        let wall = (Vec3::new(-5.0, 0.0, 30.0), Vec3::new(5.0, 4.0, 30.2));
        let mut control = tracer_game(&[wall], tracer_config());
        let mut simple = tracer_game(&[wall], tracer_config());
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();
        let simple_pull = SearchlightCommand {
            simple: true,
            ..pull_command()
        };

        tick_with(&mut control, &mut clock, &pull_command());
        tracer_frame(&mut control, &mut renderer, 0.5);
        let control_tracers = renderer.tracers.len();
        let mut drawn = 0;
        let mut most = 0;
        tick_with(&mut simple, &mut clock, &simple_pull);
        for _ in 0..4 {
            tracer_frame(&mut simple, &mut renderer, 0.5);
            drawn += tracers_in(&renderer).len();
            most = most.max(live_tracers(&mut simple, &renderer));
            tick_with(&mut simple, &mut clock, &SearchlightCommand::default());
        }

        assert_eq!(control_tracers, 1, "a round in flight has no tracer");
        assert_eq!(drawn, 0);
        assert_eq!(most, 0);
        assert!(simple.sim().round_traces().is_empty());
    }

    fn impact_at(point: Vec3, normal: Vec3) -> Feedback {
        Feedback::Impact { point, normal }
    }

    /// The marks among the last frame's unshadowed instances: those at full size, as
    /// [`impact_style`] sets it.
    fn drawn_marks(renderer: &TracerRenderer) -> Vec<InstanceGpu> {
        renderer
            .tracers
            .iter()
            .filter(|instance| {
                is_mark(instance)
                    && (column(instance, 0).length() - impact_style(0).size).abs() < 1e-3
            })
            .copied()
            .collect()
    }

    #[test]
    fn an_impact_places_one_mark_at_its_point_along_its_normal() {
        let point = Vec3::new(1.0, 2.0, 3.0);
        let cases = [Vec3::NEG_Z, Vec3::Y, Vec3::new(1.0, 1.0, 0.0).normalize()];
        for normal in cases {
            let mut game = tracer_game(&[], tracer_config());
            let mut renderer = TracerRenderer::default();

            game.stage.react(&impact_at(point, normal), None);
            tracer_frame(&mut game, &mut renderer, 0.5);

            let [mark] = renderer.tracers.as_slice() else {
                panic!(
                    "normal {normal}: {} instances drawn",
                    renderer.tracers.len()
                );
            };
            assert_eq!(live_effects(&mut game), 1, "normal {normal}");
            assert!(
                column(mark, 2).distance(normal) < 1e-3,
                "normal {normal}: faces {}",
                column(mark, 2)
            );
            assert!(
                column(mark, 0).dot(normal).abs() < 1e-3
                    && column(mark, 1).dot(normal).abs() < 1e-3,
                "normal {normal}: the mark does not lie flat on its surface"
            );
            assert!(
                column(mark, 3).distance(point) < 0.05
                    && (column(mark, 3) - point).dot(normal) > 0.0,
                "normal {normal}: at {}, not on {point}",
                column(mark, 3)
            );
            assert!(
                (column(mark, 0).length() - 0.05).abs() < 1e-4,
                "normal {normal}"
            );
            let dark = game
                .stage
                .scene
                .material_table
                .find_or_push(&Paint::ArenaMark);
            assert_ne!(
                mark.material, dark,
                "normal {normal}: drawn in the arena's mark paint"
            );
        }
    }

    #[test]
    fn consecutive_impacts_turn_each_mark_by_the_golden_angle() {
        let mut game = tracer_game(&[], tracer_config());
        let mut renderer = TracerRenderer::default();

        game.stage
            .react(&impact_at(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z), None);
        game.stage
            .react(&impact_at(Vec3::new(1.0, 1.0, 3.0), Vec3::NEG_Z), None);
        tracer_frame(&mut game, &mut renderer, 0.5);

        let marks = drawn_marks(&renderer);
        let [first, second] = marks.as_slice() else {
            panic!("{} marks", marks.len());
        };
        let turn = (column(first, 0).dot(column(second, 0)) / (0.05 * 0.05)).acos();
        let step = IMPACT_ROLL_STEP.rem_euclid(std::f32::consts::TAU);
        let expected = if step > std::f32::consts::PI {
            std::f32::consts::TAU - step
        } else {
            step
        };
        assert!(
            (turn - expected).abs() < 1e-2,
            "turned {turn}, not {expected}"
        );
    }

    #[test]
    fn the_roll_stays_within_a_turn_over_many_impacts() {
        let mut game = tracer_game(&[], tracer_config());

        for _ in 0..100 {
            game.stage
                .react(&impact_at(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z), None);

            let roll = game.stage.impact_roll;
            assert!((0.0..std::f32::consts::TAU).contains(&roll), "roll {roll}");
        }
    }

    #[test]
    fn an_impact_with_no_normal_draws_no_mark_and_no_bad_numbers() {
        let mut game = tracer_game(&[], tracer_config());
        let mut renderer = TracerRenderer::default();

        game.stage
            .react(&impact_at(Vec3::new(0.0, 1.0, 3.0), Vec3::ZERO), None);
        tracer_frame(&mut game, &mut renderer, 0.5);

        assert_eq!(live_effects(&mut game), 0);
        for instance in &renderer.tracers {
            for index in 0..4 {
                assert!(column(instance, index).is_finite(), "column {index}");
            }
        }
    }

    #[test]
    fn five_impacts_leave_five_marks() {
        let points = [
            Vec3::new(-2.0, 1.0, 3.0),
            Vec3::new(-1.0, 1.5, 3.0),
            Vec3::new(0.0, 2.0, 3.0),
            Vec3::new(1.0, 1.5, 3.0),
            Vec3::new(2.0, 1.0, 3.0),
        ];
        let mut game = tracer_game(&[], tracer_config());
        let mut renderer = TracerRenderer::default();

        for point in points {
            game.stage.react(&impact_at(point, Vec3::NEG_Z), None);
        }
        tracer_frame(&mut game, &mut renderer, 0.5);

        let marks = drawn_marks(&renderer);
        assert_eq!(marks.len(), 5, "{} instances", renderer.tracers.len());
        assert_eq!(live_effects(&mut game), 5);
        for point in points {
            let at_point = marks
                .iter()
                .filter(|mark| column(mark, 3).distance(point) < 0.05)
                .count();
            assert_eq!(at_point, 1, "marks at {point}");
        }
    }

    #[test]
    fn a_round_striking_a_wall_leaves_a_mark_at_the_strike_lying_flat() {
        let wall = (Vec3::new(-5.0, 0.0, 3.0), Vec3::new(5.0, 4.0, 3.2));
        let mut game = tracer_game(&[wall], tracer_config());
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();

        tick_with(&mut game, &mut clock, &pull_command());
        for _ in 0..10 {
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        }
        tracer_frame(&mut game, &mut renderer, 0.5);

        let marks = drawn_marks(&renderer);
        let [mark] = marks.as_slice() else {
            panic!("{} marks on the wall", marks.len());
        };
        assert!(column(mark, 2).distance(Vec3::NEG_Z) < 1e-3);
        assert!(
            (column(mark, 3).z - 3.0).abs() < 0.05,
            "{}",
            column(mark, 3)
        );
    }

    #[test]
    fn marks_use_the_impact_style_lifetime() {
        let mut game = tracer_game(&[], tracer_config());
        let mut renderer = TracerRenderer::default();
        game.stage
            .react(&impact_at(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z), None);

        game.stage
            .scene
            .effects_mut()
            .advance(Duration::from_secs(17));
        tracer_frame(&mut game, &mut renderer, 0.5);
        let before_fade = drawn_marks(&renderer);
        game.stage
            .scene
            .effects_mut()
            .advance(Duration::from_secs(2));
        tracer_frame(&mut game, &mut renderer, 0.5);
        let fading = renderer.tracers.clone();
        game.stage
            .scene
            .effects_mut()
            .advance(Duration::from_millis(1100));

        assert_eq!(before_fade.len(), 1, "the mark did not last 17 s");
        assert!((column(&before_fade[0], 0).length() - 0.05).abs() < 1e-4);
        let [fading] = fading.as_slice() else {
            panic!("{} marks in the last 2 s", fading.len());
        };
        assert!(
            (column(fading, 0).length() - 0.025).abs() < 0.003,
            "half faded at 19 s, drawn {}",
            column(fading, 0).length()
        );
        assert_eq!(live_effects(&mut game), 0, "the mark outlived 20 s");
    }

    #[test]
    fn with_effects_off_an_impact_places_nothing_and_play_goes_on() {
        let wall = (Vec3::new(-5.0, 0.0, 3.0), Vec3::new(5.0, 4.0, 3.2));
        let mut game = tracer_game(&[wall], tracer_config());
        let mut on = tracer_game(&[wall], tracer_config());
        game.stage.set_effects_quality(0);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = TracerRenderer::default();
        let mut on_renderer = TracerRenderer::default();

        for played in [&mut game, &mut on] {
            played
                .stage
                .react(&impact_at(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z), None);
            tick_with(played, &mut clock, &pull_command());
            for _ in 0..10 {
                tick_with(played, &mut clock, &SearchlightCommand::default());
            }
        }
        tracer_frame(&mut game, &mut renderer, 0.5);
        tracer_frame(&mut on, &mut on_renderer, 0.5);

        assert_eq!(
            drawn_marks(&on_renderer).len(),
            2,
            "the control drew no marks"
        );
        assert_eq!(live_effects(&mut game), 0);
        assert!(renderer.tracers.is_empty());
        assert_eq!(
            game.sim().rifle().magazine().rounds(),
            on.sim().rifle().magazine().rounds()
        );
        assert!(game.sim().round_traces().is_empty());
    }

    #[test]
    fn an_effects_quality_of_off_draws_no_tracer_and_breaks_nothing() {
        let wall = (Vec3::new(-5.0, 0.0, 20.0), Vec3::new(5.0, 4.0, 20.2));
        let mut off = tracer_game(&[wall], tracer_config());
        let mut on = tracer_game(&[wall], tracer_config());
        off.stage.set_effects_quality(0);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut off_renderer = TracerRenderer::default();
        let mut on_renderer = TracerRenderer::default();
        let (mut off_drawn, mut on_drawn) = (0, 0);

        for tick in 0..6 {
            let command = if tick == 0 {
                pull_command()
            } else {
                SearchlightCommand::default()
            };
            tick_with(&mut off, &mut clock, &command);
            tick_with(&mut on, &mut clock, &command);
            tracer_frame(&mut off, &mut off_renderer, 0.5);
            tracer_frame(&mut on, &mut on_renderer, 0.5);
            off_drawn += off_renderer.tracers.len();
            on_drawn += on_renderer.tracers.len();

            assert_eq!(
                off.sim().round_traces(),
                on.sim().round_traces(),
                "tick {tick}: the flight changed"
            );
            assert_eq!(live_effects(&mut off), 0, "tick {tick}");
        }

        assert!(on_drawn > 0, "the control drew no tracer");
        assert_eq!(off_drawn, 0);
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
        brightness: Vec<f32>,
        bloom: Vec<u8>,
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            self.shadow.push(quality);
            quality
        }
        fn set_ssao_quality(&mut self, quality: u8) {
            self.ssao.push(quality);
        }
        fn set_debug_view(&mut self, mode: Option<u32>) {
            self.debug_view.push(mode);
        }
        fn set_brightness(&mut self, brightness: f32) {
            self.brightness.push(brightness);
        }
        fn set_bloom_quality(&mut self, quality: u8) {
            self.bloom.push(quality);
        }
    }

    /// `Game::init` needs a window, so this drives the Bloom step it runs.
    #[test]
    fn init_applies_the_saved_bloom() {
        let dir = tempfile::tempdir().unwrap();
        let mut prefs = Prefs::default();
        prefs.set_bloom(1);
        let settings = DebugSettings::new(prefs, dir.path().join("searchlight.ini"));
        let game = game().with_debug_settings(settings);
        let mut renderer = SettingsRenderer::default();

        game.apply_saved_bloom(&mut renderer);

        assert_eq!(renderer.bloom, vec![1]);
    }

    /// `Game::init` needs a window, so this drives the brightness step it runs.
    #[test]
    fn init_applies_the_saved_brightness() {
        let dir = tempfile::tempdir().unwrap();
        let mut prefs = Prefs::default();
        prefs.set_brightness(1.5);
        let settings = DebugSettings::new(prefs, dir.path().join("searchlight.ini"));
        let game = game().with_debug_settings(settings);
        let mut renderer = SettingsRenderer::default();

        game.apply_saved_brightness(&mut renderer);

        assert_eq!(renderer.brightness, vec![1.5]);
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
        assert_eq!(focused(&game), Some(MenuAction::Resume));
        assert_eq!(labels(&game), ["Resume", "Settings", "Quit"]);
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

    /// Enters `fullscreen` in the open console and runs one frame with the window in `mode`.
    fn fullscreen_frame(game: &mut SearchlightGame, mode: WindowMode) -> Option<WindowMode> {
        type_into_console(game, "fullscreen");
        game.command();
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &clock, None, None).with_window_mode(mode);
        game.frame(&mut ctx, 0.5);
        ctx.requested_window_mode()
    }

    #[test]
    fn the_fullscreen_console_command_flips_the_mode_and_prints_it() {
        for (before, after, text) in [
            (
                WindowMode::Windowed,
                WindowMode::Borderless,
                "Window mode: fullscreen",
            ),
            (
                WindowMode::Borderless,
                WindowMode::Windowed,
                "Window mode: windowed",
            ),
        ] {
            let mut game = turning_game(false);
            let clock = GameClock::default();
            open_the_console(&mut game, &clock);

            let requested = fullscreen_frame(&mut game, before);

            assert_eq!(requested, Some(after), "{before:?}");
            assert_eq!(
                console_lines(&game).last().map(String::as_str),
                Some(text),
                "{before:?}"
            );
        }
    }

    #[test]
    fn the_fullscreen_command_flips_once_per_entry() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);
        fullscreen_frame(&mut game, WindowMode::Windowed);
        let mut ctx = FrameContext::new(TICK, &clock, None, None);

        game.frame(&mut ctx, 0.5);

        assert_eq!(ctx.requested_window_mode(), None);
    }

    #[test]
    fn the_fullscreen_command_saves_the_mode_and_releases_held_keys() {
        use moho_core::prefs::WindowMode as Saved;
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = turning_game(false).with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        open_the_console(&mut game, &clock);

        fullscreen_frame(&mut game, WindowMode::Windowed);

        assert_eq!(saved_window_mode(&path), Some(Saved::Borderless));
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
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

    /// The next-wave post, a metre in front of the player at the origin.
    const POST: (Vec3, Vec3) = (Vec3::new(-0.3, 0.0, 1.0), Vec3::new(0.3, 2.0, 1.4));

    /// A game whose player stands at the origin facing +Z, with a post at each of `controls`.
    fn game_with_controls(controls: &[(Vec3, Vec3)]) -> SearchlightGame {
        let mode = ArenaMode::new(
            arena_with_controls(&[], Vec3::ZERO, &[], &[], controls),
            test_config(rifle_from(mag(30, 5)), vec![]),
            wave_tuning(1),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
    }

    /// What the shell paints after one more frame of `game`, `None` when it paints nothing.
    fn painted(game: &mut SearchlightGame) -> Option<Vec<UiMesh>> {
        let mut renderer = SettingsRenderer::default();
        draw_frame(game, &mut renderer);
        let frame = game.stage.shell().ui_frame([800, 600]);
        frame.map(|frame| frame.meshes)
    }

    /// The load icon's screen text in the colour of `icon`.
    fn load_text(icon: hud::LoadIcon) -> ScreenText {
        ScreenText {
            text: "LOAD".to_owned(),
            anchor: Anchor::BottomLeft,
            offset: LOAD_ICON_OFFSET,
            size: STRIP_SIZE,
            color: load_icon_color(icon),
        }
    }

    /// What a fresh shell paints for `texts`, drawn on a twin game's shell so the glyph atlas
    /// history matches a game that painted nothing before.
    fn painted_texts(texts: Vec<ScreenText>) -> Option<Vec<UiMesh>> {
        let twin = game_with_controls(&[]);
        twin.stage
            .shell()
            .set_texts(texts)
            .expect("valid screen texts");
        let frame = twin.stage.shell().ui_frame([800, 600]);
        frame.map(|frame| frame.meshes)
    }

    /// Whether one more frame of `game` paints anything beyond the load icon a living player
    /// in play always has.
    fn prompt_shows(game: &mut SearchlightGame) -> bool {
        let live = game.screen == Screen::Playing && !game.sim().player_body().is_dead();
        let icon = game.load_icon();
        let paint = painted(game);
        if live {
            game.stage
                .shell()
                .set_texts(vec![load_text(icon)])
                .expect("a valid load icon");
            let load_only = game.stage.shell().ui_frame([800, 600]);
            paint != load_only.map(|frame| frame.meshes)
        } else {
            paint.is_some_and(|meshes| !meshes.is_empty())
        }
    }

    fn turn_around() -> SearchlightCommand {
        SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                yaw_delta: std::f32::consts::PI,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        }
    }

    #[test]
    fn the_prompt_shows_while_the_button_is_in_reach() {
        let mut in_reach = game_with_controls(&[POST]);
        let mut none_placed = game_with_controls(&[]);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        tick_with(&mut in_reach, &mut clock, &SearchlightCommand::default());

        let with_button = prompt_shows(&mut in_reach);
        let without = prompt_shows(&mut none_placed);

        assert!(with_button, "no prompt painted with the button in reach");
        assert!(!without, "a prompt painted with no button placed");
    }

    #[test]
    fn no_prompt_when_looking_away() {
        let mut game = game_with_controls(&[POST]);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        let facing_it = prompt_shows(&mut game);

        tick_with(&mut game, &mut clock, &turn_around());
        let facing_away = prompt_shows(&mut game);

        assert!(facing_it, "no prompt to take away");
        assert!(!facing_away);
    }

    #[test]
    fn no_prompt_while_dead_or_paused() {
        let mut dead = game_with_controls(&[POST]);
        let mut paused = game_with_controls(&[POST]);
        let mut turned_away_paused = game_with_controls(&[POST]);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        for game in [&mut dead, &mut paused, &mut turned_away_paused] {
            tick_with(game, &mut clock, &SearchlightCommand::default());
        }
        let showing = (
            prompt_shows(&mut dead),
            prompt_shows(&mut paused),
            prompt_shows(&mut turned_away_paused),
        );

        dead.mode.sim_mut().kill_player();
        tick_with(&mut dead, &mut clock, &SearchlightCommand::default());
        tick_with(&mut turned_away_paused, &mut clock, &turn_around());
        let away_shows = prompt_shows(&mut turned_away_paused);
        paused.pause();
        turned_away_paused.pause();
        let pause_menu = painted(&mut turned_away_paused);

        assert_eq!(showing, (true, true, true), "no prompt to take away");
        assert!(!away_shows, "the twin still sees the prompt");
        assert!(!prompt_shows(&mut dead), "a dead player sees the prompt");
        assert!(pause_menu.as_ref().is_some_and(|meshes| !meshes.is_empty()));
        assert_eq!(
            painted(&mut paused),
            pause_menu,
            "the pause screen paints a prompt over the menu"
        );
    }

    /// A game with a post in reach whose `Use` action has exactly `bindings`.
    fn game_with_use_bound_to(bindings: Vec<moho_input::bindings::Binding>) -> SearchlightGame {
        let mode = ArenaMode::new(
            arena_with_controls(&[], Vec3::ZERO, &[], &[], &[POST]),
            test_config(rifle_from(mag(30, 5)), vec![]),
            wave_tuning(1),
        );
        let (mut loaded, _) = ActionBindings::load(&BTreeMap::new());
        loaded.set(SearchlightAction::Use, bindings);
        SearchlightGame::new(mode, ActionMap::new(loaded, 1.0))
    }

    /// What a fresh shell paints for the use prompt reading `text`, drawn on a twin game's
    /// shell so the glyph atlas history matches a game that painted nothing before.
    fn painted_prompt_reading(text: &str) -> Option<Vec<UiMesh>> {
        painted_texts(vec![
            ScreenText {
                text: text.to_owned(),
                anchor: Anchor::Center,
                offset: [0.0, PROMPT_OFFSET_Y],
                size: PROMPT_SIZE,
                color: PROMPT_COLOR,
            },
            load_text(hud::LoadIcon::White),
        ])
    }

    #[test]
    fn the_prompt_names_the_first_use_binding_or_unbound() {
        use moho_input::bindings::Binding;
        use moho_input::key::Key;
        let mut rebound = game_with_use_bound_to(vec![Binding::key(Key::G), Binding::key(Key::H)]);
        let mut unbound = game_with_use_bound_to(vec![]);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        for game in [&mut rebound, &mut unbound] {
            tick_with(game, &mut clock, &SearchlightCommand::default());
        }

        let rebound_paint = painted(&mut rebound);
        let unbound_paint = painted(&mut unbound);

        let expected_rebound = painted_prompt_reading("[G] Next wave");
        let expected_unbound = painted_prompt_reading("[unbound] Next wave");
        assert!(expected_rebound.is_some_and(|meshes| !meshes.is_empty()));
        assert_eq!(rebound_paint, painted_prompt_reading("[G] Next wave"));
        assert_eq!(unbound_paint, expected_unbound);
        assert_ne!(rebound_paint, unbound_paint);
    }

    /// A game whose rifle holds an empty magazine and whose three-slot rig holds full ones.
    fn game_with_a_full_rig() -> SearchlightGame {
        let mode = ArenaMode::new(
            arena_with(&[], Vec3::ZERO, &[], &[]),
            test_config(
                rifle_from(mag(30, 0)),
                vec![mag(30, 30), mag(30, 30), mag(30, 30)],
            ),
            wave_tuning(1),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
    }

    /// Ticks `game` through a whole reload, then draws the frame that sees it finish. The
    /// rig is left holding the ejected empty magazine in slot 0 and two full ones.
    fn finish_a_reload(game: &mut SearchlightGame, clock: &mut GameClock) {
        let reload = SearchlightCommand {
            reload: true,
            ..SearchlightCommand::default()
        };
        tick_with(game, clock, &reload);
        for _ in 0..10 {
            tick_with(game, clock, &SearchlightCommand::default());
        }
        assert_eq!(game.sim().pending_feedback(), [Feedback::ReloadDone]);
        draw_frame(game, &mut SettingsRenderer::default());
    }

    fn idle_ticks(game: &mut SearchlightGame, clock: &mut GameClock, count: u32) {
        for _ in 0..count {
            tick_with(game, clock, &SearchlightCommand::default());
        }
    }

    #[test]
    fn reload_done_shows_the_strip() {
        let mut game = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let before = game.strip_marks();

        finish_a_reload(&mut game, &mut clock);

        assert_eq!(before, None, "the strip starts hidden");
        assert_eq!(
            game.strip_marks(),
            Some(vec![
                hud::SlotMark::Empty,
                hud::SlotMark::Present,
                hud::SlotMark::Present
            ])
        );
    }

    #[test]
    fn the_strip_hides_after_the_fade_ticks() {
        let mut game = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        finish_a_reload(&mut game, &mut clock);

        idle_ticks(&mut game, &mut clock, crate::tuning::STRIP_FADE_TICKS - 1);
        let just_before = game.strip_marks();
        idle_ticks(&mut game, &mut clock, 1);
        let at_the_fade = game.strip_marks();
        idle_ticks(&mut game, &mut clock, 30);
        let long_after = game.strip_marks();

        assert!(just_before.is_some(), "hidden one tick early");
        assert_eq!(at_the_fade, None, "still showing at the fade time");
        assert_eq!(long_after, None);
    }

    #[test]
    fn holding_check_gear_keeps_the_strip_until_release() {
        let mut game = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let hold = SearchlightCommand {
            check_gear: true,
            ..SearchlightCommand::default()
        };
        let before = game.strip_marks();

        let mut while_held = Vec::new();
        for _ in 0..crate::tuning::STRIP_FADE_TICKS + 30 {
            tick_with(&mut game, &mut clock, &hold);
            while_held.push(game.strip_marks().is_some());
        }
        idle_ticks(&mut game, &mut clock, 1);
        let after_release = game.strip_marks();

        assert_eq!(before, None, "the strip starts hidden");
        assert!(
            while_held.iter().all(|&showing| showing),
            "hidden while held: {while_held:?}"
        );
        assert_eq!(after_release, None, "still showing after the release");
    }

    /// What a fresh shell paints for the rig strip reading `text`, on a twin game's shell.
    fn painted_strip_reading(text: &str) -> Option<Vec<UiMesh>> {
        painted_texts(vec![
            load_text(hud::LoadIcon::White),
            ScreenText {
                text: text.to_owned(),
                anchor: Anchor::Bottom,
                offset: [0.0, STRIP_OFFSET_Y],
                size: STRIP_SIZE,
                color: STRIP_COLOR,
            },
        ])
    }

    #[test]
    fn the_strip_is_painted_after_a_reload_while_playing() {
        let mut game = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let before = prompt_shows(&mut game);
        finish_a_reload(&mut game, &mut clock);

        let paint = painted(&mut game);

        let expected = painted_strip_reading("[empty] [mag] [mag]");
        assert!(!before, "something painted before any reload");
        assert!(expected.as_ref().is_some_and(|meshes| !meshes.is_empty()));
        assert_eq!(paint, expected);
        assert_ne!(paint, painted_strip_reading("[mag] [mag] [mag]"));
    }

    #[test]
    fn the_strip_is_not_painted_for_a_dead_player() {
        let mut reloaded = game_with_a_full_rig();
        let mut checking = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        finish_a_reload(&mut reloaded, &mut clock);
        let alive = prompt_shows(&mut reloaded);
        let hold = SearchlightCommand {
            check_gear: true,
            ..SearchlightCommand::default()
        };

        reloaded.mode.sim_mut().kill_player();
        idle_ticks(&mut reloaded, &mut clock, 1);
        checking.mode.sim_mut().kill_player();
        tick_with(&mut checking, &mut clock, &hold);

        assert!(alive, "no strip to take away");
        assert!(reloaded.sim().player_body().is_dead());
        assert!(checking.sim().player_body().is_dead());
        assert!(!prompt_shows(&mut reloaded), "painted after a reload");
        assert!(!prompt_shows(&mut checking), "painted for held check gear");
    }

    #[test]
    fn a_restart_hides_the_strip_the_last_life_left_up() {
        let mut game = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        finish_a_reload(&mut game, &mut clock);
        let showing = game.strip_marks().is_some();
        game.mode.sim_mut().kill_player();
        idle_ticks(&mut game, &mut clock, 1);
        let restart = SearchlightCommand {
            restart: true,
            ..SearchlightCommand::default()
        };

        tick_with(&mut game, &mut clock, &restart);

        assert!(showing, "no strip to take away");
        assert!(
            !game.sim().player_body().is_dead(),
            "the restart did not run"
        );
        assert_eq!(game.strip_marks(), None);
    }

    #[test]
    fn a_second_reload_restarts_the_fade_in_full() {
        let mut game = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        finish_a_reload(&mut game, &mut clock);
        idle_ticks(&mut game, &mut clock, crate::tuning::STRIP_FADE_TICKS - 40);

        finish_a_reload(&mut game, &mut clock);
        idle_ticks(&mut game, &mut clock, crate::tuning::STRIP_FADE_TICKS - 1);
        let just_before = game.strip_marks();
        idle_ticks(&mut game, &mut clock, 1);
        let at_the_fade = game.strip_marks();

        assert!(
            just_before.is_some(),
            "the second reload did not renew the fade"
        );
        assert_eq!(at_the_fade, None, "the fades added up");
    }

    #[test]
    fn check_gear_held_into_a_menu_does_not_show_the_strip_on_return() {
        let mut paused = game_with_a_full_rig();
        let mut console = game_with_a_full_rig();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let hold = SearchlightCommand {
            check_gear: true,
            ..SearchlightCommand::default()
        };
        tick_with(&mut paused, &mut clock, &hold);
        tick_with(&mut console, &mut clock, &hold);
        let held = paused.strip_marks().is_some();

        paused.pause();
        paused.enter_play();
        open_the_console(&mut console, &clock);

        assert!(held, "no strip to take away");
        assert_eq!(paused.strip_marks(), None, "pause leaked the hold");
        assert_eq!(console.strip_marks(), None, "the console leaked the hold");
    }

    #[test]
    fn control_mesh_ids_share_no_id_with_arena_dummy_or_enemy_meshes() {
        use crate::arena::{arena_mesh_id, control_mesh_id};

        let controls: Vec<u64> = (0..16).map(|i| control_mesh_id(i).0).collect();
        let arena: Vec<u64> = (0..4096).map(|i| arena_mesh_id(i).0).collect();
        let dummies: Vec<u64> = dummy_segments(&SHIPPED_BODY, &[Vec3::ZERO; 8])
            .iter()
            .map(|segment| segment.id.0)
            .collect();
        let enemies: Vec<u64> = vec![ENEMY_BOX_MESH_ID.0];

        for (name, others) in [("arena", &arena), ("dummy", &dummies), ("enemy", &enemies)] {
            let shared: Vec<&u64> = controls.iter().filter(|id| others.contains(id)).collect();
            assert!(shared.is_empty(), "control ids {shared:?} are {name} ids");
        }
        let mut distinct = controls.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), controls.len());
        assert!(controls.iter().all(|&id| id >= 3 << 32), "{controls:?}");
    }

    /// The view a frame must draw: the sim's player turned by the look shown but not yet ticked.
    fn expected_view(game: &SearchlightGame) -> Mat4 {
        let (look_x, look_y) = game.input.pending_look();
        first_person_view(
            &game.previous,
            &game.sim().player(),
            ALPHA,
            (-look_x, -look_y),
        )
        .view_matrix()
    }

    #[test]
    fn the_drawn_eye_is_the_configured_height_above_the_players_feet() {
        let mut config = test_config(rifle_from(mag(30, 5)), vec![]);
        config.character_body = CharacterBody {
            standing: Stature {
                hull: Hull {
                    shape: HullShape::Capsule,
                    radius: 0.405,
                    length: 1.83,
                },
                eye_height: 1.5,
            },
            crouched: None,
            prone: None,
            lean: moho_physics::Lean::default(),
        };
        let mode = ArenaMode::new(
            arena_with(&[], Vec3::ZERO, &[], &[]),
            config,
            wave_tuning(1),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        for _ in 0..90 {
            tick_once(&mut game);
        }
        let mut renderer = SettingsRenderer::default();

        let view = draw_frame(&mut game, &mut renderer);

        let eye = view.inverse().w_axis.truncate();
        assert!(game.sim().player().grounded, "{:?}", game.sim().player());
        assert!(
            (eye.y - 1.5).abs() < 0.03,
            "the floor is at y = 0, the drawn eye is at {eye:?}"
        );
        assert!(eye.x.abs() < 1e-3 && eye.z.abs() < 1e-3, "{eye:?}");
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

    fn grade_tick(game: &mut SearchlightGame, line: &str) -> Option<Grade> {
        type_into_console(game, line);
        game.command().forced_grade
    }

    #[test]
    fn a_grade_forced_in_the_console_rides_on_every_command() {
        let mut game = game();
        let before = game.command().forced_grade;

        let forced = grade_tick(&mut game, "grade rookie");
        let later: Vec<Option<Grade>> = (0..3).map(|_| game.command().forced_grade).collect();
        let cleared = grade_tick(&mut game, "grade off");
        let after_off: Vec<Option<Grade>> = (0..3).map(|_| game.command().forced_grade).collect();

        assert_eq!(before, None);
        assert_eq!(forced, Some(Grade::Rookie));
        assert_eq!(later, [Some(Grade::Rookie); 3]);
        assert_eq!(cleared, None);
        assert_eq!(after_off, [None; 3]);
    }

    #[test]
    fn an_unknown_grade_leaves_the_force_unchanged() {
        let mut game = game();
        grade_tick(&mut game, "grade professional");

        let after_legend = grade_tick(&mut game, "grade legend");
        let after_bare = grade_tick(&mut game, "grade");

        assert_eq!(after_legend, Some(Grade::Professional));
        assert_eq!(after_bare, Some(Grade::Professional));
        let lines = console_lines(&game);
        let reply = lines.last().expect("the console replied");
        for grade in Grade::LADDER {
            assert!(reply.contains(grade.name()), "{reply:?}");
        }
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
    fn a_saved_simple_starts_the_game_in_simple() {
        let dir = tempfile::tempdir().unwrap();
        let (mut settings, _path) = settings_in(&dir);
        settings.set_simple(true).unwrap();
        let mut game = game().with_debug_settings(settings);

        let command = game.command();

        assert!(command.simple);
    }

    #[test]
    fn a_gravity_scale_set_in_one_session_is_not_restored_by_the_next() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut first = game().with_debug_settings(settings);
        type_into_console(&first, "round_gravity 30");
        type_into_console(&first, "simple 1");
        let set = first.command();
        let (saved, _) = Prefs::load_from(&path);
        let mut second = game().with_debug_settings(DebugSettings::new(saved, path));

        let restarted = second.command();

        assert_eq!(set.round_gravity, GravityScale::new(30.0).unwrap());
        assert!(restarted.simple, "the saved file was not read");
        assert_eq!(restarted.round_gravity, GravityScale::new(1.0).unwrap());
    }

    #[test]
    fn the_simple_command_saves_the_setting_and_bare_simple_prints_the_new_state() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);
        let saved = |path: &std::path::Path| {
            let (saved, _) = Prefs::load_from(path);
            saved.debug_value("simple").map(str::to_owned)
        };

        type_into_console(&game, "simple");
        let toggled_on = game.command().simple;
        let on = saved(&path);
        type_into_console(&game, "simple 0");
        let set_off = game.command().simple;
        let off = saved(&path);

        assert!(toggled_on);
        assert_eq!(on.as_deref(), Some("1"));
        assert!(!set_off);
        assert_eq!(off.as_deref(), Some("0"));
        let lines = console_lines(&game);
        assert!(lines.contains(&"Simple: on".to_owned()), "{lines:?}");
    }

    #[test]
    fn the_gravity_scale_starts_at_1_and_both_levels_survive_a_restart() {
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let at_launch = game.command();
        type_into_console(&game, "round_gravity 30");
        type_into_console(&game, "simple 1");
        let set = game.command();
        game.mode.sim_mut().kill_player();
        assert!(game.sim().player_body().is_dead());

        let restart = SearchlightCommand {
            restart: true,
            ..game.command()
        };
        tick_with(&mut game, &mut clock, &restart);
        let after = game.command();

        assert!(!at_launch.simple);
        assert_eq!(at_launch.round_gravity, GravityScale::new(1.0).unwrap());
        assert!(set.simple);
        assert_eq!(set.round_gravity, GravityScale::new(30.0).unwrap());
        assert!(!game.sim().player_body().is_dead());
        assert!(after.simple);
        assert_eq!(after.round_gravity, GravityScale::new(30.0).unwrap());
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
    use moho_ui_shell::{MenuEvent, MenuItem};

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

    /// The action of the focused row of the top menu screen, read by pressing Enter on the shell
    /// directly so the game never answers it; `None` when no screen is open.
    fn focused(game: &SearchlightGame) -> Option<MenuAction> {
        let mut shell = game.stage.shell();
        shell.route(Input::Key(Key::Enter), true, false);
        shell
            .menu_mut()
            .take_events()
            .into_iter()
            .find_map(|event| match event {
                MenuEvent::Chosen(action) => Some(action),
                MenuEvent::SettingsApplied(_)
                | MenuEvent::GameSettingsApplied(_)
                | MenuEvent::Captured { .. }
                | MenuEvent::DetailsToggled { .. }
                | MenuEvent::Picked { .. }
                | MenuEvent::ModeChosen { .. }
                | MenuEvent::Cleared { .. } => None,
                MenuEvent::MoveRequested { .. } => None,
                MenuEvent::ActionPicked { .. } => None,
            })
    }

    /// The labels of the top menu screen's rows.
    fn labels(game: &SearchlightGame) -> Vec<String> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Button { label, .. } => Some(label.clone()),
                MenuItem::Setting(_)
                | MenuItem::Apply
                | MenuItem::Control { .. }
                | MenuItem::Details { .. }
                | MenuItem::GameSlider(_) => None,
            })
            .collect()
    }

    /// The title of the top menu screen.
    fn title(game: &SearchlightGame) -> Option<String> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen.title.clone()
    }

    /// Where row `row` of the top menu screen is drawn, in physical pixels; the test window is
    /// 1280 by 720 at scale 1.
    fn row_point(game: &SearchlightGame, row: usize) -> [f64; 2] {
        let rows = labels(game).len();
        assert!(row < rows, "no menu row {row} among {rows}");
        [640.0, 360.0 + (row as f64 - (rows - 1) as f64 / 2.0) * 56.0]
    }

    fn move_cursor(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, at: [f64; 2]) {
        game.event(
            ctx,
            Event::Window(WindowEvent::CursorMoved {
                device_id: winit::event::DeviceId::dummy(),
                position: winit::dpi::PhysicalPosition::new(at[0], at[1]),
            }),
        );
    }

    fn click_at(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, at: [f64; 2]) {
        let left = Input::Mouse(MouseButton::Left);
        move_cursor(game, ctx, at);
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

        assert_eq!(focused(&game), Some(MenuAction::Quit));
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
        assert_eq!(focused(&game), Some(MenuAction::Quit));

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

        move_cursor(&mut game, &mut ctx, [0.0, 0.0]);
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

        assert_eq!(routing, Routing::Menu);
        assert!(!ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
        assert_eq!(focused(&game), Some(MenuAction::Play));
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
        let settings_title = title(&game);
        // Back is the last of the categories' seven rows, which all fit at 720 points.
        press_keys(&mut game, &mut ctx, &[Key::ArrowUp]);
        let back = [640.0, 528.0];
        click_at(&mut game, &mut ctx, back);
        game.event(&mut ctx, motion());

        assert_eq!(settings_title.as_deref(), Some("Settings"));
        assert_eq!(title(&game), None, "Back shows the main menu again");
        assert_eq!(focused(&game), Some(MenuAction::Settings));
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
        assert_eq!(focused(&game), Some(MenuAction::Quit));

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
        move_cursor(&mut game, &mut ctx, quit);

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

        assert_eq!(focused(&game), Some(MenuAction::Settings));
    }

    #[test]
    fn moving_the_cursor_during_play_opens_and_selects_no_menu_item() {
        let mut game = game_with_a_moving_enemy();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        move_cursor(&mut game, &mut ctx, [640.0, 360.0 + 56.0]);

        assert_eq!(game.screen, Screen::Playing);
        assert!(!game.stage.shell().menu_mut().is_open());
    }

    #[test]
    fn moving_the_cursor_over_a_menu_item_selects_it() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let quit = row_point(&game, 2);

        move_cursor(&mut game, &mut ctx, quit);

        assert_eq!(focused(&game), Some(MenuAction::Quit));
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

        move_cursor(&mut game, &mut ctx, [0.0, 0.0]);
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

        assert_eq!(focused(&game), Some(MenuAction::Quit));
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

        assert_eq!(title(&game).as_deref(), Some("Settings"));
        assert_eq!(game.screen, Screen::Paused);
        assert!(!ctx.exit_requested());
        assert!(!game.stage.cursor_captured);
        assert_eq!(ctx.requested_paused(), Some(true));
        assert_eq!(world_of(&game), world);
    }

    #[test]
    fn escape_while_dead_pauses_and_restart_works_only_after_resume() {
        let (mut game, clock) = game_with_a_player_dead_away_from_the_spawn();
        let mut ctx = EventContext::new(&clock, None, None);

        pause(&mut game, &mut ctx);
        let enter = Input::Key(Key::Enter);
        game.route_input(&mut ctx, enter, true, false);
        game.route_input(&mut ctx, enter, false, false);
        let after_resume = game.command();
        game.route_input(&mut ctx, Input::Key(Key::F8), true, false);
        let restart = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert!(
            !after_resume.restart,
            "the Enter that resumed also restarted"
        );
        assert!(restart.restart, "F8 restarts once playing again");
        assert!(!ctx.exit_requested());
    }

    fn head_bob_level(percent: i32) -> HeadBobLevel {
        HeadBobLevel::new(percent).expect("a percentage on the grid")
    }

    /// The flat arena with no enemies, on its main menu, so no enemy walks into the player.
    fn quiet_menu_game() -> SearchlightGame {
        game().with_main_menu()
    }

    /// A game on the flat arena at `percent` head bob, standing at the origin facing +Z.
    fn bobbing_game(percent: i32) -> SearchlightGame {
        let mut game = game();
        game.head_bob = head_bob_level(percent);
        game
    }

    fn walk_command() -> SearchlightCommand {
        SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                forward: 1.0,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        }
    }

    /// Lets the player fall onto the floor and stand still.
    fn settle(game: &mut SearchlightGame, clock: &mut GameClock) {
        for _ in 0..90 {
            tick_with(game, clock, &SearchlightCommand::default());
        }
        assert!(game.sim().player().grounded, "{:?}", game.sim().player());
    }

    fn eye_of(view: Mat4) -> Vec3 {
        view.inverse().w_axis.truncate()
    }

    /// Draws a frame at `ALPHA` and returns how far its eye is from the unbobbed eye: the sim's
    /// player blended between the last two ticks, with no lean on these games.
    fn bob_of(game: &mut SearchlightGame, renderer: &mut SettingsRenderer) -> f32 {
        let drawn = eye_of(draw_frame(game, renderer));
        let unbobbed = eye_of(expected_view(game));
        drawn.distance(unbobbed)
    }

    fn horizontal_speed(player: &CharacterState) -> f32 {
        Vec3::new(player.velocity.x, 0.0, player.velocity.z).length()
    }

    #[test]
    fn walking_at_full_head_bob_moves_the_drawn_eye_over_a_step_cycle() {
        let mut game = bobbing_game(100);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);

        let mut most = 0.0_f32;
        for _ in 0..120 {
            tick_with(&mut game, &mut clock, &walk_command());
            most = most.max(bob_of(&mut game, &mut renderer));
        }

        let player = game.sim().player();
        assert!(
            player.grounded && horizontal_speed(&player) > 3.0,
            "the player is walking: {player:?}"
        );
        assert!(most > 0.01, "the eye never left the unbobbed eye: {most} m");
        assert!(most < 0.1, "the bob is {most} m, far from a footstep");
    }

    #[test]
    fn a_dead_player_draws_the_unbobbed_eye_even_after_walking_when_killed() {
        let mut game = bobbing_game(100);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);
        for _ in 0..40 {
            tick_with(&mut game, &mut clock, &walk_command());
        }
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &walk_command());
        assert!(game.sim().player_body().is_dead());

        let mut most = 0.0_f32;
        for _ in 0..60 {
            tick_with(&mut game, &mut clock, &walk_command());
            most = most.max(bob_of(&mut game, &mut renderer));
        }

        assert!(most < 1e-4, "the dead view bobbed {most} m");
    }

    #[test]
    fn a_jumping_player_draws_the_unbobbed_eye_while_airborne() {
        let mut game = bobbing_game(100);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);
        for _ in 0..40 {
            tick_with(&mut game, &mut clock, &walk_command());
        }
        let jump = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                jump: true,
                ..walk_command().movement
            },
            ..SearchlightCommand::default()
        };

        let (mut judged, mut most, mut slowest) = (0, 0.0_f32, f32::MAX);
        let mut airborne_before = false;
        for tick in 0..120 {
            let command = if tick == 0 { jump } else { walk_command() };
            tick_with(&mut game, &mut clock, &command);
            let player = game.sim().player();
            let airborne = !player.grounded;
            let bob = bob_of(&mut game, &mut renderer);
            // The first airborne frame still blends from the last footstep, and the landing
            // frame blends toward the next; only frames between two airborne ticks are pure.
            if airborne && airborne_before {
                judged += 1;
                most = most.max(bob);
                slowest = slowest.min(horizontal_speed(&player));
            }
            airborne_before = airborne;
        }

        assert!(judged > 20, "the jump only had {judged} airborne frames");
        assert!(
            slowest > 3.0,
            "the jumper slowed to {slowest} m/s in the air"
        );
        assert!(most < 1e-4, "the airborne eye bobbed {most} m");
    }

    /// Opens Settings from the main menu, whose Settings row is its second.
    fn open_settings(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        press_keys(game, ctx, &[Key::ArrowDown, Key::Enter]);
        assert_eq!(title(game).as_deref(), Some("Settings"));
    }

    /// Focuses the first row of the top screen that `wanted` picks, scrolling it into view.
    fn focus_row(
        game: &SearchlightGame,
        what: &str,
        wanted: impl Fn(&MenuItem<MenuAction>) -> bool,
    ) {
        const SIZE: [f32; 2] = [1280.0, 720.0];
        let mut shell = game.stage.shell();
        let menu = shell.menu_mut();
        let screen = menu.top().expect("a menu screen is open");
        let row = screen
            .items
            .iter()
            .position(wanted)
            .unwrap_or_else(|| panic!("the screen lists no {what} row"));
        let first = moho_ui_shell::menu::layout(screen, SIZE)[0];
        let centre = [
            (first.min[0] + first.max[0]) / 2.0,
            (first.min[1] + first.max[1]) / 2.0,
        ];
        menu.scroll(-1000.0);
        menu.hover(centre, SIZE);
        for _ in 0..row {
            menu.on_key(Input::Key(Key::ArrowDown), true, false);
        }
    }

    fn focus_head_bob_slider(game: &SearchlightGame) {
        focus_row(
            game,
            "Head bob",
            |item| matches!(item, MenuItem::GameSlider(slider) if slider.key == HeadBobLevel::KEY),
        );
    }

    fn focus_apply(game: &SearchlightGame) {
        focus_row(game, "Apply", |item| matches!(item, MenuItem::Apply));
    }

    fn focus_back(game: &SearchlightGame) {
        focus_row(
            game,
            "Back",
            |item| matches!(item, MenuItem::Button { label, .. } if label == "Back"),
        );
    }

    /// The value the Head bob slider on the top screen starts from, `None` without that row.
    fn head_bob_on_the_settings_screen(game: &SearchlightGame) -> Option<i32> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen.items.iter().find_map(|item| match item {
            MenuItem::GameSlider(slider) if slider.key == HeadBobLevel::KEY => Some(slider.value),
            _ => None,
        })
    }

    /// A game started fresh on its main menu from the prefs file at `path`.
    fn relaunched(path: &Path) -> SearchlightGame {
        let (prefs, warnings) = Prefs::load_from(path);
        assert_eq!(warnings, vec![]);
        quiet_menu_game().with_debug_settings(DebugSettings::new(prefs, path.to_path_buf()))
    }

    #[test]
    fn head_bob_slider_at_zero_applied_keeps_the_walking_view_still() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Game");
        focus_head_bob_slider(&game);
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 10]);
        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        game.enter_play();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);

        let mut most = 0.0_f32;
        for _ in 0..120 {
            tick_with(&mut game, &mut clock, &walk_command());
            most = most.max(bob_of(&mut game, &mut renderer));
        }

        assert_eq!(game.head_bob, head_bob_level(0));
        assert!(horizontal_speed(&game.sim().player()) > 3.0, "walking");
        assert!(most < 1e-4, "the view bobbed {most} m at 0 %");
    }

    /// A game with a dummy 20 m down the +Z range, at `percent` head bob.
    fn range_game(percent: i32) -> SearchlightGame {
        let mut arena = (*arena_with(&[], Vec3::ZERO, &[], &[])).clone();
        arena.dummy_spawns = vec![Vec3::new(0.0, 0.0, 20.0)];
        let mode = ArenaMode::without_waves(
            Arc::new(arena),
            test_config(rifle_from(mag(30, 30)), vec![]),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        game.head_bob = head_bob_level(percent);
        game
    }

    /// Every hit of a walk toward the dummy that fires every sixth tick, with the tick it
    /// landed on, and the most the drawn eye left the unbobbed eye.
    fn walk_and_fire(percent: i32) -> (Vec<(usize, Feedback)>, f32) {
        let mut game = range_game(percent);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);
        game.mode.drain_feedback();

        let mut hits = Vec::new();
        let mut most = 0.0_f32;
        for tick in 0..90 {
            let command = SearchlightCommand {
                fire: tick % 6 == 0,
                simple: true,
                ..walk_command()
            };
            tick_with(&mut game, &mut clock, &command);
            hits.extend(
                game.mode
                    .drain_feedback()
                    .into_iter()
                    .filter(|feedback| matches!(feedback, Feedback::Hit { .. }))
                    .map(|feedback| (tick, feedback)),
            );
            most = most.max(bob_of(&mut game, &mut renderer));
        }
        (hits, most)
    }

    #[test]
    fn shots_and_hits_match_at_zero_and_full_head_bob() {
        let (still_hits, still_bob) = walk_and_fire(0);
        let (bobbing_hits, bobbing_bob) = walk_and_fire(100);

        assert!(!bobbing_hits.is_empty(), "no shot ever hit the dummy");
        assert_eq!(still_hits, bobbing_hits);
        assert!(still_bob < 1e-4, "the view bobbed {still_bob} m at 0 %");
        assert!(bobbing_bob > 0.01, "the view never bobbed at 100 %");
    }

    #[test]
    fn bob_moves_only_the_drawn_eye_never_the_view_direction() {
        let mut game = turning_game(false);
        game.head_bob = head_bob_level(100);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);

        let (mut turned, mut off_course, mut bobbed) = (0.0_f32, 0.0_f32, 0.0_f32);
        for _ in 0..120 {
            tick_with(&mut game, &mut clock, &walk_command());
            mouse(&mut game, 5.0, 2.0);
            let drawn = draw_frame(&mut game, &mut renderer);
            let unbobbed = expected_view(&game);
            turned = turned.max(forward_of(drawn).distance(Vec3::Z));
            off_course = off_course.max(forward_of(drawn).distance(forward_of(unbobbed)));
            bobbed = bobbed.max(eye_of(drawn).distance(eye_of(unbobbed)));
        }

        assert!(turned > 0.01, "the view never turned");
        assert!(bobbed > 0.01, "the eye never bobbed");
        assert!(off_course < 1e-4, "the bob turned the view by {off_course}");
    }

    #[test]
    fn head_bob_set_from_the_console_loads_after_a_relaunch_and_shows_in_settings() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        type_into_console(&game, "head_bob 25");
        game.command();

        let mut relaunched = relaunched(&path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut relaunched, &mut ctx);
        choose_page(&mut relaunched, &mut ctx, "Game");

        assert_eq!(relaunched.head_bob, head_bob_level(25));
        assert_eq!(head_bob_on_the_settings_screen(&relaunched), Some(25));
    }

    #[test]
    fn head_bob_applied_from_settings_loads_after_a_relaunch_and_shows_in_settings() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Game");
        focus_head_bob_slider(&game);
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 5]);
        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        let mut relaunched = relaunched(&path);
        open_settings(&mut relaunched, &mut ctx);
        choose_page(&mut relaunched, &mut ctx, "Game");

        assert_eq!(game.head_bob, head_bob_level(25));
        assert_eq!(relaunched.head_bob, head_bob_level(25));
        assert_eq!(head_bob_on_the_settings_screen(&relaunched), Some(25));
    }

    #[test]
    fn back_after_moving_the_head_bob_slider_keeps_the_saved_value_and_reopens_at_it() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Game");
        focus_head_bob_slider(&game);
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 10]);

        focus_back(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        focus_back(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let after_back = (title(&game), game.head_bob);
        let saved_by_back = path.exists();
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        choose_page(&mut game, &mut ctx, "Game");
        let reopened = head_bob_on_the_settings_screen(&game);

        assert_eq!(after_back, (None, head_bob_level(50)));
        assert!(!saved_by_back, "Back saved the discarded change");
        assert_eq!(reopened, Some(50));
    }

    #[test]
    fn a_head_bob_set_from_the_console_shows_on_the_slider() {
        let mut game = quiet_menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        type_into_console(&game, "head_bob 30");
        game.command();
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Game");

        assert_eq!(game.head_bob, head_bob_level(30));
        assert_eq!(head_bob_on_the_settings_screen(&game), Some(30));
    }

    #[test]
    fn a_refused_head_bob_keeps_the_previous_value() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);
        type_into_console(&game, "head_bob 25");
        game.command();

        for line in ["head_bob 150", "head_bob 23", "head_bob x", "head_bob 5 10"] {
            type_into_console(&game, line);
            game.command();

            let lines = console_lines(&game);
            assert_eq!(
                lines.last().map(String::as_str),
                Some("Usage: head_bob [percent] (0 to 100, steps of 5)"),
                "{line}"
            );
            assert_eq!(game.head_bob, head_bob_level(25), "{line}");
        }

        let (saved, _) = Prefs::load_from(&path);
        assert_eq!(saved.game_value("head_bob"), Some("25"));
    }

    #[test]
    fn bare_head_bob_prints_the_current_percent() {
        let mut game = game();

        type_into_console(&game, "head_bob");
        game.command();
        type_into_console(&game, "head_bob 25");
        game.command();
        type_into_console(&game, "head_bob");
        game.command();

        let lines = console_lines(&game);
        assert_eq!(
            lines[lines.len() - 6..],
            [
                "> head_bob",
                "Head bob: 50 %",
                "> head_bob 25",
                "Head bob: 25 %",
                "> head_bob",
                "Head bob: 25 %"
            ]
        );
    }

    #[test]
    fn a_failed_head_bob_save_keeps_the_value_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));
        let mut game = game().with_debug_settings(settings);

        type_into_console(&game, "head_bob 15");
        game.command();

        assert_eq!(game.head_bob, head_bob_level(15));
    }

    #[test]
    fn game_settings_applied_without_head_bob_leaves_it_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        type_into_console(&game, "head_bob 25");
        game.command();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.stage
            .shell()
            .menu_mut()
            .push(moho_ui_shell::MenuScreen {
                title: Some("Other".to_owned()),
                items: vec![
                    MenuItem::GameSlider(moho_ui_shell::GameSlider {
                        key: "sway".to_owned(),
                        label: "Sway".to_owned(),
                        min: 0,
                        max: 100,
                        step: 5,
                        unit: "%".to_owned(),
                        value: 20,
                    }),
                    MenuItem::Apply,
                ],
                back: None,
                settings: None,
            });

        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        assert_eq!(game.head_bob, head_bob_level(25));
        let (saved, _) = Prefs::load_from(&path);
        assert_eq!(saved.game_value("head_bob"), Some("25"));
    }

    /// Opens the Settings page labelled `page` from the categories screen.
    fn choose_page(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, page: &str) {
        focus_row(
            game,
            page,
            |item| matches!(item, MenuItem::Button { label, .. } if label == page),
        );
        press_keys(game, ctx, &[Key::Enter]);
        assert_eq!(title(game).as_deref(), Some(page));
    }

    fn focus_engine_row(game: &SearchlightGame, setting: moho_ui_shell::EngineSetting) {
        focus_row(game, setting.label(), |item| {
            *item == MenuItem::Setting(setting)
        });
    }

    fn focus_game_slider(game: &SearchlightGame, key: &str) {
        focus_row(
            game,
            key,
            |item| matches!(item, MenuItem::GameSlider(slider) if slider.key == key),
        );
    }

    /// The engine settings the top screen stages.
    fn staged_settings(game: &SearchlightGame) -> EngineSettings {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen.settings.expect("the page stages engine settings")
    }

    fn game_with_prefs(prefs: Prefs, path: &std::path::Path) -> SearchlightGame {
        quiet_menu_game().with_debug_settings(DebugSettings::new(prefs, path.to_path_buf()))
    }

    /// Records the projection of every frame begun; the draw calls do nothing.
    #[derive(Default)]
    struct ProjectionRenderer {
        projections: Vec<Mat4>,
    }

    impl RendererBackend for ProjectionRenderer {
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
            self.projections.push(camera.1);
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            quality
        }
        fn set_ssao_quality(&mut self, _quality: u8) {}
    }

    /// The vertical field of view, in degrees, the game's next arena frame is drawn with.
    fn drawn_fov_degrees(game: &mut SearchlightGame) -> f32 {
        let mut renderer = ProjectionRenderer::default();
        game.stage.draw(&mut renderer, (Mat4::IDENTITY, Vec3::ZERO));
        let projection = renderer.projections.pop().expect("a frame began");
        (1.0 / projection.y_axis.y).atan().to_degrees() * 2.0
    }

    #[test]
    fn settings_opens_on_the_six_categories_in_order() {
        let mut game = quiet_menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        open_settings(&mut game, &mut ctx);

        assert_eq!(
            labels(&game),
            [
                "Controls", "Mouse", "Audio", "Graphics", "Display", "Game", "Back"
            ]
        );
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("Settings is open");
        assert_eq!(screen.items.len(), 7, "no setting row is shown yet");
        assert_eq!(screen.settings, None);
    }

    #[test]
    fn back_from_a_page_without_apply_leaves_the_saved_value() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut prefs = Prefs::default();
        *prefs.music_volume_mut() = 4.0;
        let mut game = game_with_prefs(prefs, &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Audio");
        focus_engine_row(&game, moho_ui_shell::EngineSetting::MusicVolume);
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight, Key::ArrowRight]);

        focus_back(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let after_back = title(&game);
        choose_page(&mut game, &mut ctx, "Audio");

        assert_eq!(after_back.as_deref(), Some("Settings"));
        assert_eq!(staged_settings(&game).music_volume, 4.0);
        let saved = game.debug_settings.as_ref().unwrap().prefs();
        assert_eq!(saved.music_volume(), 4.0);
        assert!(!path.exists(), "nothing was applied, so nothing was saved");
    }

    #[test]
    fn applying_mouse_leaves_graphics_values_as_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut prefs = Prefs::default();
        prefs.set_shadow_quality(1);
        prefs.set_ssao_quality(2);
        prefs.set_effects_quality(1);
        let mut game = game_with_prefs(prefs, &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let before = staged_sensitivity(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Mouse");
        focus_engine_row(&game, moho_ui_shell::EngineSetting::MouseSensitivity);
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight; 5]);
        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        focus_back(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        choose_page(&mut game, &mut ctx, "Graphics");

        let shown = staged_settings(&game);
        assert_eq!(
            (
                shown.shadow_quality,
                shown.ssao_quality,
                shown.effects_quality
            ),
            (1, 2, 1)
        );
        assert!(shown.mouse_sensitivity > before, "the Mouse apply is kept");
        let (reloaded, _) = Prefs::load_from(&path);
        assert_eq!(
            (
                reloaded.shadow_quality(),
                reloaded.ssao_quality(),
                reloaded.effects_quality()
            ),
            (1, 2, 1)
        );
        assert!(reloaded.mouse_sensitivity() > before);
    }

    /// The sensitivity the Mouse page starts from, leaving Settings on its categories.
    fn staged_sensitivity(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) -> f32 {
        open_settings(game, ctx);
        choose_page(game, ctx, "Mouse");
        let sensitivity = staged_settings(game).mouse_sensitivity;
        focus_back(game);
        press_keys(game, ctx, &[Key::Enter]);
        sensitivity
    }

    #[test]
    fn escape_on_a_page_returns_to_the_categories() {
        let mut game = quiet_menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Audio");

        press_keys(&mut game, &mut ctx, &[Key::Escape]);

        assert_eq!(title(&game).as_deref(), Some("Settings"));
        assert_eq!(
            labels(&game),
            [
                "Controls", "Mouse", "Audio", "Graphics", "Display", "Game", "Back"
            ]
        );
    }

    #[test]
    fn applied_field_of_view_sets_the_projection_and_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut game = game_with_prefs(Prefs::default(), &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let before = drawn_fov_degrees(&mut game);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Display");
        focus_game_slider(&game, "fov");

        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 15]);
        let unapplied = drawn_fov_degrees(&mut game);
        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        assert!(
            (before - 75.0).abs() < 1e-3,
            "starts at 75 degrees: {before}"
        );
        assert!((unapplied - 75.0).abs() < 1e-3, "unapplied: {unapplied}");
        let applied = drawn_fov_degrees(&mut game);
        assert!((applied - 60.0).abs() < 1e-3, "applied: {applied}");
        let (reloaded, _) = Prefs::load_from(&path);
        assert_eq!(reloaded.game_value("fov"), Some("60"));
        let mut relaunched = relaunched(&path);
        let after_relaunch = drawn_fov_degrees(&mut relaunched);
        assert!(
            (after_relaunch - 60.0).abs() < 1e-3,
            "after a relaunch: {after_relaunch}"
        );
    }

    #[test]
    fn applying_display_keeps_the_crosshair_opacity_and_head_bob() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut prefs = Prefs::default();
        prefs.set_game_value("crosshair_opacity", "40").unwrap();
        prefs.set_game_value("head_bob", "25").unwrap();
        let mut game = game_with_prefs(prefs, &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Display");
        focus_game_slider(&game, "fov");
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 5]);

        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        let (reloaded, _) = Prefs::load_from(&path);
        assert_eq!(reloaded.game_value("fov"), Some("70"));
        assert_eq!(reloaded.game_value("crosshair_opacity"), Some("40"));
        assert_eq!(reloaded.game_value("head_bob"), Some("25"));
        assert_eq!(game.head_bob, head_bob_level(25));
    }

    #[test]
    fn applying_game_saves_the_crosshair_opacity_and_keeps_the_field_of_view() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut prefs = Prefs::default();
        prefs.set_game_value("fov", "60").unwrap();
        prefs.set_game_value("head_bob", "25").unwrap();
        let mut game = game_with_prefs(prefs, &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Game");
        focus_game_slider(&game, "crosshair_opacity");
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 3]);

        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        let (reloaded, _) = Prefs::load_from(&path);
        assert_eq!(reloaded.game_value("crosshair_opacity"), Some("70"));
        assert_eq!(reloaded.game_value("fov"), Some("60"));
        assert_eq!(reloaded.game_value("head_bob"), Some("25"));
        assert_eq!(game.head_bob, head_bob_level(25));
        let drawn = drawn_fov_degrees(&mut game);
        assert!((drawn - 60.0).abs() < 1e-3, "the view stays at 60: {drawn}");
    }

    #[test]
    fn crosshair_at_zero_opacity_draws_no_bars_but_keeps_edge_bars() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut hidden_prefs = Prefs::default();
        hidden_prefs
            .set_game_value("crosshair_opacity", "0")
            .unwrap();
        let hidden = game().with_debug_settings(DebugSettings::new(hidden_prefs, path.clone()));
        let shown = game().with_debug_settings(DebugSettings::new(Prefs::default(), path));
        let frame_of = |game: &SearchlightGame| {
            let mut crosshair = game.stage.crosshair.lock().unwrap();
            crosshair.dead = true;
            crosshair.edges = [100, 0, 0, 0];
            crosshair.ui_frame([800, 600]).expect("a frame")
        };

        let without_bars = frame_of(&hidden);
        let with_bars = frame_of(&shown);

        let [r, g, b] = EDGE_RGB;
        let mesh = &without_bars.meshes[0];
        assert_eq!(
            mesh.vertices.len(),
            8,
            "the edge bar and the death tint, no crosshair bars"
        );
        assert_eq!(mesh.indices.len(), 12);
        assert!(mesh.vertices[..4].iter().all(|v| v.color == [r, g, b, 100]));
        assert_eq!(
            with_bars.meshes[0].vertices.len(),
            24,
            "at full opacity the bars show"
        );
    }

    #[test]
    fn crosshair_bars_scale_their_alpha_with_the_opacity_and_keep_white() {
        let bar_colors = |opacity: i32| {
            let mut crosshair = Crosshair {
                opacity,
                ..Crosshair::default()
            };
            let frame = crosshair.ui_frame([800, 600]).expect("a frame");
            frame.meshes[0]
                .vertices
                .iter()
                .map(|vertex| vertex.color)
                .collect::<Vec<_>>()
        };

        for (opacity, alpha) in [(100, 255), (50, 128), (10, 26)] {
            let colors = bar_colors(opacity);

            assert_eq!(colors.len(), 16, "opacity {opacity}");
            assert!(
                colors.iter().all(|&color| color == [255, 255, 255, alpha]),
                "opacity {opacity}: {colors:?}"
            );
        }
    }

    #[test]
    fn a_saved_field_of_view_draws_the_first_frame_and_sizes_the_crosshair_gap() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut prefs = Prefs::default();
        prefs.set_game_value("fov", "50").unwrap();
        let mut saved = game().with_debug_settings(DebugSettings::new(prefs, path.clone()));
        let mut default = game().with_debug_settings(DebugSettings::new(Prefs::default(), path));
        let spread = 0.05_f32;
        let first_bar_left = |game: &SearchlightGame| {
            let mut crosshair = game.stage.crosshair.lock().unwrap();
            crosshair.spread = spread;
            let frame = crosshair.ui_frame([800, 600]).expect("a frame");
            frame.meshes[0].vertices[0].pos[0]
        };

        let drawn_saved = drawn_fov_degrees(&mut saved);
        let drawn_default = drawn_fov_degrees(&mut default);
        let saved_left = first_bar_left(&saved);
        let default_left = first_bar_left(&default);

        assert!((drawn_saved - 50.0).abs() < 1e-3, "saved: {drawn_saved}");
        assert!(
            (drawn_default - 75.0).abs() < 1e-3,
            "default: {drawn_default}"
        );
        let left_at =
            |fov: f32| 400.0 - Crosshair::gap(spread, fov.to_radians(), 600.0) - Crosshair::LENGTH;
        assert!((saved_left - left_at(50.0)).abs() < 1e-3, "{saved_left}");
        assert!(
            (default_left - left_at(75.0)).abs() < 1e-3,
            "{default_left}"
        );
        assert!(
            saved_left < default_left,
            "a narrower view opens the gap wider"
        );
    }

    #[test]
    fn applying_the_game_page_after_display_keeps_the_applied_field_of_view() {
        let dir = tempfile::tempdir().unwrap();
        let (_, path) = settings_in(&dir);
        let mut prefs = Prefs::default();
        prefs.set_game_value("crosshair_opacity", "40").unwrap();
        prefs.set_game_value("head_bob", "25").unwrap();
        let mut game = game_with_prefs(prefs, &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings(&mut game, &mut ctx);
        choose_page(&mut game, &mut ctx, "Display");
        focus_game_slider(&game, "fov");
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 5]);
        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let after_display = Prefs::load_from(&path).0;

        focus_back(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        choose_page(&mut game, &mut ctx, "Game");
        focus_game_slider(&game, "crosshair_opacity");
        press_keys(&mut game, &mut ctx, &[Key::ArrowLeft; 2]);
        focus_game_slider(&game, "head_bob");
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight; 2]);
        focus_apply(&game);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let after_game = Prefs::load_from(&path).0;

        let stored = |prefs: &Prefs| {
            ["fov", "crosshair_opacity", "head_bob"]
                .map(|key| prefs.game_value(key).map(str::to_owned))
        };
        assert_eq!(
            stored(&after_display),
            [Some("70"), Some("40"), Some("25")].map(|value| value.map(str::to_owned)),
            "a Display apply writes the other rows as they were"
        );
        assert_eq!(
            stored(&after_game),
            [Some("70"), Some("20"), Some("35")].map(|value| value.map(str::to_owned))
        );
        let drawn = drawn_fov_degrees(&mut game);
        assert!((drawn - 70.0).abs() < 1e-3, "live view: {drawn}");
        assert_eq!(game.head_bob, head_bob_level(35));
    }

    #[test]
    fn respawn_resets_the_bob() {
        let mut game = bobbing_game(100);
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = SettingsRenderer::default();
        settle(&mut game, &mut clock);
        let mut most = 0.0_f32;
        for _ in 0..60 {
            tick_with(&mut game, &mut clock, &walk_command());
            most = most.max(bob_of(&mut game, &mut renderer));
        }
        assert!(most > 0.01, "the walk never bobbed before the death");
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &walk_command());
        assert!(game.sim().player_body().is_dead());

        tick_with(&mut game, &mut clock, &restart_command());

        assert!(!game.sim().player_body().is_dead());
        let standing = game.sim().player().eye_position();
        for alpha in [0.0, 0.5] {
            let mut renderer = SettingsRenderer::default();
            let ctx_clock = GameClock::default();
            let mut ctx = FrameContext::new(TICK, &ctx_clock, Some(&mut renderer), None);
            game.frame(&mut ctx, alpha);
            let eye = eye_of(*renderer.views.last().expect("a frame began"));

            assert!(
                eye.distance(standing) < 1e-3,
                "alpha {alpha}: the eye is {eye:?}, standing is {standing:?}"
            );
        }
    }

    use moho_input::Action;
    use moho_input::bindings::Binding;

    use crate::controls::label;

    type ControlRow = ([Option<Binding>; 2], Option<String>);

    fn is_button(wanted: &'static str) -> impl Fn(&MenuItem<MenuAction>) -> bool {
        move |item| matches!(item, MenuItem::Button { label, .. } if label == wanted)
    }

    fn is_apply(item: &MenuItem<MenuAction>) -> bool {
        matches!(item, MenuItem::Apply) || is_button("Apply")(item)
    }

    /// Focuses the first row `wanted` picks on the top screen and presses Enter on it.
    fn press_item(
        game: &mut SearchlightGame,
        ctx: &mut EventContext<'_>,
        what: &str,
        wanted: impl Fn(&MenuItem<MenuAction>) -> bool,
    ) {
        focus_row(game, what, wanted);
        press_keys(game, ctx, &[Key::Enter]);
    }

    fn focus_control(game: &SearchlightGame, action: SearchlightAction) {
        focus_row(
            game,
            "control",
            |item| matches!(item, MenuItem::Control { label: shown, .. } if shown == label(action)),
        );
    }

    /// The slots and refusal note the top screen shows on `action`'s row.
    fn control_row(game: &SearchlightGame, action: SearchlightAction) -> ControlRow {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen
            .items
            .iter()
            .find_map(|item| match item {
                MenuItem::Control {
                    label: shown,
                    slots,
                    note,
                    ..
                } if shown == label(action) => Some((*slots, note.clone())),
                _ => None,
            })
            .expect("the screen lists the action")
    }

    fn lists_controls(game: &SearchlightGame) -> bool {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen
            .items
            .iter()
            .any(|item| matches!(item, MenuItem::Control { .. }))
    }

    /// Opens the Controls page from the Settings screen on top.
    fn open_controls_from_settings(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        press_item(game, ctx, "Controls", is_button("Controls"));
        assert!(lists_controls(game), "Controls did not open");
    }

    /// Opens Controls from a game on its main menu.
    fn open_controls(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        open_settings(game, ctx);
        open_controls_from_settings(game, ctx);
    }

    /// Starts a capture on `action`'s primary slot and presses `input` for it.
    fn capture_input(
        game: &mut SearchlightGame,
        ctx: &mut EventContext<'_>,
        action: SearchlightAction,
        input: Input,
    ) {
        focus_control(game, action);
        press_keys(game, ctx, &[Key::Enter]);
        game.route_input(ctx, input, true, false);
        game.route_input(ctx, input, false, false);
    }

    /// Presses and releases `input` in play; returns the commands of the tick after the press and
    /// of the tick after the release.
    fn tap(
        game: &mut SearchlightGame,
        ctx: &mut EventContext<'_>,
        input: Input,
    ) -> [SearchlightCommand; 2] {
        game.route_input(ctx, input, true, false);
        let pressed = game.command();
        game.route_input(ctx, input, false, false);
        let released = game.command();
        [pressed, released]
    }

    fn reloads(commands: [SearchlightCommand; 2]) -> bool {
        commands.iter().any(|command| command.reload)
    }

    fn fires(commands: [SearchlightCommand; 2]) -> bool {
        commands.iter().any(|command| command.fire)
    }

    const KEY_R: Input = Input::Key(Key::R);
    const KEY_T: Input = Input::Key(Key::T);
    const KEY_Q: Input = Input::Key(Key::Q);
    const MIDDLE_BUTTON: Input = Input::Mouse(MouseButton::Middle);
    const LEFT_BUTTON: Input = Input::Mouse(MouseButton::Left);

    #[test]
    fn an_applied_rebinding_does_the_action_and_the_old_input_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_T);
        capture_input(&mut game, &mut ctx, SearchlightAction::Fire, MIDDLE_BUTTON);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();

        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));
        let fire_on_middle = fires(tap(&mut game, &mut ctx, MIDDLE_BUTTON));
        let fire_on_left = fires(tap(&mut game, &mut ctx, LEFT_BUTTON));

        assert!(reload_on_t, "T reloads");
        assert!(!reload_on_r, "R no longer reloads");
        assert!(fire_on_middle, "the middle button fires");
        assert!(!fire_on_left, "the left button no longer fires");
        let (prefs, warnings) = Prefs::load_from(&path);
        let (_toggle_keys, without_fullscreen) =
            moho_app::split_fullscreen_binding(prefs.bindings());
        let (_console, section) = crate::split_console_binding(&without_fullscreen);
        let (saved, game_warnings) = ActionBindings::<SearchlightAction>::load(&section);
        assert_eq!((warnings, game_warnings), (vec![], vec![]));
        assert_eq!(
            saved.get(SearchlightAction::Reload),
            [Binding::key(Key::T)],
            "Apply saved the rebinding"
        );
        assert_eq!(
            saved.get(SearchlightAction::Fire),
            [Binding::mouse(MouseButton::Middle)]
        );
    }

    #[test]
    fn an_applied_secondary_does_the_action_beside_the_primary() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight, Key::Enter]);
        game.route_input(&mut ctx, KEY_T, true, false);
        game.route_input(&mut ctx, KEY_T, false, false);
        let staged = control_row(&game, SearchlightAction::Reload);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();

        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));

        assert_eq!(
            staged.0,
            [Some(Binding::key(Key::R)), Some(Binding::key(Key::T))]
        );
        assert!(reload_on_r, "the primary still reloads");
        assert!(reload_on_t, "the secondary reloads");
        let (prefs, _warnings) = Prefs::load_from(&path);
        let (_toggle_keys, without_fullscreen) =
            moho_app::split_fullscreen_binding(prefs.bindings());
        let (_console, section) = crate::split_console_binding(&without_fullscreen);
        let (saved, _game_warnings) = ActionBindings::<SearchlightAction>::load(&section);
        assert_eq!(
            saved.get(SearchlightAction::Reload),
            [Binding::key(Key::R), Binding::key(Key::T)],
            "Apply saved both"
        );
    }

    #[test]
    fn back_from_controls_keeps_the_old_binding_live_and_shown() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_T);
        let staged = control_row(&game, SearchlightAction::Reload);

        press_item(&mut game, &mut ctx, "Back", is_button("Back"));
        press_item(&mut game, &mut ctx, "Back", is_button("Back"));
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        open_controls_from_settings(&mut game, &mut ctx);
        let reopened = control_row(&game, SearchlightAction::Reload);
        game.enter_play();
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));

        assert_eq!(staged.0, [Some(Binding::key(Key::T)), None]);
        assert_eq!(reopened, ([Some(Binding::key(Key::R)), None], None));
        assert!(reload_on_r, "R still reloads");
        assert!(!reload_on_t, "T never became a binding");
        assert!(!path.exists(), "Back saved the discarded capture");
    }

    /// Backs out of Controls and Settings to the main menu, then reopens Controls from a fresh
    /// draft of what is live.
    fn reopen_controls(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        press_item(game, ctx, "Back", is_button("Back"));
        press_item(game, ctx, "Back", is_button("Back"));
        press_keys(game, ctx, &[Key::Enter]);
        open_controls_from_settings(game, ctx);
    }

    /// The bindings the saved prefs at `path` hold, with defaults for what they leave out.
    fn saved_bindings(path: &std::path::Path) -> ActionBindings<SearchlightAction> {
        let (prefs, _warnings) = Prefs::load_from(path);
        let (_toggle_keys, without_fullscreen) =
            moho_app::split_fullscreen_binding(prefs.bindings());
        let (_console, section) = crate::split_console_binding(&without_fullscreen);
        ActionBindings::<SearchlightAction>::load(&section).0
    }

    fn default_slots(action: SearchlightAction) -> [Option<Binding>; 2] {
        let held = action.default_bindings();
        [held.first().copied(), held.get(1).copied()]
    }

    #[test]
    fn a_cleared_secondary_stops_working_and_the_primary_still_does() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight, Key::Enter]);
        game.route_input(&mut ctx, KEY_T, true, false);
        game.route_input(&mut ctx, KEY_T, false, false);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        reopen_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);

        press_keys(&mut game, &mut ctx, &[Key::ArrowRight, Key::Backspace]);
        let cleared = control_row(&game, SearchlightAction::Reload);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert_eq!(cleared, ([Some(Binding::key(Key::R)), None], None));
        assert!(!reload_on_t, "the cleared secondary does nothing");
        assert!(reload_on_r, "the primary still reloads");
        assert_eq!(
            saved_bindings(&path).get(SearchlightAction::Reload),
            [Binding::key(Key::R)],
            "Apply saved the cleared slot"
        );
    }

    #[test]
    fn an_applied_reset_puts_every_action_on_its_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_T);
        capture_input(&mut game, &mut ctx, SearchlightAction::Fire, MIDDLE_BUTTON);
        focus_control(&game, SearchlightAction::Jump);
        press_keys(&mut game, &mut ctx, &[Key::Backspace]);
        press_item(&mut game, &mut ctx, "Apply", is_apply);

        press_item(&mut game, &mut ctx, "Reset", is_button("Reset"));
        let shown: Vec<_> = SearchlightAction::ALL
            .iter()
            .map(|&action| control_row(&game, action).0)
            .collect();
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let fire_on_left = fires(tap(&mut game, &mut ctx, LEFT_BUTTON));
        let fire_on_middle = fires(tap(&mut game, &mut ctx, MIDDLE_BUTTON));

        let expected: Vec<_> = SearchlightAction::ALL
            .iter()
            .map(|&action| default_slots(action))
            .collect();
        assert_eq!(shown, expected, "Controls shows every default");
        assert!(reload_on_r, "Reload is back on R");
        assert!(!reload_on_t, "T no longer reloads");
        assert!(fire_on_left, "Fire is back on the left button");
        assert!(!fire_on_middle, "the middle button no longer fires");
        let saved = saved_bindings(&path);
        for &action in SearchlightAction::ALL {
            assert_eq!(
                saved.get(action),
                action.default_bindings(),
                "{action:?} saved on its defaults"
            );
        }
    }

    #[test]
    fn reset_then_back_keeps_the_applied_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_T);
        press_item(&mut game, &mut ctx, "Apply", is_apply);

        press_item(&mut game, &mut ctx, "Reset", is_button("Reset"));
        let after_reset = control_row(&game, SearchlightAction::Reload);
        reopen_controls(&mut game, &mut ctx);
        let reopened = control_row(&game, SearchlightAction::Reload);
        game.enter_play();
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert_eq!(after_reset, ([Some(Binding::key(Key::R)), None], None));
        assert_eq!(reopened, ([Some(Binding::key(Key::T)), None], None));
        assert!(reload_on_t, "Reload keeps the player's T");
        assert!(!reload_on_r, "the unapplied reset did not put R back");
        assert_eq!(
            saved_bindings(&path).get(SearchlightAction::Reload),
            [Binding::key(Key::T)],
            "Back saved nothing"
        );
    }

    #[test]
    fn clear_then_back_keeps_the_applied_secondary() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight, Key::Enter]);
        game.route_input(&mut ctx, KEY_T, true, false);
        game.route_input(&mut ctx, KEY_T, false, false);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        reopen_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::ArrowRight, Key::Backspace]);
        let staged = control_row(&game, SearchlightAction::Reload);

        reopen_controls(&mut game, &mut ctx);
        let reopened = control_row(&game, SearchlightAction::Reload);
        game.enter_play();
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert_eq!(staged.0, [Some(Binding::key(Key::R)), None]);
        assert_eq!(
            reopened.0,
            [Some(Binding::key(Key::R)), Some(Binding::key(Key::T))]
        );
        assert!(reload_on_t, "the applied secondary still reloads");
        assert!(reload_on_r);
        assert_eq!(
            saved_bindings(&path).get(SearchlightAction::Reload),
            [Binding::key(Key::R), Binding::key(Key::T)]
        );
    }

    #[test]
    fn a_cleared_primary_is_reusable_and_applies() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::Backspace]);
        let cleared = control_row(&game, SearchlightAction::Reload);

        capture_input(
            &mut game,
            &mut ctx,
            SearchlightAction::Reload,
            Input::Key(Key::G),
        );
        let recaptured = control_row(&game, SearchlightAction::Reload);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();
        let reload_on_g = reloads(tap(&mut game, &mut ctx, Input::Key(Key::G)));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert_eq!(cleared.0, [None, None]);
        assert_eq!(recaptured.0, [Some(Binding::key(Key::G)), None]);
        assert!(reload_on_g, "G reloads");
        assert!(!reload_on_r, "R does nothing");
    }

    #[test]
    fn a_cleared_slot_then_reset_then_apply_is_on_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::Backspace]);

        press_item(&mut game, &mut ctx, "Reset", is_button("Reset"));
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert!(reload_on_r, "the cleared primary is back");
        let saved = saved_bindings(&path);
        for &action in SearchlightAction::ALL {
            assert_eq!(saved.get(action), action.default_bindings(), "{action:?}");
        }
    }

    #[test]
    fn escape_and_the_console_key_never_reach_capture() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let unchanged: ControlRow = ([Some(Binding::key(Key::R)), None], None);
        open_controls(&mut game, &mut ctx);

        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::Enter, Key::Escape, Key::T]);
        let after_escape = control_row(&game, SearchlightAction::Reload);
        let still_on_controls = lists_controls(&game);
        focus_control(&game, SearchlightAction::Reload);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let opening = game.route_input(&mut ctx, BACKTICK, true, false);
        let closing = game.route_input(&mut ctx, BACKTICK, true, false);
        press_keys(&mut game, &mut ctx, &[Key::T]);
        let after_console = control_row(&game, SearchlightAction::Reload);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));

        assert_eq!(after_escape, unchanged, "Escape cancelled the capture");
        assert!(still_on_controls, "Escape did not leave the page");
        assert_eq!((opening, closing), (Routing::Opened, Routing::Closed));
        assert_eq!(after_console, unchanged, "the console key was not captured");
        assert!(reload_on_r, "Reload keeps R");
        assert!(!reload_on_t);
    }

    /// The title and button labels of the screen on top.
    fn top_dialog(game: &SearchlightGame) -> (Option<String>, Vec<String>) {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        let buttons = screen
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Button { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        (screen.title.clone(), buttons)
    }

    #[test]
    fn capturing_an_input_in_use_pushes_a_dialog_naming_its_action() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);

        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_Q);

        let (title, buttons) = top_dialog(&game);
        assert_eq!(title.as_deref(), Some("Q is used by Lean left"));
        assert_eq!(buttons, ["Replace", "Cancel"]);
        assert!(!lists_controls(&game), "the dialog is on top of Controls");
    }

    #[test]
    fn an_applied_replace_moves_the_input_in_play() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_Q);
        press_item(&mut game, &mut ctx, "Replace", is_button("Replace"));
        let reload_row = control_row(&game, SearchlightAction::Reload).0;
        let lean_row = control_row(&game, SearchlightAction::LeanLeft).0;
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();

        let on_q = tap(&mut game, &mut ctx, KEY_Q);

        assert_eq!(reload_row, [Some(Binding::key(Key::Q)), None]);
        assert_eq!(lean_row, [None, None], "Lean left shows Unbound");
        assert!(reloads(on_q), "Q reloads");
        assert!(
            on_q.iter().all(|command| command.lean.is_none()),
            "Q no longer leans"
        );
    }

    #[test]
    fn escape_on_the_dialog_cancels_and_returns_to_the_same_row() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, _) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_Q);

        press_keys(&mut game, &mut ctx, &[Key::Escape]);

        assert!(lists_controls(&game), "Escape popped the dialog");
        assert_eq!(
            control_row(&game, SearchlightAction::Reload),
            ([Some(Binding::key(Key::R)), None], None)
        );
        assert_eq!(
            control_row(&game, SearchlightAction::LeanLeft),
            ([Some(Binding::key(Key::Q)), None], None)
        );
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        game.route_input(&mut ctx, KEY_T, true, false);
        game.route_input(&mut ctx, KEY_T, false, false);
        assert_eq!(
            control_row(&game, SearchlightAction::Reload).0,
            [Some(Binding::key(Key::T)), None],
            "focus stayed on the Reload row"
        );
    }

    #[test]
    fn a_failed_bindings_save_keeps_the_rebinding_live_for_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        let settings = DebugSettings::new(Prefs::default(), blocker.join("searchlight.ini"));
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_T);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        game.enter_play();

        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert!(reload_on_t, "T reloads although the save failed");
        assert!(!reload_on_r, "R no longer reloads");
    }

    #[test]
    fn a_capture_after_apply_is_discarded_by_back_and_the_applied_binding_stays() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = quiet_menu_game().with_debug_settings(settings);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_controls(&mut game, &mut ctx);
        capture_input(&mut game, &mut ctx, SearchlightAction::Reload, KEY_T);
        press_item(&mut game, &mut ctx, "Apply", is_apply);
        capture_input(
            &mut game,
            &mut ctx,
            SearchlightAction::Reload,
            Input::Key(Key::Z),
        );
        let staged = control_row(&game, SearchlightAction::Reload).0;

        press_item(&mut game, &mut ctx, "Back", is_button("Back"));
        press_item(&mut game, &mut ctx, "Back", is_button("Back"));
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        open_controls_from_settings(&mut game, &mut ctx);
        let reopened = control_row(&game, SearchlightAction::Reload).0;
        game.enter_play();
        let reload_on_t = reloads(tap(&mut game, &mut ctx, KEY_T));
        let reload_on_z = reloads(tap(&mut game, &mut ctx, Input::Key(Key::Z)));
        let reload_on_r = reloads(tap(&mut game, &mut ctx, KEY_R));

        assert_eq!(staged, [Some(Binding::key(Key::Z)), None]);
        assert_eq!(reopened, [Some(Binding::key(Key::T)), None]);
        assert!(reload_on_t, "the applied T still reloads");
        assert!(!reload_on_z && !reload_on_r);
        let (prefs, _) = Prefs::load_from(&path);
        assert_eq!(
            prefs.bindings().get("reload").map(String::as_str),
            Some("T"),
            "the saved file keeps the applied binding"
        );
    }

    use crate::input::InventoryCommand;

    const TAB: Input = Input::Key(Key::Tab);

    #[test]
    fn the_inventory_key_opens_and_closes_the_inventory() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        game.route_input(&mut ctx, TAB, true, false);
        game.event(&mut ctx, motion());
        let open = (game.screen, ctx.requested_paused());
        game.route_input(&mut ctx, TAB, true, false);
        game.event(&mut ctx, motion());
        let closed = (game.screen, ctx.requested_paused());
        let closing = game.command();
        let after = game.command();

        assert_eq!(open, (Screen::Inventory, Some(false)));
        assert_eq!(closed, (Screen::Playing, Some(false)));
        assert_eq!(closing.inventory, Some(InventoryCommand::Close));
        assert_eq!(after.inventory, None, "the close is sent once");
    }

    #[test]
    fn the_second_inventory_binding_opens_it_too() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        game.route_input(&mut ctx, Input::Key(Key::I), true, false);

        assert_eq!(game.screen, Screen::Inventory);
    }

    #[test]
    fn a_held_inventory_key_repeating_does_not_open_it() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        game.route_input(&mut ctx, TAB, true, true);

        assert_eq!(game.screen, Screen::Playing);
    }

    #[test]
    fn escape_closes_the_inventory_and_sends_close() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, TAB, true, false);
        assert_eq!(game.screen, Screen::Inventory);

        game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);
        let closing = game.command();
        let after = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert!(!ctx.exit_requested());
        assert_eq!(closing.inventory, Some(InventoryCommand::Close));
        assert_eq!(after.inventory, None, "the close is sent once");
    }

    #[test]
    fn an_open_inventory_sends_no_movement_fire_or_look_but_keeps_the_host_levels() {
        let mut game = game();
        game.god_mode = true;
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        game.route_input(&mut ctx, TAB, true, false);
        assert_eq!(game.screen, Screen::Inventory);

        press(game.input_mut(), SearchlightAction::Fire);
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion {
                delta: (40.0, 30.0),
            }),
        );
        let command = game.command();

        assert_eq!(command.movement.forward, 0.0);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert!(!command.fire);
        assert!(command.god_mode);
    }

    #[test]
    fn an_enemy_hit_lands_while_the_inventory_is_open() {
        let mut config = arena_config();
        config.enemy.damage = 10;
        let mode = ArenaMode::new(arena_arena(), config, wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let clock = GameClock::default();
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, TAB, true, false);
        assert_eq!(game.screen, Screen::Inventory);
        let full = game.sim().player_body().health();

        headless.advance(&mut game, Duration::ZERO);
        for _ in 0..600 {
            headless.advance(&mut game, Duration::from_secs(1) / 60);
            if game.sim().player_body().health() < full {
                break;
            }
        }

        assert!(!headless.paused());
        assert_eq!(game.screen, Screen::Inventory);
        assert!(
            game.sim().player_body().health() < full,
            "the enemy's shots never landed"
        );
    }

    /// The vertical field of view in degrees that shows objects `factor` times larger than a
    /// view `base_degrees` tall, written out here independently of `aim`.
    fn aimed_degrees(base_degrees: f32, factor: f32) -> f32 {
        2.0 * ((base_degrees.to_radians() / 2.0).tan() / factor)
            .atan()
            .to_degrees()
    }

    fn zoom_factor(factor: f32) -> crate::aim::Magnification {
        crate::aim::Magnification::new(factor).expect("a factor from 1 to 12")
    }

    fn hold_aim(game: &mut SearchlightGame) {
        press(game.input_mut(), SearchlightAction::Aim);
    }

    fn release_aim(game: &mut SearchlightGame) {
        match SearchlightAction::Aim.default_bindings()[0].input {
            Input::Key(key) => game.input_mut().key(key, false),
            Input::Mouse(button) => game.input_mut().mouse_button(button, false),
            Input::Pad(_) => panic!("Aim defaults to a pad input"),
        }
    }

    /// Runs one frame and returns the vertical field of view, in degrees, it drew with.
    fn drawn_play_fov(game: &mut SearchlightGame, renderer: &mut ProjectionRenderer) -> f32 {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &clock, Some(renderer), None);
        game.frame(&mut ctx, ALPHA);
        let projection = *renderer.projections.last().expect("a frame began");
        (1.0 / projection.y_axis.y).atan().to_degrees() * 2.0
    }

    #[test]
    fn holding_aim_draws_at_the_zoomed_field_of_view() {
        let mut game = turning_game(false);
        let mut renderer = ProjectionRenderer::default();
        let before = drawn_play_fov(&mut game, &mut renderer);

        hold_aim(&mut game);
        game.command();
        let aimed = drawn_play_fov(&mut game, &mut renderer);

        assert!((before - 75.0).abs() < 1e-3, "before: {before}");
        assert!(
            (aimed - aimed_degrees(75.0, 10.0)).abs() < 1e-3,
            "aimed: {aimed}"
        );
        assert!((aimed - 8.78).abs() < 0.01, "aimed: {aimed}");
    }

    #[test]
    fn releasing_aim_restores_the_base_field_of_view_within_a_tick() {
        let mut game = turning_game(false);
        let mut renderer = ProjectionRenderer::default();
        hold_aim(&mut game);
        game.command();
        let aimed = drawn_play_fov(&mut game, &mut renderer);

        release_aim(&mut game);
        game.command();
        let released = drawn_play_fov(&mut game, &mut renderer);

        assert!(aimed < 10.0, "the view was never zoomed: {aimed}");
        assert!((released - 75.0).abs() < 1e-3, "released: {released}");
    }

    #[test]
    fn aiming_turns_the_command_by_look_over_the_factor() {
        let command_after_motion = |aiming: bool| {
            let mut game = turning_game(false);
            if aiming {
                hold_aim(&mut game);
                game.command();
            }
            let mut renderer = SettingsRenderer::default();
            mouse(&mut game, 120.0, 40.0);
            draw_frame(&mut game, &mut renderer);
            game.command().movement
        };

        let plain = command_after_motion(false);
        let aimed = command_after_motion(true);

        assert!(plain.yaw_delta.abs() > 0.1, "{plain:?}");
        assert!(plain.pitch_delta.abs() > 0.01, "{plain:?}");
        assert!(
            (aimed.yaw_delta - plain.yaw_delta / 10.0).abs() < 1e-6,
            "{aimed:?} vs {plain:?}"
        );
        assert!(
            (aimed.pitch_delta - plain.pitch_delta / 10.0).abs() < 1e-6,
            "{aimed:?} vs {plain:?}"
        );
    }

    #[test]
    fn death_closes_the_inventory() {
        let mode = ArenaMode::new(arena_arena(), arena_config(), wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let clock = GameClock::default();
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, TAB, true, false);
        assert_eq!(game.screen, Screen::Inventory);

        headless.advance(&mut game, Duration::ZERO);
        for _ in 0..600 {
            headless.advance(&mut game, Duration::from_secs(1) / 60);
            if game.sim().player_body().is_dead() {
                break;
            }
        }
        headless.advance(&mut game, Duration::from_secs(1) / 60);

        assert!(game.sim().player_body().is_dead(), "the player never died");
        assert_eq!(game.screen, Screen::Playing);
    }

    use moho_ui_shell::ListId;
    use searchlight_rules::{Item, Place, Remains, Source};

    /// A game whose one enemy lies dead 1.5 m ahead of the player carrying one magazine, the
    /// view tilted onto it and the use key not yet pressed.
    fn game_beside_a_body() -> (SearchlightGame, GameClock) {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.enemy.sight_range = 0.5;
        config.enemy.patrol_speed = 0.0;
        config.enemy_remains = Remains {
            carried: vec![Item::Magazine(mag(30, 30))],
            ..Remains::default()
        };
        let body_at = Vec3::new(0.0, 0.0, 1.5);
        let arena = arena_with(&[], Vec3::ZERO, &[body_at], &[Vec3::new(0.0, 0.0, 10.0)]);
        let mode = ArenaMode::new(arena, config, wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::default();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        game.mode.sim_mut().kill_enemy(0);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        let pitch = (game.sim().eye_height() - 0.3).atan2(body_at.z);
        let look = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                pitch_delta: -pitch,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &look);
        game.mode.drain_feedback();
        (game, clock)
    }

    #[test]
    fn the_prompt_reads_search_with_the_use_key_over_a_body_in_view() {
        let (mut game, _clock) = game_beside_a_body();
        assert_eq!(game.sim().body_in_view(), Some(0));

        let paint = painted(&mut game);

        let expected = painted_prompt_reading("[F] Search");
        assert!(expected.as_ref().is_some_and(|meshes| !meshes.is_empty()));
        assert_eq!(paint, expected);
    }

    #[test]
    fn the_prompt_reads_searching_while_the_search_is_under_way() {
        let (mut game, mut clock) = game_beside_a_body();
        let using = SearchlightCommand {
            use_pressed: true,
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &using);
        assert_eq!(game.sim().searching(), Some(0), "no search under way");

        let paint = painted(&mut game);

        assert_eq!(paint, painted_prompt_reading("[F] Searching"));
        assert_ne!(paint, painted_prompt_reading("[F] Search"));
    }

    #[test]
    fn no_search_prompt_shows_once_the_inventory_is_open() {
        let opened_over_the_body = {
            let (mut game, _clock) = game_beside_a_body();
            let events_clock = GameClock::default();
            let mut ctx = EventContext::new(&events_clock, None, None);
            game.route_input(&mut ctx, TAB, true, false);
            assert_eq!(game.screen, Screen::Inventory);
            assert_eq!(game.sim().body_in_view(), Some(0));
            painted(&mut game)
        };
        let opened_looking_away = {
            let (mut game, mut clock) = game_beside_a_body();
            tick_with(&mut game, &mut clock, &turn_around());
            let events_clock = GameClock::default();
            let mut ctx = EventContext::new(&events_clock, None, None);
            game.route_input(&mut ctx, TAB, true, false);
            assert_eq!(game.screen, Screen::Inventory);
            assert_eq!(game.sim().body_in_view(), None);
            painted(&mut game)
        };

        assert!(
            opened_looking_away
                .as_ref()
                .is_some_and(|meshes| !meshes.is_empty())
        );
        assert_eq!(opened_over_the_body, opened_looking_away);
    }

    #[test]
    fn the_inventory_key_does_nothing_while_dead() {
        let (mut game, mut clock) = game_beside_a_body();
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        assert!(game.sim().player_body().is_dead());
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);

        game.route_input(&mut ctx, TAB, true, false);
        game.route_input(&mut ctx, Input::Key(Key::I), true, false);

        assert_eq!(game.screen, Screen::Playing);
        assert!(!game.stage.shell().menu_mut().is_open());
        assert_eq!(game.command().inventory, None);
    }

    /// Searches the body of [`game_beside_a_body`] until the sim reports it open.
    fn search_the_body(game: &mut SearchlightGame, clock: &mut GameClock) {
        let using = SearchlightCommand {
            use_pressed: true,
            ..SearchlightCommand::default()
        };
        tick_with(game, clock, &using);
        for _ in 0..30 {
            tick_with(game, clock, &SearchlightCommand::default());
        }
        assert_eq!(game.sim().looting(), Some(0), "the search never finished");
    }

    #[test]
    fn a_search_opening_in_a_frame_is_heard_and_drawn_from_the_view_without_the_look_it_discarded()
    {
        let (mut game, mut clock) = game_beside_a_body();
        game.input_mut()
            .set_look(moho_input::action_map::LookSettings {
                look_per_count: 0.002,
                smoothing: false,
                invert_y: false,
                acceleration: 0.0,
            });
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        game.event(&mut ctx, Event::Window(WindowEvent::Focused(true)));
        search_the_body(&mut game, &mut clock);
        let without_look = game.frame_view(ALPHA).0;
        mouse(&mut game, 200.0, 0.0);
        let mut renderer = SettingsRenderer::default();

        let drawn = draw_frame(&mut game, &mut renderer);

        assert_eq!(game.screen, Screen::Inventory);
        assert_eq!(drawn, without_look);
        assert_eq!(
            game.frame_listener(),
            moho_audio::Listener::from_view(drawn)
        );
    }

    #[test]
    fn a_finished_search_opens_the_inventory_with_the_bodys_list() {
        let (mut game, mut clock) = game_beside_a_body();
        assert_eq!(game.screen, Screen::Playing);

        search_the_body(&mut game, &mut clock);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);

        assert_eq!(game.screen, Screen::Inventory);
        assert_eq!(
            list_ids_on_top(&game),
            Some(vec![crate::inventory::INVENTORY, crate::inventory::BODY]),
            "the loot screen is the inventory beside the body"
        );
        let mut shell = game.stage.shell();
        let lists = shell
            .menu_mut()
            .top_lists()
            .expect("the inventory shows its lists");
        let body = lists
            .lists
            .iter()
            .find(|list| list.id == crate::inventory::BODY)
            .expect("the body's list is on the screen");
        assert_eq!(body.rows.len(), 1);
    }

    /// Where `list` sits among the lists on top of the screen.
    fn position_of(game: &SearchlightGame, list: ListId) -> usize {
        let mut shell = game.stage.shell();
        let lists = shell.menu_mut().top_lists().expect("the inventory is open");
        lists
            .lists
            .iter()
            .position(|candidate| candidate.id == list)
            .expect("the list is on the screen")
    }

    /// Picks the first row of list `from` and drops it on list `to` with the keys, starting from
    /// the list at position `focus`. Returns the position of `to`, where the focus ends.
    fn move_first_row(
        game: &mut SearchlightGame,
        ctx: &mut EventContext<'_>,
        focus: usize,
        from: ListId,
        to: ListId,
    ) -> usize {
        let (from_at, to_at) = (position_of(game, from), position_of(game, to));
        let steps = |at: usize, goal: usize| {
            let key = if goal >= at {
                Key::ArrowRight
            } else {
                Key::ArrowLeft
            };
            std::iter::repeat_n(key, at.abs_diff(goal))
        };
        let keys: Vec<Key> = steps(focus, from_at)
            .chain([Key::Enter])
            .chain(steps(from_at, to_at))
            .chain([Key::Enter])
            .collect();
        for key in keys {
            game.route_input(ctx, Input::Key(key), true, false);
            game.route_input(ctx, Input::Key(key), false, false);
        }
        to_at
    }

    fn open_body_screen() -> SearchlightGame {
        let (mut game, mut clock) = game_beside_a_body();
        search_the_body(&mut game, &mut clock);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);
        assert_eq!(game.screen, Screen::Inventory);
        game
    }

    #[test]
    fn a_search_finishing_with_the_gear_screen_open_turns_it_into_the_loot_screen() {
        use crate::inventory::{BODY, INVENTORY};

        let (mut game, mut clock) = game_beside_a_body();
        let clock_for_ctx = GameClock::default();
        let mut ctx = EventContext::new(&clock_for_ctx, None, None);
        let using = SearchlightCommand {
            use_pressed: true,
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &using);
        game.route_input(&mut ctx, TAB, true, false);
        game.route_input(&mut ctx, TAB, false, false);
        assert_eq!(game.screen, Screen::Inventory);
        let before = lists_on_top(&game);

        for _ in 0..30 {
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        }
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);
        let after = lists_on_top(&game);
        let after_ids = list_ids_on_top(&game);
        game.route_input(&mut ctx, Input::Key(Key::Escape), true, false);

        assert_eq!(before, Some(3), "the gear screen while no body is open");
        assert_eq!(after, Some(2), "the one screen became the loot screen");
        assert_eq!(after_ids, Some(vec![INVENTORY, BODY]));
        assert_eq!(game.screen, Screen::Playing);
        assert!(!game.stage.shell().menu_mut().is_open(), "no screen left");
    }

    /// How many lists the screen on top shows, or `None` when it is not a lists screen.
    fn lists_on_top(game: &SearchlightGame) -> Option<usize> {
        let mut shell = game.stage.shell();
        shell.menu_mut().top_lists().map(|lists| lists.lists.len())
    }

    /// The ids of the lists the screen on top shows, left to right.
    fn list_ids_on_top(game: &SearchlightGame) -> Option<Vec<ListId>> {
        let mut shell = game.stage.shell();
        shell
            .menu_mut()
            .top_lists()
            .map(|lists| lists.lists.iter().map(|list| list.id).collect())
    }

    #[test]
    fn the_inventory_key_opens_the_gear_screen_even_beside_a_body() {
        use crate::inventory::{EQUIPMENT, INVENTORY, MAGAZINE_SLOTS};

        let (mut game, _clock) = game_beside_a_body();
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);

        game.route_input(&mut ctx, TAB, true, false);
        game.route_input(&mut ctx, TAB, false, false);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);

        assert_eq!(game.screen, Screen::Inventory);
        assert_eq!(
            list_ids_on_top(&game),
            Some(vec![EQUIPMENT, MAGAZINE_SLOTS, INVENTORY])
        );
    }

    /// A game whose player carries `kg` of ballast against 30 kg of capacity, ticked once.
    fn game_carrying(kg: f32) -> SearchlightGame {
        let config = crate::test_support::carrying(test_config(rifle_from(mag(30, 5)), vec![]), kg);
        let mode = ArenaMode::new(
            arena_with(&[], Vec3::ZERO, &[], &[]),
            config,
            wave_tuning(1),
        );
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::default();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        game
    }

    /// The titles of the lists on the top screen and the row labels of each.
    fn character_screen(game: &mut SearchlightGame) -> Vec<(String, Vec<String>)> {
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::K), true, false);
        game.route_input(&mut ctx, Input::Key(Key::K), false, false);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(game, Duration::ZERO);
        let mut shell = game.stage.shell();
        shell
            .menu_mut()
            .top_lists()
            .map(|screen| {
                screen
                    .lists
                    .iter()
                    .map(|list| {
                        let rows = list.rows.iter().map(|row| row.label.clone()).collect();
                        (list.title.clone(), rows)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn character_screen_lists_load_under_movement_speed() {
        let mut game = game_carrying(20.0);

        let screen = character_screen(&mut game);

        let titles: Vec<&str> = screen.iter().map(|(title, _)| title.as_str()).collect();
        assert_eq!(titles, ["Carry capacity", "Movement speed", "Stamina"]);
        let speed = &screen[1].1;
        assert!(
            speed.contains(&"Load: speed ×0.70".to_owned()),
            "movement speed lists {speed:?}"
        );
    }

    #[test]
    fn the_hud_load_icon_follows_the_sims_load_state() {
        let expected = [
            (15.0, hud::LoadIcon::White),
            (20.0, hud::LoadIcon::Yellow),
            (25.0, hud::LoadIcon::Yellow),
            (30.0, hud::LoadIcon::Red),
        ];

        for (kg, icon) in expected {
            let game = game_carrying(kg);

            assert_eq!(game.load_icon(), icon, "{kg} of 30 kg");
        }
    }

    #[test]
    fn the_hud_paints_the_load_icon_in_the_colour_of_the_load_state() {
        let expected = [
            (15.0, hud::LoadIcon::White),
            (20.0, hud::LoadIcon::Yellow),
            (30.0, hud::LoadIcon::Red),
        ];
        let mut paints = Vec::new();

        for (kg, icon) in expected {
            let mut game = game_carrying(kg);

            let paint = painted(&mut game);

            assert!(paint.is_some(), "{kg} kg: nothing painted");
            assert_eq!(paint, painted_texts(vec![load_text(icon)]), "{kg} kg");
            paints.push(paint);
        }
        assert_ne!(paints[0], paints[1], "white and yellow paint alike");
        assert_ne!(paints[1], paints[2], "yellow and red paint alike");
        assert_ne!(paints[0], paints[2], "white and red paint alike");
    }

    #[test]
    fn the_hud_paints_the_load_icon_white_for_a_player_carrying_nothing() {
        let mut game = game_carrying(0.0);

        let paint = painted(&mut game);

        assert_eq!(game.sim().load_kg(), 0.0);
        assert_eq!(paint, painted_texts(vec![load_text(hud::LoadIcon::White)]));
    }

    #[test]
    fn the_open_inventory_paints_the_load_header_with_the_threshold_marks() {
        let mut game = game_carrying(20.0);
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        game.route_input(&mut ctx, TAB, true, false);
        let header = |text: &str| ScreenText {
            text: text.to_owned(),
            anchor: Anchor::Top,
            offset: [0.0, LOAD_HEADER_OFFSET_Y],
            size: PROMPT_SIZE,
            color: PROMPT_COLOR,
        };
        let paint_with = |game: &mut SearchlightGame, text: &str| {
            game.stage
                .shell()
                .set_texts(vec![header(text)])
                .expect("valid screen texts");
            let frame = game.stage.shell().ui_frame([800, 600]);
            frame.map(|frame| frame.meshes)
        };

        let paint = painted(&mut game);

        let expected = paint_with(
            &mut game,
            "20.0 of 30.0 kg  (slowed 18.0, no sprint 24.0, cannot move 30.0)",
        );
        let without_marks = paint_with(&mut game, "20.0 of 30.0 kg");
        assert_eq!(game.screen, Screen::Inventory);
        assert_eq!(paint, expected);
        assert_ne!(paint, without_marks);
    }

    #[test]
    fn the_open_inventory_shows_the_load_header_and_the_closed_one_none() {
        let mut game = game_carrying(20.0);
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        let closed = game.screen;

        game.route_input(&mut ctx, TAB, true, false);
        let header = game.inventory_header();

        assert_eq!(closed, Screen::Playing);
        assert_eq!(game.screen, Screen::Inventory);
        let header = header.expect("the open inventory shows a header");
        assert_eq!(header.text, "20.0 of 30.0 kg");
        assert_eq!(header.marks, [18.0, 24.0, 30.0]);
    }

    #[test]
    fn character_screen_lists_no_load_under_movement_speed_for_a_fine_player() {
        let mut game = game_carrying(15.0);

        let screen = character_screen(&mut game);

        let speed: Vec<&String> = screen
            .iter()
            .filter(|(title, _)| title == "Movement speed")
            .flat_map(|(_, rows)| rows)
            .collect();
        assert!(!screen.is_empty(), "the character screen did not open");
        assert!(
            speed.iter().all(|row| !row.starts_with("Load")),
            "{speed:?}"
        );
    }

    fn key_press(game: &mut SearchlightGame, key: Key) -> Routing {
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        let routing = game.route_input(&mut ctx, Input::Key(key), true, false);
        game.route_input(&mut ctx, Input::Key(key), false, false);
        routing
    }

    fn advance_a_frame(game: &mut SearchlightGame) {
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(game, Duration::ZERO);
    }

    /// The row labels of the list titled `title` on the top screen.
    fn rows_of(game: &SearchlightGame, title: &str) -> Vec<String> {
        let mut shell = game.stage.shell();
        shell
            .menu_mut()
            .top_lists()
            .and_then(|screen| screen.lists.iter().find(|list| list.title == title))
            .map(|list| list.rows.iter().map(|row| row.label.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn typing_k_into_the_console_does_not_open_the_character_screen() {
        let mut game = game();
        let clock = GameClock::default();
        open_the_console(&mut game, &clock);

        let routing = key_press(&mut game, Key::K);
        advance_a_frame(&mut game);

        assert_eq!(routing, Routing::Console);
        assert_eq!(game.screen, Screen::Playing);
        assert!(game.stage.shell().menu_mut().top_lists().is_none());
    }

    #[test]
    fn the_character_key_does_nothing_while_paused() {
        let mut game = game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);

        key_press(&mut game, Key::K);
        advance_a_frame(&mut game);

        assert_eq!(game.screen, Screen::Paused);
        assert!(game.stage.shell().menu_mut().top_lists().is_none());
    }

    #[test]
    fn the_character_key_does_nothing_while_dead() {
        let (mut game, mut clock) = game_beside_a_body();
        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        assert!(game.sim().player_body().is_dead());

        key_press(&mut game, Key::K);
        advance_a_frame(&mut game);

        assert_eq!(game.screen, Screen::Playing);
        assert!(!game.stage.shell().menu_mut().is_open());
    }

    #[test]
    fn dying_with_the_character_screen_open_closes_it() {
        let (mut game, mut clock) = game_beside_a_body();
        key_press(&mut game, Key::K);
        advance_a_frame(&mut game);
        assert_eq!(game.screen, Screen::Character);

        game.mode.sim_mut().kill_player();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        advance_a_frame(&mut game);

        assert_eq!(game.screen, Screen::Playing);
        assert!(!game.stage.shell().menu_mut().is_open());
    }

    #[test]
    fn an_open_character_screen_sends_no_movement_fire_or_look_but_keeps_the_host_levels() {
        let mut game = game();
        game.god_mode = true;
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        game.route_input(&mut ctx, Input::Key(Key::K), true, false);
        assert_eq!(game.screen, Screen::Character);

        press(game.input_mut(), SearchlightAction::Fire);
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion {
                delta: (40.0, 30.0),
            }),
        );
        let command = game.command();

        assert_eq!(command.movement.forward, 0.0);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert!(!command.fire);
        assert!(command.god_mode);
    }

    /// Like [`game_beside_a_body`], with the player carrying `kg` of ballast.
    fn game_beside_a_body_carrying(kg: f32) -> (SearchlightGame, GameClock) {
        let mut config =
            crate::test_support::carrying(test_config(rifle_from(mag(30, 30)), vec![]), kg);
        config.enemy.sight_range = 0.5;
        config.enemy.patrol_speed = 0.0;
        config.enemy_remains = Remains::default();
        let body_at = Vec3::new(0.0, 0.0, 1.5);
        let arena = arena_with(&[], Vec3::ZERO, &[body_at], &[Vec3::new(0.0, 0.0, 10.0)]);
        let mode = ArenaMode::new(arena, config, wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::default();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        game.mode.sim_mut().kill_enemy(0);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        let pitch = (game.sim().eye_height() - 0.3).atan2(body_at.z);
        let look = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                pitch_delta: -pitch,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &look);
        game.mode.drain_feedback();
        (game, clock)
    }

    #[test]
    fn the_character_screen_drops_the_load_row_when_the_sim_lightens_the_player() {
        let (mut game, mut clock) = game_beside_a_body_carrying(20.0);
        key_press(&mut game, Key::K);
        advance_a_frame(&mut game);
        let before = rows_of(&game, "Movement speed");
        let using = SearchlightCommand {
            use_pressed: true,
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &using);
        for _ in 0..30 {
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        }
        assert_eq!(game.sim().looting(), Some(0), "the search never finished");
        assert_eq!(game.screen, Screen::Character);
        let unchanged = rows_of(&game, "Movement speed");

        let unload = SearchlightCommand {
            inventory: Some(InventoryCommand::Move {
                from: Source::Inventory(0),
                to: Place::Body,
            }),
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &unload);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        advance_a_frame(&mut game);
        let after = rows_of(&game, "Movement speed");

        assert!(
            before.contains(&"Load: speed ×0.70".to_owned()),
            "{before:?}"
        );
        assert_eq!(unchanged, before);
        assert!(
            after.iter().all(|row| !row.starts_with("Load")),
            "{after:?}"
        );
    }

    /// The inventory screen open on a body that carried a magazine and then a rag, and the
    /// clock its ticks are run on.
    fn body_screen_carrying_a_magazine_then_a_rag() -> (SearchlightGame, GameClock) {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.enemy.sight_range = 0.5;
        config.enemy.patrol_speed = 0.0;
        config.enemy_remains = Remains {
            carried: vec![
                Item::Magazine(mag(30, 30)),
                Item::Other(searchlight_rules::ItemId::new("rag")),
            ],
            ..Remains::default()
        };
        let body_at = Vec3::new(0.0, 0.0, 1.5);
        let arena = arena_with(&[], Vec3::ZERO, &[body_at], &[Vec3::new(0.0, 0.0, 10.0)]);
        let mode = ArenaMode::new(arena, config, wave_tuning(1));
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = GameClock::default();
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        game.mode.sim_mut().kill_enemy(0);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        let pitch = (game.sim().eye_height() - 0.3).atan2(body_at.z);
        let look = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                pitch_delta: -pitch,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        tick_with(&mut game, &mut clock, &look);
        search_the_body(&mut game, &mut clock);
        let mut headless = HeadlessLoop::new(LoopConfig::new(60));
        headless.advance(&mut game, Duration::ZERO);
        assert_eq!(game.screen, Screen::Inventory);
        (game, clock)
    }

    /// Takes the queued command, runs one tick on it, and lets a frame rebuild the lists.
    fn tick_and_frame(game: &mut SearchlightGame, clock: &mut GameClock) {
        let command = game.command();
        tick_with(game, clock, &command);
        HeadlessLoop::new(LoopConfig::new(60)).advance(game, Duration::ZERO);
    }

    /// The labels of every row of list `list` on the screen on top.
    fn row_labels(game: &SearchlightGame, list: ListId) -> Vec<String> {
        let mut shell = game.stage.shell();
        let lists = shell.menu_mut().top_lists().expect("the inventory is open");
        lists
            .lists
            .iter()
            .find(|candidate| candidate.id == list)
            .expect("the list is on the screen")
            .rows
            .iter()
            .map(|row| row.label.clone())
            .collect()
    }

    fn tap_keys(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, keys: &[Key]) {
        for &key in keys {
            game.route_input(ctx, Input::Key(key), true, false);
            game.route_input(ctx, Input::Key(key), false, false);
        }
    }

    #[test]
    fn look_sampled_on_the_press_tick_is_unscaled_and_on_the_release_tick_scaled() {
        let mut renderer = SettingsRenderer::default();
        let mut plain = turning_game(false);
        mouse(&mut plain, 120.0, 40.0);
        draw_frame(&mut plain, &mut renderer);
        let plain = plain.command().movement;
        let mut game = turning_game(false);
        hold_aim(&mut game);
        mouse(&mut game, 120.0, 40.0);
        draw_frame(&mut game, &mut renderer);

        let on_press = game.command().movement;
        mouse(&mut game, 120.0, 40.0);
        draw_frame(&mut game, &mut renderer);
        release_aim(&mut game);
        let on_release = game.command().movement;

        assert!(plain.yaw_delta.abs() > 0.1, "{plain:?}");
        assert_eq!(on_press, plain, "sampled before aim was read");
        assert!(
            (on_release.yaw_delta - plain.yaw_delta / 10.0).abs() < 1e-6,
            "{on_release:?} vs {plain:?}"
        );
        assert!(
            (on_release.pitch_delta - plain.pitch_delta / 10.0).abs() < 1e-6,
            "{on_release:?} vs {plain:?}"
        );
    }

    #[test]
    fn tick_lands_on_the_drawn_view_while_aiming() {
        const MOTION: [(f64, f64); 3] = [(120.0, 40.0), (-30.0, 80.0), (200.0, -150.0)];
        let mut game = turning_game(false);
        hold_aim(&mut game);
        game.command();
        let mut renderer = SettingsRenderer::default();
        draw_frame(&mut game, &mut renderer);
        let start = game.sim().player();

        for (dx, dy) in MOTION {
            mouse(&mut game, dx, dy);
            draw_frame(&mut game, &mut renderer);
        }
        let drawn = *renderer.views.last().expect("a frame began");
        let (look_x, look_y) = game.input.pending_look();
        let (turn_x, turn_y) = (-look_x / 10.0, -look_y / 10.0);
        let scaled = first_person_view(
            &game.previous,
            &game.sim().player(),
            ALPHA,
            (turn_x, turn_y),
        )
        .view_matrix();
        let unscaled = expected_view(&game);
        tick_once(&mut game);

        assert!(look_x.abs() > 0.1, "the frames sampled no look");
        assert!(
            forward_of(drawn).distance(forward_of(scaled)) < 1e-5,
            "the frame did not draw the scaled look"
        );
        assert!(
            forward_of(drawn).distance(forward_of(unscaled)) > 1e-3,
            "the frame drew the unscaled look"
        );
        let landed = game.sim().player();
        assert!(
            (landed.yaw - (start.yaw + turn_x)).abs() < 1e-6,
            "{landed:?}"
        );
        assert!(
            (landed.pitch - (start.pitch + turn_y)).abs() < 1e-6,
            "{landed:?}"
        );
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
    }

    /// The commands of a session on the range with a shot at the dummy, the hits it caused as
    /// `(tick, point)`, and the field of view the last frame drew with. Aim is held from the first
    /// tick when `aiming`.
    fn range_session(aiming: bool) -> (Vec<SearchlightCommand>, Vec<(usize, Vec3)>, f32) {
        let mut game = range_game(0);
        game.simple = true;
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        settle(&mut game, &mut clock);
        game.mode.drain_feedback();
        let (mut commands, mut hits) = (Vec::new(), Vec::new());

        for tick in 0..20 {
            if aiming && tick == 0 {
                hold_aim(&mut game);
            }
            if tick == 3 {
                press(game.input_mut(), SearchlightAction::Fire);
            }
            if tick == 4 {
                match SearchlightAction::Fire.default_bindings()[0].input {
                    Input::Mouse(button) => game.input_mut().mouse_button(button, false),
                    other => panic!("Fire defaults to {other:?}"),
                }
            }
            let command = game.command();
            commands.push(command);
            tick_with(&mut game, &mut clock, &command);
            hits.extend(game.mode.drain_feedback().into_iter().filter_map(
                |feedback| match feedback {
                    Feedback::Hit { point, .. } => Some((tick, point)),
                    _ => None,
                },
            ));
        }
        let fov = drawn_play_fov(&mut game, &mut ProjectionRenderer::default());
        (commands, hits, fov)
    }

    #[test]
    fn commands_recorded_while_aiming_replay_to_the_same_hits_without_aim() {
        let (recorded, aimed_hits, aimed_fov) = range_session(true);
        let mut replay = range_game(0);
        replay.simple = true;
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        settle(&mut replay, &mut clock);
        replay.mode.drain_feedback();
        let mut replayed_hits = Vec::new();

        for (tick, command) in recorded.iter().enumerate() {
            tick_with(&mut replay, &mut clock, command);
            replayed_hits.extend(
                replay
                    .mode
                    .drain_feedback()
                    .into_iter()
                    .filter_map(|feedback| match feedback {
                        Feedback::Hit { point, .. } => Some((tick, point)),
                        _ => None,
                    }),
            );
        }
        let replay_fov = drawn_play_fov(&mut replay, &mut ProjectionRenderer::default());
        let (unaimed_commands, unaimed_hits, _) = range_session(false);

        assert!(!aimed_hits.is_empty(), "no shot hit the dummy");
        assert!(
            (aimed_fov - aimed_degrees(75.0, 10.0)).abs() < 1e-3,
            "the recorded game never aimed: {aimed_fov}"
        );
        assert!((replay_fov - 75.0).abs() < 1e-3, "{replay_fov}");
        assert_eq!(replayed_hits, aimed_hits);
        assert_eq!(recorded, unaimed_commands, "aim changed a command");
        assert_eq!(aimed_hits, unaimed_hits);
    }

    #[test]
    fn zoom_command_sets_the_aimed_field_of_view_and_is_saved() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, path) = settings_in(&dir);
        let mut game = game().with_debug_settings(settings);
        let mut renderer = ProjectionRenderer::default();

        type_into_console(&game, "zoom 5");
        game.command();
        let reply = console_lines(&game).last().cloned();
        hold_aim(&mut game);
        game.command();
        let aimed = drawn_play_fov(&mut game, &mut renderer);
        let (reloaded, warnings) = Prefs::load_from(&path);
        let mut relaunched = game_with_debug(reloaded, &path);
        hold_aim(&mut relaunched);
        relaunched.command();
        let after_relaunch = drawn_play_fov(&mut relaunched, &mut renderer);

        assert_eq!(reply.as_deref(), Some("Zoom: 5x"));
        assert!(
            (aimed - aimed_degrees(75.0, 5.0)).abs() < 1e-3,
            "aimed: {aimed}"
        );
        assert_eq!(warnings, vec![]);
        assert_eq!(
            DebugSettings::new(Prefs::load_from(&path).0, path.clone()).zoom(),
            zoom_factor(5.0)
        );
        assert!(
            (after_relaunch - aimed_degrees(75.0, 5.0)).abs() < 1e-3,
            "after a relaunch: {after_relaunch}"
        );
    }

    /// A game on the flat arena started from `prefs` saved at `path`.
    fn game_with_debug(prefs: Prefs, path: &Path) -> SearchlightGame {
        game().with_debug_settings(DebugSettings::new(prefs, path.to_path_buf()))
    }

    #[test]
    fn a_refused_zoom_keeps_the_previous_magnification() {
        let mut game = game();
        let mut renderer = ProjectionRenderer::default();
        type_into_console(&game, "zoom 5");
        game.command();

        for line in ["zoom 13", "zoom 0", "zoom x", "zoom"] {
            type_into_console(&game, line);
            game.command();
        }
        hold_aim(&mut game);
        game.command();
        let aimed = drawn_play_fov(&mut game, &mut renderer);

        assert!(
            (aimed - aimed_degrees(75.0, 5.0)).abs() < 1e-3,
            "aimed: {aimed}"
        );
    }

    #[test]
    fn opening_the_console_drops_aim() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        let mut renderer = ProjectionRenderer::default();
        hold_aim(&mut game);
        game.command();
        let aimed = drawn_play_fov(&mut game, &mut renderer);

        open_the_console(&mut game, &clock);
        game.command();
        let behind_the_console = drawn_play_fov(&mut game, &mut renderer);
        let mut ctx = EventContext::new(&clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        let resumed = drawn_play_fov(&mut game, &mut renderer);

        assert!(aimed < 10.0, "the view was never zoomed: {aimed}");
        assert!(
            (behind_the_console - 75.0).abs() < 1e-3,
            "behind the console: {behind_the_console}"
        );
        assert_eq!(routing, Routing::Closed);
        assert!((resumed - 75.0).abs() < 1e-3, "resumed: {resumed}");
    }

    #[test]
    fn opening_the_console_while_aiming_keeps_the_drawn_look_and_the_tick_lands_on_it() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        let mut renderer = SettingsRenderer::default();
        hold_aim(&mut game);
        game.command();
        let start = game.sim().player();
        mouse(&mut game, 120.0, 40.0);
        let before = draw_frame(&mut game, &mut renderer);
        let (look_x, look_y) = game.input.pending_look();

        open_the_console(&mut game, &clock);
        game.command();
        let behind_the_console = draw_frame(&mut game, &mut renderer);
        let mut ctx = EventContext::new(&clock, None, None);
        let routing = game.route_input(&mut ctx, BACKTICK, true, false);
        let resumed = draw_frame(&mut game, &mut renderer);
        tick_once(&mut game);

        assert!(look_x.abs() > 0.1, "the frames sampled no look");
        assert_eq!(routing, Routing::Closed);
        assert!(
            forward_of(before).distance(forward_of(behind_the_console)) < 1e-5,
            "the view jumped behind the console"
        );
        assert!(
            forward_of(before).distance(forward_of(resumed)) < 1e-5,
            "the view jumped on resume"
        );
        let landed = game.sim().player();
        assert!(
            (landed.yaw - (start.yaw - look_x / 10.0)).abs() < 1e-6,
            "{landed:?}"
        );
        assert!(
            (landed.pitch - (start.pitch - look_y / 10.0)).abs() < 1e-6,
            "{landed:?}"
        );
    }

    #[test]
    fn zooming_from_the_console_does_not_rescale_the_look_already_drawn() {
        let mut game = turning_game(false);
        let clock = GameClock::default();
        let mut renderer = SettingsRenderer::default();
        hold_aim(&mut game);
        game.command();
        let start = game.sim().player();
        mouse(&mut game, 120.0, 40.0);
        let before = draw_frame(&mut game, &mut renderer);
        let (look_x, look_y) = game.input.pending_look();

        open_the_console(&mut game, &clock);
        game.command();
        type_into_console(&game, "zoom 2");
        game.command();
        let behind_the_console = draw_frame(&mut game, &mut renderer);
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, BACKTICK, true, false);
        tick_once(&mut game);

        assert!(look_x.abs() > 0.1, "the frames sampled no look");
        assert!(
            forward_of(before).distance(forward_of(behind_the_console)) < 1e-5,
            "the view jumped when the zoom changed"
        );
        let landed = game.sim().player();
        assert!(
            (landed.yaw - (start.yaw - look_x / 10.0)).abs() < 1e-6,
            "{landed:?}"
        );
        assert!(
            (landed.pitch - (start.pitch - look_y / 10.0)).abs() < 1e-6,
            "{landed:?}"
        );
    }

    #[test]
    fn the_crosshair_keeps_its_gap_while_aiming() {
        let mut game = game();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut renderer = ProjectionRenderer::default();
        for _ in 0..2 {
            tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        }
        hold_aim(&mut game);
        game.command();
        for _ in 0..10 {
            tick_with(&mut game, &mut clock, &walk_command());
        }

        let aimed = drawn_play_fov(&mut game, &mut renderer);

        let spread = game.sim().player_spread();
        let gap = Crosshair::gap(spread, FOV_DEGREES.to_radians(), 600.0);
        assert!(aimed < 10.0, "the view was never zoomed: {aimed}");
        assert!(spread > 0.0, "the walking player's shot cone is closed");
        assert!(gap > Crosshair::GAP + 1.0, "{gap}");
        let mut crosshair = game
            .stage
            .crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let frame = crosshair.ui_frame([800, 600]).expect("a frame");
        assert_eq!(frame.meshes[0].vertices[4].pos[0], 400.0 + gap);
    }

    /// Runs a frame with no renderer; the scene's markers are placed all the same.
    fn frame_headless(game: &mut SearchlightGame) {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(TICK, &clock, None, None);
        game.frame(&mut ctx, ALPHA);
    }

    /// The outlines the scene would draw, as the renderer inflates them; hit volumes are
    /// outlines, never fills.
    fn outlines_drawn(game: &mut SearchlightGame) -> Vec<moho_render_api::MarkerGpu> {
        let draws = game.stage.scene.markers_mut().draws(Vec3::ZERO);
        assert!(draws.fills.is_empty(), "hit volumes drew fills");
        draws.outlines
    }

    fn run_hitvolumes(game: &mut SearchlightGame) {
        type_into_console(game, "hitvolumes");
        game.command();
    }

    /// The world-axis bounds of `volume` on a body at `position` turned `yaw`, from its eight
    /// oriented corners.
    fn corner_bounds(position: Vec3, yaw: f32, volume: &HitVolume) -> (Vec3, Vec3) {
        let facing = Quat::from_rotation_y(yaw);
        (0..8_u32)
            .map(|bits| {
                let sign = |bit: u32| if bits & bit == 0 { -1.0 } else { 1.0 };
                let corner = volume.half_extents * Vec3::new(sign(1), sign(2), sign(4));
                position + facing * (volume.offset + volume.rotation * corner)
            })
            .fold((Vec3::MAX, Vec3::MIN), |(min, max), corner| {
                (min.min(corner), max.max(corner))
            })
    }

    fn is_outline_of(outline: &moho_render_api::MarkerGpu, bounds: (Vec3, Vec3)) -> bool {
        let grow = Vec3::splat(moho_renderer::MARKER_INFLATE);
        Vec3::from_slice(&outline.min[..3]).abs_diff_eq(bounds.0 - grow, 1e-3)
            && Vec3::from_slice(&outline.max[..3]).abs_diff_eq(bounds.1 + grow, 1e-3)
    }

    fn outlined(outlines: &[moho_render_api::MarkerGpu], bounds: (Vec3, Vec3)) -> bool {
        outlines
            .iter()
            .any(|outline| is_outline_of(outline, bounds))
    }

    const DUMMY_SPAWN: Vec3 = Vec3::new(-8.0, 0.0, 4.0);

    /// A level with one dummy and three enemies, none of them the player.
    fn game_with_a_dummy_and_three_enemies() -> SearchlightGame {
        let mut arena = (*arena_with(
            &[],
            Vec3::new(0.0, 0.0, -20.0),
            &[
                Vec3::new(-3.0, 0.0, 4.0),
                Vec3::new(3.0, 0.0, 4.0),
                Vec3::new(9.0, 0.0, 4.0),
            ],
            &[Vec3::new(13.0, 0.0, 4.0)],
        ))
        .clone();
        arena.dummy_spawns = vec![DUMMY_SPAWN];
        let mut mode = ArenaMode::without_waves(
            Arc::new(arena),
            test_config(rifle_from(mag(30, 30)), vec![]),
        );
        for marker in 0..3 {
            mode.sim_mut()
                .spawn_enemy(marker, Grade::default())
                .expect("the marker is free");
        }
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
            .with_arena(Vec::new(), &[DUMMY_SPAWN])
    }

    #[test]
    fn hit_volume_outlines_are_off_at_launch() {
        let mut game = game_with_a_dummy_and_three_enemies();

        frame_headless(&mut game);

        assert!(outlines_drawn(&mut game).is_empty());
    }

    #[test]
    fn the_hitvolumes_command_shows_then_hides_outlines_for_every_character() {
        let mut game = game_with_a_dummy_and_three_enemies();
        frame_headless(&mut game);
        let before = outlines_drawn(&mut game).len();

        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let shown = outlines_drawn(&mut game).len();
        let after_on = console_lines(&game);
        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let hidden = outlines_drawn(&mut game).len();
        let after_off = console_lines(&game);

        assert_eq!(before, 0);
        assert_eq!(shown, 6 * 5, "the player, a dummy and three enemies");
        assert_eq!(hidden, 0);
        assert_eq!(after_on.last().map(String::as_str), Some("Hit volumes: on"));
        assert_eq!(
            after_off.last().map(String::as_str),
            Some("Hit volumes: off")
        );
    }

    #[test]
    fn outlines_cover_the_player_and_every_living_enemy() {
        let mut game = game_with_a_dummy_and_three_enemies();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let all_alive = outlines_drawn(&mut game).len();

        game.mode.sim_mut().kill_enemy(1);
        tick_with(&mut game, &mut clock, &SearchlightCommand::default());
        frame_headless(&mut game);
        let outlines = outlines_drawn(&mut game);

        let sim = game.sim();
        let body = sim.character_body();
        assert_eq!(all_alive, 6 * 5);
        assert!(sim.enemy_state(1).is_none(), "the enemy died");
        assert_eq!(outlines.len(), 6 * 4, "the dead enemy still has outlines");
        let player = sim.player();
        for volume in sim.player_hit_volumes() {
            let bounds = corner_bounds(player.position, player.yaw, volume);
            assert!(outlined(&outlines, bounds), "player {volume:?}");
        }
        for index in [0, 2] {
            let enemy = sim.enemy_state(index).expect("the enemy lives");
            for volume in dummy_hit_volumes_of(body, 0) {
                let bounds = corner_bounds(enemy.position, enemy.yaw, &volume);
                assert!(outlined(&outlines, bounds), "enemy {index} {volume:?}");
            }
        }
        let (target, centre) = dummy_at(body, 0, DUMMY_SPAWN);
        for volume in dummy_hit_volumes_of(body, target) {
            let bounds = corner_bounds(centre, 0.0, &volume);
            assert!(outlined(&outlines, bounds), "dummy {volume:?}");
        }
    }

    #[test]
    fn outline_markers_follow_the_leaned_volumes() {
        let mut game = game();
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let upright = outlines_drawn(&mut game);
        let lean_right = SearchlightCommand {
            lean: Some(LeanSide::Right),
            ..SearchlightCommand::default()
        };
        for _ in 0..30 {
            tick_with(&mut game, &mut clock, &lean_right);
        }

        frame_headless(&mut game);
        let leaned = outlines_drawn(&mut game);

        let sim = game.sim();
        let player = sim.player();
        let right = lean_offset(player.yaw, 1.0).normalize();
        let upright_volumes = dummy_hit_volumes_of(sim.character_body(), 0);
        assert_eq!(upright.len(), 6);
        assert_eq!(leaned.len(), 6);
        for (slot, volume) in sim.player_hit_volumes().iter().enumerate() {
            let at_rest = corner_bounds(player.position, player.yaw, &upright_volumes[slot]);
            let now = corner_bounds(player.position, player.yaw, volume);
            assert!(outlined(&upright, at_rest), "slot {slot} upright");
            assert!(outlined(&leaned, now), "slot {slot} leaned");
            let slide = ((now.0 + now.1) - (at_rest.0 + at_rest.1)) / 2.0;
            if slot < 4 {
                assert!(slide.dot(right) > 0.05, "slot {slot} slid {slide:?}");
                assert!(!outlined(&leaned, at_rest), "slot {slot} stayed upright");
            } else {
                assert!(outlined(&leaned, at_rest), "the legs stay");
            }
        }
    }

    #[test]
    fn a_picked_row_survives_a_tick_before_it_is_dropped() {
        use crate::inventory::{BODY, INVENTORY};

        let mut game = open_body_screen();
        let mut clock = GameClock::default();
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        let (body_at, inventory_at) = (position_of(&game, BODY), position_of(&game, INVENTORY));

        tap_keys(&mut game, &mut ctx, &vec![Key::ArrowRight; body_at]);
        tap_keys(&mut game, &mut ctx, &[Key::Enter]);
        for _ in 0..3 {
            tick_and_frame(&mut game, &mut clock);
        }
        tap_keys(
            &mut game,
            &mut ctx,
            &vec![Key::ArrowLeft; body_at - inventory_at],
        );
        tap_keys(&mut game, &mut ctx, &[Key::Enter]);
        let command = game.command();
        tick_with(&mut game, &mut clock, &command);

        assert_eq!(
            command.inventory,
            Some(InventoryCommand::Move {
                from: Source::BodyCarried(0),
                to: Place::Inventory,
            })
        );
        assert!(matches!(
            game.sim().loadout().inventory(),
            [Item::Magazine(_)]
        ));
    }

    #[test]
    fn after_a_move_the_rows_shift_and_the_next_move_takes_the_next_item() {
        use crate::inventory::{BODY, INVENTORY};

        let (mut game, mut clock) = body_screen_carrying_a_magazine_then_a_rag();
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        let before = (row_labels(&game, BODY), row_labels(&game, INVENTORY));

        let focus = move_first_row(&mut game, &mut ctx, 0, BODY, INVENTORY);
        tick_and_frame(&mut game, &mut clock);
        let between = (row_labels(&game, BODY), row_labels(&game, INVENTORY));
        move_first_row(&mut game, &mut ctx, focus, BODY, INVENTORY);
        tick_and_frame(&mut game, &mut clock);

        assert_eq!((before.0.len(), before.1.len()), (2, 0));
        assert_eq!((between.0.len(), between.1.len()), (1, 1));
        assert_eq!(between.0, ["rag"], "the rag is now the first row");
        assert!(matches!(
            game.sim().loadout().inventory(),
            [Item::Magazine(_), Item::Other(rag)] if rag.as_str() == "rag"
        ));
        assert!(row_labels(&game, BODY).is_empty());
    }

    #[test]
    fn a_body_left_behind_keeps_its_list_but_refuses_moves() {
        use crate::inventory::{BODY, INVENTORY};

        let (mut game, mut clock) = body_screen_carrying_a_magazine_then_a_rag();
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        let backing_away = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                forward: -1.0,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        for _ in 0..90 {
            tick_with(&mut game, &mut clock, &backing_away);
        }
        tick_and_frame(&mut game, &mut clock);
        let before = (row_labels(&game, BODY), row_labels(&game, INVENTORY));
        let loadout = game.sim().loadout().clone();

        move_first_row(&mut game, &mut ctx, 0, BODY, INVENTORY);
        tick_and_frame(&mut game, &mut clock);

        assert_eq!(game.screen, Screen::Inventory);
        assert_eq!(before.0.len(), 2, "the body's list is still shown");
        assert_eq!(
            (row_labels(&game, BODY), row_labels(&game, INVENTORY)),
            before
        );
        assert_eq!(game.sim().loadout(), &loadout);
    }

    #[test]
    fn a_second_move_before_the_command_is_taken_is_dropped() {
        use crate::inventory::{BODY, INVENTORY};

        let (mut game, _clock) = body_screen_carrying_a_magazine_then_a_rag();
        let events_clock = GameClock::default();
        let mut ctx = EventContext::new(&events_clock, None, None);
        let (body_at, inventory_at) = (position_of(&game, BODY), position_of(&game, INVENTORY));

        let focus = move_first_row(&mut game, &mut ctx, 0, BODY, INVENTORY);
        assert_eq!(focus, inventory_at);
        // The second pick is the body's other row, so a kept-last queue would show it.
        let to_body = if body_at > inventory_at {
            Key::ArrowRight
        } else {
            Key::ArrowLeft
        };
        let back = if body_at > inventory_at {
            Key::ArrowLeft
        } else {
            Key::ArrowRight
        };
        tap_keys(
            &mut game,
            &mut ctx,
            &[to_body, Key::ArrowDown, Key::Enter, back, Key::Enter],
        );
        let first = game.command();
        let second = game.command();

        assert_eq!(
            first.inventory,
            Some(InventoryCommand::Move {
                from: Source::BodyCarried(0),
                to: Place::Inventory,
            }),
            "the first pick stays queued"
        );
        assert_eq!(second.inventory, None, "the second pick was dropped");
    }

    #[test]
    fn outlines_of_a_quarter_turned_enemy_use_its_turned_bounds() {
        let mut game = game_with_a_dummy_and_three_enemies();
        run_hitvolumes(&mut game);

        frame_headless(&mut game);
        let outlines = outlines_drawn(&mut game);

        let sim = game.sim();
        let enemy = sim.enemy_state(1).expect("the enemy lives");
        assert!(
            (enemy.yaw - std::f32::consts::FRAC_PI_2).abs() < 0.05,
            "the enemy faces its waypoint at +X: {}",
            enemy.yaw
        );
        let torso = dummy_hit_volumes_of(sim.character_body(), 0)[1];
        let turned = corner_bounds(enemy.position, enemy.yaw, &torso);
        let unturned = corner_bounds(enemy.position, 0.0, &torso);
        assert!(outlined(&outlines, turned), "the turned torso");
        assert!(!outlined(&outlines, unturned), "the yaw-0 torso");
        assert!(
            (turned.1.x - turned.0.x - (unturned.1.z - unturned.0.z)).abs() < 1e-3,
            "half extents on X and Z swap"
        );
    }

    #[test]
    fn hitvolumes_off_and_on_again_while_paused_removes_and_restores_outlines() {
        let mut game = game_with_a_dummy_and_three_enemies();
        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let playing = outlines_drawn(&mut game).len();
        game.pause();
        assert_eq!(game.screen, Screen::Paused);

        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let off = outlines_drawn(&mut game).len();
        run_hitvolumes(&mut game);
        frame_headless(&mut game);
        let on = outlines_drawn(&mut game).len();

        assert_eq!(playing, 6 * 5);
        assert_eq!(off, 0);
        assert_eq!(on, 6 * 5);
    }

    #[test]
    fn a_volume_on_a_quarter_turned_body_has_swapped_half_extents() {
        let mut volume = dummy_hit_volumes_of(&SHIPPED_BODY, 0)[1];
        volume.offset = Vec3::new(0.3, 0.25, 0.0);
        volume.half_extents = Vec3::new(0.2, 0.35, 0.12);

        let (min, max) = volume_bounds(
            Vec3::new(1.0, 2.0, 3.0),
            std::f32::consts::FRAC_PI_2,
            &volume,
        );

        assert!(min.abs_diff_eq(Vec3::new(0.88, 1.9, 2.5), 1e-5), "{min:?}");
        assert!(max.abs_diff_eq(Vec3::new(1.12, 2.6, 2.9), 1e-5), "{max:?}");
    }

    #[test]
    fn a_volume_on_an_eighth_turned_body_is_bounded_by_its_oriented_box() {
        let volume = dummy_hit_volumes_of(&SHIPPED_BODY, 0)[1];
        assert_eq!(volume.half_extents, Vec3::new(0.2, 0.35, 0.12));
        let reach = (0.2 + 0.12) * std::f32::consts::FRAC_1_SQRT_2;

        let (min, max) = volume_bounds(Vec3::ZERO, std::f32::consts::FRAC_PI_4, &volume);

        assert!(
            min.abs_diff_eq(Vec3::new(-reach, -0.1, -reach), 1e-5),
            "{min:?}"
        );
        assert!(
            max.abs_diff_eq(Vec3::new(reach, 0.6, reach), 1e-5),
            "{max:?}"
        );
        assert!(reach > 0.2, "wider than the box on its own axes");
    }
}

/// The menus, observed through the shell's screen stack and the game's outward behavior.
#[cfg(test)]
mod menu_tests {
    use moho_app::GameClock;
    use moho_input::key::MouseButton;
    use moho_ui_shell::MenuItem;
    use winit::dpi::PhysicalPosition;
    use winit::event::{DeviceEvent, DeviceId, WindowEvent};

    use super::tests::game_with_a_moving_enemy;
    use super::*;

    const MAIN: [&str; 3] = ["Play", "Settings", "Quit"];
    const PAUSE: [&str; 3] = ["Resume", "Settings", "Quit"];

    fn menu_game() -> SearchlightGame {
        game_with_a_moving_enemy().with_main_menu()
    }

    /// A game in play, its look set up so mouse motion would turn the view.
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

    fn press_keys(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, keys: &[Key]) {
        for &key in keys {
            game.route_input(ctx, Input::Key(key), true, false);
            game.route_input(ctx, Input::Key(key), false, false);
        }
    }

    fn pause(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        press_keys(game, ctx, &[Key::Escape]);
        assert_eq!(game.screen, Screen::Paused, "Escape did not pause");
    }

    /// The labels of the screen the player sees on top.
    fn labels(game: &SearchlightGame) -> Vec<String> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Button { label, .. } => Some(label.clone()),
                MenuItem::Setting(_)
                | MenuItem::Apply
                | MenuItem::Control { .. }
                | MenuItem::Details { .. }
                | MenuItem::GameSlider(_) => None,
            })
            .collect()
    }

    fn title(game: &SearchlightGame) -> Option<String> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen.title.clone()
    }

    fn menu_is_open(game: &SearchlightGame) -> bool {
        game.stage.shell().menu_mut().is_open()
    }

    /// The centre of row `row` of a `rows`-row screen on the 1280 by 720 test window.
    fn row_point(rows: usize, row: usize) -> [f64; 2] {
        [640.0, 360.0 + (row as f64 - (rows - 1) as f64 / 2.0) * 56.0]
    }

    fn move_cursor(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, at: [f64; 2]) {
        game.event(
            ctx,
            Event::Window(WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position: PhysicalPosition::new(at[0], at[1]),
            }),
        );
    }

    fn click(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        let left = Input::Mouse(MouseButton::Left);
        game.route_input(ctx, left, true, false);
        game.route_input(ctx, left, false, false);
    }

    fn mouse_motion(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, dx: f64, dy: f64) {
        game.event(
            ctx,
            Event::Device(DeviceEvent::MouseMotion { delta: (dx, dy) }),
        );
    }

    #[test]
    fn the_main_menu_survives_replacing_the_shell() {
        let shell = UiShell::new(console_commands::console(&BTreeMap::new()).0);
        let mut game = game_with_a_moving_enemy()
            .with_main_menu()
            .with_shell(shell);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        assert_eq!(game.screen, Screen::MainMenu);
        assert!(menu_is_open(&game), "the new shell has no menu open");
        assert_eq!(labels(&game), MAIN);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);

        assert_eq!(game.screen, Screen::Playing);
    }

    #[test]
    fn down_from_play_focuses_settings_and_up_focuses_quit() {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut down = menu_game();
        assert_eq!(labels(&down), MAIN);
        let mut up = menu_game();
        let up_clock = GameClock::default();
        let mut up_ctx = EventContext::new(&up_clock, None, None);

        press_keys(&mut down, &mut ctx, &[Key::ArrowDown, Key::Enter]);
        press_keys(&mut up, &mut up_ctx, &[Key::ArrowUp, Key::Enter]);

        assert_eq!(title(&down).as_deref(), Some("Settings"));
        assert!(!ctx.exit_requested());
        assert!(up_ctx.exit_requested());
    }

    #[test]
    fn enter_on_quit_requests_exit() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        assert_eq!(labels(&game), MAIN);

        press_keys(
            &mut game,
            &mut ctx,
            &[Key::ArrowDown, Key::ArrowDown, Key::Enter],
        );

        assert!(ctx.exit_requested());
        assert_eq!(game.screen, Screen::MainMenu);
    }

    #[test]
    fn clicking_resume_returns_to_play() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);
        assert_eq!(labels(&game), PAUSE);
        assert!(!game.stage.cursor_captured, "the cursor is free in a menu");
        press_keys(&mut game, &mut ctx, &[Key::ArrowDown]);

        move_cursor(&mut game, &mut ctx, row_point(3, 0));
        click(&mut game, &mut ctx);

        assert_eq!(game.screen, Screen::Playing);
        assert!(!menu_is_open(&game));
        assert!(game.stage.cursor_captured);
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn a_click_that_misses_every_row_chooses_nothing() {
        let mut game = menu_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        assert_eq!(labels(&game), MAIN);

        move_cursor(&mut game, &mut ctx, [10.0, 10.0]);
        click(&mut game, &mut ctx);

        assert_eq!(game.screen, Screen::MainMenu);
        assert_eq!(labels(&game), MAIN);
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn back_from_settings_shows_the_menu_it_came_from() {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut main = menu_game();
        let mut paused = playing_game();
        pause(&mut paused, &mut ctx);
        assert_eq!(labels(&main), MAIN);
        assert_eq!(labels(&paused), PAUSE);

        for game in [&mut main, &mut paused] {
            press_keys(game, &mut ctx, &[Key::ArrowDown, Key::Enter]);
            let settings = (title(game), labels(game));
            press_keys(game, &mut ctx, &[Key::Escape]);
            let after_back = (title(game), labels(game));
            press_keys(game, &mut ctx, &[Key::Enter]);
            let reopened = title(game);

            assert_eq!(
                settings,
                (
                    Some("Settings".to_owned()),
                    [
                        "Controls", "Mouse", "Audio", "Graphics", "Display", "Game", "Back"
                    ]
                    .map(String::from)
                    .to_vec()
                )
            );
            assert_eq!(after_back.0, None);
            assert_eq!(reopened.as_deref(), Some("Settings"), "Back kept the focus");
        }
        assert_eq!(main.screen, Screen::MainMenu);
        assert_eq!(paused.screen, Screen::Paused);
        assert!(!ctx.exit_requested());
    }

    #[test]
    fn escape_in_settings_from_pause_shows_the_pause_menu_still_paused() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);
        assert_eq!(labels(&game), PAUSE);
        let world = (game.sim().player(), game.sim().enemy_state(0));
        press_keys(&mut game, &mut ctx, &[Key::ArrowDown, Key::Enter]);
        assert_eq!(title(&game).as_deref(), Some("Settings"));

        press_keys(&mut game, &mut ctx, &[Key::Escape]);

        assert_eq!(labels(&game), PAUSE);
        assert_eq!(game.screen, Screen::Paused);
        assert!(!game.stage.cursor_captured);
        assert!(!ctx.exit_requested());
        assert_eq!((game.sim().player(), game.sim().enemy_state(0)), world);
    }

    #[test]
    fn escape_on_the_pause_menu_resumes_and_on_the_main_menu_does_nothing() {
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut paused = playing_game();
        pause(&mut paused, &mut ctx);
        assert_eq!(labels(&paused), PAUSE);
        let mut main = menu_game();
        assert_eq!(labels(&main), MAIN);

        press_keys(&mut paused, &mut ctx, &[Key::Escape]);
        press_keys(&mut main, &mut ctx, &[Key::Escape]);

        assert_eq!(paused.screen, Screen::Playing);
        assert!(!menu_is_open(&paused));
        assert!(paused.stage.cursor_captured);
        assert_eq!(main.screen, Screen::MainMenu);
        assert_eq!(labels(&main), MAIN);
    }

    #[test]
    fn fire_and_mouse_behind_the_pause_menu_do_nothing() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        pause(&mut game, &mut ctx);
        assert_eq!(labels(&game), PAUSE);

        move_cursor(&mut game, &mut ctx, [5.0, 5.0]);
        game.route_input(&mut ctx, Input::Mouse(MouseButton::Left), true, false);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        mouse_motion(&mut game, &mut ctx, 40.0, 30.0);
        let command = game.command();

        assert!(!command.fire);
        assert_eq!(command.movement.forward, 0.0);
        assert_eq!(command.movement.yaw_delta, 0.0);
        assert_eq!(command.movement.pitch_delta, 0.0);
        assert_eq!(game.input.pending_look(), (0.0, 0.0));
        assert_eq!(game.screen, Screen::Paused);
        assert_eq!(labels(&game), PAUSE);
    }

    #[test]
    fn forward_held_as_the_pause_menu_opens_does_not_move_after_resume() {
        let mut game = playing_game();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        assert_eq!(game.command().movement.forward, 1.0);

        pause(&mut game, &mut ctx);
        assert_eq!(labels(&game), PAUSE);
        game.route_input(&mut ctx, Input::Key(Key::W), false, false);
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        let resumed = game.command();
        game.route_input(&mut ctx, Input::Key(Key::W), true, false);
        let pressed_again = game.command();

        assert_eq!(game.screen, Screen::Playing);
        assert!(!menu_is_open(&game));
        assert_eq!(resumed.movement.forward, 0.0);
        assert_eq!(pressed_again.movement.forward, 1.0);
    }
}

/// The Settings screen as the player drives it: through keys and the game's frames, with the
/// prefs file and the renderer as the places a change shows.
#[cfg(test)]
mod settings_tests {
    use std::time::Duration;

    use moho_app::GameClock;
    use moho_core::prefs::{Prefs, WindowMode as Saved};
    use moho_input::bindings::ActionBindings;
    use moho_ui_shell::{EngineSetting, MenuItem};
    use winit::event::DeviceEvent;

    use super::tests::game_with_a_moving_enemy;
    use super::*;
    use crate::input::build_action_map;

    const SIZE: [f32; 2] = [1280.0, 720.0];
    const PAUSE: [&str; 3] = ["Resume", "Settings", "Quit"];

    /// Records the engine's quality calls; the draw calls do nothing.
    #[derive(Default)]
    struct QualityRenderer {
        shadow: Vec<u8>,
        /// Highest shadow level the fake GPU can allocate; `None` allows every level.
        shadow_cap: Option<u8>,
        ssao: Vec<u8>,
    }

    impl RendererBackend for QualityRenderer {
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
            _camera: (Mat4, Mat4, Vec3),
        ) -> Result<(), moho_renderer::FrameError> {
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            self.shadow.push(quality);
            self.shadow_cap.map_or(quality, |cap| quality.min(cap))
        }
        fn set_ssao_quality(&mut self, quality: u8) {
            self.ssao.push(quality);
        }
    }

    #[derive(Debug, PartialEq)]
    struct Requests {
        paused: Option<bool>,
        window_mode: Option<WindowMode>,
        window_size: Option<(u32, u32)>,
    }

    fn run_frame(game: &mut SearchlightGame, renderer: &mut QualityRenderer) -> Requests {
        let clock = GameClock::default();
        let backend: &mut dyn RendererBackend = renderer;
        let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);
        game.frame(&mut ctx, 0.5);
        Requests {
            paused: ctx.requested_paused(),
            window_mode: ctx.requested_window_mode(),
            window_size: ctx.requested_window_size(),
        }
    }

    fn settings_file(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("searchlight.ini")
    }

    fn with_prefs(game: SearchlightGame, prefs: Prefs, path: &Path) -> SearchlightGame {
        game.with_debug_settings(DebugSettings::new(prefs, path.to_path_buf()))
    }

    fn main_menu_game(prefs: Prefs, path: &Path) -> SearchlightGame {
        with_prefs(game_with_a_moving_enemy().with_main_menu(), prefs, path)
    }

    fn playing_game(prefs: Prefs, path: &Path) -> SearchlightGame {
        with_prefs(game_with_a_moving_enemy(), prefs, path)
    }

    fn game_prefs(game: &SearchlightGame) -> &Prefs {
        game.debug_settings
            .as_ref()
            .expect("the game holds debug settings")
            .prefs()
    }

    fn saved(path: &Path) -> Prefs {
        assert!(path.exists(), "nothing was saved to {}", path.display());
        let (prefs, warnings) = Prefs::load_from(path);
        assert_eq!(warnings, vec![]);
        prefs
    }

    fn press_keys(game: &mut SearchlightGame, ctx: &mut EventContext<'_>, keys: &[Key]) {
        for &key in keys {
            game.route_input(ctx, Input::Key(key), true, false);
            game.route_input(ctx, Input::Key(key), false, false);
        }
    }

    fn back_button() -> MenuItem<MenuAction> {
        MenuItem::Button {
            label: "Back".to_owned(),
            action: MenuAction::Back,
        }
    }

    fn focus_item(game: &SearchlightGame, item: &MenuItem<MenuAction>) {
        let mut shell = game.stage.shell();
        let menu = shell.menu_mut();
        let screen = menu.top().expect("a menu screen is open");
        let row = screen
            .items
            .iter()
            .position(|candidate| candidate == item)
            .expect("the screen lists the item");
        let first = moho_ui_shell::menu::layout(screen, SIZE)[0];
        let centre = [
            (first.min[0] + first.max[0]) / 2.0,
            (first.min[1] + first.max[1]) / 2.0,
        ];
        // The sixteen-row Settings screen scrolls at this window size, so a row below the fold
        // has no point to hover: go to the first row, then step down to `row`.
        menu.scroll(-1000.0);
        menu.hover(centre, SIZE);
        for _ in 0..row {
            menu.on_key(Input::Key(Key::ArrowDown), true, false);
        }
    }

    fn change(
        game: &mut SearchlightGame,
        ctx: &mut EventContext<'_>,
        setting: EngineSetting,
        keys: &[Key],
    ) {
        show_page_of(game, ctx, setting);
        focus_item(game, &MenuItem::Setting(setting));
        press_keys(game, ctx, keys);
    }

    /// Opens the Settings page `setting` is on, going back to the categories first when another
    /// page is open; that page's unapplied changes are discarded.
    fn show_page_of(
        game: &mut SearchlightGame,
        ctx: &mut EventContext<'_>,
        setting: EngineSetting,
    ) {
        let page = crate::menu::page_of(setting);
        if title(game).as_deref() == Some(page.label()) {
            return;
        }
        if title(game).as_deref() != Some("Settings") {
            click_back(game, ctx);
        }
        focus_item(
            game,
            &MenuItem::Button {
                label: page.label().to_owned(),
                action: MenuAction::Page(page),
            },
        );
        press_keys(game, ctx, &[Key::Enter]);
        assert_eq!(title(game).as_deref(), Some(page.label()));
    }

    fn click_apply(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        focus_item(game, &MenuItem::Apply);
        press_keys(game, ctx, &[Key::Enter]);
    }

    fn click_back(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        focus_item(game, &back_button());
        press_keys(game, ctx, &[Key::Enter]);
    }

    /// Opens Settings from the main menu, whose Settings row is its second.
    fn open_settings_from_main_menu(game: &mut SearchlightGame, ctx: &mut EventContext<'_>) {
        press_keys(game, ctx, &[Key::ArrowDown, Key::Enter]);
        assert_eq!(title(game).as_deref(), Some("Settings"));
    }

    fn title(game: &SearchlightGame) -> Option<String> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen.title.clone()
    }

    fn labels(game: &SearchlightGame) -> Vec<String> {
        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("a menu screen is open");
        screen
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Button { label, .. } => Some(label.clone()),
                MenuItem::Setting(_)
                | MenuItem::Apply
                | MenuItem::Control { .. }
                | MenuItem::Details { .. }
                | MenuItem::GameSlider(_) => None,
            })
            .collect()
    }

    /// The look one mouse movement turns the game's input map into, leaving the map as it was.
    fn look_of(map: &mut ActionMap<SearchlightAction>) -> (f32, f32) {
        map.reset_look();
        map.mouse_motion(40.0, 25.0);
        map.sample_frame(Duration::from_millis(16));
        let look = map.pending_look();
        map.reset_look();
        look
    }

    fn reference_map(prefs: &Prefs) -> ActionMap<SearchlightAction> {
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        build_action_map(bindings, prefs)
    }

    #[test]
    fn settings_shows_the_saved_sensitivity() {
        let dir = tempfile::tempdir().unwrap();
        let mut game = main_menu_game(
            Prefs::default().with_mouse_sensitivity(2.5),
            &settings_file(&dir),
        );
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);

        open_settings_from_main_menu(&mut game, &mut ctx);
        show_page_of(&mut game, &mut ctx, EngineSetting::MouseSensitivity);

        let mut shell = game.stage.shell();
        let screen = shell.menu_mut().top().expect("Mouse is open");
        let expected = vec![
            MenuItem::Setting(EngineSetting::MouseSensitivity),
            MenuItem::Setting(EngineSetting::MouseSmoothing),
            MenuItem::Setting(EngineSetting::InvertMouseY),
            MenuItem::Apply,
            back_button(),
        ];
        assert_eq!(
            screen.items, expected,
            "the three mouse rows, Apply and Back"
        );
        assert_eq!(screen.back, Some(MenuAction::Back));
        assert_eq!(
            screen.settings.map(|settings| settings.mouse_sensitivity),
            Some(2.5)
        );
    }

    #[test]
    fn a_shadow_quality_the_gpu_cannot_allocate_is_saved_at_the_level_it_fell_back_to() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(&dir);
        let mut prefs = Prefs::default();
        prefs.set_shadow_quality(4);
        let mut game = main_menu_game(prefs, &path);
        let mut renderer = QualityRenderer {
            shadow_cap: Some(2),
            ..QualityRenderer::default()
        };

        run_frame(&mut game, &mut renderer);

        assert_eq!(renderer.shadow, vec![4]);
        assert_eq!(game_prefs(&game).shadow_quality(), 2);
        assert_eq!(
            saved(&path).shadow_quality(),
            2,
            "the next start asks for 2"
        );
    }

    #[test]
    fn a_shadow_quality_the_gpu_allocates_is_not_saved_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(&dir);
        let mut prefs = Prefs::default();
        prefs.set_shadow_quality(4);
        let mut game = main_menu_game(prefs, &path);

        run_frame(&mut game, &mut QualityRenderer::default());

        assert!(!path.exists(), "nothing changed, so nothing was written");
    }

    #[test]
    fn a_change_before_apply_leaves_prefs_and_audio_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(&dir);
        let mut game = main_menu_game(Prefs::default(), &path);
        let mut renderer = QualityRenderer::default();
        run_frame(&mut game, &mut renderer);
        let launch = (renderer.shadow.clone(), renderer.ssao.clone());
        let look_before = look_of(&mut game.input);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings_from_main_menu(&mut game, &mut ctx);

        change(
            &mut game,
            &mut ctx,
            EngineSetting::MusicVolume,
            &[Key::ArrowRight, Key::ArrowRight],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::ShadowQuality,
            &[Key::ArrowLeft],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::WindowMode,
            &[Key::ArrowRight],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::InvertMouseY,
            &[Key::Enter],
        );
        let requests = run_frame(&mut game, &mut renderer);

        let prefs = game_prefs(&game);
        assert_eq!(prefs.music_volume(), 6.0);
        assert_eq!(prefs.shadow_quality(), 3);
        assert_eq!(prefs.window_mode(), Saved::Windowed);
        assert!(!prefs.invert_mouse_y());
        assert!(!path.exists(), "an unapplied change was saved");
        assert_eq!((renderer.shadow, renderer.ssao), launch);
        assert_eq!(requests.window_mode, None);
        assert_eq!(requests.window_size, None);
        assert_eq!(look_of(&mut game.input), look_before);
    }

    #[test]
    fn apply_sets_the_look_from_the_applied_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut game = main_menu_game(Prefs::default(), &settings_file(&dir));
        let look_before = look_of(&mut game.input);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings_from_main_menu(&mut game, &mut ctx);
        let mut applied = Prefs::default();
        *applied.mouse_sensitivity_mut() = 10.0;
        *applied.invert_mouse_y_mut() = true;

        change(
            &mut game,
            &mut ctx,
            EngineSetting::MouseSensitivity,
            &[Key::ArrowRight; 80],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::InvertMouseY,
            &[Key::Enter],
        );
        click_apply(&mut game, &mut ctx);

        let look = look_of(&mut game.input);
        assert_ne!(look, look_before, "the look did not change");
        assert_eq!(look, look_of(&mut reference_map(&applied)));
    }

    #[test]
    fn saved_settings_apply_on_the_first_frame() {
        let dir = tempfile::tempdir().unwrap();
        let mut prefs = Prefs::default();
        prefs.set_shadow_quality(1);
        prefs.set_ssao_quality(2);
        prefs.set_window_resolution(1280, 720);
        let mut game = main_menu_game(prefs, &settings_file(&dir));
        let mut unconfigured = game_with_a_moving_enemy().with_main_menu();
        let mut renderer = QualityRenderer::default();
        let mut bare_renderer = QualityRenderer::default();

        let first = run_frame(&mut game, &mut renderer);
        let after_first = (renderer.shadow.clone(), renderer.ssao.clone());
        let second = run_frame(&mut game, &mut renderer);
        run_frame(&mut unconfigured, &mut bare_renderer);

        assert_eq!(after_first, (vec![1], vec![2]));
        assert_eq!(first.window_size, Some((1280, 720)));
        assert_eq!((renderer.shadow, renderer.ssao), after_first);
        assert_eq!(second.window_size, None, "the launch settings apply once");
        assert_eq!(second.window_mode, None);
        assert!(
            bare_renderer.shadow.is_empty(),
            "no saved settings, no apply"
        );
    }

    /// How many streaks the game's scene lets live, found by asking for more than any cap.
    fn effects_cap(game: &mut SearchlightGame) -> usize {
        let effects = game.stage.scene.effects_mut();
        for _ in 0..300 {
            effects.begin_streak(moho_renderer::StreakStyle::new(1.0, 0.1, 0));
        }
        effects.live()
    }

    #[test]
    fn the_saved_effects_quality_applies_on_the_first_frame() {
        for (quality, cap) in [(0, 0), (1, 32)] {
            let dir = tempfile::tempdir().unwrap();
            let mut prefs = Prefs::default();
            prefs.set_effects_quality(quality);
            let mut game = main_menu_game(prefs, &settings_file(&dir));
            let mut renderer = QualityRenderer::default();

            run_frame(&mut game, &mut renderer);

            assert_eq!(effects_cap(&mut game), cap, "saved quality {quality}");
        }
    }

    #[test]
    fn applying_settings_caps_the_scenes_effects() {
        let dir = tempfile::tempdir().unwrap();
        let mut game = main_menu_game(Prefs::default(), &settings_file(&dir));
        let mut renderer = QualityRenderer::default();
        run_frame(&mut game, &mut renderer);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings_from_main_menu(&mut game, &mut ctx);
        change(
            &mut game,
            &mut ctx,
            EngineSetting::EffectsQuality,
            &[Key::ArrowLeft, Key::ArrowLeft],
        );
        click_apply(&mut game, &mut ctx);
        let untouched_dir = tempfile::tempdir().unwrap();
        let mut untouched = main_menu_game(Prefs::default(), &settings_file(&untouched_dir));
        run_frame(&mut untouched, &mut QualityRenderer::default());
        let default_cap = effects_cap(&mut untouched);

        run_frame(&mut game, &mut renderer);

        assert_ne!(default_cap, 32, "the default cap is not the applied one");
        assert_eq!(game_prefs(&game).effects_quality(), 1);
        assert_eq!(effects_cap(&mut game), 32);
    }

    #[test]
    fn an_applied_change_reaches_the_renderer_and_window_on_the_next_frame() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(&dir);
        let mut game = main_menu_game(Prefs::default(), &path);
        let mut renderer = QualityRenderer::default();
        run_frame(&mut game, &mut renderer);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings_from_main_menu(&mut game, &mut ctx);

        change(
            &mut game,
            &mut ctx,
            EngineSetting::ShadowQuality,
            &[Key::ArrowRight],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::SsaoQuality,
            &[Key::ArrowLeft],
        );
        let before_apply = renderer.shadow.len();
        click_apply(&mut game, &mut ctx);
        change(
            &mut game,
            &mut ctx,
            EngineSetting::WindowMode,
            &[Key::ArrowRight],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::Resolution,
            &[Key::ArrowRight],
        );
        click_apply(&mut game, &mut ctx);
        let applied = run_frame(&mut game, &mut renderer);
        let after = run_frame(&mut game, &mut renderer);

        assert_eq!(before_apply, 1, "only the launch settings so far");
        assert_eq!(renderer.shadow, vec![3, 4]);
        assert_eq!(renderer.ssao, vec![3, 2]);
        assert_eq!(applied.window_mode, Some(WindowMode::Borderless));
        assert_eq!(applied.window_size, Some((2560, 1440)));
        assert_eq!(after.window_size, None, "an applied value is taken once");
        let reloaded = saved(&path);
        assert_eq!(reloaded.shadow_quality(), 4);
        assert_eq!(reloaded.ssao_quality(), 2);
        assert_eq!(reloaded.window_mode(), Saved::Borderless);
        assert_eq!(reloaded.window_resolution(), (2560, 1440));
    }

    #[test]
    fn back_after_changing_window_mode_keeps_prefs_and_reopens_with_the_old_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(&dir);
        let mut game = main_menu_game(Prefs::default(), &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        open_settings_from_main_menu(&mut game, &mut ctx);
        change(
            &mut game,
            &mut ctx,
            EngineSetting::WindowMode,
            &[Key::ArrowRight],
        );
        change(
            &mut game,
            &mut ctx,
            EngineSetting::ShadowQuality,
            &[Key::ArrowLeft],
        );

        click_back(&mut game, &mut ctx);
        click_back(&mut game, &mut ctx);
        let after_back = (title(&game), game_prefs(&game).window_mode());
        let saved_by_back = path.exists();
        press_keys(&mut game, &mut ctx, &[Key::Enter]);
        show_page_of(&mut game, &mut ctx, EngineSetting::ShadowQuality);
        let reopened = {
            let mut shell = game.stage.shell();
            shell.menu_mut().top().and_then(|screen| screen.settings)
        };
        click_apply(&mut game, &mut ctx);

        assert_eq!(after_back, (None, Saved::Windowed));
        assert!(!saved_by_back, "Back saved the discarded change");
        let reopened = reopened.expect("Settings reopened with its values");
        assert_eq!(reopened.window_mode, Saved::Windowed);
        assert_eq!(reopened.shadow_quality, 3);
        let applied = saved(&path);
        assert_eq!(applied.window_mode(), Saved::Windowed, "Apply after Back");
        assert_eq!(applied.shadow_quality(), 3);
    }

    #[test]
    fn apply_and_back_from_pause_settings_leave_the_game_paused() {
        let dir = tempfile::tempdir().unwrap();
        let mut game = playing_game(Prefs::default(), &settings_file(&dir));
        let mut renderer = QualityRenderer::default();
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        press_keys(&mut game, &mut ctx, &[Key::Escape]);
        assert_eq!(game.screen, Screen::Paused);
        press_keys(&mut game, &mut ctx, &[Key::ArrowDown, Key::Enter]);
        assert_eq!(title(&game).as_deref(), Some("Settings"));
        let world = (game.sim().player(), game.sim().enemy_state(0));

        change(
            &mut game,
            &mut ctx,
            EngineSetting::SoundEffectVolume,
            &[Key::ArrowLeft],
        );
        click_apply(&mut game, &mut ctx);
        let after_apply = (game.screen, title(&game));
        game.event(
            &mut ctx,
            Event::Device(DeviceEvent::MouseMotion { delta: (1.0, 1.0) }),
        );
        let paused_by_event = ctx.requested_paused();
        let applied_frame = run_frame(&mut game, &mut renderer);
        click_back(&mut game, &mut ctx);
        click_back(&mut game, &mut ctx);
        let after_back = (game.screen, labels(&game));
        let back_frame = run_frame(&mut game, &mut renderer);

        assert_eq!(after_apply, (Screen::Paused, Some("Audio".to_owned())));
        assert_eq!(paused_by_event, Some(true));
        assert_eq!(applied_frame.paused, Some(true));
        assert_eq!(
            after_back,
            (Screen::Paused, PAUSE.map(String::from).to_vec())
        );
        assert_eq!(back_frame.paused, Some(true));
        assert_eq!((game.sim().player(), game.sim().enemy_state(0)), world);
    }

    #[test]
    fn a_console_debug_value_survives_a_settings_apply() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(&dir);
        let mut game = main_menu_game(Prefs::default(), &path);
        let clock = GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        game.stage.shell().console_mut().submit("tgm 1");
        game.command();
        open_settings_from_main_menu(&mut game, &mut ctx);

        change(
            &mut game,
            &mut ctx,
            EngineSetting::SoundEffectVolume,
            &[Key::ArrowRight, Key::ArrowRight],
        );
        click_apply(&mut game, &mut ctx);
        let after_apply = saved(&path);
        game.stage.shell().console_mut().submit("tgm");
        game.command();
        let after_console = saved(&path);

        assert_eq!(after_apply.sound_effect_volume(), 9.0);
        assert_eq!(after_apply.debug_value("god_mode"), Some("1"));
        assert_eq!(after_console.sound_effect_volume(), 9.0);
        assert_eq!(after_console.debug_value("god_mode"), Some("0"));
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
            point: Vec3::ZERO,
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

    use crate::sim::{STAND_HEIGHT, dummy_hit_volumes};

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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            quality
        }
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
            point: Vec3::ZERO,
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

        let segments = dummy_segments(&SHIPPED_BODY, &spawns);

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
    fn a_mark_mesh_gets_the_mark_paint() {
        let mut stage = Stage::new();
        let meshes = vec![
            box_mesh(Vec3::ZERO, Vec3::ONE),
            box_mesh(Vec3::new(5.0, 0.0, 0.0), Vec3::ONE),
            box_mesh(Vec3::new(10.0, 0.0, 0.0), Vec3::ONE),
        ];
        let names = [
            Some("floor".to_owned()),
            Some("mark".to_owned()),
            Some("wall".to_owned()),
        ];
        stage.add_named_arena(meshes, &names, &[]);
        let mut renderer = FakeRenderer::default();

        draw_once(&mut stage, &mut renderer);

        let drawn = renderer.drawn_materials();
        assert_eq!(drawn.len(), 3);
        assert_material(drawn[0], Paint::ArenaFloor);
        assert_material(drawn[1], Paint::ArenaMark);
        assert_material(drawn[2], Paint::Arena);
        let (mark, plain) = (Paint::ArenaMark.to_gpu(), Paint::Arena.to_gpu());
        assert_ne!((mark.albedo, mark.params), (plain.albedo, plain.params));
    }

    #[test]
    fn a_mark_run_mesh_gets_its_own_paint() {
        let mut stage = Stage::new();
        let meshes = vec![
            box_mesh(Vec3::ZERO, Vec3::ONE),
            box_mesh(Vec3::new(5.0, 0.0, 0.0), Vec3::ONE),
        ];
        let names = [Some("mark_run".to_owned()), Some("mark".to_owned())];
        stage.add_named_arena(meshes, &names, &[]);
        let mut renderer = FakeRenderer::default();

        draw_once(&mut stage, &mut renderer);

        let drawn = renderer.drawn_materials();
        assert_eq!(drawn.len(), 2);
        assert_material(drawn[0], Paint::ArenaMarkRun);
        assert_material(drawn[1], Paint::ArenaMark);
        for other in [Paint::Arena, Paint::ArenaFloor, Paint::ArenaMark] {
            let (run, other) = (Paint::ArenaMarkRun.to_gpu(), other.to_gpu());
            assert_ne!((run.albedo, run.params), (other.albedo, other.params));
        }
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
        use crate::test_support::{arena_with, mag, rifle_from, test_config, wave_tuning};

        let mode = ArenaMode::new(
            arena_with(
                &[],
                Vec3::ZERO,
                &[Vec3::new(0.0, 0.0, 10.0)],
                &[Vec3::new(0.0, 0.0, 6.0)],
            ),
            test_config(rifle_from(mag(30, 30)), vec![]),
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
        let menu_enemies = game.stage.enemies.len();

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
        let shown = game.stage.enemies.len();
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

    fn feed_camera(x: f32, forward: Vec3) -> MenuCamera {
        MenuCamera {
            eye: Vec3::new(x, 1.7, -30.0),
            forward,
        }
    }

    fn two_cameras() -> Vec<MenuCamera> {
        vec![feed_camera(40.0, Vec3::X), feed_camera(-40.0, Vec3::NEG_X)]
    }

    fn frame_of(game: &mut SearchlightGame, renderer: &mut FakeRenderer, dt: Duration) {
        let clock = moho_app::GameClock::default();
        let backend: &mut dyn RendererBackend = renderer;
        let mut ctx = FrameContext::new(dt, &clock, Some(backend), None);
        game.frame(&mut ctx, 0.5);
    }

    /// The eye the last frame was drawn from, and where its view carries a point `ahead` of it.
    fn seen_from(renderer: &FakeRenderer, ahead: Vec3) -> (Vec3, Vec3) {
        let (view, _, eye) = renderer.camera.expect("a frame began");
        (eye, view.transform_point3(eye + ahead))
    }

    #[test]
    fn the_main_menu_draws_the_level_from_the_first_menu_camera() {
        let mut game = game_with_an_arena_on_the_main_menu().with_menu_cameras(two_cameras());
        let mut renderer = FakeRenderer::default();

        frame_with(&mut game, &mut renderer);
        let (eye, in_view) = seen_from(&renderer, Vec3::X * 3.0);

        assert_eq!(eye, Vec3::new(40.0, 1.7, -30.0));
        assert!(
            (in_view - Vec3::new(0.0, 0.0, -3.0)).length() < 1e-4,
            "the first camera looks along +X: {in_view}"
        );
        assert_eq!(
            renderer.draws.len(),
            1 + 6,
            "the arena and its dummies are drawn, as in play"
        );
        let enemies = game.stage.enemies.len();
        assert_eq!(enemies, 0, "enemies synced behind the menu");
        assert_eq!(crosshair_vertices(&game), 0, "crosshair drawn on the menu");
    }

    #[test]
    fn the_main_menu_cuts_to_the_next_camera_after_five_seconds_of_frames() {
        let mut game = game_with_an_arena_on_the_main_menu().with_menu_cameras(two_cameras());
        let mut renderer = FakeRenderer::default();

        frame_of(&mut game, &mut renderer, Duration::from_millis(4900));
        let (before, _) = seen_from(&renderer, Vec3::X);
        frame_of(&mut game, &mut renderer, Duration::from_millis(100));
        let (after, _) = seen_from(&renderer, Vec3::X);

        assert_eq!(before.x, 40.0, "cut before five seconds");
        assert_eq!(after.x, -40.0, "no cut at five seconds");
    }

    #[test]
    fn the_main_menu_feed_loops_back_to_the_first_camera() {
        let mut game = game_with_an_arena_on_the_main_menu().with_menu_cameras(two_cameras());
        let mut renderer = FakeRenderer::default();
        let mut shown = Vec::new();

        for _ in 0..3 {
            frame_of(&mut game, &mut renderer, Duration::from_secs(5));
            shown.push(seen_from(&renderer, Vec3::X).0.x);
        }

        assert_eq!(shown, [-40.0, 40.0, -40.0]);
    }

    #[test]
    fn without_menu_cameras_the_menu_draws_the_empty_backdrop() {
        let mut game = game_with_an_arena_on_the_main_menu().with_menu_cameras(Vec::new());
        let mut renderer = FakeRenderer::default();

        frame_with(&mut game, &mut renderer);
        let (view, _, eye) = renderer.camera.expect("the backdrop frame began");

        assert!(renderer.draws.is_empty(), "the level drawn without cameras");
        assert_eq!(view, Mat4::IDENTITY);
        assert_eq!(eye, Vec3::ZERO);
    }

    #[test]
    fn play_from_a_camera_view_starts_at_the_player_spawn() {
        let mut game = game_with_an_arena_on_the_main_menu().with_menu_cameras(two_cameras());
        let mut renderer = FakeRenderer::default();
        let clock = moho_app::GameClock::default();
        let mut ctx = EventContext::new(&clock, None, None);
        let mut reference = game_with_an_arena_on_the_main_menu();
        let mut reference_renderer = FakeRenderer::default();
        reference.enter_play();
        frame_with(&mut reference, &mut reference_renderer);
        frame_with(&mut game, &mut renderer);
        assert_eq!(seen_from(&renderer, Vec3::X).0.x, 40.0, "a camera shows");

        game.route_input(&mut ctx, Input::Key(Key::Enter), true, false);
        frame_with(&mut game, &mut renderer);

        let (eye, _) = seen_from(&renderer, Vec3::X);
        let (spawn_eye, _) = seen_from(&reference_renderer, Vec3::X);
        assert_eq!(game.screen, Screen::Playing);
        assert_eq!(eye, spawn_eye, "play began from a menu camera");
        let player = game.sim().player().position;
        assert!(
            (eye.x - player.x).abs() < 1e-3 && (eye.z - player.z).abs() < 1e-3,
            "{eye} is not over the player at {player}"
        );
        assert_eq!(renderer.draws.len(), 1 + 6 + 7);
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

    use moho_app::GameClock;
    use moho_input::bindings::ActionBindings;
    use moho_renderer::FrameError;
    use searchlight_rules::Grade;

    use super::*;
    use crate::arena::{Arena, ArenaMarkers, oriented_box_mesh};
    use crate::sim::{Start, dummy_hit_volumes};
    use crate::test_support::{arena_with, level_marker, mag, rifle_from, test_config};

    /// Keeps the vertices of every mesh uploaded, the draws of the last frame, and counts what
    /// crosses the backend seam: uploads over the renderer's life, draw calls and instances in
    /// the last frame.
    #[derive(Default)]
    struct RecordingRenderer {
        next_handle: u32,
        uploads: HashMap<u32, Vec<[f32; 3]>>,
        normals: HashMap<u32, Vec<[f32; 3]>>,
        materials: Vec<MaterialGpu>,
        draws: Vec<(u32, InstanceGpu)>,
        /// `register_indexed_mesh` calls so far.
        registered: usize,
        /// `enqueue_*draw` calls with at least one instance in the last frame.
        draw_calls: usize,
        /// Instances in those calls.
        instances: usize,
    }

    impl RecordingRenderer {
        fn count(&mut self, instances: &[InstanceGpu]) {
            if !instances.is_empty() {
                self.draw_calls += 1;
                self.instances += instances.len();
            }
        }

        /// The handles uploaded with exactly the unit box `box_mesh(ZERO, splat(0.5))`.
        fn unit_box_handles(&self) -> Vec<u32> {
            let unit = box_mesh(Vec3::ZERO, Vec3::splat(0.5));
            self.uploads
                .iter()
                .filter(|(_, vertices)| vertices.as_slice() == unit.positions())
                .map(|(handle, _)| *handle)
                .collect()
        }

        /// Every instance of the unit box in the last frame, in draw order.
        fn unit_box_instances(&self) -> Vec<InstanceGpu> {
            let handles = self.unit_box_handles();
            self.draws
                .iter()
                .filter(|(handle, _)| handles.contains(handle))
                .map(|(_, instance)| *instance)
                .collect()
        }

        /// Each instance of the unit box as its vertices' world position and unit normal, the
        /// model applied to the uploaded mesh.
        fn placed_vertices(&self) -> Vec<Vec<(Vec3, Vec3)>> {
            let handles = self.unit_box_handles();
            self.draws
                .iter()
                .filter(|(handle, _)| handles.contains(handle))
                .map(|(handle, instance)| {
                    let model = Mat4::from_cols_array_2d(&instance.model);
                    self.uploads[handle]
                        .iter()
                        .zip(&self.normals[handle])
                        .map(|(&point, &normal)| {
                            (
                                model.transform_point3(Vec3::from(point)),
                                model.transform_vector3(Vec3::from(normal)).normalize(),
                            )
                        })
                        .collect()
                })
                .collect()
        }

        /// Each draw as its mesh's world-space centre and half extents, then its material, in
        /// draw order.
        fn drawn(&self) -> Vec<(Vec3, Vec3, MaterialGpu)> {
            self.draws
                .iter()
                .map(|(handle, instance)| {
                    let model = Mat4::from_cols_array_2d(&instance.model);
                    let points: Vec<Vec3> = self.uploads[handle]
                        .iter()
                        .map(|&p| model.transform_point3(Vec3::from(p)))
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
            normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            _material: &[u32],
            _indices: &[u32],
        ) -> u32 {
            self.next_handle += 1;
            self.registered += 1;
            self.uploads.insert(self.next_handle, vertices.to_vec());
            self.normals.insert(self.next_handle, normals.to_vec());
            self.next_handle
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(&mut self, _camera: (Mat4, Mat4, Vec3)) -> Result<(), FrameError> {
            self.draws.clear();
            self.draw_calls = 0;
            self.instances = 0;
            Ok(())
        }
        fn enqueue_draw(&mut self, mesh: u32, instances: &[InstanceGpu]) {
            self.count(instances);
            self.draws
                .extend(instances.iter().map(|instance| (mesh, *instance)));
        }
        fn enqueue_shadow_only_draw(&mut self, _mesh: u32, instances: &[InstanceGpu]) {
            self.count(instances);
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            quality
        }
        fn set_ssao_quality(&mut self, _quality: u8) {}
    }

    fn sim_of(arena: Arc<crate::arena::Arena>) -> SearchlightSim {
        SearchlightSim::start(arena, test_config(rifle_from(mag(30, 30)), vec![]))
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
        let segments = enemy_segments(&SHIPPED_BODY);

        for (segment, volume) in segments.iter().zip(dummy_hit_volumes(0)) {
            assert_eq!(*segment, (volume.offset, volume.half_extents));
        }
        assert_eq!(
            segments[6],
            (Vec3::new(-0.3, 0.25, 0.4), Vec3::new(0.04, 0.04, 0.3))
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
        let config = test_config(rifle_from(mag(30, 30)), vec![]);
        let mut sim = SearchlightSim::with_start(arena, config, Start::Empty);
        for marker in 0..markers.len() {
            sim.spawn_enemy(marker, Grade::default())
                .expect("the marker is free");
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
        sim.spawn_enemy(0, Grade::default())
            .expect("the restart emptied the markers");
        assert_eq!(sim.enemy_count(), 1);

        let after = drawn_after_sync(&mut stage, &mut renderer, &sim).len();

        assert_eq!(before, 3 * 7);
        assert_eq!(after, 7, "stale enemy models are still drawn");
    }

    /// Syncs the enemies and the bodies of `sim` onto `stage` and draws them.
    fn drawn_with_bodies(
        stage: &mut Stage,
        renderer: &mut RecordingRenderer,
        sim: &SearchlightSim,
    ) -> Vec<(Vec3, Vec3, MaterialGpu)> {
        stage.sync_enemies(sim);
        stage.sync_bodies(sim);
        stage.draw(renderer, (Mat4::IDENTITY, Vec3::ZERO));
        renderer.drawn()
    }

    fn tick_once(sim: &mut SearchlightSim, command: SearchlightCommand) {
        sim.tick(&command, 1.0 / 60.0);
    }

    #[test]
    fn a_dead_enemys_body_is_a_box_in_the_scene_at_its_point() {
        let mut sim = lone_enemy();
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();
        let alive = drawn_with_bodies(&mut stage, &mut renderer, &sim);
        sim.kill_enemy(0);
        tick_once(&mut sim, SearchlightCommand::default());
        let point = sim.bodies()[&0].point;

        let dead = drawn_with_bodies(&mut stage, &mut renderer, &sim);

        assert_eq!(alive.len(), 7, "the living enemy is seven segments");
        assert!(stage.shown_bodies.contains(&0));
        assert_eq!(dead.len(), 1, "only the body box remains: {dead:?}");
        let (centre, half, material) = dead[0];
        assert!(
            centre.distance(point + Vec3::Y * BODY_BOX_HALF.y) < 1e-4,
            "the box is at {centre}, not on the body at {point}"
        );
        assert!(half.distance(BODY_BOX_HALF) < 1e-4, "{half}");
        assert!(is_paint(material, Paint::Enemy), "not the enemy material");
    }

    #[test]
    fn restarting_removes_the_boxes_of_bodies_that_no_longer_exist() {
        let mut sim = lone_enemy();
        let mut stage = Stage::new();
        let mut renderer = RecordingRenderer::default();
        sim.kill_enemy(0);
        tick_once(&mut sim, SearchlightCommand::default());
        let with_body = drawn_with_bodies(&mut stage, &mut renderer, &sim);
        sim.kill_player();
        tick_once(&mut sim, SearchlightCommand::default());
        tick_once(
            &mut sim,
            SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            },
        );
        assert!(sim.bodies().is_empty());

        let after = drawn_with_bodies(&mut stage, &mut renderer, &sim);

        assert_eq!(with_body.len(), 1, "no body box to take away");
        assert!(stage.shown_bodies.is_empty());
        assert_eq!(
            after.len(),
            7,
            "only the standing enemy is drawn: {after:?}"
        );
        assert!(
            after.iter().all(|(_, half, _)| *half != BODY_BOX_HALF),
            "a stale body box is still drawn"
        );
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

        stage.react(
            &Feedback::EnemyFired {
                target: 2,
                from: Vec3::ZERO,
            },
            None,
        );
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

        stage.react(
            &Feedback::EnemyFired {
                target: 0,
                from: Vec3::ZERO,
            },
            None,
        );
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

        stage.react(
            &Feedback::EnemyFired {
                target: 4,
                from: Vec3::ZERO,
            },
            None,
        );
        stage.react(
            &Feedback::EnemyFired {
                target: 9,
                from: Vec3::ZERO,
            },
            None,
        );

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

    /// A game on `arena` with `enemies` enemies spawned from its first markers, loaded as the
    /// binary loads one. The enemies never notice the player, so only their routes move them.
    fn game_on(arena: Arc<Arena>, enemies: usize) -> SearchlightGame {
        let mut config = test_config(rifle_from(mag(30, 30)), vec![]);
        config.enemy.sight_range = 0.5;
        let mode = ArenaMode::without_waves(Arc::clone(&arena), config);
        let (bindings, _) = ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0))
            .with_named_arena(
                arena.meshes.clone(),
                &arena.material_names,
                &arena.dummy_spawns,
            )
            .with_controls(arena.controls.iter().map(|control| control.mesh.clone()));
        for marker in 0..enemies {
            game.mode
                .sim_mut()
                .spawn_enemy(marker, Grade::default())
                .expect("the marker is free");
        }
        game
    }

    /// One enemy at (3, 0, 4) whose only waypoint is ten metres to +X, so it faces +X.
    fn lone_game() -> SearchlightGame {
        game_on(
            arena_with(
                &[],
                Vec3::new(0.0, 0.0, -20.0),
                &[Vec3::new(3.0, 0.0, 4.0)],
                &[Vec3::new(13.0, 0.0, 4.0)],
            ),
            1,
        )
    }

    fn play_tick(game: &mut SearchlightGame) {
        let mut clock = GameClock::new(0.0, 1.0, 1.0);
        let mut ctx = TickContext {
            tick: 0,
            tick_length: Duration::from_secs_f32(1.0 / 60.0),
            clock: &mut clock,
        };
        game.tick(&mut ctx, &SearchlightCommand::default());
    }

    fn play_frame(game: &mut SearchlightGame, renderer: &mut RecordingRenderer) {
        let clock = GameClock::default();
        let backend: &mut dyn RendererBackend = renderer;
        let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);
        game.frame(&mut ctx, 0.5);
    }

    fn translation_of(instance: &InstanceGpu) -> Vec3 {
        Mat4::from_cols_array_2d(&instance.model).w_axis.truncate()
    }

    #[test]
    fn an_enemy_draws_each_segment_at_its_old_box() {
        let mut game = lone_game();
        let state = game.sim().enemy_state(0).expect("the enemy spawned");
        let mut renderer = RecordingRenderer::default();

        play_frame(&mut game, &mut renderer);
        let placed = renderer.placed_vertices();

        let segments = enemy_segments(game.sim().character_body());
        assert_eq!(placed.len(), segments.len(), "one placed box per segment");
        let facing = Quat::from_rotation_y(state.yaw);
        for (slot, ((offset, half), got)) in segments.into_iter().zip(&placed).enumerate() {
            let want = oriented_box_mesh(state.position + facing * offset, half, facing);
            let want: Vec<Vec3> = want.positions().iter().map(|&p| Vec3::from(p)).collect();
            assert_eq!(got.len(), want.len(), "slot {slot}");
            for corner in &want {
                let placed_there = got.iter().any(|(at, _)| at.abs_diff_eq(*corner, 1e-4));
                assert!(placed_there, "slot {slot}: nothing placed at {corner:?}");
            }
            for (at, _) in got {
                let wanted = want.iter().any(|corner| corner.abs_diff_eq(*at, 1e-4));
                assert!(wanted, "slot {slot}: a corner strays to {at:?}");
            }
        }
    }

    #[test]
    fn a_placed_segment_keeps_outward_normals_under_scale() {
        let mut game = lone_game();
        let state = game.sim().enemy_state(0).expect("the enemy spawned");
        let mut renderer = RecordingRenderer::default();

        play_frame(&mut game, &mut renderer);
        let placed = renderer.placed_vertices();

        let segments = enemy_segments(game.sim().character_body());
        assert_eq!(placed.len(), segments.len(), "one placed box per segment");
        let facing = Quat::from_rotation_y(state.yaw);
        for (slot, ((offset, half), got)) in segments.into_iter().zip(&placed).enumerate() {
            let want = oriented_box_mesh(state.position + facing * offset, half, facing);
            assert_eq!(got.len(), want.normals().len(), "slot {slot}");
            for (&point, &normal) in want.positions().iter().zip(want.normals()) {
                let (point, normal) = (Vec3::from(point), Vec3::from(normal));
                let kept = got
                    .iter()
                    .any(|(at, n)| at.abs_diff_eq(point, 1e-4) && n.abs_diff_eq(normal, 1e-4));
                assert!(kept, "slot {slot}: no normal {normal:?} at {point:?}");
            }
        }
    }

    #[test]
    fn a_firing_enemy_places_its_gun_struck_and_its_body_plain() {
        let mut game = lone_game();
        let mut renderer = RecordingRenderer::default();
        let (struck, plain) = (game.stage.struck_material, game.stage.enemy_material);

        game.stage.react(
            &Feedback::EnemyFired {
                target: 0,
                from: Vec3::ZERO,
            },
            None,
        );
        play_frame(&mut game, &mut renderer);
        let flashing: Vec<u32> = renderer
            .unit_box_instances()
            .iter()
            .map(|instance| instance.material)
            .collect();
        for _ in 0..FLASH_TICKS {
            game.stage.tick_flashes();
        }
        play_frame(&mut game, &mut renderer);
        let settled: Vec<u32> = renderer
            .unit_box_instances()
            .iter()
            .map(|instance| instance.material)
            .collect();

        assert_eq!(flashing, [plain, plain, plain, plain, plain, plain, struck]);
        assert_eq!(settled, [plain; 7]);
    }

    #[test]
    fn a_killed_enemy_is_not_placed() {
        let mut game = game_on(
            arena_with(
                &[],
                Vec3::new(0.0, 0.0, -20.0),
                &[Vec3::new(-3.0, 0.0, 4.0), Vec3::new(3.0, 0.0, 4.0)],
                &[Vec3::new(13.0, 0.0, 4.0)],
            ),
            2,
        );
        let mut renderer = RecordingRenderer::default();
        play_frame(&mut game, &mut renderer);
        let before = renderer.unit_box_instances().len();

        game.mode.sim_mut().kill_enemy(0);
        play_tick(&mut game);
        let survivor = game.sim().enemy_state(1).expect("the second enemy lives");
        assert!(
            game.sim().enemy_state(0).is_none(),
            "the first enemy is out"
        );
        play_frame(&mut game, &mut renderer);
        let after = renderer.unit_box_instances();

        assert_eq!(before, 2 * 7);
        assert_eq!(after.len(), 7);
        let (torso_offset, _) = enemy_segments(game.sim().character_body())[0];
        let facing = Quat::from_rotation_y(survivor.yaw);
        let want = survivor.position + facing * torso_offset;
        assert!(
            translation_of(&after[0]).abs_diff_eq(want, 1e-4),
            "the placed boxes are the survivor's"
        );
    }

    #[test]
    fn a_turned_enemy_places_its_segments_around_its_position() {
        let mut game = game_on(
            arena_with(
                &[],
                Vec3::new(0.0, 0.0, -20.0),
                &[Vec3::new(3.0, 0.0, 4.0)],
                &[Vec3::new(10.0, 0.0, 11.0)],
            ),
            1,
        );
        let state = game.sim().enemy_state(0).expect("the enemy spawned");
        let facing = Quat::from_rotation_y(state.yaw);
        assert!(
            !(facing * GUN_OFFSET).abs_diff_eq(GUN_OFFSET, 0.1),
            "the enemy must be turned for the offsets to move: yaw {}",
            state.yaw
        );
        let mut renderer = RecordingRenderer::default();

        play_frame(&mut game, &mut renderer);
        let placed = renderer.unit_box_instances();

        let segments = enemy_segments(game.sim().character_body());
        assert_eq!(placed.len(), segments.len(), "one placed box per segment");
        for (slot, ((offset, half), instance)) in segments.into_iter().zip(&placed).enumerate() {
            let model = Mat4::from_cols_array_2d(&instance.model);
            let (scale, rotation, at) = model.to_scale_rotation_translation();
            assert!(
                scale.abs_diff_eq(half * 2.0, 1e-4),
                "slot {slot}: {scale:?}"
            );
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                assert!(
                    (rotation * axis).abs_diff_eq(facing * axis, 1e-4),
                    "slot {slot}: axis {axis:?} is not turned to the enemy's facing"
                );
            }
            let want = state.position + facing * offset;
            assert!(
                at.abs_diff_eq(want, 1e-4),
                "slot {slot}: {at:?} not {want:?}"
            );
        }
    }

    /// The shipped arena with fifty enemy spawn markers added on a grid over its open floor,
    /// and fifty enemies spawned. Also how many static arena meshes and control posts it draws.
    fn fifty_enemy_game() -> (SearchlightGame, usize) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/arena.glb");
        let mut level = moho_level::load(&path).expect("the shipped arena loads");
        level
            .markers
            .retain(|marker| !marker.name.starts_with("enemy_spawn_"));
        let rows = [-14.0, -11.0, -8.0, -1.5, 1.0];
        let spawns = rows.into_iter().flat_map(|z| {
            (0..10u8).map(move |column| Vec3::new(-12.0 + 2.5 * f32::from(column), 0.0, z))
        });
        for (number, at) in spawns.enumerate() {
            level
                .markers
                .push(level_marker(&format!("enemy_spawn_{:02}", number + 1), at));
        }
        let markers = ArenaMarkers::from_level(&level).expect("the markers sort");
        let arena = Arena::from_level(&level, &markers).expect("the arena builds");
        assert_eq!(arena.enemy_spawns.len(), 50);
        let statics = arena.meshes.len() + arena.controls.len();
        (game_on(Arc::new(arena), 50), statics)
    }

    #[test]
    fn fifty_moving_enemies_upload_nothing_after_the_first_frame() {
        let (mut game, _) = fifty_enemy_game();
        let mut renderer = RecordingRenderer::default();
        for _ in 0..5 {
            play_tick(&mut game);
        }
        play_frame(&mut game, &mut renderer);
        let first = renderer.registered;
        let start = game.sim().enemy_state(0).expect("the enemy lives").position;

        for _ in 0..10 {
            play_tick(&mut game);
            play_frame(&mut game, &mut renderer);
        }

        let end = game.sim().enemy_state(0).expect("the enemy lives").position;
        assert!(start.distance(end) > 0.05, "the enemies must be moving");
        assert!(first > 0, "the first frame uploads the world");
        assert_eq!(
            renderer.registered, first,
            "meshes uploaded after frame one"
        );
    }

    #[test]
    fn fifty_moving_enemies_draw_once_per_distinct_mesh() {
        let (mut game, statics) = fifty_enemy_game();
        let mut renderer = RecordingRenderer::default();
        for _ in 0..5 {
            play_tick(&mut game);
        }
        play_frame(&mut game, &mut renderer);

        play_tick(&mut game);
        play_frame(&mut game, &mut renderer);

        let dummies = game.stage.segments.len();
        assert_eq!(renderer.draw_calls, statics + dummies + 1);
    }

    #[test]
    fn fifty_moving_enemies_place_seven_segments_each() {
        let (mut game, statics) = fifty_enemy_game();
        let mut renderer = RecordingRenderer::default();
        for _ in 0..5 {
            play_tick(&mut game);
        }
        let dummies = game.stage.segments.len();

        play_frame(&mut game, &mut renderer);

        assert_eq!(renderer.unit_box_instances().len(), 50 * 7);
        assert_eq!(renderer.instances, statics + dummies + 50 * 7);

        for index in 0..10 {
            game.mode.sim_mut().kill_enemy(index);
        }
        play_tick(&mut game);
        play_frame(&mut game, &mut renderer);

        assert_eq!(renderer.unit_box_instances().len(), 40 * 7);
        // Each of the ten dead leaves one body box.
        assert_eq!(game.mode.sim().bodies().len(), 10);
        assert_eq!(renderer.instances, statics + dummies + 40 * 7 + 10);
    }

    #[test]
    fn a_paused_frame_still_places_living_enemies() {
        let mut game = lone_game();
        let mut renderer = RecordingRenderer::default();
        game.pause();

        play_frame(&mut game, &mut renderer);

        assert_eq!(game.screen, Screen::Paused);
        assert_eq!(renderer.unit_box_instances().len(), 7);
    }

    fn materials_placed(renderer: &RecordingRenderer) -> Vec<u32> {
        renderer
            .unit_box_instances()
            .iter()
            .map(|instance| instance.material)
            .collect()
    }

    #[test]
    fn an_enemy_respawned_into_a_dead_enemys_slot_does_not_inherit_its_gun_flash() {
        let mut game = lone_game();
        let mut renderer = RecordingRenderer::default();
        let plain = game.stage.enemy_material;
        game.stage.react(
            &Feedback::EnemyFired {
                target: 0,
                from: Vec3::ZERO,
            },
            None,
        );
        play_frame(&mut game, &mut renderer);
        assert!(
            game.stage.enemies[0].gun_flash > 0,
            "the flash must be running when the enemy dies"
        );

        game.mode.sim_mut().kill_enemy(0);
        play_tick(&mut game);
        play_frame(&mut game, &mut renderer);
        game.mode.sim_mut().kill_player();
        game.mode.sim_mut().tick(
            &SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            },
            1.0 / 60.0,
        );
        game.mode
            .sim_mut()
            .spawn_enemy(0, Grade::default())
            .expect("the restart emptied the markers");
        play_frame(&mut game, &mut renderer);

        assert_eq!(materials_placed(&renderer), [plain; 7]);
    }

    #[test]
    fn an_enemy_spawned_after_a_restart_into_a_vanished_index_does_not_inherit_its_gun_flash() {
        let mut game = game_on(
            arena_with(
                &[],
                Vec3::new(0.0, 0.0, -20.0),
                &[Vec3::new(-3.0, 0.0, 4.0), Vec3::new(3.0, 0.0, 4.0)],
                &[Vec3::new(13.0, 0.0, 4.0)],
            ),
            2,
        );
        let mut renderer = RecordingRenderer::default();
        let plain = game.stage.enemy_material;
        game.stage.react(
            &Feedback::EnemyFired {
                target: 1,
                from: Vec3::ZERO,
            },
            None,
        );
        play_frame(&mut game, &mut renderer);
        assert!(
            game.stage.enemy_rig(1).gun_flash > 0,
            "the flash must be running at the restart"
        );

        game.mode.sim_mut().kill_player();
        game.mode.sim_mut().tick(
            &SearchlightCommand {
                restart: true,
                ..SearchlightCommand::default()
            },
            1.0 / 60.0,
        );
        assert_eq!(
            game.sim().enemy_count(),
            0,
            "the restart emptied the markers"
        );
        play_frame(&mut game, &mut renderer);
        for marker in 0..2 {
            game.mode
                .sim_mut()
                .spawn_enemy(marker, Grade::default())
                .expect("the restart emptied the markers");
        }
        play_frame(&mut game, &mut renderer);

        assert_eq!(materials_placed(&renderer), [plain; 14]);
    }

    #[test]
    fn only_the_firing_enemys_gun_is_struck() {
        let mut game = game_on(
            arena_with(
                &[],
                Vec3::new(0.0, 0.0, -20.0),
                &[Vec3::new(-3.0, 0.0, 4.0), Vec3::new(3.0, 0.0, 4.0)],
                &[Vec3::new(13.0, 0.0, 4.0)],
            ),
            2,
        );
        let mut renderer = RecordingRenderer::default();
        let (struck, plain) = (game.stage.struck_material, game.stage.enemy_material);
        game.stage.react(
            &Feedback::EnemyFired {
                target: 1,
                from: Vec3::ZERO,
            },
            None,
        );

        play_frame(&mut game, &mut renderer);
        let materials = materials_placed(&renderer);

        assert_eq!(materials.len(), 14);
        assert_eq!(materials.iter().filter(|&&m| m == struck).count(), 1);
        assert_eq!(materials.iter().filter(|&&m| m == plain).count(), 13);
        assert_eq!(
            materials[13], struck,
            "the second enemy's gun is the last placed"
        );
    }

    #[test]
    fn a_gun_at_yaw_ninety_degrees_is_offset_in_the_enemys_frame() {
        let mut game = lone_game();
        let state = game.sim().enemy_state(0).expect("the enemy spawned");
        let mut renderer = RecordingRenderer::default();
        assert!(
            (state.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "yaw {}",
            state.yaw
        );

        play_frame(&mut game, &mut renderer);
        let placed = renderer.unit_box_instances();

        let gun = translation_of(&placed[6]);
        let facing = Quat::from_rotation_y(state.yaw);
        assert!(gun.abs_diff_eq(state.position + facing * GUN_OFFSET, 1e-4));
        assert!(!gun.abs_diff_eq(state.position + GUN_OFFSET, 0.1));
    }

    #[test]
    fn an_enemy_whose_state_vanished_places_nothing() {
        let mut game = lone_game();
        let mut renderer = RecordingRenderer::default();
        play_frame(&mut game, &mut renderer);
        game.mode.sim_mut().kill_enemy(0);
        play_tick(&mut game);
        assert_eq!(
            game.sim().enemy_count(),
            1,
            "the dead enemy keeps its index"
        );
        assert!(game.sim().enemy_state(0).is_none());

        play_frame(&mut game, &mut renderer);

        assert!(renderer.unit_box_instances().is_empty());
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

#[cfg(test)]
mod crosshair_spread_tests {
    use super::*;

    /// Hand-computed as 4 + tan(spread) / tan(fov / 2) * height / 2: tan(0.03) = 0.030009,
    /// tan(37.5 deg) = 0.76733, tan(15 deg) = 0.26795. The 30 deg row is a narrow aimed view.
    #[test]
    fn the_crosshair_gap_is_today_s_gap_when_settled_and_opens_with_spread() {
        let table = [
            (0.0, 75.0_f32, 600.0, 4.0),
            (0.03, 75.0, 600.0, 15.73),
            (0.03, 30.0, 600.0, 37.60),
            (0.03, 75.0, 300.0, 9.87),
            (0.03, 75.0, 1200.0, 27.47),
        ];

        for (spread, fov_degrees, height, expected) in table {
            let gap = Crosshair::gap(spread, fov_degrees.to_radians(), height);

            assert!(
                (gap - expected).abs() < 1e-2,
                "spread {spread}, fov {fov_degrees}, height {height}: gap {gap}, expected {expected}"
            );
        }
    }

    #[test]
    fn a_crosshair_with_spread_draws_its_bars_out_at_the_gap() {
        let spread = 0.03;
        let mut crosshair = Crosshair {
            spread,
            ..Crosshair::default()
        };
        let gap = Crosshair::gap(spread, FOV_DEGREES.to_radians(), 600.0);

        let frame = crosshair.ui_frame([800, 600]).expect("a frame");

        let vertices = &frame.meshes[0].vertices;
        assert!(gap > Crosshair::GAP);
        assert_eq!(
            vertices[0].pos[0],
            400.0 - gap - Crosshair::LENGTH,
            "left bar outer"
        );
        assert_eq!(vertices[4].pos[0], 400.0 + gap, "right bar inner");
        assert_eq!(
            vertices[8].pos[1],
            300.0 - gap - Crosshair::LENGTH,
            "top bar outer"
        );
        assert_eq!(vertices[12].pos[1], 300.0 + gap, "bottom bar inner");
    }

    /// The right bar's inner edge on an 800 by 600 target, whose centre is at x = 400.
    fn right_bar_inner(game: &SearchlightGame) -> f32 {
        let mut crosshair = game
            .stage
            .crosshair
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let frame = crosshair.ui_frame([800, 600]).expect("a frame");
        frame.meshes[0].vertices[4].pos[0]
    }

    #[test]
    fn a_frame_while_walking_opens_the_crosshair_and_it_closes_after_the_settle_time() {
        // `test_config` friction coasts a walking player to rest in about 36 ticks; the
        // spread follows that speed down, then settles.
        const COAST_TICKS: usize = 40;
        let config = crate::test_support::test_config(
            crate::test_support::rifle_from(crate::test_support::mag(30, 5)),
            vec![],
        );
        let settle_ticks = (config.accuracy.settle_seconds * 60.0).ceil() as usize;
        let mode = ArenaMode::new(
            crate::test_support::arena_with(&[], Vec3::ZERO, &[], &[]),
            config,
            crate::test_support::wave_tuning(1),
        );
        let (bindings, _) = moho_input::bindings::ActionBindings::load(&BTreeMap::new());
        let mut game = SearchlightGame::new(mode, ActionMap::new(bindings, 1.0));
        let mut clock = moho_app::GameClock::new(0.0, 1.0, 1.0);
        let walk = SearchlightCommand {
            movement: moho_physics::CharacterCommand {
                forward: 1.0,
                ..moho_physics::CharacterCommand::default()
            },
            ..SearchlightCommand::default()
        };
        let mut step = |game: &mut SearchlightGame, command: &SearchlightCommand, ticks: usize| {
            for _ in 0..ticks {
                let mut ctx = TickContext {
                    tick: 0,
                    tick_length: std::time::Duration::from_secs_f32(1.0 / 60.0),
                    clock: &mut clock,
                };
                game.tick(&mut ctx, command);
            }
            let frame_clock = moho_app::GameClock::default();
            let mut ctx = FrameContext::new(
                std::time::Duration::from_millis(16),
                &frame_clock,
                None,
                None,
            );
            game.frame(&mut ctx, 0.5);
            right_bar_inner(game)
        };

        let settled = step(&mut game, &SearchlightCommand::default(), 2);
        let walking = step(&mut game, &walk, 10);
        let stopped = step(
            &mut game,
            &SearchlightCommand::default(),
            COAST_TICKS + settle_ticks + 3,
        );

        assert_eq!(settled, 400.0 + Crosshair::GAP);
        assert!(
            walking > 400.0 + Crosshair::GAP + 1.0,
            "the bar is at {walking} while walking"
        );
        assert_eq!(stopped, 400.0 + Crosshair::GAP);
    }
}
