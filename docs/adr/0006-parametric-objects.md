# ADR-0006: Own parametric object system

- Status: proposed
- Date: 2026-10-08

## Context

Doors, windows and fixtures need parameters, 2D symbols and 3D geometry that adapt to their host. Commercial tools
use proprietary scripting languages; compatibility with them raises legal questions (see legal brief §9.6).

## Decision (proposed)

Define Kanavos objects as a sandboxed, deterministic script plus a parameter schema, producing 2D symbol, 3D geometry
and IFC classification. Candidate embedded languages: Rhai, Luau, or a small declarative DSL. Choose after a spike
measuring safety (no file or network access), speed and authoring ergonomics.

## Consequences

- Our own object library must be authored from scratch for the MVP (doors, windows, sanitary fixtures, basic
  furniture).
- Vendor-language compatibility stays a separate, legally reviewed decision.
