---
id: SPEC-ui-003
title: Drawing canvas — hosting tools, snapping feedback and typed input
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §1 (drawing is direct), §3 (typed input, inference and snapping, Esc backs out)
  - docs/adr/0005-command-bus.md (every change is a command)
---

# Drawing canvas — tools

## Purpose

The canvas is where tools are used. It turns the pointer and the keyboard into tool input, shows what the tool
proposes, and applies what the tool decides — without knowing anything about walls or slabs itself.

## Behaviour

### Input to the tool (plan views)

- Pointer positions are converted to model metres and sent as moves; several moves within one frame are merged
  into the last one. When the view pans or zooms under a still pointer, a move with the new model position is sent.
- A primary button press over the canvas is sent as a press (preceded by a move to the same point); its release
  is sent as a release even if the pointer has left the canvas meanwhile, so tools can tell clicks from drags.
  Presses while `Space` is held, or while panning, go to navigation instead.
- A double click is sent after the press/release pair.
- Modifiers travel with every event: `Shift`, `Ctrl` (`Cmd` on macOS), `Alt` (tools use it to switch snapping off).
- Keys, while the canvas has keyboard focus: `Enter` (with `Shift` in a text box: a new line), `Backspace`, `Tab`,
  `Delete`, and `F` (flip), `R` (rotate), `M` (mirror) while the tool is in the middle of an operation or the
  selection tool has a selection — otherwise those letters stay application shortcuts. Shortcuts with `Ctrl` are
  left to the application. Keys the canvas takes are consumed, and the application can ask beforehand whether
  the canvas claims a key.
- `Esc` goes to the tool while it is in the middle of an operation; when the tool is idle, `Esc` returns to the
  selection tool; in the selection tool it clears the selection.
- Typed input (ux.md §3): while the tool asks for typed input, or a field holds a typed value, digits, operators
  (`+ - * / ( ) <`), separators (`. , ;`), `@`, `#` and the unit letters `m` and `c` go to the tool; letters are
  otherwise application shortcuts. While a text box is open, every printable character goes to it, spaces
  included.
- The canvas takes keyboard focus when clicked, or when hovered and nothing else has focus; it keeps `Tab`, the
  arrows and `Esc` for itself while focused.

### Feedback from the tool

- **Preview** geometry is drawn over the drawing at the view's scale (weights and dashes as on paper).
- **Snap**: the snapped point is marked by a glyph whose shape tells the kind of snap — square (end point),
  triangle (midpoint), corner bracket (corner), circle (centre), cross (intersection), diamond (on an edge),
  right-angle mark (perpendicular), plus (extension), wedge (angle lock), small cross (grid) — in the inference
  colour with a white halo, and the tool's localised label in a small box beside the cursor.
- **Inference guides** (e.g. the line of a locked angle) are drawn dashed in the inference colour.
- **Typed-input fields** are shown as a small box next to the cursor, one row per field (label and value). The
  active field is outlined in the selection colour; typed values are black with a caret, live values grey. The box
  stays inside the canvas and below the snap label.
- **Handles** of the selection are drawn as small marks in the selection colour — squares for ends and vertices, a
  circle with a plus where dragging inserts a vertex, a circle with arrows for moving, a bar for an opening's jamb
  — filled when under the pointer.
- **Highlight**: what the tool points at (the element under the pointer, or what a click would act on) is drawn in
  the hover colour and becomes the application's hover element.
- **Tracking points** the pointer aligns with are marked with small crosses in the inference colour.
- **Warning**: when the previewed operation cannot be done as it stands, the preview is drawn in the warning
  colour (red) and the reason is shown in a red label next to the cursor.
- **Text box**: while the text tool collects text, the text so far is drawn at its printed size and rotation at
  its anchor, with a caret after the last character and a dashed frame.
- **Cursor**: arrow, crosshair, move or grab as the tool asks; grab/grabbing while panning.
- **Prompt**: the tool's prompt is available to the status bar.
- **Commands** returned by the tool run at once through the application state; several commands from one event
  form one undo step. A selection returned by the tool replaces the selection.
- When the application's tool changes, the current operation is cancelled and the new tool starts idle.

### Sections and elevations

- A click selects the element under the pointer (Shift adds, Ctrl toggles, a click on nothing clears); `Delete`
  deletes the selection; `Esc` clears it. Hits prefer openings over walls over slabs over spaces.

### Sheets

- A click selects the placed view under the pointer and outlines it (dashed, selection colour); dragging a placed
  view moves it, previewed by its moving outline, and on release updates the sheet in one undo step. `Delete`
  removes the selected view from the sheet (the view itself stays in the project); `Esc` deselects.

### Hover

- The element under the pointer (within 6 points) is the hover element of the whole application; it is cleared
  when the pointer leaves the canvas.

## Edge cases

- Releasing the button outside the window still ends the tool's drag.
- A tool that returns no snap on a key event keeps the last snap marker until the pointer moves.
- A failing command leaves the document unchanged; its message goes to the status bar.

## Acceptance tests

1. Given the wall tool idle, when the user presses `Esc`, then the selection tool becomes active.
2. Given the wall tool after its first click, when the user types `4.5` and presses `Enter`, then one wall of 4.5 m
   is created as one undo step.
3. Given the selection tool, when the user presses on a selected element, drags and releases, then the tool receives
   press, moves and release in that order and the move runs as one command.
4. Given a sheet with two placed views, when the user drags one by 20 mm, then the sheet is updated once with its
   centre moved by 20 mm.
5. Given the wall tool, when a different tool is chosen, then the preview, fields, handles, text box and warning
   are cleared.

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
