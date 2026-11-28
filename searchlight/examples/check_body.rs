//! Checks the character body of an art folder over the placeholders:
//! `cargo run -p searchlight --example check_body -- <art folder>`.
//! With no folder it checks the placeholders alone. Exits non-zero when the body is refused.

use std::error::Error;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use moho_core::assets::AssetRoots;
use searchlight::assets_root;
use searchlight::body::check;
use searchlight::tuning::SHIPPED_BODY;

fn run() -> Result<String, Box<dyn Error>> {
    let mut roots = AssetRoots::placeholders(&assets_root())?;
    if let Some(folder) = std::env::args_os().nth(1) {
        roots = roots.with_folder(Path::new(&folder))?;
    }
    let report = check(&roots, SHIPPED_BODY.standing.eye_height)?;
    Ok(report.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(report) => {
            if writeln!(std::io::stdout(), "{report}").is_err() {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
