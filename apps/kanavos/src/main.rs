//! Kanavos desktop application.
//!
//! Usage: `kanavos [project.kanavos]`. Without a file, the first launch opens the sample house and later
//! launches start a new project.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let file = std::env::args_os().nth(1).map(PathBuf::from);
    let startup = kanavos_ui::Startup { file, document: None };
    match kanavos_ui::run(startup) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kanavos: {e}");
            ExitCode::FAILURE
        }
    }
}
