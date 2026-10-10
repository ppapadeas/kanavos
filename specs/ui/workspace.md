---
id: SPEC-ui-001
title: Workspace layout, panels, view tabs and split view
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §1, §2, §5, §6
  - docs/architecture.md §3 (derived views, background work)
---

# Workspace layout, panels, view tabs and split view

## Purpose

Give the architect one calm working surface where the drawing has most of the space, the model's structure is
one click away, and the properties of whatever is selected are always visible.

## Behaviour

### Layout

From top to bottom: menu bar; context bar; the working row; status bar. The working row holds, from left to
right: project tree, tool palette, view area, inspector.

- **Menu bar**: File, Edit, View, Model, Help. The project name is shown at the right, followed by a dot while
  there are unsaved changes.
- **Context bar** (one row, 40 pt): icon and name of the active tool, then its key parameters, then a search
  entry ("Search … Ctrl+K") at the right edge.
- **Tool palette**: every tool as a square icon button (30 pt), two per row, in five groups separated by
  hairlines: selection and measuring; building parts; annotation; drafting; views and site. The active tool is
  tinted with the selection colour. Hovering shows the tool name, its single-key shortcut and a one-line
  description.
- **Project tree** (default 260 pt, resizable 200–440): see SPEC-ui-002.
- **Inspector** (default 320 pt, resizable 260–520): properties of the selection, or the regulation meters
  when nothing is selected (SPEC-ui-004); a second page lists the history.
- **Status bar** (26 pt): the tool's prompt or the latest notice at the left; at the right the pointer position
  in project coordinates and ΕΓΣΑ87, snap toggles (on/off, points, edges, 5 cm grid), the scale of the focused
  view, the active storey with its level, and a spinner while background work runs.

### Canvas first

- Either side panel collapses to a 38 pt rail showing its section icons and an expand button. Clicking any rail
  icon expands the panel.
- `Tab` toggles both side panels together: if either is open both close, otherwise both open. `Tab` is left to
  focused widgets (keyboard navigation) and to the canvas while the user is typing a value into a tool.

### View tabs

- Tabs are one ordered bar above the view area. Each tab shows an icon for its kind (plan, section, elevation,
  3D, site plan, coverage diagram, sheet, table), its title and, when focused or hovered, a close button.
  Middle-click also closes. Dragging a tab onto another reorders the bar.
- Opening something that already has a tab focuses that tab; nothing is opened twice.
- Plans, sections, elevations, site plans, coverage diagrams and sheets open in a 2D canvas; 3D views in the 3D
  viewport; schedules and the area table in a table view (SPEC-ui-006).
- Tabs whose view or sheet disappears (deleted, or removed by undo) close automatically.
- A plan with no elements shows a hint card near its bottom edge: start with a wall, press W or pick the wall
  tool.

### Split view

- The view area shows one pane, or two side by side with a 6 pt gap. The default is two panes: the active
  storey's plan on the left and the 3D view on the right.
- The pane last clicked is the focused pane; it has an accent outline and its tab carries an accent underline in
  the bar (the other pane's tab has a grey underline). Tabs opened from the tree, palette or menus go into the
  focused pane. Choosing in one pane the tab shown in the other swaps the two.
- In split mode each pane has a slim header with the tab's icon and title.

### Active storey and scale

- When the focused pane shows a plan, its storey becomes the active storey (used by tools, the inspector's
  storey section and "select all").
- The status bar shows the scale of the focused view (plans, sections, elevations, site plan, coverage); none
  for 3D, sheets and tables.

### Themes, scale and language

- Light and dark themes are defined as colour tokens: neutral surfaces, one blue for selection and focus, amber
  for warnings, red for errors, green for "within limits". Text keeps a luminance contrast of at least 150/255
  against panels (muted text at least 80/255).
- The interface scale (90–150 %) applies to everything; keyboard zoom (Ctrl+= / Ctrl+−) updates the preference.
- The language (Ελληνικά / English) switches at once, without restart; every string comes from the catalogue.
- Interface fonts are loaded from the system (regular, semibold, fixed-width and a symbol face for arrows); none
  are bundled.

### Accessibility

Every custom widget (icon buttons, tabs, tree rows, table headers, meters, palette results) reports a role, a
label and, where it applies, a selected state to assistive technology.

## Edge cases

- Closing the last tab leaves the pane empty with a hint to open something from the project tree.
- Turning split view on when only one tab is open leaves the right pane empty until a second tab exists.
- Very long names truncate with an ellipsis in tree rows and drop-downs instead of widening panels.

## Acceptance tests

1. Given the starter project, when the workspace opens, then the left pane shows the lowest above-ground plan
   and the right pane the 3D view.
2. Given an open tab, when it is opened again, then the number of tabs does not change and it is focused.
3. Given two panes, when the right pane's tab is chosen for the left pane, then the panes swap tabs.
4. Given any sequence of closing tabs, then both panes always refer to existing tabs or none.
5. Given a reordered tab bar, then each pane still shows the same tab as before.
6. Given a document without the views of the open tabs, when tabs are pruned, then those tabs close and
   model-independent tabs (schedules, area table) stay.
7. Given both themes, then text/panel and muted-text/panel contrasts meet the thresholds above.

## Open questions

- Dragging a view from the tree onto a sheet tab to place it (docs/ux.md §2) waits for viewport editing on
  sheets.

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
