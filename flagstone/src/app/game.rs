//! The strategy game as a [`moho_app::Game`].

use crate::app::event_loop::{EventProcessor, FrameProcessor, GenerationProcessor};
use crate::game_state::GameState;
use crate::{App, RenderRequest};
use flagstone_game::controller::{
    CameraMode, ControllerInput, EDGE_PAN_MARGIN_PX, controller_to_camera, edge_pan_axes,
};
use flagstone_game::raycast::raycast_surface;
use flagstone_ui::actions::StrategyAction;
use flagstone_ui::console_commands::StrategyConsoleCommand;
use moho_core::pointer::{Ray, screen_ray};
use moho_input::action_map;
use moho_input::bindings::Input;
use moho_ui_shell::host::input_of;
use moho_ui_shell::{EngineCommand, Routing};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// How far a pointer ray reaches into the world, in metres.
pub const POINTER_RAY_RANGE: f32 = 256.0;

/// One tick's worth of player input.
#[derive(Clone, Debug, Default)]
pub struct StrategyCommand {
    pub input: ControllerInput,
    pub jump: bool,
    /// Switch between the strategy view and first person.
    pub toggle_view: bool,
    /// The world ray under the pointer when `Select` was pressed in the strategy view.
    ///
    /// A ray rather than a pixel, so the command replays without a window.
    pub select: Option<Ray>,
    /// The world ray under the pointer when the same press ordered a mining mark, in place of
    /// `select`, because the mining designation was active.
    pub mark: Option<Ray>,
    /// Strategy console commands typed since the last tick.
    pub console: Vec<StrategyConsoleCommand>,
    /// Engine console commands typed since the last tick.
    pub engine: Vec<EngineCommand>,
}

/// Yaw and pitch deltas for a look. Mouse motion is fed unnegated, so look is inverted here.
///
/// The one mapping, shared by the command and the camera so a tick lands where the frame drew.
fn look_to_turn((look_x, look_y): (f32, f32)) -> (f32, f32) {
    (-look_x, -look_y)
}

impl App {
    /// Sums the held bindings into the movement and look input for one tick.
    fn sample_command(&mut self) -> StrategyCommand {
        let frame = self.input.actions.end_tick();
        let is_active = |action: StrategyAction| frame.held(action);
        let axis = |positive: StrategyAction, negative: StrategyAction| -> f32 {
            f32::from(u8::from(is_active(positive))) - f32::from(u8::from(is_active(negative)))
        };

        let edge_pan = if self.simulation.camera_mode() == CameraMode::Isometric
            && self.game_state == GameState::Playing
            && self.input.focused
        {
            edge_pan_axes(frame.pointer(), self.viewport, EDGE_PAN_MARGIN_PX)
        } else {
            glam::Vec2::ZERO
        };
        let forward = (axis(StrategyAction::MoveForward, StrategyAction::MoveBack) + edge_pan.y)
            .clamp(-1.0, 1.0);
        let right = (axis(StrategyAction::MoveRight, StrategyAction::MoveLeft) + edge_pan.x)
            .clamp(-1.0, 1.0);
        // No up/down in isometric mode
        let up = if self.simulation.camera_mode() == CameraMode::FirstPerson {
            axis(StrategyAction::Ascend, StrategyAction::Descend)
        } else {
            0.0
        };
        let sprint = is_active(StrategyAction::Sprint);
        // A tap shorter than one tick still jumps.
        let jump = is_active(StrategyAction::Jump) || frame.pressed(StrategyAction::Jump);

        let (yaw_delta, pitch_delta) = look_to_turn(frame.look());

        // The camera the player saw: the one drawn, not the one this tick will move.
        let select = (self.simulation.camera_mode() == CameraMode::Isometric
            && frame.pressed(StrategyAction::Select))
        .then(|| frame.press_pointer())
        .flatten()
        .and_then(|(x, y)| {
            screen_ray(
                self.camera.0,
                self.camera.1,
                self.viewport,
                glam::Vec2::new(x, y),
            )
        });

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
            toggle_view: frame.pressed(StrategyAction::Possess),
            select,
            mark: None,
            console: Vec::new(),
            engine: Vec::new(),
        }
    }

    /// Where one key or mouse button press or release goes. The console sees it before egui or
    /// the action map; `Opened` and `Closed` also enter and leave `GameState::ConsoleOpen`.
    /// Outside `Playing` and `ConsoleOpen` the console is skipped and the input is the game's.
    /// Takes an `Input` so it is testable without a winit `KeyEvent`.
    fn route_console_input(&mut self, input: Input, pressed: bool, repeat: bool) -> Routing {
        if !matches!(self.game_state, GameState::Playing | GameState::ConsoleOpen) {
            return Routing::Game;
        }
        let Some(ui_adapter) = &self.ui_adapter else {
            return Routing::Game;
        };
        let routing = match ui_adapter.lock() {
            Ok(mut adapter) => adapter.on_key(input, pressed, repeat),
            Err(error) => {
                tracing::warn!(%error, "UI adapter lock poisoned; console input skipped");
                return Routing::Game;
            }
        };
        // The adapter lock is released: the transitions lock it again.
        match routing {
            Routing::Opened => self.enter_console(),
            Routing::Closed => self.exit_console(),
            Routing::Console | Routing::Game | Routing::Menu => {}
        }
        routing
    }

    /// Takes the commands the console has queued: strategy's, then the engine's.
    fn take_console_commands(&mut self) -> (Vec<StrategyConsoleCommand>, Vec<EngineCommand>) {
        let Some(ui_adapter) = &self.ui_adapter else {
            return (Vec::new(), Vec::new());
        };
        match ui_adapter.lock() {
            Ok(mut adapter) => (
                adapter.take_console_commands(),
                adapter.take_engine_commands(),
            ),
            Err(error) => {
                tracing::warn!(%error, "UI adapter lock poisoned; console commands dropped");
                (Vec::new(), Vec::new())
            }
        }
    }

    /// Feeds pad events to the action map while playing with focus. Otherwise the
    /// events are discarded unapplied, so no press edge waits for resume, and pad
    /// actions are released.
    fn poll_gamepads(&mut self) {
        let live = self.game_state == GameState::Playing && self.input.focused;
        if let Some(gamepads) = self.input.gamepads.as_mut() {
            if live {
                gamepads.poll(&mut self.input.actions);
            } else {
                gamepads.discard();
            }
        }
        if !live {
            self.input.actions.pad_disconnected();
        }
    }

    fn apply_command(&mut self, command: &StrategyCommand) {
        *self.simulation.controller_input_mut() = command.input;
        self.physics.jump_pressed = command.jump;
        if command.toggle_view {
            self.toggle_camera_mode();
            self.apply_cursor();
        }
        if let Some(ray) = command.select
            && let Some(light_system) = &self.light_system
        {
            self.pointer_hit = raycast_surface(
                light_system.grid(),
                ray.origin,
                ray.direction,
                POINTER_RAY_RANGE,
            );
            tracing::debug!(hit = ?self.pointer_hit, "Pointer ray cast");
        }
    }

    /// Applies what the player asked for through the HUD this frame.
    fn apply_hud_intents(&mut self, _intents: flagstone_ui::overlays::HudIntents) {}

    fn apply_pending_render(&mut self, renderer: &mut dyn moho_renderer::RendererBackend) {
        for request in std::mem::take(&mut self.pending_render) {
            match request {
                RenderRequest::AddPointLight {
                    position,
                    color,
                    intensity,
                    range,
                } => {
                    renderer.add_point_light(position, color, intensity, range);
                }
                RenderRequest::ShadowQuality(quality) => {
                    let applied = renderer.set_shadow_quality(quality);
                    self.shadow_quality_applied(applied);
                }
                RenderRequest::SsaoQuality(quality) => renderer.set_ssao_quality(quality),
                RenderRequest::BloomQuality(quality) => renderer.set_bloom_quality(quality),
                RenderRequest::ViewHeight(height) => renderer.set_view_height(height),
                RenderRequest::Vsync(on) => renderer.set_vsync(on),
                RenderRequest::Brightness(brightness) => renderer.set_brightness(brightness),
                RenderRequest::ToneMap(enabled) => renderer.set_tone_map(enabled),
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

    /// The camera to draw this frame. In first person it is the simulation turned by the look
    /// frames sampled but no tick has consumed yet: while walking, the eye blends between the
    /// last two ticks; otherwise it is the tick camera's, since ticks keep cycling `alpha`
    /// outside play while the player stands frozen. Isometric draws the tick camera.
    fn render_camera(&self, alpha: f32) -> (glam::Mat4, glam::Mat4, glam::Vec3) {
        if self.simulation.camera_mode() != CameraMode::FirstPerson {
            return self.camera;
        }
        let pending = self.input.actions.pending_look();
        let (yaw_delta, pitch_delta) = look_to_turn(pending);
        let walking_states = self
            .is_kcc_walking()
            .then(|| self.physics.player_states())
            .flatten();
        match walking_states {
            Some((previous, current)) => {
                let view = moho_physics::first_person_view(
                    &previous,
                    &current,
                    alpha,
                    (yaw_delta, pitch_delta),
                );
                (view.view_matrix(), self.camera.1, view.eye)
            }
            // Nothing shown since the tick: its camera stands, which is also the initial
            // camera before any tick.
            None if pending == (0.0, 0.0) => self.camera,
            None => controller_to_camera(
                &self
                    .simulation
                    .player_controller
                    .turned(yaw_delta, pitch_delta),
            ),
        }
    }

    /// Whether ticks move the player through the character controller: playing,
    /// first person, collision on.
    pub(crate) fn is_kcc_walking(&self) -> bool {
        self.game_state == GameState::Playing
            && self.simulation.camera_mode() == CameraMode::FirstPerson
            && self.physics.is_kcc_active()
    }

    fn render_scene(
        &mut self,
        renderer: &mut dyn moho_renderer::RendererBackend,
        camera: (glam::Mat4, glam::Mat4, glam::Vec3),
    ) {
        if let Err(e) = self.scene.render(
            renderer,
            self.entities.actors.spheres(),
            self.entities.actors.cubes(),
            self.mesh_handle,
            self.cube_mesh_handle,
            camera,
        ) {
            tracing::warn!(error = %e, "Skipped frame");
        }
    }

    fn initial_render(&mut self, ctx: &mut moho_app::InitContext<'_>) {
        tracing::info!("Initial render");
        self.render_scene(&mut *ctx.renderer, self.camera);
        ctx.window.request_redraw();
    }

    /// Autosaves with the renderer's lights, queued ones included, logging a failure, and
    /// tells the schedule how it went.
    fn autosave(
        &mut self,
        clock: &moho_app::GameClock,
        renderer: Option<&mut (dyn moho_renderer::RendererBackend + 'static)>,
    ) {
        let lights = renderer.map_or_else(Vec::new, |r| {
            self.apply_pending_render(r);
            r.all_lights_as_descs()
        });
        match self.auto_save_on_shutdown(clock, &lights) {
            Ok(()) => self.autosave.saved(),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to auto-save");
                self.autosave.save_failed();
            }
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
            action_map::handle_window_event(&mut self.input.actions, &event);
            return;
        }

        if let WindowEvent::Resized(size) = &event {
            self.viewport = glam::UVec2::new(size.width, size.height);
        }

        if let WindowEvent::Focused(focused) = event {
            self.input.focused = focused;
        }
        // Focus loss must release held actions whatever the UI or game state does.
        if matches!(event, WindowEvent::Focused(false)) {
            action_map::handle_window_event(&mut self.input.actions, &event);
        }

        if let Some((input, pressed, repeat)) = input_of(&event) {
            match self.route_console_input(input, pressed, repeat) {
                Routing::Opened | Routing::Closed => return,
                // Only egui sees what the open console takes.
                Routing::Console => {
                    self.dispatcher.dispatch(&event);
                    return;
                }
                Routing::Game | Routing::Menu => {}
            }
        }

        if self.take_escape_before_ui(&event) {
            return;
        }

        if self.dispatcher.dispatch(&event) {
            return;
        }

        // Hotkeys run first; an event that leaves or enters play does not reach the action map.
        let was_playing = self.game_state == GameState::Playing;
        match &event {
            WindowEvent::CloseRequested => {
                self.autosave(ctx.clock(), ctx.renderer());
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

    /// Input stops reaching the action map outside play: held actions would stick, and a
    /// press recorded but not yet ticked would fire on resume.
    pub(crate) fn drop_pending_input(&mut self) {
        self.input.actions.release_all();
        self.input.actions.discard_edges();
    }

    fn toggle_camera_mode(&mut self) {
        // Placeholder cues until the mode switch has its own clips.
        let cue = match self.simulation.camera_mode() {
            CameraMode::FirstPerson => moho_core::events::AudioEvent::Cancel,
            CameraMode::Isometric => moho_core::events::AudioEvent::Confirm,
        };
        self.event_bus.publish(cue);
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
        // Before the `Playing` check: the console is open only outside `Playing`.
        let (console, engine) = self.take_console_commands();
        let mut command = if self.game_state == GameState::Playing {
            self.sample_command()
        } else {
            StrategyCommand::default()
        };
        command.console = console;
        command.engine = engine;
        command
    }

    fn tick(&mut self, ctx: &mut moho_app::TickContext<'_>, command: &StrategyCommand) {
        let dt = ctx.tick_length.as_secs_f32();
        // The world's time runs only while playing.
        ctx.clock
            .set_time_scale(f32::from(self.game_state == GameState::Playing));
        if self.game_state == GameState::Playing {
            self.autosave.tick();
            self.pawn.pickups.tick();
        }
        let frame_processor = FrameProcessor::new();
        let event_processor = EventProcessor::new();

        let frame_number = frame_processor.publish_frame_start(self, dt);
        if self.game_state == GameState::Playing {
            self.apply_command(command);
        }
        for console_command in &command.console {
            crate::app::console::apply_console_command(self, console_command);
        }
        for engine_command in &command.engine {
            crate::app::console::apply_engine_command(self, engine_command);
        }
        frame_processor.update_game_state(self, dt);
        frame_processor.update_chunk_streaming(self);
        // Light propagation runs after game state
        frame_processor.update_light_system(self);
        frame_processor.publish_frame_end(self, frame_number);

        event_processor.process_ui_events(self, ctx.clock);
        event_processor.process_graphics_events(self, ctx.clock);
        event_processor.process_world_events(self);
        event_processor.process_input_events(self);
        event_processor.process_debug_events(self);

        event_processor.check_generation_cancel(self);

        GenerationProcessor::new().poll_generation(self);
    }

    fn init(&mut self, ctx: &mut moho_app::InitContext<'_>) {
        let size = ctx.window.inner_size();
        self.viewport = glam::UVec2::new(size.width, size.height);
        crate::app::renderer_setup::init_renderer_and_ui(self, ctx.window, &mut *ctx.renderer);
        crate::app::renderer_setup::apply_video_settings(&self.prefs, ctx.window);
        ctx.renderer.set_vsync(self.prefs.vsync());
        self.set_pending_frame_cap(self.prefs.frame_cap());
        self.initial_render(ctx);
    }

    fn event(&mut self, ctx: &mut moho_app::EventContext<'_>, event: moho_app::Event) {
        match event {
            moho_app::Event::Window(event) => self.handle_window_event(ctx, event),
            moho_app::Event::Device(event) => self.handle_device_input(&event),
        }
    }

    fn frame(&mut self, ctx: &mut moho_app::FrameContext<'_>, alpha: f32) {
        let frame_processor = FrameProcessor::new();

        if std::mem::take(&mut self.frame_cap_changed) {
            ctx.set_frame_cap(self.pending_frame_cap);
        }
        self.poll_gamepads();
        if self.game_state == GameState::Playing
            && self.input.focused
            && self.simulation.camera_mode() == CameraMode::FirstPerson
        {
            self.input.actions.sample_frame(ctx.frame_dt);
        }
        let chosen = self
            .ui_adapter
            .as_ref()
            .and_then(|adapter| adapter.lock().ok())
            .and_then(|mut adapter| adapter.take_hud_intents().view_level);
        crate::app::view_level::update(self, chosen);
        if let Some(renderer) = ctx.renderer() {
            self.apply_pending_render(renderer);
        }
        let camera = self.render_camera(alpha);
        if let Some(audio) = ctx.audio() {
            if let Some(settings) = self.pending_audio_settings.take() {
                audio.set_settings(settings);
            }
            audio.set_listener(moho_audio::Listener::from_view(camera.0));
        }
        self.drain_audio_events(ctx.audio());

        if self.exit_requested {
            self.exit_requested = false;
            self.autosave(ctx.clock(), ctx.renderer());
            ctx.request_exit();
            return;
        }

        if self.main_menu_requested {
            self.main_menu_requested = false;
            self.autosave(ctx.clock(), ctx.renderer());
            self.show_menu();
        }

        if self.autosave.due() {
            self.autosave(ctx.clock(), ctx.renderer());
        }

        let clock = ctx.clock();
        frame_processor.update_hud_data(self, clock, ctx.tick_length);
        frame_processor.update_cut_caps(self);
        if let Some(renderer) = ctx.renderer() {
            frame_processor.update_lighting(self, clock, &mut *renderer);
            self.render_scene(renderer, camera);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use flagstone_game::TICK_HZ;
    use flagstone_ui::actions::StrategyAction;
    use moho_app::{Game, GameClock, HeadlessLoop, LoopConfig, TickContext};
    use moho_input::bindings::Input;
    use moho_physics::HitTag;

    fn playing_app() -> App {
        let mut app = App::headless();
        app.game_state = GameState::Playing;
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(10.0, 20.0, 30.0), 0.7, 0.0);
        app
    }

    #[test]
    fn app_starts_with_audio_settings_from_saved_prefs() {
        let mut prefs = moho_core::prefs::Prefs::default();
        *prefs.sound_effect_volume_mut() = 5.0;

        let app = App::from_config(
            crate::app::config::AppConfig::builder()
                .prefs(prefs)
                .build(),
        );

        let settings = app
            .pending_audio_settings
            .as_ref()
            .expect("startup queues audio settings from prefs");
        let volume = settings.effective_volume(&moho_audio::AudioCategory::SoundEffect);
        assert!((volume - 0.5).abs() < 1e-6, "volume={volume}");
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
            .find_map(|binding| match binding.input {
                Input::Key(key) => Some(key),
                Input::Mouse(_) | Input::Pad(_) => None,
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

    use std::time::Duration;

    const TICKS: u32 = 120;

    fn floor() -> moho_render_api::WorldMesh {
        moho_render_api::WorldMesh::new(
            vec![
                [-50.0, 0.0, -50.0],
                [50.0, 0.0, -50.0],
                [-50.0, 0.0, 50.0],
                [50.0, 0.0, 50.0],
            ],
            vec![[0.0, 1.0, 0.0]; 4],
            vec![1.0; 4],
            vec![[0.0; 3]; 4],
            vec![1.0; 4],
            vec![0; 4],
            vec![0, 2, 1, 1, 2, 3],
        )
        .expect("valid floor mesh")
    }

    /// A first-person player standing on a floor, facing +Z, holding forward and jump.
    fn walking_app() -> App {
        let mut app = standing_app();
        hold(&mut app, StrategyAction::MoveForward);
        hold(&mut app, StrategyAction::Jump);
        app
    }

    /// A first-person player on a floor, facing +Z, holding nothing.
    fn standing_app() -> App {
        let mut app = playing_app();
        app.simulation.set_camera_mode(CameraMode::FirstPerson);
        app.simulation
            .set_position_yaw_pitch(glam::Vec3::new(0.0, 1.2, 0.0), 0.0, 0.0);
        app.physics
            .world
            .as_mut()
            .expect("physics world")
            .set_world_mesh(moho_render_api::WorldMeshId(1), &floor(), HitTag(0));
        app.physics.spawn_player(glam::Vec3::new(0.0, 1.2, 0.0));
        app
    }

    /// Position after `TICKS` ticks delivered in frames of `1 / fps` s, and the highest point reached.
    fn drive_at(fps: u64) -> (glam::Vec3, f32) {
        let mut app = walking_app();
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        // Rounded up so the integer accumulator never lands one tick short.
        let frame = Duration::from_nanos(1_000_000_000u64.div_ceil(fps));
        let mut ticks = 0;
        let mut highest = f32::MIN;

        while ticks < TICKS {
            ticks += sim.advance(&mut app, frame);
            highest = highest.max(app.simulation.position().y);
        }

        assert_eq!(ticks, TICKS, "frames deliver exactly the requested ticks");
        (app.simulation.position(), highest)
    }

    #[test]
    fn walking_and_jumping_end_at_the_same_position_at_30_60_and_144_fps() {
        let (at_60, highest) = drive_at(60);

        let (at_30, _) = drive_at(30);
        let (at_144, _) = drive_at(144);

        assert!(at_60.z > 2.0, "the player walked forward: {at_60:?}");
        assert!(highest > 1.5, "the player jumped: highest {highest}");
        assert!(
            at_30.distance(at_60) < 1e-4,
            "30 fps {at_30:?} vs 60 fps {at_60:?}"
        );
        assert!(
            at_144.distance(at_60) < 1e-4,
            "144 fps {at_144:?} vs 60 fps {at_60:?}"
        );
    }

    use moho_app::{EventContext, FrameContext};
    use moho_renderer::{FrameError, InstanceGpu, LightingGpu, MaterialGpu, RendererBackend};

    /// Records the renderer calls the game makes; everything else is a no-op.
    #[derive(Default)]
    struct FakeRenderer {
        lights: Vec<(glam::Vec3, glam::Vec3, f32, f32)>,
        shadow_qualities: Vec<u8>,
        /// Highest shadow level the fake GPU can allocate; `None` allows every level.
        shadow_cap: Option<u8>,
        ssao_qualities: Vec<u8>,
        view_heights: Vec<Option<f32>>,
        vsyncs: Vec<bool>,
        brightnesses: Vec<f32>,
        tone_maps: Vec<bool>,
        lightings: Vec<LightingGpu>,
        frames_begun: u32,
        cameras: Vec<(glam::Mat4, glam::Mat4, glam::Vec3)>,
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
            _material: &[u32],
            _indices: &[u32],
        ) -> u32 {
            0
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(
            &mut self,
            camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), FrameError> {
            self.frames_begun += 1;
            self.cameras.push(camera);
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
        fn set_shadow_quality(&mut self, quality: u8) -> u8 {
            self.shadow_qualities.push(quality);
            self.shadow_cap.map_or(quality, |cap| quality.min(cap))
        }
        fn set_ssao_quality(&mut self, quality: u8) {
            self.ssao_qualities.push(quality);
        }
        fn set_view_height(&mut self, height: Option<f32>) {
            self.view_heights.push(height);
        }
        fn set_vsync(&mut self, on: bool) {
            self.vsyncs.push(on);
        }
        fn set_brightness(&mut self, brightness: f32) {
            self.brightnesses.push(brightness);
        }
        fn set_tone_map(&mut self, enabled: bool) {
            self.tone_maps.push(enabled);
        }
    }

    fn tick() -> std::time::Duration {
        LoopConfig::new(TICK_HZ).tick_length()
    }

    /// An app with a loaded world whose autosaves land in a fresh temp dir, which must
    /// outlive it.
    fn app_with_saves_dir() -> (App, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("temp dir");
        let mut app = playing_app();
        app.generation.last_spec = Some(flagstone_game::scene_builders::WorldSpec::default());
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

    fn frame_without_renderer(app: &mut App) {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(tick(), &clock, None, None);
        Game::frame(app, &mut ctx, 0.5);
    }

    fn walker_state(app: &App) -> moho_physics::CharacterState {
        app.physics
            .world
            .as_ref()
            .expect("physics world")
            .character(app.physics.player.expect("player"))
            .expect("player resolves")
    }

    /// Walks `TICKS` ticks, rendering `frames_per_tick` frames after each, and returns the final state.
    fn walk_rendering(
        frames_per_tick: u32,
        fake: &mut FakeRenderer,
    ) -> moho_physics::CharacterState {
        let mut app = walking_app();
        let mut clock = GameClock::default();
        for n in 0..TICKS {
            let command = Game::command(&mut app);
            let mut ctx = TickContext {
                tick: u64::from(n),
                tick_length: tick(),
                clock: &mut clock,
            };
            Game::tick(&mut app, &mut ctx, &command);
            for f in 0..frames_per_tick {
                let alpha = f as f32 / frames_per_tick as f32;
                let mut ctx = FrameContext::new(tick(), &clock, Some(&mut *fake), None);
                Game::frame(&mut app, &mut ctx, alpha);
            }
        }
        walker_state(&app)
    }

    #[test]
    fn rendering_many_frames_between_ticks_leaves_the_walk_unchanged() {
        let mut silent = FakeRenderer::default();
        let mut rendering = FakeRenderer::default();

        let without_frames = walk_rendering(0, &mut silent);
        let with_frames = walk_rendering(10, &mut rendering);

        assert_eq!(silent.frames_begun, 0);
        assert_eq!(rendering.frames_begun, 10 * TICKS, "frames were rendered");
        assert!(without_frames.position.z > 2.0, "the player walked");
        assert_eq!(with_frames, without_frames);
    }

    #[test]
    fn walking_frame_uses_the_blended_eye() {
        let mut app = walking_app();
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let before = walker_state(&app).position;
        sim.step(&mut app, 1);
        let after = walker_state(&app).position;
        assert!(before.distance(after) > 1e-3, "the tick moved the player");
        let mut fake = FakeRenderer::default();

        run_frame(&mut app, &mut fake);

        let (view, _projection, eye) = *fake.cameras.last().expect("a frame began");
        let expected = before.lerp(after, 0.5);
        assert!(eye.distance(expected) < 1e-4, "eye {eye:?} vs {expected:?}");
        assert!(
            view.transform_point3(eye).length() < 1e-4,
            "the view matrix is centred on the eye"
        );
    }

    /// Walks one tick, then returns the app with a renderer that has seen no frame.
    fn app_after_a_moving_tick() -> App {
        let mut app = walking_app();
        let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));
        let before = walker_state(&app).position;
        sim.step(&mut app, 1);
        let after = walker_state(&app).position;
        assert!(before.distance(after) > 1e-3, "the tick moved the player");
        app
    }

    #[test]
    fn paused_frame_renders_the_tick_camera() {
        let mut app = app_after_a_moving_tick();
        app.game_state = GameState::Menu;
        let mut fake = FakeRenderer::default();

        run_frame(&mut app, &mut fake);

        assert_eq!(fake.cameras.last().copied(), Some(app.camera));
    }

    #[test]
    fn a_frame_before_any_tick_draws_the_initial_camera() {
        let mut app = App::headless();
        let mut fake = FakeRenderer::default();

        run_frame(&mut app, &mut fake);

        assert_eq!(fake.cameras.last().copied(), Some(app.camera));
    }

    #[test]
    fn isometric_frame_renders_the_tick_camera() {
        let mut app = app_after_a_moving_tick();
        app.simulation.set_camera_mode(CameraMode::Isometric);
        let mut fake = FakeRenderer::default();

        run_frame(&mut app, &mut fake);

        assert_eq!(fake.cameras.last().copied(), Some(app.camera));
    }

    #[test]
    fn noclip_frame_renders_the_tick_camera() {
        let mut app = app_after_a_moving_tick();
        app.physics.noclip = true;
        let mut fake = FakeRenderer::default();

        run_frame(&mut app, &mut fake);

        assert_eq!(fake.cameras.last().copied(), Some(app.camera));
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
            let toggle = StrategyCommand {
                toggle_view: true,
                ..StrategyCommand::default()
            };

            tick_app(&mut app, &toggle);
            assert_eq!(app.simulation.camera_mode(), CameraMode::Isometric);

            tick_app(&mut app, &toggle);
            assert_eq!(app.simulation.camera_mode(), CameraMode::FirstPerson);
        }
    }

    mod possess {
        use super::*;
        use moho_core::events::AudioEvent;
        use std::sync::mpsc::Receiver;

        fn tap(app: &mut App) {
            let key = default_key(app, StrategyAction::Possess);
            app.input.actions.key(key, true);
            app.input.actions.key(key, false);
        }

        fn press(app: &mut App) {
            let key = default_key(app, StrategyAction::Possess);
            app.input.actions.key(key, true);
        }

        fn release(app: &mut App) {
            let key = default_key(app, StrategyAction::Possess);
            app.input.actions.key(key, false);
        }

        fn step(app: &mut App) -> StrategyCommand {
            let command = Game::command(app);
            tick_app(app, &command);
            command
        }

        fn mode(app: &App) -> CameraMode {
            app.simulation.camera_mode()
        }

        fn pose(app: &App) -> (glam::Vec3, (f32, f32)) {
            (app.simulation.position(), app.simulation.yaw_pitch())
        }

        fn app_in(camera_mode: CameraMode) -> App {
            let mut app = playing_app();
            app.simulation
                .set_position_yaw_pitch(glam::Vec3::new(10.0, 20.0, 30.0), 0.7, 0.2);
            app.simulation.set_camera_mode(camera_mode);
            app
        }

        fn record_audio(app: &App) -> Receiver<AudioEvent> {
            let (tx, rx) = std::sync::mpsc::channel();
            app.event_bus.subscribe(move |event: &AudioEvent| {
                let _ = tx.send(event.clone());
            });
            rx
        }

        fn cues(app: &App, audio: &Receiver<AudioEvent>) -> Vec<AudioEvent> {
            app.event_bus.process_deferred();
            audio.try_iter().collect()
        }

        #[test]
        fn possess_enters_first_person_at_the_pawns_pose() {
            let mut app = app_in(CameraMode::Isometric);
            let before = pose(&app);
            tap(&mut app);

            step(&mut app);

            assert_eq!(mode(&app), CameraMode::FirstPerson);
            assert_eq!(pose(&app), before);
        }

        #[test]
        fn possess_return_possess_restores_the_same_pawn_pose() {
            let mut app = app_in(CameraMode::FirstPerson);
            let before = pose(&app);

            tap(&mut app);
            step(&mut app);
            let in_strategy = mode(&app);
            tap(&mut app);
            step(&mut app);

            assert_eq!(in_strategy, CameraMode::Isometric);
            assert_eq!(mode(&app), CameraMode::FirstPerson);
            assert_eq!(pose(&app), before);
        }

        #[test]
        fn possess_key_in_first_person_switches_to_strategy() {
            let mut app = app_in(CameraMode::FirstPerson);
            tap(&mut app);

            step(&mut app);

            assert_eq!(mode(&app), CameraMode::Isometric);
        }

        #[test]
        fn mode_switch_applies_on_the_next_tick_not_on_the_key_event() {
            let mut app = app_in(CameraMode::FirstPerson);

            press(&mut app);
            let on_the_key_event = mode(&app);
            let command = Game::command(&mut app);
            let before_the_tick = mode(&app);
            tick_app(&mut app, &command);

            assert_eq!(on_the_key_event, CameraMode::FirstPerson);
            assert!(command.toggle_view, "the press is in the sampled command");
            assert_eq!(before_the_tick, CameraMode::FirstPerson);
            assert_eq!(mode(&app), CameraMode::Isometric);
        }

        #[test]
        fn a_tap_shorter_than_a_tick_still_switches_once() {
            let mut app = app_in(CameraMode::FirstPerson);
            tap(&mut app);

            let first = step(&mut app);
            let after_first = mode(&app);
            let second = step(&mut app);

            assert!(first.toggle_view);
            assert_eq!(after_first, CameraMode::Isometric);
            assert!(!second.toggle_view, "the edge is consumed by one tick");
            assert_eq!(mode(&app), CameraMode::Isometric);
        }

        #[test]
        fn held_possess_key_switches_once() {
            let mut app = app_in(CameraMode::FirstPerson);
            press(&mut app);

            let first = step(&mut app);
            press(&mut app);
            let repeats: Vec<_> = (0..5).map(|_| step(&mut app)).collect();

            assert!(first.toggle_view);
            assert!(
                repeats.iter().all(|command| !command.toggle_view),
                "key repeat toggled again"
            );
            assert_eq!(mode(&app), CameraMode::Isometric);

            release(&mut app);
            step(&mut app);
            assert_eq!(mode(&app), CameraMode::Isometric);
        }

        #[test]
        fn pawn_does_not_move_while_in_strategy_mode() {
            let mut app = standing_app();
            let before = (pose(&app), walker_state(&app).position);
            tap(&mut app);
            step(&mut app);
            hold(&mut app, StrategyAction::MoveForward);
            hold(&mut app, StrategyAction::MoveRight);
            hold(&mut app, StrategyAction::Jump);

            for _ in 0..30 {
                step(&mut app);
            }

            assert_eq!(mode(&app), CameraMode::Isometric);
            assert_eq!((pose(&app), walker_state(&app).position), before);
        }

        #[test]
        fn switch_publishes_the_placeholder_cue() {
            let mut app = app_in(CameraMode::Isometric);
            let audio = record_audio(&app);

            tap(&mut app);
            step(&mut app);
            let entering = cues(&app, &audio);
            tap(&mut app);
            step(&mut app);
            let returning = cues(&app, &audio);

            assert!(
                matches!(entering.as_slice(), [AudioEvent::Confirm]),
                "entering first person: {entering:?}"
            );
            assert!(
                matches!(returning.as_slice(), [AudioEvent::Cancel]),
                "returning to strategy: {returning:?}"
            );
        }

        #[test]
        fn a_possess_press_queued_before_a_pause_does_not_switch_on_resume() {
            let mut app = app_in(CameraMode::FirstPerson);
            let audio = record_audio(&app);
            tap(&mut app);

            app.game_state = GameState::Paused;
            app.drop_pending_input();
            app.game_state = GameState::Playing;
            step(&mut app);

            assert_eq!(mode(&app), CameraMode::FirstPerson);
            assert!(cues(&app, &audio).is_empty());
        }

        #[test]
        fn possess_is_ignored_outside_playing() {
            let mut app = app_in(CameraMode::FirstPerson);
            let audio = record_audio(&app);
            app.game_state = GameState::Menu;
            tap(&mut app);

            let sampled = step(&mut app);
            let forced = StrategyCommand {
                toggle_view: true,
                ..StrategyCommand::default()
            };
            tick_app(&mut app, &forced);

            assert!(!sampled.toggle_view);
            assert_eq!(mode(&app), CameraMode::FirstPerson);
            assert!(cues(&app, &audio).is_empty());
        }
    }

    mod frame {
        use super::*;
        use moho_core::prefs::Prefs;

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
        fn a_shadow_quality_the_gpu_cannot_allocate_is_saved_at_the_level_it_fell_back_to() {
            let temp = tempfile::tempdir().expect("temp dir");
            let path = temp.path().join("prefs.ini");
            let mut app = playing_app();
            app.prefs.set_shadow_quality(4);
            app.prefs_path = Some(path.clone());
            let mut fake = FakeRenderer {
                shadow_cap: Some(2),
                ..FakeRenderer::default()
            };
            app.pending_render.push(RenderRequest::ShadowQuality(4));

            run_frame(&mut app, &mut fake);

            assert_eq!(fake.shadow_qualities, vec![4]);
            assert_eq!(app.prefs.shadow_quality(), 2);
            let (on_disk, _) = moho_core::prefs::Prefs::load_from(&path);
            assert_eq!(on_disk.shadow_quality(), 2, "the next start asks for 2");
        }

        #[test]
        fn a_shadow_quality_the_gpu_allocates_is_not_saved_again() {
            let temp = tempfile::tempdir().expect("temp dir");
            let path = temp.path().join("prefs.ini");
            let mut app = playing_app();
            app.prefs.set_shadow_quality(4);
            app.prefs_path = Some(path.clone());
            app.pending_render.push(RenderRequest::ShadowQuality(4));

            run_frame(&mut app, &mut FakeRenderer::default());

            assert_eq!(app.prefs.shadow_quality(), 4);
            assert!(!path.exists(), "nothing changed, so nothing was written");
        }

        #[test]
        fn view_height_request_reaches_the_renderer() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            app.pending_render
                .push(RenderRequest::ViewHeight(Some(12.5)));
            app.pending_render.push(RenderRequest::ViewHeight(None));

            run_frame(&mut app, &mut fake);

            assert_eq!(fake.view_heights, vec![Some(12.5), None]);
            assert!(app.pending_render.is_empty());
        }

        #[test]
        fn brightness_request_reaches_the_renderer() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            app.pending_render.push(RenderRequest::Brightness(1.5));
            app.pending_render.push(RenderRequest::ToneMap(false));

            run_frame(&mut app, &mut fake);

            assert_eq!(fake.brightnesses, vec![1.5]);
            assert_eq!(fake.tone_maps, vec![false]);
            assert!(app.pending_render.is_empty());
        }

        #[test]
        fn adopting_vsync_off_reaches_the_renderer() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            let mut off = Prefs::default();
            off.set_vsync(false);

            app.adopt_settings(off);
            run_frame(&mut app, &mut fake);

            assert_eq!(fake.vsyncs, vec![false]);
            assert!(app.pending_render.is_empty());
        }

        #[test]
        fn adopting_vsync_back_on_reaches_the_renderer_after_off() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();
            let mut off = Prefs::default();
            off.set_vsync(false);

            app.adopt_settings(off);
            app.adopt_settings(Prefs::default());
            run_frame(&mut app, &mut fake);

            assert_eq!(fake.vsyncs, vec![false, true]);
        }

        #[test]
        fn adopting_unchanged_vsync_does_not_touch_the_renderer() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();

            app.adopt_settings(Prefs::default());
            run_frame(&mut app, &mut fake);

            assert!(fake.vsyncs.is_empty());
        }

        /// The cap `frame` hands the loop after the game adopts `prefs`; fails the test
        /// if `frame` hands it none at all.
        fn cap_handed_to_the_loop(prefs: Prefs) -> Option<std::num::NonZeroU32> {
            let mut app = playing_app();
            app.adopt_settings(prefs);
            let clock = GameClock::default();
            let mut ctx = FrameContext::new(tick(), &clock, None, None);

            Game::frame(&mut app, &mut ctx, 0.5);

            ctx.requested_frame_cap()
                .expect("frame hands the loop a cap request")
        }

        #[test]
        fn vsync_off_hands_the_default_cap_to_the_loop() {
            let mut prefs = Prefs::default();
            prefs.set_vsync(false);

            let cap = cap_handed_to_the_loop(prefs);

            assert_eq!(cap, std::num::NonZeroU32::new(300));
        }

        #[test]
        fn vsync_off_hands_the_configured_cap_to_the_loop() {
            let mut prefs = Prefs::default();
            prefs.set_vsync(false);
            prefs.set_fps_max(144);

            let cap = cap_handed_to_the_loop(prefs);

            assert_eq!(cap, std::num::NonZeroU32::new(144));
        }

        #[test]
        fn zero_cap_hands_no_cap_to_the_loop() {
            let mut prefs = Prefs::default();
            prefs.set_vsync(false);
            prefs.set_fps_max(0);

            let cap = cap_handed_to_the_loop(prefs);

            assert_eq!(cap, None);
        }

        #[test]
        fn vsync_on_hands_no_cap_to_the_loop() {
            let mut prefs = Prefs::default();
            prefs.set_fps_max(60);

            let cap = cap_handed_to_the_loop(prefs);

            assert_eq!(cap, None);
        }

        #[test]
        fn frame_after_vsync_is_turned_back_on_lifts_the_cap() {
            let mut app = playing_app();
            let mut off = Prefs::default();
            off.set_vsync(false);
            app.adopt_settings(off);
            app.adopt_settings(Prefs::default());
            let clock = GameClock::default();
            let mut ctx = FrameContext::new(tick(), &clock, None, None);

            Game::frame(&mut app, &mut ctx, 0.5);

            assert_eq!(ctx.requested_frame_cap(), Some(None));
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
        fn paused_game_does_not_advance_the_autosave_schedule() {
            for state in [GameState::Paused, GameState::Menu, GameState::ConsoleOpen] {
                let mut app = playing_app();
                app.autosave_interval = Duration::from_secs(1);
                app.restart_autosave();
                app.autosave.mark_edited();
                app.game_state = state;
                let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

                sim.step(&mut app, 2 * TICK_HZ);
                assert!(
                    !app.autosave.due(),
                    "{state:?}: two intervals of standing still came due"
                );
                app.game_state = GameState::Playing;
                sim.step(&mut app, TICK_HZ / 2);
                assert!(
                    !app.autosave.due(),
                    "{state:?}: half an interval of play came due, so the paused ticks counted"
                );
                sim.step(&mut app, TICK_HZ);

                assert!(
                    app.autosave.due(),
                    "{state:?}: a full interval of play never came due"
                );
            }
        }

        fn push_pickup(app: &mut App) {
            app.pawn
                .pickups
                .push(flagstone_game::inventory::ResourceYield {
                    resource_id: 7,
                    amount: 1,
                });
        }

        #[test]
        fn playing_ages_the_player_pawns_pickups() {
            use flagstone_game::pickups::PICKUP_POPUP_TICKS;
            let mut app = playing_app();
            push_pickup(&mut app);
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

            sim.step(&mut app, PICKUP_POPUP_TICKS - 1);
            assert_eq!(app.pawn.pickups.entries().len(), 1, "expired a tick early");
            sim.step(&mut app, 1);

            assert!(app.pawn.pickups.entries().is_empty());
        }

        #[test]
        fn paused_game_does_not_age_the_player_pawns_pickups() {
            use flagstone_game::pickups::PICKUP_POPUP_TICKS;
            for state in [GameState::Paused, GameState::Menu, GameState::ConsoleOpen] {
                let mut app = playing_app();
                push_pickup(&mut app);
                app.game_state = state;
                let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ));

                sim.step(&mut app, PICKUP_POPUP_TICKS);

                let ticks_left: Vec<u32> = app
                    .pawn
                    .pickups
                    .entries()
                    .iter()
                    .map(|e| e.ticks_left)
                    .collect();
                assert_eq!(ticks_left, [PICKUP_POPUP_TICKS], "{state:?}");
            }
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
        fn playing_frame_uploads_the_sky_ambient_strength_by_day() {
            let mut app = playing_app();
            let mut fake = FakeRenderer::default();

            run_frame_at(&mut app, &mut fake, 12.0);

            assert_eq!(fake.lightings[0].ambient, [0.0, 0.0, 0.0, 0.15]);
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

    mod pad_release {
        use super::*;
        use moho_input::pad::PadButton;

        fn app_holding_pad_jump() -> App {
            let mut app = playing_app();
            app.input.actions.pad_button(PadButton::South, true);
            assert!(app.input.actions.end_tick().held(StrategyAction::Jump));
            app
        }

        #[test]
        fn pad_actions_release_when_play_stops() {
            let mut app = app_holding_pad_jump();

            app.game_state = GameState::Menu;
            app.poll_gamepads();
            app.game_state = GameState::Playing;
            let frame = app.input.actions.end_tick();

            assert!(!frame.held(StrategyAction::Jump));
            assert!(!frame.pressed(StrategyAction::Jump));
        }

        #[test]
        fn pad_actions_release_while_unfocused() {
            let mut app = app_holding_pad_jump();
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);

            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::Focused(false)),
            );
            app.poll_gamepads();
            let frame = app.input.actions.end_tick();

            assert!(!frame.held(StrategyAction::Jump));
            assert!(!frame.pressed(StrategyAction::Jump));
        }
    }

    mod play_stop_release {
        use super::*;

        fn app_holding_forward() -> (App, moho_input::key::Key) {
            let mut app = playing_app();
            let key = default_key(&app, StrategyAction::MoveForward);
            hold(&mut app, StrategyAction::MoveForward);
            assert!(
                app.input
                    .actions
                    .end_tick()
                    .held(StrategyAction::MoveForward)
            );
            assert_eq!(app.game_state, GameState::Playing);
            (app, key)
        }

        fn assert_forward_not_held_on_resume(app: &mut App) {
            assert_eq!(app.game_state, GameState::Playing);
            let frame = app.input.actions.end_tick();
            assert!(!frame.held(StrategyAction::MoveForward));
            assert!(!frame.pressed(StrategyAction::MoveForward));
            let command = Game::command(app);
            assert_eq!(command.input.forward, 0.0);
        }

        #[test]
        fn forward_held_when_the_menu_opens_is_not_held_on_resume() {
            let (mut app, _key) = app_holding_forward();

            app.show_menu();
            assert_eq!(app.game_state, GameState::Menu);
            app.hide_menu();

            assert_forward_not_held_on_resume(&mut app);
        }

        #[test]
        fn forward_held_when_the_console_opens_is_not_held_on_resume() {
            let (mut app, _key) = app_holding_forward();

            app.enter_console();
            assert_eq!(app.game_state, GameState::ConsoleOpen);
            app.exit_console();

            assert_forward_not_held_on_resume(&mut app);
        }

        #[test]
        fn key_still_down_on_resume_acts_only_after_it_is_pressed_again() {
            let (mut app, key) = app_holding_forward();

            app.show_menu();
            assert_eq!(app.game_state, GameState::Menu);
            app.hide_menu();
            assert_eq!(app.game_state, GameState::Playing);
            let before = app.input.actions.end_tick();
            assert!(!before.held(StrategyAction::MoveForward));
            app.input.actions.key(key, true);
            let after = app.input.actions.end_tick();

            assert!(after.held(StrategyAction::MoveForward));
            assert!(after.pressed(StrategyAction::MoveForward));
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
            frame_without_renderer(&mut app);
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
            frame_without_renderer(&mut app);
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

            frame_without_renderer(&mut app);

            assert_eq!(app.input.actions.end_tick().look(), (0.0, 0.0));
        }

        #[test]
        fn positive_mouse_motion_turns_first_person_yaw_and_pitch_negative() {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app.simulation
                .set_position_yaw_pitch(glam::Vec3::new(100.0, 100.0, 100.0), 0.0, 0.0);
            app.input.actions.mouse_motion(5.0, 5.0);

            frame_without_renderer(&mut app);
            let cmd = Game::command(&mut app);
            tick_app(&mut app, &cmd);

            let (yaw, pitch) = app.simulation.yaw_pitch();
            assert!(yaw < 0.0, "yaw = {yaw}");
            assert!(pitch < 0.0, "pitch = {pitch}");
        }
    }

    mod view_turn {
        use super::*;
        use flagstone_game::controller::controller_to_camera;
        use glam::{Mat4, Vec3};
        use moho_input::action_map::LookSettings;
        use moho_input::pad::{Stick, StickAxis};
        use moho_physics::{PITCH_LIMIT, first_person_view};
        use winit::event::DeviceEvent;

        /// How the first-person view is drawn: from the character controller or the free camera.
        #[derive(Clone, Copy, Debug)]
        enum Rig {
            Walking,
            FreeCam,
        }

        const RIGS: [Rig; 2] = [Rig::Walking, Rig::FreeCam];
        /// The `alpha` `run_frame` draws with.
        const ALPHA: f32 = 0.5;

        fn rig_app(rig: Rig) -> App {
            let mut app = match rig {
                Rig::Walking => standing_app(),
                Rig::FreeCam => {
                    let mut app = playing_app();
                    app.simulation.set_camera_mode(CameraMode::FirstPerson);
                    app.physics.noclip = true;
                    app
                }
            };
            HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ)).step(&mut app, 1);
            app
        }

        fn mouse(app: &mut App, dx: f64, dy: f64) {
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);
            Game::event(
                app,
                &mut ctx,
                moho_app::Event::Device(DeviceEvent::MouseMotion { delta: (dx, dy) }),
            );
        }

        fn drawn(fake: &FakeRenderer) -> Mat4 {
            fake.cameras.last().expect("a frame began").0
        }

        fn forward_of(view: Mat4) -> Vec3 {
            -view.transpose().col(2).truncate()
        }

        fn yaw_of(view: Mat4) -> f32 {
            let forward = forward_of(view);
            forward.x.atan2(forward.z)
        }

        fn pitch_of(view: Mat4) -> f32 {
            forward_of(view).y.asin()
        }

        /// The character's yaw and pitch as the simulation holds them.
        fn sim_look(app: &App, rig: Rig) -> (f32, f32) {
            match rig {
                Rig::Walking => {
                    let state = walker_state(app);
                    (state.yaw, state.pitch)
                }
                Rig::FreeCam => app.simulation.yaw_pitch(),
            }
        }

        /// The view a frame must draw: the simulation turned by the look shown but not yet ticked.
        fn expected_view(app: &App, rig: Rig) -> Mat4 {
            let (look_x, look_y) = app.input.actions.pending_look();
            match rig {
                Rig::Walking => {
                    let (previous, current) = app.physics.player_states().expect("a walker");
                    first_person_view(&previous, &current, ALPHA, (-look_x, -look_y)).view_matrix()
                }
                Rig::FreeCam => {
                    controller_to_camera(&app.simulation.player_controller.turned(-look_x, -look_y))
                        .0
                }
            }
        }

        #[test]
        fn mouse_motion_turns_the_next_frame_before_any_tick() {
            for rig in RIGS {
                let mut app = rig_app(rig);
                let mut fake = FakeRenderer::default();
                let before_look = sim_look(&app, rig);
                run_frame(&mut app, &mut fake);
                let before = drawn(&fake);

                mouse(&mut app, 400.0, 200.0);
                run_frame(&mut app, &mut fake);
                let after = drawn(&fake);

                assert!(
                    forward_of(after).distance(forward_of(before)) > 1e-3,
                    "{rig:?}: the frame did not turn"
                );
                assert!(
                    yaw_of(after) < yaw_of(before),
                    "{rig:?}: right is negative yaw"
                );
                assert!(
                    pitch_of(after) < pitch_of(before),
                    "{rig:?}: down is negative pitch"
                );
                assert_eq!(sim_look(&app, rig), before_look, "{rig:?}: no tick ran");
                assert_eq!(after, expected_view(&app, rig), "{rig:?}");
            }
        }

        #[test]
        fn tick_lands_on_the_last_frames_view() {
            const MOTION: [(f64, f64); 3] = [(120.0, 40.0), (-30.0, 80.0), (200.0, -150.0)];

            for rig in RIGS {
                for smoothing in [false, true] {
                    let mut app = rig_app(rig);
                    app.input.actions.set_look(LookSettings {
                        look_per_count: 0.002,
                        smoothing,
                        invert_y: false,
                        acceleration: 0.0,
                    });
                    let mut fake = FakeRenderer::default();
                    let (yaw_before, pitch_before) = sim_look(&app, rig);
                    run_frame(&mut app, &mut fake);
                    let level = drawn(&fake);

                    for (dx, dy) in MOTION {
                        mouse(&mut app, dx, dy);
                        run_frame(&mut app, &mut fake);
                    }
                    let last_frame = drawn(&fake);
                    assert_ne!(last_frame, level, "{rig:?}: the frames turned the view");
                    let (look_x, look_y) = app.input.actions.pending_look();
                    let command = Game::command(&mut app);
                    tick_app(&mut app, &command);

                    let case = format!("{rig:?}, smoothing {smoothing}");
                    let yaw = yaw_before + (-look_x);
                    let pitch = (pitch_before + (-look_y)).clamp(-PITCH_LIMIT, PITCH_LIMIT);
                    assert_ne!(yaw, yaw_before, "{case}: the frames turned");
                    assert_eq!(sim_look(&app, rig), (yaw, pitch), "{case}");
                    assert_eq!(app.input.actions.pending_look(), (0.0, 0.0), "{case}");
                    if let Rig::FreeCam = rig {
                        assert_eq!(app.camera.0, last_frame, "{case}: the tick camera");
                    }
                }
            }
        }

        #[test]
        fn drawn_view_is_the_simulation_turned_by_the_pending_look() {
            const MOTION: [(f64, f64); 3] = [(120.0, 40.0), (-30.0, 80.0), (200.0, -150.0)];

            for rig in RIGS {
                for smoothing in [false, true] {
                    let mut app = rig_app(rig);
                    app.input.actions.set_look(LookSettings {
                        look_per_count: 0.002,
                        smoothing,
                        invert_y: false,
                        acceleration: 0.0,
                    });
                    let mut fake = FakeRenderer::default();
                    run_frame(&mut app, &mut fake);
                    let mut previous = drawn(&fake);

                    for (dx, dy) in MOTION {
                        mouse(&mut app, dx, dy);
                        run_frame(&mut app, &mut fake);
                        assert_ne!(drawn(&fake), previous, "{rig:?}: the frame turned");
                        previous = drawn(&fake);

                        assert_eq!(
                            drawn(&fake),
                            expected_view(&app, rig),
                            "{rig:?}, smoothing {smoothing}"
                        );
                    }
                }
            }
        }

        #[test]
        fn pitch_held_at_the_limit_on_screen_and_after_the_tick() {
            for rig in RIGS {
                for (direction, limit) in [("up", PITCH_LIMIT), ("down", -PITCH_LIMIT)] {
                    let mut app = rig_app(rig);
                    let mut fake = FakeRenderer::default();
                    // Mouse y is down-positive.
                    let toward = -f64::from(limit.signum());

                    for (round, count) in [100_000.0_f64, 5_000.0].into_iter().enumerate() {
                        mouse(&mut app, 0.0, toward * count);
                        run_frame(&mut app, &mut fake);
                        let height = forward_of(drawn(&fake)).y;
                        let command = Game::command(&mut app);
                        tick_app(&mut app, &command);

                        assert!(
                            (height - limit.sin()).abs() < 1e-5,
                            "{rig:?} {direction} round {round}: view at {height}"
                        );
                        assert_eq!(
                            sim_look(&app, rig).1,
                            limit,
                            "{rig:?} {direction} round {round}"
                        );
                    }
                }
            }
        }

        #[test]
        fn stick_turns_each_frame_between_ticks() {
            for rig in RIGS {
                let mut app = rig_app(rig);
                let mut fake = FakeRenderer::default();
                let before_look = sim_look(&app, rig);
                run_frame(&mut app, &mut fake);
                let mut yaws = vec![yaw_of(drawn(&fake))];

                app.input
                    .actions
                    .pad_axis(Stick::RightStick, StickAxis::X, 1.0);
                for _ in 0..4 {
                    run_frame(&mut app, &mut fake);
                    yaws.push(yaw_of(drawn(&fake)));
                }

                assert!(
                    yaws.windows(2).all(|pair| pair[1] < pair[0]),
                    "{rig:?}: yaw should fall (turn right) every frame: {yaws:?}"
                );
                assert_eq!(sim_look(&app, rig), before_look, "{rig:?}: no tick ran");
            }
        }

        #[test]
        fn stick_turn_follows_the_frame_time() {
            const LOOK_PER_COUNT: f32 = 0.002;

            for rig in RIGS {
                let pending_after = |frame_dt: Duration| {
                    let mut app = rig_app(rig);
                    app.input.actions.set_look(LookSettings {
                        look_per_count: LOOK_PER_COUNT,
                        smoothing: false,
                        invert_y: false,
                        acceleration: 0.0,
                    });
                    app.input
                        .actions
                        .pad_axis(Stick::RightStick, StickAxis::X, 1.0);
                    let clock = GameClock::default();
                    let mut fake = FakeRenderer::default();
                    let mut ctx = FrameContext::new(tick(), &clock, Some(&mut fake), None);
                    ctx.frame_dt = frame_dt;
                    Game::frame(&mut app, &mut ctx, ALPHA);
                    app.input.actions.pending_look().0
                };

                let full = pending_after(tick());
                let half = pending_after(tick() / 2);

                let by_hand = moho_input::action_map::PAD_LOOK_PER_SECOND
                    * tick().as_secs_f32()
                    * LOOK_PER_COUNT;
                assert!(
                    (full - by_hand).abs() < 1e-5,
                    "{rig:?}: {full} vs {by_hand}"
                );
                assert_eq!(half * 2.0, full, "{rig:?}: half the time, half the turn");
            }
        }

        fn focus(app: &mut App, focused: bool) {
            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);
            Game::event(
                app,
                &mut ctx,
                moho_app::Event::Window(WindowEvent::Focused(focused)),
            );
        }

        /// Why the view must hold still.
        #[derive(Clone, Copy, Debug)]
        enum Stop {
            Paused,
            Menu,
            Console,
            Unfocused,
        }

        const STOPS: [Stop; 4] = [Stop::Paused, Stop::Menu, Stop::Console, Stop::Unfocused];

        impl Stop {
            /// Enters the stop through the game's own transition.
            fn enter(self, app: &mut App) {
                match self {
                    Stop::Paused => app.escape_pressed(),
                    Stop::Menu => app.show_menu(),
                    Stop::Console => app.enter_console(),
                    Stop::Unfocused => focus(app, false),
                }
            }

            /// Leaves the stop through the matching transition. Resuming from pause uses
            /// `toggle_pause`, which Escape calls too, because Escape resumes only from the
            /// home screen.
            fn leave(self, app: &mut App) {
                match self {
                    Stop::Paused => app.toggle_pause(),
                    Stop::Menu => app.hide_menu(),
                    Stop::Console => app.exit_console(),
                    Stop::Unfocused => focus(app, true),
                }
            }
        }

        #[test]
        fn paused_or_unfocused_frames_do_not_turn() {
            for rig in RIGS {
                for stop in STOPS {
                    let mut app = rig_app(rig);
                    let mut fake = FakeRenderer::default();
                    run_frame(&mut app, &mut fake);
                    let level = forward_of(drawn(&fake));
                    mouse(&mut app, 300.0, 150.0);
                    run_frame(&mut app, &mut fake);
                    let shown_view = drawn(&fake);
                    let shown = forward_of(shown_view);
                    assert!(
                        shown.distance(level) > 1e-3,
                        "{rig:?} {stop:?}: look is live first"
                    );
                    let (look_x, look_y) = app.input.actions.pending_look();
                    let (yaw_before, pitch_before) = sim_look(&app, rig);

                    stop.enter(&mut app);
                    assert!(
                        app.game_state != GameState::Playing || !app.input.focused,
                        "{rig:?} {stop:?}: stopped"
                    );
                    let mut views = Vec::new();
                    for _ in 0..3 {
                        if !matches!(stop, Stop::Unfocused) {
                            // A paused loop asks for the command on every frame.
                            Game::command(&mut app);
                        }
                        mouse(&mut app, 500.0, 500.0);
                        run_frame(&mut app, &mut fake);
                        views.push(forward_of(drawn(&fake)));
                    }

                    for view in views {
                        assert!(
                            view.distance(shown) < 1e-4,
                            "{rig:?} {stop:?}: the view moved from {shown:?} to {view:?}"
                        );
                    }
                    assert_eq!(
                        sim_look(&app, rig),
                        (yaw_before, pitch_before),
                        "{rig:?} {stop:?}"
                    );

                    stop.leave(&mut app);
                    assert!(
                        app.game_state == GameState::Playing && app.input.focused,
                        "{rig:?} {stop:?}: play resumed"
                    );
                    run_frame(&mut app, &mut fake);
                    assert_eq!(
                        drawn(&fake),
                        shown_view,
                        "{rig:?} {stop:?}: the first frame after resuming still draws the shown view"
                    );
                    let command = Game::command(&mut app);
                    tick_app(&mut app, &command);

                    assert_eq!(
                        sim_look(&app, rig),
                        (
                            yaw_before + (-look_x),
                            (pitch_before + (-look_y)).clamp(-PITCH_LIMIT, PITCH_LIMIT)
                        ),
                        "{rig:?} {stop:?}: the tick lands on the view that was shown"
                    );
                }
            }
        }

        fn frame_at(app: &mut App, fake: &mut FakeRenderer, alpha: f32) -> (Mat4, Vec3) {
            let clock = GameClock::default();
            let mut ctx = FrameContext::new(tick(), &clock, Some(&mut *fake), None);
            Game::frame(app, &mut ctx, alpha);
            let camera = fake.cameras.last().expect("a frame began");
            (camera.0, camera.2)
        }

        #[test]
        fn paused_walking_view_holds_still() {
            for stop in [Stop::Paused, Stop::Menu, Stop::Console] {
                let mut app = walking_app();
                HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ)).step(&mut app, 3);
                let (previous, current) = app.physics.player_states().expect("a walker");
                assert!(
                    previous.position.distance(current.position) > 1e-3,
                    "the walk is moving"
                );
                let mut fake = FakeRenderer::default();
                mouse(&mut app, 300.0, 150.0);
                frame_at(&mut app, &mut fake, 0.5);
                let pending = app.input.actions.pending_look();
                assert_ne!(pending, (0.0, 0.0), "{stop:?}: look is live first");

                stop.enter(&mut app);
                let (yaw_delta, pitch_delta) = look_to_turn(pending);
                let (expected_view, _, expected_eye) = controller_to_camera(
                    &app.simulation
                        .player_controller
                        .turned(yaw_delta, pitch_delta),
                );
                let mut drawn_views = Vec::new();
                for alpha in [0.0, 0.3, 0.7, 1.0] {
                    // The strategy loop keeps ticking while stopped.
                    let command = Game::command(&mut app);
                    tick_app(&mut app, &command);
                    drawn_views.push((alpha, frame_at(&mut app, &mut fake, alpha)));
                }

                assert_eq!(
                    app.physics.player_states(),
                    Some((previous, current)),
                    "{stop:?}: the tick states stay frozen"
                );
                for (alpha, (view, eye)) in drawn_views {
                    assert_eq!(eye, expected_eye, "{stop:?} at alpha {alpha}: the eye");
                    assert_eq!(view, expected_view, "{stop:?} at alpha {alpha}: the view");
                }
            }
        }

        #[test]
        fn unfocused_device_motion_does_not_turn_after_refocus() {
            for rig in RIGS {
                let mut app = rig_app(rig);
                let mut fake = FakeRenderer::default();
                run_frame(&mut app, &mut fake);
                let level = forward_of(drawn(&fake));
                focus(&mut app, false);

                mouse(&mut app, 400.0, 200.0);
                focus(&mut app, true);
                run_frame(&mut app, &mut fake);
                let command = Game::command(&mut app);

                assert!(
                    forward_of(drawn(&fake)).distance(level) < 1e-6,
                    "{rig:?}: the view turned"
                );
                assert_eq!(command.input.yaw_delta, 0.0, "{rig:?}");
                assert_eq!(command.input.pitch_delta, 0.0, "{rig:?}");
            }
        }

        #[test]
        fn isometric_frames_do_not_turn_the_view() {
            let mut app = rig_app(Rig::FreeCam);
            app.simulation.set_camera_mode(CameraMode::Isometric);
            let mut fake = FakeRenderer::default();

            app.input.actions.mouse_motion(400.0, 200.0);
            run_frame(&mut app, &mut fake);

            let command = Game::command(&mut app);

            assert_eq!(fake.cameras.last().copied(), Some(app.camera));
            assert_eq!(command.input.yaw_delta, 0.0);
            assert_eq!(command.input.pitch_delta, 0.0);
        }
    }

    mod pointer_select {
        use super::*;
        use glam::{UVec2, Vec3};
        use moho_input::key::MouseButton;

        pub(super) const VIEWPORT: UVec2 = UVec2::new(1280, 720);

        /// A flat floor of smooth blocks seen from above and behind, in `mode`.
        pub(super) fn app_over_flat_ground(mode: CameraMode) -> App {
            let mut app = playing_app();
            app.simulation.set_camera_mode(mode);
            let grid = app
                .light_system
                .as_mut()
                .expect("App starts with a light system")
                .grid_mut();
            for x in 2..14 {
                for z in 2..14 {
                    for y in 2..=3 {
                        grid.mutator()
                            .place(flagstone_voxel::BlockPos::new(x, y, z), 1, None);
                    }
                }
            }
            let eye = Vec3::new(8.0, 14.0, 20.0);
            let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(8.0, 3.0, 8.0), Vec3::Y);
            let proj = glam::camera::rh::proj::directx::perspective(
                std::f32::consts::FRAC_PI_3,
                16.0 / 9.0,
                0.1,
                500.0,
            );
            app.camera = (view, proj, eye);
            app.viewport = VIEWPORT;
            app
        }

        /// The ground surface point under world `(x, z)`, found by a straight-down cast.
        pub(super) fn ground_at(app: &App, x: f32, z: f32) -> Vec3 {
            let grid = app.light_system.as_ref().expect("light system").grid();
            flagstone_game::raycast::raycast_surface(
                grid,
                Vec3::new(x, 30.0, z),
                Vec3::NEG_Y,
                100.0,
            )
            .expect("the floor is under the point")
            .position
        }

        pub(super) fn click_on(app: &mut App, world: Vec3) {
            let (view, proj, _) = app.camera;
            let ndc = (proj * view).project_point3(world);
            let (x, y) = (
                (ndc.x + 1.0) * 0.5 * VIEWPORT.x as f32,
                (1.0 - ndc.y) * 0.5 * VIEWPORT.y as f32,
            );
            app.input.actions.pointer_moved(x, y);
            app.input.actions.mouse_button(MouseButton::Left, true);
            app.input.actions.mouse_button(MouseButton::Left, false);
        }

        #[test]
        fn strategy_click_records_the_surface_hit() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            let ground = ground_at(&app, 6.3, 9.1);
            click_on(&mut app, ground);

            let command = Game::command(&mut app);
            tick_app(&mut app, &command);

            assert!(command.select.is_some());
            let hit = app.pointer_hit.expect("the click found the ground");
            assert!(
                hit.position.distance(ground) < 0.05,
                "hit {:?} vs ground {ground:?}",
                hit.position
            );
        }

        #[test]
        fn first_person_click_records_no_pointer_hit() {
            let mut app = app_over_flat_ground(CameraMode::FirstPerson);
            let ground = ground_at(&app, 6.3, 9.1);
            click_on(&mut app, ground);

            let command = Game::command(&mut app);
            tick_app(&mut app, &command);

            assert!(command.select.is_none());
            assert!(app.pointer_hit.is_none());
        }

        #[test]
        fn strategy_click_without_a_pointer_records_no_hit() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            app.input.actions.mouse_button(MouseButton::Left, true);

            let command = Game::command(&mut app);
            tick_app(&mut app, &command);

            assert!(command.select.is_none());
            assert!(app.pointer_hit.is_none());
        }

        #[test]
        fn strategy_click_on_the_sky_clears_the_previous_hit() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            let ground = ground_at(&app, 6.3, 9.1);
            click_on(&mut app, ground);
            let command = Game::command(&mut app);
            tick_app(&mut app, &command);
            assert!(app.pointer_hit.is_some());
            app.input.actions.pointer_moved(640.0, 2.0);
            app.input.actions.mouse_button(MouseButton::Left, true);
            app.input.actions.mouse_button(MouseButton::Left, false);

            let command = Game::command(&mut app);
            tick_app(&mut app, &command);

            assert!(command.select.is_some());
            assert!(app.pointer_hit.is_none());
        }

        #[test]
        fn tick_without_a_click_keeps_the_last_hit() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            let ground = ground_at(&app, 6.3, 9.1);
            click_on(&mut app, ground);
            let command = Game::command(&mut app);
            tick_app(&mut app, &command);

            let idle = Game::command(&mut app);
            tick_app(&mut app, &idle);

            assert!(idle.select.is_none());
            assert!(app.pointer_hit.is_some());
        }
    }

    mod mine_designation {
        use super::pointer_select::{app_over_flat_ground, click_on, ground_at};
        use super::*;
        use crate::app::input_state::Designation;
        use flagstone_ui::overlays::HudIntents;
        use glam::Vec3;
        use moho_core::events::AudioEvent;
        use moho_input::key::MouseButton;
        use moho_renderer::MARKER_INFLATE;
        use std::sync::mpsc::Receiver;

        fn designating_app() -> App {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            app.input.designation = Designation::Mine;
            app
        }

        /// The block the straight-down surface cast names under world `(x, z)`.
        fn block_under(app: &App, x: f32, z: f32) -> [i32; 3] {
            let grid = app.light_system.as_ref().expect("light system").grid();
            raycast_surface(grid, Vec3::new(x, 30.0, z), Vec3::NEG_Y, 100.0)
                .expect("the floor is under the point")
                .block_pos
                .to_array()
        }

        fn step(app: &mut App) -> StrategyCommand {
            let command = Game::command(app);
            tick_app(app, &command);
            command
        }

        fn marked(app: &App) -> Vec<[i32; 3]> {
            app.colony.marks.iter().map(|(pos, _)| pos).collect()
        }

        fn fills(app: &mut App) -> Vec<moho_render_api::MarkerGpu> {
            app.scene.markers_mut().draws(Vec3::ZERO).fills
        }

        /// Whether `fill` is the box of the unit cell at `pos`, within the renderer's inflation.
        fn is_cell(fill: &moho_render_api::MarkerGpu, pos: [i32; 3]) -> bool {
            (0..3).all(|axis| {
                let lo = pos[axis] as f32;
                (fill.min[axis] - (lo - MARKER_INFLATE)).abs() < 1e-4
                    && (fill.max[axis] - (lo + 1.0 + MARKER_INFLATE)).abs() < 1e-4
            })
        }

        fn click_the_sky(app: &mut App) {
            app.input.actions.pointer_moved(640.0, 2.0);
            app.input.actions.mouse_button(MouseButton::Left, true);
            app.input.actions.mouse_button(MouseButton::Left, false);
        }

        fn click_ground(app: &mut App) -> [i32; 3] {
            let ground = ground_at(app, 6.3, 9.1);
            click_on(app, ground);
            block_under(app, 6.3, 9.1)
        }

        fn tap(app: &mut App) {
            let key = default_key(app, StrategyAction::MineDesignation);
            app.input.actions.key(key, true);
            app.input.actions.key(key, false);
        }

        fn record_audio(app: &App) -> Receiver<AudioEvent> {
            let (tx, rx) = std::sync::mpsc::channel();
            app.event_bus.subscribe(move |event: &AudioEvent| {
                let _ = tx.send(event.clone());
            });
            rx
        }

        fn sounds(app: &App, audio: &Receiver<AudioEvent>) -> Vec<(String, f32)> {
            app.event_bus.process_deferred();
            audio
                .try_iter()
                .filter_map(|event| match event {
                    AudioEvent::PlaySound { path, volume } => Some((path, volume)),
                    _ => None,
                })
                .collect()
        }

        fn asset(name: &str) -> std::path::PathBuf {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("assets")
                .join(name)
        }

        #[test]
        fn a_click_with_the_designation_active_marks_the_hit_block() {
            let mut app = designating_app();
            let block = click_ground(&mut app);

            let command = step(&mut app);

            assert!(command.mark.is_some(), "the press forms the mark command");
            assert!(command.select.is_none(), "and not a selection");
            assert_eq!(marked(&app), [block]);
            assert!(app.pointer_hit.is_none(), "a mark click never selects");
        }

        #[test]
        fn a_mark_shows_a_marker_on_the_next_frame() {
            let mut app = designating_app();
            let block = click_ground(&mut app);
            step(&mut app);

            run_frame(&mut app, &mut FakeRenderer::default());

            let drawn = fills(&mut app);
            assert_eq!(drawn.len(), 1);
            assert!(is_cell(&drawn[0], block), "{:?} vs {block:?}", drawn[0]);
            assert!(drawn[0].color[3] > 0.0 && drawn[0].color[3] < 1.0);
        }

        #[test]
        fn a_second_click_on_a_marked_block_cancels_it_and_its_marker() {
            let mut app = designating_app();
            click_ground(&mut app);
            step(&mut app);
            run_frame(&mut app, &mut FakeRenderer::default());
            let marked_once = (marked(&app).len(), fills(&mut app).len());

            click_ground(&mut app);
            step(&mut app);
            run_frame(&mut app, &mut FakeRenderer::default());

            assert_eq!(marked_once, (1, 1));
            assert!(marked(&app).is_empty(), "the second click cancels");
            assert!(fills(&mut app).is_empty(), "and its marker goes");
        }

        #[test]
        fn a_click_on_the_sky_marks_nothing() {
            let mut app = designating_app();
            let audio = record_audio(&app);
            let block = click_ground(&mut app);
            step(&mut app);
            run_frame(&mut app, &mut FakeRenderer::default());
            let marking_cues = sounds(&app, &audio).len();
            click_the_sky(&mut app);

            let command = step(&mut app);
            run_frame(&mut app, &mut FakeRenderer::default());

            assert_eq!(marking_cues, 1, "the first click marked a block");
            assert!(command.mark.is_some(), "the click is still a mark command");
            assert_eq!(marked(&app), [block], "the sky click cancels nothing");
            assert_eq!(fills(&mut app).len(), 1, "and its marker stays");
            assert!(sounds(&app, &audio).is_empty(), "and plays no cue");
        }

        #[test]
        fn a_click_without_the_designation_selects_and_marks_nothing() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            click_ground(&mut app);

            let command = step(&mut app);
            run_frame(&mut app, &mut FakeRenderer::default());

            assert!(command.select.is_some());
            assert!(command.mark.is_none());
            assert!(app.pointer_hit.is_some(), "the click selected");
            assert!(marked(&app).is_empty());
            assert!(fills(&mut app).is_empty());
        }

        #[test]
        fn the_mark_command_is_not_formed_in_first_person() {
            let mut app = app_over_flat_ground(CameraMode::FirstPerson);
            app.input.designation = Designation::Mine;
            click_ground(&mut app);

            let command = step(&mut app);

            assert!(command.mark.is_none());
            assert!(marked(&app).is_empty());
        }

        #[test]
        fn the_designation_key_toggles_the_control_once_per_press() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            let initially = app.input.designation;

            tap(&mut app);
            step(&mut app);
            let after_first_press = app.input.designation;
            step(&mut app);
            let with_no_press = app.input.designation;
            tap(&mut app);
            step(&mut app);
            let after_second_press = app.input.designation;
            let key = default_key(&app, StrategyAction::MineDesignation);
            app.input.actions.key(key, true);
            step(&mut app);
            app.input.actions.key(key, true);
            let repeats: Vec<_> = (0..4)
                .map(|_| {
                    step(&mut app);
                    app.input.designation
                })
                .collect();

            assert_eq!(initially, Designation::None);
            assert_eq!(after_first_press, Designation::Mine);
            assert_eq!(with_no_press, Designation::Mine, "the edge is consumed");
            assert_eq!(after_second_press, Designation::None);
            assert!(
                repeats.iter().all(|&d| d == Designation::Mine),
                "a held key toggled again: {repeats:?}"
            );
        }

        #[test]
        fn the_hud_toggle_flips_the_designation() {
            let mut app = app_over_flat_ground(CameraMode::Isometric);
            let toggle = HudIntents {
                toggle_mine_designation: true,
                ..HudIntents::default()
            };

            app.apply_hud_intents(toggle);
            let on = app.input.designation;
            app.apply_hud_intents(HudIntents::default());
            let unchanged = app.input.designation;
            app.apply_hud_intents(toggle);

            assert_eq!(on, Designation::Mine);
            assert_eq!(unchanged, Designation::Mine, "no intent, no change");
            assert_eq!(app.input.designation, Designation::None);
        }

        #[test]
        fn the_mark_applies_on_the_next_tick_not_on_the_event() {
            let mut app = designating_app();

            click_ground(&mut app);
            let on_the_event = marked(&app);
            let command = Game::command(&mut app);
            let before_the_tick = marked(&app);
            tick_app(&mut app, &command);

            assert!(on_the_event.is_empty());
            assert!(
                command.mark.is_some(),
                "the press is in the sampled command"
            );
            assert!(before_the_tick.is_empty(), "sampling marks nothing");
            assert_eq!(marked(&app).len(), 1);
        }

        #[test]
        fn marking_and_cancelling_publish_their_cues() {
            let mut app = designating_app();
            let audio = record_audio(&app);

            click_ground(&mut app);
            step(&mut app);
            let marking = sounds(&app, &audio);
            click_ground(&mut app);
            step(&mut app);
            let cancelling = sounds(&app, &audio);
            click_the_sky(&mut app);
            step(&mut app);
            let missing = sounds(&app, &audio);

            assert_eq!(marking.len(), 1, "{marking:?}");
            assert_eq!(
                std::path::Path::new(&marking[0].0),
                asset("mark.wav").as_path()
            );
            assert!(marking[0].1 > 0.0, "audible: {marking:?}");
            assert_eq!(cancelling.len(), 1, "{cancelling:?}");
            assert_eq!(
                std::path::Path::new(&cancelling[0].0),
                asset("unmark.wav").as_path()
            );
            assert!(cancelling[0].1 > 0.0, "audible: {cancelling:?}");
            assert!(missing.is_empty(), "a miss is silent: {missing:?}");
        }

        #[test]
        fn mark_assets_exist() {
            for name in ["mark.wav", "unmark.wav"] {
                let path = asset(name);

                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

                assert!(bytes.starts_with(b"RIFF"), "{name} is not a WAV file");
            }
        }

        /// A first-person player facing a wall of blocks, tool in hand, with an input channel.
        fn app_facing_a_wall() -> (App, std::sync::mpsc::Sender<crate::input_event::InputEvent>) {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app.simulation.set_position_yaw_pitch(
                Vec3::new(4.5, 70.5, 6.5),
                std::f32::consts::PI,
                0.0,
            );
            app.pawn.select_slot(flagstone_game::pawn::TOOL_SLOT);
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
            let (tx, rx) = std::sync::mpsc::channel();
            app.input.unconsumed_rx = Some(rx);
            (app, tx)
        }

        #[test]
        fn mining_a_marked_block_by_hand_removes_its_marker() {
            let (mut app, input) = app_facing_a_wall();
            for x in 2..=6 {
                for y in 68..=72 {
                    for z in 1..=3 {
                        app.colony.marks.toggle([x, y, z]);
                    }
                }
            }
            run_frame(&mut app, &mut FakeRenderer::default());
            let before = fills(&mut app).len();
            input
                .send(crate::input_event::InputEvent::MineRequested)
                .expect("the receiver is alive");

            step(&mut app);
            step(&mut app);
            run_frame(&mut app, &mut FakeRenderer::default());

            let grid = app.light_system.as_ref().expect("light system").grid();
            let solid_left = marked(&app)
                .into_iter()
                .filter(|&[x, y, z]| grid.is_solid_at(flagstone_voxel::BlockPos::new(x, y, z)))
                .count();
            assert_eq!(before, 75, "every marked block shows a marker");
            assert_eq!(marked(&app).len(), 74, "the mined block's mark is gone");
            assert_eq!(solid_left, 74, "every remaining mark is still a block");
            assert_eq!(fills(&mut app).len(), 74, "and so is its marker");
        }
    }

    mod strategy_camera {
        use super::*;
        use flagstone_game::controller::CAMERA_CLEARANCE;
        use glam::{UVec2, Vec3};

        const VIEWPORT: UVec2 = UVec2::new(1280, 720);

        /// A strategy view over an empty world, the window focused and `VIEWPORT` pixels wide.
        fn strategy_app(mode: CameraMode) -> App {
            let mut app = standing_app();
            app.simulation.set_camera_mode(mode);
            app.simulation.look_at(Vec3::new(8.0, 0.0, 8.0));
            app.viewport = VIEWPORT;
            app.input.focused = true;
            app
        }

        fn tick_once(app: &mut App) -> StrategyCommand {
            let command = Game::command(app);
            tick_app(app, &command);
            command
        }

        fn pawn_state(app: &App) -> moho_physics::CharacterState {
            let handle = app.physics.player.expect("the pawn was spawned");
            app.physics
                .world
                .as_ref()
                .expect("physics world")
                .character(handle)
                .expect("the pawn exists")
        }

        #[test]
        fn strategy_pan_leaves_the_pawn_where_it_was() {
            let mut app = strategy_app(CameraMode::Isometric);
            hold(&mut app, StrategyAction::MoveForward);
            tick_once(&mut app);
            let pawn = app.simulation.position();
            let body = pawn_state(&app).position;
            let eye = app.camera.2;

            for _ in 0..60 {
                tick_once(&mut app);
            }

            assert_eq!(app.simulation.position(), pawn);
            assert!(pawn_state(&app).position.distance(body) < 1e-4);
            assert!(
                app.camera.2.distance(eye) > 3.0,
                "the eye moved from {eye} to {}",
                app.camera.2
            );
        }

        #[test]
        fn pointer_at_the_screen_edge_pans_toward_it() {
            let forward = Vec3::new(-1.0, 0.0, -1.0).normalize();
            let right = Vec3::new(1.0, 0.0, -1.0).normalize();
            let rows = [
                ("left", (0.0, 360.0), -right),
                ("right", (1279.0, 360.0), right),
                ("top", (640.0, 0.0), forward),
                ("bottom", (640.0, 719.0), -forward),
            ];

            for (name, (x, y), direction) in rows {
                let mut app = strategy_app(CameraMode::Isometric);
                let start = app.simulation.player_controller.rts_look_target;
                app.input.actions.pointer_moved(x, y);

                let command = tick_once(&mut app);

                let moved = app.simulation.player_controller.rts_look_target - start;
                let step = app.simulation.player_controller.speed * tick().as_secs_f32();
                assert!(
                    command.input.forward != 0.0 || command.input.right != 0.0,
                    "{name}: no pan in the command"
                );
                assert!(moved.length() > 0.9 * step, "{name}: moved {moved}");
                assert!(
                    moved.normalize().dot(direction) > 0.99,
                    "{name}: moved {moved}"
                );
            }
        }

        #[test]
        fn pointer_outside_the_window_does_not_pan() {
            let mut app = strategy_app(CameraMode::Isometric);
            let start = app.simulation.player_controller.rts_look_target;
            app.input.actions.pointer_moved(0.0, 0.0);
            app.input.actions.pointer_left();

            let command = tick_once(&mut app);

            assert_eq!(command.input.forward, 0.0);
            assert_eq!(command.input.right, 0.0);
            assert_eq!(app.simulation.player_controller.rts_look_target, start);
        }

        #[test]
        fn edge_pan_is_ignored_outside_focused_strategy_mode() {
            let cases = [
                (
                    "first person, focused",
                    CameraMode::FirstPerson,
                    true,
                    GameState::Playing,
                ),
                (
                    "strategy, unfocused",
                    CameraMode::Isometric,
                    false,
                    GameState::Playing,
                ),
                (
                    "strategy, focused, paused",
                    CameraMode::Isometric,
                    true,
                    GameState::Paused,
                ),
                (
                    "strategy, focused, menu",
                    CameraMode::Isometric,
                    true,
                    GameState::Menu,
                ),
            ];

            for (name, mode, focused, state) in cases {
                let mut app = strategy_app(mode);
                app.game_state = state;
                app.input.focused = focused;
                app.input.actions.pointer_moved(0.0, 0.0);

                let command = Game::command(&mut app);

                assert_eq!(command.input.forward, 0.0, "{name}");
                assert_eq!(command.input.right, 0.0, "{name}");
            }
        }

        #[test]
        fn strategy_camera_clears_a_hillside_in_the_grid() {
            let mut app = strategy_app(CameraMode::Isometric);
            app.simulation.player_controller.rts_height = 5.0;
            let grid = app
                .light_system
                .as_mut()
                .expect("App starts with a light system")
                .grid_mut();
            // A 45 degree slope rising toward +x only: the column at x has its top block at y = x.
            for x in 2..30 {
                for z in 2..30 {
                    for y in [x - 1, x] {
                        grid.mutator()
                            .place(flagstone_voxel::BlockPos::new(x, y, z), 1, None);
                    }
                }
            }
            app.simulation.look_at(Vec3::new(10.5, 11.0, 10.5));
            hold(&mut app, StrategyAction::MoveBack);

            for tick_number in 0..120 {
                tick_once(&mut app);

                let eye = app.camera.2;
                let top = app
                    .light_system
                    .as_ref()
                    .expect("light system")
                    .grid()
                    .get_height(eye.x.floor() as i32, eye.z.floor() as i32)
                    .expect("the eye is over loaded ground");
                let floor = top as f32 + 1.0 + CAMERA_CLEARANCE;
                assert!(
                    eye.y >= floor - 1e-3,
                    "tick {tick_number}: eye {eye} below {floor}"
                );
            }
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
        fn console_time_command_resets_the_loop_clock() {
            let mut app = playing_app();
            app.game_state = GameState::ConsoleOpen;
            app.event_bus
                .publish(moho_core::events::GraphicsEvent::TimeOfDayChanged {
                    time: 18.5,
                    sun_angle: 0.0,
                });
            app.event_bus.process_deferred();
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ).with_clock(noon()));

            sim.step(&mut app, 1);

            assert_eq!(sim.clock().time_of_day(), 18.5);
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

    mod pause {
        use super::*;
        use moho_core::events::UiEvent;
        use std::sync::{Arc, Mutex};
        use winit::event::DeviceEvent;

        fn noon() -> GameClock {
            GameClock::new(12.0, 600.0, 420.0)
        }

        fn attach_adapter(app: &mut App) -> Arc<Mutex<flagstone_ui::EguiAdapter>> {
            let adapter = Arc::new(Mutex::new(flagstone_ui::EguiAdapter::new(
                None,
                app.event_bus.clone(),
                app.prefs.clone(),
            )));
            app.ui_adapter = Some(adapter.clone());
            adapter
        }

        fn screen_shown(adapter: &Arc<Mutex<flagstone_ui::EguiAdapter>>) -> Option<String> {
            adapter
                .lock()
                .expect("adapter lock")
                .active_screen_name()
                .map(str::to_owned)
        }

        fn publish_ui(app: &mut App, clock: &mut GameClock, event: UiEvent) {
            app.event_bus.publish(event);
            app.event_bus.process_deferred();
            EventProcessor::new().process_ui_events(app, clock);
        }

        fn first_person_app() -> App {
            let mut app = playing_app();
            app.simulation.set_camera_mode(CameraMode::FirstPerson);
            app
        }

        /// Records every scene load the UI requests on `app`'s bus.
        fn record_loads(app: &App) -> Arc<Mutex<Vec<std::path::PathBuf>>> {
            let loads = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&loads);
            app.event_bus.subscribe(move |e: &UiEvent| {
                if let UiEvent::LoadSceneRequested { path } = e {
                    sink.lock().expect("load sink").push(path.clone());
                }
            });
            loads
        }

        #[test]
        fn escape_while_playing_pauses_in_place() {
            let mut app = first_person_app();
            let adapter = attach_adapter(&mut app);

            app.escape_pressed();

            assert_eq!(app.game_state, GameState::Paused);
            assert_eq!(screen_shown(&adapter).as_deref(), Some("pause"));
            let position = app.simulation.position();
            let look = app.simulation.yaw_pitch();
            hold(&mut app, StrategyAction::MoveForward);
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ).with_clock(noon()));
            sim.step(&mut app, 60);
            assert_eq!(app.simulation.position(), position, "the pawn moved");
            assert_eq!(app.simulation.yaw_pitch(), look, "the camera turned");
            assert_eq!(sim.clock().time_of_day(), 12.0, "game time ran");
            let mut fake = FakeRenderer::default();
            let exit = run_frame(&mut app, &mut fake);
            assert_eq!(fake.frames_begun, 1, "the world is still drawn");
            assert!(!exit);
        }

        #[test]
        fn resuming_restores_play_without_a_load() {
            type Resume = fn(&mut App, &mut GameClock);
            let rows: [(&str, Resume); 3] = [
                ("Continue", |app, clock| {
                    publish_ui(app, clock, UiEvent::ResumeRequested);
                }),
                ("Escape without a UI", |app, _| app.escape_pressed()),
                ("Escape on the pause window", |app, _| {
                    let adapter = attach_adapter(app);
                    adapter.lock().expect("adapter lock").show_menu("pause");
                    app.escape_pressed();
                }),
            ];

            for (row, resume) in rows {
                let mut app = first_person_app();
                app.game_state = GameState::Paused;
                let position = app.simulation.position();
                let look = app.simulation.yaw_pitch();
                let loads = record_loads(&app);
                let mut clock = noon();

                resume(&mut app, &mut clock);
                app.event_bus.process_deferred();

                assert_eq!(app.game_state, GameState::Playing, "row {row}");
                assert_eq!(app.simulation.position(), position, "row {row}");
                assert_eq!(app.simulation.yaw_pitch(), look, "row {row}");
                assert_eq!(clock.time_of_day(), 12.0, "row {row}");
                let loads = loads.lock().expect("load sink");
                assert!(loads.is_empty(), "row {row}: loaded {loads:?}");
            }
        }

        #[test]
        fn settings_from_pause_keep_the_game_paused() {
            let mut app = first_person_app();
            let adapter = attach_adapter(&mut app);
            let mut clock = noon();
            app.escape_pressed();
            assert_eq!(app.game_state, GameState::Paused);

            publish_ui(
                &mut app,
                &mut clock,
                UiEvent::MenuShown {
                    name: "settings".to_string(),
                },
            );
            assert_eq!(screen_shown(&adapter).as_deref(), Some("settings"));
            assert_eq!(app.game_state, GameState::Paused);
            publish_ui(
                &mut app,
                &mut clock,
                UiEvent::MenuShown {
                    name: "pause".to_string(),
                },
            );

            assert_eq!(screen_shown(&adapter).as_deref(), Some("pause"));
            assert_eq!(app.game_state, GameState::Paused);
        }

        #[test]
        fn exit_to_main_menu_saves_then_shows_the_start_menu() {
            let (mut app, _temp) = app_with_saves_dir();
            let mut clock = noon();
            let at_pause = glam::Vec3::new(3.0, 40.0, 5.0);
            app.simulation.set_position_yaw_pitch(at_pause, 0.7, 0.1);
            app.escape_pressed();
            assert_eq!(app.game_state, GameState::Paused);
            publish_ui(&mut app, &mut clock, UiEvent::MainMenuRequested);
            let mut fake = FakeRenderer::default();

            let exit = run_frame_at(&mut app, &mut fake, 17.5);

            assert!(!exit, "leaving for the menu must not quit");
            assert_eq!(app.game_state, GameState::Menu);
            assert!(!app.main_menu_requested, "the request is consumed");
            let (spec, scene_bytes, _) =
                crate::save::read_scene_and_metadata(app.saves_dir.join("scene.bin"))
                    .expect("the world is saved before leaving it");
            assert_eq!(spec.initial_time_of_day, 17.5);
            let mut entities = flagstone_game::scene::SceneEntities::default();
            let (camera, _lights) =
                flagstone_game::scene_persistence::load_from_bytes(&scene_bytes, &mut entities)
                    .expect("the save decodes");
            assert_eq!(camera, Some((at_pause, 0.7, 0.1)));
        }

        #[test]
        fn play_input_does_not_reach_the_world_while_paused() {
            let mut app = first_person_app();
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
            app.escape_pressed();
            assert_eq!(app.game_state, GameState::Paused);
            let position = app.simulation.position();
            let look = app.simulation.yaw_pitch();
            let (tx, rx) = std::sync::mpsc::channel();
            app.input.unconsumed_tx = Some(tx.clone());
            app.input.unconsumed_rx = Some(rx);

            let clock = GameClock::default();
            let mut ctx = EventContext::new(&clock, None, None);
            Game::event(
                &mut app,
                &mut ctx,
                moho_app::Event::Device(DeviceEvent::MouseMotion {
                    delta: (400.0, 200.0),
                }),
            );
            hold(&mut app, StrategyAction::MoveForward);
            tx.send(crate::input_event::InputEvent::MineRequested)
                .expect("channel open");
            tx.send(crate::input_event::InputEvent::MouseWheel { delta_y: 3.0 })
                .expect("channel open");
            EventProcessor::new().process_input_events(&mut app);
            let command = Game::command(&mut app);
            tick_app(&mut app, &command);

            assert_eq!(command.input.yaw_delta, 0.0);
            assert_eq!(command.input.pitch_delta, 0.0);
            assert_eq!(command.input.forward, 0.0);
            assert_eq!(app.simulation.position(), position, "the pawn moved");
            assert_eq!(app.simulation.yaw_pitch(), look, "the camera turned");
            let grid = app.light_system.as_ref().expect("light system").grid();
            assert!(grid.is_solid_at(aimed), "a block was mined while paused");
        }

        #[test]
        fn key_held_at_pause_does_not_stick() {
            let mut app = playing_app();
            let key = default_key(&app, StrategyAction::MoveForward);
            hold(&mut app, StrategyAction::MoveForward);
            assert!(
                app.input
                    .actions
                    .end_tick()
                    .held(StrategyAction::MoveForward)
            );
            let mut clock = noon();

            app.escape_pressed();
            assert_eq!(app.game_state, GameState::Paused);
            app.input.actions.key(key, false);
            publish_ui(&mut app, &mut clock, UiEvent::ResumeRequested);

            assert_eq!(app.game_state, GameState::Playing);
            let frame = app.input.actions.end_tick();
            assert!(!frame.held(StrategyAction::MoveForward));
            assert!(!frame.pressed(StrategyAction::MoveForward));
            assert_eq!(Game::command(&mut app).input.forward, 0.0);
        }

        #[test]
        fn escape_on_settings_from_pause_changes_nothing() {
            let mut app = first_person_app();
            let adapter = attach_adapter(&mut app);
            let mut clock = noon();
            app.escape_pressed();
            assert_eq!(app.game_state, GameState::Paused);
            publish_ui(
                &mut app,
                &mut clock,
                UiEvent::MenuShown {
                    name: "settings".to_string(),
                },
            );
            assert_eq!(screen_shown(&adapter).as_deref(), Some("settings"));

            app.escape_pressed();

            assert_eq!(app.game_state, GameState::Paused);
            assert_eq!(screen_shown(&adapter).as_deref(), Some("settings"));
        }

        /// egui swallows Escape once a pause-window button has keyboard focus (Tab),
        /// so an Escape that pauses or resumes must be taken before the UI sees it.
        #[test]
        fn escape_skips_the_ui_only_where_it_pauses_or_resumes() {
            let rows: [(&str, GameState, Option<&str>, bool); 5] = [
                ("playing", GameState::Playing, None, true),
                ("pause window", GameState::Paused, Some("pause"), true),
                (
                    "settings from pause",
                    GameState::Paused,
                    Some("settings"),
                    false,
                ),
                ("console", GameState::ConsoleOpen, None, false),
                ("start menu", GameState::Menu, Some("start"), false),
            ];

            for (label, state, screen, expected) in rows {
                let mut app = first_person_app();
                let adapter = attach_adapter(&mut app);
                app.game_state = state;
                if let Some(screen) = screen {
                    adapter.lock().expect("adapter lock").show_menu(screen);
                }

                assert_eq!(app.escape_skips_the_ui(), expected, "row {label}");
            }
        }
    }

    mod console {
        use super::*;
        use flagstone_ui::console_commands::StrategyConsoleCommand;
        use moho_input::key::Key;
        use moho_ui_shell::{EngineCommand, Routing};
        use std::collections::BTreeMap;
        use std::sync::{Arc, Mutex};

        type SharedAdapter = Arc<Mutex<flagstone_ui::EguiAdapter>>;

        fn noon() -> GameClock {
            GameClock::new(12.0, 600.0, 420.0)
        }

        /// A playing app whose UI adapter is built from the app's own prefs.
        fn app_with_console() -> (App, SharedAdapter) {
            let mut app = playing_app();
            let adapter = Arc::new(Mutex::new(flagstone_ui::EguiAdapter::new(
                None,
                app.event_bus.clone(),
                app.prefs.clone(),
            )));
            app.ui_adapter = Some(adapter.clone());
            (app, adapter)
        }

        fn press(app: &mut App, key: Key) -> Routing {
            app.route_console_input(Input::Key(key), true, false)
        }

        fn type_line(adapter: &SharedAdapter, line: &str) {
            adapter
                .lock()
                .expect("adapter lock")
                .console_mut()
                .submit(line);
        }

        fn console_is_open(adapter: &SharedAdapter) -> bool {
            adapter
                .lock()
                .expect("adapter lock")
                .console_mut()
                .is_open()
        }

        #[test]
        fn console_key_opens_the_console_and_stops_game_time() {
            let (mut app, adapter) = app_with_console();
            let mut sim = HeadlessLoop::<App>::new(LoopConfig::new(TICK_HZ).with_clock(noon()));
            sim.step(&mut app, 60);
            let playing = sim.clock().time_of_day();
            assert!(playing > 12.0, "time runs while playing: {playing}");

            let routing = press(&mut app, Key::Backtick);
            let opened_at = sim.clock().time_of_day();
            sim.step(&mut app, 60);

            assert_eq!(routing, Routing::Opened);
            assert_eq!(app.game_state, GameState::ConsoleOpen);
            assert!(console_is_open(&adapter));
            assert_eq!(sim.clock().time_of_day(), opened_at, "time ran while open");
        }

        #[test]
        fn console_key_and_escape_close_the_console() {
            for closing in [Key::Backtick, Key::Escape] {
                let (mut app, adapter) = app_with_console();
                assert_eq!(press(&mut app, Key::Backtick), Routing::Opened);

                let routing = press(&mut app, closing);

                assert_eq!(routing, Routing::Closed, "{closing:?}");
                assert_eq!(app.game_state, GameState::Playing, "{closing:?}");
                assert!(!console_is_open(&adapter), "{closing:?}");
            }
        }

        #[test]
        fn console_key_is_skipped_in_menu_and_paused() {
            for state in [GameState::Menu, GameState::Paused] {
                let (mut app, adapter) = app_with_console();
                app.game_state = state;

                press(&mut app, Key::Backtick);

                assert_eq!(app.game_state, state, "{state:?}");
                assert!(!console_is_open(&adapter), "{state:?}");
            }
        }

        #[test]
        fn leaving_the_console_through_the_menu_closes_it() {
            let (mut app, adapter) = app_with_console();
            assert_eq!(press(&mut app, Key::Backtick), Routing::Opened);

            app.show_menu();
            app.hide_menu();
            let movement = press(&mut app, Key::W);
            let reopened = press(&mut app, Key::Backtick);

            assert_eq!(movement, Routing::Game, "the closed console took W");
            assert_eq!(reopened, Routing::Opened);
            assert_eq!(app.game_state, GameState::ConsoleOpen);
            assert!(console_is_open(&adapter));
        }

        #[test]
        fn a_rebound_console_key_opens_the_console() {
            let mut app = playing_app();
            app.prefs.set_bindings(BTreeMap::from([(
                "console".to_owned(),
                "Backslash".to_owned(),
            )]));
            let adapter = Arc::new(Mutex::new(flagstone_ui::EguiAdapter::new(
                None,
                app.event_bus.clone(),
                app.prefs.clone(),
            )));
            app.ui_adapter = Some(adapter.clone());

            let default_key = press(&mut app, Key::Backtick);
            let default_state = app.game_state;
            let rebound = press(&mut app, Key::Backslash);

            assert_eq!(default_key, Routing::Game);
            assert_eq!(default_state, GameState::Playing);
            assert_eq!(rebound, Routing::Opened);
            assert_eq!(app.game_state, GameState::ConsoleOpen);
            assert!(console_is_open(&adapter));
        }

        #[test]
        fn command_carries_console_commands_outside_playing() {
            let (mut app, adapter) = app_with_console();
            press(&mut app, Key::Backtick);
            type_line(&adapter, "time 18");
            type_line(&adapter, "quit");
            assert_eq!(app.game_state, GameState::ConsoleOpen);

            let command = Game::command(&mut app);
            let drained = Game::command(&mut app);

            assert_eq!(
                command.console,
                vec![StrategyConsoleCommand::SetTimeOfDay(18.0)]
            );
            assert_eq!(command.engine, vec![EngineCommand::Quit]);
            assert_eq!(command.input.forward, 0.0);
            assert!(!command.jump);
            assert!(drained.console.is_empty(), "commands are delivered once");
            assert!(drained.engine.is_empty(), "commands are delivered once");
        }

        #[test]
        fn tick_applies_the_console_commands_while_the_console_is_open() {
            let (mut app, adapter) = app_with_console();
            press(&mut app, Key::Backtick);
            type_line(&adapter, "time 18");
            type_line(&adapter, "noclip");
            type_line(&adapter, "quit");
            let command = Game::command(&mut app);
            let mut clock = noon();

            let mut ctx = TickContext {
                tick: 0,
                tick_length: tick(),
                clock: &mut clock,
            };
            Game::tick(&mut app, &mut ctx, &command);

            assert_eq!(clock.time_of_day(), 18.0);
            assert!(app.physics.noclip);
            assert!(app.exit_requested);
        }
    }
}
