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

            // Announces the new window settings; the engine is asked for them through
            // `EguiAdapter::record_menu_action`, which `App::frame` reads.
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

/// The music event that moves menu music from `playing` to `want`: a looped start of the file
/// at `music`, a stop, or `None` when they already agree.
pub fn menu_music_change(
    want: bool,
    playing: bool,
    music: &std::path::Path,
) -> Option<moho_core::events::AudioEvent> {
    use moho_core::events::AudioEvent;

    match (want, playing) {
        (true, false) => Some(AudioEvent::MusicStart {
            path: music.to_string_lossy().into_owned(),
            volume: 1.0,
            looped: true,
        }),
        (false, true) => Some(AudioEvent::MusicStop),
        _ => None,
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

        const MUSIC: &str = "assets/audio/music/menu.ogg";

        #[test]
        fn starts_from_the_given_music_path() {
            let event = menu_music_change(true, false, MUSIC.as_ref());

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
            let event = menu_music_change(false, true, MUSIC.as_ref());

            assert!(
                matches!(event, Some(AudioEvent::MusicStop)),
                "unexpected {event:?}"
            );
        }

        #[test]
        fn no_change_when_want_matches_playing() {
            for state in [true, false] {
                let event = menu_music_change(state, state, MUSIC.as_ref());

                assert!(event.is_none(), "state {state}: unexpected {event:?}");
            }
        }
    }
    mod window_mode {
        use super::*;
        use crate::adapter::EguiAdapter;
        use crate::prefs::{Prefs, WindowMode as Saved};
        use moho_app::WindowMode as Live;
        use moho_core::events::UiEvent;
        use std::path::PathBuf;

        fn prefs_with(mode: Saved) -> Prefs {
            let mut prefs = Prefs::default();
            prefs.set_window_mode(mode);
            prefs
        }

        fn adapter_on(bus: &Arc<EventBus>) -> EguiAdapter {
            EguiAdapter::new(None, bus.clone(), Prefs::default())
        }

        fn prefs_file(dir: &tempfile::TempDir) -> PathBuf {
            dir.path().join("prefs.ini")
        }

        /// An adapter whose open Settings screen was seeded with `saved`.
        fn with_settings_open(saved: Saved, path: &std::path::Path) -> EguiAdapter {
            let mut adapter = adapter_on(&Arc::new(EventBus::new()));
            adapter.open_settings(prefs_with(saved), path.to_path_buf());
            adapter
        }

        /// What the Video tab shows, and whether Settings reads as having unsaved changes.
        fn video_tab(adapter: &mut EguiAdapter) -> (Saved, bool) {
            let menu = adapter.settings_menu_mut().expect("Settings is open");
            (menu.staged_window_mode(), menu.has_unsaved_changes())
        }

        #[test]
        fn a_key_toggle_updates_the_video_tab_without_marking_it_dirty() {
            let dir = tempfile::tempdir().unwrap();
            let mut adapter = with_settings_open(Saved::Windowed, &prefs_file(&dir));

            adapter.window_mode_changed(Live::Borderless);
            let into_fullscreen = video_tab(&mut adapter);
            adapter.window_mode_changed(Live::Windowed);
            let back = video_tab(&mut adapter);

            assert_eq!(into_fullscreen, (Saved::Borderless, false));
            assert_eq!(back, (Saved::Windowed, false));
        }

        #[test]
        fn a_key_toggle_into_fullscreen_keeps_a_saved_fullscreen_choice() {
            let dir = tempfile::tempdir().unwrap();
            let mut adapter = with_settings_open(Saved::Fullscreen, &prefs_file(&dir));

            adapter.window_mode_changed(Live::Windowed);
            let windowed = video_tab(&mut adapter);
            adapter.window_mode_changed(Live::Borderless);
            let fullscreen = video_tab(&mut adapter);

            assert_eq!(windowed, (Saved::Windowed, false));
            assert_eq!(fullscreen, (Saved::Fullscreen, false));
        }

        #[test]
        fn the_video_tab_opens_on_the_mode_a_key_set() {
            let dir = tempfile::tempdir().unwrap();
            let path = prefs_file(&dir);
            let rows = [
                (Saved::Windowed, Live::Borderless, Saved::Borderless),
                (Saved::Borderless, Live::Borderless, Saved::Borderless),
                (Saved::Fullscreen, Live::Borderless, Saved::Fullscreen),
                (Saved::Fullscreen, Live::Windowed, Saved::Windowed),
                (Saved::Borderless, Live::Windowed, Saved::Windowed),
            ];

            for (saved, live, shown) in rows {
                let mut adapter = adapter_on(&Arc::new(EventBus::new()));
                adapter.window_mode_changed(live);
                adapter.open_settings(prefs_with(saved), path.clone());

                assert_eq!(
                    video_tab(&mut adapter),
                    (shown, false),
                    "saved {saved:?}, window {live:?}"
                );
            }
        }

        #[test]
        fn applying_a_mode_requests_it_from_the_engine() {
            let rows = [
                (Saved::Windowed, Live::Windowed),
                (Saved::Fullscreen, Live::Borderless),
                (Saved::Borderless, Live::Borderless),
            ];

            for (saved, asked) in rows {
                let dir = tempfile::tempdir().unwrap();
                let mut adapter = with_settings_open(Saved::Windowed, &prefs_file(&dir));
                assert_eq!(adapter.take_requested_window_mode(), None, "{saved:?}");

                adapter.record_menu_action(&MenuAction::SettingsSaved(prefs_with(saved)));

                assert_eq!(
                    adapter.take_requested_window_mode(),
                    Some(asked),
                    "{saved:?}"
                );
                assert_eq!(adapter.take_requested_window_mode(), None, "taken once");
            }
        }

        #[test]
        fn a_key_toggle_does_not_change_saved_prefs() {
            let dir = tempfile::tempdir().unwrap();
            let path = prefs_file(&dir);
            let bus = Arc::new(EventBus::new());
            let ui = record::<UiEvent>(&bus);
            let mut adapter = adapter_on(&bus);
            adapter.open_settings(prefs_with(Saved::Windowed), path.clone());

            adapter.window_mode_changed(Live::Borderless);

            let menu = adapter.settings_menu_mut().expect("Settings is open");
            assert_eq!(menu.saved_window_mode(), Saved::Windowed);
            assert!(!path.exists(), "a key toggle wrote the prefs file");
            assert_eq!(adapter.take_saved_prefs(), None);
            assert_eq!(adapter.take_requested_window_mode(), None);
            let published: Vec<_> = ui.try_iter().collect();
            assert!(published.is_empty(), "events: {published:?}");
        }

        #[test]
        fn saving_settings_after_a_toggle_persists_the_mode() {
            let dir = tempfile::tempdir().unwrap();
            let path = prefs_file(&dir);
            let mut adapter = with_settings_open(Saved::Windowed, &path);
            adapter.window_mode_changed(Live::Borderless);

            adapter
                .settings_menu_mut()
                .expect("Settings is open")
                .apply_staged_changes()
                .unwrap();

            assert_eq!(Prefs::load_from(&path).0.window_mode(), Saved::Borderless);
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
