//! Modern, modular egui adapter for moho UI
//!
//! This adapter manages menus and UI state in a scalable way,
//! allowing easy addition of new menus and menu types.
mod event_routing;
mod rendering;

pub use crate::app_state::GameState;
use crate::app_state::game_state::cursor_grabbed;
use crate::prefs::Prefs;
use crate::screens::{Menu, MenuAction, SettingsMenu};
use crate::ui_state::UiStateManager;
use flagstone_game::controller::CameraMode;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event::WindowEvent;
use winit::window::Window;

/// UI events sent to the application
#[derive(Debug, Clone)]
pub enum UiEvent {
    LoadScene(PathBuf),
    /// Start generation of a new world with provided parameters
    NewWorld(flagstone_game::scene_builders::WorldSpec),
    ShowMenu(String),
    Exit,
    OverlayToggled(bool),
    SettingsSaved(Prefs),
    AudioEvent(UiAudioEvent),
}

/// Audio events from UI interactions
#[derive(Debug, Clone)]
pub enum UiAudioEvent {
    ButtonClick,
    MenuNavigate,
    Confirm,
    Cancel,
    Error,
}

pub type UiReceiver = std::sync::mpsc::Receiver<UiEvent>;

/// Global flag for UI overlay visibility
pub static UI_OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Modern egui adapter that manages multiple menus
pub struct EguiAdapter {
    // egui integration
    context: egui::Context,
    winit_state: Option<egui_winit::State>,

    // UI state management (extracted from adapter)
    ui_state: UiStateManager,

    // Event bus for application-wide events
    event_bus: Arc<moho_core::EventBus>,

    // Window reference
    window: Option<Arc<Window>>,

    // Game state (used to determine what to render)
    // This is updated by the main app before each frame
    current_game_state: GameState,

    // Optional progress overlay state. When Some, the adapter will render a
    // simple modal progress overlay showing percent complete and an optional
    // cancel button. The main application can control this by locking the
    // adapter and calling the helper methods below.
    pub progress: Option<ProgressState>,

    // Hover SFX tracking: stores the action key of the last-hovered menu item
    // so we only fire MenuNavigate once per new hover, not every frame.
    last_hovered_key: Option<String>,

    // Whether menu music is currently playing (to avoid redundant start/stop)
    menu_music_playing: bool,

    // Checked once at construction.
    menu_music_exists: bool,

    // Prefs from the last `SettingsSaved` action, until the app takes them.
    saved_prefs: Option<Prefs>,
}

/// Lightweight progress state used by the adapter to render an overlay.
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub title: String,
    pub percent: f32, // 0.0 ..= 1.0
    pub cancellable: bool,
    pub canceled: bool,
}

// Shared as `Arc<Mutex<EguiAdapter>>`, which is `Send + Sync` iff the adapter is `Send`
// (`Sync` is unattainable: egui-winit's clipboard holds an `mpsc::Receiver`). Asserting it
// here makes a future non-`Send` field fail in this crate, not at the binary's
// `InputDispatcher::register` call site.
const _: () = {
    fn assert_send<T: Send>() {}
    let _ = assert_send::<EguiAdapter>;
};

impl EguiAdapter {
    /// Create a new adapter with event bus
    pub fn new(
        window: Option<Arc<Window>>,
        event_bus: Arc<moho_core::EventBus>,
        prefs: moho_core::prefs::Prefs,
    ) -> Self {
        let context = egui::Context::default();
        context.set_visuals(egui::Visuals::dark());

        // Initialize winit state if we have a window
        let winit_state = window.as_ref().map(|w| {
            egui_winit::State::new(
                context.clone(),
                egui::ViewportId::ROOT,
                w.as_ref(),
                None,
                None,
                None,
            )
        });

        Self {
            context,
            winit_state,
            ui_state: UiStateManager::new(prefs),
            event_bus,
            window,
            current_game_state: GameState::Menu,
            progress: None,
            last_hovered_key: None,
            menu_music_playing: false,
            menu_music_exists: std::path::Path::new(event_routing::MENU_MUSIC_PATH).exists(),
            saved_prefs: None,
        }
    }

    /// Update the current game state (called by main app each frame)
    /// The console is open exactly while the game is in `ConsoleOpen`, whichever transition got
    /// it there or took it out.
    pub fn set_game_state(&mut self, state: GameState) {
        self.current_game_state = state;
        if state == GameState::ConsoleOpen {
            self.ui_state.console.open();
        } else {
            self.ui_state.console.close();
        }
    }

    /// Add a new menu to the manager
    pub fn add_menu(&mut self, name: String, menu: Box<dyn Menu>) {
        self.ui_state.add_screen(name, menu);
    }

    /// Show a specific menu
    pub fn show_menu(&mut self, name: &str) {
        self.ui_state.show_screen(name);
    }

    /// Replace the settings screen with one seeded from `prefs` (saving to `save_path`)
    /// and show it, so it starts from the running prefs rather than the file.
    pub fn open_settings(&mut self, prefs: Prefs, save_path: PathBuf) {
        self.ui_state.add_screen(
            "settings".to_string(),
            Box::new(SettingsMenu::with_prefs_at(prefs, save_path)),
        );
        self.ui_state.show_screen("settings");
    }

    /// The registered settings screen, if it is a `SettingsMenu`.
    pub fn settings_menu_mut(&mut self) -> Option<&mut SettingsMenu> {
        self.ui_state
            .get_screen_mut("settings")?
            .as_any_mut()
            .downcast_mut::<SettingsMenu>()
    }

    /// Hand over the prefs from the last saved Settings screen, once.
    ///
    /// Prefs don't ride on the event bus, so the app collects them here after
    /// `UiEvent::SettingsSaved`.
    pub fn take_saved_prefs(&mut self) -> Option<Prefs> {
        self.saved_prefs.take()
    }

    /// Keep what a menu action means for the adapter itself: a Settings save's prefs,
    /// held for `take_saved_prefs`. `ui_frame` calls it for every action.
    pub fn record_menu_action(&mut self, action: &MenuAction) {
        if let MenuAction::SettingsSaved(prefs) = action {
            self.saved_prefs = Some(prefs.clone());
        }
    }

    /// Hide all menus
    pub fn hide_menus(&mut self) {
        self.ui_state.hide_all();
    }

    /// Handle window events (mouse, keyboard).
    /// Returns true if the event was consumed by egui/winit state.
    pub fn handle_winit_event(&mut self, event: &WindowEvent) -> bool {
        if let Some(state) = &mut self.winit_state {
            // on_window_event returns an EventResponse; use its consumed flag
            let resp = state.on_window_event(self.window.as_ref().unwrap(), event);
            return resp.consumed;
        }
        false
    }

    /// Whether egui's last frame had a panel under the pointer.
    pub fn pointer_over_ui(&self) -> bool {
        self.context.is_pointer_over_egui()
    }

    /// Whether the cursor is free to point at the screen, going by the game state and the
    /// camera mode in the last HUD data.
    pub fn cursor_free(&self) -> bool {
        let camera_mode = if self.ui_state.overlay_manager.is_fps_mode() {
            CameraMode::FirstPerson
        } else {
            CameraMode::Isometric
        };
        !cursor_grabbed(self.current_game_state, camera_mode)
    }

    /// Handles a window event while no menu is visible (gameplay).
    ///
    /// With a free cursor, pointer motion and mouse buttons feed egui, so it knows what is
    /// under the pointer and widgets can be dragged; a mouse press that lands on a panel is
    /// returned as true, which the game must not see. With a grabbed cursor only a mouse
    /// release is fed, so egui never holds a button pressed before the grab; nothing is
    /// consumed: the pointer is hidden, so panels cannot be aimed at.
    pub fn handle_gameplay_event(&mut self, event: &WindowEvent) -> bool {
        let cursor_free = self.cursor_free();
        let input = GameplayInput::from(event);
        if feeds_egui(input, cursor_free) {
            self.handle_winit_event(event);
        }
        input == GameplayInput::Press && press_consumed(self.pointer_over_ui(), cursor_free)
    }

    /// Return true if the active menu/screen captures raw input events.
    pub fn active_screen_captures_input(&self) -> bool {
        if let Some(screen) = self.ui_state.active_screen() {
            return screen.captures_raw_input();
        }
        false
    }

    /// Try to let the active menu/screen process raw WindowEvent input.
    /// Returns true if the event was consumed.
    pub fn try_handle_screen_input(&mut self, event: &WindowEvent) -> bool {
        if let Some(screen) = self.ui_state.active_screen_mut() {
            return screen.handle_raw_input(event);
        }
        false
    }

    /// Returns whether the UI is currently visible
    pub fn is_visible(&self) -> bool {
        self.ui_state.visible
    }

    /// Sets the UI visibility
    pub fn set_visible(&mut self, visible: bool) {
        self.ui_state.visible = visible;
    }

    /// Start showing a progress overlay with the given title. This will
    /// overwrite any existing progress state.
    pub fn start_progress(&mut self, title: impl Into<String>, cancellable: bool) {
        self.progress = Some(ProgressState {
            title: title.into(),
            percent: 0.0,
            cancellable,
            canceled: false,
        });
        self.ui_state.visible = true;
    }

    /// Update the visible progress fraction (0.0 ..= 1.0)
    pub fn set_progress(&mut self, percent: f32) {
        if let Some(p) = &mut self.progress {
            p.percent = percent.clamp(0.0, 1.0);
        }
    }

    /// Mark the progress overlay finished and remove it from view.
    pub fn finish_progress(&mut self) {
        self.progress = None;
    }

    /// Check whether the user clicked cancel on the progress overlay. This
    /// returns true once and clears the flag; it is safe for the caller to
    /// poll periodically while a long-running task is in progress.
    pub fn take_progress_canceled(&mut self) -> bool {
        if let Some(p) = &mut self.progress {
            let was = p.canceled;
            p.canceled = false;
            was
        } else {
            false
        }
    }

    /// Get input from egui_winit
    fn take_egui_input(&mut self) -> egui::RawInput {
        if let (Some(state), Some(window)) = (&mut self.winit_state, &self.window) {
            state.take_egui_input(window.as_ref())
        } else {
            egui::RawInput::default()
        }
    }

    /// Handle platform output from egui
    fn handle_platform_output(&mut self, platform_output: egui::PlatformOutput) {
        if let (Some(state), Some(window)) = (&mut self.winit_state, &self.window) {
            state.handle_platform_output(window.as_ref(), platform_output);
        }
    }

    /// Name of the screen currently shown, if any.
    pub fn active_screen_name(&self) -> Option<&str> {
        self.ui_state.active_screen_name()
    }

    /// Update overlay HUD data for the current frame.
    pub fn update_hud_data(&mut self, data: crate::overlays::HudData) {
        self.ui_state.overlay_manager.update_data(data);
    }

    /// What the player asked for through the HUD since the last call.
    pub fn take_hud_intents(&mut self) -> crate::overlays::HudIntents {
        self.ui_state.overlay_manager.take_intents()
    }

    /// Routes one key or button through the console before egui sees it.
    pub fn on_key(
        &mut self,
        input: moho_input::bindings::Input,
        pressed: bool,
        repeat: bool,
    ) -> moho_ui_shell::Routing {
        self.ui_state.console.on_key(input, pressed, repeat)
    }

    pub fn console_mut(
        &mut self,
    ) -> &mut moho_ui_shell::Console<crate::console_commands::StrategyConsoleCommand> {
        &mut self.ui_state.console
    }

    pub fn take_console_commands(
        &mut self,
    ) -> Vec<crate::console_commands::StrategyConsoleCommand> {
        self.ui_state.console.take_commands()
    }

    pub fn take_engine_commands(&mut self) -> Vec<moho_ui_shell::EngineCommand> {
        self.ui_state.console.take_engine_commands()
    }

    /// Toggle the debug HUD overlay (F3).
    pub fn toggle_debug_hud(&mut self) {
        self.ui_state.overlay_manager.toggle("debug");
    }

    /// Toggle any overlay by name.
    pub fn toggle_overlay(&mut self, name: &str) {
        self.ui_state.overlay_manager.toggle(name);
    }
}

impl moho_render_api::UiFrameSource for EguiAdapter {
    fn ui_frame(&mut self, _size_in_pixels: [u32; 2]) -> Option<moho_render_api::UiFrame> {
        use crate::modal::ModalResult;
        // Update global visibility flag (tracks menu visibility for input routing)
        UI_OVERLAY_VISIBLE.store(self.ui_state.visible, Ordering::SeqCst);

        // Render when menus are visible OR during any gameplay state (for overlays)
        let needs_render = self.ui_state.visible || self.current_game_state != GameState::Menu;
        if !needs_render {
            return None;
        }

        // Take input from winit integration
        let raw_input = self.take_egui_input();

        // Run egui and collect menu actions + modal result
        let mut menu_actions = Vec::new();
        let mut new_hovered_key: Option<String> = None;
        let mut modal_result = crate::modal::ModalResult::None;

        let full_output = self.context.run_ui(raw_input, |ui| {
            // Render based on current game state (delegates to rendering module)
            let result =
                rendering::render_game_state(ui, &mut self.ui_state, self.current_game_state);
            menu_actions.extend(result.actions);
            new_hovered_key = result.hovered_key;

            // Check if any screen wants to show a modal
            if let Some(screen) = self.ui_state.active_screen_mut()
                && let Some(modal) = screen.take_pending_modal()
            {
                self.ui_state.modal_manager.show(modal);
            }

            // Render modal on top of menu (if active); Settings reached from the pause
            // window can raise one too.
            if matches!(self.current_game_state, GameState::Menu | GameState::Paused) {
                modal_result = self.ui_state.modal_manager.render(ui);
            }

            // Render progress overlay (if present)
            if let Some(progress) = self.progress.as_mut() {
                rendering::render_progress_overlay(ui, progress);
            }
        });

        // Handle modal result
        match modal_result {
            ModalResult::Confirm => {
                if let Some(screen) = self.ui_state.active_screen_mut() {
                    screen.on_modal_confirm();
                }
            }
            ModalResult::Cancel => {
                if let Some(screen) = self.ui_state.active_screen_mut() {
                    screen.on_modal_cancel();
                }
            }
            ModalResult::None => {}
        }

        // Hover SFX: emit MenuNavigate exactly once when hover changes to a new item
        if new_hovered_key != self.last_hovered_key && new_hovered_key.is_some() {
            event_routing::emit_audio_event(&self.event_bus, UiAudioEvent::MenuNavigate);
        }
        self.last_hovered_key = new_hovered_key;

        // Process menu actions (delegates to event_routing module)
        for action in &menu_actions {
            self.record_menu_action(action);
            event_routing::process_menu_action(action, self.current_game_state, &self.event_bus);
        }

        // Handle Close action locally (hide menus)
        for action in &menu_actions {
            if matches!(action, MenuAction::Close) {
                self.hide_menus();
                break;
            }
        }

        // Runs only on rendered frames. Every non-`Menu` state renders, so a stop is never missed;
        // a hidden `Menu` keeps whatever is playing (music carries on through world generation).
        let want_menu_music = self.current_game_state == GameState::Menu && self.menu_music_exists;
        if let Some(event) =
            event_routing::menu_music_change(want_menu_music, self.menu_music_playing)
        {
            self.event_bus.publish(event);
            self.menu_music_playing = want_menu_music;
        }

        // Handle platform output
        self.handle_platform_output(full_output.platform_output);

        let clipped_primitives = self
            .context
            .tessellate(full_output.shapes, full_output.pixels_per_point);
        Some(moho_ui_shell::paint_map::to_ui_frame(
            &clipped_primitives,
            full_output.textures_delta,
            full_output.pixels_per_point,
        ))
    }
}

/// The kinds of window event `feeds_egui` tells apart. winit builds its events from a device
/// id that tests cannot construct under `unsafe_code = "deny"`, so the rule sits on the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayInput {
    Press,
    Release,
    /// The pointer moves or leaves the window.
    Motion,
    /// Anything else, keyboard included.
    Other,
}

/// Whether egui is given a window event during gameplay: pointer motion and buttons with a
/// free cursor, and a release always, so a press on a widget followed by a cursor grab
/// does not leave egui holding the button.
pub fn feeds_egui(input: GameplayInput, cursor_free: bool) -> bool {
    match input {
        GameplayInput::Release => true,
        GameplayInput::Press | GameplayInput::Motion => cursor_free,
        GameplayInput::Other => false,
    }
}

impl From<&WindowEvent> for GameplayInput {
    fn from(event: &WindowEvent) -> Self {
        match event {
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                ..
            } => Self::Press,
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Released,
                ..
            } => Self::Release,
            WindowEvent::CursorMoved { .. } | WindowEvent::CursorLeft { .. } => Self::Motion,
            _ => Self::Other,
        }
    }
}

/// Whether a mouse press is taken by the UI: it lands on a panel and the cursor is free.
fn press_consumed(pointer_over_ui: bool, cursor_free: bool) -> bool {
    pointer_over_ui && cursor_free
}

/// Build adapter function
pub fn build_adapter(
    window: Option<Arc<Window>>,
    event_bus: Arc<moho_core::EventBus>,
    prefs: moho_core::prefs::Prefs,
) -> EguiAdapter {
    EguiAdapter::new(window, event_bus, prefs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use moho_core::events::AudioEvent;
    use moho_render_api::UiFrameSource;

    fn adapter() -> EguiAdapter {
        EguiAdapter::new(
            None,
            Arc::new(moho_core::EventBus::new()),
            moho_core::prefs::Prefs::default(),
        )
    }

    #[test]
    fn hidden_menu_state_returns_no_paint_data() {
        let mut adapter = adapter();
        adapter.set_game_state(GameState::Menu);
        adapter.hide_menus();
        adapter.set_visible(false);

        let frame = adapter.ui_frame([800, 600]);

        assert!(frame.is_none());
    }

    #[test]
    fn visible_menu_state_returns_paint_data() {
        let mut adapter = adapter();
        adapter.set_game_state(GameState::Menu);
        adapter.set_visible(true);

        let frame = adapter.ui_frame([800, 600]).expect("a visible menu paints");

        assert!(frame.pixels_per_point > 0.0);
        assert!(
            !frame.textures_set.is_empty(),
            "the first frame uploads egui's font atlas"
        );
    }

    /// Runs one gameplay frame in the isometric view with the pointer at `pos`.
    fn gameplay_frame_with_pointer_at(adapter: &mut EguiAdapter, pos: egui::Pos2) {
        use crate::overlays::test_support::{input, run_ui};
        adapter.set_game_state(GameState::Playing);
        adapter.update_hud_data(crate::overlays::HudData {
            is_fps_mode: false,
            ..Default::default()
        });
        for events in [Vec::new(), vec![egui::Event::PointerMoved(pos)]] {
            let _ = run_ui(&adapter.context, input(events), |ui| {
                rendering::render_game_state(ui, &mut adapter.ui_state, GameState::Playing);
            });
        }
    }

    #[test]
    fn press_over_a_hud_panel_is_consumed() {
        let mut adapter = adapter();

        gameplay_frame_with_pointer_at(&mut adapter, egui::pos2(770.0, 22.0));

        assert!(adapter.pointer_over_ui());
    }

    #[test]
    fn press_over_open_world_is_not_consumed() {
        let mut adapter = adapter();

        gameplay_frame_with_pointer_at(&mut adapter, egui::pos2(100.0, 300.0));

        assert!(!adapter.pointer_over_ui());
    }

    #[test]
    fn press_over_the_view_height_slider_is_consumed() {
        use crate::overlays::test_support::{input, run_ui, second_frame, small_rect_centers};
        use crate::overlays::{HudData, Overlay, RtsHud, ViewHeightHud};
        let data = |view_height| HudData {
            is_fps_mode: false,
            view_height,
            ..Default::default()
        };
        let slider = Some(ViewHeightHud {
            floor: 0,
            top: 144,
            level: 100,
        });
        let mut hud = RtsHud::new();
        let handle = small_rect_centers(
            &second_frame(&egui::Context::default(), |ui| {
                hud.render(ui, &data(slider));
            }),
            20.0,
        )
        .first()
        .copied()
        .expect("the slider draws its handle");
        let mut adapter = adapter();
        adapter.set_game_state(GameState::Playing);
        adapter.update_hud_data(data(slider));

        for events in [Vec::new(), vec![egui::Event::PointerMoved(handle)]] {
            let _ = run_ui(&adapter.context, input(events), |ui| {
                rendering::render_game_state(ui, &mut adapter.ui_state, GameState::Playing);
            });
        }

        assert!(adapter.pointer_over_ui());
    }

    #[test]
    fn pointer_input_reaches_egui_with_a_free_cursor_and_a_release_always_does() {
        let rows = [
            (GameplayInput::Press, true, true),
            (GameplayInput::Release, true, true),
            (GameplayInput::Motion, true, true),
            (GameplayInput::Other, true, false),
            (GameplayInput::Press, false, false),
            (GameplayInput::Release, false, true),
            (GameplayInput::Motion, false, false),
            (GameplayInput::Other, false, false),
        ];

        for (input, cursor_free, fed) in rows {
            assert_eq!(
                feeds_egui(input, cursor_free),
                fed,
                "{input:?}, cursor_free {cursor_free}"
            );
        }
    }

    #[test]
    fn press_is_consumed_only_over_a_panel_with_a_free_cursor() {
        let rows = [
            (true, true, true),
            (true, false, false),
            (false, true, false),
            (false, false, false),
        ];

        for (over_ui, cursor_free, consumed) in rows {
            assert_eq!(
                press_consumed(over_ui, cursor_free),
                consumed,
                "over_ui {over_ui}, cursor_free {cursor_free}"
            );
        }
    }

    #[test]
    fn cursor_is_not_free_while_playing_in_first_person() {
        let mut adapter = adapter();
        adapter.set_game_state(GameState::Playing);
        adapter.update_hud_data(crate::overlays::HudData {
            is_fps_mode: true,
            ..Default::default()
        });

        assert!(!adapter.cursor_free());
    }

    #[test]
    fn cursor_is_free_while_playing_in_the_strategy_view() {
        let mut adapter = adapter();
        adapter.set_game_state(GameState::Playing);
        adapter.update_hud_data(crate::overlays::HudData {
            is_fps_mode: false,
            ..Default::default()
        });

        assert!(adapter.cursor_free());
    }

    fn audio_adapter() -> (EguiAdapter, std::sync::mpsc::Receiver<AudioEvent>) {
        let bus = Arc::new(moho_core::EventBus::new());
        let (tx, rx) = std::sync::mpsc::channel();
        bus.subscribe(move |e: &AudioEvent| {
            let _ = tx.send(e.clone());
        });
        let adapter = EguiAdapter::new(None, bus, moho_core::prefs::Prefs::default());
        (adapter, rx)
    }

    fn music_events(rx: &std::sync::mpsc::Receiver<AudioEvent>) -> Vec<AudioEvent> {
        rx.try_iter()
            .filter(|e| matches!(e, AudioEvent::MusicStart { .. } | AudioEvent::MusicStop))
            .collect()
    }

    #[test]
    fn menu_frame_starts_menu_music_once() {
        let (mut adapter, rx) = audio_adapter();
        adapter.menu_music_exists = true;
        adapter.set_game_state(GameState::Menu);
        adapter.set_visible(true);

        adapter.ui_frame([800, 600]);
        adapter.ui_frame([800, 600]);

        let events = music_events(&rx);
        assert_eq!(events.len(), 1, "events: {events:?}");
        assert!(
            matches!(&events[0], AudioEvent::MusicStart { path, looped: true, .. }
                if path == "assets/audio/music/menu.ogg"),
            "unexpected {:?}",
            events[0]
        );
    }

    #[test]
    fn menu_without_music_file_publishes_nothing_and_still_paints() {
        let (mut adapter, rx) = audio_adapter();
        adapter.menu_music_exists = false;
        adapter.set_game_state(GameState::Menu);
        adapter.set_visible(true);

        let frame = adapter.ui_frame([800, 600]);

        assert!(frame.is_some(), "the menu still paints");
        let events = music_events(&rx);
        assert!(events.is_empty(), "events: {events:?}");
    }

    #[test]
    fn leaving_the_menu_state_stops_menu_music() {
        let (mut adapter, rx) = audio_adapter();
        adapter.menu_music_exists = true;
        adapter.set_game_state(GameState::Menu);
        adapter.set_visible(true);
        adapter.ui_frame([800, 600]);

        adapter.set_game_state(GameState::Playing);
        adapter.ui_frame([800, 600]);

        let events = music_events(&rx);
        assert_eq!(events.len(), 2, "events: {events:?}");
        assert!(matches!(events[0], AudioEvent::MusicStart { .. }));
        assert!(matches!(events[1], AudioEvent::MusicStop));
    }

    fn prefs_forward_on_q() -> Prefs {
        let mut prefs = Prefs::default();
        prefs.set_bindings(
            [("move_forward".to_string(), "Q".to_string())]
                .into_iter()
                .collect(),
        );
        prefs
    }

    #[test]
    fn open_settings_seeds_the_screen_from_the_given_prefs() {
        use moho_input::bindings::Binding;
        use moho_input::key::Key;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.ini");
        let mut adapter = adapter();
        let mut prefs = prefs_forward_on_q();
        prefs.set_shadow_quality(1);

        adapter.open_settings(prefs, path.clone());

        assert_eq!(adapter.active_screen_name(), Some("settings"));
        assert!(adapter.is_visible());
        let menu = adapter
            .settings_menu_mut()
            .expect("the settings screen is registered");
        assert_eq!(
            menu.get_staged_binding(0).first(),
            Some(&Binding::key(Key::Q))
        );
        menu.apply_staged_changes().unwrap();
        let saved = Prefs::load_from(&path).0;
        assert_eq!(
            saved.shadow_quality(),
            1,
            "the screen saves what it was seeded with"
        );
        assert_eq!(
            saved.bindings().get("move_forward").map(String::as_str),
            Some("Q")
        );
    }

    #[test]
    fn saved_prefs_are_handed_over_once() {
        let mut adapter = adapter();
        let mut saved = prefs_forward_on_q();
        saved.set_shadow_quality(1);
        assert!(adapter.take_saved_prefs().is_none(), "nothing saved yet");
        adapter.record_menu_action(&MenuAction::Close);
        assert!(
            adapter.take_saved_prefs().is_none(),
            "Close carries no prefs"
        );

        adapter.record_menu_action(&MenuAction::SettingsSaved(saved.clone()));

        assert_eq!(adapter.take_saved_prefs(), Some(saved));
        assert!(adapter.take_saved_prefs().is_none(), "handed over once");
    }
}
