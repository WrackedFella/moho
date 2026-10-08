//! Event Processing
//!
//! Handles processing of various event types from the event bus:
//! - UI events (menu requests, settings, exit)
//! - Audio events (sounds, music)
//! - Graphics events (time of day, lighting)
//! - Input events (mouse wheel, keyboard)

use crate::App;
use crate::input_event::InputEvent;
use moho_core::events::{GraphicsEvent, UiEvent, WorldEvent};
use moho_voxel::VoxelChunk;
use winit::event_loop::ActiveEventLoop;

// ── Spawn / interaction constants ──────────────────────────────────────
const MOUSE_WHEEL_ZOOM_FACTOR: f32 = 0.5;
const SPAWN_RAYCAST_MAX_DISTANCE: f32 = 100.0;
const SPAWN_NORMAL_OFFSET: f32 = 0.5;
const SPAWN_FALLBACK_DISTANCE: f32 = 5.0;
/// Placeholder material ID for torch blocks.
const TORCH_MATERIAL_ID: u32 = 3; // TODO: look up from MaterialRegistry
/// Maximum reach for the player's direct mining action — shorter than
/// `SPAWN_RAYCAST_MAX_DISTANCE`, which is a debug-console placement aid, not
/// a gameplay reach limit.
const MINE_MAX_DISTANCE: f32 = 8.0;
const DEFAULT_POINT_LIGHT_INTENSITY: f32 = 5.0;
const DEFAULT_POINT_LIGHT_RANGE: f32 = 20.0;

/// Handles processing of all event types
pub struct EventProcessor;

impl EventProcessor {
    /// Create a new event processor
    pub fn new() -> Self {
        Self
    }

    /// Process all pending UI events from the event bus
    pub fn process_ui_events(&self, app: &mut App, event_loop: &ActiveEventLoop) {
        while let Ok(event) = app.ui_event_rx.try_recv() {
            self.handle_ui_event(app, event_loop, event);
        }
    }

    /// Process a single UI event
    fn handle_ui_event(&self, app: &mut App, event_loop: &ActiveEventLoop, event: UiEvent) {
        match event {
            UiEvent::LoadSceneRequested { path } => {
                tracing::info!(path = %path.display(), "UI requested load scene");
                if let Err(e) = app.load_scene(&path) {
                    tracing::error!(path = %path.display(), error = %e, "Failed to load scene");
                }
            }
            UiEvent::NewWorldRequested { name, seed, size } => {
                tracing::info!(name = %name, seed = ?seed, size, "UI requested new world");
                let spec = moho_game::scene_builders::WorldSpec {
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
                // Auto-save before exit
                if let Err(e) = app.auto_save_on_shutdown() {
                    tracing::warn!(error = %e, "Failed to auto-save on exit");
                }
                event_loop.exit();
            }
            UiEvent::MenuShown { name } => {
                tracing::info!(name = %name, "UI requested show menu");
                if let Some(ui_adapter) = &app.ui_adapter
                    && let Ok(mut adapter) = ui_adapter.lock()
                {
                    adapter.show_menu(&name);
                }
            }
            UiEvent::MenuHidden { name } => {
                tracing::info!(name = %name, "Menu hidden");
                // Handle console close event
                if name == "console" {
                    app.exit_console();
                }
            }
            UiEvent::OverlayToggled { name, visible } => {
                tracing::info!(name = %name, visible, "Overlay toggled");
                if visible && let Some(ref wr) = app.window_renderer {
                    wr.window.set_cursor_visible(true);
                }
            }
            UiEvent::SettingsSaved => {
                tracing::info!("Settings saved");
                // Settings are already saved by the UI adapter
                // Here we could reload/apply them if needed
            }
            UiEvent::WindowSettingsChanged {
                mode,
                width,
                height,
            } => {
                tracing::info!(mode = ?mode, width, height, "Window settings changed");
                if let Some(ref wr) = app.window_renderer {
                    use moho_core::events::WindowMode;
                    use winit::dpi::PhysicalSize;
                    use winit::window::Fullscreen;

                    match mode {
                        WindowMode::Fullscreen | WindowMode::Borderless => {
                            wr.window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                        }
                        WindowMode::Windowed => {
                            wr.window.set_fullscreen(None);
                            let _ = wr
                                .window
                                .request_inner_size(PhysicalSize::new(width, height));
                        }
                    }
                }
            }
        }
    }

    /// Process all pending audio events from the event bus
    pub fn process_audio_events(&self, app: &mut App) {
        while let Ok(event) = app.audio_event_rx.try_recv() {
            app.handle_audio_event(event);
        }
    }

    /// Process all pending graphics events from the event bus
    pub fn process_graphics_events(&self, app: &mut App) {
        while let Ok(event) = app.graphics_event_rx.try_recv() {
            match event {
                GraphicsEvent::TimeOfDayChanged { time, .. } => {
                    // Set the game clock time directly (time is in hours 0-24)
                    app.simulation.set_time_of_day(time);
                    tracing::info!(
                        time,
                        time_string = %app.simulation.game_clock().time_string(),
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
                        moho_game::controller::CameraMode::FirstPerson => {
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
                        moho_game::controller::CameraMode::Isometric => {
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
            || app.simulation.camera_mode() != moho_game::controller::CameraMode::FirstPerson
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

        let chunk_pos = moho_voxel::VoxelGrid::get_chunk_pos(outcome.block_pos, grid.chunk_size());

        app.event_bus.publish(WorldEvent::BlockRemoved {
            position: outcome.block_pos,
            old_material_id: outcome.old_material_id,
            reason: moho_core::events::BlockChangeReason::Player,
        });
        app.event_bus.publish(WorldEvent::ChunkMeshDirty {
            chunk_pos,
            terrain_dirty: true,
            structure_dirty: true,
        });

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

    /// Process all pending world events (chunk updates, etc.)
    pub fn process_world_events(&self, app: &mut App) {
        while let Ok(event) = app.world_event_rx.try_recv() {
            if let WorldEvent::ChunkMeshDirty { chunk_pos, .. } = event {
                // Regenerate chunk mesh
                // We need to scope the borrow of light_system so we can access world later
                let player_chunk = lod_player_chunk(app.simulation.position());
                let lod = lod_for_chunk(chunk_pos, player_chunk);
                let new_chunk = if let Some(light_system) = &app.light_system {
                    let grid = light_system.grid();
                    Some(VoxelChunk::from_grid_lod(grid, chunk_pos, lod))
                } else {
                    None
                };

                if let Some(chunk) = new_chunk {
                    if crate::app::world_geometry::insert_chunk(app, chunk).is_some() {
                        tracing::trace!(chunk = ?chunk_pos, "Updated mesh for chunk");
                    } else {
                        tracing::trace!(chunk = ?chunk_pos, "Created new mesh for chunk");
                    }
                }
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
            } => {
                // If position is None (from console), raycast to find it
                // We need the camera position and forward vector
                let (view_matrix, _, _) = app.camera;
                let camera_pos = view_matrix.inverse().col(3).truncate();
                let forward = -view_matrix.inverse().col(2).truncate().normalize();

                // Raycast
                let grid_opt = app
                    .light_system
                    .as_mut()
                    .map(moho_voxel::LightSystem::grid_mut);

                if let Some(grid) = grid_opt {
                    // Use raycast utility
                    // Max distance 100 units
                    let hit = moho_game::raycast::raycast(
                        grid,
                        camera_pos,
                        forward,
                        SPAWN_RAYCAST_MAX_DISTANCE,
                    );

                    let spawn_pos = if let Some(hit) = hit {
                        // Spawn at hit position + normal * offset
                        hit.position
                            + glam::Vec3::new(
                                hit.normal.x as f32,
                                hit.normal.y as f32,
                                hit.normal.z as f32,
                            ) * SPAWN_NORMAL_OFFSET
                    } else {
                        // Spawn in front of camera if no hit
                        camera_pos + forward * SPAWN_FALLBACK_DISTANCE
                    };

                    tracing::info!(entity_type = %entity_type, pos = ?spawn_pos, "Spawning entity");

                    match entity_type.to_lowercase().as_str() {
                        "torch" => {
                            // Set block to torch
                            // Assuming torch ID is 3 (need to verify or look up)
                            // For now, let's use a hardcoded ID or look it up if possible
                            // MaterialRegistry is in VoxelGrid but not easily accessible by name here
                            // Let's assume 3 for now as a placeholder
                            let torch_id = TORCH_MATERIAL_ID;
                            let block_pos = glam::IVec3::new(
                                spawn_pos.x.floor() as i32,
                                spawn_pos.y.floor() as i32,
                                spawn_pos.z.floor() as i32,
                            );

                            // Use LightSystem to handle block placement if available (it handles events)
                            // Or modify grid directly and notify

                            // Better approach: Publish BlockPlaced event
                            // But we are in event processor, we can modify app state directly

                            // If we have light system, use it to handle events?
                            // Actually, we should modify the grid and let the system react

                            // Let's use the event bus to trigger block placement properly
                            // This ensures all systems (light, mesh) get notified
                            app.event_bus.publish(WorldEvent::BlockPlaced {
                                position: block_pos,
                                material_id: torch_id,
                                reason: moho_core::events::BlockChangeReason::Player,
                            });
                        }
                        "light" => {
                            // Spawn dynamic light
                            // We need to access the renderer backend to add a light
                            if let Some(wr) = &mut app.window_renderer {
                                // Parse color from args if present
                                let color = if let Some(arg_str) = &args {
                                    // Simple parsing: "r g b"
                                    let parts: Vec<&str> = arg_str.split_whitespace().collect();
                                    if parts.len() >= 3 {
                                        glam::Vec3::new(
                                            parts[0].parse().unwrap_or(1.0),
                                            parts[1].parse().unwrap_or(1.0),
                                            parts[2].parse().unwrap_or(1.0),
                                        )
                                    } else {
                                        glam::Vec3::ONE // White default
                                    }
                                } else {
                                    glam::Vec3::ONE // White default
                                };

                                wr.renderer.add_point_light(
                                    spawn_pos,
                                    color,
                                    DEFAULT_POINT_LIGHT_INTENSITY,
                                    DEFAULT_POINT_LIGHT_RANGE,
                                );
                                tracing::info!(pos = ?spawn_pos, "Added point light");

                                // Spawn a small gizmo sphere so the light origin is
                                // visible in world space. Emissive material bypasses
                                // lighting so the gizmo glows at the light's own colour.
                                let gizmo = moho_game::actors::Sphere::new(
                                    spawn_pos,
                                    0.15,
                                    moho_core::materials::MaterialType::Emissive {
                                        color,
                                        intensity: 1.5,
                                    },
                                );
                                app.entities.actors.spawn_sphere(gizmo);
                            }
                        }
                        "cube" => {
                            // Spawn cube actor
                            use moho_core::materials::MaterialType;
                            use moho_game::actors::Cube;

                            let cube = Cube::new(
                                spawn_pos,
                                1.0,
                                1.0,
                                1.0,
                                MaterialType::Lambertian {
                                    albedo: glam::Vec3::new(0.8, 0.2, 0.2),
                                },
                            );
                            let entity = app.entities.actors.spawn_cube(cube);
                            if let Some(pw) = app.physics.world.as_mut() {
                                let handle = pw.add_dynamic_cuboid(spawn_pos, 0.5, 0.5, 0.5);
                                app.physics.test_bodies.push((handle, entity));
                            }
                            tracing::info!(pos = ?spawn_pos, "Spawned cube");
                        }
                        "sphere" => {
                            // Spawn sphere actor
                            use moho_core::materials::MaterialType;
                            use moho_game::actors::Sphere;

                            let sphere = Sphere::new(
                                spawn_pos,
                                0.5,
                                MaterialType::Metal {
                                    albedo: glam::Vec3::new(0.8, 0.8, 0.8),
                                    fuzz: 0.1,
                                },
                            );
                            let entity = app.entities.actors.spawn_sphere(sphere);
                            if let Some(pw) = app.physics.world.as_mut() {
                                let handle = pw.add_dynamic_sphere(spawn_pos, 0.5);
                                app.physics.test_bodies.push((handle, entity));
                            }
                            tracing::info!(pos = ?spawn_pos, "Spawned sphere");
                        }
                        _ => {
                            tracing::warn!(entity_type = %entity_type, "Unknown entity type");
                        }
                    }
                }
            }
            DebugEvent::ToggleGodMode { enabled } => {
                tracing::info!(enabled, "God mode toggled");
                // TODO: Implement god mode logic
            }
            DebugEvent::ToggleCollision { enabled } => {
                tracing::info!(enabled, "Collision toggled");
                // enabled=false means noclip ON (collision disabled)
                if let Some(ref mut pw) = app.physics.world {
                    pw.noclip = !enabled;
                    tracing::info!(noclip = pw.noclip, "Noclip toggled");
                }
            }
            DebugEvent::SetShadowQuality { quality } => {
                tracing::info!(quality, "Setting shadow quality");
                if let Some(wr) = &mut app.window_renderer {
                    wr.renderer.set_shadow_quality(quality as u8);

                    // Update prefs
                    app.prefs.set_shadow_quality(quality);
                    let _ = app.save_prefs();
                }
            }
            DebugEvent::SetSsaoQuality { quality } => {
                tracing::info!(quality, "Setting SSAO quality");
                if let Some(wr) = &mut app.window_renderer {
                    wr.renderer.set_ssao_quality(quality as u8);

                    // Update prefs
                    app.prefs.set_ssao_quality(quality);
                    let _ = app.save_prefs();
                }
            }
            _ => {}
        }
    }
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
    fn chunk_mesh_dirty_event_stores_meshed_chunk_with_collider() {
        let mut app = App::headless();
        let chunk_pos = glam::IVec3::new(0, 4, 0);
        app.light_system
            .as_mut()
            .expect("App starts with a light system")
            .grid_mut()
            .mutator()
            .place(moho_voxel::BlockPos::new(3, 70, 3), 1, None);
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
    fn lod_for_chunk_tiers_by_chebyshev_xz_distance() {
        let cases = [
            (glam::IVec3::new(0, 0, 0), 0),
            (glam::IVec3::new(3, 0, 0), 0),
            (glam::IVec3::new(-3, 0, 0), 0),
            (glam::IVec3::new(3, 0, 3), 0),
            (glam::IVec3::new(0, 10, 0), 0),
            (glam::IVec3::new(4, 0, 0), 1),
            (glam::IVec3::new(0, 0, 4), 1),
            (glam::IVec3::new(3, 0, 4), 1),
            (glam::IVec3::new(16, 0, 0), 1),
        ];

        for (chunk, expected) in cases {
            assert_eq!(
                lod_for_chunk(chunk, glam::IVec3::ZERO),
                expected,
                "row {chunk:?}"
            );
        }
    }
}
