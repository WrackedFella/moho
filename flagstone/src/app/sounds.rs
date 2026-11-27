//! The sound files flagstone plays, each resolved through the asset folders.

use std::path::{Path, PathBuf};

use moho_core::assets::{AssetError, AssetRoots};

const CLICK: &str = "audio/ui/button_click.mp3";
const MENU_MUSIC: &str = "audio/music/menu.ogg";

/// The files flagstone plays: four cues every build ships, and a UI click and menu music a
/// folder may supply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sounds {
    pub pickup: PathBuf,
    pub dig: PathBuf,
    pub mark: PathBuf,
    pub unmark: PathBuf,
    /// `audio/ui/button_click.mp3`, when an asset folder holds it.
    pub click: Option<PathBuf>,
    /// `audio/music/menu.ogg`, when an asset folder holds it.
    pub menu_music: Option<PathBuf>,
}

impl Sounds {
    /// Resolves the four cues and, when present, the click and the menu music.
    ///
    /// # Errors
    /// The [`AssetError`] of a cue found in no folder, or of an optional file that fails for any
    /// reason other than being absent.
    pub fn resolve(assets: &AssetRoots) -> Result<Self, AssetError> {
        Ok(Self {
            pickup: assets.resolve(Path::new("pickup.wav"))?,
            dig: assets.resolve(Path::new("dig.wav"))?,
            mark: assets.resolve(Path::new("mark.wav"))?,
            unmark: assets.resolve(Path::new("unmark.wav"))?,
            click: optional(assets, CLICK)?,
            menu_music: optional(assets, MENU_MUSIC)?,
        })
    }
}

#[cfg(test)]
impl Sounds {
    /// The sounds as resolved from the checkout's placeholder folder, for tests that build an
    /// [`App`](crate::App).
    pub(crate) fn in_checkout() -> Self {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let roots = AssetRoots::placeholders(&folder).expect("the checkout holds its assets");
        Self::resolve(&roots).expect("the checkout holds every cue")
    }
}

/// The file `name` if some folder holds it; `None`, logged with the folders searched, if none
/// does.
fn optional(assets: &AssetRoots, name: &str) -> Result<Option<PathBuf>, AssetError> {
    match assets.resolve(Path::new(name)) {
        Ok(file) => Ok(Some(file)),
        Err(AssetError::NotFound { name, searched }) => {
            let searched: Vec<String> = searched.iter().map(|p| p.display().to_string()).collect();
            tracing::info!(
                asset = %name.display(),
                searched = %searched.join(", "),
                "optional asset not found"
            );
            Ok(None)
        }
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use moho_core::install::{Install, Origin};

    use super::*;

    const CUES: [&str; 4] = ["pickup.wav", "dig.wav", "mark.wav", "unmark.wav"];

    fn checkout() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn placeholders() -> AssetRoots {
        AssetRoots::placeholders(&checkout().join("assets"))
            .expect("the committed placeholder folder exists")
    }

    /// Copies the files under `from` into `to`, skipping `tools` folders and the README, which a
    /// packaged build does not ship.
    fn copy_shipped(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap();
            if path.is_dir() {
                if name != "tools" {
                    copy_shipped(&path, &to.join(name));
                }
            } else if name != "README.md" {
                fs::copy(&path, to.join(name)).unwrap();
            }
        }
    }

    fn copied_install() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        copy_shipped(&checkout().join("assets"), &dir.path().join("assets"));
        dir
    }

    fn folder_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, file).unwrap();
        }
        dir
    }

    fn cues(sounds: &Sounds) -> [&Path; 4] {
        [&sounds.pickup, &sounds.dig, &sounds.mark, &sounds.unmark]
    }

    /// Collects what `tracing` writes while `run` executes.
    fn logged(run: impl FnOnce()) -> String {
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("log sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || Sink(Arc::clone(&writer)))
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, run);
        let bytes = buffer.lock().expect("log sink").clone();
        String::from_utf8(bytes).expect("log output is utf-8")
    }

    #[test]
    fn a_copy_outside_the_checkout_resolves_every_cue() {
        let copy = copied_install();
        let no_checkout = copy.path().join("no-checkout");
        let install = Install::locate(copy.path(), &no_checkout, &["assets"]).unwrap();
        let roots = AssetRoots::placeholders(&install.root().join("assets")).unwrap();

        let sounds = Sounds::resolve(&roots).unwrap();

        assert_eq!(install.origin(), Origin::BesideExecutable);
        for (cue, name) in cues(&sounds).into_iter().zip(CUES) {
            assert_eq!(cue, copy.path().join("assets").join(name));
        }
        assert_eq!(sounds.click, None);
        assert_eq!(sounds.menu_music, None);
    }

    #[test]
    fn resolved_cues_are_absolute() {
        let nothing_beside = tempfile::tempdir().unwrap();
        let install = Install::locate(nothing_beside.path(), &checkout(), &["assets"]).unwrap();
        let roots = AssetRoots::placeholders(&install.root().join("assets")).unwrap();

        let sounds = Sounds::resolve(&roots).unwrap();

        assert_eq!(install.origin(), Origin::SourceCheckout);
        for (cue, name) in cues(&sounds).into_iter().zip(CUES) {
            assert!(cue.is_absolute(), "{cue:?}");
            assert_eq!(cue, checkout().join("assets").join(name));
        }
    }

    #[test]
    fn a_missing_cue_is_an_error_naming_it_and_every_folder() {
        let copy = copied_install();
        let installed = copy.path().join("assets");
        fs::remove_file(installed.join("mark.wav")).unwrap();
        let named = folder_with(&["unrelated.txt"]);
        let roots = AssetRoots::placeholders(&installed)
            .unwrap()
            .with_folder(named.path())
            .unwrap();

        let error = Sounds::resolve(&roots).unwrap_err();

        assert!(
            matches!(
                &error,
                AssetError::NotFound { name, searched }
                    if name == Path::new("mark.wav")
                        && searched == &[named.path().to_owned(), installed.clone()]
            ),
            "{error:?}"
        );
        let text = error.to_string();
        assert!(text.contains("mark.wav"), "{text}");
        assert!(text.contains(&named.path().display().to_string()), "{text}");
        assert!(text.contains(&installed.display().to_string()), "{text}");
    }

    #[test]
    fn click_and_menu_music_come_from_the_assets_folder() {
        let named = folder_with(&["audio/ui/button_click.mp3", "audio/music/menu.ogg"]);
        let roots = placeholders().with_folder(named.path()).unwrap();

        let sounds = Sounds::resolve(&roots).unwrap();

        assert_eq!(
            sounds.click,
            Some(named.path().join("audio/ui/button_click.mp3"))
        );
        assert_eq!(
            sounds.menu_music,
            Some(named.path().join("audio/music/menu.ogg"))
        );
        for (cue, name) in cues(&sounds).into_iter().zip(CUES) {
            assert_eq!(cue, checkout().join("assets").join(name));
        }
    }

    #[test]
    fn a_named_folder_with_one_optional_file_leaves_the_other_none() {
        let named = folder_with(&["audio/music/menu.ogg"]);
        let roots = placeholders().with_folder(named.path()).unwrap();

        let sounds = Sounds::resolve(&roots).unwrap();

        assert_eq!(sounds.click, None);
        assert_eq!(
            sounds.menu_music,
            Some(named.path().join("audio/music/menu.ogg"))
        );
    }

    #[test]
    fn absent_click_and_music_are_none_and_logged() {
        let named = folder_with(&["unrelated.txt"]);
        let roots = placeholders().with_folder(named.path()).unwrap();
        let mut resolved = None;

        let log = logged(|| resolved = Some(Sounds::resolve(&roots)));

        let sounds = resolved.unwrap().unwrap();
        assert_eq!(sounds.click, None);
        assert_eq!(sounds.menu_music, None);
        for asset in ["button_click.mp3", "menu.ogg"] {
            let lines: Vec<&str> = log.lines().filter(|l| l.contains(asset)).collect();
            assert_eq!(lines.len(), 1, "one line for {asset}: {log}");
            assert!(lines[0].contains("INFO"), "{}", lines[0]);
            assert!(
                lines[0].contains(&named.path().display().to_string()),
                "{}",
                lines[0]
            );
            assert!(
                lines[0].contains(&checkout().join("assets").display().to_string()),
                "{}",
                lines[0]
            );
        }
    }
}
