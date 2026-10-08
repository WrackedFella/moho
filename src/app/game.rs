//! The strategy game as a [`moho_app::Game`].

use crate::app::event_loop::{EventProcessor, FrameProcessor, GenerationProcessor};
use crate::game_state::GameState;
use crate::{App, RenderRequest};
use moho_game::controller::{CameraMode, ControllerInput};
use moho_input::bindings::Binding;
use moho_ui::actions::StrategyAction;
use winit::event::{DeviceEvent, ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// One tick's worth of player input.
#[derive(Clone, Copy, Debug, Default)]
pub struct StrategyCommand {
    pub input: ControllerInput,
    pub jump: bool,
}

impl App {
    /// Sums the held bindings into the movement and look input for one tick.
    fn sample_command(&mut self) -> StrategyCommand {
        // An action is active when any of its bindings is held
        let is_active = |action: StrategyAction| -> bool {
            self.bindings
                .get(action)
                .iter()
                .any(|Binding::Key(key)| self.input.active_keys.contains(key))
        };
        let axis = |positive: StrategyAction, negative: StrategyAction| -> f32 {
            f32::from(u8::from(is_active(positive))) - f32::from(u8::from(is_active(negative)))
        };

        let forward = axis(StrategyAction::MoveForward, StrategyAction::MoveBack);
        // A is left, so negative
        let right = axis(StrategyAction::MoveRight, StrategyAction::MoveLeft);
        // No up/down in isometric mode
        let up = if self.simulation.camera_mode() == CameraMode::FirstPerson {
            axis(StrategyAction::Ascend, StrategyAction::Descend)
        } else {
            0.0
        };
        let sprint = is_active(StrategyAction::Sprint);
        let jump = is_active(StrategyAction::Jump);

        let (yaw_delta, pitch_delta) = self.input.system.sample_frame_input();

        StrategyCommand {
            input: ControllerInput {
                forward,
                right,
                up,
                yaw_delta,
                pitch_delta,
                sprint,
                // Zoom is handled separately via mouse wheel input
                zoom_delta: 0.0,
            },
            jump,
        }
    }

    fn apply_command(&mut self, command: &StrategyCommand) {
        *self.simulation.controller_input_mut() = command.input;
        self.physics.jump_pressed = command.jump;
    }

    fn apply_pending_render(&mut self, renderer: &mut dyn moho_renderer::RendererBackend) {
        for request in self.pending_render.drain(..) {
            match request {
                RenderRequest::AddPointLight {
                    position,
                    color,
                    intensity,
                    range,
                } => {
                    renderer.add_point_light(position, color, intensity, range);
                }
                RenderRequest::ShadowQuality(quality) => renderer.set_shadow_quality(quality),
                RenderRequest::SsaoQuality(quality) => renderer.set_ssao_quality(quality),
            }
        }
    }

    fn drain_audio_events(&mut self, mut audio: Option<&mut moho_audio::AudioSystem>) {
        while let Ok(event) = self.audio_event_rx.try_recv() {
            if let Some(audio) = audio.as_mut()
                && let Err(e) = audio.handle_event(event)
            {
                // Don't spam errors for missing audio files during development
                tracing::debug!(error = %e, "Audio event failed");
            }
        }
    }

    fn render_scene(&mut self, renderer: &mut dyn moho_renderer::RendererBackend) {
        if let Err(e) = self.scene.render(
            renderer,
            self.entities.actors.spheres(),
            self.entities.actors.cubes(),
            self.mesh_handle,
            self.cube_mesh_handle,
            self.camera,
        ) {
            tracing::warn!(error = %e, "Skipped frame");
        }
    }

    fn initial_render(&mut self, ctx: &mut moho_app::InitContext<'_>) {
        tracing::info!("Initial render");
        self.render_scene(&mut *ctx.renderer);
        ctx.window.request_redraw();
    }

    /// Autosaves with the renderer's current lights, logging a failure.
    fn autosave(&mut self, renderer: Option<&mut (dyn moho_renderer::RendererBackend + 'static)>) {
        let lights = renderer.map_or_else(Vec::new, |r| r.all_lights_as_descs());
        if let Err(e) = self.auto_save_on_shutdown(&lights) {
            tracing::warn!(error = %e, "Failed to auto-save on exit");
        }
    }

    fn handle_window_event(&mut self, ctx: &mut moho_app::EventContext<'_>, event: WindowEvent) {
        // Handle Tab key BEFORE dispatcher to prevent UI from consuming it
        if let WindowEvent::KeyboardInput {
            event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(KeyCode::Tab),
                    state: ElementState::Pressed,
                    ..
                },
            ..
        } = &event
            && self.game_state == GameState::Playing
        {
            self.toggle_camera_mode();
            return; // Don't dispatch Tab further
        }

        // Dispatch the event to registered subscribers (UI first). If consumed,
        // skip further application-level handling.
        if self.dispatcher.dispatch(&event) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.autosave(ctx.renderer());
                ctx.request_exit();
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => self.handle_keyboard_input(&key_event),
            _ => {}
        }
    }

    fn toggle_camera_mode(&mut self) {
        match self.simulation.camera_mode() {
            CameraMode::FirstPerson => {
                // Switch to RTS first so look_at runs in Isometric mode,
                // setting rts_look_target without touching FPS yaw/pitch.
                self.simulation.set_camera_mode(CameraMode::Isometric);
                self.simulation.look_at(self.simulation.position());
            }
            CameraMode::Isometric => {
                self.simulation.set_camera_mode(CameraMode::FirstPerson);
            }
        }
        tracing::info!(
            mode = ?self.simulation.camera_mode(),
            "Switched to camera mode"
        );
    }
}

impl moho_app::Game for App {
    type Command = StrategyCommand;

    fn command(&mut self) -> StrategyCommand {
        if self.game_state != GameState::Playing {
            return StrategyCommand::default();
        }
        self.sample_command()
    }

    fn tick(&mut self, ctx: &mut moho_app::TickContext, command: &StrategyCommand) {
        let dt = ctx.tick_length.as_secs_f32();
        let frame_processor = FrameProcessor::new();
        let event_processor = EventProcessor::new();

        let frame_number = frame_processor.publish_frame_start(self, dt);
        if self.game_state == GameState::Playing {
            self.apply_command(command);
        }
        frame_processor.update_game_state(self, dt);
        frame_processor.update_chunk_streaming(self);
        // Light propagation runs after game state
        frame_processor.update_light_system(self);
        frame_processor.publish_frame_end(self, frame_number);

        event_processor.process_ui_events(self);
        event_processor.process_graphics_events(self);
        event_processor.process_world_events(self);
        event_processor.process_input_events(self);
        event_processor.process_debug_events(self);

        // Check for generation cancellation from UI
        event_processor.check_generation_cancel(self);

        // Poll async generation if running
        GenerationProcessor::new().poll_generation(self);
    }

    fn init(&mut self, ctx: &mut moho_app::InitContext<'_>) {
        crate::app::renderer_setup::init_renderer_and_ui(self, ctx.window, &mut *ctx.renderer);
        crate::app::renderer_setup::apply_video_settings(&self.prefs, ctx.window);
        self.initial_render(ctx);
    }

    fn event(&mut self, ctx: &mut moho_app::EventContext<'_>, event: moho_app::Event) {
        match event {
            moho_app::Event::Window(event) => self.handle_window_event(ctx, event),
            moho_app::Event::Device(DeviceEvent::MouseMotion { delta }) => {
                self.handle_mouse_motion(delta);
            }
            moho_app::Event::Device(_) => {}
        }
    }

    fn frame(&mut self, ctx: &mut moho_app::FrameContext<'_>, _alpha: f32) {
        let frame_processor = FrameProcessor::new();

        if let Some(renderer) = ctx.renderer() {
            self.apply_pending_render(renderer);
        }
        self.drain_audio_events(ctx.audio());

        if self.exit_requested {
            self.exit_requested = false;
            self.autosave(ctx.renderer());
            ctx.request_exit();
            return;
        }

        frame_processor.update_hud_data(self, ctx.tick_length);
        if let Some(renderer) = ctx.renderer() {
            frame_processor.update_lighting(self, &mut *renderer);
            self.render_scene(renderer);
        }

        // Recall staging belt after render
        if let Some(ui_adapter) = &self.ui_adapter
            && let Ok(mut a) = ui_adapter.lock()
        {
            a.recall_staging_belt();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use moho_app::{Game, LoopConfig, TickContext};
    use moho_game::TICK_HZ;
    use moho_input::bindings::Binding;
    use moho_ui::actions::StrategyAction;

    fn playing_app() -> App {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(10.0, 20.0, 30.0), 0.7, 0.0);
        app
    }

    fn hold(app: &mut App, action: StrategyAction) {
        let Binding::Key(key) = *app
            .bindings
            .get(action)
            .first()
            .expect("the action has a default binding");
        app.input.active_keys.insert(key);
    }

    fn tick_context() -> TickContext {
        TickContext {
            tick: 0,
            tick_length: LoopConfig::new(TICK_HZ).tick_length(),
        }
    }

    #[test]
    fn held_forward_binding_is_in_the_tick_command_and_moves_player() {
        let mut app = playing_app();
        hold(&mut app, StrategyAction::MoveForward);
        let (yaw, _) = app.simulation.yaw_pitch();
        let expected_dir = glam::Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let before = app.simulation.position();
        let speed = app.simulation.player_controller.speed;
        let ctx_dt = tick_context().tick_length.as_secs_f32();

        let cmd = Game::command(&mut app);
        Game::tick(&mut app, &mut tick_context(), &cmd);

        let moved = app.simulation.position() - before;
        assert_eq!(cmd.input.forward, 1.0);
        assert!(
            moved.dot(expected_dir) > 0.9 * speed * ctx_dt,
            "player should advance along its facing, moved {moved:?}"
        );
        assert!(
            moved.normalize_or_zero().dot(expected_dir) > 0.99,
            "movement should follow the facing, moved {moved:?}"
        );
    }

    #[test]
    fn tick_without_held_keys_does_not_move_player() {
        let mut app = playing_app();
        let before = app.simulation.position();

        let cmd = Game::command(&mut app);
        Game::tick(&mut app, &mut tick_context(), &cmd);

        assert_eq!(cmd.input.forward, 0.0);
        assert!(!cmd.jump);
        assert!(
            (app.simulation.position() - before).length() < 1e-6,
            "no input, no movement"
        );
    }

    #[test]
    fn held_jump_binding_sets_the_command_jump_flag() {
        let mut app = playing_app();
        hold(&mut app, StrategyAction::Jump);

        let cmd = Game::command(&mut app);

        assert!(cmd.jump);
        assert_eq!(cmd.input.forward, 0.0);
    }

    #[test]
    fn command_outside_playing_is_the_default_command() {
        let mut app = playing_app();
        app.game_state = GameState::Menu;
        hold(&mut app, StrategyAction::MoveForward);
        hold(&mut app, StrategyAction::Jump);

        let cmd = Game::command(&mut app);

        assert_eq!(cmd.input.forward, 0.0);
        assert!(!cmd.jump);
    }
}
