//! The launch options every game line reads the same way.

use std::ffi::OsString;
use std::path::PathBuf;

use thiserror::Error;

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

/// Takes `--assets <folder>` out of `args` and returns the folder with the other arguments, in
/// their order.
///
/// # Errors
/// [`LaunchError::MissingValue`] for `--assets` with nothing after it;
/// [`LaunchError::Repeated`] for a second `--assets`.
pub fn split_assets_option(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(Option<PathBuf>, Vec<OsString>), LaunchError> {
    let _ = args;
    todo!("split_assets_option")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_option_keeps_every_argument() {
        let none = split_assets_option(os(&[])).unwrap();
        let some = split_assets_option(os(&["arena", "ignored", "also"])).unwrap();

        assert_eq!(none, (None, os(&[])));
        assert_eq!(some, (None, os(&["arena", "ignored", "also"])));
    }

    #[test]
    fn option_before_and_after_other_arguments_agree() {
        let before = split_assets_option(os(&["--assets", "mine", "arena", "extra"])).unwrap();
        let after = split_assets_option(os(&["arena", "extra", "--assets", "mine"])).unwrap();
        let between = split_assets_option(os(&["arena", "--assets", "mine", "extra"])).unwrap();
        let alone = split_assets_option(os(&["--assets", "mine"])).unwrap();

        let expected = (Some(PathBuf::from("mine")), os(&["arena", "extra"]));
        assert_eq!(before, expected);
        assert_eq!(after, expected);
        assert_eq!(between, expected);
        assert_eq!(alone, (Some(PathBuf::from("mine")), os(&[])));
    }

    #[test]
    fn the_option_value_is_never_left_among_the_rest() {
        let value_named_like_the_option = split_assets_option(os(&["--assets", "arena"])).unwrap();

        assert_eq!(
            value_named_like_the_option,
            (Some(PathBuf::from("arena")), os(&[]))
        );
    }

    #[test]
    fn option_without_value_is_refused() {
        let alone = split_assets_option(os(&["--assets"]));
        let after_other = split_assets_option(os(&["arena", "--assets"]));

        assert_eq!(alone, Err(LaunchError::MissingValue));
        assert_eq!(after_other, Err(LaunchError::MissingValue));
    }

    #[test]
    fn repeated_option_is_refused() {
        let repeated = split_assets_option(os(&["--assets", "a", "--assets", "b"]));
        let split = split_assets_option(os(&["--assets", "a", "arena", "--assets", "b"]));

        assert_eq!(repeated, Err(LaunchError::Repeated));
        assert_eq!(split, Err(LaunchError::Repeated));
    }

    #[test]
    fn launch_errors_name_the_option() {
        assert!(LaunchError::MissingValue.to_string().contains("--assets"));
        assert!(LaunchError::Repeated.to_string().contains("--assets"));
    }
}
