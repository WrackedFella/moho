//! What the tester named on the command line, and the asset folders it makes.

use std::error::Error as StdError;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use moho_core::assets::{AssetError, AssetRoots};
use moho_core::content::ContentRoots;
use moho_core::install::{Install, InstallError};
use thiserror::Error;

use crate::kit::{ENEMY_KIT_ID, START_KIT_ID};
use crate::levels::LevelChoice;
use crate::sim::SimConfig;
use crate::tuning::shipped_with_enemy;
use crate::{SHIPPED_FOLDERS, checkout_root, mods_dir};

const ASSETS_OPTION: &str = "--assets";

/// The launch arguments after the program name.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// The level argument: a name or a glTF path, as typed.
    pub level: Option<OsString>,
    /// The folder named by `--assets`, whose files override the placeholders.
    pub assets: Option<PathBuf>,
}

/// A launch argument that cannot be used.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum LaunchError {
    /// `--assets` was the last argument.
    #[error("`--assets` needs a folder after it")]
    MissingValue,
    /// `--assets` was given more than once.
    #[error("`--assets` was given more than once")]
    Repeated,
}

/// Reads `--assets <folder>` and the first other argument as the level; later arguments are
/// ignored.
///
/// # Errors
/// [`LaunchError::MissingValue`] for `--assets` with nothing after it;
/// [`LaunchError::Repeated`] for a second `--assets`.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Launch, LaunchError> {
    let mut launch = Launch::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == ASSETS_OPTION {
            if launch.assets.is_some() {
                return Err(LaunchError::Repeated);
            }
            let folder = args.next().ok_or(LaunchError::MissingValue)?;
            launch.assets = Some(PathBuf::from(folder));
        } else if launch.level.is_none() {
            launch.level = Some(arg);
        }
    }
    Ok(launch)
}

/// The install holding the shipped folders: beside the executable at `exe_dir`, else the
/// source checkout.
///
/// # Errors
/// [`InstallError::Incomplete`] when `exe_dir` holds some but not all shipped folders (no
/// fallback to the checkout); [`InstallError::NotFound`] when it holds none and the checkout
/// lacks one.
pub fn locate(exe_dir: &Path) -> Result<Install, InstallError> {
    Install::locate(exe_dir, &checkout_root(), SHIPPED_FOLDERS)
}

/// The placeholder folder, then the named folder if any. The only place an override layer is
/// built.
///
/// # Errors
/// [`AssetError::MissingFolder`] naming a folder that does not exist;
/// [`AssetError::NotAFolder`] naming a path that is a file.
pub fn asset_roots(launch: &Launch, placeholders: &Path) -> Result<AssetRoots, AssetError> {
    let roots = AssetRoots::placeholders(placeholders)?;
    match &launch.assets {
        Some(folder) => roots.with_folder(folder),
        None => Ok(roots),
    }
}

/// The level file and the rifle shot sound, each from the highest asset folder holding it.
///
/// # Errors
/// [`AssetError`] when either file is in no asset folder.
pub fn resolve_shipped(
    choice: &LevelChoice,
    assets: &AssetRoots,
) -> Result<(PathBuf, PathBuf), AssetError> {
    Ok((
        choice.path(assets)?,
        assets.resolve(Path::new("rifle_shot.wav"))?,
    ))
}

/// The sim tuning built from the starting kit and the enemy kit, both loaded from the
/// `content` folder and the listed mod roots.
///
/// # Errors
/// The content or kit load failure.
pub fn load_start_kit(content: &Path, mods: &[String]) -> Result<SimConfig, Box<dyn StdError>> {
    let roots = ContentRoots::base(content)?.with_mods(&mods_dir(), mods)?;
    Ok(shipped_with_enemy(&roots, START_KIT_ID, ENEMY_KIT_ID)?)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::levels::{self, LevelChoice};
    use crate::test_support::{logged, placeholder_assets};
    use crate::{arena, assets_root, content_root};
    use moho_core::install::Origin;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn folder_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            fs::write(dir.path().join(file), file).unwrap();
        }
        dir
    }

    fn launch_naming(dir: &Path) -> Launch {
        parse([
            OsString::from("--assets"),
            dir.as_os_str().to_owned(),
            OsString::from("arena"),
        ])
        .expect("a folder and a level parse")
    }

    #[test]
    fn level_only_keeps_todays_argument() {
        let none = parse(os(&[])).unwrap();
        let one = parse(os(&["proving_ground"])).unwrap();
        let extra = parse(os(&["arena", "ignored", "also"])).unwrap();

        assert_eq!(none, Launch::default());
        assert_eq!(
            one,
            Launch {
                level: Some("proving_ground".into()),
                assets: None
            }
        );
        assert_eq!(
            extra,
            Launch {
                level: Some("arena".into()),
                assets: None
            }
        );
    }

    #[test]
    fn assets_then_level_and_level_then_assets_agree() {
        let assets_first = parse(os(&["--assets", "mine", "arena"])).unwrap();
        let level_first = parse(os(&["arena", "--assets", "mine"])).unwrap();
        let assets_only = parse(os(&["--assets", "mine"])).unwrap();

        let expected = Launch {
            level: Some("arena".into()),
            assets: Some(PathBuf::from("mine")),
        };
        assert_eq!(assets_first, expected);
        assert_eq!(level_first, expected);
        assert_eq!(
            assets_only,
            Launch {
                level: None,
                assets: Some(PathBuf::from("mine"))
            }
        );
    }

    #[test]
    fn assets_without_value_is_refused() {
        let alone = parse(os(&["--assets"]));
        let after_level = parse(os(&["arena", "--assets"]));

        assert_eq!(alone, Err(LaunchError::MissingValue));
        assert_eq!(after_level, Err(LaunchError::MissingValue));
    }

    #[test]
    fn repeated_assets_is_refused() {
        let repeated = parse(os(&["--assets", "a", "--assets", "b"]));
        let split = parse(os(&["--assets", "a", "arena", "--assets", "b"]));

        assert_eq!(repeated, Err(LaunchError::Repeated));
        assert_eq!(split, Err(LaunchError::Repeated));
    }

    #[test]
    fn the_assets_value_is_never_taken_as_the_level() {
        let value_only = parse(os(&["--assets", "arena"])).unwrap();
        let mixed = parse(os(&["a", "--assets", "d", "b"])).unwrap();

        assert_eq!(
            value_only,
            Launch {
                level: None,
                assets: Some(PathBuf::from("arena"))
            }
        );
        assert_eq!(
            mixed,
            Launch {
                level: Some("a".into()),
                assets: Some(PathBuf::from("d"))
            }
        );
    }

    #[test]
    fn launch_errors_name_the_option() {
        assert!(LaunchError::MissingValue.to_string().contains("--assets"));
        assert!(LaunchError::Repeated.to_string().contains("--assets"));
    }

    #[test]
    fn named_folder_supplies_its_arena_and_placeholders_the_shot() {
        let dir = folder_with(&["arena.glb"]);
        let launch = launch_naming(dir.path());
        let roots = asset_roots(&launch, &assets_root()).unwrap();
        let choice = levels::choose(launch.level.as_deref()).unwrap();

        let level = choice.path(&roots).unwrap();
        let shot = roots.resolve(Path::new("rifle_shot.wav")).unwrap();

        assert!(matches!(choice, LevelChoice::Named(_)));
        assert_eq!(level, dir.path().join("arena.glb"));
        assert_eq!(shot, assets_root().join("rifle_shot.wav"));
    }

    #[test]
    fn resolving_logs_the_layer_and_folder_of_each_asset() {
        let dir = folder_with(&["arena.glb"]);
        let roots = asset_roots(&launch_naming(dir.path()), &assets_root()).unwrap();

        let log = logged(|| {
            roots.resolve(Path::new("arena.glb")).unwrap();
            roots.resolve(Path::new("rifle_shot.wav")).unwrap();
        });

        let line_of = |asset: &str| {
            let lines: Vec<&str> = log.lines().filter(|l| l.contains(asset)).collect();
            assert_eq!(lines.len(), 1, "one line for {asset}: {log}");
            lines[0]
        };
        let arena = line_of("arena.glb");
        let shot = line_of("rifle_shot.wav");
        assert!(arena.contains("INFO"), "{arena}");
        assert!(arena.contains("assets folder"), "{arena}");
        assert!(arena.contains(&dir.path().display().to_string()), "{arena}");
        assert!(!arena.contains("placeholders"), "{arena}");
        assert!(shot.contains("INFO"), "{shot}");
        assert!(shot.contains("placeholders"), "{shot}");
        assert!(
            shot.contains(&assets_root().display().to_string()),
            "{shot}"
        );
        assert!(!shot.contains("assets folder"), "{shot}");
    }

    #[test]
    fn missing_assets_folder_is_an_error_naming_it() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("no_such_folder");
        let launch = parse([OsString::from("--assets"), missing.as_os_str().to_owned()]).unwrap();

        let error = asset_roots(&launch, &assets_root()).unwrap_err();

        assert!(
            matches!(&error, AssetError::MissingFolder { path } if path == &missing),
            "{error:?}"
        );
        assert!(
            error.to_string().contains(&missing.display().to_string()),
            "{error}"
        );
    }

    #[test]
    fn no_assets_option_searches_placeholders_only() {
        let roots = asset_roots(&Launch::default(), &assets_root()).unwrap();

        let arena = roots.resolve(Path::new("arena.glb")).unwrap();

        assert_eq!(arena, assets_root().join("arena.glb"));
        assert_eq!(roots, placeholder_assets());
    }

    /// Copies the files under `from` into `to`, keeping the layout and skipping `tools`
    /// folders, which a packaged build does not ship.
    fn copy_shipped(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap();
            if path.is_dir() {
                if name != "tools" {
                    copy_shipped(&path, &to.join(name));
                }
            } else {
                fs::copy(&path, to.join(name)).unwrap();
            }
        }
    }

    /// A folder holding copies of the checkout's `assets` and `content` files, as a packaged
    /// build lays them out.
    fn copied_install() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        copy_shipped(&assets_root(), &dir.path().join("assets"));
        copy_shipped(&content_root(), &dir.path().join("content"));
        dir
    }

    const THREE_MAGAZINES: &str = "magazines = [
    { magazine = \"mag_rifle\", round = \"round_rifle_ball\" },
    { magazine = \"mag_rifle\", round = \"round_rifle_ball\" },
    { magazine = \"mag_rifle\", round = \"round_rifle_ball\" },
]";

    const TWO_MAGAZINES: &str = "magazines = [
    { magazine = \"mag_rifle\", round = \"round_rifle_ball\" },
    { magazine = \"mag_rifle\", round = \"round_rifle_ball\" },
]";

    #[test]
    fn a_copy_outside_the_checkout_starts_from_its_own_folders() {
        let copy = copied_install();
        let kits = copy.path().join("content").join("kits.toml");
        let original = fs::read_to_string(&kits).unwrap();
        let edited = original.replacen(THREE_MAGAZINES, TWO_MAGAZINES, 1);
        assert_ne!(
            edited, original,
            "the start kit's three magazines were not found"
        );
        fs::write(&kits, edited).unwrap();
        let no_checkout = copy.path().join("no-checkout");

        let install = Install::locate(copy.path(), &no_checkout, SHIPPED_FOLDERS).unwrap();
        let assets = asset_roots(&Launch::default(), &install.root().join("assets")).unwrap();
        let config = load_start_kit(&install.root().join("content"), &[]).unwrap();

        assert_eq!(install.root(), copy.path());
        assert_eq!(config.loadout.rig().magazines().count(), 2);
        for name in ["arena", "proving_ground"] {
            let choice = levels::choose(Some(OsString::from(name).as_os_str())).unwrap();

            let (level, shot) = resolve_shipped(&choice, &assets).unwrap();

            assert!(level.starts_with(copy.path()), "{level:?}");
            assert!(shot.starts_with(copy.path()), "{shot:?}");
            arena::load(&level).unwrap();
        }
    }

    #[test]
    fn an_assets_folder_overrides_a_copied_install() {
        let copy = copied_install();
        let folder = folder_with(&["arena.glb"]);
        let launch = launch_naming(folder.path());
        let install = Install::locate(
            copy.path(),
            &copy.path().join("no-checkout"),
            SHIPPED_FOLDERS,
        )
        .unwrap();
        let assets = asset_roots(&launch, &install.root().join("assets")).unwrap();
        let choice = levels::choose(launch.level.as_deref()).unwrap();

        let (level, shot) = resolve_shipped(&choice, &assets).unwrap();

        assert_eq!(level, folder.path().join("arena.glb"));
        assert_eq!(shot, copy.path().join("assets").join("rifle_shot.wav"));
    }

    #[test]
    fn a_build_missing_its_content_folder_is_refused_not_borrowed_from_the_checkout() {
        let copy = copied_install();
        fs::remove_dir_all(copy.path().join("content")).unwrap();

        let error = locate(copy.path()).unwrap_err();

        assert!(
            matches!(
                &error,
                InstallError::Incomplete { root, missing }
                    if root == copy.path() && missing == &["content"]
            ),
            "{error:?}"
        );
    }

    #[test]
    fn an_executable_folder_with_nothing_in_it_starts_from_the_checkout() {
        let empty = tempfile::tempdir().unwrap();

        let install = locate(empty.path()).unwrap();

        assert_eq!(install.root(), checkout_root());
        assert_eq!(install.origin(), Origin::SourceCheckout);
    }

    #[test]
    fn a_file_named_as_the_assets_folder_is_an_error_saying_so() {
        let holder = folder_with(&["plain.txt"]);
        let file = holder.path().join("plain.txt");
        let launch = parse([OsString::from("--assets"), file.as_os_str().to_owned()]).unwrap();

        let error = asset_roots(&launch, &assets_root()).unwrap_err();

        assert!(
            matches!(&error, AssetError::NotAFolder { path } if path == &file),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(text.contains("not a folder"), "{text}");
        assert!(text.contains(&file.display().to_string()), "{text}");
    }
}
