# Kanavos

**Kanavos (κάναβος, the structural grid) is open-source BIM for architects. Model once; plans, sections, elevations, schedules and permit-ready sheets follow
the model.** Built in Rust, with Greek building regulations built in.

> **Status: early alpha.** You can model a two-storey house and produce a Greek permit drawing set: walls with
> layered build-ups and clean junctions, slabs, columns, beams, doors and windows, stairs, pitched and flat roofs,
> railings, rooms, furniture; plans, sections and elevations with hidden-line removal; live ΝΟΚ meters, the
> coverage diagram, area tables and schedules; sheets exported to PDF, DXF, SVG and PNG; IFC 4.3 and glTF. Specs
> are written but still awaiting clean-room review (`status: in-review`). See [ROADMAP.md](ROADMAP.md) and the
> [getting-started guide](docs/guide/getting-started.md) ([ελληνικά](docs/guide/odigos.el.md)).

## Why

- **Fast and calm.** Typed input while drawing, live drawings, an interface that never freezes.
- **Open.** IFC 4.3 in and out, a documented native format, MIT OR Apache‑2.0 code, no lock-in.
- **Made for practice.** ΕΓΣΑ87 surveys, live ΝΟΚ coverage and floor-area meters, area tables and a one-command
  permit drawing set (διάγραμμα κάλυψης, κατόψεις, τομές, όψεις).
- **Scriptable.** Every action is a command, available to scripts and automation clients over a local control
  protocol and an MCP server.
- **Bilingual.** Ελληνικά and English throughout.

Kanavos opens IFC files exported from GRAPHISOFT® Archicad® and other BIM tools. It is an independent project built
under a documented clean-room process and is not affiliated with any BIM vendor (see [NOTICE](NOTICE)).

## Read first

| Document | What it covers |
| --- | --- |
| [docs/guide/getting-started.md](docs/guide/getting-started.md) | Getting started ([ελληνικά](docs/guide/odigos.el.md)) |
| [docs/ux.md](docs/ux.md) | UX principles, workspace, interaction model, vocabulary |
| [docs/scope-mvp.md](docs/scope-mvp.md) | What the first release does, and does not do |
| [docs/architecture.md](docs/architecture.md) | Crates, data flow, geometry, files, budgets |
| [docs/plan/full-parity.el.md](docs/plan/full-parity.el.md) | Full-functionality plan: feature map, subsystems, phases, team (Greek) |
| [docs/adr/](docs/adr) | Architecture decisions |
| [docs/control-protocol.md](docs/control-protocol.md) | Automation API ([commands](docs/commands.md)) |
| [docs/updates.md](docs/updates.md) | Updates: what the check sends, how to turn it off, the signing scheme |
| [docs/format.md](docs/format.md), [docs/formats/](docs/formats) | The `.kanavos` format; IFC, DXF, PDF, glTF notes |
| [docs/legal/clean-room.md](docs/legal/clean-room.md) | **Binding** clean-room rules for every contributor |
| [docs/legal/legal-brief.md](docs/legal/legal-brief.md) | EU/Greek legal basis and open questions |
| [AGENTS.md](AGENTS.md) | Working rules for this repository |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |

## Build

```sh
cargo run --release -p kanavos                 # desktop app (opens a sample house on first launch)
cargo run --release -p kanavos-cli -- help     # headless: new, sample, info, export, run, serve, mcp
cargo xtask ci                                 # fmt, clippy, tests, layering, provenance
cargo deny check licenses bans sources         # licence policy
```

On Windows, Rust's MSVC toolchain needs the Visual Studio C++ build tools. The interface uses an installed system
font with Greek coverage (no fonts are bundled); set `KANAVOS_FONT` to choose one.

Requires the Rust toolchain pinned in `rust-toolchain.toml`.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your
option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Kanavos, as
defined in the Apache‑2.0 license, shall be dual licensed as above, without any additional terms or conditions.

Archicad, BIMcloud, BIMx, GDL and GRAPHISOFT are trademarks of GRAPHISOFT SE, used here only to describe
interoperability.
