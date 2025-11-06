//! Shared types used across the moho workspace.
//!
//! This crate provides common types and structures that need to be shared
//! between the main application and integration tests, as well as between
//! different moho crates.

pub mod app_state;

pub use app_state::{AppState, GameState};
