//! Kanavos desktop application.

use std::process::ExitCode;

fn main() -> ExitCode {
    match kanavos_ui::run(kanavos_model_starter()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kanavos: {e}");
            ExitCode::FAILURE
        }
    }
}

fn kanavos_model_starter() -> kanavos_ui::Document {
    kanavos_ui::starter_document()
}
