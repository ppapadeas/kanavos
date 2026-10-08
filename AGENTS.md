# AGENTS.md

Working rules for anyone changing this repository, people and coding tools alike. Read
[`docs/legal/clean-room.md`](docs/legal/clean-room.md) before anything else; it overrides everything below.

## 1. Clean room (non-negotiable)

- Build features only from **approved** specs in `specs/`, open standards (IFC, STEP, DXF, PDF, SVG, glTF, EPSG),
  Greek/EU regulations, and published research.
- Never use, quote, paraphrase or imitate: commercial BIM product manuals, help text, UI strings, icons, screen
  layouts, library objects, templates, SDKs, add-on code or file internals.
- Never decompile, disassemble or byte-inspect any commercial BIM binary or project/library file.
- Never use vendor names for tools, panels or settings in code, UI strings, specs or docs. Use the Domos vocabulary in
  [`docs/ux.md` §7](docs/ux.md#7-vocabulary). Vendor trademarks may appear only in `NOTICE`, the legal docs, and
  factual interoperability statements.
- Never paste code or text whose source and licence you cannot name. Record third-party code in `NOTICE`.
- If a task would need any of the above, stop and ask instead.

## 2. Commits

- `git commit -s` (DCO) and a `Clean-Room:` trailer (`yes`, `same-person` or `n/a`); feature work adds `Spec:`.
- One logical change per commit; message subject `<crate or area>: <imperative summary>`.
- Commit messages and code contain no attribution to tools.

## 3. Layering

- Crates live in layers L0–L10 (`docs/architecture.md` §2). A crate depends only on lower layers.
- New crate → add it to the layer table in `xtask/src/layers.rs` and to `docs/architecture.md`.
- `domos-ui` contains no model or geometry logic. `domos-model` contains no I/O.

## 4. Code rules

- Rust edition and toolchain from `rust-toolchain.toml`. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`.
- Shipped crates: `#![forbid(unsafe_code)]` (exceptions only in `domos-render` and FFI crates, reviewed), and no
  `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or indexing that can panic on user data. Return typed errors.
- All changes to a document go through a `Command` (ADR‑0005). No direct mutation from tools or UI.
- Lengths are `domos_units::Length` (metres, f64) with the project tolerance; never compare floats with `==`.
- Geometry functions document their tolerance and failure modes.
- Public items have doc comments. Commands have JSON schemas and examples.
- User-facing strings go through the i18n catalogue, with both `el` and `en` entries in the same change.

## 5. Tests

- Unit tests next to code; integration tests in `tests/`.
- Geometry and view derivation: golden tests in `domos-testkit` (SVG output compared with tolerance).
- File formats: round-trip property tests.
- Every bug fix adds a test that fails before the fix.
- Performance-sensitive changes run `cargo xtask bench` and stay within the budgets in `docs/architecture.md` §7.

## 6. Quality gate

`cargo xtask ci` must pass before every push. It runs: format check, clippy with warnings as errors, tests,
the layering check, and the provenance check (sign-off and `Clean-Room:` trailer on new commits).

## 7. Dependencies

- Licence allow-list in `deny.toml`: MIT, Apache‑2.0, BSD‑2/3, ISC, Zlib, Unicode‑3.0, MPL‑2.0. Nothing GPL/LGPL/AGPL.
- Prefer few, well-maintained crates. Justify each new dependency in the commit message.

## 8. Docs

- Behaviour changes update the relevant spec status and `docs/`.
- Decisions with trade-offs get an ADR in `docs/adr/`.
- Writing style: plain, specific, no marketing language.
