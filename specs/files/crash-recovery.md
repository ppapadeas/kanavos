---
id: SPEC-files-002
title: Autosave and crash recovery
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - docs/architecture.md §1 goal 5 and §8 (autosave with crash recovery)
  - SPEC-files-001
---

# Autosave and crash recovery

## Purpose

People never lose more than a minute of work when the application or the computer stops unexpectedly.

## Behaviour

1. While a project has unsaved changes, the application writes a recovery copy at a fixed interval (60 s by
   default, set by the application) into a recovery directory that belongs to the editing session.
2. The recovery copy is an ordinary project file named `recovery.kanavos` (SPEC-files-001), written with the same
   atomic procedure as a save, but without keeping a `.bak`. Each autosave replaces the previous copy.
3. The manifest of the recovery copy may name the project file it belongs to (`recovery_of`) and carries the
   timestamps supplied by the application.
4. After a normal save or a normal close, the application deletes the recovery copy.
5. On start, the application looks for a recovery copy. If one exists, the previous session did not end normally:
   the application reports the copy (its path, the file system modification time, and its manifest when readable)
   and offers to open it.
6. A recovery copy that is damaged is still reported (without a manifest), so the user can decide whether to try to
   open it or discard it.
7. Looking in a recovery directory that does not exist reports "no recovery copy". Deleting a recovery copy that
   does not exist is not an error.

## Edge cases

- The recovery directory does not exist yet: the first autosave creates it.
- An autosave interrupted by a crash leaves the previous recovery copy intact (atomic replacement).

## Acceptance tests

1. Given an empty recovery directory, when a document is autosaved, then a recovery copy is found whose manifest
   counts match the document, and opening it gives the same document.
2. Given an existing recovery copy, when another document is autosaved with `recovery_of` set, then the copy is
   replaced, its manifest names the project, and no `.bak` is created.
3. Given a recovery copy, when it is discarded, then none is found; discarding again succeeds.
4. Given a recovery file containing garbage, then it is found with no manifest, and opening it fails with an error.

## Open questions

- Several projects open in one application instance need one recovery directory each; the application chooses the
  directory names.

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: <name>, <date> — Result: approved / changes requested
