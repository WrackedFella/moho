//! Overlay management system for layered HUD elements.
//!
//! Provides a generic [`Overlay`] trait and an [`OverlayManager`] that
//! renders registered overlays each frame. Each overlay receives shared
//! [`HudData`] and decides internally whether to draw based on its
//! visibility flag and the current camera mode.

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

    /// Render the overlay into the egui context.
    fn render(&mut self, ctx: &egui::Context, data: &HudData);

    /// Whether this overlay is currently enabled (user toggle).
    fn is_visible(&self) -> bool;

    /// Set visibility (user toggle).
    fn set_visible(&mut self, visible: bool);

    /// Toggle visibility.
    fn toggle(&mut self) {
        let v = self.is_visible();
        self.set_visible(!v);
    }
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
                &self
                    .overlays
                    .iter()
                    .map(|o| o.name())
                    .collect::<Vec<_>>(),
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

    /// Render every registered overlay (each self-filters on visibility/mode).
    pub fn render_all(&mut self, ctx: &egui::Context) {
        for overlay in &mut self.overlays {
            overlay.render(ctx, &self.hud_data);
        }
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

    /// Read-only access to the current HUD data.
    pub fn hud_data(&self) -> &HudData {
        &self.hud_data
    }

    /// Mutable access to the current HUD data.
    pub fn hud_data_mut(&mut self) -> &mut HudData {
        &mut self.hud_data
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
        visible: bool,
    }

    impl Overlay for StubOverlay {
        fn name(&self) -> &str {
            "stub"
        }
        fn render(&mut self, _ctx: &egui::Context, _data: &HudData) {}
        fn is_visible(&self) -> bool {
            self.visible
        }
        fn set_visible(&mut self, visible: bool) {
            self.visible = visible;
        }
    }

    #[test]
    fn toggle_overlay_by_name() {
        let mut mgr = OverlayManager::new();
        mgr.register(Box::new(StubOverlay { visible: false }));
        assert!(!mgr.overlays[0].is_visible());

        mgr.toggle("stub");
        assert!(mgr.overlays[0].is_visible());

        mgr.toggle("stub");
        assert!(!mgr.overlays[0].is_visible());
    }

    #[test]
    fn toggle_unknown_name_is_noop() {
        let mut mgr = OverlayManager::new();
        mgr.toggle("nonexistent"); // should not panic
    }

    #[test]
    fn update_data_replaces_snapshot() {
        let mut mgr = OverlayManager::new();
        assert!((mgr.hud_data().time_of_day - 12.0).abs() < f32::EPSILON);

        let mut data = HudData::default();
        data.time_of_day = 18.5;
        mgr.update_data(data);
        assert!((mgr.hud_data().time_of_day - 18.5).abs() < f32::EPSILON);
    }
}
