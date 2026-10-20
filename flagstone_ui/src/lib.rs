//! Modern, modular UI system for the Moho game engine.
//!
//! This crate provides a flexible, scalable UI system based on egui with support
//! for multiple menu types and easy extensibility for new UI components.

/// Stub UI implementation for when no UI features are enabled
#[derive(Debug)]
pub struct StubUi;

impl StubUi {
    pub fn new() -> Self {
        StubUi {}
    }
}

impl Default for StubUi {
    fn default() -> Self {
        Self::new()
    }
}

impl moho_render_api::UiFrameSource for StubUi {
    fn ui_frame(&mut self, _size_in_pixels: [u32; 2]) -> Option<moho_render_api::UiFrame> {
        None
    }
}

// Modern egui-based UI system
pub mod actions;
pub mod adapter;

pub mod console_commands;

pub mod app_state;

pub mod screens;

pub mod overlays;

pub mod modal;

pub mod modals;

pub mod input_handling;

pub mod ui_state;

// Re-export the main types for easy access
pub use adapter::{
    EguiAdapter, UI_OVERLAY_VISIBLE, UiAudioEvent, UiEvent, UiReceiver, build_adapter,
};

pub use adapter::EguiAdapter as EguiUi;

pub use app_state::{GameState, StateTransitionActions, StateTransitionCoordinator};

pub use screens::{
    FormControls, Menu, MenuAction, MenuItem, NewWorldMenu, PauseMenu, Screen, ScreenSpec,
    SettingsMenu, StartMenu, UiComponent,
};

pub use overlays::{
    ChunkDebugOverlay, DebugHud, FpsHud, GameplayHud, HudData, Overlay, OverlayManager, RtsHud,
};

pub use modal::{Modal, ModalManager, ModalResult};

pub use modals::KeybindConflictModal;

pub use moho_core::prefs;

#[cfg(test)]
mod tests {
    use super::StubUi;
    use moho_render_api::UiFrameSource;

    #[test]
    fn stub_ui_paints_nothing() {
        assert!(StubUi::new().ui_frame([800, 600]).is_none());
    }
}
