# Domos

**Open-source BIM for architects. Model once; plans, sections, elevations, schedules and permit-ready sheets follow
the model.** Built in Rust, with Greek building regulations built in.

> **Status: pre-alpha.** Architecture, legal process and workspace are in place; nothing is usable yet.
> See [ROADMAP.md](ROADMAP.md).

## Why

- **Fast and calm.** Typed input while drawing, live drawings, an interface that never freezes.
- **Open.** IFC 4.3 in and out, a documented native format, MIT OR Apache‑2.0 code, no lock-in.
- **Made for practice.** ΕΓΣΑ87 surveys, live ΝΟΚ coverage and floor-area meters, area tables and a one-command
  permit drawing set (διάγραμμα κάλυψης, κατόψεις, τομές, όψεις).
- **Scriptable.** Every action is a command, available to scripts and automation clients over a local control
  protocol and an MCP server.
- **Bilingual.** Ελληνικά and English throughout.

Domos opens IFC files exported from GRAPHISOFT® Archicad® and other BIM tools. It is an independent project built
under a documented clean-room process and is not affiliated with any BIM vendor (see [NOTICE](NOTICE)).

## Read first

| Document | What it covers |
| --- | --- |
| [docs/ux.md](docs/ux.md) | UX principles, workspace, interaction model, vocabulary |
| [docs/scope-mvp.md](docs/scope-mvp.md) | What the first release does, and does not do |
| [docs/architecture.md](docs/architecture.md) | Crates, data flow, geometry, files, budgets |
| [docs/adr/](docs/adr) | Architecture decisions |
| [docs/control-protocol.md](docs/control-protocol.md) | Automation API |
| [docs/legal/clean-room.md](docs/legal/clean-room.md) | **Binding** clean-room rules for every contributor |
| [docs/legal/legal-brief.md](docs/legal/legal-brief.md) | EU/Greek legal basis and open questions |
| [AGENTS.md](AGENTS.md) | Working rules for this repository |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |

## Build

```sh
cargo run -p domos            # desktop app (placeholder)
cargo run -p domos-cli -- --help
cargo xtask ci                # fmt, clippy, tests, layering, provenance
```

Requires the Rust toolchain pinned in `rust-toolchain.toml`.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your
option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Domos, as
defined in the Apache‑2.0 license, shall be dual licensed as above, without any additional terms or conditions.

Archicad, BIMcloud, BIMx, GDL and GRAPHISOFT are trademarks of GRAPHISOFT SE, used here only to describe
interoperability.
