//! Applies the console's commands to the strategy game.

use flagstone_ui::console_commands::StrategyConsoleCommand;
use moho_core::events::{AudioEvent, DebugEvent, GraphicsEvent, UiEvent};
use moho_ui_shell::EngineCommand;

use crate::App;

/// Applies one strategy console command to `app` by publishing the event whose handler does the
/// work in the same tick.
pub fn apply_console_command(app: &mut App, command: &StrategyConsoleCommand) {
    match command {
        StrategyConsoleCommand::ToggleGodMode => {
            app.event_bus
                .publish(DebugEvent::ToggleGodMode { enabled: true });
        }
        StrategyConsoleCommand::ToggleNoclip => {
            // The handler sets noclip to `!enabled`, so passing the current flag flips it.
            app.event_bus.publish(DebugEvent::ToggleCollision {
                enabled: app.physics.noclip,
            });
        }
        StrategyConsoleCommand::SetTimeOfDay(time) => {
            app.event_bus.publish(GraphicsEvent::TimeOfDayChanged {
                time: *time,
                sun_angle: 0.0,
            });
        }
        StrategyConsoleCommand::SetSunDirection { yaw_deg, pitch_deg } => {
            app.event_bus.publish(GraphicsEvent::SunDirectionChanged {
                yaw: yaw_deg.to_radians(),
                pitch: pitch_deg.to_radians(),
            });
        }
        StrategyConsoleCommand::Spawn { kind, args } => {
            app.event_bus.publish(DebugEvent::SpawnEntity {
                entity_type: kind.clone(),
                args: args.clone(),
                position: None,
            });
        }
        StrategyConsoleCommand::ShowMarker {
            id,
            min,
            max,
            style,
        } => {
            app.event_bus.publish(DebugEvent::ShowMarker {
                id: *id,
                min: *min,
                max: *max,
                style: *style,
            });
        }
        StrategyConsoleCommand::ClearMarkers(id) => {
            app.event_bus.publish(DebugEvent::ClearMarkers { id: *id });
        }
        StrategyConsoleCommand::SetViewHeight(height) => {
            app.event_bus
                .publish(DebugEvent::SetViewHeight { height: *height });
        }
    }
}

/// Applies one engine console command to `app`. `Quit` goes the way closing the window does,
/// so strategy autosaves.
pub fn apply_engine_command(app: &mut App, command: &EngineCommand) {
    match command {
        EngineCommand::Quit => app.event_bus.publish(UiEvent::ExitRequested),
        EngineCommand::DebugView(mode) => {
            app.event_bus
                .publish(GraphicsEvent::DebugViewChanged { mode: *mode });
        }
        EngineCommand::ShadowQuality(quality) => {
            app.event_bus.publish(DebugEvent::SetShadowQuality {
                quality: u32::from(*quality),
            });
        }
        EngineCommand::SsaoQuality(quality) => {
            app.event_bus.publish(DebugEvent::SetSsaoQuality {
                quality: u32::from(*quality),
            });
        }
        EngineCommand::PlaySoundAt {
            path,
            position,
            volume,
        } => {
            app.event_bus.publish(AudioEvent::PlaySoundAt {
                path: path.to_string_lossy().into_owned(),
                volume: *volume,
                position: *position,
            });
        }
        other => tracing::warn!(?other, "engine console command not handled by strategy"),
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    use std::sync::mpsc::Receiver;

    use flagstone_game::TICK_HZ;
    use moho_app::{Game, GameClock, LoopConfig, TickContext};
    use moho_core::events::{AudioEvent, DebugEvent, Event, GraphicsEvent};
    use moho_core::prefs::Prefs;
    use moho_render_api::MarkerStyle;

    use super::*;
    use crate::RenderRequest;
    use crate::app::game::StrategyCommand;
    use crate::game_state::GameState;

    fn tick_with(app: &mut App, clock: &mut GameClock) {
        let mut ctx = TickContext {
            tick: 0,
            tick_length: LoopConfig::new(TICK_HZ).tick_length(),
            clock,
        };
        Game::tick(app, &mut ctx, &StrategyCommand::default());
    }

    fn tick(app: &mut App) -> GameClock {
        let mut clock = GameClock::default();
        tick_with(app, &mut clock);
        clock
    }

    fn record<E>(app: &App) -> Receiver<E>
    where
        E: Event + Clone,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        app.event_bus.subscribe(move |event: &E| {
            let _ = tx.send(event.clone());
        });
        rx
    }

    fn headless_saving_to(path: &std::path::Path) -> App {
        let mut app = App::headless();
        app.prefs_path = Some(path.to_path_buf());
        app
    }

    #[test]
    fn time_command_sets_the_clock() {
        let mut app = App::headless();
        let mut clock = GameClock::new(6.0, 600.0, 420.0);
        apply_console_command(&mut app, &StrategyConsoleCommand::SetTimeOfDay(18.0));

        tick_with(&mut app, &mut clock);

        assert_eq!(clock.time_of_day(), 18.0);
    }

    #[test]
    fn noclip_turns_on_then_off() {
        let mut app = App::headless();
        assert!(!app.physics.noclip);

        apply_console_command(&mut app, &StrategyConsoleCommand::ToggleNoclip);
        tick(&mut app);
        let after_first = app.physics.noclip;
        apply_console_command(&mut app, &StrategyConsoleCommand::ToggleNoclip);
        tick(&mut app);
        let after_second = app.physics.noclip;

        assert!(after_first, "the first noclip turns it on");
        assert!(!after_second, "the second noclip turns it off");
    }

    #[test]
    fn god_publishes_toggle_god_mode() {
        let mut app = App::headless();
        let debug = record::<DebugEvent>(&app);
        let graphics = record::<GraphicsEvent>(&app);

        apply_console_command(&mut app, &StrategyConsoleCommand::ToggleGodMode);

        let published: Vec<_> = debug.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [DebugEvent::ToggleGodMode { enabled: true }]
            ),
            "events: {published:?}"
        );
        assert_eq!(graphics.try_iter().count(), 0);
        assert!(!app.physics.noclip);
    }

    #[test]
    fn sun_publishes_direction_in_radians() {
        let mut app = App::headless();
        let graphics = record::<GraphicsEvent>(&app);

        apply_console_command(
            &mut app,
            &StrategyConsoleCommand::SetSunDirection {
                yaw_deg: 90.0,
                pitch_deg: 45.0,
            },
        );

        let published: Vec<_> = graphics.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [GraphicsEvent::SunDirectionChanged { yaw, pitch }]
                    if (yaw - FRAC_PI_2).abs() < 1e-6 && (pitch - FRAC_PI_4).abs() < 1e-6
            ),
            "events: {published:?}"
        );
    }

    #[test]
    fn spawn_sphere_adds_an_actor() {
        let mut app = App::headless();
        apply_console_command(
            &mut app,
            &StrategyConsoleCommand::Spawn {
                kind: "sphere".to_owned(),
                args: None,
            },
        );

        tick(&mut app);

        assert_eq!(app.entities.actors.spheres().len(), 1);
        assert!(app.entities.actors.cubes().is_empty());
        assert_eq!(app.physics.test_bodies.len(), 1);
    }

    #[test]
    fn spawn_publishes_the_kind_and_args_with_no_position() {
        let mut app = App::headless();
        let debug = record::<DebugEvent>(&app);

        apply_console_command(
            &mut app,
            &StrategyConsoleCommand::Spawn {
                kind: "tree".to_owned(),
                args: Some("oak".to_owned()),
            },
        );

        let published: Vec<_> = debug.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [DebugEvent::SpawnEntity { entity_type, args: Some(args), position: None }]
                    if entity_type == "tree" && args == "oak"
            ),
            "events: {published:?}"
        );
    }

    #[test]
    fn marker_commands_publish_debug_events() {
        let min = glam::Vec3::new(1.0, 2.0, 3.0);
        let max = glam::Vec3::new(2.0, 3.0, 4.0);
        let mut app = App::headless();
        let debug = record::<DebugEvent>(&app);

        apply_console_command(
            &mut app,
            &StrategyConsoleCommand::ShowMarker {
                id: 7,
                min,
                max,
                style: MarkerStyle::Outline,
            },
        );
        apply_console_command(&mut app, &StrategyConsoleCommand::ClearMarkers(Some(7)));
        apply_console_command(&mut app, &StrategyConsoleCommand::ClearMarkers(None));

        let published: Vec<_> = debug.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [
                    DebugEvent::ShowMarker { id: 7, min: a, max: b, style },
                    DebugEvent::ClearMarkers { id: Some(7) },
                    DebugEvent::ClearMarkers { id: None },
                ] if *a == min && *b == max && *style == MarkerStyle::Outline
            ),
            "events: {published:?}"
        );
    }

    #[test]
    fn view_height_command_publishes_debug_events() {
        let mut app = App::headless();
        let debug = record::<DebugEvent>(&app);

        apply_console_command(&mut app, &StrategyConsoleCommand::SetViewHeight(Some(12.5)));
        apply_console_command(&mut app, &StrategyConsoleCommand::SetViewHeight(None));

        let published: Vec<_> = debug.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [
                    DebugEvent::SetViewHeight { height: Some(h) },
                    DebugEvent::SetViewHeight { height: None },
                ] if *h == 12.5
            ),
            "events: {published:?}"
        );
    }

    #[test]
    fn view_height_command_queues_a_render_request_on_the_tick() {
        let mut app = App::headless();
        apply_console_command(&mut app, &StrategyConsoleCommand::SetViewHeight(Some(12.5)));

        tick(&mut app);

        assert!(
            matches!(
                app.pending_render.as_slice(),
                [RenderRequest::ViewHeight(Some(h))] if *h == 12.5
            ),
            "queued {} requests",
            app.pending_render.len()
        );
    }

    #[test]
    fn quit_requests_exit_through_the_ui_path() {
        let mut app = App::headless();
        apply_engine_command(&mut app, &EngineCommand::Quit);

        tick(&mut app);

        assert!(app.exit_requested);
    }

    #[test]
    fn shadow_quality_command_saves_prefs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("prefs.ini");
        let mut app = headless_saving_to(&path);
        apply_engine_command(&mut app, &EngineCommand::ShadowQuality(2));

        tick(&mut app);

        assert_eq!(app.prefs.shadow_quality(), 2);
        assert_eq!(Prefs::load_from(&path).0.shadow_quality(), 2);
    }

    #[test]
    fn ssao_quality_command_saves_prefs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("prefs.ini");
        let mut app = headless_saving_to(&path);
        apply_engine_command(&mut app, &EngineCommand::SsaoQuality(3));

        tick(&mut app);

        assert_eq!(app.prefs.ssao_quality(), 3);
        assert_eq!(Prefs::load_from(&path).0.ssao_quality(), 3);
    }

    #[test]
    fn debug_view_command_sets_the_debug_mode() {
        let mut app = App::headless();
        apply_engine_command(&mut app, &EngineCommand::DebugView(3));

        tick(&mut app);

        assert_eq!(app.debug_mode, 3);
    }

    #[test]
    fn audio_cue_command_publishes_a_positioned_sound() {
        let mut app = App::headless();
        let audio = record::<AudioEvent>(&app);

        apply_engine_command(
            &mut app,
            &EngineCommand::PlaySoundAt {
                path: "sounds/boom.wav".into(),
                position: glam::Vec3::new(1.0, 2.0, 3.0),
                volume: 0.4,
            },
        );

        let published: Vec<_> = audio.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [AudioEvent::PlaySoundAt { path, volume, position }]
                    if path == "sounds/boom.wav"
                        && *volume == 0.4
                        && *position == glam::Vec3::new(1.0, 2.0, 3.0)
            ),
            "events: {published:?}"
        );
    }

    #[test]
    fn noclip_is_off_after_a_relaunch() {
        let dir = tempfile::tempdir().expect("temp dir");
        let prefs_path = dir.path().join("prefs.ini");
        let mut app = headless_saving_to(&prefs_path);
        app.saves_dir = dir.path().join("saves");
        app.game_state = GameState::Playing;
        apply_console_command(&mut app, &StrategyConsoleCommand::ToggleNoclip);
        let clock = tick(&mut app);
        assert!(app.physics.noclip, "noclip is on before the relaunch");
        app.auto_save_on_shutdown(&clock, &[])
            .expect("the autosave is written");
        app.save_prefs();

        let relaunched = App::from_config(crate::app::config::AppConfig::from_prefs_struct(
            Prefs::load_from(&prefs_path).0,
        ));

        assert!(!relaunched.physics.noclip);
    }
}
