# ADR-0003: Mesh-first solid modelling behind a façade

- Status: accepted
- Date: 2026-10-08

## Context

Architecture mostly needs prisms, extrusions, sweeps and booleans (openings cut into walls, slabs trimmed by
roofs). Robustness matters more than curved-surface exactness. Pure-Rust options exist for robust mesh booleans
(a Manifold port, Apache-2.0) and for B-rep with NURBS (truck family, Apache-2.0), at different maturity levels.

## Decision

- All solid operations go through `kanavos-solid`, which exposes our own types.
- First backend: robust mesh booleans. Plans do not depend on 3D booleans (they are computed in 2D from footprints).
- Add an exact B-rep backend behind the same façade when curved elements require it.

## Consequences

- Fast, robust results for the MVP; curved surfaces are tessellated at a project tolerance until B-rep arrives.
- Backends can be benchmarked and swapped without touching element code.
