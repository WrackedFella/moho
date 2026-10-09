//! The strategy game as a [`moho_app::Game`].

use crate::app::event_loop::{EventProcessor, FrameProcessor, GenerationProcessor};
use crate::game_state::GameState;
use crate::{App, RenderRequest};
use moho_game::controller::{CameraMode, ControllerInput};
use moho_input::action_map;
use moho_ui::actions::StrategyAction;
use winit::event::{ElementState, KeyEvent, WindowEvent};
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
        let frame = self.input.actions.end_tick();
        let is_active = |action: StrategyAction| frame.held(action);
        let axis = |positive: StrategyAction, negative: StrategyAction| -> f32 {
            f32::from(u8::from(is_active(positive))) - f32::from(u8::from(is_active(negative)))
        };

        let forward = axis(StrategyAction::MoveForward, StrategyAction::MoveBack);
        let right = axis(StrategyAction::MoveRight, StrategyAction::MoveLeft);
        // No up/down in isometric mode
        let up = if self.simulation.camera_mode() == CameraMode::FirstPerson {
            axis(StrategyAction::Ascend, StrategyAction::Descend)
        } else {
            0.0
        };
        let sprint = is_active(StrategyAction::Sprint);
        // A tap shorter than one tick still jumps.
        let jump = is_active(StrategyAction::Jump) || frame.pressed(StrategyAction::Jump);

        // Mouse motion is fed unnegated; look is inverted here.
        let (look_x, look_y) = frame.look();
        let (yaw_delta, pitch_delta) = (-look_x, -look_y);

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

    /// Autosaves with the renderer's lights, queued ones included, logging a failure.
    fn autosave(&mut self, renderer: Option<&mut (dyn moho_renderer::RendererBackend + 'static)>) {
        let lights = renderer.map_or_else(Vec::new, |r| {
            self.apply_pending_render(r);
            r.all_lights_as_descs()
        });
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
            return;
        }

        // Focus loss must release held actions whatever the UI or game state does.
        if matches!(event, WindowEvent::Focused(false)) {
            action_map::handle_window_event(&mut self.input.actions, &event);
        }

        if self.dispatcher.dispatch(&event) {
            return;
        }

        // Hotkeys run first; an event that leaves or enters play does not reach the action map.
        let was_playing = self.game_state == GameState::Playing;
        match &event {
            WindowEvent::CloseRequested => {
                self.autosave(ctx.renderer());
                ctx.request_exit();
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => self.handle_keyboard_input(key_event),
            _ => {}
        }
        if was_playing && self.game_state == GameState::Playing {
            action_map::handle_window_event(&mut self.input.actions, &event);
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

        event_processor.check_generation_cancel(self);

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
            moho_app::Event::Device(event) => self.handle_device_input(&event),
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
    use moho_app::{Game, GameClock, HeadlessLoop, LoopConfig, TickContext};
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
        let key = default_key(app, action);
        app.input.actions.key(key, true);
    }

    fn default_key(app: &App, action: StrategyAction) -> moho_input::key::Key {
        app.input
            .actions
            .bindings()
            .get(action)
            .iter()
            .find_map(|binding| match binding {
                Binding::Key(key) => Some(*key),
                Binding::Mouse(_) => None,
            })
            .expect("the action has a default key binding")
    }

    fn tick_app(app: &mut App, command: &StrategyCommand) {
        let mut clock = GameClock::default();
        let mut ctx = TickContext {
            tick: 0,
            tick_length: tick(),
            clock: &mut clock,
        };
        Game::tick(app, &mut ctx, command);
    }

    #[test]
    fn held_forward_binding_is_in_the_tick_command_and_moves_player() {
        let mut app = playing_app();
        hold(&mut app, StrategyAction::MoveForward);
        let (yaw, _) = app.simulation.yaw_pitch();
        let expected_dir = glam::Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let before = app.simulation.position();
        let speed = app.simulation.player_controller.speed;
        let ctx_dt = tick().as_secs_f32();

        let cmd = Game::command(&mut app);
        tick_app(&mut app, &cmd);

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
        tick_app(&mut app, &cmd);

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
    fn jump_tapped_within_one_tick_sets_the_command_jump_flag() {
        let mut app = playing_app();
        let key = default_key(&app, StrategyAction::Jump);
        app.input.actions.key(key, true);
        app.input.actions.key(key, false);

        let cmd = Game::command(&mut app);

        assert!(cmd.jump);
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

    use moho_app::{EventContext, FrameContext};
    use moho_renderer::{FrameError, InstanceGpu, LightingGpu, MaterialGpu, RendererBackend};

    /// Records the renderer calls the game makes; everything else is a no-op.
    #[derive(Default)]
    struct FakeRenderer {
        lights: Vec<(glam::Vec3, glam::Vec3, f32, f32)>,
        shadow_qualities: Vec<u8>,
        ssao_qualities: Vec<u8>,
        lightings: Vec<LightingGpu>,
        frames_begun: u32,
    }

    impl RendererBackend for FakeRenderer {
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
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), FrameError> {
            self.frames_begun += 1;
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
        fn update_lighting(&mut self, lighting: LightingGpu) {
            self.lightings.push(lighting);
        }
        fn add_point_light(
            &mut self,
            position: glam::Vec3,
            color: glam::Vec3,
            intensity: f32,
            range: f32,
        ) -> u32 {
            self.lights.push((position, color, intensity, range));
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: glam::Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, quality: u8) {
            self.shadow_qualities.push(quality);
        }
        fn set_ssao_quality(&mut self, quality: u8) {
            self.ssao_qualities.push(quality);
        }
        fn set_frame_callback_arc(
            &mut self,
            _cb: Option<std::sync::Arc<std::sync::Mutex<dyn moho_renderer::FrameCallback>>>,
        ) {
        }
    }

    fn tick() -> std::time::Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// An app whose autosaves land in a fresh temp dir, which must outlive it.
    fn app_with_saves_dir() -> (App, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut app = playing_app();
        app.saves_dir = temp.path().join("saves");
        (app, temp)
    }

    fn run_frame(app: &mut App, fake: &mut FakeRenderer) -> bool {
        run_frame_at(app, fake, GameClock::default().time_of_day())
    }

    fn run_frame_at(app: &mut App, fake: &mut FakeRenderer, hours: f32) -> bool {
        let mut clock = GameClock::default();
        clock.reset_to(hours);
        let mut ctx = FrameContext::new(tick(), &clock, Some(fake), None);
        Game::frame(app, &mut ctx, 0.5);
        ctx.exit_requested()
    }

    mod command_axes {
        use super::*;

        #[test]
        fn held_move_back_makes_forward_negative() {
            let mut app = playing_app();
            hold(&mut app, StrategyAction::MoveBack);

            let cmd = Game::command(&mut app);

            assert_eq!(cmd.input.forward, -1.0);
        }

        #[test]
        fn held_move_forward_and_back_cancel_out() {
            let mut app = playing_app();
            hold(&mut app, StrategyAction::MoveForward);
            hold(&mut app, StrategyAction::MoveBack);

            let cmd = Game::command(&mut app);

            assert_eq!(cmd.input.forward, 0.0);
        }

        #[test]
        fn held_left_and_right_map_to_negative_and_positive_right() {
            let mut app = playing_app();
            hold(&mut app, StrategyAction::MoveLeft);
            assert_eq!(Game::command(&mut app).input.right, -1.0);

            hold(&mut app, StrategyAction::MoveRight);
            assert_eq!(Game::command(&mut app).input.right, 0.0);
        }

        #[test]
        fn ascend_and_descend_apply_in_first_person() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            hold(&mut app, StrategyAction::Ascend);
            assert_eq!(Game::command(&mut app).input.up, 1.0);

            hold(&mut app, StrategyAction::Descend);
            assert_eq!(Game::command(&mut app).input.up, 0.0);
        }

        #[test]
        fn descend_alone_is_negative_up_in_first_person() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            hold(&mut app, StrategyAction::Descend);

            assert_eq!(Game::command(&mut app).input.up, -1.0);
        }

        #[test]
        fn ascend_is_ignored_in_isometric() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::Isometric);
            hold(&mut app, StrategyAction::Ascend);

            assert_eq!(Game::command(&mut app).input.up, 0.0);
        }
    }

    mod camera_mode {
        use super::*;

        #[test]
        fn toggle_switches_between_first_person_and_isometric() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);

            app.toggle_camera_mode();
            assert_eq!(app.simulation.camera_mode(), CameraMode::Isometric);

            app.toggle_camera_mode();
            assert_eq!(app.simulation.camera_mode(), CameraMode::FirstPerson);
        }
    }

    mod frame {
        use super::*;

        #[test]
        fn queued_point_light_reaches_the_renderer_and_the_queue_empties() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            app.pending_render.push(RenderRequest::AddPointLight {
                position: glam::Vec3::new(1.0, 2.0, 3.0),
                color: glam::Vec3::new(0.1, 0.2, 0.3),
                intensity: 7.0,
                range: 9.0,
            });

            run_frame(&mut app, &mut fake);

            assert_eq!(
                fake.lights,
                vec![(
                    glam::Vec3::new(1.0, 2.0, 3.0),
                    glam::Vec3::new(0.1, 0.2, 0.3),
                    7.0,
                    9.0
                )]
            );
            assert!(app.pending_render.is_empty());
        }

        #[test]
        fn queued_quality_changes_reach_the_renderer() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            app.pending_render.push(RenderRequest::ShadowQuality(2));
            app.pending_render.push(RenderRequest::SsaoQuality(3));

            run_frame(&mut app, &mut fake);

            assert_eq!(fake.shadow_qualities, vec![2]);
            assert_eq!(fake.ssao_qualities, vec![3]);
        }

        #[test]
        fn frame_renders_the_scene_and_does_not_exit() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();

            let exit = run_frame(&mut app, &mut fake);

            assert_eq!(fake.frames_begun, 1);
            assert!(!exit);
        }

        #[test]
        fn frame_without_a_renderer_does_not_panic() {
            let mut app = playing_app();
            app.pending_render.push(RenderRequest::ShadowQuality(1));
            let clock = GameClock::default();
            let mut ctx = FrameContext::new(tick(), &clock, None, None);

            Game::frame(&mut app, &mut ctx, 0.0);

            assert!(!ctx.exit_requested());
        }

        #[test]
        fn frame_drains_queued_audio_events_even_without_an_audio_system() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            app.event_bus
                .publish(moho_core::events::AudioEvent::ButtonClick);
            app.event_bus.process_deferred();

            run_frame(&mut app, &mut fake);

            assert!(app.audio_event_rx.try_recv().is_err());
        }

        #[test]
        fn exit_request_autosaves_requests_exit_clears_flag_and_skips_render() {
            let (mut app, _temp) = app_with_saves_dir();
            let mut fake = FakeRenderer::default();
            app.exit_requested = true;

            let exit = run_frame(&mut app, &mut fake);

            assert!(exit);
            assert!(!app.exit_requested);
            assert!(app.saves_dir.join("scene.bin").is_file());
            assert_eq!(fake.frames_begun, 0);
        }

        #[test]
        fn frame_without_exit_request_does_not_autosave() {
            let (mut app, _temp) = app_with_saves_dir();
            let mut fake = FakeRenderer::default();

            run_frame(&mut app, &mut fake);

            assert!(!app.saves_dir.exists());
        }

        #[test]
        fn playing_frame_uploads_full_sun_intensity_by_day() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();

            run_frame_at(&mut app, &mut fake, 12.0);

            assert_eq!(fake.lightings.len(), 1);
            assert_eq!(fake.lightings[0].sun_direction[3], 1.0);
        }

        #[test]
        fn playing_frame_uploads_zero_sun_intensity_by_night() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();

            run_frame_at(&mut app, &mut fake, 0.0);

            assert_eq!(fake.lightings.len(), 1);
            assert_eq!(fake.lightings[0].sun_direction[3], 0.0);
        }

        #[test]
        fn sun_exactly_on_the_horizon_has_no_intensity() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();

            run_frame_at(&mut app, &mut fake, 6.0);

            assert_eq!(fake.lightings[0].sun_direction[1], 0.0);
            assert_eq!(fake.lightings[0].sun_direction[3], 0.0);
        }

        #[test]
        fn lighting_is_not_uploaded_outside_playing() {
            let mut app = playing_app();
            app.game_state = GameState::Menu;
            let mut fake = FakeRenderer::default();

            run_frame(&mut app, &mut fake);

            assert!(fake.lightings.is_empty());
        }
    }

    mod events {
        use super::*;
        use winit::event::DeviceEvent;

        #[test]
        fn close_requested_autosaves_and_requests_exit() {
            let (mut app, _temp) = app_with_saves_dir();
            let mut fake = FakeRenderer::default();
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, Some(&mut fake), None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::CloseRequested),
            );

            assert!(ctx.exit_requested());
            assert!(app.saves_dir.join("scene.bin").is_file());
        }

        #[test]
        fn close_requested_applies_queued_lights_before_autosaving() {
            let (mut app, _temp) = app_with_saves_dir();
            let position = glam::Vec3::new(1.0, 2.0, 3.0);
            app.pending_render.push(RenderRequest::AddPointLight {
                position,
                color: glam::Vec3::ONE,
                intensity: 5.0,
                range: 20.0,
            });
            let mut fake = FakeRenderer::default();
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, Some(&mut fake), None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::CloseRequested),
            );

            assert!(app.pending_render.is_empty());
            assert_eq!(fake.lights, vec![(position, glam::Vec3::ONE, 5.0, 20.0)]);
        }

        #[test]
        fn unrelated_window_event_does_not_exit_or_save() {
            let (mut app, _temp) = app_with_saves_dir();
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::Focused(true)),
            );

            assert!(!ctx.exit_requested());
            assert!(!app.saves_dir.exists());
        }

        #[test]
        fn mouse_motion_in_first_person_reaches_the_next_command_look_delta() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Device(DeviceEvent::MouseMotion {
                    delta: (400.0, 200.0),
                }),
            );
            let cmd = Game::command(&mut app);

            assert_ne!(cmd.input.yaw_delta, 0.0);
            assert_ne!(cmd.input.pitch_delta, 0.0);
        }

        #[test]
        fn mouse_motion_in_isometric_is_ignored() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::Isometric);
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Device(DeviceEvent::MouseMotion {
                    delta: (400.0, 200.0),
                }),
            );
            let cmd = Game::command(&mut app);

            assert_eq!(cmd.input.yaw_delta, 0.0);
        }

        #[test]
        fn mouse_motion_outside_play_is_ignored() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app.game_state = GameState::Menu;
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Device(DeviceEvent::MouseMotion {
                    delta: (400.0, 200.0),
                }),
            );

            assert_eq!(app.input.actions.end_tick().look(), (0.0, 0.0));
        }

        #[test]
        fn positive_mouse_motion_turns_first_person_yaw_and_pitch_negative() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app.simulation
                .set_position_yaw_pitch(glam::Vec3::new(100.0, 100.0, 100.0), 0.0, 0.0);
            app.input.actions.set_filtering(false);
            app.input.actions.mouse_motion(5.0, 5.0);

            let cmd = Game::command(&mut app);
            tick_app(&mut app, &cmd);

            let (yaw, pitch) = app.simulation.yaw_pitch();
            assert!(yaw < 0.0, "yaw = {yaw}");
            assert!(pitch < 0.0, "pitch = {pitch}");
        }
    }

    mod day_night {
        use super::*;

        fn time_after_60_ticks(state: GameState, clock: GameClock) -> f32 {
            let mut app = playing_app();
            app.game_state = state;
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ).with_clock(clock));

            sim.step(&mut app, 60);

            sim.clock().time_of_day()
        }

        /// Day runs 12 h per 600 s, so one second of ticks is 0.02 h.
        fn noon() -> GameClock {
            GameClock::new(12.0, 600.0, 420.0)
        }

        #[test]
        fn time_of_day_advances_only_while_playing() {
            let playing = time_after_60_ticks(GameState::Playing, noon());

            assert!((playing - 12.02).abs() < 1e-3, "playing: {playing}");
            for state in [GameState::Menu, GameState::Paused, GameState::ConsoleOpen] {
                assert_eq!(time_after_60_ticks(state, noon()), 12.0, "{state:?}");
            }
        }

        #[test]
        fn playing_overrides_a_frozen_time_scale() {
            let mut frozen = noon();
            frozen.set_time_scale(0.0);

            let time = time_after_60_ticks(GameState::Playing, frozen);

            assert!((time - 12.02).abs() < 1e-3, "time {time}");
        }

        #[test]
        fn close_requested_autosaves_the_clock_time_of_day() {
            let (mut app, _temp) = app_with_saves_dir();
            let mut clock = GameClock::default();
            clock.reset_to(17.5);
            let mut ctx = EventContext::new(&clock, None, None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::CloseRequested),
            );

            let (spec, _, _) =
                crate::save::read_scene_and_metadata(app.saves_dir.join("scene.bin"))
                    .expect("autosave is readable");
            assert_eq!(spec.initial_time_of_day, 17.5);
        }
    }
}
