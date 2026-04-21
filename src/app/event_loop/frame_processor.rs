//! Frame Processing
//!
//! Handles frame timing, game state updates, and lighting calculations.

use crate::App;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use winit::event_loop::{ActiveEventLoop, ControlFlow};

/// Frame counter for tracking frame numbers
static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Handles frame timing and game state updates
pub struct FrameProcessor;

impl FrameProcessor {
    /// Create a new frame processor
    pub fn new() -> Self {
        Self
    }

    /// Check if it's time for a new frame
    pub fn should_process_frame(&self, app: &App) -> bool {
        Instant::now() >= app.last_frame + app.frame_duration
    }

    /// Publish frame start event
    pub fn publish_frame_start(&self, app: &mut App, dt: f32) -> u64 {
        let frame_number = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
        app.event_bus
            .publish(moho_core::events::SystemEvent::FrameStart {
                frame_number,
                delta_time: dt,
            });
        frame_number
    }

    /// Update game state for the current frame
    pub fn update_game_state(&self, app: &mut App, dt: f32) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }

        // Update controller input from keyboard state
        app.update_controller_input();

        let (yaw_delta, pitch_delta) = app.input_system.sample_frame_input();
        {
            let ci = app.simulation.controller_input_mut();
            ci.yaw_delta = yaw_delta;
            ci.pitch_delta = pitch_delta;
        }

        // --- Physics KCC path (FPS only) ---
        let is_fps = app.simulation.camera_mode() == moho_core::controller::CameraMode::FirstPerson;
        let use_kcc = is_fps
            && app
                .physics_world
                .as_ref()
                .is_some_and(|pw| !pw.noclip && pw.character_body.is_some());

        if use_kcc {
            self.update_game_state_fps_kcc(app, dt);
        } else {
            // Flying camera / noclip / RTS
            let (view, proj, eye) = app.simulation.apply_input(dt);
            app.camera = (view, proj, eye);
        }

        // Step rigid bodies in both KCC and non-KCC paths (test spheres, etc.)
        if !use_kcc {
            self.step_physics_bodies(app, dt);
        }

        // Clear zoom delta after use
        app.simulation.controller_input.zoom_delta = 0.0;
    }

    fn update_game_state_fps_kcc(&self, app: &mut App, dt: f32) {
        const MOVE_SPEED: f32 = 4.0;
        const SPRINT_SPEED: f32 = 6.0;
        const JUMP_VELOCITY: f32 = 8.0;

        // Capture movement intent before zeroing it
        let forward_input = app.simulation.controller_input.forward;
        let right_input = app.simulation.controller_input.right;
        let sprint = app.simulation.controller_input.sprint;

        // Zero translational input so apply_input only updates yaw/pitch and clock
        {
            let ci = app.simulation.controller_input_mut();
            ci.forward = 0.0;
            ci.right = 0.0;
            ci.up = 0.0;
        }
        // apply_input updates yaw/pitch from deltas, advances the game clock, returns camera
        app.simulation.apply_input(dt);

        // Read the now-updated yaw/pitch for horizontal movement calculation
        let (yaw, pitch) = app.simulation.yaw_pitch();

        let speed = if sprint { SPRINT_SPEED } else { MOVE_SPEED };

        // Build horizontal movement in world space from yaw + input.
        // Right vector matches controller.rs convention: (-cos(yaw), 0, sin(yaw))
        let sy = yaw.sin();
        let cy = yaw.cos();
        let fwd_world = glam::Vec3::new(sy, 0.0, cy);
        let right_world = glam::Vec3::new(-cy, 0.0, sy);

        let horizontal =
            (fwd_world * forward_input + right_world * right_input).normalize_or_zero()
                * speed
                * dt;

        // Physics character movement + jump
        {
            let pw = app.physics_world.as_mut().unwrap();
            if app.jump_pressed && pw.is_grounded {
                pw.vertical_velocity = JUMP_VELOCITY;
            }
            let new_pos = pw.move_character(horizontal, dt);
            // Override simulation position with physics result
            app.simulation.set_position_yaw_pitch(new_pos, yaw, pitch);
        }

        // Rebuild camera from the updated simulation state
        app.camera =
            moho_core::controller::controller_to_camera(&app.simulation.player_controller);

        // Step dynamic rigid bodies and sync ECS transforms
        self.step_physics_bodies(app, dt);
    }

    /// Step dynamic rigid bodies and sync their positions into ECS components.
    /// Called from both the KCC path and the non-KCC path so test spheres move in all modes.
    fn step_physics_bodies(&self, app: &mut App, dt: f32) {
        let pw = match app.physics_world.as_mut() {
            Some(pw) => pw,
            None => return,
        };
        pw.step(dt);

        // Sync test sphere ECS transforms from physics
        let body_positions: Vec<(legion::Entity, glam::Vec3)> = app
            .test_physics_bodies
            .iter()
            .filter_map(|(handle, entity)| pw.body_position(*handle).map(|p| (*entity, p)))
            .collect();

        for (entity, pos) in body_positions {
            if let Some(mut entry) = app.world.entry(entity) {
                if let Ok(sphere) = entry.get_component_mut::<moho_core::actors::Sphere>() {
                    sphere.center = pos;
                } else if let Ok(cube) = entry.get_component_mut::<moho_core::actors::Cube>() {
                    cube.center = pos;
                }
            }
        }
    }

    /// Update light propagation system for the current frame
    pub fn update_light_system(&self, app: &mut App) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }

        if let Some(ref mut light_system) = app.light_system {
            // Update player position for priority calculation
            let player_pos = app.camera.2; // Camera eye position
            light_system.set_player_position(player_pos);

            // Process light updates within frame budget
            light_system.process_frame();

            // Emit mesh dirty events for affected chunks
            let dirty_count = light_system.emit_dirty_events();
            if dirty_count > 0 {
                log::trace!("Light system emitted {} mesh dirty events", dirty_count);
            }
        }
    }

    /// Update lighting based on celestial positions and time of day
    pub fn update_lighting(&self, app: &mut App) {
        if app.game_state != crate::game_state::GameState::Playing {
            return;
        }

        if let Some(ref mut wr) = app.window_renderer {
            let (sun_dir, moon_dir) = app.simulation.celestial_directions();
            let time = app.simulation.time_of_day();

            // Calculate sun intensity (0 when below horizon)
            let sun_intensity = if sun_dir.y > 0.0 { 1.0 } else { 0.0 };

            // Calculate moon intensity based on position and time
            let moon_base_intensity = self.calculate_moon_intensity(moon_dir, time);

            // Calculate ambient lighting based on time of day
            let (ambient_color, ambient_intensity) = self.calculate_ambient_lighting(time);

            let lighting = moho_renderer::LightingGpu {
                sun_direction: [sun_dir.x, sun_dir.y, sun_dir.z, sun_intensity],
                sun_color: [1.0, 0.95, 0.8, 0.0], // Warm sunlight
                moon_direction: [moon_dir.x, moon_dir.y, moon_dir.z, moon_base_intensity],
                moon_color: [0.7, 0.8, 0.9, 0.0], // Silver-blue moonlight
                ambient: [
                    ambient_color[0],
                    ambient_color[1],
                    ambient_color[2],
                    ambient_intensity,
                ],
                params: [time, app.debug_mode as f32, 0.0, 0.0],
            };
            wr.renderer.update_lighting(lighting);
        }
    }

    /// Calculate moon intensity based on position and time
    fn calculate_moon_intensity(&self, moon_dir: glam::Vec3, time: f32) -> f32 {
        if moon_dir.y <= 0.0 {
            return 0.0; // Moon below horizon
        }

        // Moon is stronger at night, weaker during day transitions
        if !(5.0..21.0).contains(&time) {
            // Deep night: full moon brightness
            0.4
        } else if (5.0..7.0).contains(&time) {
            // Dawn: moon fading
            let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
            0.4 * (1.0 - t) // 0.4 -> 0.0
        } else if (17.0..21.0).contains(&time) {
            // Dusk: moon rising
            let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
            0.4 * t // 0.0 -> 0.4
        } else {
            // Day: moon barely visible if at all
            0.0
        }
    }

    /// Calculate ambient lighting based on time of day
    /// Returns (color, intensity)
    fn calculate_ambient_lighting(&self, time: f32) -> ([f32; 3], f32) {
        // Night: 0-5, 21-24 (very low, blue-tinted)
        // Dawn: 5-7 (increasing, warm tint)
        // Day: 7-17 (full brightness, neutral)
        // Dusk: 17-21 (decreasing, warm tint)
        if (7.0..17.0).contains(&time) {
            // Day: full brightness, cool ambient
            ([0.4, 0.5, 0.6], 0.15)
        } else if (5.0..7.0).contains(&time) {
            // Dawn: increasing brightness, warm tint
            let t = (time - 5.0) / 2.0; // 0-1 over 2 hours
            let intensity = 0.05 + t * 0.10; // 0.05 -> 0.15
            ([0.5, 0.45, 0.4], intensity)
        } else if (17.0..21.0).contains(&time) {
            // Dusk: decreasing brightness, warm tint
            let t = (time - 17.0) / 4.0; // 0-1 over 4 hours
            let intensity = 0.15 - t * 0.10; // 0.15 -> 0.05
            ([0.5, 0.4, 0.35], intensity)
        } else {
            // Night: very low brightness, blue tint
            ([0.3, 0.35, 0.5], 0.05)
        }
    }

    /// Request window redraw if renderer is available
    pub fn request_redraw(&self, app: &App) {
        if let Some(ref wr) = app.window_renderer {
            wr.window.request_redraw();
        }
    }

    /// Publish frame end event and process deferred events
    pub fn publish_frame_end(&self, app: &mut App, frame_number: u64) {
        app.event_bus
            .publish(moho_core::events::SystemEvent::FrameEnd { frame_number });
        app.event_bus.process_deferred();
    }

    /// Update control flow for next frame
    pub fn update_control_flow(&self, app: &App, event_loop: &ActiveEventLoop) {
        let next = app.last_frame + app.frame_duration;
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
    }

    /// Process a complete frame update
    pub fn process_frame(&self, app: &mut App, event_loop: &ActiveEventLoop) {
        let dt = app.frame_duration.as_secs_f32();
        app.last_frame += app.frame_duration;

        // Frame lifecycle
        let frame_number = self.publish_frame_start(app, dt);
        self.update_game_state(app, dt);
        self.update_light_system(app); // Process light propagation after game state
        self.update_lighting(app);
        self.update_hud_data(app);
        self.request_redraw(app);
        self.publish_frame_end(app, frame_number);
        self.update_control_flow(app, event_loop);
    }

    /// Push current world state into the overlay HUD data.
    fn update_hud_data(&self, app: &mut App) {
        let ui_adapter = match &app.ui_adapter {
            Some(a) => a,
            None => return,
        };

        let pos = app.simulation.position();
        let chunk_size = 16i32;
        let chunk_pos = [
            (pos.x.floor() as i32).div_euclid(chunk_size),
            (pos.y.floor() as i32).div_euclid(chunk_size),
            (pos.z.floor() as i32).div_euclid(chunk_size),
        ];

        let mode = app.simulation.camera_mode();
        let is_fps = mode == moho_core::controller::CameraMode::FirstPerson;

        let (yaw, _pitch) = app.simulation.yaw_pitch();

        let data = moho_ui::overlays::HudData {
            player_position: [pos.x, pos.y, pos.z],
            chunk_position: chunk_pos,
            camera_mode: format!("{mode:?}"),
            is_fps_mode: is_fps,
            frame_time_secs: app.frame_duration.as_secs_f32(),
            time_of_day: app.simulation.time_of_day(),
            material_under_crosshair: None,
            camera_yaw: yaw,
            player_health: 1.0,
            player_stamina: 1.0,
        };

        if let Ok(mut adapter) = ui_adapter.lock() {
            adapter.update_hud_data(data);
        }
    }
}

impl Default for FrameProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_processor_creation() {
        let processor = FrameProcessor::new();
        assert_eq!(std::mem::size_of_val(&processor), 0); // Zero-sized type
    }

    #[test]
    fn test_frame_processor_default() {
        let _processor = FrameProcessor;
        // Just verify it compiles and constructs
    }

    #[test]
    fn test_moon_intensity_below_horizon() {
        let processor = FrameProcessor::new();
        let moon_dir = glam::Vec3::new(0.0, -1.0, 0.0); // Below horizon
        let intensity = processor.calculate_moon_intensity(moon_dir, 12.0);
        assert_eq!(intensity, 0.0);
    }

    #[test]
    fn test_moon_intensity_deep_night() {
        let processor = FrameProcessor::new();
        let moon_dir = glam::Vec3::new(0.0, 1.0, 0.0); // Above horizon
        let intensity = processor.calculate_moon_intensity(moon_dir, 23.0); // 11 PM
        assert_eq!(intensity, 0.4);
    }

    #[test]
    fn test_moon_intensity_dawn() {
        let processor = FrameProcessor::new();
        let moon_dir = glam::Vec3::new(0.0, 1.0, 0.0); // Above horizon
        let intensity = processor.calculate_moon_intensity(moon_dir, 5.0); // 5 AM
        assert_eq!(intensity, 0.4); // Full brightness at start of dawn

        let intensity = processor.calculate_moon_intensity(moon_dir, 7.0); // 7 AM
        assert_eq!(intensity, 0.0); // Faded by end of dawn
    }

    #[test]
    fn test_moon_intensity_day() {
        let processor = FrameProcessor::new();
        let moon_dir = glam::Vec3::new(0.0, 1.0, 0.0); // Above horizon
        let intensity = processor.calculate_moon_intensity(moon_dir, 12.0); // Noon
        assert_eq!(intensity, 0.0);
    }

    #[test]
    fn test_ambient_lighting_day() {
        let processor = FrameProcessor::new();
        let (color, intensity) = processor.calculate_ambient_lighting(12.0); // Noon
        assert_eq!(color, [0.4, 0.5, 0.6]);
        assert_eq!(intensity, 0.15);
    }

    #[test]
    fn test_ambient_lighting_night() {
        let processor = FrameProcessor::new();
        let (color, intensity) = processor.calculate_ambient_lighting(23.0); // 11 PM
        assert_eq!(color, [0.3, 0.35, 0.5]);
        assert_eq!(intensity, 0.05);
    }

    #[test]
    fn test_ambient_lighting_dawn_start() {
        let processor = FrameProcessor::new();
        let (color, intensity) = processor.calculate_ambient_lighting(5.0); // 5 AM
        assert_eq!(color, [0.5, 0.45, 0.4]);
        assert!((intensity - 0.05).abs() < 0.001); // Start of dawn
    }

    #[test]
    fn test_ambient_lighting_dawn_end() {
        let processor = FrameProcessor::new();
        let (color, intensity) = processor.calculate_ambient_lighting(6.99); // Just before 7 AM - end of dawn
        assert_eq!(color, [0.5, 0.45, 0.4]);
        assert!((intensity - 0.149).abs() < 0.01); // Very close to full brightness
    }
}
