//! Application initialization modules.
//!
//! This module contains sub-modules for different aspects of application initialization,
//! making the main App::new() function more modular and testable.

pub mod audio_init;
pub mod autosave;
pub mod camera;
pub mod chunk_streamer;
pub mod config;
pub mod event_loop;
pub mod event_setup;
pub mod generation_job;
pub mod initializer;
pub mod input_state;
pub mod physics_controller;
pub mod renderer_setup;
pub mod scene_loader;
pub mod world_generator;
