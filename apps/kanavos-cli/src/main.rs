//! Kanavos headless command-line interface.

use std::process::ExitCode;

use kanavos_engine::Session;
use kanavos_engine::commands::CreateElement;
use kanavos_geom::Point2;
use kanavos_model::{ElementKind, ReferenceLine, Structure, Wall, starter};
use kanavos_units::{Length, LengthDisplay, Locale};

const HELP: &str = "\
kanavos-cli — headless Kanavos

USAGE:
    kanavos-cli <COMMAND>

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
            println!("kanavos-cli {}", env!("CARGO_PKG_VERSION"));
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

fn demo() -> Result<(), kanavos_engine::CommandError> {
    let mut session = Session::new(starter());
    let doc = session.document().clone();
    let Some(storey) = doc.storeys().first().map(|s| s.id) else { return Ok(()) };
    let Some(build_up) = doc.defaults().exterior_wall else { return Ok(()) };
    let corners = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    for (i, &(x, y)) in corners.iter().enumerate() {
        let &(nx, ny) = corners.get((i + 1) % corners.len()).unwrap_or(&(x, y));
        session.run(CreateElement::on(
            storey,
            ElementKind::Wall(Wall {
                start: Point2::new(x, y),
                end: Point2::new(nx, ny),
                bulge: 0.0,
                reference: ReferenceLine::Centre,
                structure: Structure::Layered { build_up },
                height: Length::m(3.2),
                base_offset: Length::ZERO,
            }),
        ))?;
    }
    let doc = session.document();
    let total = doc.walls_on(storey).fold(Length::ZERO, |acc, (_, w)| acc + w.length());
    println!("Walls on storey: {}", doc.walls_on(storey).count());
    println!("Total wall length: {}", LengthDisplay(total, Locale::El));
    println!("Undo steps: {}", session.steps().len());
    Ok(())
}
