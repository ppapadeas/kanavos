---
id: SPEC-updates-001
title: Over-the-air updates from signed release packages
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - docs/ux.md §1 (nothing is modal), §8 (telemetry off; no project content leaves the machine)
  - docs/architecture.md §1 goal 5 and §8 (never lose work)
  - SPEC-files-002 (autosave and crash recovery), SPEC-ui-006 (files, unsaved changes, preferences)
  - RFC 8032 (Ed25519), FIPS 180-4 (SHA-256), RFC 4648 §4 (base64)
  - Semantic Versioning 2.0.0 §11 (precedence)
  - GitHub REST API, "List releases" (GET /repos/{owner}/{repo}/releases)
---

# Over-the-air updates from signed release packages

## Purpose

Testers stay on the current version without unpacking archives by hand, and an update never installs anything that
the Kanavos maintainers did not sign, never touches their projects, and never loses unsaved work.

## Behaviour

### Discovery

1. The release list is fetched with one HTTPS request to the GitHub REST API for the repository `ppapadeas/kanavos`
   (`GET /repos/ppapadeas/kanavos/releases?per_page=20`), anonymously, with `User-Agent: Kanavos/<version> (<os>;
   <arch>)` and `Accept: application/vnd.github+json`. Nothing else about the computer, the person or their
   projects is sent.
2. Drafts are ignored. A release tag is `v` followed by a semantic version (`vX.Y.Z` or `vX.Y.Z-pre.N`); other tags
   are ignored. The **stable** channel accepts releases that are not marked as pre-releases and have no
   pre-release part; the **test** channel accepts all.
3. A release is installable on this computer only when it has, for this operating system and architecture (named as
   Rust's `std::env::consts::OS` and `ARCH`), the package `kanavos-<version>-<os>-<arch>.zip` (Windows) or
   `.tar.gz` (others), its `<package>.sha256` and its `<package>.sig`, all completely uploaded, with download
   addresses on `https://github.com/…/releases/download/…`.
4. The offer is the installable release with the highest precedence (semver §11; build metadata ignored) that is
   strictly newer than the running version. Without one, the running version is the latest.
5. A rate-limit answer (HTTP 403 or 429) is reported with the waiting time when the server gives one; no network,
   a failed name lookup, a refused connection or a timeout is reported as "could not reach the update server".
   Neither is shown after the automatic check at start.

### Download and verification

6. Before downloading, the installation is checked: the program's folder must contain `QUICKSTART.txt` (present in
   every release package) and be writable by the current account. Otherwise the person is told why (for example a
   folder under Program Files) and where to download the release by hand.
7. The checksum and signature files are fetched first; the package is then downloaded into a new temporary folder,
   with progress and a way to cancel. A package larger than announced (or than 512 MiB) is refused.
8. The package is accepted only when (a) its SHA-256 equals the value in the `.sha256` file for that file name and
   (b) the `.sig` file, standard base64 of 64 bytes, is a valid Ed25519 signature of the raw package bytes for the
   release public key built into the program. A missing or unreadable file, a mismatch or an invalid signature
   rejects the package and deletes it.
9. A build whose release public key is the placeholder refuses to check or install and reports "updates are not
   configured in this build".

### Installation

10. The package is unpacked into the temporary folder. Every entry must lie inside the top-level folder
    `kanavos-<version>-<os>-<arch>/` of the version on offer; absolute names, `..` components, drive or stream
    names, backslashes, links, special files, duplicate names and oversized content reject the whole package. The
    package must contain both programs and `QUICKSTART.txt`.
11. The files installed are the package's files, except project files (`*.kanavos`) other than the bundled
    `sample-house.kanavos`. Files in the installation folder that are not in the package are never changed.
12. Each file is first copied next to its destination as `<file>.new` and written to disk. Then, file by file, the
    current file is renamed to `<file>.old` (or `<file>.old.1` … when an older leftover cannot be removed) and the
    new file is renamed into place. A journal in the installation folder records the plan and the stage (staging,
    swapping, installed); a lock file prevents two processes from working on the same folder (a lock older than
    two minutes is considered abandoned).
13. If a rename fails, every file already replaced is put back and the previous version keeps working. If putting
    back also fails, or the computer stops during the renames, the next start completes the installation from the
    `.new` copies. If the computer stops while copying, the next start removes the `.new` copies.
14. At each start the `.old` files recorded in the journal are deleted; those of a program still running are kept
    and deleted at a later start. Nothing that is not recorded in the journal as an update leftover is deleted.

### Desktop application

15. Preferences: *check for updates at start* (on by default), *channel* (stable by default) and the *skipped
    version*, stored with the other preferences (SPEC-ui-006). The settings are in the Help menu and in the About
    window, next to a one-line statement of what the check sends.
16. At start, when the setting is on, the updates are configured and `KANAVOS_NO_UPDATE_CHECK` is not set, the
    check runs in the background; the interface never waits. The visual-verification hook never checks.
17. An offer appears as an unobtrusive notice in the status bar and as a Help-menu item; either opens a non-modal
    window with the version, the publication date and the release notes as plain text, and three choices:
    *Install and restart*, *Later* (keeps the notice) and *Skip this version* (the automatic check does not offer
    that version again; a manual check still does).
18. *Help › Check for updates…* always reports: the offer, "You have the latest version", or the reason it failed.
19. *Install and restart*: the download runs with a progress bar and *Cancel*. When it is verified and the project
    has unsaved changes, the person chooses *Save*, *Don't save* or *Cancel* (the same question as before closing).
    A recovery copy (SPEC-files-002) is written in every case before installing; if it cannot be written while
    there are unsaved changes, nothing is installed. After installing, the new version is started with the open
    project file as its argument and the running one closes; with no unsaved changes the recovery copy is removed
    first, otherwise the new version offers it.

### Command line

20. `kanavos-cli update [--check] [--channel stable|test]` checks and, without `--check`, downloads, verifies and
    installs as above into the folder of the command-line program (both programs are replaced). Exit codes: 0
    installed or up to date, 1 failure, 2 usage error, 3 not configured or turned off.

### Turning updates off

21. `KANAVOS_NO_UPDATE_CHECK` set to anything but empty or `0` turns off every check and download, including manual
    ones and the command line.

## Edge cases

- The running version is a test build and the channel is stable: only a newer release (by precedence) is offered,
  for example `0.3.0` over `0.3.0-pre.4`, never `0.2.9`.
- The newest release lacks the files for this platform (or they are still uploading): the newest complete one is
  offered.
- The repository has no releases yet or is not reachable: a manual check reports the server's answer; the automatic
  check stays silent.
- The installation folder is a build folder or a system folder without `QUICKSTART.txt`: updating is refused.
- A package whose top-level folder names another version: refused, even though it is signed.
- Two Kanavos processes start while an installation runs: the second leaves the folder alone.
- A `.old` file of a program that is still running cannot be deleted on Windows: it stays recorded and is deleted
  at a later start.

## Acceptance tests

1. Given versions from semver §11.4, when they are compared, then they order as the specification lists, and
   `0.3.0-pre.10` is newer than `0.3.0-pre.2`.
2. Given a recorded release list with a draft, test builds, a non-version tag, a release with an untrusted address,
   one with an incomplete upload and one without a signature for Windows, when a release is selected for each
   channel and platform, then the expected version (or none) is chosen.
3. Given a package signed with a test key, when it is verified with that key and its checksum file, then it is
   accepted; when one byte changes, when the checksum is recomputed for the changed bytes, when another key is
   used, or when the signature or checksum file is missing or malformed, then it is rejected.
4. Given zip and tar.gz archives with `..`, absolute, backslash, outside-root or link entries, when extracted, then
   they are rejected and nothing is written outside the temporary folder.
5. Given an installation folder with the previous programs, guides, the sample and user files, when a package is
   installed, then the package files are new, the user files and other projects are unchanged, and no journal,
   lock, `.new` or `.old` files remain.
6. Given a rename that fails during installation, then the previous files are all back and nothing else remains;
   given that putting back also fails, then the next start completes the installation.
7. Given a journal listing user files as leftovers, when cleaning up, then only update leftovers are deleted.
8. Given a fresh lock, then installation reports another update in progress and clean-up does nothing; given a lock
   from a stopped process, then installation proceeds.
9. Given `KANAVOS_NO_UPDATE_CHECK=1`, then `kanavos-cli update` exits with 3; given an unknown channel, it exits
   with 2.
10. Given preferences written before updates existed, then the check at start is on and the channel is stable.

## Open questions

- macOS packages are not produced yet; the same naming would apply (`.tar.gz`).
- A health check of the new version before deleting the `.old` files (automatic rollback when it does not start).

---

## Review (completed by the reviewer)

- [x] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [x] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [x] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [x] No screenshots or images of the reference product.
- [x] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [x] Describes behaviour, not implementation details of the reference product.

Reviewed by: Pierros Papadeas, 2026-10-10 — Result: approved

Note: the reviewer is also the author of record; clean-room §2 asks for a second reviewer before the first public
release.
