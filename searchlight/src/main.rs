//! The FPS prototype binary: loads a level by name or path and runs the fixed-tick loop.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use moho_app::{AppConfig, LoopConfig, split_fullscreen_binding};
use moho_core::prefs::Prefs;
use moho_input::bindings::ActionBindings;
use moho_ui_shell::UiShell;
use searchlight::arena::{self, Arena};
use searchlight::console_commands;
use searchlight::debug_settings::DebugSettings;
use searchlight::game::{SearchlightGame, WINDOW_TITLE, start_window_mode};
use searchlight::input::build_action_map;
use searchlight::launch;
use searchlight::levels;
use searchlight::mode::ArenaMode;
use searchlight::wave::WaveTuning;
use searchlight::{TICK_HZ, prefs_path, split_console_binding};

/// The folder holding the running executable.
fn executable_folder() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent().map(Path::to_owned).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the executable path has no parent folder",
        )
    })
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let exe_dir = match executable_folder() {
        Ok(dir) => dir,
        Err(error) => {
            tracing::error!(%error, "Executable folder unknown");
            return Ok(ExitCode::FAILURE);
        }
    };
    let install = match launch::locate(&exe_dir) {
        Ok(install) => install,
        Err(error) => {
            tracing::error!(%error, "Shipped folders not found");
            return Ok(ExitCode::FAILURE);
        }
    };
    let launch = match launch::parse(std::env::args_os().skip(1)) {
        Ok(launch) => launch,
        Err(error) => {
            tracing::error!(%error, "Launch arguments refused");
            return Ok(ExitCode::FAILURE);
        }
    };
    let assets = match launch::asset_roots(&launch, &install.root().join("assets")) {
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
    let config = match launch::load_start_kit(&install.root().join("content"), prefs.content_mods())
    {
        Ok(config) => config,
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
    let (level_file, shot_sound) = match launch::resolve_shipped(&choice, &assets) {
        Ok(files) => files,
        Err(error) => {
            tracing::error!(%error, "Asset not found");
            return Ok(ExitCode::FAILURE);
        }
    };
    let (level, markers) = arena::load(&level_file)?;
    let arena = Arc::new(Arena::from_level(&level, &markers)?);
    let notices = levels::load_notices(&arena);
    let mode = ArenaMode::for_level(arena.clone(), config, WaveTuning::default());
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
        .with_menu_cameras(arena.menu_cameras.clone())
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
