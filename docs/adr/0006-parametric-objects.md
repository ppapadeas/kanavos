# ADR-0006: Own parametric object system

- Status: phase 1 accepted; phase 2 proposed
- Date: 2026-10-08 (proposed), 2026-10-10 (phase 1 accepted)

## Context

Doors, windows and fixtures need parameters, 2D symbols and 3D geometry that adapt to their host. Commercial tools
use proprietary scripting languages; compatibility with them raises legal questions (see legal brief §9.6).

An architect needs a complete library on the first day: the doors, windows, sanitary ware, kitchen and furniture
of a house, with plan symbols that follow Greek and European drafting conventions. A sandboxed authoring language
is worth having, but its design and safety review would delay that library by months.

## Decision

Kanavos has its own object system, in two phases.

### Phase 1 (accepted): built-in parametric definitions in Rust

- The catalogue lives in `kanavos-objects` (layer L4) as Rust code: each definition has a stable identifier
  (`door.single_swing`), a name and parameter labels from the message catalogue (Greek and English), a default
  size, typed parameters with defaults and ranges, an origin convention, and the IFC entity and predefined type it
  exports as.
- For every definition, pure functions produce the plan symbol (as `kanavos-drawing` items on `door.symbol*`,
  `window.symbol*`, `opening.symbol*` and `object.symbol*` layers, with detail chosen from the drawing scale) and
  coloured closed meshes for 3D. Openings are drawn in one canonical state and mirrored for the swap-side and
  swap-hand flags.
- Stored parameter values are validated on use (defaults for missing or non-finite values, clamped to the range),
  so a document can never make an object fail.
- Every definition is authored from scratch from drafting conventions and common product dimensions, specified in
  `specs/objects/`, and checked by tests and by rendered preview sheets of the whole catalogue.

### Phase 2 (proposed): sandboxed authoring language

Define user objects as a sandboxed, deterministic script plus a parameter schema, producing 2D symbol, 3D geometry
and IFC classification. Candidate embedded languages: Rhai, Luau, or a small declarative DSL. Choose after a spike
measuring safety (no file or network access), speed and authoring ergonomics. Phase 1 definitions keep working;
the phase 1 API (`catalogue`, `opening_symbol_at`, `object_parts`, …) is the contract a script runtime must meet.

## Consequences

- The library is available now, fast (no interpreter) and covered by the normal test and lint gates.
- Adding or changing an object needs a code change and a release; users cannot author objects until phase 2.
- Identifiers are part of the document format: once released they are never renamed or removed.
- Vendor-language compatibility stays a separate, legally reviewed decision (clean-room protocol §8).
