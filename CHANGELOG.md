# Changelog

Each released version has a section here. The release workflow copies the section into the GitHub release notes,
so write it for the people who install Kanavos. Versions follow [Semantic Versioning](https://semver.org/); before
1.0 a minor version may change the file format, and newer versions always open older files.

## 0.1.0

First test release, for Windows and Linux (x86-64). Early alpha: expect missing pieces and report what you find.

**Model**

- Storeys, walls with layered build-ups and clean junctions, slabs with holes, columns, beams, doors and windows,
  stairs checked against the Greek building regulation, flat and pitched roofs (mono-pitch, gable, hip) with holes,
  railings that follow slopes, rooms from walls, furniture, sanitary and kitchen objects.
- Typed input while drawing, snapping with named inferences, unlimited undo with a history list, search (Ctrl+K).

**Drawings and permit set**

- Plans, sections and elevations derived from the model with hidden-line removal, material hatches and levels; 3D
  view with section box.
- Live ΝΟΚ meters (coverage, floor area, height, volume, floors), coverage diagram, area and opening schedules,
  and a one-command permit drawing set with title block.
- ΕΓΣΑ87 survey import from DXF, including Greek text in code page 1253.

**Files**

- Native `.kanavos` files with autosave and crash recovery.
- Export to PDF, DXF, SVG, PNG, IFC 4.3, glTF and CSV; IFC import.

**Automation**

- `kanavos-cli` for headless export and command scripts, a JSON-RPC control server and an MCP server.

**Updates**

- Kanavos checks for new versions, downloads them, verifies their signature and installs them on request.

**Known limits**

- The mapping of ΝΟΚ terms to the Κώδικας Χωροταξίας και Πολεοδομίας (ν. 5306/2026) is not yet confirmed; check
  the meters against the current rules before submitting.
- The packages are not code-signed, so Windows SmartScreen warns on first launch.
- No macOS build yet.
