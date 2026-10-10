# Architecture

Kanavos is a BIM authoring application: people model a building once, and plans, sections, elevations, schedules and
sheets come out of that model automatically and stay in sync. This document describes how the code is organised to
make that fast, robust and scriptable. Decisions with real trade-offs are recorded as ADRs in [`adr/`](adr).

## 1. Goals that shape the design

1. **Drawings are derived, never drawn twice.** Every 2D view is computed from the model. Editing a wall in plan
   updates the section, the schedule and the sheet in the same frame budget.
2. **Every action is a command.** Mouse tools, menus, the control protocol and tests all go through the same command
   bus. That gives unlimited undo, reliable automation and reproducible bug reports for free.
3. **The interface never waits for geometry.** Heavy work (booleans, hidden-line removal, exports) runs off the UI
   thread, is cancellable and streams results in.
4. **Open by default.** IFC is a first-class format, the native format is documented, and the engine runs headless.
5. **Never crash, never lose work.** No panics in shipped code, autosave with crash recovery, file round-trips
   covered by property tests.
6. **Greek practice built in.** ΕΓΣΑ87 georeferencing, ΝΟΚ area and coverage calculations and permit sheet sets are
   part of the core, not plug-ins.

## 2. Layered workspace

Crates may depend only on crates in **lower** layers. `cargo xtask layering` enforces this in CI.

```
 L10 apps/kanavos (desktop)        apps/kanavos-cli (headless, batch, control server)
 L9  kanavos-ui (egui front end)   kanavos-mcp (MCP adapter)
 L8  kanavos-tools (interactive tools, snapping)   kanavos-render (wgpu)   kanavos-control (JSON-RPC)
     kanavos-update (signed over-the-air updates)
 L7  kanavos-engine (session, command bus, undo, scheduling, selection)
 L6  kanavos-sheets   kanavos-format (.kanavos)   kanavos-ifc   kanavos-dxf   kanavos-pdf   kanavos-gltf
 L5  kanavos-views (plan/section/elevation/3D derivation, hidden-line removal)   kanavos-schedules   kanavos-rules-gr
 L4  kanavos-elements (walls, slabs, openings, roofs, stairs, spaces …)   kanavos-objects (parametric objects)
 L3  kanavos-model (project document, attributes, relations, IDs)   kanavos-raster (drawing → PNG)
 L2  kanavos-solid (solid modelling façade)   kanavos-drawing (2D drawing primitives)   kanavos-geo (CRS, ΕΓΣΑ87)
 L1  kanavos-geom   kanavos-i18n (message catalogue, el/en)
 L0  kanavos-units   kanavos-fonts (font discovery, glyph outlines, built-in stroke font)
 dev kanavos-testkit (fixtures, golden drawings, visual diffs)
```

### Responsibilities

| Crate | Owns |
| --- | --- |
| `kanavos-units` | Lengths, angles, areas, volumes; tolerances; locale-aware formatting (decimal comma for `el`). |
| `kanavos-fonts` | Discovery of an installed Greek-capable sans-serif font (fonts are not bundled), glyph outlines and advance widths, and a built-in single-stroke lettering font so text output never fails. |
| `kanavos-geom` | 2D/3D primitives, robust predicates, polygons with holes, booleans, offsets, arcs. |
| `kanavos-i18n` | The message catalogue: every user-facing string, in Greek and English, one file per area. |
| `kanavos-solid` | Extrude, sweep, revolve, CSG; one façade over the chosen kernel(s) so they can be swapped. |
| `kanavos-drawing` | Resolution-independent 2D drawing: strokes with line weight and line type, fills, hatches, text, dimensions. Shared by views, sheets and exports. Output helpers: hatch expansion, dashing, text layout, page placement and SVG export. |
| `kanavos-geo` | Project survey point, true north, ΕΓΣΑ87 (EPSG:2100) ↔ project coordinates, survey import helpers. |
| `kanavos-raster` | Antialiased rendering of drawings to RGBA images and PNG for previews and thumbnails. |
| `kanavos-model` | The document: site, buildings, storeys, elements, attributes (materials, build-ups, line types, fills, layers), relations, stable IDs mapped to IFC GlobalIds. Persistent (structurally shared) data for cheap undo. |
| `kanavos-elements` | Rules that turn element parameters into geometry: wall build-ups and junctions, slab edges, openings cut into hosts, roofs, stairs, spaces and their areas. |
| `kanavos-objects` | Parametric object library for doors, windows, voids, furniture and fixtures: built-in Rust definitions now, a sandboxed authoring language later (ADR‑0006). |
| `kanavos-views` | Derives views: plan cut at a storey's cut plane, projection above/below, sections and elevations with hidden-line removal, 3D tessellation; output is `kanavos-drawing`. |
| `kanavos-schedules` | Quantities and tables (doors, windows, areas, materials). |
| `kanavos-rules-gr` | ΝΟΚ (ν. 4067/2012) calculations: κάλυψη, δόμηση, όγκος, εξαιρούμενοι χώροι; διάγραμμα κάλυψης; permit sheet-set presets. |
| `kanavos-sheets` | Sheets, title blocks, viewports with scale, sheet sets, publishing. |
| `kanavos-format` | Native `.kanavos` container read/write and schema migrations. |
| `kanavos-ifc` | IFC 4.3 (ISO 16739‑1:2024) import/export over STEP Part 21. |
| `kanavos-dxf` | DXF import (survey, underlays) and export (drawings). |
| `kanavos-pdf` | Vector PDF export, PDF/A for archiving. |
| `kanavos-gltf` | glTF 2.0 binary (`.glb`) export of the 3D model for visualisation tools. |
| `kanavos-engine` | Open session, command bus, undo/redo, selection, change propagation, background job scheduling and cancellation, autosave. |
| `kanavos-tools` | Interactive tools that turn input into commands; snapping and inference; typed numeric input. |
| `kanavos-render` | GPU rendering of 2D views and 3D viewports (wgpu): offscreen viewport image, orbit camera, picking, section box, edges, shadows and ambient occlusion; headless rendering for tests and batch export (ADR‑0007). |
| `kanavos-control` | JSON-RPC 2.0 control protocol over stdio and local WebSocket (see [`control-protocol.md`](control-protocol.md)). |
| `kanavos-update` | Over-the-air updates: finds a newer release on GitHub, downloads its package, checks the SHA-256 and the Ed25519 signature, installs it over the running installation with a journal and rollback, and restarts (see [`updates.md`](updates.md)). |
| `kanavos-ui` | The desktop interface (egui). Contains no model logic. |
| `kanavos-mcp` | Exposes the control protocol as an MCP server for automation clients. |

## 3. Data flow

```
 user input ─▶ tool ─▶ Command ─▶ engine ─▶ new Document (persistent) ─┬─▶ dirty set
                          ▲                                            │
 control protocol ────────┘                                            ▼
                                                     dependency graph recomputes only what changed
                                                     element geometry → view derivation → drawing
                                                                            │
                                                     render (UI) ◀──────────┴──────▶ export (PDF/DXF/IFC)
```

- **Document** is an immutable, structurally shared value. A command produces a new document; undo keeps the old
  one. Memory cost per step is proportional to what changed.
- **Dependency graph**: each derived artefact (wall solid, plan of storey 1, area table) records which document
  entities it read. A change marks only dependants dirty.
- **Jobs**: recomputation runs on a work-stealing thread pool. Jobs carry a document version; results for an old
  version are discarded. The UI shows the last complete result plus a subtle "updating" state, never a blank view.

## 4. Commands

```rust
pub trait Command {
    /// Stable, documented name, e.g. "wall.create". Used by menus, shortcuts, scripts and the control protocol.
    fn name(&self) -> &'static str;
    /// Apply to a document, returning the new document or a user-facing error. Never panics.
    fn apply(&self, doc: &Document) -> Result<Document, CommandError>;
}
```

- Every command has a JSON schema for its arguments, generated from the Rust type.
- Command names are the public API. They are versioned and listed in `docs/commands.md` (generated).
- Interactive tools build a preview command while the user drags, and commit it on click.

## 5. Geometry strategy

- **2D first.** Plans are computed from footprints and cut sections of elements, not by slicing a full 3D mesh,
  which keeps plans exact and fast. Sections and elevations use 3D solids plus exact hidden-line removal.
- **Solids** go through the `kanavos-solid` façade. Initial backend: robust mesh booleans (a pure-Rust Manifold port,
  Apache‑2.0). Exact B-rep (NURBS) can be added behind the same façade for curved work (ADR‑0003).
- **Tolerances** are explicit and project-wide (default 0.1 mm), never ad hoc epsilons.
- **Junctions** (walls meeting walls, slabs meeting walls) are resolved by our own priority rules defined in specs,
  recomputed locally around changed elements (`specs/elements/wall-junctions.md`). A whole storey resolves in one
  pass (`StoreyGeometry`), a single wall from its neighbours only, with identical results.
- **Element solids without general CSG**: walls with door and window holes and roof-clipped tops are built directly
  as closed meshes from plan pieces with vertical intervals; roofs come from the weighted straight skeleton; 2D
  Booleans run on one fixed project-wide integer grid so that pieces cut in separate operations meet exactly.

## 6. Files

- **Native `.kanavos`**: a ZIP container with a versioned JSON model (deterministic key order for diffs), binary
  geometry caches that can always be rebuilt, embedded assets and a manifest. Documented in
  [`format.md`](format.md). Readers must accept older schema versions through migrations.
- **IFC 4.3** import and export is a first-class path, tested against buildingSMART sample files and models exported
  from other BIM tools.
- **DXF** for survey underlays (τοπογραφικό) and drawing export. **PDF** vector export, PDF/A for archiving.
- **glTF** export for visualisation.

## 7. Performance budgets

| Interaction | Budget |
| --- | --- |
| Pointer move to updated preview (2D) | ≤ 8 ms at 120 Hz, ≤ 16 ms worst case |
| Edit a wall → plan of its storey updated | ≤ 50 ms for a 5,000-element storey |
| Edit a wall → section updated | ≤ 300 ms, streamed |
| Open a 50,000-element project | ≤ 2 s to first interactive view |
| Save | ≤ 200 ms incremental, never blocks input |
| 3D viewport frame, 5,000 elements / 0.5 M triangles at 1920 × 1080 | ≤ 16 ms (60 fps), integrated graphics |
| Pick the element under the cursor in 3D | ≤ 1 frame (asynchronous) |

`cargo xtask bench` tracks these on reference projects; regressions over 10 % fail CI.

## 8. Robustness

- `#![forbid(unsafe_code)]` except in narrowly reviewed crates (render, FFI), and `#![deny(clippy::unwrap_used,
  clippy::expect_used, clippy::panic)]` in all shipped crates.
- Every fallible geometry operation returns a typed error; the UI shows which element failed and why, and keeps the
  last good geometry.
- Autosave every 60 s to a recovery journal; crash recovery on next start.
- Property tests: save → load → save is byte-identical; IFC export → import preserves element counts and quantities.

## 9. Platforms

Linux, Windows and macOS desktop builds from one codebase. A WebAssembly viewer (read-only, then light editing) is a
later target; the engine has no OS dependencies to keep that open.

## 10. Licensing of dependencies

Allowed: MIT, Apache‑2.0, BSD‑2/3, ISC, Zlib, Unicode‑3.0, MPL‑2.0. Denied: GPL, LGPL, AGPL, SSPL, proprietary.
Enforced by `cargo deny` with [`deny.toml`](../deny.toml). Candidate dependencies are checked before adoption:

| Need | Candidate | Licence (verify on adoption) |
| --- | --- | --- |
| GPU | wgpu | MIT OR Apache‑2.0 |
| GPU buffer casts | bytemuck | Zlib OR Apache‑2.0 OR MIT |
| Blocking on GPU futures (headless) | pollster | Apache‑2.0 OR MIT |
| UI | egui / eframe, with AccessKit | MIT OR Apache‑2.0 |
| Mesh booleans | manifold-rust (pure-Rust port of Manifold) | Apache‑2.0 |
| 2D polygon Booleans and offsets (adopted in `kanavos-geom`) | i_overlay 9 (with i_float, i_shape, i_tree, i_key_sort, libm) | MIT OR Apache‑2.0; dependencies MIT |
| B-rep (later) | truck / monstertruck | Apache‑2.0 |
| STEP parsing | ruststep, or own Part 21 parser | Apache‑2.0 |
| Persistent collections | im / rpds | MPL‑2.0 / MPL‑2.0 |
| Serialisation | serde, serde_json | MIT OR Apache‑2.0 |
| Font parsing | ttf-parser | MIT OR Apache‑2.0 |
| Rasterising | tiny-skia (+ png) | BSD‑3‑Clause (png: MIT OR Apache‑2.0) |
| Compression (PDF streams) | miniz_oxide | MIT OR Zlib OR Apache‑2.0 |
| HTTPS client (adopted in `kanavos-update`) | ureq 3 (rustls, no bundled roots) | MIT OR Apache‑2.0 |
| TLS (adopted) | rustls with the *ring* provider; roots from the OS through rustls-native-certs | rustls: Apache‑2.0 OR ISC OR MIT; ring: Apache‑2.0 AND ISC; rustls-native-certs: Apache‑2.0 OR ISC OR MIT |
| SHA-256 and Ed25519 (adopted) | ring | Apache‑2.0 AND ISC |
| Release packages (adopted) | zip (already used by `kanavos-format`), tar, flate2 | MIT; MIT OR Apache‑2.0; MIT OR Apache‑2.0 |
| Versions and base64 (adopted) | semver, base64 | MIT OR Apache‑2.0 |
