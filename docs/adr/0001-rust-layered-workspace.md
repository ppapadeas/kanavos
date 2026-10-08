# ADR-0001: Rust, one layered workspace

- Status: accepted
- Date: 2026-10-08

## Context

A BIM application needs predictable performance (large models, interactive geometry), memory safety (users trust it
with months of work) and one codebase for Linux, Windows, macOS and later the web.

## Decision

Write Kanavos in Rust as a single Cargo workspace of small crates arranged in strict layers (see
`docs/architecture.md` §2). Lower layers never depend on higher ones; the UI never contains model logic.
`cargo xtask layering` enforces the rule in CI.

## Consequences

- The engine can run headless (CLI, tests, control protocol) and the UI can be replaced without touching it.
- More crates means more boilerplate; accepted for the clarity of boundaries.
