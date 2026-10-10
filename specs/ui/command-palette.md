---
id: SPEC-ui-005
title: Command palette and keyboard shortcuts
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §3 (command palette, shortcuts)
---

# Command palette and keyboard shortcuts

## Purpose

Reach any tool, command, view, sheet, storey, setting or element by typing a few letters, in Greek or English.

## Behaviour

### Palette

- `Ctrl+K` (or the search entry in the context bar) opens a floating box near the top of the window with a
  text field and up to 60 results. Each result shows an icon, its title, its kind (tool, command, view, sheet,
  storey, setting, element) and its shortcut.
- Arrow keys move the highlight, `Enter` runs it, `Esc` or a click outside closes. Clicking a result runs it.
- Entries: every tool; tool settings people search by name (wall thickness, wall height, door size, window sill,
  stair riser, roof pitch, text size — they activate the tool); file, edit, view and model commands; exports and
  imports; the attribute managers; theme and language; every storey (activate), view, schedule, area table and
  sheet (open); every element (select it, show its storey, open the inspector). Elements are titled by
  category, mark and id, spaces also by number and name.
- Every entry is searchable by its Greek and English text, whatever the interface language.
- Matching: text is folded to lower case without Greek accents or diaeresis and with final sigma as σ. The query
  is split into words; every word must match a word of the entry, best as the whole word, then as its start,
  then anywhere inside it, then as letters in order (two letters or more). Earlier entry words score higher;
  shorter entries win ties; equal scores keep the input order.

### Shortcuts

- Single letters choose tools (V select, W wall, F slab, C column, D door, N window, S stair, R roof, A space,
  M dimension, T text, L line, Q measure) when no widget has keyboard focus and the canvas is not taking typed
  input. The letter is not passed on as typed text.
- `Ctrl+N/O/S`, `Ctrl+Shift+S`, `Ctrl+K`, `F1` work everywhere; `Ctrl+Z`, `Ctrl+Y`/`Ctrl+Shift+Z`, `Ctrl+A`,
  `Delete`, `Home` work when no text field has focus (text fields keep their own undo and select-all).
- The Help menu lists every shortcut.

## Acceptance tests

1. "τοιχ" and "wall" both rank the wall tool first in a Greek interface.
2. "τοιχ πάχ" finds the wall-thickness setting in an English interface.
3. In the sample house, "τομη α" finds a section view, "ισογειο" a storey, "παραθυρ" window elements, "door
   schedule" the door schedule.
4. Folding: "Τοίχος" → "τοιχοσ", "ΔΙΑΣΤΡΩΜΆΤΩΣΗ" → "διαστρωματωση".
5. Every tool appears exactly once in the tool palette, and tool labels carry their shortcut.

---

## Review (completed by the reviewer)

- [x] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [x] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [x] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [x] No screenshots or images of the reference product.
- [x] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [x] Describes behaviour, not implementation details of the reference product.

Reviewed by: Pierros Papadeas, 2026-10-10 — Result: approved

Note: the reviewer is also the author of record. Clean-room protocol §2 asks for a second reviewer; add one when a second contributor joins, before the first public release.
