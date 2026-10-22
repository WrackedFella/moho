//! The FPS prototype binary: loads the arena and runs the fixed-tick loop.

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use moho_app::{AppConfig, LoopConfig};
use moho_core::content::{ContentError, ContentRoots, load_kind};
use moho_core::prefs::Prefs;
use moho_input::bindings::ActionBindings;
use moho_ui_shell::UiShell;
use searchlight::arena::{self, Arena};
use searchlight::console_commands;
use searchlight::debug_settings::DebugSettings;
use searchlight::game::{SearchlightGame, WINDOW_TITLE};
use searchlight::input::build_action_map;
use searchlight::mode::ArenaMode;
use searchlight::tuning::tuning;
use searchlight::wave::WaveTuning;
use searchlight::{RIFLE_ID, TICK_HZ, content_root, mods_dir, prefs_path, split_console_binding};
use searchlight_rules::RifleDef;

/// Loads the rifle definition from the base content folder and the listed mod roots.
fn load_rifle(mods: &[String]) -> Result<RifleDef, ContentError> {
    let roots = ContentRoots::base(&content_root())?.with_mods(&mods_dir(), mods)?;
    let weapons = load_kind::<RifleDef>(&roots, "weapons")?;
    weapons.require(RIFLE_ID).cloned()
}

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let (prefs, prefs_warnings) = Prefs::load_from(&prefs_path());
    for warning in prefs_warnings {
        tracing::warn!(%warning, "Preferences load fallback");
    }
    let rifle = match load_rifle(prefs.content_mods()) {
        Ok(rifle) => rifle,
        Err(error) => {
            tracing::error!(%error, "Content failed to load");
            return Ok(ExitCode::FAILURE);
        }
    };

    let arena_path = std::env::args_os()
        .nth(1)
        .map_or_else(|| assets().join("arena.glb"), PathBuf::from);
    let (level, markers) = arena::load(&arena_path)?;
    let arena = Arc::new(Arena::from_level(&level, &markers)?);
    let mode = ArenaMode::new(arena.clone(), tuning(&rifle)?, WaveTuning::default());
    let (console_section, game_section) = split_console_binding(prefs.bindings());
    let (bindings, warnings) = ActionBindings::load(&game_section);
    for warning in warnings {
        tracing::warn!(?warning, "bindings");
    }
    let actions = build_action_map(bindings, &prefs);
    let debug_settings = DebugSettings::new(prefs, prefs_path());
    let (console, console_warnings) = console_commands::console(&console_section);
    for warning in console_warnings {
        tracing::warn!(?warning, "console binding");
    }
    let game = SearchlightGame::new(mode, actions)
        .with_shell(UiShell::new(console))
        .with_debug_settings(debug_settings)
        .with_arena(arena.meshes.clone(), &arena.dummy_spawns)
        .with_shot_sound(&assets().join("rifle_shot.wav"))
        .with_main_menu();

    moho_app::run(
        game,
        AppConfig {
            window_title: WINDOW_TITLE.to_owned(),
            loop_config: LoopConfig::new(TICK_HZ),
            init_audio: true,
        },
    )?;
    Ok(ExitCode::SUCCESS)
}
