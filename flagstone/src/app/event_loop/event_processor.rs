//! Event Processing
//!
//! Handles processing of various event types from the event bus:
//! - UI events (menu requests, settings, exit)
//! - Audio events (sounds, music)
//! - Graphics events (time of day, lighting)
//! - Input events (mouse wheel, keyboard)

use crate::input_event::InputEvent;
use crate::{App, RenderRequest};
use flagstone_voxel::{BlockChangeReason, CRYSTAL_MATERIAL_ID, VoxelChunk, WorldEvent};
use moho_core::events::{GraphicsEvent, UiEvent};
use moho_render_api::{Marker, MarkerId};
use std::collections::HashSet;

// ── Spawn / interaction constants ──────────────────────────────────────
const MOUSE_WHEEL_ZOOM_FACTOR: f32 = 0.5;
const SPAWN_RAYCAST_MAX_DISTANCE: f32 = 100.0;
const SPAWN_NORMAL_OFFSET: f32 = 0.5;
const SPAWN_FALLBACK_DISTANCE: f32 = 5.0;
/// Maximum reach for the player's direct mining action — shorter than
/// `SPAWN_RAYCAST_MAX_DISTANCE`, which is a debug-console placement aid, not
/// a gameplay reach limit.
const MINE_MAX_DISTANCE: f32 = 8.0;
const DEFAULT_POINT_LIGHT_INTENSITY: f32 = 5.0;
const DEFAULT_POINT_LIGHT_RANGE: f32 = 20.0;
/// Console markers' colour: translucent amber, linear RGBA with straight alpha.
const MARKER_COLOR: [f32; 4] = [1.0, 0.55, 0.0, 0.35];

/// Handles processing of all event types
pub struct EventProcessor;

impl EventProcessor {
    /// Create a new event processor
    pub fn new() -> Self {
        Self
    }

    /// Process all pending UI events from the event bus
    pub fn process_ui_events(&self, app: &mut App, clock: &mut moho_app::GameClock) {
        while let Ok(event) = app.ui_event_rx.try_recv() {
            self.handle_ui_event(app, clock, event);
        }
    }

    /// Process a single UI event
    fn handle_ui_event(&self, app: &mut App, clock: &mut moho_app::GameClock, event: UiEvent) {
        match event {
            UiEvent::LoadSceneRequested { path } => {
                tracing::info!(path = %path.display(), "UI requested load scene");
                if let Err(e) = app.load_scene(clock, &path) {
                    tracing::error!(path = %path.display(), error = %e, "Failed to load scene");
                }
            }
            UiEvent::NewWorldRequested { name, seed, size } => {
                tracing::info!(name = %name, seed = ?seed, size, "UI requested new world");
                let spec = flagstone_game::scene_builders::WorldSpec {
                    name,
                    seed,
                    size_xz: size,
                    day_length_seconds: 600.0,
                    night_length_seconds: 420.0,
                    initial_time_of_day: 6.0,
                };
                if let Err(e) = app.generate_new_world(spec) {
                    tracing::error!(error = %e, "Failed to generate new world");
                }
            }
            UiEvent::ExitRequested => {
                tracing::info!("UI requested exit");
                app.exit_requested = true;
            }
            UiEvent::ResumeRequested => app.resume_game(),
            UiEvent::MainMenuRequested => app.main_menu_requested = true,
            UiEvent::MenuShown { name } => {
                tracing::info!(name = %name, "UI requested show menu");
                if let Some(ui_adapter) = &app.ui_adapter
                    && let Ok(mut adapter) = ui_adapter.lock()
                {
                    if name == "settings" {
                        // Seed from the running prefs so a console change isn't overwritten on save.
                        let path = app
                            .prefs_path
                            .clone()
                            .unwrap_or_else(moho_core::prefs::Prefs::config_path);
                        adapter.open_settings(app.prefs.clone(), path);
                    } else {
                        adapter.show_menu(&name);
                    }
                }
            }
            UiEvent::MenuHidden { name } => {
                tracing::info!(name = %name, "Menu hidden");
            }
            UiEvent::OverlayToggled { name, visible } => {
                tracing::info!(name = %name, visible, "Overlay toggled");
            }
            UiEvent::SettingsSaved => {
                tracing::info!("Settings saved");
                // Prefs never ride on the bus (ADR-0011); the adapter holds the saved copy.
                let saved = app
                    .ui_adapter
                    .as_ref()
                    .and_then(|adapter| adapter.lock().ok())
                    .and_then(|mut adapter| adapter.take_saved_prefs());
                if let Some(prefs) = saved {
                    app.adopt_settings(prefs);
                }
            }
            UiEvent::WindowSettingsChanged {
                mode,
                width,
                height,
            } => {
                tracing::info!(mode = ?mode, width, height, "Window settings changed");
                if let Some(window) = &app.window {
                    use moho_core::events::WindowMode;
                    use winit::dpi::PhysicalSize;
                    use winit::window::Fullscreen;

                    match mode {
                        WindowMode::Fullscreen | WindowMode::Borderless => {
                            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                        }
                        WindowMode::Windowed => {
                            window.set_fullscreen(None);
                            let _ = window.request_inner_size(PhysicalSize::new(width, height));
                        }
                    }
                }
            }
        }
    }

    /// Process all pending graphics events from the event bus
    pub fn process_graphics_events(&self, app: &mut App, clock: &mut moho_app::GameClock) {
        while let Ok(event) = app.graphics_event_rx.try_recv() {
            match event {
                GraphicsEvent::TimeOfDayChanged { time, .. } => {
                    clock.reset_to(time);
                    tracing::info!(
                        time,
                        time_string = %clock.time_string(),
                        "Time set"
                    );
                }
                GraphicsEvent::DebugViewChanged { mode } => {
                    app.debug_mode = mode;
                    tracing::info!(mode = %mode, "Debug view mode set");
                }
                _ => {
                    // Other graphics events not yet handled
                }
            }
        }
    }

    /// Process all pending input events (mouse wheel, etc.)
    pub fn process_input_events(&self, app: &mut App) {
        // Collect all events first to avoid borrow checker issues
        let events: Vec<InputEvent> = if let Some(rx) = &app.input.unconsumed_rx {
            rx.try_iter().collect()
        } else {
            Vec::new()
        };

        // Now process the collected events
        for iev in events {
            self.handle_input_event(app, iev);
        }
    }

    /// Handle a single input event
    fn handle_input_event(&self, app: &mut App, event: InputEvent) {
        match event {
            InputEvent::MouseWheel { delta_y } => {
                // Only act on wheel events in game mode
                if app.game_state == crate::game_state::GameState::Playing {
                    match app.simulation.camera_mode() {
                        flagstone_game::controller::CameraMode::FirstPerson => {
                            // First-person: scroll moves forward/back along look direction
                            let dz = delta_y * MOUSE_WHEEL_ZOOM_FACTOR;
                            let (yaw, pitch) = app.simulation.yaw_pitch();
                            let sy = yaw.sin();
                            let cy = yaw.cos();
                            let cp = pitch.cos();
                            let sp = pitch.sin();
                            let forward = glam::Vec3::new(sy * cp, sp, cy * cp).normalize_or_zero();
                            let new_pos = app.simulation.position() + forward * dz;
                            app.simulation.set_position_yaw_pitch(new_pos, yaw, pitch);
                        }
                        flagstone_game::controller::CameraMode::Isometric => {
                            // RTS camera: scroll adjusts camera height (zoom)
                            app.simulation.controller_input.zoom_delta = delta_y;
                        }
                    }
                }
            }
            InputEvent::MineRequested => {
                self.handle_mine_requested(app);
            }
            InputEvent::SlotSelected { slot } => {
                app.pawn.select_slot(slot);
            }
        }
    }

    /// Mine whatever voxel the player is aiming at, if anything is in range.
    /// Only acts while playing in first-person — mining isn't meaningful in
    /// the isometric/RTS camera today.
    fn handle_mine_requested(&self, app: &mut App) {
        if app.game_state != crate::game_state::GameState::Playing
            || app.simulation.camera_mode() != flagstone_game::controller::CameraMode::FirstPerson
        {
            return;
        }

        let (view_matrix, _, _) = app.camera;
        let camera_pos = view_matrix.inverse().col(3).truncate();
        let forward = -view_matrix.inverse().col(2).truncate().normalize();

        if app.pawn.equipped_tool.is_none() {
            tracing::debug!("Mine attempt blocked: no tool equipped");
            return;
        }

        let Some(light_system) = app.light_system.as_mut() else {
            return;
        };
        let grid = light_system.grid_mut();

        let Some(outcome) = app.pawn.mine(grid, camera_pos, forward, MINE_MAX_DISTANCE) else {
            tracing::debug!(
                distance = MINE_MAX_DISTANCE,
                "Mine attempt found nothing solid within range"
            );
            return;
        };

        let dirty_chunks: Vec<_> =
            flagstone_voxel::chunks_meshed_from(outcome.block_pos, grid.chunk_size()).collect();

        app.event_bus.publish(WorldEvent::BlockRemoved {
            position: outcome.block_pos,
            old_material_id: outcome.old_material_id,
            reason: BlockChangeReason::Player,
        });
        for chunk_pos in dirty_chunks {
            app.event_bus.publish(WorldEvent::ChunkMeshDirty {
                chunk_pos,
                terrain_dirty: true,
                structure_dirty: true,
            });
        }

        if let Some(y) = outcome.yield_ {
            tracing::debug!(
                pos = ?outcome.block_pos,
                distance = outcome.distance,
                resource_id = y.resource_id,
                "Mined block → resource"
            );
        } else {
            tracing::debug!(
                pos = ?outcome.block_pos,
                distance = outcome.distance,
                "Mined block (no resource)"
            );
        }
    }

    /// Check and propagate generation cancellation from UI
    pub fn check_generation_cancel(&self, app: &mut App) {
        if let Some(ui_adapter) = &app.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
            && a.take_progress_canceled()
            && let Some(cancel_flag) = &app.generation.cancel
        {
            cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Process all pending world events: every one reaches the light system,
    /// then each chunk marked dirty is re-meshed once however many events named it.
    pub fn process_world_events(&self, app: &mut App) {
        let mut dirty_chunks: HashSet<glam::IVec3> = HashSet::new();
        while let Ok(event) = app.world_event_rx.try_recv() {
            if let Some(light_system) = app.light_system.as_mut() {
                light_system.handle_event(&event);
            }
            if let WorldEvent::ChunkMeshDirty { chunk_pos, .. } = event {
                dirty_chunks.insert(chunk_pos);
            }
        }

        let Some(light_system) = &app.light_system else {
            return;
        };
        let player_chunk = lod_player_chunk(app.simulation.position());
        let chunks: Vec<(glam::IVec3, VoxelChunk)> = dirty_chunks
            .into_iter()
            .map(|chunk_pos| {
                let lod = lod_for_chunk(chunk_pos, player_chunk);
                (
                    chunk_pos,
                    VoxelChunk::from_grid_lod(light_system.grid(), chunk_pos, lod),
                )
            })
            .collect();
        for (chunk_pos, chunk) in chunks {
            if crate::app::world_geometry::insert_chunk(app, chunk).is_some() {
                tracing::trace!(chunk = ?chunk_pos, "Updated mesh for chunk");
            } else {
                tracing::trace!(chunk = ?chunk_pos, "Created new mesh for chunk");
            }
        }
    }

    /// Process all pending debug events from the event bus
    pub fn process_debug_events(&self, app: &mut App) {
        while let Ok(event) = app.debug_event_rx.try_recv() {
            self.handle_debug_event(app, event);
        }
    }

    /// Process a single debug event
    fn handle_debug_event(&self, app: &mut App, event: moho_core::events::DebugEvent) {
        use moho_core::events::DebugEvent;

        match event {
            DebugEvent::SpawnEntity {
                entity_type,
                args,
                position: _,
            } => spawn_entity(app, &entity_type, args.as_deref()),
            DebugEvent::ToggleCollision { enabled } => {
                tracing::info!(enabled, "Collision toggled");
                // enabled=false means noclip ON (collision disabled)
                app.physics.noclip = !enabled;
                if enabled {
                    let (yaw, pitch) = app.simulation.yaw_pitch();
                    app.physics.set_player_look(yaw, pitch);
                }
                tracing::info!(noclip = app.physics.noclip, "Noclip toggled");
            }
            DebugEvent::SetShadowQuality { quality } => {
                tracing::info!(quality, "Setting shadow quality");
                app.prefs.set_shadow_quality(quality);
                app.save_prefs();
                if app.window.is_some() {
                    app.pending_render
                        .push(RenderRequest::ShadowQuality(quality as u8));
                }
            }
            DebugEvent::SetSsaoQuality { quality } => {
                tracing::info!(quality, "Setting SSAO quality");
                app.prefs.set_ssao_quality(quality);
                app.save_prefs();
                if app.window.is_some() {
                    app.pending_render
                        .push(RenderRequest::SsaoQuality(quality as u8));
                }
            }
            DebugEvent::SetBloomQuality { quality } => {
                tracing::info!(quality, "Setting bloom quality");
                app.prefs.set_bloom(quality);
                app.save_prefs();
                app.pending_render
                    .push(RenderRequest::BloomQuality(app.prefs.bloom() as u8));
            }
            DebugEvent::ShowMarker {
                id,
                min,
                max,
                style,
            } => {
                app.scene.markers_mut().upsert(
                    MarkerId(id),
                    Marker::new(min.to_array(), max.to_array(), MARKER_COLOR).with_style(style),
                );
            }
            DebugEvent::ClearMarkers { id } => match id {
                Some(id) => app.scene.markers_mut().remove(MarkerId(id)),
                None => app.scene.markers_mut().clear(),
            },
            DebugEvent::SetViewHeight { height } => {
                app.cut_caps.set_cut(height);
                app.pending_render.push(RenderRequest::ViewHeight(height));
            }
            _ => {}
        }
    }
}

/// Camera position and forward direction read from a view matrix.
fn spawn_ray(view: &glam::Mat4) -> (glam::Vec3, glam::Vec3) {
    let camera_to_world = view.inverse();
    let origin = camera_to_world.col(3).truncate();
    let forward = -camera_to_world.col(2).truncate().normalize();
    (origin, forward)
}

/// Where a console spawn lands: just off the hit face, else in front of the camera.
fn spawn_position(
    origin: glam::Vec3,
    forward: glam::Vec3,
    hit: Option<&flagstone_game::raycast::RaycastResult>,
) -> glam::Vec3 {
    match hit {
        Some(hit) => {
            hit.position
                + glam::Vec3::new(
                    hit.normal.x as f32,
                    hit.normal.y as f32,
                    hit.normal.z as f32,
                ) * SPAWN_NORMAL_OFFSET
        }
        None => origin + forward * SPAWN_FALLBACK_DISTANCE,
    }
}

/// Light colour from console args "r g b"; white unless three parts are given.
fn parse_light_color(args: Option<&str>) -> glam::Vec3 {
    let Some(args) = args else {
        return glam::Vec3::ONE;
    };
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 3 {
        return glam::Vec3::ONE;
    }
    glam::Vec3::new(
        parts[0].parse().unwrap_or(1.0),
        parts[1].parse().unwrap_or(1.0),
        parts[2].parse().unwrap_or(1.0),
    )
}

/// Spawn `kind` where the camera is aimed. Does nothing without a light system,
/// which owns the voxel grid the aim raycast needs.
fn spawn_entity(app: &mut App, kind: &str, args: Option<&str>) {
    let (origin, forward) = spawn_ray(&app.camera.0);
    let Some(light_system) = app.light_system.as_mut() else {
        return;
    };
    let hit = flagstone_game::raycast::raycast(
        light_system.grid_mut(),
        origin,
        forward,
        SPAWN_RAYCAST_MAX_DISTANCE,
    );
    let spawn_pos = spawn_position(origin, forward, hit.as_ref());

    tracing::info!(entity_type = %kind, pos = ?spawn_pos, "Spawning entity");

    match kind.to_lowercase().as_str() {
        "crystal" => spawn_crystal(app, spawn_pos),
        "light" => spawn_light(app, spawn_pos, args),
        "cube" => spawn_cube(app, spawn_pos),
        "sphere" => spawn_sphere(app, spawn_pos),
        _ => tracing::warn!(entity_type = %kind, "Unknown entity type"),
    }
}

fn spawn_crystal(app: &App, pos: glam::Vec3) {
    // The event path lets the light and mesh systems react.
    app.event_bus.publish(WorldEvent::BlockPlaced {
        position: pos.floor().as_ivec3(),
        material_id: CRYSTAL_MATERIAL_ID,
        reason: BlockChangeReason::Player,
    });
}

fn spawn_light(app: &mut App, pos: glam::Vec3, args: Option<&str>) {
    // The renderer is only reachable from `frame`, so the light is queued.
    if app.window.is_none() {
        return;
    }
    let color = parse_light_color(args);
    app.pending_render.push(RenderRequest::AddPointLight {
        position: pos,
        color,
        intensity: DEFAULT_POINT_LIGHT_INTENSITY,
        range: DEFAULT_POINT_LIGHT_RANGE,
    });
    tracing::info!(pos = ?pos, "Added point light");

    // A small emissive gizmo makes the light origin visible; emissive bypasses
    // lighting so it glows at the light's own colour.
    let gizmo = flagstone_game::actors::Sphere::new(
        pos,
        0.15,
        flagstone_voxel::MaterialType::Emissive {
            color,
            intensity: 1.5,
        },
    );
    app.entities.actors.spawn_sphere(gizmo);
}

fn spawn_cube(app: &mut App, pos: glam::Vec3) {
    let cube = flagstone_game::actors::Cube::new(
        pos,
        1.0,
        1.0,
        1.0,
        flagstone_voxel::MaterialType::Lambertian {
            albedo: glam::Vec3::new(0.8, 0.2, 0.2),
        },
    );
    let entity = app.entities.actors.spawn_cube(cube);
    if let Some(pw) = app.physics.world.as_mut() {
        let handle = pw.add_dynamic_cuboid(pos, 0.5, 0.5, 0.5);
        app.physics.test_bodies.push((handle, entity));
    }
    tracing::info!(pos = ?pos, "Spawned cube");
}

fn spawn_sphere(app: &mut App, pos: glam::Vec3) {
    let sphere = flagstone_game::actors::Sphere::new(
        pos,
        0.5,
        flagstone_voxel::MaterialType::Metal {
            albedo: glam::Vec3::new(0.8, 0.8, 0.8),
            fuzz: 0.1,
        },
    );
    let entity = app.entities.actors.spawn_sphere(sphere);
    if let Some(pw) = app.physics.world.as_mut() {
        let handle = pw.add_dynamic_sphere(pos, 0.5);
        app.physics.test_bodies.push((handle, entity));
    }
    tracing::info!(pos = ?pos, "Spawned sphere");
}

/// Compute chunk coordinates from a world-space position.
pub(crate) fn lod_player_chunk(pos: glam::Vec3) -> glam::IVec3 {
    const CHUNK_SIZE: i32 = 16;
    glam::IVec3::new(
        (pos.x.floor() as i32).div_euclid(CHUNK_SIZE),
        (pos.y.floor() as i32).div_euclid(CHUNK_SIZE),
        (pos.z.floor() as i32).div_euclid(CHUNK_SIZE),
    )
}

/// Determine the LOD tier for a chunk given the player's chunk position.
///
/// Uses Chebyshev XZ distance (max of |dx|, |dz|) so all chunks in a square ring
/// at the same XZ distance share a tier, regardless of vertical offset.
///
/// - LOD 0 (< 4 chunks): full 16³ hybrid mesh
/// - LOD 1 (4–16 chunks): coarse 8³ blocky mesh
pub(crate) fn lod_for_chunk(chunk_pos: glam::IVec3, player_chunk: glam::IVec3) -> u8 {
    let dx = (chunk_pos.x - player_chunk.x).abs();
    let dz = (chunk_pos.z - player_chunk.z).abs();
    let dist = dx.max(dz);
    u8::from(dist >= 4)
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ui_exit_request_sets_the_exit_flag() {
        let mut app = App::headless();
        app.event_bus.publish(UiEvent::ExitRequested);
        app.event_bus.process_deferred();
        assert!(!app.exit_requested, "nothing processed yet");

        EventProcessor::new().process_ui_events(&mut app, &mut moho_app::GameClock::default());

        assert!(app.exit_requested);
    }

    use super::*;
    use moho_core::events::DebugEvent;

    fn publish_spawn(app: &App, entity_type: &str) {
        app.event_bus.publish(DebugEvent::SpawnEntity {
            entity_type: entity_type.to_string(),
            args: None,
            position: None,
        });
    }

    /// Where a console spawn lands when the raycast hits nothing.
    fn fallback_spawn_pos(app: &App) -> glam::Vec3 {
        let camera_to_world = app.camera.0.inverse();
        let camera_pos = camera_to_world.col(3).truncate();
        let forward = -camera_to_world.col(2).truncate().normalize();
        camera_pos + forward * SPAWN_FALLBACK_DISTANCE
    }

    #[test]
    fn toggle_collision_off_enables_noclip() {
        let mut app = App::headless();
        app.event_bus
            .publish(DebugEvent::ToggleCollision { enabled: false });

        EventProcessor::new().process_debug_events(&mut app);

        assert!(app.physics.noclip);
    }

    #[test]
    fn toggle_collision_on_disables_noclip() {
        let mut app = App::headless();
        app.physics.noclip = true;
        app.event_bus
            .publish(DebugEvent::ToggleCollision { enabled: true });

        EventProcessor::new().process_debug_events(&mut app);

        assert!(!app.physics.noclip);
    }

    #[test]
    fn spawn_sphere_adds_actor_with_physics_body() {
        let mut app = App::headless();
        publish_spawn(&app, "sphere");

        EventProcessor::new().process_debug_events(&mut app);

        let spheres = app.entities.actors.spheres();
        assert_eq!(spheres.len(), 1);
        assert!(spheres[0].center.distance(fallback_spawn_pos(&app)) < 1e-3);
        assert!(app.entities.actors.cubes().is_empty());
        assert_eq!(app.physics.test_bodies.len(), 1);
    }

    #[test]
    fn spawn_cube_adds_actor_with_physics_body() {
        let mut app = App::headless();
        publish_spawn(&app, "cube");

        EventProcessor::new().process_debug_events(&mut app);

        let cubes = app.entities.actors.cubes();
        assert_eq!(cubes.len(), 1);
        assert!(cubes[0].center.distance(fallback_spawn_pos(&app)) < 1e-3);
        assert!(app.entities.actors.spheres().is_empty());
        assert_eq!(app.physics.test_bodies.len(), 1);
    }

    #[test]
    fn spawn_crystal_publishes_crystal_placement_at_the_spawn_block() {
        let mut app = App::headless();
        let placed = collect_block_placements(&app);
        publish_spawn(&app, "crystal");

        EventProcessor::new().process_debug_events(&mut app);

        let expected = fallback_spawn_pos(&app).floor().as_ivec3();
        assert_eq!(
            *placed.lock().unwrap(),
            vec![(
                expected,
                flagstone_voxel::CRYSTAL_MATERIAL_ID,
                BlockChangeReason::Player
            )]
        );
        assert!(app.entities.actors.spheres().is_empty());
        assert!(app.entities.actors.cubes().is_empty());
    }

    #[test]
    fn spawn_torch_places_nothing() {
        let mut app = App::headless();
        let placed = collect_block_placements(&app);
        publish_spawn(&app, "torch");

        EventProcessor::new().process_debug_events(&mut app);

        assert!(placed.lock().unwrap().is_empty());
        assert!(app.entities.actors.spheres().is_empty());
        assert!(app.entities.actors.cubes().is_empty());
    }

    type Placements = std::sync::Arc<std::sync::Mutex<Vec<(glam::IVec3, u32, BlockChangeReason)>>>;

    fn collect_block_placements(app: &App) -> Placements {
        let placed = Placements::default();
        let sink = std::sync::Arc::clone(&placed);
        app.event_bus.subscribe(move |e: &WorldEvent| {
            if let WorldEvent::BlockPlaced {
                position,
                material_id,
                reason,
            } = e
            {
                sink.lock()
                    .unwrap()
                    .push((*position, *material_id, reason.clone()));
            }
        });
        placed
    }

    #[test]
    fn chunk_mesh_dirty_event_stores_meshed_chunk_with_collider() {
        let mut app = App::headless();
        let chunk_pos = glam::IVec3::new(0, 4, 0);
        app.light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut()
            .mutator()
            .place(flagstone_voxel::BlockPos::new(3, 70, 3), 1, None);
        app.event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos,
            terrain_dirty: true,
            structure_dirty: false,
        });

        EventProcessor::new().process_world_events(&mut app);

        let chunk = app.entities.chunks.get(chunk_pos).expect("chunk is stored");
        assert!(!chunk.vertices().is_empty(), "the placed block is meshed");
        let id = crate::app::world_geometry::chunk_mesh_id(chunk_pos);
        let pw = app.physics.world.as_ref().expect("physics world");
        assert!(pw.world_mesh_collider(id).is_some());
    }

    #[test]
    fn mine_request_in_first_person_removes_aimed_block_and_publishes_removal() {
        let mut app = App::headless();
        app.game_state = crate::game_state::GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        // Identity rotation: the camera looks down -Z from just in front of the wall.
        app.camera.0 = glam::Mat4::from_translation(glam::Vec3::new(4.5, 70.5, 6.5)).inverse();
        let aimed = flagstone_voxel::BlockPos::new(4, 70, 3);
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        for x in 2..=6 {
            for y in 68..=72 {
                for z in 1..=3 {
                    grid.mutator()
                        .place(flagstone_voxel::BlockPos::new(x, y, z), 1, None);
                }
            }
        }
        assert!(grid.is_solid_at(aimed));
        let removed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&removed);
        app.event_bus.subscribe(move |e: &WorldEvent| {
            if let WorldEvent::BlockRemoved { position, .. } = e {
                sink.lock().unwrap().push(*position);
            }
        });

        EventProcessor::new().handle_input_event(&mut app, InputEvent::MineRequested);

        let removed = removed.lock().unwrap();
        assert_eq!(removed.len(), 1, "exactly one block is mined");
        let mined = removed[0];
        let grid = app.light_system.as_ref().expect("light system").grid();
        assert!(
            !grid.is_solid_at(mined),
            "the mined block is gone from the grid"
        );
        assert!(
            mined.z == 3 && mined.x == 4,
            "the block facing the camera is mined: {mined:?}"
        );
    }

    #[test]
    fn mining_a_chunk_edge_block_dirties_the_negative_neighbours() {
        let mut app = App::headless();
        app.game_state = crate::game_state::GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        // The camera looks down -Z at block (16, 70, 3): local x 0 of chunk (1, 4, 0).
        app.camera.0 = glam::Mat4::from_translation(glam::Vec3::new(16.5, 70.5, 6.5)).inverse();
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        // Solid only from x = 16, so the negative-x neighbour chunk holds no blocks at all.
        for x in 16..=18 {
            for y in 68..=72 {
                for z in 1..=3 {
                    grid.mutator()
                        .place(flagstone_voxel::BlockPos::new(x, y, z), 1, None);
                }
            }
        }
        let removed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let dirty = std::sync::Arc::new(std::sync::Mutex::new(HashSet::new()));
        let removed_sink = std::sync::Arc::clone(&removed);
        let dirty_sink = std::sync::Arc::clone(&dirty);
        app.event_bus.subscribe(move |e: &WorldEvent| match e {
            WorldEvent::BlockRemoved { position, .. } => {
                removed_sink.lock().unwrap().push(*position);
            }
            WorldEvent::ChunkMeshDirty { chunk_pos, .. } => {
                dirty_sink.lock().unwrap().insert(*chunk_pos);
            }
            _ => {}
        });

        EventProcessor::new().handle_input_event(&mut app, InputEvent::MineRequested);

        assert_eq!(
            *removed.lock().unwrap(),
            vec![flagstone_voxel::BlockPos::new(16, 70, 3)]
        );
        let expected: HashSet<glam::IVec3> = [glam::IVec3::new(1, 4, 0), glam::IVec3::new(0, 4, 0)]
            .into_iter()
            .collect();
        assert_eq!(*dirty.lock().unwrap(), expected);
    }

    #[test]
    fn mined_block_queues_a_light_job_at_its_position() {
        let mut app = App::headless();
        app.game_state = crate::game_state::GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::FirstPerson);
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        app.camera.0 = glam::Mat4::from_translation(glam::Vec3::new(4.5, 70.5, 6.5)).inverse();
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        for x in 2..=6 {
            for y in 68..=72 {
                for z in 1..=3 {
                    grid.mutator()
                        .place(flagstone_voxel::BlockPos::new(x, y, z), 1, None);
                }
            }
        }
        let removed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&removed);
        app.event_bus.subscribe(move |e: &WorldEvent| {
            if let WorldEvent::BlockRemoved { position, .. } = e {
                sink.lock().unwrap().push(*position);
            }
        });
        let processor = EventProcessor::new();
        processor.handle_input_event(&mut app, InputEvent::MineRequested);
        let mined = removed.lock().unwrap()[0];
        // Light left behind by the mined block: only a job at that position clears it.
        let light_system = app.light_system.as_mut().expect("light system");
        light_system
            .grid_mut()
            .set_block_light_rgb(mined, [10, 0, 0]);
        assert_eq!(
            light_system.pending_jobs(),
            0,
            "nothing queued before routing"
        );

        processor.process_world_events(&mut app);

        let light_system = app.light_system.as_mut().expect("light system");
        assert_eq!(light_system.pending_jobs(), 1, "one job for the one block");
        assert_eq!(light_system.process_frame(), 1);
        assert_eq!(
            light_system.grid().block_light_rgb_at(mined),
            [0, 0, 0],
            "the job ran at the mined position"
        );
        assert!(
            light_system.emit_dirty_events() > 0,
            "the cleared light dirties its chunk"
        );
    }

    #[test]
    fn placed_and_batch_modified_events_reach_the_light_system() {
        let mut app = App::headless();
        let placed_at = flagstone_voxel::BlockPos::new(3, 70, 3);
        let batch = [
            flagstone_voxel::BlockPos::new(20, 70, 3),
            flagstone_voxel::BlockPos::new(21, 70, 3),
            flagstone_voxel::BlockPos::new(40, 70, 3),
        ];
        let light_system = app.light_system.as_mut().expect("light system");
        let grid = light_system.grid_mut();
        for pos in [placed_at, batch[0], batch[1]] {
            grid.mutator().place(pos, 1, None);
        }
        for pos in [placed_at, batch[0], batch[1], batch[2]] {
            grid.set_block_light_rgb(pos, [10, 0, 0]);
        }
        let _ = app.world_event_rx.try_iter().count();
        app.event_bus.publish(WorldEvent::BlockPlaced {
            position: placed_at,
            material_id: 1,
            reason: BlockChangeReason::Player,
        });
        app.event_bus.publish(WorldEvent::BlocksBatchModified {
            chunk_pos: glam::IVec3::new(1, 4, 0),
            positions: batch.to_vec(),
            reason: BlockChangeReason::Explosion,
        });

        EventProcessor::new().process_world_events(&mut app);

        let light_system = app.light_system.as_mut().expect("light system");
        assert_eq!(
            light_system.pending_jobs(),
            4,
            "one for the opaque placement, one per batched position, the empty one included"
        );
        assert_eq!(light_system.process_frame(), 4);
        let grid = light_system.grid();
        for pos in [placed_at, batch[0], batch[1], batch[2]] {
            assert_eq!(grid.block_light_rgb_at(pos), [0, 0, 0], "{pos} reconciled");
        }
    }

    #[test]
    fn duplicate_mesh_dirty_events_mesh_a_chunk_once() {
        let chunk_pos = glam::IVec3::new(0, 4, 0);
        let meshed_collider = |dirty_flags: &[(bool, bool)]| {
            let mut app = App::headless();
            app.light_system
                .as_mut()
                .expect("App starts with a light system")
                .grid_mut()
                .mutator()
                .place(flagstone_voxel::BlockPos::new(3, 70, 3), 1, None);
            for &(terrain_dirty, structure_dirty) in dirty_flags {
                app.event_bus.publish(WorldEvent::ChunkMeshDirty {
                    chunk_pos,
                    terrain_dirty,
                    structure_dirty,
                });
            }

            EventProcessor::new().process_world_events(&mut app);

            let id = crate::app::world_geometry::chunk_mesh_id(chunk_pos);
            app.physics
                .world
                .as_ref()
                .expect("physics world")
                .world_mesh_collider(id)
                .expect("the chunk is meshed")
        };

        let once = meshed_collider(&[(true, false)]);
        let thrice = meshed_collider(&[(true, false), (false, true), (true, true)]);

        // Each meshing replaces the collider, so a repeat would hand out a newer handle.
        assert_eq!(thrice, once, "the chunk was meshed once, not three times");
    }

    #[test]
    fn mine_request_while_playing_outside_first_person_leaves_aimed_block() {
        let mut app = App::headless();
        app.game_state = crate::game_state::GameState::Playing;
        app.simulation
            .set_camera_mode(flagstone_game::controller::CameraMode::Isometric);
        app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
        // Identity rotation: the camera looks down -Z from just in front of the wall.
        app.camera.0 = glam::Mat4::from_translation(glam::Vec3::new(4.5, 70.5, 6.5)).inverse();
        let aimed = flagstone_voxel::BlockPos::new(4, 70, 3);
        let grid = app
            .light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut();
        for x in 2..=6 {
            for y in 68..=72 {
                for z in 1..=3 {
                    grid.mutator()
                        .place(flagstone_voxel::BlockPos::new(x, y, z), 1, None);
                }
            }
        }
        assert!(grid.is_solid_at(aimed));

        EventProcessor::new().handle_input_event(&mut app, InputEvent::MineRequested);

        let grid = app.light_system.as_ref().expect("light system").grid();
        assert!(grid.is_solid_at(aimed), "the aimed block is not mined");
    }

    #[test]
    fn spawn_position_offsets_a_hit_along_its_normal() {
        let hit = flagstone_game::raycast::RaycastResult {
            block_pos: flagstone_voxel::BlockPos::new(4, 9, -3),
            normal: glam::IVec3::new(0, -1, 0),
            position: glam::Vec3::new(4.25, 9.0, -2.5),
            distance: 7.0,
        };

        let pos = spawn_position(
            glam::Vec3::new(1.0, 2.0, 3.0),
            glam::Vec3::new(0.0, 0.0, -1.0),
            Some(&hit),
        );

        let expected = glam::Vec3::new(4.25, 9.0 - SPAWN_NORMAL_OFFSET, -2.5);
        assert!(pos.distance(expected) < 1e-5, "{pos:?}");
    }

    #[test]
    fn spawn_position_without_a_hit_is_in_front_of_the_camera() {
        let origin = glam::Vec3::new(1.0, 2.0, 3.0);
        let forward = glam::Vec3::new(0.0, 0.6, -0.8);

        let pos = spawn_position(origin, forward, None);

        let expected = origin + forward * SPAWN_FALLBACK_DISTANCE;
        assert!(pos.distance(expected) < 1e-5, "{pos:?}");
    }

    #[test]
    fn spawn_ray_reads_origin_and_forward_from_the_view_matrix() {
        let eye = glam::Vec3::new(3.0, -2.0, 7.0);
        let dir = glam::Vec3::new(2.0, 1.0, -2.0) / 3.0;
        let look = glam::Quat::from_rotation_arc(glam::Vec3::NEG_Z, dir);
        let view = glam::Mat4::from_rotation_translation(look, eye).inverse();

        let (origin, forward) = spawn_ray(&view);

        assert!(origin.distance(eye) < 1e-4, "origin {origin:?}");
        assert!(forward.distance(dir) < 1e-4, "forward {forward:?}");
    }

    #[test]
    fn light_color_reads_three_channels() {
        let color = parse_light_color(Some("0.2 0.4 0.6"));

        assert_eq!(color, glam::Vec3::new(0.2, 0.4, 0.6));
    }

    #[test]
    fn light_color_defaults_to_white() {
        for args in [None, Some(""), Some("0.2 0.4")] {
            assert_eq!(parse_light_color(args), glam::Vec3::ONE, "args {args:?}");
        }
    }

    #[test]
    fn light_color_defaults_an_unparseable_channel_to_one() {
        let color = parse_light_color(Some("0.2 abc 0.6"));

        assert_eq!(color, glam::Vec3::new(0.2, 1.0, 0.6));
    }

    #[test]
    fn lod_player_chunk_maps_world_pos_to_chunk() {
        let cases = [
            (glam::Vec3::new(0.0, 0.0, 0.0), glam::IVec3::new(0, 0, 0)),
            (glam::Vec3::new(16.0, 0.0, 16.0), glam::IVec3::new(1, 0, 1)),
            (
                glam::Vec3::new(-1.0, 0.0, -1.0),
                glam::IVec3::new(-1, 0, -1),
            ),
            (glam::Vec3::new(0.0, -1.0, 0.0), glam::IVec3::new(0, -1, 0)),
            (glam::Vec3::new(0.0, 32.0, 0.0), glam::IVec3::new(0, 2, 0)),
        ];

        for (pos, expected) in cases {
            assert_eq!(lod_player_chunk(pos), expected, "row {pos:?}");
        }
    }

    #[test]
    fn lod_for_chunk_tiers_by_chebyshev_xz_distance_from_the_player() {
        let origin = glam::IVec3::ZERO;
        let off_origin = glam::IVec3::new(10, 0, -7);
        let mixed_sign = glam::IVec3::new(-5, 2, 5);
        let cases = [
            (origin, glam::IVec3::new(0, 0, 0), 0),
            (origin, glam::IVec3::new(3, 0, 0), 0),
            (origin, glam::IVec3::new(-3, 0, 0), 0),
            (origin, glam::IVec3::new(3, 0, 3), 0),
            (origin, glam::IVec3::new(0, 10, 0), 0),
            (origin, glam::IVec3::new(4, 0, 0), 1),
            (origin, glam::IVec3::new(-4, 0, 0), 1),
            (origin, glam::IVec3::new(0, 0, 4), 1),
            (origin, glam::IVec3::new(3, 0, 4), 1),
            (origin, glam::IVec3::new(16, 0, 0), 1),
            (off_origin, glam::IVec3::new(13, 0, -7), 0),
            (off_origin, glam::IVec3::new(14, 0, -7), 1),
            (off_origin, glam::IVec3::new(10, 0, -11), 1),
            (mixed_sign, glam::IVec3::new(-2, 0, 8), 0),
            (mixed_sign, glam::IVec3::new(-5, 9, 1), 1),
        ];

        for (player, chunk, expected) in cases {
            assert_eq!(
                lod_for_chunk(chunk, player),
                expected,
                "player {player:?}, chunk {chunk:?}"
            );
        }
    }

    use flagstone_ui::actions::StrategyAction;
    use moho_core::prefs::{Prefs, WindowMode};
    use moho_input::bindings::Binding;
    use moho_input::key::Key;
    use std::path::{Path, PathBuf};

    fn headless_saving_to(path: &Path) -> App {
        let mut app = App::headless();
        app.prefs_path = Some(path.to_path_buf());
        app
    }

    fn forward_bound_to_q(prefs: &mut Prefs) {
        prefs.set_bindings(
            [("move_forward".to_string(), "Q".to_string())]
                .into_iter()
                .collect(),
        );
    }

    fn forward_keys(app: &App) -> Vec<Binding> {
        app.input
            .actions
            .bindings()
            .get(StrategyAction::MoveForward)
            .to_vec()
    }

    fn run_console_shadow_command(app: &mut App, quality: u32) {
        app.event_bus
            .publish(DebugEvent::SetShadowQuality { quality });
        EventProcessor::new().process_debug_events(app);
    }

    fn prefs_file(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("prefs.ini")
    }

    fn attach_adapter(
        app: &mut App,
    ) -> std::sync::Arc<std::sync::Mutex<flagstone_ui::EguiAdapter>> {
        let adapter = std::sync::Arc::new(std::sync::Mutex::new(flagstone_ui::EguiAdapter::new(
            None,
            app.event_bus.clone(),
            app.prefs.clone(),
        )));
        app.ui_adapter = Some(adapter.clone());
        adapter
    }

    #[test]
    fn console_shadow_command_keeps_saved_settings() {
        type Stage = fn(&mut Prefs);
        type Holds = fn(&Prefs) -> bool;
        let rows: [(&str, Stage, Holds); 2] = [
            ("a key binding", forward_bound_to_q, |p| {
                p.bindings().get("move_forward").map(String::as_str) == Some("Q")
            }),
            (
                "the window mode",
                |p| p.set_window_mode(WindowMode::Borderless),
                |p| p.window_mode() == WindowMode::Borderless,
            ),
        ];

        for (row, stage, holds) in rows {
            let dir = tempfile::tempdir().unwrap();
            let path = prefs_file(&dir);
            let mut app = headless_saving_to(&path);
            let mut saved = Prefs::default();
            stage(&mut saved);
            saved.save_to(&path).unwrap();
            app.adopt_settings(saved);

            run_console_shadow_command(&mut app, 1);

            let on_disk = Prefs::load_from(&path).0;
            assert!(holds(&on_disk), "row {row}: the Settings change was lost");
            assert_eq!(on_disk.shadow_quality(), 1, "row {row}: shadow not saved");
            assert!(holds(&app.prefs), "row {row}: the running prefs lost it");
        }
    }

    #[test]
    fn settings_opened_after_a_console_command_save_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = prefs_file(&dir);
        let mut app = headless_saving_to(&path);
        let adapter = attach_adapter(&mut app);
        run_console_shadow_command(&mut app, 1);
        app.event_bus.publish(UiEvent::MenuShown {
            name: "settings".to_string(),
        });
        app.event_bus.process_deferred();
        EventProcessor::new().process_ui_events(&mut app, &mut moho_app::GameClock::default());

        {
            let mut adapter = adapter.lock().unwrap();
            let menu = adapter
                .settings_menu_mut()
                .expect("the settings screen is registered");
            menu.start_listening(0);
            assert!(menu.apply_key_while_listening(Key::Q));
            menu.apply_staged_changes().unwrap();
            // The Save button's action carries the screen's saved prefs.
            let saved = flagstone_ui::screens::MenuAction::SettingsSaved(Prefs::load_from(&path).0);
            adapter.record_menu_action(&saved);
        }
        app.event_bus.publish(UiEvent::SettingsSaved);
        app.event_bus.process_deferred();
        EventProcessor::new().process_ui_events(&mut app, &mut moho_app::GameClock::default());

        let on_disk = Prefs::load_from(&path).0;
        assert_eq!(on_disk.shadow_quality(), 1, "the console change was lost");
        // Capturing a key keeps the default pad binding, so only the key is checked.
        assert!(
            on_disk
                .bindings()
                .get("move_forward")
                .is_some_and(|b| b.split(", ").any(|k| k == "Q")),
            "the Settings binding was lost: {:?}",
            on_disk.bindings().get("move_forward")
        );
        assert_eq!(app.prefs.shadow_quality(), 1);
        assert!(forward_keys(&app).contains(&Binding::key(Key::Q)));
        assert!(!forward_keys(&app).contains(&Binding::key(Key::W)));
    }

    #[test]
    fn saved_binding_and_console_shadow_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = prefs_file(&dir);
        let mut app = headless_saving_to(&path);
        let mut saved = Prefs::default();
        forward_bound_to_q(&mut saved);
        saved.save_to(&path).unwrap();
        app.adopt_settings(saved);
        run_console_shadow_command(&mut app, 1);

        let restarted = App::from_config(crate::app::config::AppConfig::from_prefs_struct(
            Prefs::load_from(&path).0,
        ));

        let keys = forward_keys(&restarted);
        assert!(keys.contains(&Binding::key(Key::Q)), "bindings: {keys:?}");
        assert!(!keys.contains(&Binding::key(Key::W)), "bindings: {keys:?}");
        assert_eq!(restarted.prefs.shadow_quality(), 1);
    }

    #[test]
    fn adopting_settings_rebinds_the_running_game() {
        let dir = tempfile::tempdir().unwrap();
        let path = prefs_file(&dir);
        let mut app = headless_saving_to(&path);
        assert!(forward_keys(&app).contains(&Binding::key(Key::W)));
        let mut adopted = Prefs::default();
        forward_bound_to_q(&mut adopted);

        app.adopt_settings(adopted.clone());

        let keys = forward_keys(&app);
        assert!(keys.contains(&Binding::key(Key::Q)), "bindings: {keys:?}");
        assert!(!keys.contains(&Binding::key(Key::W)), "bindings: {keys:?}");
        assert_eq!(app.prefs, adopted);
        assert!(!path.exists(), "adopting does not write the file");
    }

    fn look_after_mouse_up(app: &mut App) -> (f32, f32) {
        app.input.actions.mouse_motion(100.0, -100.0);
        app.input
            .actions
            .sample_frame(std::time::Duration::from_millis(16));
        app.input.actions.end_tick().look()
    }

    #[test]
    fn adopted_sensitivity_and_invert_apply_to_the_next_motion() {
        let dir = tempfile::tempdir().unwrap();
        let path = prefs_file(&dir);
        let mut app = headless_saving_to(&path);
        let before = look_after_mouse_up(&mut app);
        assert!(before.1 < 0.0, "mouse up pitches up by default: {before:?}");
        let saved = Prefs::default()
            .with_mouse_sensitivity(1.8)
            .with_invert_mouse_y(true);
        saved.save_to(&path).unwrap();

        app.adopt_settings(saved);
        let after = look_after_mouse_up(&mut app);

        let per_count = moho_input::action_map::source_look_per_count(1.8);
        assert!((after.0 - 100.0 * per_count).abs() < 1e-5, "yaw {after:?}");
        assert!(
            (after.1 - 100.0 * per_count).abs() < 1e-5,
            "pitch {after:?}"
        );

        let restarted_prefs = Prefs::load_from(&path).0;
        assert_eq!(restarted_prefs.mouse_sensitivity(), 1.8);
        assert!(restarted_prefs.invert_mouse_y());
        let mut restarted = App::from_config(crate::app::config::AppConfig::from_prefs_struct(
            restarted_prefs,
        ));
        let again = look_after_mouse_up(&mut restarted);
        assert!(
            (again.1 - 100.0 * per_count).abs() < 1e-5,
            "restart {again:?}"
        );
    }

    #[test]
    fn console_prefs_command_updates_prefs_without_a_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = prefs_file(&dir);
        let mut app = headless_saving_to(&path);
        assert!(app.window.is_none());

        run_console_shadow_command(&mut app, 1);
        app.event_bus
            .publish(DebugEvent::SetSsaoQuality { quality: 2 });
        EventProcessor::new().process_debug_events(&mut app);

        assert_eq!(app.prefs.shadow_quality(), 1);
        assert_eq!(app.prefs.ssao_quality(), 2);
        let on_disk = Prefs::load_from(&path).0;
        assert_eq!(on_disk.shadow_quality(), 1);
        assert_eq!(on_disk.ssao_quality(), 2);
        assert!(
            app.pending_render.is_empty(),
            "no window, so nothing is queued for the renderer"
        );
    }

    #[test]
    fn set_bloom_quality_saves_and_queues_the_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = prefs_file(&dir);
        let mut app = headless_saving_to(&path);
        app.event_bus
            .publish(DebugEvent::SetBloomQuality { quality: 1 });

        EventProcessor::new().process_debug_events(&mut app);

        assert_eq!(app.prefs.bloom(), 1);
        assert_eq!(Prefs::load_from(&path).0.bloom(), 1);
        assert!(
            matches!(
                app.pending_render.as_slice(),
                [RenderRequest::BloomQuality(1)]
            ),
            "queued {} requests",
            app.pending_render.len()
        );
    }

    #[test]
    fn adopting_vsync_off_queues_a_vsync_request() {
        let mut app = App::headless();
        let mut off = Prefs::default();
        off.set_vsync(false);

        app.adopt_settings(off);

        assert!(
            matches!(app.pending_render.as_slice(), [RenderRequest::Vsync(false)]),
            "queued {} requests",
            app.pending_render.len()
        );
    }

    #[test]
    fn unchanged_vsync_queues_no_render_request() {
        let mut app = App::headless();
        let mut other_change = Prefs::default();
        other_change.set_fps_max(60);

        app.adopt_settings(other_change);
        app.adopt_settings(Prefs::default());

        assert!(app.pending_render.is_empty());
    }

    #[test]
    fn view_height_event_queues_a_render_request() {
        let mut app = App::headless();
        app.event_bus
            .publish(DebugEvent::SetViewHeight { height: Some(12.5) });
        app.event_bus
            .publish(DebugEvent::SetViewHeight { height: None });

        EventProcessor::new().process_debug_events(&mut app);

        assert!(
            matches!(
                app.pending_render.as_slice(),
                [
                    RenderRequest::ViewHeight(Some(h)),
                    RenderRequest::ViewHeight(None),
                ] if *h == 12.5
            ),
            "queued {} requests",
            app.pending_render.len()
        );
    }

    #[test]
    fn view_height_event_sets_the_cut() {
        let mut app = App::headless();
        app.event_bus
            .publish(DebugEvent::SetViewHeight { height: Some(12.5) });

        EventProcessor::new().process_debug_events(&mut app);

        assert_eq!(app.cut_caps.cut(), Some(12.5));
        assert!(
            matches!(
                app.pending_render.as_slice(),
                [RenderRequest::ViewHeight(Some(h))] if *h == 12.5
            ),
            "the renderer request is still queued"
        );

        app.event_bus
            .publish(DebugEvent::SetViewHeight { height: None });
        EventProcessor::new().process_debug_events(&mut app);

        assert_eq!(
            app.cut_caps.cut(),
            None,
            "clearing the height clears the cut"
        );
    }

    fn show_marker(app: &App, id: u64, min: glam::Vec3, max: glam::Vec3) {
        show_styled_marker(app, id, min, max, moho_render_api::MarkerStyle::Fill);
    }

    fn show_styled_marker(
        app: &App,
        id: u64,
        min: glam::Vec3,
        max: glam::Vec3,
        style: moho_render_api::MarkerStyle,
    ) {
        app.event_bus.publish(DebugEvent::ShowMarker {
            id,
            min,
            max,
            style,
        });
    }

    fn marker_draws(app: &mut App) -> Vec<moho_render_api::MarkerGpu> {
        app.scene.markers_mut().draws(glam::Vec3::ZERO).fills
    }

    #[test]
    fn show_marker_event_with_outline_style_draws_in_the_outline_list() {
        let mut app = App::headless();
        show_styled_marker(
            &app,
            3,
            glam::Vec3::ZERO,
            glam::Vec3::ONE,
            moho_render_api::MarkerStyle::Fill,
        );
        show_styled_marker(
            &app,
            3,
            glam::Vec3::ZERO,
            glam::Vec3::ONE,
            moho_render_api::MarkerStyle::Outline,
        );

        EventProcessor::new().process_debug_events(&mut app);

        let draws = app.scene.markers_mut().draws(glam::Vec3::ZERO);
        assert!(draws.fills.is_empty(), "the replaced fill is gone");
        assert_eq!(draws.outlines.len(), 1);
    }

    #[test]
    fn show_marker_event_draws_one_translucent_marker_at_the_given_box() {
        let mut app = App::headless();
        show_marker(
            &app,
            3,
            glam::Vec3::new(1.0, 2.0, 3.0),
            glam::Vec3::new(4.0, 5.0, 6.0),
        );

        EventProcessor::new().process_debug_events(&mut app);

        let draws = marker_draws(&mut app);
        assert_eq!(draws.len(), 1);
        let grow = moho_renderer::MARKER_INFLATE;
        for axis in 0..3 {
            let lo = [1.0, 2.0, 3.0][axis] - grow;
            let hi = [4.0, 5.0, 6.0][axis] + grow;
            assert!((draws[0].min[axis] - lo).abs() < 1e-5, "min {axis}");
            assert!((draws[0].max[axis] - hi).abs() < 1e-5, "max {axis}");
        }
        assert!(draws[0].color[3] > 0.0 && draws[0].color[3] < 1.0);
    }

    #[test]
    fn show_marker_event_with_the_same_id_replaces_the_marker() {
        let mut app = App::headless();
        show_marker(&app, 3, glam::Vec3::ZERO, glam::Vec3::ONE);
        show_marker(&app, 3, glam::Vec3::splat(10.0), glam::Vec3::splat(12.0));

        EventProcessor::new().process_debug_events(&mut app);

        let draws = marker_draws(&mut app);
        assert_eq!(draws.len(), 1);
        assert!((draws[0].min[0] - (10.0 - moho_renderer::MARKER_INFLATE)).abs() < 1e-4);
    }

    #[test]
    fn clear_markers_event_with_an_id_removes_only_that_marker() {
        let mut app = App::headless();
        show_marker(&app, 1, glam::Vec3::ZERO, glam::Vec3::ONE);
        show_marker(&app, 2, glam::Vec3::splat(10.0), glam::Vec3::splat(11.0));
        app.event_bus
            .publish(DebugEvent::ClearMarkers { id: Some(1) });

        EventProcessor::new().process_debug_events(&mut app);

        let draws = marker_draws(&mut app);
        assert_eq!(draws.len(), 1);
        assert!(draws[0].min[0] > 5.0, "the surviving marker is id 2");
    }

    #[test]
    fn clear_markers_event_without_an_id_removes_every_marker() {
        let mut app = App::headless();
        show_marker(&app, 1, glam::Vec3::ZERO, glam::Vec3::ONE);
        show_marker(&app, 2, glam::Vec3::splat(10.0), glam::Vec3::splat(11.0));
        app.event_bus.publish(DebugEvent::ClearMarkers { id: None });

        EventProcessor::new().process_debug_events(&mut app);

        assert!(marker_draws(&mut app).is_empty());
    }
}
