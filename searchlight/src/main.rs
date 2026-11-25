//! The FPS prototype binary: loads a level by name or path and runs the fixed-tick loop.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use moho_app::{AppConfig, LoopConfig, split_fullscreen_binding};
use moho_core::assets::{AssetError, AssetRoots};
use moho_core::content::ContentRoots;
use moho_core::prefs::Prefs;
use moho_input::bindings::ActionBindings;
use moho_ui_shell::UiShell;
use searchlight::arena::{self, Arena};
use searchlight::console_commands;
use searchlight::debug_settings::DebugSettings;
use searchlight::game::{SearchlightGame, WINDOW_TITLE, start_window_mode};
use searchlight::input::build_action_map;
use searchlight::kit::{self, START_KIT_ID};
use searchlight::launch;
use searchlight::levels;
use searchlight::mode::ArenaMode;
use searchlight::tuning::tuning;
use searchlight::wave::WaveTuning;
use searchlight::{TICK_HZ, content_root, mods_dir, prefs_path, split_console_binding};
use searchlight_rules::Kit;

/// Loads the starting kit from the base content folder and the listed mod roots.
fn load_start_kit(mods: &[String]) -> Result<Kit, Box<dyn Error>> {
    let roots = ContentRoots::base(&content_root())?.with_mods(&mods_dir(), mods)?;
    Ok(kit::load(&roots, START_KIT_ID)?)
}

/// The level file and the rifle shot sound, each from the highest asset folder holding it.
fn resolve_shipped(
    choice: &levels::LevelChoice,
    assets: &AssetRoots,
) -> Result<(PathBuf, PathBuf), AssetError> {
    Ok((
        choice.path(assets)?,
        assets.resolve(Path::new("rifle_shot.wav"))?,
    ))
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let launch = match launch::parse(std::env::args_os().skip(1)) {
        Ok(launch) => launch,
        Err(error) => {
            tracing::error!(%error, "Launch arguments refused");
            return Ok(ExitCode::FAILURE);
        }
    };
    let assets = match launch::asset_roots(&launch) {
        Ok(assets) => assets,
        Err(error) => {
            tracing::error!(%error, "Asset folder refused");
            return Ok(ExitCode::FAILURE);
        }
    };

    let (prefs, prefs_warnings) = Prefs::load_from(&prefs_path());
    for warning in prefs_warnings {
        tracing::warn!(%warning, "Preferences load fallback");
    }
    let start_kit = match load_start_kit(prefs.content_mods()) {
        Ok(kit) => kit,
        Err(error) => {
            tracing::error!(%error, "Content failed to load");
            return Ok(ExitCode::FAILURE);
        }
    };

    let choice = match levels::choose(launch.level.as_deref()) {
        Ok(choice) => choice,
        Err(error) => {
            tracing::error!(%error, "Level not found");
            return Ok(ExitCode::FAILURE);
        }
    };
    let (level_file, shot_sound) = match resolve_shipped(&choice, &assets) {
        Ok(files) => files,
        Err(error) => {
            tracing::error!(%error, "Asset not found");
            return Ok(ExitCode::FAILURE);
        }
    };
    let (level, markers) = arena::load(&level_file)?;
    let arena = Arc::new(Arena::from_level(&level, &markers)?);
    let notices = levels::load_notices(&arena);
    let mode = ArenaMode::for_level(arena.clone(), tuning(start_kit), WaveTuning::default());
    let (toggle_keys, bindings_section) = split_fullscreen_binding(prefs.bindings());
    let (console_section, game_section) = split_console_binding(&bindings_section);
    let (bindings, warnings) = ActionBindings::load(&game_section);
    for warning in warnings {
        tracing::warn!(?warning, "bindings");
    }
    let actions = build_action_map(bindings, &prefs);
    let window_mode = start_window_mode(prefs.window_mode());
    let debug_settings = DebugSettings::new(prefs, prefs_path());
    let (mut console, console_warnings) =
        console_commands::console_for(&console_section, mode.has_waves());
    for warning in console_warnings {
        tracing::warn!(?warning, "console binding");
    }
    for notice in notices {
        console.print(notice);
        tracing::warn!(notice, "Level loaded with a notice");
    }
    let game = SearchlightGame::new(mode, actions)
        .with_shell(UiShell::new(console))
        .with_debug_settings(debug_settings)
        .with_named_arena(
            arena.meshes.clone(),
            &arena.material_names,
            &arena.dummy_spawns,
        )
        .with_controls(arena.controls.iter().map(|control| control.mesh.clone()))
        .with_shot_sound(&shot_sound)
        .with_main_menu();

    moho_app::run(
        game,
        AppConfig {
            window_title: WINDOW_TITLE.to_owned(),
            loop_config: LoopConfig::new(TICK_HZ),
            init_audio: true,
            toggle_keys,
            window_mode,
        },
    )?;
    Ok(ExitCode::SUCCESS)
}
