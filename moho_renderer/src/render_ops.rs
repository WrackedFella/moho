//! Render operation modules for extracting rendering logic from the main Renderer.
//!
//! This module contains focused submodules for different rendering operations:
//! - `camera_ops`: Camera uniform buffer updates
//! - `shadow_ops`: Shadow matrix calculation and shadow pass rendering
//! - `main_pass_ops`: Main color pass rendering with skybox
//! - `frame_ops`: Frame finalization and presentation

pub mod camera_ops;
pub mod frame_ops;
pub mod main_pass_ops;
pub mod shadow_ops;
