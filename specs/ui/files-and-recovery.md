---
id: SPEC-ui-006
title: Files, unsaved changes, autosave and recovery, preferences, first launch, tables
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/ux.md §1, §5
  - docs/architecture.md §1.5, §6, §8 (never lose work, autosave every 60 s)
---

# Files, unsaved changes, autosave and recovery, preferences, first launch, tables

## Purpose

Never lose work, never ask more than necessary, and open straight into something useful.

## Behaviour

### Opening and saving

- New, Open (native file dialog), Open recent (last ten files), Open sample house, Save, Save as. Save without a
  file behaves as Save as. Saving writes through a temporary file and a rename, so a failed save never damages
  the previous file. A project given on the command line is opened at start.
- After a successful open or save, a notice names the file and the file goes to the top of the recent list.
- New, Open, Open sample and Exit (menu or window close) ask first when there are unsaved changes: Save,
  Don't save, Cancel. Save that is cancelled or fails keeps the project open.

### Autosave and recovery

- Every 60 s, if the document changed since the last copy and is not saved, a recovery copy is written in the
  background to the user's data directory, with the original file name and the time.
- A clean exit with nothing unsaved, a save, opening another project, or "Don't save" on exit removes the copy.
- At start, an existing copy is offered: Recover (the document opens, still counted as unsaved, with a notice to
  save it) or Discard.

### Preferences

- Kept as a small JSON file in the configuration directory: theme, language, interface scale, recent files,
  side panels shown, split view, first launch done, autosave interval. Unknown fields are ignored and missing ones
  take defaults; a damaged file gives the defaults. No project content is stored.

### First launch

- The first launch opens a sample house — two storeys with walls, openings, spaces, furniture, stair, roof,
  sections, elevations and the permit sheet set — so people see results before learning tools. Later launches
  start a new project.

### Tables

- Schedules and the area table open as live tables: header row, striped body, bold totals under a rule.
- Clicking a header sorts by that column (again to reverse): numbers numerically, text without regard to case
  or accents, empty cells last. Sorting is view state only.
- "Export CSV" writes the table in the interface language (`;` and decimal comma in Greek, `,` and decimal point
  in English).
- An empty table says there are no such elements yet; a schedule that does not exist yet says so.

### Exports and imports

- Export: PDF of sheets, DXF/SVG/PNG of the focused view or sheet, IFC and glTF of the model, CSV of the focused
  table. Import: DXF survey as an underlay, IFC as a new project. Entries that are not part of the build are
  marked "coming soon" and explain this when chosen.

### Visual verification hook

- With `KANAVOS_SCREENSHOT=<path>` the application renders `KANAVOS_SCREENSHOT_FRAMES` frames (default 40),
  saves a PNG of the window and exits, without touching preferences or recovery files. `KANAVOS_SCENE` sets up
  the capture (language, theme, selection, open windows, tabs, palette query, history page, tool, scale,
  panels).

## Acceptance tests

1. Save then open gives an equal document, and saving it again is byte-identical.
2. Foreign or damaged files are rejected with a reason; missing files report an I/O error.
3. A recovery copy round-trips with its original path; after clearing, nothing is offered.
4. Preferences round-trip; unknown fields are ignored; ten recent files at most, without duplicates.
5. Tables sort numbers numerically with empty cells last, and Greek text accent-insensitively.
6. The sample house builds with no rejected steps and contains walls, doors, windows, spaces, a roof, a plot and
   sheets.
