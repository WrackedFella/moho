//! The FPS prototype binary: loads a level by name or path and runs the fixed-tick loop.

use std::error::Error;
use std::process::ExitCode;
use std::sync::Arc;

use moho_app::{AppConfig, LoopConfig, split_fullscreen_binding};
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
use searchlight::levels;
use searchlight::mode::ArenaMode;
use searchlight::tuning::tuning;
use searchlight::wave::WaveTuning;
use searchlight::{
    TICK_HZ, assets_root, content_root, mods_dir, prefs_path, split_console_binding,
};
use searchlight_rules::Kit;

/// Loads the starting kit from the base content folder and the listed mod roots.
fn load_start_kit(mods: &[String]) -> Result<Kit, Box<dyn Error>> {
    let roots = ContentRoots::base(&content_root())?.with_mods(&mods_dir(), mods)?;
    Ok(kit::load(&roots, START_KIT_ID)?)
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

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

    let choice = match levels::choose(std::env::args_os().nth(1).as_deref()) {
        Ok(choice) => choice,
        Err(error) => {
            tracing::error!(%error, "Level not found");
            return Ok(ExitCode::FAILURE);
        }
    };
    let (level, markers) = arena::load(&choice.path(&assets_root()))?;
    let arena = Arc::new(Arena::from_level(&level, &markers)?);
    let notices = levels::load_notices(&markers, &arena);
    let mode = ArenaMode::for_level(
        arena.clone(),
        markers.mode,
        tuning(start_kit),
        WaveTuning::default(),
    );
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
        console_commands::console_for(&console_section, markers.mode);
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
        .with_shot_sound(&assets_root().join("rifle_shot.wav"))
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
