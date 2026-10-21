//! The FPS prototype binary: loads the arena and runs the fixed-tick loop.

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;

use moho_app::{AppConfig, LoopConfig};
use moho_core::prefs::Prefs;
use moho_input::action_map::ActionMap;
use moho_input::bindings::ActionBindings;
use moho_physics::MoveSettings;
use moho_ui_shell::UiShell;
use searchlight::arena::{self, Arena};
use searchlight::brain::EnemyTuning;
use searchlight::console_commands;
use searchlight::debug_settings::DebugSettings;
use searchlight::game::{SearchlightGame, WINDOW_TITLE};
use searchlight::mode::ArenaMode;
use searchlight::sim::SimConfig;
use searchlight::wave::WaveTuning;
use searchlight::{TICK_HZ, prefs_path, split_console_binding};
use searchlight_rules::{BodyPart, BodyProfile, Magazine, Rifle};

/// Radians of turn per pixel of mouse motion.
const LOOK_PER_PIXEL: f32 = 0.002;
const MAGAZINE_CAPACITY: u32 = 30;
const SPARE_MAGAZINES: usize = 3;

fn tuning() -> Result<SimConfig, Box<dyn Error>> {
    let full = Magazine::new(MAGAZINE_CAPACITY, MAGAZINE_CAPACITY)?;
    Ok(SimConfig {
        move_settings: MoveSettings {
            max_speed: 6.0,
            ground_accel: 80.0,
            friction: 10.0,
            stop_speed: 1.0,
            air_accel: 20.0,
            air_speed_cap: 1.0,
            gravity: 18.0,
            jump_speed: 6.5,
        },
        eye_height: 0.7,
        range: 100.0,
        damage: 30,
        cooldown_ticks: 6,
        reload_ticks: 120,
        body: BodyProfile {
            max_health: 100,
            head_status: 30,
            arm_status: 60,
            leg_status: 60,
        },
        rifle: Rifle::loaded(full.clone()),
        spares: vec![full; SPARE_MAGAZINES],
        enemy: EnemyTuning::default(),
        enemy_aim_at: BodyPart::Torso,
    })
}

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let arena_path = std::env::args_os()
        .nth(1)
        .map_or_else(|| assets().join("arena.glb"), PathBuf::from);
    let (level, markers) = arena::load(&arena_path)?;
    let arena = Arc::new(Arena::from_level(&level, &markers)?);
    let mode = ArenaMode::new(arena.clone(), tuning()?, WaveTuning::default());
    let (prefs, prefs_warnings) = Prefs::load_from(&prefs_path());
    for warning in prefs_warnings {
        tracing::warn!(%warning, "Preferences load fallback");
    }
    let (console_section, game_section) = split_console_binding(prefs.bindings());
    let debug_settings = DebugSettings::new(prefs, prefs_path());
    let (bindings, warnings) = ActionBindings::load(&game_section);
    for warning in warnings {
        tracing::warn!(?warning, "bindings");
    }
    let (console, console_warnings) = console_commands::console(&console_section);
    for warning in console_warnings {
        tracing::warn!(?warning, "console binding");
    }
    let game = SearchlightGame::new(mode, ActionMap::new(bindings, LOOK_PER_PIXEL))
        .with_shell(UiShell::new(console))
        .with_debug_settings(debug_settings)
        .with_arena(arena.meshes.clone(), &arena.dummy_spawns)
        .with_shot_sound(&assets().join("rifle_shot.wav"));

    moho_app::run(
        game,
        AppConfig {
            window_title: WINDOW_TITLE.to_owned(),
            loop_config: LoopConfig::new(TICK_HZ),
            init_audio: true,
        },
    )?;
    Ok(())
}
