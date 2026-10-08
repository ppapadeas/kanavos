# ADR-0002: IFC for interchange, a documented native format for work

- Status: accepted
- Date: 2026-10-08

## Context

People need to bring existing projects in and send models to consultants. Proprietary native formats of other
products are closed and, under our clean-room protocol, out of bounds. IFC (ISO 16739-1:2024) is the open standard
every serious BIM tool exports. IFC is an exchange schema, not an efficient editing format.

## Decision

- Interchange: IFC 4.3 import and export are first-class and continuously tested. IFC 2x3 import is supported.
- Native: `.domos`, a ZIP container with a versioned JSON model (deterministic ordering), rebuildable binary caches,
  embedded assets and a manifest. The format is documented in the repository and versioned with migrations.
- No proprietary native formats are read or written (clean-room protocol §7).

## Consequences

- Migration path for new users is "export IFC from your current tool". Some semantics (parametric objects,
  drawing settings) will not survive that path; import maps what it can and keeps the rest as reference geometry.
- A documented native format lets others build tools on Domos projects.
