---
id: SPEC-files-001
title: Saving and opening projects in the native format
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ISO/IEC 21320-1:2015 Document Container File — Part 1 (ZIP container)
  - RFC 1951 DEFLATE Compressed Data Format
  - RFC 8259 The JavaScript Object Notation (JSON) Data Interchange Format
  - RFC 3339 Date and Time on the Internet: Timestamps
  - docs/architecture.md §6 and §8; ADR-0002
---

# Saving and opening projects in the native format

## Purpose

People keep their work in one project file that opens exactly as it was saved, survives crashes during saving,
and stays readable by later versions of Kanavos and by other programs.

## Behaviour

### File layout

1. A project file is a ZIP archive with the extension `.kanavos` holding `manifest.json`, `model.json` and
   optionally `thumbnail.png`, in that order. `assets/` and `cache/` are reserved name prefixes.
2. `manifest.json` names the format (`"kanavos"`), the model schema version, the application version, optional
   creation and modification timestamps (RFC 3339, supplied by the caller), counts of storeys, elements (total and
   per category), materials, build-ups, views and sheets, and whether a thumbnail is present.
3. `model.json` is the whole project document as pretty-printed UTF-8 JSON with a fixed key order. Lengths are
   metres, angles radians. Numbers are written so that they read back to the identical binary value.
4. The thumbnail is a PNG image stored without compression.

### Saving

5. Saving the same document with the same options produces identical bytes on every platform: fixed entry order,
   entry timestamps of 1980-01-01 00:00:00, fixed permissions and host type, fixed DEFLATE level.
6. Save → open → save produces a byte-identical file when the same options are used.
7. A save first writes `<file>.tmp`, flushes it to disk, copies an existing `<file>` to `<file>.bak` (unless
   backups are switched off), and then renames `<file>.tmp` over `<file>`. At every moment `<file>` holds a
   complete old or new version. On failure the `.tmp` file is removed and `<file>` is unchanged.
8. A document holding NaN or an infinite number is not saved; the error names the field (for example
   `storeys.12.height`).
9. Timestamps that are not RFC 3339 and thumbnails that are not PNG images are refused.

### Opening

10. Bytes that are not a ZIP archive are reported as "not a Kanavos project file".
11. A ZIP archive without `manifest.json`, or whose manifest names another format, is reported as not a Kanavos
    project.
12. A manifest with a schema version newer than this build supports is refused before the model is read, telling
    the user to update Kanavos.
13. A missing `model.json` is reported as an incomplete file.
14. Malformed JSON, or JSON of the wrong shape, is reported with the entry name, the path to the failing field
    where it can be determined, the line and column, and the parser message.
15. A model with an older schema version is upgraded step by step to the current version and the user is told that
    saving will use the new version.
16. After reading, the document's consistency check runs (ids, storeys, opening hosts, build-up materials, plan
    view storeys). An inconsistent document is refused, unless the caller asks for lenient loading, in which case it
    opens with a warning.
17. Warnings, not errors: unknown entries outside `assets/` and `cache/` (dropped on the next save), counts in the
    manifest that differ from the model, and a thumbnail that is not a PNG (ignored).
18. Entries whose uncompressed size exceeds 2 GiB are refused.

## Edge cases

- An empty document (no storeys or elements) saves and opens.
- Negative zero, subnormal numbers and the largest finite numbers round-trip exactly.
- Greek and other non-ASCII text in any string round-trips exactly.
- A manifest of a newer version whose other fields have changed shape is still recognised as newer (only `format`
  and `schema_version` are read before the version check).
- Saving into a directory that does not exist fails with a file system error and creates nothing.

## Acceptance tests

1. Given the starter template, when it is saved, opened and saved again with the same options, then both files are
   identical byte for byte and the opened document equals the original.
2. Given a house with walls, openings, a slab with a hole, a space, a column, a stair, a pitched roof, an object, a
   Greek text note and a thumbnail, the same holds and the thumbnail is returned on opening.
3. Given 128 or more random documents built from the starter by random edits (storeys, walls, slabs, spaces,
   columns, openings, materials, properties, deletions) with extreme numbers and arbitrary Unicode text, the same
   holds for every one.
4. Given an existing project file, when a new version is saved, then `<file>.bak` holds the previous bytes, no
   `.tmp` file remains, and the file opens as the new version.
5. Given bytes that are not ZIP, a ZIP from another application, a manifest naming another format, a manifest with
   schema 99, and an archive without `model.json`, opening reports respectively not-an-archive, wrong format (twice),
   newer version (99 vs supported) and missing entry.
6. Given a model where a wall's height is the string `"tall"`, opening reports corrupt JSON in `model.json` with a
   path starting at that wall's element entry.
7. Given a storey whose height is NaN, saving fails and names `storeys.<id>.height`.
8. Given a hand-written schema 0 file (tables as arrays, `last_id`), opening upgrades it, warns, and the next save
   writes schema 1 that opens without warnings.
9. Given an opening whose host wall does not exist, strict opening fails and lenient opening succeeds with a warning.

## Open questions

- Binary geometry caches under `cache/` and embedded assets under `assets/` are reserved but not yet specified.
- Incremental saving for very large projects (architecture budget: 200 ms) is not addressed by this layout.

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
