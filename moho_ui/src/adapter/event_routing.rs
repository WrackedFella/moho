//! Event routing and action processing for the UI adapter
//!
//! This module handles:
//! - Menu action to event bus conversion
//! - Audio event emission
//! - Console action processing
//! - Background music lifecycle for the main menu
use crate::UiAudioEvent;
use crate::screens::MenuAction;
use moho_core::EventBus;

/// Process a menu action and publish corresponding events to the event bus
pub fn process_menu_action(action: &MenuAction, event_bus: &EventBus) {
    use moho_core::events::UiEvent as CoreUiEvent;

    match action {
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
                name: "New World".to_string(),
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

/// Manage menu background music lifecycle.
///
/// Starts music when the player navigates to the start menu, stops it when
/// they leave. Silently no-ops if the music file does not exist on disk.
pub fn update_menu_music(actions: &[MenuAction], event_bus: &EventBus, music_playing: &mut bool) {
    use moho_core::events::AudioEvent;
    use std::path::Path;

    const MENU_MUSIC_PATH: &str = "audio/music/menu.ogg";

    for action in actions {
        match action {
            // Entering the start menu — start music
            MenuAction::ShowMenu(name)
                if name == "start" && !*music_playing && Path::new(MENU_MUSIC_PATH).exists() =>
            {
                event_bus.publish(AudioEvent::MusicStart {
                    path: MENU_MUSIC_PATH.to_string(),
                    volume: 1.0,
                    looped: true,
                });
                *music_playing = true;
            }
            // Leaving the menu (loading, new world, exit, or switching to non-start screen)
            MenuAction::LoadScene(_) | MenuAction::GenerateWorld(_) | MenuAction::Exit
                if *music_playing =>
            {
                event_bus.publish(AudioEvent::MusicStop);
                *music_playing = false;
            }
            // Navigating to any screen other than start also stops music
            MenuAction::ShowMenu(_) if *music_playing => {
                event_bus.publish(AudioEvent::MusicStop);
                *music_playing = false;
            }
            _ => {}
        }
    }
}

/// Process console action and publish corresponding events
pub fn process_console_action(action: crate::overlays::ConsoleAction, event_bus: &EventBus) {
    use crate::overlays::ConsoleAction;

    match action {
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
        ConsoleAction::None => {
            // No action
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::events::EventBus;
    use moho_game::scene_builders::WorldSpec;
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

            process_menu_action(&action, &bus);

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

        fn run(
            actions: &[MenuAction],
            mut playing: bool,
            music_exists: bool,
        ) -> (bool, Vec<AudioEvent>) {
            let bus = EventBus::new();
            let rx = record::<AudioEvent>(&bus);

            update_menu_music(actions, &bus, &mut playing, music_exists);

            (playing, rx.try_iter().collect())
        }

        fn leaving_actions() -> Vec<(&'static str, MenuAction)> {
            vec![
                (
                    "load scene",
                    MenuAction::LoadScene(std::path::PathBuf::from("saves/scene.bin")),
                ),
                (
                    "generate world",
                    MenuAction::GenerateWorld(WorldSpec {
                        name: "Test World".to_string(),
                        seed: Some(1),
                        size_xz: 16,
                        day_length_seconds: 1200.0,
                        night_length_seconds: 600.0,
                        initial_time_of_day: 6.0,
                    }),
                ),
                ("exit", MenuAction::Exit),
                ("settings", MenuAction::ShowMenu("settings".to_string())),
            ]
        }

        #[test]
        fn menu_music_starts_on_show_start() {
            let actions = [MenuAction::ShowMenu("start".to_string())];

            let (playing, events) = run(&actions, false, true);

            assert!(playing);
            assert_eq!(events.len(), 1, "events: {events:?}");
            assert!(
                matches!(&events[0], AudioEvent::MusicStart { looped: true, .. }),
                "unexpected {:?}",
                events[0]
            );
        }

        #[test]
        fn menu_music_missing_file_publishes_nothing() {
            let actions = [MenuAction::ShowMenu("start".to_string())];

            let (playing, events) = run(&actions, false, false);

            assert!(!playing);
            assert!(events.is_empty(), "events: {events:?}");
        }

        #[test]
        fn menu_music_stops_when_leaving_start() {
            for (label, action) in leaving_actions() {
                let (playing, events) = run(&[action], true, true);

                assert!(!playing, "{label}: flag should clear");
                assert_eq!(events.len(), 1, "{label}: events: {events:?}");
                assert!(
                    matches!(events[0], AudioEvent::MusicStop),
                    "{label}: unexpected {:?}",
                    events[0]
                );
            }
        }

        #[test]
        fn menu_music_no_double_stop() {
            for (label, action) in leaving_actions() {
                let (playing, events) = run(&[action], false, true);

                assert!(!playing, "{label}");
                assert!(events.is_empty(), "{label}: events: {events:?}");
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

            process_menu_action(&MenuAction::SettingsSaved(prefs), &bus);

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

            process_menu_action(&MenuAction::Close, &bus);

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
