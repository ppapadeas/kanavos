# The `.kanavos` file format

A Kanavos project is one file with the extension `.kanavos`. This page describes it completely, so that other
programs can read and write projects without Kanavos. The reference implementation is the `kanavos-format` crate;
the behaviour is specified in [`specs/files/native-format.md`](../specs/files/native-format.md) and
[`specs/files/crash-recovery.md`](../specs/files/crash-recovery.md).

## 1. Container

The file is a ZIP archive (ISO/IEC 21320-1, a subset of the PKWARE `.ZIP` application note) with these entries,
written in this order:

| Entry | Required | Compression | Contents |
| --- | --- | --- | --- |
| `manifest.json` | yes | DEFLATE | What the file is and which schema it uses (§2) |
| `model.json` | yes | DEFLATE | The project document (§3) |
| `thumbnail.png` | no | stored | A preview image, PNG |
| `assets/…` | — | — | Reserved for embedded assets (images, imported drawings). Not written yet. |
| `cache/…` | — | — | Reserved for caches that can always be rebuilt from the model. Not written yet. |

Writers make archives **deterministic**: every entry has the timestamp 1980-01-01 00:00:00 (the ZIP epoch), Unix
host type with permissions `0644`, DEFLATE level 6, no comments and no extra fields. Saving the same document with
the same options therefore produces the same bytes on every platform, and save → load → save is byte-identical.
This makes files easy to compare and lets tests catch any loss of information.

Readers do not depend on any of this: they accept any valid ZIP archive with the entries above, in any order, with
any compression method the ZIP library supports (stored or DEFLATE). Entries larger than 2 GiB uncompressed are
refused, to guard against archive bombs. Unknown entries outside `assets/` and `cache/` are ignored with a warning
and are not kept when the project is saved again.

## 2. `manifest.json`

```json
{
  "format": "kanavos",
  "schema_version": 2,
  "app_version": "0.0.1",
  "created": "2026-10-01T08:00:00Z",
  "modified": "2026-10-09T12:30:00+03:00",
  "counts": {
    "storeys": 2,
    "elements": 13,
    "materials": 19,
    "build_ups": 9,
    "views": 9,
    "sheets": 0,
    "by_category": { "door": 1, "slab": 1, "space": 1, "wall": 4, "window": 1 }
  },
  "thumbnail": false
}
```

| Field | Meaning |
| --- | --- |
| `format` | Always `"kanavos"`. Anything else means "not a Kanavos project". |
| `schema_version` | Version of the `model.json` schema (§4). |
| `app_version` | Version of the program that wrote the file. Informational. |
| `created`, `modified` | RFC 3339 date-times supplied by the program that saves. Optional. A re-save keeps `created`. |
| `counts` | Number of storeys, elements, materials, build-ups, views and sheets, and elements per category key (`wall`, `slab`, `door`, …). Lets file browsers describe a project without parsing the model. |
| `thumbnail` | Whether `thumbnail.png` is present. |
| `recovery_of` | Only in crash-recovery files: the path of the project the copy belongs to. |

`format` and `schema_version` are permanent: every version of the format keeps them with this meaning, so any
reader can decide whether it understands a file before reading anything else. Other fields may be added later;
readers ignore fields they do not know.

## 3. `model.json`

The project document as JSON (RFC 8259), UTF-8, **pretty-printed** with two-space indentation and a final
newline.

Why pretty rather than compact: the model is the part people inspect when something goes wrong, and the part
version-control tools diff when a project is kept in a repository (unzip, or a `textconv` filter). With one value
per line, a moved wall changes a handful of lines. The cost is small because whitespace compresses well: the
model of a small test house is 21 KB pretty and 11 KB compact, but 3.5 KB and 3.1 KB once deflated, about 12 %
apart.

### 3.1 Conventions

- Lengths in metres, areas in m², angles in radians, all as plain JSON numbers (`3.2`, not `"3.20 m"`).
- Numbers are written in the shortest form that reads back to exactly the same binary value, so no precision is
  lost. NaN and infinity cannot be written; a save that would contain one fails with the path of the field.
- Plan points are `[x, y]`. Polygons are lists of points without repeating the first. Polygons with holes are
  `{"outer": [...], "holes": [[...], ...]}`.
- Key order is deterministic: object fields appear in a fixed order, and tables are maps keyed by id in increasing
  numeric order.
- Variant types are tagged with a `"type"` field in `snake_case` (`{"type": "wall", ...}`).
- Optional fields at their default value are often left out; readers must treat a missing field as its default.

### 3.2 Top level

| Field | Contents |
| --- | --- |
| `next_id` | The highest id handed out so far. Every id in the file is ≤ `next_id`; new objects get `next_id + 1`. |
| `project_id` | The project's permanent id, a UUID string such as `"5f0c2a9e-7d41-4b8e-9a3c-1e2f3a4b5c6d"` (§3.2.1). Left out while the project has none. |
| `tolerance` | `{"length": 0.0001, "angle": 1e-05}`: project tolerance (metres, radians). |
| `info` | Project information printed on sheets (title, location, owner, designer, date, revision, notes). |
| `site` | Plot boundary, street and building lines, building terms, georeferencing (EPSG code, easting, northing, rotation, altitude), terrain points, ground level. |
| `defaults` | Default build-ups and materials for new elements. |
| `storeys` | Map id → storey (`name`, `elevation`, `height`, `cut_height`, `below_ground`). |
| `materials` | Map id → material (`name`, `category`, `color` `{r,g,b,a}`, `cut_pattern`, `cut_background`, `priority`, `conductivity`, `density`). |
| `build_ups` | Map id → build-up (`name`, `usage`, `layers`: `{material, thickness, core}`). Wall layers run left face → right face looking from start to end; slab and roof layers run top → bottom. |
| `elements` | Map id → element (§3.3). |
| `views` | Map id → view (plan, section, elevation, 3D, site plan, coverage diagram, area table, schedule). |
| `sheets` | Map id → sheet (number, name, paper, viewports, title block). |

Ids are unique across all tables and never reused. Map keys are the id written as a decimal string, and every
object repeats its own `id`.

#### 3.2.1 Project id

`project_id` identifies the project for its whole life: renaming the project, editing its information or moving
the file keeps it. Exports derive their stable identifiers from it (the IFC `GlobalId`s, see
[`formats/ifc.md`](formats/ifc.md)).

- A new project gets a random id (128 bits in the RFC 9562 version 4 layout) when it is created from the starter
  template. The sample project uses a fixed id so that it is the same every time it is built.
- Files written before project ids existed, and documents imported from IFC, have no id. They load with the nil id
  and get a fresh one the first time they are saved by the application, the command-line tools or the control
  server (the engine's file adapter, `kanavos_engine::io`, writes every file with an id; the session keeps the
  id it was given, so later saves write the same one, and undo does not remove it).
- The format library itself writes the document as it is: saving the same document twice gives the same bytes.

### 3.3 Elements

Every element has `id`, `kind` (tagged by `type`), and optionally `storey`, `view` (for annotation drawn in a
section or elevation), `phase` (`new`, `existing`, `demolished`), `tag`, `properties` (free text pairs) and
`external_id` (its identifier in the file it was imported from, e.g. an IFC `GlobalId`; copies of the element do
not get it).
For example, a straight 10 m wall on storey 28 using build-up 19:

```json
"39": {
  "id": 39,
  "storey": 28,
  "kind": {
    "type": "wall",
    "start": [0.0, 0.0],
    "end": [10.0, 0.0],
    "reference": "centre",
    "structure": { "type": "layered", "build_up": 19 },
    "height": 3.0,
    "base_offset": 0.0
  }
}
```

(Points are shown on one line here; the file puts each coordinate on its own line.)

Element kinds: `wall`, `slab`, `column`, `beam`, `opening` (door, window or void hosted by a wall), `stair`,
`roof`, `railing`, `space`, `object`, `dimension`, `text`, `level`, `drafting`, `grid`, `underlay`. Their fields
are the public fields of the corresponding types in `kanavos-model`, whose documentation is the field reference.

### 3.4 Consistency rules

A loaded document must satisfy:

- every id is ≤ `next_id` and every table key equals the object's `id`;
- an element's `storey` exists;
- an opening's `host` is a wall;
- every build-up layer's material exists;
- every plan view's storey exists.

Readers refuse files that break these rules unless asked to load leniently (recovery tools do this and show the
problem as a warning).

## 4. Versions and migration

`schema_version` counts incompatible changes to `model.json`. This build writes **schema 2** and reads schemas 0,
1 and 2.

- A file with a **newer** schema is refused with "saved by a newer version of Kanavos", before the model is read.
  It is never read partially.
- A file with an **older** schema is upgraded in memory, one version at a time, by migration steps that rewrite the
  JSON. The user is told; the next save writes the current schema.
- Adding an optional field with a default does not need a new schema version: older files simply lack it. Renaming,
  removing or reinterpreting a field does.
- When an older file kept information somewhere else before its field existed, the reader moves it into place on
  load, after any schema migration (`crates/format/src/legacy.rs`); the next save writes the current layout. So far: the IFC
  `GlobalId` of imported elements, once the user property `ifc:GlobalId`, becomes `external_id`.

Each step lives in `crates/format/src/migrate.rs` with a test that loads a hand-written file in the old layout.

### Schema history

| Version | Change |
| --- | --- |
| 0 | Draft layout used while the format was designed. Never written by a released build; kept as the worked example of a migration. The id counter is called `last_id`, and `storeys`, `materials`, `build_ups`, `elements`, `views` and `sheets` are arrays of objects with an `id` instead of maps keyed by id. |
| 1 | The layout of §3, with the earlier meaning of an opening's `flip_side`: unflipped, a door opened towards its wall's left face and a window's outer face was the right face. |
| 2 | Current layout (§3). An opening's `flip_side` follows the wall build-ups (SPEC-objects-006): unflipped, a window's outer face is its wall's left face and a door opens towards the right face. Upgrading from schema 1 inverts every opening's `flip_side`, so doors and windows stay where they were. |

Optional fields added without a new schema version, all read as their defaults from files that lack them (of any
schema): `project_id` (top level), `external_id` (elements, replacing the `ifc:GlobalId` user property, which is
moved into it on load).

## 5. Saving safely

- A save writes the complete archive to `<name>.tmp` next to the project and flushes it to disk.
- If the project file already exists and backups are on (the default), it is copied to `<name>.bak`.
- `<name>.tmp` is then renamed over the project file. The rename replaces the file in one step, so the project file
  always holds either the previous or the new complete version; a crash leaves at most a stray `.tmp`.
- If any step fails, the `.tmp` file is removed and the project file is untouched.

## 6. Autosave and crash recovery

While a project has unsaved changes the application periodically writes a recovery copy, `recovery.kanavos`, into
a per-session recovery directory, with the same atomic procedure and no backup. The recovery file is an ordinary
`.kanavos` file whose manifest may name the project it belongs to (`recovery_of`). After a normal save or close the
application deletes it. On start, a recovery file that is still present means the previous session ended
unexpectedly, and the application offers to open it. A damaged recovery file is still reported, so the user can
decide what to do with it.
