//! Domos headless command-line interface.

use std::process::ExitCode;

use domos_engine::Session;
use domos_engine::commands::{CreateStorey, CreateWall};
use domos_geom::Point2;
use domos_units::{Length, LengthDisplay, Locale};

const HELP: &str = "\
domos-cli — headless Domos

USAGE:
    domos-cli <COMMAND>

COMMANDS:
    demo        Build a small sample model and print its quantities
    version     Print version
    help        Print this help

Planned: convert, export, serve --stdio, mcp (see docs/control-protocol.md)
";

fn main() -> ExitCode {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "help".to_owned());
    match arg.as_str() {
        "demo" => match demo() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        "version" | "--version" | "-V" => {
            println!("domos-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}\n\n{HELP}");
            ExitCode::FAILURE
        }
    }
}

fn demo() -> Result<(), domos_engine::CommandError> {
    let mut session = Session::default();
    let created = session
        .run(&CreateStorey {
            name: "Ισόγειο".into(), elevation: Length::ZERO, height: Length::m(3.2)
        })?
        .created;
    let Some(&storey) = created.first() else {
        return Ok(());
    };
    let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for (i, &(x, y)) in corners.iter().enumerate() {
        let &(nx, ny) = corners.get((i + 1) % corners.len()).unwrap_or(&(x, y));
        session.run(&CreateWall::centred(
            storey,
            Point2::new(x, y),
            Point2::new(nx, ny),
            Length::cm(25.0),
        ))?;
    }
    let doc = session.document();
    let total = doc.walls_on(storey).fold(Length::ZERO, |acc, w| acc + w.length());
    println!("Walls on storey: {}", doc.walls_on(storey).count());
    println!("Total wall length: {}", LengthDisplay(total, Locale::El));
    println!("Undo steps: {}", session.steps().len());
    Ok(())
}
