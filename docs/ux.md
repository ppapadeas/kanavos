# User experience

The measure of Domos is simple: an architect who already works in a commercial BIM tool should model a small house
and print a permit-ready drawing set **faster** in Domos, on day one, without a manual. Everything below serves that.

Domos has its own visual identity, names and layout. It keeps the working *concepts* architects already rely on —
storeys, cut planes, layered build-ups, drawings derived from the model — because those are the craft, not anyone's
property. It does not imitate any product's screens (see the [clean-room protocol](legal/clean-room.md) §4.6).

## 1. Principles

1. **Drawing is direct.** You draw on the canvas and type numbers while drawing. Dialogs are a last resort.
2. **Nothing is modal.** Every panel is live. Changes preview immediately, `Esc` always backs out, and everything
   is undoable, including selection changes made by mistake.
3. **The model is always right.** Plans, sections, schedules and sheets are live views of one model. There is no
   "update drawings" button, and nothing gets out of sync.
4. **Fast is a feature.** The interface never freezes. Heavy work runs in the background and the last good picture
   stays on screen until the new one is ready (see budgets in [`architecture.md`](architecture.md#7-performance-budgets)).
5. **Explain, don't fail.** When something can't be built (a roof that doesn't close, a wall that can't join), Domos
   highlights the element, says why in plain words, and keeps going.
6. **Show the rules while you design.** Greek regulations are visible while drawing, not discovered at submission.
7. **Two languages, one product.** Ελληνικά and English are complete from the first release, including units,
   decimal separators and regulation terms.
8. **Everyone can use it.** Keyboard and mouse parity, scalable UI, high-contrast themes, colour-blind-safe palettes
   and screen-reader labels.

## 2. Workspace

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│  Context bar: active tool · its key parameters · presets           ⌘K  search all    │
├──────────────┬───────────────────────────────────────────────────────┬───────────────┤
│ Project tree │  View tabs:  Ισόγειο ▸  Τομή Α-Α ▸  3D ▸  Φύλλο 01                     │ Inspector     │
│              │                                                       │               │
│ ▸ Site       │                                                       │ Selection-    │
│ ▾ Storeys    │                    Canvas                             │ aware         │
│   +2  Δώμα   │      (on-canvas handles, live dimensions,             │ properties,   │
│   +1  Όροφος │       inference labels, quick actions)                │ mixed values  │
│    0  Ισόγειο│                                                       │ shown as —    │
│ ▸ Views      │                                                       │               │
│ ▸ Sheets     │                                                       │ Regulation    │
│ ▸ Schedules  │                                                       │ meters (ΝΟΚ)  │
├──────────────┴───────────────────────────────────────────────────────┴───────────────┤
│ Status: X/Y in project and ΕΓΣΑ87 · snap state · scale · storey · background jobs      │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

- **Canvas first.** Side panels collapse to a slim rail; `Tab` toggles them all.
- **Split views.** Any two views side by side, synced selection and hover (plan + 3D is the default pair).
- **Project tree** holds storeys, views, sheets and schedules. Drag a view onto a sheet to place it.
- **Inspector** shows the selection's properties grouped by what people change most. Multi-selection shows shared
  values and "—" for mixed ones; editing a mixed field applies to all.

## 3. Interaction model

### Drawing and typed input

- Click to start, move, click to finish. While moving, a floating field shows length and angle.
- **Start typing a number** at any time: it goes into the length. `Tab` cycles length → angle → offset. `Enter`
  commits. Arithmetic works (`3.20+0.25`), and so do units (`320cm`).
- Relative input from the last point and absolute ΕΓΣΑ87 coordinates are both supported in the same field
  (`@2.5<90`, `#476532.12,4097345.88`).

### Inference and snapping

- Snaps to endpoints, midpoints, intersections, perpendiculars, extensions, parallels, wall faces and centrelines,
  layer boundaries of a build-up, and grid.
- Every active inference is **named on screen** ("κάθετη στον τοίχο 12", "προέκταση") so users know why the cursor
  jumped.
- Holding `Shift` locks the current direction; `Alt` disables snapping temporarily.

### On-canvas editing

- Selecting an element shows handles for what can change: ends, reference line, thickness, height (in 3D and
  section), opening width and sill.
- A small **quick-action strip** appears near the selection with the 4–6 most likely operations for that element type
  (split, join, offset, mirror, change build-up, add opening). It never covers the element.

### Command palette

- `Ctrl+K` opens a search box for every command, setting, view and element, in Greek and English, with fuzzy
  matching ("τοιχ πάχ" finds wall thickness).
- Natural queries select elements: "τοίχοι ισογείου πάχους > 25cm", "παράθυρα χωρίς ποδιά".

### Undo and history

- Unlimited undo. The history panel lists named steps ("Μετακίνηση 3 τοίχων") and lets you jump back.
- View navigation (pan, zoom, switching tabs) is kept out of the model history.

### Shortcuts

- Domos ships its own default keymap, designed for one-hand access while the other hand is on the mouse.
- Every shortcut is remappable; keymaps are plain files users can share. Domos does not ship keymaps that mirror any
  other product.

## 4. Regulation-aware design (Greek practice)

- Set the plot (from a DXF τοπογραφικό or by drawing it) and the building terms (συντελεστής δόμησης, κάλυψη, ύψος,
  όροφοι, υποχρεωτικές αποστάσεις).
- The inspector shows **live meters**: κάλυψη, δόμηση, όγκος, ύψος, with used vs allowed and the remaining margin.
  They update while dragging a wall.
- Spaces know their category (κύρια χρήση, βοηθητικοί, εξαιρούμενοι κατά ΝΟΚ), so area tables are correct by
  construction.
- **Permit set preset**: one command generates the standard drawing set (διάγραμμα κάλυψης, κατόψεις, τομές, όψεις)
  on sheets with the correct scales and a title block, ready to export as PDF and DXF.

## 5. First run and learning

- First launch opens a **sample house** project, already on sheets, so people see the result before they learn tools.
- Empty states teach: an empty storey says "Ξεκινήστε με έναν τοίχο — πατήστε W ή σύρετε από το εργαλείο".
- Hovering any control for a second shows a short description and its shortcut; a second line links to a 20-second
  clip in the built-in help.
- No feature requires reading documentation to discover. Measured by usability tests (§8).

## 6. Visual design

- Our own icon set, drawn on a 24 px grid with consistent stroke weight, and our own colour system with light and
  dark themes defined as tokens.
- Canvas defaults follow drafting practice: black on white for plan views, line weights shown true to print at the
  current scale, colour used for meaning (selection, inference, warnings), never decoration.
- Typography: an open-licensed sans with full Greek support for UI, and a drafting font for drawings.

## 7. Vocabulary

Domos uses plain, standard terms, aligned with IFC where possible. Specs and UI strings use only these.

| Domos term (EN) | Ελληνικά | Meaning | IFC |
| --- | --- | --- | --- |
| Storey | Επίπεδο / Όροφος | Horizontal level of the building with an elevation and height | IfcBuildingStorey |
| Cut plane | Επίπεδο τομής | Height at which a plan view slices the storey | — |
| Element | Δομικό στοιχείο | Any modelled building part | IfcElement |
| Build-up | Διαστρωμάτωση | Ordered layers of materials making up a wall, slab or roof | IfcMaterialLayerSet |
| Material | Υλικό | Physical material with graphic and physical properties | IfcMaterial |
| Space | Χώρος | Bounded volume with use, area and category | IfcSpace |
| Opening | Άνοιγμα | Door, window or void cut into a host element | IfcOpeningElement |
| Object | Αντικείμενο | Parametric component (door, window, fixture, furniture) | IfcElement subtypes |
| Line weight | Πάχος γραμμής | Printed width of a stroke | — |
| Visibility set | Σετ ορατότητας | Saved combination of which categories/layers are shown | — |
| Display rules | Κανόνες απεικόνισης | Rules that change how elements look in a view by criteria | — |
| Sheet | Φύλλο σχεδίου | Printable page holding views and a title block | — |
| Sheet set | Σετ φύλλων | Ordered group of sheets published together | — |
| Linked model | Συνδεδεμένο μοντέλο | Another file referenced into this project | — |
| Phase | Φάση | Existing / demolished / new construction status | — |
| Preset | Προεπιλογή | Saved tool or element settings | — |

## 8. Measuring UX

- **Benchmark task**: model a two-storey house from a DXF survey and publish a permit set. Target: under 90 minutes
  for an experienced BIM user on first use, under 45 minutes after one week.
- Monthly moderated tests with practising architects (Kalamata/Messinia first), recorded with consent, with
  findings filed as issues labelled `ux`.
- Telemetry is **off by default** and opt-in only, anonymous, documented, and limited to performance and crash
  data. No content from projects ever leaves the machine.
- Every release notes the benchmark-task times and the top three fixed friction points.
