//! Overlay management system for layered HUD elements.
//!
//! Provides a generic [`Overlay`] trait and an [`OverlayManager`] that
//! renders registered overlays each frame. Each overlay receives shared
//! [`HudData`] and decides internally whether to draw based on its
//! visibility flag and the current camera mode.

/// The strategy view-height slider's range and position, in whole metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewHeightHud {
    /// Height of the lowest drawn layer's bottom; the slider's lowest step is `floor + 1`.
    pub floor: i32,

    /// Height of the topmost step, which means no cut.
    pub top: i32,

    /// The current step, `top` when nothing is cut.
    pub level: i32,
}

/// What the player asked for through the HUD this frame, for the game to apply.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HudIntents {
    pub view_level: Option<i32>,

    /// The player pressed the mining-designation control.
    pub toggle_mine_designation: bool,
}

/// Per-frame data pushed from the game loop into all overlays.
#[derive(Debug, Clone)]
pub struct HudData {
    /// Player world position `[x, y, z]`.
    pub player_position: [f32; 3],

    /// Current chunk coordinates `[x, y, z]`.
    pub chunk_position: [i32; 3],

    /// Camera mode display name (e.g. `"FirstPerson"`, `"Isometric"`).
    pub camera_mode: String,

    /// `true` when the camera is in first-person mode.
    pub is_fps_mode: bool,

    /// Delta time for the current frame (seconds).
    pub frame_time_secs: f32,

    /// Time of day in hours (`0.0`–`24.0`).
    pub time_of_day: f32,

    /// Material ID under the crosshair, if a raycast hit.
    pub material_under_crosshair: Option<u8>,

    /// Camera yaw in radians. `0.0` = North (+Z), increases clockwise.
    pub camera_yaw: f32,

    /// Player health fraction (`0.0`–`1.0`).
    pub player_health: f32,

    /// Player stamina fraction (`0.0`–`1.0`).
    pub player_stamina: f32,

    /// XZ coordinates of every currently-loaded chunk, for the chunk debug minimap.
    pub loaded_chunk_xz: Vec<[i32; 2]>,

    /// Hotbar contents as `(resource_id, count)` pairs, denormalized from the
    /// player pawn's inventory. Rendered starting at hotbar slot 1 — slot 0
    /// is reserved for the equipped tool (`equipped_tool_label`). Deliberately
    /// primitive — `flagstone_ui` must not depend on `flagstone_game`'s
    /// `Inventory`/`Pawn` types; the binary performs the `Pawn::inventory` →
    /// this shape mapping at its `HudData` construction site.
    pub hotbar: Vec<(u32, u32)>,

    /// Label for the tool equipped in hotbar slot 0, if any (e.g. "Pickaxe").
    /// `None` means no tool is currently equipped — slot 0 renders empty and
    /// mining is gated off game-side.
    pub equipped_tool_label: Option<String>,

    /// Which hotbar slot (`0..8`) is currently selected, for the highlight.
    pub selected_slot: usize,

    /// The view-height slider's range and level; `None` when no world is loaded.
    pub view_height: Option<ViewHeightHud>,

    /// Recent pickups as `(resource_id, amount, remaining_life)`, life from `0.0` to `1.0`.
    /// Primitive for the same reason as `hotbar`.
    pub pickups: Vec<(u32, u32, f32)>,

    /// Whether the mining designation is active, for the HUD control's state.
    pub mine_designation_active: bool,
}

impl Default for HudData {
    fn default() -> Self {
        Self {
            player_position: [0.0; 3],
            chunk_position: [0; 3],
            camera_mode: "FirstPerson".to_string(),
            is_fps_mode: true,
            frame_time_secs: 1.0 / 60.0,
            time_of_day: 12.0,
            material_under_crosshair: None,
            camera_yaw: 0.0,
            player_health: 1.0,
            player_stamina: 1.0,
            loaded_chunk_xz: Vec::new(),
            hotbar: Vec::new(),
            equipped_tool_label: Some("Pickaxe".to_string()),
            selected_slot: 0,
            view_height: None,
            pickups: Vec::new(),
            mine_designation_active: false,
        }
    }
}

/// Trait for overlay UI components rendered on top of the game world.
///
/// Overlays self-filter: `render()` receives [`HudData`] and should
/// check both `is_visible()` and any mode-appropriateness conditions
/// before drawing.
pub trait Overlay: std::fmt::Debug + Send + Sync {
    /// Unique name used for lookup (e.g. `"debug"`, `"fps_hud"`).
    fn name(&self) -> &str;

    /// Render the overlay; `ui` is the frame's root ui, and `ui.ctx()` reaches windows and areas.
    fn render(&mut self, ui: &mut egui::Ui, data: &HudData);

    /// Whether this overlay is currently enabled (user toggle).
    fn is_visible(&self) -> bool;

    /// Set visibility (user toggle).
    fn set_visible(&mut self, visible: bool);

    /// Toggle visibility.
    fn toggle(&mut self) {
        let v = self.is_visible();
        self.set_visible(!v);
    }

    /// Whether this overlay is a diagnostic that stays drawn while the game is paused.
    /// Gameplay HUDs keep the default and step aside.
    fn shows_while_paused(&self) -> bool {
        false
    }

    /// Move what the player asked for since the last call into `out`, leaving none behind.
    fn take_intents(&mut self, _out: &mut HudIntents) {}
}

/// Manages a collection of overlay layers, rendering them in registration order.
pub struct OverlayManager {
    overlays: Vec<Box<dyn Overlay>>,
    hud_data: HudData,
}

impl std::fmt::Debug for OverlayManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayManager")
            .field("overlay_count", &self.overlays.len())
            .field(
                "overlay_names",
                &self.overlays.iter().map(|o| o.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
            hud_data: HudData::default(),
        }
    }

    /// Register a new overlay layer.
    pub fn register(&mut self, overlay: Box<dyn Overlay>) {
        self.overlays.push(overlay);
    }

    /// Replace the shared HUD data for this frame.
    pub fn update_data(&mut self, data: HudData) {
        self.hud_data = data;
    }

    /// Whether the last pushed HUD data says the camera is first person.
    pub fn is_fps_mode(&self) -> bool {
        self.hud_data.is_fps_mode
    }

    /// Render every registered overlay (each self-filters on visibility/mode).
    pub fn render_all(&mut self, ui: &mut egui::Ui) {
        for overlay in &mut self.overlays {
            overlay.render(ui, &self.hud_data);
        }
    }

    /// Render only the overlays that stay drawn while paused (each self-filters on visibility).
    pub fn render_paused(&mut self, ui: &mut egui::Ui) {
        for overlay in self.overlays.iter_mut().filter(|o| o.shows_while_paused()) {
            overlay.render(ui, &self.hud_data);
        }
    }

    /// Collect, and clear, what every overlay's player asked for since the last call.
    pub fn take_intents(&mut self) -> HudIntents {
        let mut intents = HudIntents::default();
        for overlay in &mut self.overlays {
            overlay.take_intents(&mut intents);
        }
        intents
    }

    /// Toggle an overlay by name. No-op if the name is not found.
    pub fn toggle(&mut self, name: &str) {
        if let Some(overlay) = self.overlays.iter_mut().find(|o| o.name() == name) {
            overlay.toggle();
        }
    }

    /// Set visibility of an overlay by name. No-op if not found.
    pub fn set_visible(&mut self, name: &str, visible: bool) {
        if let Some(overlay) = self.overlays.iter_mut().find(|o| o.name() == name) {
            overlay.set_visible(visible);
        }
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct StubOverlay {
        name: &'static str,
        visible: bool,
    }

    impl StubOverlay {
        fn new(name: &'static str, visible: bool) -> Self {
            Self { name, visible }
        }
    }

    impl Overlay for StubOverlay {
        fn name(&self) -> &'static str {
            self.name
        }
        fn render(&mut self, _ui: &mut egui::Ui, _data: &HudData) {}
        fn is_visible(&self) -> bool {
            self.visible
        }
        fn set_visible(&mut self, visible: bool) {
            self.visible = visible;
        }
    }

    /// An overlay holding a view level the player picked, until it is taken.
    #[derive(Debug)]
    struct AskingOverlay {
        asked: Option<i32>,
    }

    impl Overlay for AskingOverlay {
        fn name(&self) -> &'static str {
            "asking"
        }
        fn render(&mut self, _ui: &mut egui::Ui, _data: &HudData) {}
        fn is_visible(&self) -> bool {
            true
        }
        fn set_visible(&mut self, _visible: bool) {}
        fn take_intents(&mut self, out: &mut HudIntents) {
            out.view_level = self.asked.take();
        }
    }

    #[test]
    fn take_intents_collects_and_clears() {
        let mut mgr = OverlayManager::new();
        mgr.register(Box::new(StubOverlay::new("silent", true)));
        mgr.register(Box::new(AskingOverlay { asked: Some(7) }));
        mgr.register(Box::new(StubOverlay::new("also silent", true)));

        let first = mgr.take_intents();
        let second = mgr.take_intents();

        assert_eq!(
            first,
            HudIntents {
                view_level: Some(7),
                ..HudIntents::default()
            }
        );
        assert_eq!(second, HudIntents::default(), "a take empties the overlays");
    }

    fn visibility(mgr: &OverlayManager) -> Vec<bool> {
        mgr.overlays.iter().map(|o| o.is_visible()).collect()
    }

    #[test]
    fn toggle_overlay_by_name() {
        let mut mgr = OverlayManager::new();
        mgr.register(Box::new(StubOverlay::new("first", false)));
        mgr.register(Box::new(StubOverlay::new("second", false)));

        mgr.toggle("second");
        assert_eq!(visibility(&mgr), [false, true]);

        mgr.toggle("second");
        assert_eq!(visibility(&mgr), [false, false]);
    }

    #[test]
    fn toggle_unknown_name_is_noop() {
        let mut mgr = OverlayManager::new();
        mgr.register(Box::new(StubOverlay::new("first", true)));
        mgr.register(Box::new(StubOverlay::new("second", false)));

        mgr.toggle("nonexistent");

        assert_eq!(visibility(&mgr), [true, false]);
    }

    #[test]
    fn render_paused_draws_only_overlays_that_show_while_paused() {
        use crate::overlays::test_support::{second_frame, texts};

        let ctx = egui::Context::default();
        let mut mgr = OverlayManager::new();
        let mut hud = crate::overlays::RtsHud::new();
        hud.set_visible(true);
        let mut debug = crate::overlays::DebugHud::new();
        debug.set_visible(true);
        mgr.register(Box::new(hud));
        mgr.register(Box::new(debug));
        mgr.update_data(HudData {
            is_fps_mode: false,
            time_of_day: 7.25,
            ..Default::default()
        });

        let all = texts(&second_frame(&ctx, |ui| mgr.render_all(ui)));
        let paused = texts(&second_frame(&ctx, |ui| mgr.render_paused(ui)));

        assert!(all.contains(&"07:15".to_string()), "{all:?}");
        assert!(!paused.contains(&"07:15".to_string()), "{paused:?}");
        assert!(paused.iter().any(|t| t.starts_with("Pos:")), "{paused:?}");
    }

    #[test]
    fn render_all_renders_each_overlay_with_current_data() {
        use crate::overlays::test_support::{second_frame, texts};

        let ctx = egui::Context::default();
        let mut mgr = OverlayManager::new();
        let mut debug = crate::overlays::DebugHud::new();
        debug.set_visible(true);
        mgr.register(Box::new(debug));
        mgr.update_data(HudData {
            player_position: [9.0, 8.0, 7.0],
            ..Default::default()
        });

        let output = second_frame(&ctx, |ui| mgr.render_all(ui));

        assert!(texts(&output).contains(&"Pos: (9.0, 8.0, 7.0)".to_string()));
    }
}
