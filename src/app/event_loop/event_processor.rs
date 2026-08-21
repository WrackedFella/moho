//! Event Processing
//!
//! Handles processing of various event types from the event bus:
//! - UI events (menu requests, settings, exit)
//! - Audio events (sounds, music)
//! - Graphics events (time of day, lighting)
//! - Input events (mouse wheel, keyboard)

use crate::App;
use crate::input_event::InputEvent;
use legion::IntoQuery;
use moho_core::events::{GraphicsEvent, UiEvent, WorldEvent};
use moho_core::voxel::VoxelChunk;
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
                log::info!("UI requested load scene: {:?}", path);
                if let Err(e) = app.load_scene(&path) {
                    log::error!("Failed to load scene from {:?}: {}", path, e);
                }
            }
            UiEvent::NewWorldRequested { name, seed, size } => {
                log::info!(
                    "UI requested new world: {} (seed: {:?}, size: {})",
                    name,
                    seed,
                    size
                );
                let spec = moho_game::scene_builders::WorldSpec {
                    name,
                    seed,
                    size_xz: size,
                    day_length_seconds: 600.0,
                    night_length_seconds: 420.0,
                    initial_time_of_day: 6.0,
                };
                if let Err(e) = app.generate_new_world(spec) {
                    log::error!("Failed to generate new world: {}", e);
                }
            }
            UiEvent::ExitRequested => {
                log::info!("UI requested exit");
                // Auto-save before exit
                if let Err(e) = app.auto_save_on_shutdown() {
                    log::warn!("Failed to auto-save on exit: {}", e);
                }
                event_loop.exit();
            }
            UiEvent::MenuShown { name } => {
                log::info!("UI requested show menu: {}", name);
                if let Some(ui_adapter) = &app.ui_adapter
                    && let Ok(mut adapter) = ui_adapter.lock()
                {
                    adapter.show_menu(&name);
                }
            }
            UiEvent::MenuHidden { name } => {
                log::info!("Menu hidden: {}", name);
                // Handle console close event
                if name == "console" {
                    app.exit_console();
                }
            }
            UiEvent::OverlayToggled { name, visible } => {
                log::info!("Overlay {} toggled: {}", name, visible);
                if visible && let Some(ref wr) = app.window_renderer {
                    wr.window.set_cursor_visible(true);
                }
            }
            UiEvent::SettingsSaved => {
                log::info!("Settings saved");
                // Settings are already saved by the UI adapter
                // Here we could reload/apply them if needed
            }
            UiEvent::WindowSettingsChanged {
                mode,
                width,
                height,
            } => {
                log::info!("Window settings changed: {:?} {}x{}", mode, width, height);
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
                    log::info!(
                        "Time set to {:.2} ({})",
                        time,
                        app.simulation.game_clock().time_string()
                    );
                }
                GraphicsEvent::DebugViewChanged { mode } => {
                    app.debug_mode = mode;
                    log::info!("Debug view mode set to {}", mode);
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

        let Some(light_system) = app.light_system.as_mut() else {
            return;
        };
        let grid = light_system.grid_mut();

        let Some(outcome) = app.pawn.mine(grid, camera_pos, forward, MINE_MAX_DISTANCE) else {
            log::debug!(
                "Mine attempt found nothing solid within {} units",
                MINE_MAX_DISTANCE
            );
            return;
        };

        let chunk_pos =
            moho_core::voxel::VoxelGrid::get_chunk_pos(outcome.block_pos, grid.chunk_size());

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

        // Temporary until 3.1b puts the inventory on screen.
        match outcome.yield_ {
            Some(y) => log::info!(
                "Mined {:?} at {:.1}m → resource {} (inventory: {})",
                outcome.block_pos,
                outcome.distance,
                y.resource_id,
                app.pawn.inventory.count(y.resource_id)
            ),
            None => log::info!(
                "Mined {:?} at {:.1}m (no resource)",
                outcome.block_pos,
                outcome.distance
            ),
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
                    // Remove old collider for this chunk and re-add updated one
                    app.physics
                        .update_chunk_collider(chunk_pos, chunk.vertices(), chunk.indices());

                    // Update or insert into ECS world.
                    debug_assert_chunk_entity_unique(&app.world, chunk_pos);
                    let mut query = <(legion::Entity, &VoxelChunk)>::query();
                    let entity = query
                        .iter(&app.world)
                        .find(|(_, c)| c.chunk_pos() == chunk_pos)
                        .map(|(e, _)| *e);

                    if let Some(e) = entity {
                        if let Some(mut entry) = app.world.entry(e) {
                            entry.add_component(chunk);
                            log::trace!("Updated mesh for chunk {:?}", chunk_pos);
                        }
                    } else {
                        // New chunk
                        app.world.push((chunk,));
                        log::trace!("Created new mesh for chunk {:?}", chunk_pos);
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
                let grid_opt = app.light_system.as_mut().map(|ls| ls.grid_mut());

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

                    log::info!("Spawning {} at {:?}", entity_type, spawn_pos);

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

                                let light_id = wr.renderer.add_point_light(
                                    spawn_pos,
                                    color,
                                    DEFAULT_POINT_LIGHT_INTENSITY,
                                    DEFAULT_POINT_LIGHT_RANGE,
                                );
                                log::info!("Added point light at {:?}", spawn_pos);

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
                                let entity = app.world.push((gizmo,));
                                app.light_gizmos.insert(light_id, entity);
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
                            let entity = app.world.push((cube,));
                            if let Some(pw) = app.physics.world.as_mut() {
                                let handle = pw.add_dynamic_cuboid(spawn_pos, 0.5, 0.5, 0.5);
                                app.physics.test_bodies.push((handle, entity));
                            }
                            log::info!("Spawned cube at {:?}", spawn_pos);
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
                            let entity = app.world.push((sphere,));
                            if let Some(pw) = app.physics.world.as_mut() {
                                let handle = pw.add_dynamic_sphere(spawn_pos, 0.5);
                                app.physics.test_bodies.push((handle, entity));
                            }
                            log::info!("Spawned sphere at {:?}", spawn_pos);
                        }
                        _ => {
                            log::warn!("Unknown entity type: {}", entity_type);
                        }
                    }
                }
            }
            DebugEvent::ToggleGodMode { enabled } => {
                log::info!("God mode toggled: {}", enabled);
                // TODO: Implement god mode logic
            }
            DebugEvent::ToggleCollision { enabled } => {
                log::info!("Collision toggled: {}", enabled);
                // enabled=false means noclip ON (collision disabled)
                if let Some(ref mut pw) = app.physics.world {
                    pw.noclip = !enabled;
                    log::info!("Noclip {}", if pw.noclip { "enabled" } else { "disabled" });
                }
            }
            DebugEvent::SetShadowQuality { quality } => {
                log::info!("Setting shadow quality to: {}", quality);
                if let Some(wr) = &mut app.window_renderer {
                    wr.renderer.set_shadow_quality(quality as u8);

                    // Update prefs
                    app.prefs.set_shadow_quality(quality);
                    let _ = app.prefs.save();
                }
            }
            DebugEvent::SetSsaoQuality { quality } => {
                log::info!("Setting SSAO quality to: {}", quality);
                if let Some(wr) = &mut app.window_renderer {
                    wr.renderer.set_ssao_quality(quality as u8);

                    // Update prefs
                    app.prefs.set_ssao_quality(quality);
                    let _ = app.prefs.save();
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
    if dist < 4 { 0 } else { 1 }
}

/// A `chunk_pos` should map to at most one `VoxelChunk` entity — if streaming
/// or LOD transitions ever left a duplicate, `process_world_events`'s `find`
/// would silently update one while a stale mesh keeps rendering alongside it.
/// Debug-only: `ChunkMeshDirty` fires continuously during streaming, so this
/// scans the chunk archetype on every call — `debug_assert!`'s condition is
/// never evaluated in a release build, so this costs nothing there.
fn debug_assert_chunk_entity_unique(world: &legion::World, chunk_pos: glam::IVec3) {
    debug_assert!(
        {
            let mut query = <&VoxelChunk>::query();
            query
                .iter(world)
                .filter(|c| c.chunk_pos() == chunk_pos)
                .count()
                <= 1
        },
        "chunk {:?} has more than one VoxelChunk entity — a stale mesh may be rendering alongside the fresh one",
        chunk_pos
    );
}

impl Default for EventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lod_player_chunk_origin() {
        let chunk = lod_player_chunk(glam::Vec3::ZERO);
        assert_eq!(chunk, glam::IVec3::ZERO);
    }

    #[test]
    fn test_lod_player_chunk_positive() {
        // Block 16 in world space → chunk 1
        let chunk = lod_player_chunk(glam::Vec3::new(16.0, 0.0, 16.0));
        assert_eq!(chunk.x, 1);
        assert_eq!(chunk.z, 1);
    }

    #[test]
    fn test_lod_player_chunk_negative() {
        // Negative coords use div_euclid so -1 → chunk -1, not 0
        let chunk = lod_player_chunk(glam::Vec3::new(-1.0, 0.0, -1.0));
        assert_eq!(chunk.x, -1);
        assert_eq!(chunk.z, -1);
    }

    #[test]
    fn test_lod_for_chunk_same_position() {
        let player = glam::IVec3::ZERO;
        assert_eq!(lod_for_chunk(glam::IVec3::ZERO, player), 0);
    }

    #[test]
    fn test_lod_for_chunk_within_lod0() {
        let player = glam::IVec3::ZERO;
        // 3 chunks away in X → still LOD 0
        assert_eq!(lod_for_chunk(glam::IVec3::new(3, 0, 0), player), 0);
        assert_eq!(lod_for_chunk(glam::IVec3::new(-3, 0, 0), player), 0);
        assert_eq!(lod_for_chunk(glam::IVec3::new(3, 0, 3), player), 0);
    }

    #[test]
    fn test_lod_for_chunk_boundary() {
        let player = glam::IVec3::ZERO;
        // Exactly 4 chunks away → LOD 1
        assert_eq!(lod_for_chunk(glam::IVec3::new(4, 0, 0), player), 1);
        assert_eq!(lod_for_chunk(glam::IVec3::new(0, 0, 4), player), 1);
        // Diagonal: Chebyshev distance is max(3, 4) = 4 → LOD 1
        assert_eq!(lod_for_chunk(glam::IVec3::new(3, 0, 4), player), 1);
    }

    #[test]
    fn test_lod_for_chunk_far() {
        let player = glam::IVec3::ZERO;
        assert_eq!(lod_for_chunk(glam::IVec3::new(16, 0, 0), player), 1);
    }

    #[test]
    fn test_lod_for_chunk_ignores_y() {
        let player = glam::IVec3::ZERO;
        // A chunk directly above (only Y differs) counts as distance 0 in XZ
        assert_eq!(lod_for_chunk(glam::IVec3::new(0, 10, 0), player), 0);
    }
}
