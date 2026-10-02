//! Modern, modular UI system for the Moho game engine.
//!
//! This crate provides a flexible, scalable UI system based on egui with support
//! for multiple menu types and easy extensibility for new UI components.

// egui 0.34 deprecated `Context::run`/`Panel::show`/`CentralPanel::show` in favor
// of `run_ui`/`show_inside`, which take a `&mut Ui` instead of a `&Context` — every
// screen and overlay in this crate is built around the Context-driven pattern, so
// migrating is a UI-architecture change (how screens obtain their root `Ui`), not a
// mechanical rename. Deferred as a follow-up; tracked in
// _todo/tech-debt/dependency-upgrades.md rather than fixed as a side effect of the
// wgpu/egui version bump.
#![allow(deprecated)]

use moho_renderer::FrameCallback;

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

impl FrameCallback for StubUi {
    fn call(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _view: &wgpu::TextureView,
        _encoder: &mut wgpu::CommandEncoder,
        _surface_width: u32,
        _surface_height: u32,
    ) {
        // no-op when no UI is available
    }
}

// Modern egui-based UI system
pub mod adapter;

pub mod screens;

pub mod overlays;

pub mod modal;

pub mod modals;

pub mod input_handling;

pub mod ui_state;

// Re-export the main types for easy access
pub use adapter::{
    EguiAdapter, GameState, UI_OVERLAY_VISIBLE, UiAudioEvent, UiEvent, UiReceiver, build_adapter,
};

pub use adapter::EguiAdapter as EguiUi;

pub use screens::{
    FormControls, Menu, MenuAction, MenuItem, NewWorldMenu, Screen, ScreenSpec, SettingsMenu,
    StartMenu, UiComponent,
};

pub use overlays::{
    ChunkDebugOverlay, Console, ConsoleAction, DebugHud, FpsHud, GameplayHud, HudData, Overlay,
    OverlayManager, RtsHud,
};

pub use modal::{Modal, ModalManager, ModalResult};

pub use modals::KeybindConflictModal;

pub use moho_core::prefs;
