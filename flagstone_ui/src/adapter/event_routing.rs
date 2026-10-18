//! Event routing and action processing for the UI adapter
//!
//! This module handles:
//! - Menu action to event bus conversion
//! - Audio event emission
//! - Console action processing
//! - Background music start/stop decisions for the main menu
use crate::GameState;
use crate::UiAudioEvent;
use crate::screens::MenuAction;
use moho_core::EventBus;

/// Process a menu action and publish corresponding events to the event bus
pub fn process_menu_action(action: &MenuAction, game_state: GameState, event_bus: &EventBus) {
    use moho_core::events::UiEvent as CoreUiEvent;

    match action {
        MenuAction::Back => {
            emit_audio_event(event_bus, UiAudioEvent::MenuNavigate);
            if let Some(home) = game_state.home_screen() {
                event_bus.publish(CoreUiEvent::MenuShown {
                    name: home.to_string(),
                });
            }
        }
        MenuAction::Resume => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::ResumeRequested);
        }
        MenuAction::ExitToMainMenu => {
            emit_audio_event(event_bus, UiAudioEvent::ButtonClick);
            event_bus.publish(CoreUiEvent::MainMenuRequested);
        }
        MenuAction::LoadScene(path) => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::LoadSceneRequested { path: path.clone() });
        }
        MenuAction::NewWorld => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            // Signal to open the new_world menu
            event_bus.publish(CoreUiEvent::MenuShown {
                name: "new_world".to_string(),
            });
        }
        MenuAction::GenerateWorld(spec) => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::NewWorldRequested {
                name: spec.name.clone(),
                seed: spec.seed,
                size: spec.size_xz,
            });
        }
        MenuAction::Exit => {
            emit_audio_event(event_bus, UiAudioEvent::ButtonClick);
            event_bus.publish(CoreUiEvent::ExitRequested);
        }
        MenuAction::ShowMenu(name) => {
            emit_audio_event(event_bus, UiAudioEvent::MenuNavigate);
            event_bus.publish(CoreUiEvent::MenuShown { name: name.clone() });
        }
        MenuAction::Close => {
            emit_audio_event(event_bus, UiAudioEvent::Cancel);
            // Note: Menu hiding is handled by adapter, not through event bus
        }
        MenuAction::SettingsSaved(prefs) => {
            emit_audio_event(event_bus, UiAudioEvent::Confirm);
            event_bus.publish(CoreUiEvent::SettingsSaved);

            // Emit window settings change so main.rs can apply them
            let mode = match prefs.window_mode() {
                crate::prefs::WindowMode::Windowed => moho_core::events::WindowMode::Windowed,
                crate::prefs::WindowMode::Fullscreen => moho_core::events::WindowMode::Fullscreen,
                crate::prefs::WindowMode::Borderless => moho_core::events::WindowMode::Borderless,
            };
            let (width, height) = prefs.window_resolution();
            event_bus.publish(CoreUiEvent::WindowSettingsChanged {
                mode,
                width,
                height,
            });
        }
        MenuAction::None => {
            // No action
        }
    }
}

/// Emit a UI audio event to the event bus
pub fn emit_audio_event(event_bus: &EventBus, audio_event: UiAudioEvent) {
    use moho_core::events::AudioEvent;

    let core_event = match audio_event {
        UiAudioEvent::ButtonClick => AudioEvent::ButtonClick,
        UiAudioEvent::MenuNavigate => AudioEvent::MenuNavigate,
        UiAudioEvent::Confirm => AudioEvent::Confirm,
        UiAudioEvent::Cancel => AudioEvent::Cancel,
        UiAudioEvent::Error => AudioEvent::Error,
    };

    event_bus.publish(core_event);
}

/// Menu background music, relative to the working directory.
pub const MENU_MUSIC_PATH: &str = "assets/audio/music/menu.ogg";

/// The music event that moves menu music from `playing` to `want`: a looped
/// [`MENU_MUSIC_PATH`] start, a stop, or `None` when they already agree.
pub fn menu_music_change(want: bool, playing: bool) -> Option<moho_core::events::AudioEvent> {
    use moho_core::events::AudioEvent;

    match (want, playing) {
        (true, false) => Some(AudioEvent::MusicStart {
            path: MENU_MUSIC_PATH.to_string(),
            volume: 1.0,
            looped: true,
        }),
        (false, true) => Some(AudioEvent::MusicStop),
        _ => None,
    }
}

/// Process console action and publish corresponding events
pub fn process_console_action(action: crate::overlays::ConsoleAction, event_bus: &EventBus) {
    use crate::overlays::ConsoleAction;

    match action {
        ConsoleAction::ShowMarker { id, min, max } => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::ShowMarker { id, min, max });
        }
        ConsoleAction::ClearMarkers(id) => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::ClearMarkers { id });
        }
        ConsoleAction::Close => {
            use moho_core::events::UiEvent;
            event_bus.publish(UiEvent::MenuHidden {
                name: "console".to_string(),
            });
        }
        ConsoleAction::Quit => {
            use moho_core::events::UiEvent;
            event_bus.publish(UiEvent::ExitRequested);
        }
        ConsoleAction::ToggleGodMode => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::ToggleGodMode { enabled: true });
        }
        ConsoleAction::ToggleNoclip => {
            use moho_core::events::DebugEvent;
            // TODO: publishes a fixed `enabled: false`, so the console can only turn noclip on; carry the current state or toggle in the handler.
            event_bus.publish(DebugEvent::ToggleCollision { enabled: false });
        }
        ConsoleAction::SetSunDirection(yaw, pitch) => {
            use moho_core::events::GraphicsEvent;
            // Convert degrees to radians
            let yaw_rad = yaw.to_radians();
            let pitch_rad = pitch.to_radians();
            event_bus.publish(GraphicsEvent::SunDirectionChanged {
                yaw: yaw_rad,
                pitch: pitch_rad,
            });
        }
        ConsoleAction::SetTimeOfDay(time) => {
            use moho_core::events::GraphicsEvent;
            // Time is now in hours (0-24) and will be set directly on the game clock
            // The sun_angle field is kept for backward compatibility but not used
            event_bus.publish(GraphicsEvent::TimeOfDayChanged {
                time,
                sun_angle: 0.0,
            });
        }
        ConsoleAction::SetDebugView(mode) => {
            use moho_core::events::GraphicsEvent;
            event_bus.publish(GraphicsEvent::DebugViewChanged { mode });
        }
        ConsoleAction::SetShadowQuality(quality) => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::SetShadowQuality { quality });
        }
        ConsoleAction::SetSsaoQuality(quality) => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::SetSsaoQuality { quality });
        }
        ConsoleAction::Spawn(entity_type, args) => {
            use moho_core::events::DebugEvent;
            event_bus.publish(DebugEvent::SpawnEntity {
                entity_type,
                args,
                position: None, // Position will be determined by raycast in the handler
            });
        }
        ConsoleAction::PlaySoundAt {
            path,
            position,
            volume,
        } => {
            use moho_core::events::AudioEvent;
            event_bus.publish(AudioEvent::PlaySoundAt {
                path,
                volume,
                position,
            });
        }
        ConsoleAction::None => {
            // No action
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flagstone_game::scene_builders::WorldSpec;
    use moho_core::events::EventBus;
    use std::sync::Arc;

    #[test]
    fn test_process_menu_action_publishes_matching_ui_event() {
        use moho_core::events::UiEvent as CoreUiEvent;

        type ExpectedEvent = fn(&CoreUiEvent) -> bool;

        let cases: Vec<(MenuAction, ExpectedEvent)> = vec![
            (
                MenuAction::LoadScene(std::path::PathBuf::from("test.bin")),
                |e| matches!(e, CoreUiEvent::LoadSceneRequested { path } if path == std::path::Path::new("test.bin")),
            ),
            (
                MenuAction::NewWorld,
                |e| matches!(e, CoreUiEvent::MenuShown { name } if name == "new_world"),
            ),
            (
                MenuAction::GenerateWorld(WorldSpec {
                    name: "Test World".to_string(),
                    seed: Some(12345),
                    size_xz: 16,
                    day_length_seconds: 1200.0,
                    night_length_seconds: 600.0,
                    initial_time_of_day: 6.0,
                }),
                |e| {
                    matches!(
                        e,
                        CoreUiEvent::NewWorldRequested {
                            seed: Some(12345),
                            size: 16,
                            ..
                        }
                    )
                },
            ),
            (MenuAction::Exit, |e| {
                matches!(e, CoreUiEvent::ExitRequested)
            }),
            (
                MenuAction::ShowMenu("settings".to_string()),
                |e| matches!(e, CoreUiEvent::MenuShown { name } if name == "settings"),
            ),
        ];

        for (action, matches_expected) in cases {
            let bus = Arc::new(EventBus::new());
            let (tx, rx) = std::sync::mpsc::channel();
            bus.subscribe(move |event: &CoreUiEvent| {
                let _ = tx.send(event.clone());
            });

            process_menu_action(&action, GameState::Menu, &bus);

            let published = rx
                .recv_timeout(std::time::Duration::from_millis(100))
                .expect("expected a UiEvent to be published");
            assert!(
                matches_expected(&published),
                "unexpected event {published:?} for action {action:?}"
            );
        }
    }

    fn record<E>(bus: &EventBus) -> std::sync::mpsc::Receiver<E>
    where
        E: moho_core::events::Event + Clone,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        bus.subscribe(move |event: &E| {
            let _ = tx.send(event.clone());
        });
        rx
    }

    #[test]
    fn back_goes_to_the_home_screen_of_the_state() {
        use moho_core::events::{AudioEvent, UiEvent};

        let rows = [(GameState::Paused, "pause"), (GameState::Menu, "start")];

        for (state, home) in rows {
            let bus = EventBus::new();
            let ui = record::<UiEvent>(&bus);
            let audio = record::<AudioEvent>(&bus);

            process_menu_action(&MenuAction::Back, state, &bus);

            let shown: Vec<_> = ui.try_iter().collect();
            assert!(
                matches!(shown.as_slice(), [UiEvent::MenuShown { name }] if name == home),
                "{state:?}: expected MenuShown {home:?}, got {shown:?}"
            );
            let sounds: Vec<_> = audio.try_iter().collect();
            assert!(
                matches!(sounds.as_slice(), [AudioEvent::MenuNavigate]),
                "{state:?}: audio {sounds:?}"
            );
        }
    }

    #[test]
    fn back_in_a_state_without_a_home_screen_publishes_no_menu() {
        use moho_core::events::UiEvent;

        for state in [GameState::Playing, GameState::ConsoleOpen] {
            let bus = EventBus::new();
            let ui = record::<UiEvent>(&bus);

            process_menu_action(&MenuAction::Back, state, &bus);

            let shown: Vec<_> = ui.try_iter().collect();
            assert!(
                shown
                    .iter()
                    .all(|e| !matches!(e, UiEvent::MenuShown { .. })),
                "{state:?}: unexpected {shown:?}"
            );
        }
    }

    #[test]
    fn resume_and_exit_to_main_menu_publish_their_events() {
        use moho_core::events::UiEvent;

        type ExpectedEvent = fn(&UiEvent) -> bool;
        let rows: [(MenuAction, ExpectedEvent); 2] = [
            (MenuAction::Resume, |e| {
                matches!(e, UiEvent::ResumeRequested)
            }),
            (MenuAction::ExitToMainMenu, |e| {
                matches!(e, UiEvent::MainMenuRequested)
            }),
        ];

        for (action, matches_expected) in rows {
            let bus = EventBus::new();
            let ui = record::<UiEvent>(&bus);

            process_menu_action(&action, GameState::Paused, &bus);

            let published: Vec<_> = ui.try_iter().collect();
            assert!(
                matches!(published.as_slice(), [e] if matches_expected(e)),
                "{action:?}: unexpected {published:?}"
            );
        }
    }

    #[test]
    fn generate_world_carries_the_typed_name() {
        use moho_core::events::UiEvent as CoreUiEvent;
        let bus = EventBus::new();
        let rx = record::<CoreUiEvent>(&bus);
        let spec = WorldSpec {
            name: "Highlands".to_string(),
            seed: Some(7),
            size_xz: 16,
            day_length_seconds: 1200.0,
            night_length_seconds: 600.0,
            initial_time_of_day: 6.0,
        };

        process_menu_action(&MenuAction::GenerateWorld(spec), GameState::Menu, &bus);

        let names: Vec<_> = rx
            .try_iter()
            .filter_map(|e| match e {
                CoreUiEvent::NewWorldRequested { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["Highlands"]);
    }

    #[test]
    fn test_emit_audio_events() {
        use moho_core::events::AudioEvent;

        type ExpectedEvent = fn(&AudioEvent) -> bool;

        let cases: Vec<(UiAudioEvent, ExpectedEvent)> = vec![
            (UiAudioEvent::ButtonClick, |e| {
                matches!(e, AudioEvent::ButtonClick)
            }),
            (UiAudioEvent::MenuNavigate, |e| {
                matches!(e, AudioEvent::MenuNavigate)
            }),
            (UiAudioEvent::Confirm, |e| matches!(e, AudioEvent::Confirm)),
            (UiAudioEvent::Cancel, |e| matches!(e, AudioEvent::Cancel)),
            (UiAudioEvent::Error, |e| matches!(e, AudioEvent::Error)),
        ];

        for (ui_event, matches_expected) in cases {
            let bus = EventBus::new();
            let rx = record::<AudioEvent>(&bus);

            emit_audio_event(&bus, ui_event.clone());

            let published: Vec<_> = rx.try_iter().collect();
            assert_eq!(published.len(), 1, "one event for {ui_event:?}");
            assert!(
                matches_expected(&published[0]),
                "unexpected {:?} for {ui_event:?}",
                published[0]
            );
        }
    }

    mod menu_music {
        use super::*;
        use moho_core::events::AudioEvent;

        #[test]
        fn starts_from_the_assets_music_path() {
            let event = menu_music_change(true, false);

            match event {
                Some(AudioEvent::MusicStart {
                    path,
                    volume,
                    looped,
                }) => {
                    assert_eq!(path, "assets/audio/music/menu.ogg");
                    assert!((volume - 1.0).abs() < f32::EPSILON, "volume {volume}");
                    assert!(looped, "menu music loops");
                }
                other => panic!("expected MusicStart, got {other:?}"),
            }
        }

        #[test]
        fn stops_when_playing_but_no_longer_wanted() {
            let event = menu_music_change(false, true);

            assert!(
                matches!(event, Some(AudioEvent::MusicStop)),
                "unexpected {event:?}"
            );
        }

        #[test]
        fn no_change_when_want_matches_playing() {
            for state in [true, false] {
                let event = menu_music_change(state, state);

                assert!(event.is_none(), "state {state}: unexpected {event:?}");
            }
        }
    }

    #[test]
    fn console_action_publishes_matching_event() {
        use crate::overlays::ConsoleAction;
        use moho_core::events::{DebugEvent, GraphicsEvent, UiEvent};
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

        #[derive(Debug)]
        enum Published {
            Ui(UiEvent),
            Debug(DebugEvent),
            Graphics(GraphicsEvent),
        }
        type ExpectedEvent = fn(&Published) -> bool;

        let cases: Vec<(ConsoleAction, ExpectedEvent)> = vec![
            (
                ConsoleAction::Close,
                |e| matches!(e, Published::Ui(UiEvent::MenuHidden { name }) if name == "console"),
            ),
            (ConsoleAction::Quit, |e| {
                matches!(e, Published::Ui(UiEvent::ExitRequested))
            }),
            (ConsoleAction::ToggleGodMode, |e| {
                matches!(
                    e,
                    Published::Debug(DebugEvent::ToggleGodMode { enabled: true })
                )
            }),
            (ConsoleAction::SetSunDirection(90.0, 45.0), |e| {
                matches!(
                    e,
                    Published::Graphics(GraphicsEvent::SunDirectionChanged { yaw, pitch })
                        if (yaw - FRAC_PI_2).abs() < 1e-6 && (pitch - FRAC_PI_4).abs() < 1e-6
                )
            }),
            (ConsoleAction::SetTimeOfDay(13.5), |e| {
                matches!(
                    e,
                    Published::Graphics(GraphicsEvent::TimeOfDayChanged { time, .. })
                        if *time == 13.5
                )
            }),
            (ConsoleAction::SetDebugView(3), |e| {
                matches!(
                    e,
                    Published::Graphics(GraphicsEvent::DebugViewChanged { mode: 3 })
                )
            }),
            (ConsoleAction::SetShadowQuality(2), |e| {
                matches!(
                    e,
                    Published::Debug(DebugEvent::SetShadowQuality { quality: 2 })
                )
            }),
            (ConsoleAction::SetSsaoQuality(4), |e| {
                matches!(
                    e,
                    Published::Debug(DebugEvent::SetSsaoQuality { quality: 4 })
                )
            }),
            (
                ConsoleAction::Spawn("tree".to_string(), Some("oak".to_string())),
                |e| {
                    matches!(
                        e,
                        Published::Debug(DebugEvent::SpawnEntity {
                            entity_type,
                            args: Some(args),
                            position: None,
                        }) if entity_type == "tree" && args == "oak"
                    )
                },
            ),
        ];

        for (action, matches_expected) in cases {
            let bus = EventBus::new();
            let (tx, rx) = std::sync::mpsc::channel();
            let t = tx.clone();
            bus.subscribe(move |e: &UiEvent| {
                let _ = t.send(Published::Ui(e.clone()));
            });
            let t = tx.clone();
            bus.subscribe(move |e: &DebugEvent| {
                let _ = t.send(Published::Debug(e.clone()));
            });
            bus.subscribe(move |e: &GraphicsEvent| {
                let _ = tx.send(Published::Graphics(e.clone()));
            });

            process_console_action(action.clone(), &bus);

            let published: Vec<_> = rx.try_iter().collect();
            assert_eq!(published.len(), 1, "events for {action:?}: {published:?}");
            assert!(
                matches_expected(&published[0]),
                "unexpected {:?} for {action:?}",
                published[0]
            );
        }
    }

    #[test]
    fn console_play_sound_at_publishes_positioned_audio_event() {
        use crate::overlays::ConsoleAction;
        use moho_core::events::AudioEvent;
        let bus = EventBus::new();
        let audio = record::<AudioEvent>(&bus);

        process_console_action(
            ConsoleAction::PlaySoundAt {
                path: "sounds/boom.wav".to_string(),
                position: glam::Vec3::new(1.0, 2.0, 3.0),
                volume: 0.4,
            },
            &bus,
        );

        let published: Vec<_> = audio.try_iter().collect();
        assert_eq!(published.len(), 1, "events: {published:?}");
        assert!(
            matches!(
                &published[0],
                AudioEvent::PlaySoundAt { path, volume, position }
                    if path == "sounds/boom.wav"
                        && *volume == 0.4
                        && *position == glam::Vec3::new(1.0, 2.0, 3.0)
            ),
            "unexpected {:?}",
            published[0]
        );
    }

    #[test]
    fn console_marker_actions_publish_debug_events() {
        use crate::overlays::ConsoleAction;
        use moho_core::events::DebugEvent;
        let min = glam::Vec3::new(1.0, 2.0, 3.0);
        let max = glam::Vec3::new(2.0, 3.0, 4.0);
        let bus = EventBus::new();
        let debug = record::<DebugEvent>(&bus);

        process_console_action(ConsoleAction::ShowMarker { id: 7, min, max }, &bus);
        process_console_action(ConsoleAction::ClearMarkers(Some(7)), &bus);
        process_console_action(ConsoleAction::ClearMarkers(None), &bus);

        let published: Vec<_> = debug.try_iter().collect();
        assert!(
            matches!(
                published.as_slice(),
                [
                    DebugEvent::ShowMarker { id: 7, min: a, max: b },
                    DebugEvent::ClearMarkers { id: Some(7) },
                    DebugEvent::ClearMarkers { id: None },
                ] if *a == min && *b == max
            ),
            "events: {published:?}"
        );
    }

    mod menu_table {
        use super::*;
        use moho_core::events::{AudioEvent, UiEvent};

        #[test]
        fn settings_saved_publishes_saved_and_window_settings() {
            let mut prefs = crate::prefs::Prefs::default();
            prefs.set_window_mode(crate::prefs::WindowMode::Fullscreen);
            prefs.set_window_resolution(1280, 720);
            let bus = EventBus::new();
            let ui = record::<UiEvent>(&bus);

            process_menu_action(&MenuAction::SettingsSaved(prefs), GameState::Menu, &bus);

            let published: Vec<_> = ui.try_iter().collect();
            assert_eq!(published.len(), 2, "events: {published:?}");
            assert!(matches!(published[0], UiEvent::SettingsSaved));
            assert!(
                matches!(
                    published[1],
                    UiEvent::WindowSettingsChanged {
                        mode: moho_core::events::WindowMode::Fullscreen,
                        width: 1280,
                        height: 720,
                    }
                ),
                "unexpected {:?}",
                published[1]
            );
        }

        #[test]
        fn close_publishes_no_ui_event_and_plays_cancel() {
            let bus = EventBus::new();
            let ui = record::<UiEvent>(&bus);
            let audio = record::<AudioEvent>(&bus);

            process_menu_action(&MenuAction::Close, GameState::Menu, &bus);

            let ui_events: Vec<_> = ui.try_iter().collect();
            assert!(ui_events.is_empty(), "events: {ui_events:?}");
            let audio_events: Vec<_> = audio.try_iter().collect();
            assert!(
                matches!(audio_events.as_slice(), [AudioEvent::Cancel]),
                "events: {audio_events:?}"
            );
        }
    }
}
