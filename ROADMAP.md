# Roadmap

Honest estimates for a small core team (2–3 people) working steadily. Each milestone ends with something an
architect can try on a real project.

| Milestone | Outcome | Rough duration |
| --- | --- | --- |
| **M0 — Foundations** | Workspace, CI, clean-room process live; units, geometry, document model, command bus, undo; `.kanavos` save/load; headless CLI | 2–3 months |
| **M1 — Plans** | Storeys, walls with build-ups and junctions, slabs, columns, openings; live plan views; typed input and inference; first UI and design system | 4–6 months |
| **M2 — Drawings** | Sections and elevations with hidden-line removal; dimensions, levels, labels; sheets and PDF/DXF export; first usability tests | 4–6 months |
| **M3 — Permit set (MVP)** | Stairs, roofs, spaces, areas; ΝΟΚ meters, area tables, διάγραμμα κάλυψης, permit sheet-set preset; DXF survey and ΕΓΣΑ87; IFC import/export | 3–4 months |
| **M4 — Pilot** | Three real permit submissions prepared in Kanavos; hardening, performance, Greek/English polish; signed installers for Linux, Windows, macOS | 2–3 months |
| **Later** | Curved/B-rep elements, curtain walls, phases, linked models, energy (ΚΕΝΑΚ) hooks, collaboration, web viewer | — |

**MVP target: about 15–22 months from M0 start.**

Status as of 2026-10-10: a first end-to-end version covers much of M0–M3 in breadth (model, junctions, plans,
sections and elevations with hidden-line removal, ΝΟΚ meters and permit sheets, PDF/DXF/IFC/glTF, control protocol
and MCP). It needs depth before a pilot: clean-room review of the specs, usability tests with practising architects,
the benchmark task (docs/ux.md §8), and the performance budgets measured by `cargo xtask bench`.
